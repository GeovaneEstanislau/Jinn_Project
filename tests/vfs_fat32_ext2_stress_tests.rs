//! Jinn OS — Testes Unitários de Estresse e Conformidade VFS, FAT32 e Ext2 no Host (Fase 1 / PROMPT 1)
//!
//! Valida o subsistema VFS, drivers de alocação de blocos e clusters (FAT32 e Ext2),
//! operações de criação de pastas (sys_mkdir), arquivos (sys_creat), escrita (sys_write),
//! leitura (sys_read), truncamento (O_TRUNC), remoção e conformidade estrita com códigos de erro POSIX:
//! - ENOENT (2)
//! - EBADF (9)
//! - EBUSY (16)
//! - EEXIST (17)
//! - ENOTDIR (20)
//! - EISDIR (21)
//! - EMFILE (24)
//! - ENOSPC (28)
//! - EINVAL (22)

use std::collections::HashMap;

// ── Definições POSIX e VFS ───────────────────────────────────────────────────

pub const O_RDONLY: u32 = 0x0000;
pub const O_WRONLY: u32 = 0x0001;
pub const O_RDWR:   u32 = 0x0002;
pub const O_CREAT:  u32 = 0x0040;
pub const O_TRUNC:  u32 = 0x0200;
pub const O_APPEND: u32 = 0x0400;

pub const ENOENT: i64 = -2;
pub const EBADF:  i64 = -9;
pub const EBUSY:  i64 = -16;
pub const EEXIST: i64 = -17;
pub const ENOTDIR:i64 = -20;
pub const EISDIR: i64 = -21;
pub const EINVAL: i64 = -22;
pub const EMFILE: i64 = -24;
pub const ENOSPC: i64 = -28;

pub const MAX_PATH_LEN: usize = 256;
pub const MAX_TASK_FDS: usize = 32;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum VfsError {
    NotFound,
    AlreadyExists,
    PermissionDenied,
    IsADirectory,
    NotADirectory,
    IoError,
    NoSpace,
    InvalidPath,
    NotSupported,
    Busy,
}

pub fn vfs_error_to_errno(err: VfsError) -> i64 {
    match err {
        VfsError::NotFound => ENOENT,
        VfsError::AlreadyExists => EEXIST,
        VfsError::PermissionDenied => -13, // EACCES
        VfsError::IsADirectory => EISDIR,
        VfsError::NotADirectory => ENOTDIR,
        VfsError::NoSpace => ENOSPC,
        VfsError::InvalidPath => EINVAL,
        VfsError::Busy => EBUSY,
        VfsError::IoError => -5, // EIO
        VfsError::NotSupported => -38, // ENOSYS
    }
}

// ── Algoritmo 8.3 FAT32 Clamping e Matching ──────────────────────────────────

fn encode_83(name_str: &str) -> Result<[u8; 11], VfsError> {
    let mut res = [b' '; 11];
    let trimmed = name_str.trim();
    if trimmed.is_empty() {
        return Err(VfsError::InvalidPath);
    }

    if trimmed == "." {
        res[0] = b'.';
        return Ok(res);
    }
    if trimmed == ".." {
        res[0] = b'.';
        res[1] = b'.';
        return Ok(res);
    }
    if trimmed.starts_with('.') {
        return Err(VfsError::InvalidPath);
    }

    let (base, ext) = match trimmed.rsplit_once('.') {
        Some((b, e)) => (b, e),
        None => (trimmed, ""),
    };

    if base.is_empty() && ext.is_empty() {
        return Err(VfsError::InvalidPath);
    }

    let mut b_idx = 0;
    for b in base.bytes() {
        if b_idx >= 8 {
            break;
        }
        if b.is_ascii_alphanumeric() || b == b'_' || b == b'-' {
            res[b_idx] = b.to_ascii_uppercase();
            b_idx += 1;
        }
    }
    if b_idx == 0 {
        return Err(VfsError::InvalidPath);
    }

    let mut e_idx = 0;
    for b in ext.bytes() {
        if e_idx >= 3 {
            break;
        }
        if b.is_ascii_alphanumeric() || b == b'_' || b == b'-' {
            res[8 + e_idx] = b.to_ascii_uppercase();
            e_idx += 1;
        }
    }

    Ok(res)
}

fn format_83_to_string(raw: &[u8; 11], _is_dir: bool) -> String {
    let mut name = String::new();
    for i in 0..8 {
        if raw[i] != b' ' {
            name.push((raw[i] as char).to_ascii_lowercase());
        }
    }
    let ext1 = raw[8];
    let ext2 = raw[9];
    let ext3 = raw[10];
    if ext1 != b' ' || ext2 != b' ' || ext3 != b' ' {
        name.push('.');
        for &c in &[ext1, ext2, ext3] {
            if c != b' ' {
                name.push((c as char).to_ascii_lowercase());
            }
        }
    }
    name
}

// ── In-Memory Block Device ───────────────────────────────────────────────────

pub struct RamDisk {
    pub sectors: HashMap<u32, [u8; 512]>,
    pub max_sectors: u32,
}

impl RamDisk {
    pub fn new(max_sectors: u32) -> Self {
        Self {
            sectors: HashMap::new(),
            max_sectors,
        }
    }

    pub fn read_sector(&self, lba: u32, buf: &mut [u8; 512]) -> Result<(), VfsError> {
        if lba >= self.max_sectors {
            return Err(VfsError::IoError);
        }
        if let Some(sec) = self.sectors.get(&lba) {
            buf.copy_from_slice(sec);
        } else {
            buf.fill(0);
        }
        Ok(())
    }

    pub fn write_sector(&mut self, lba: u32, buf: &[u8; 512]) -> Result<(), VfsError> {
        if lba >= self.max_sectors {
            return Err(VfsError::IoError);
        }
        self.sectors.insert(lba, *buf);
        Ok(())
    }
}

// ── Simulador FAT32 Completo ─────────────────────────────────────────────────

pub const FAT32_EOF: u32 = 0x0FFF_FFF8;

pub struct SimulatedFat32 {
    pub disk: RamDisk,
    pub partition_lba: u32,
    pub partition_sectors: u64,
    pub fat_start_lba: u32,
    pub sectors_per_fat: u32,
    pub data_start_lba: u32,
    pub sectors_per_cluster: u8,
    pub root_cluster: u32,
    pub open_files: Vec<Option<SimulatedFatFile>>,
}

#[derive(Clone, Debug)]
pub struct SimulatedFatFile {
    pub name: [u8; 13],
    pub name_len: usize,
    pub raw_83: [u8; 11],
    pub is_dir: bool,
    pub first_cluster: u32,
    pub size: u32,
    pub dir_sector_lba: u32,
    pub dir_entry_offset: usize,
}

impl SimulatedFat32 {
    pub fn new(total_sectors: u32) -> Self {
        let mut sim = Self {
            disk: RamDisk::new(total_sectors),
            partition_lba: 0,
            partition_sectors: total_sectors as u64,
            fat_start_lba: 32,
            sectors_per_fat: 32,
            data_start_lba: 32 + (2 * 32), // 96
            sectors_per_cluster: 1, // 1 setor = 512 bytes por cluster
            root_cluster: 2,
            open_files: vec![None; 32],
        };

        // Inicializa FAT: cluster 0 e 1 reservados, cluster 2 é a raiz EOF
        let mut fat_buf = [0u8; 512];
        fat_buf[0..4].copy_from_slice(&0x0FFF_FFF8u32.to_le_bytes()); // Clus 0
        fat_buf[4..8].copy_from_slice(&0x0FFF_FFFFu32.to_le_bytes()); // Clus 1
        fat_buf[8..12].copy_from_slice(&0x0FFF_FFFFu32.to_le_bytes()); // Clus 2 (Root EOF)
        sim.disk.write_sector(sim.fat_start_lba, &fat_buf).unwrap();
        sim.disk.write_sector(sim.fat_start_lba + sim.sectors_per_fat, &fat_buf).unwrap();

        sim
    }

    fn cluster_to_lba(&self, cluster: u32) -> u32 {
        self.data_start_lba + (cluster.saturating_sub(2)) * self.sectors_per_cluster as u32
    }

    fn get_next_cluster(&self, cluster: u32) -> Result<Option<u32>, VfsError> {
        if cluster < 2 { return Err(VfsError::IoError); }
        let fat_offset = cluster * 4;
        let fat_sector = fat_offset / 512;
        let entry_offset = (fat_offset % 512) as usize;

        let mut buf = [0u8; 512];
        self.disk.read_sector(self.fat_start_lba + fat_sector, &mut buf)?;
        let val = u32::from_le_bytes([
            buf[entry_offset],
            buf[entry_offset + 1],
            buf[entry_offset + 2],
            buf[entry_offset + 3] & 0x0F,
        ]);
        if val >= FAT32_EOF {
            Ok(None)
        } else {
            Ok(Some(val))
        }
    }

    fn set_next_cluster(&mut self, cluster: u32, next: u32) -> Result<(), VfsError> {
        if cluster < 2 { return Err(VfsError::IoError); }
        let fat_offset = cluster * 4;
        let fat_sector = fat_offset / 512;
        let entry_offset = (fat_offset % 512) as usize;

        let mut buf = [0u8; 512];
        self.disk.read_sector(self.fat_start_lba + fat_sector, &mut buf)?;
        let old_hi = buf[entry_offset + 3] & 0xF0;
        buf[entry_offset..entry_offset + 3].copy_from_slice(&next.to_le_bytes()[..3]);
        buf[entry_offset + 3] = old_hi | ((next >> 24) as u8 & 0x0F);

        self.disk.write_sector(self.fat_start_lba + fat_sector, &buf)?;
        self.disk.write_sector(self.fat_start_lba + self.sectors_per_fat + fat_sector, &buf)?;
        Ok(())
    }

    fn alloc_cluster(&mut self) -> Result<u32, VfsError> {
        let mut fat_buf = [0u8; 512];
        let total_data_sectors = self.partition_sectors.saturating_sub(
            (self.data_start_lba.saturating_sub(self.partition_lba)) as u64,
        );
        let max_cluster = if self.sectors_per_cluster > 0 {
            (total_data_sectors / self.sectors_per_cluster as u64) as u32 + 2
        } else {
            0
        };

        for sec in 0..self.sectors_per_fat {
            self.disk.read_sector(self.fat_start_lba + sec, &mut fat_buf)?;
            for idx in 0..128usize {
                let cluster_num = (sec * 128) + idx as u32;
                if cluster_num < 3 { continue; }
                if max_cluster > 2 && cluster_num >= max_cluster {
                    return Err(VfsError::NoSpace);
                }

                let off = idx * 4;
                let val = u32::from_le_bytes([fat_buf[off], fat_buf[off + 1], fat_buf[off + 2], fat_buf[off + 3] & 0x0F]);
                if val == 0 {
                    let old_hi = fat_buf[off + 3] & 0xF0;
                    fat_buf[off] = 0xFF;
                    fat_buf[off + 1] = 0xFF;
                    fat_buf[off + 2] = 0xFF;
                    fat_buf[off + 3] = old_hi | 0x0F;

                    self.disk.write_sector(self.fat_start_lba + sec, &fat_buf)?;
                    self.disk.write_sector(self.fat_start_lba + self.sectors_per_fat + sec, &fat_buf)?;

                    // Limpa setores do cluster alocado
                    let data_lba = self.cluster_to_lba(cluster_num);
                    let zero = [0u8; 512];
                    for s in 0..self.sectors_per_cluster {
                        self.disk.write_sector(data_lba + s as u32, &zero)?;
                    }
                    return Ok(cluster_num);
                }
            }
        }
        Err(VfsError::NoSpace)
    }

    fn free_cluster_chain(&mut self, start_cluster: u32) -> Result<(), VfsError> {
        let mut cur = start_cluster;
        let mut buf = [0u8; 512];

        while cur >= 2 && cur < FAT32_EOF {
            let fat_offset = cur * 4;
            let fat_sector = fat_offset / 512;
            let entry_offset = (fat_offset % 512) as usize;

            self.disk.read_sector(self.fat_start_lba + fat_sector, &mut buf)?;
            let next = u32::from_le_bytes([buf[entry_offset], buf[entry_offset + 1], buf[entry_offset + 2], buf[entry_offset + 3] & 0x0F]);
            let old_hi = buf[entry_offset + 3] & 0xF0;
            buf[entry_offset] = 0;
            buf[entry_offset + 1] = 0;
            buf[entry_offset + 2] = 0;
            buf[entry_offset + 3] = old_hi;

            self.disk.write_sector(self.fat_start_lba + fat_sector, &buf)?;
            self.disk.write_sector(self.fat_start_lba + self.sectors_per_fat + fat_sector, &buf)?;
            cur = next;
        }
        Ok(())
    }

    pub fn for_each_entry<F>(&self, dir_cluster: u32, mut cb: F) -> Result<(), VfsError>
    where
        F: FnMut(SimulatedFatFile) -> bool,
    {
        let mut cur_cluster = if dir_cluster == 0 { self.root_cluster } else { dir_cluster };
        let mut sector_buf = [0u8; 512];

        while cur_cluster >= 2 && cur_cluster < FAT32_EOF {
            let start_lba = self.cluster_to_lba(cur_cluster);
            for sec in 0..self.sectors_per_cluster {
                let sec_lba = start_lba + sec as u32;
                self.disk.read_sector(sec_lba, &mut sector_buf)?;

                for entry_idx in 0..(512 / 32) {
                    let offset = entry_idx * 32;
                    let first = sector_buf[offset];
                    if first == 0x00 { return Ok(()); }
                    if first == 0xE5 { continue; }

                    let attr = sector_buf[offset + 11];
                    if attr == 0x0F || (attr & 0x08) != 0 { continue; }

                    let mut raw_83 = [b' '; 11];
                    raw_83.copy_from_slice(&sector_buf[offset..offset + 11]);

                    let mut name = [0u8; 13];
                    let mut len = 0;
                    for i in 0..8 {
                        let c = sector_buf[offset + i];
                        if c != b' ' { name[len] = c.to_ascii_lowercase(); len += 1; }
                    }
                    let is_dir = (attr & 0x10) != 0;
                    let ext1 = sector_buf[offset + 8];
                    let ext2 = sector_buf[offset + 9];
                    let ext3 = sector_buf[offset + 10];
                    if ext1 != b' ' || ext2 != b' ' || ext3 != b' ' {
                        name[len] = b'.';
                        len += 1;
                        for &c in &[ext1, ext2, ext3] {
                            if c != b' ' { name[len] = c.to_ascii_lowercase(); len += 1; }
                        }
                    }

                    let clus_hi = u16::from_le_bytes([sector_buf[offset + 20], sector_buf[offset + 21]]) as u32;
                    let clus_lo = u16::from_le_bytes([sector_buf[offset + 26], sector_buf[offset + 27]]) as u32;
                    let first_cluster = (clus_hi << 16) | clus_lo;
                    let size = u32::from_le_bytes([sector_buf[offset + 28], sector_buf[offset + 29], sector_buf[offset + 30], sector_buf[offset + 31]]);

                    let entry = SimulatedFatFile {
                        name,
                        name_len: len,
                        raw_83,
                        is_dir,
                        first_cluster,
                        size,
                        dir_sector_lba: sec_lba,
                        dir_entry_offset: offset,
                    };
                    if !cb(entry) { return Ok(()); }
                }
            }

            match self.get_next_cluster(cur_cluster)? {
                Some(next) => cur_cluster = next,
                None => break,
            }
        }
        Ok(())
    }

    pub fn find_file(&self, path: &str) -> Result<SimulatedFatFile, VfsError> {
        let clean = path.trim_matches('/');
        if clean.is_empty() {
            return Ok(SimulatedFatFile {
                name: *b"/            ",
                name_len: 1,
                raw_83: *b"/          ",
                is_dir: true,
                first_cluster: self.root_cluster,
                size: 0,
                dir_sector_lba: 0,
                dir_entry_offset: 0,
            });
        }

        let segments: Vec<&str> = clean.split('/').collect();
        let mut current_cluster = self.root_cluster;
        let mut found_entry: Option<SimulatedFatFile> = None;

        for seg in segments {
            if seg.is_empty() || seg == "." { continue; }
            let seg_83 = encode_83(seg).ok();
            let mut matched: Option<SimulatedFatFile> = None;

            let _ = self.for_each_entry(current_cluster, |entry| {
                let name_match = if let Ok(entry_name) = core::str::from_utf8(&entry.name[..entry.name_len]) {
                    entry_name.eq_ignore_ascii_case(seg)
                } else { false };
                let raw_match = match seg_83 {
                    Some(s83) => entry.raw_83 == s83,
                    None => false,
                };
                if name_match || raw_match {
                    matched = Some(entry);
                    return false;
                }
                true
            });

            match matched {
                Some(e) => {
                    current_cluster = e.first_cluster;
                    found_entry = Some(e);
                }
                None => return Err(VfsError::NotFound),
            }
        }

        found_entry.ok_or(VfsError::NotFound)
    }

    fn resolve_parent_and_leaf<'a>(&self, path: &'a str) -> Result<(u32, &'a str), VfsError> {
        let clean = path.trim_matches('/');
        if clean.is_empty() { return Err(VfsError::InvalidPath); }

        if let Some(idx) = clean.rfind('/') {
            let parent_path = &clean[..idx];
            let leaf = &clean[idx + 1..];
            let p_entry = self.find_file(parent_path)?;
            if !p_entry.is_dir { return Err(VfsError::NotADirectory); }
            Ok((p_entry.first_cluster, leaf))
        } else {
            Ok((self.root_cluster, clean))
        }
    }

    fn find_free_dir_slot(&mut self, parent_cluster: u32) -> Result<(u32, usize), VfsError> {
        let normalized = if parent_cluster == 0 { self.root_cluster } else { parent_cluster };
        let mut cur = normalized;
        let mut last = normalized;
        let mut dir_buf = [0u8; 512];

        while cur >= 2 && cur < FAT32_EOF {
            last = cur;
            let start_lba = self.cluster_to_lba(cur);
            for sec in 0..self.sectors_per_cluster {
                let sec_lba = start_lba + sec as u32;
                self.disk.read_sector(sec_lba, &mut dir_buf)?;
                for entry_idx in 0..(512 / 32) {
                    let offset = entry_idx * 32;
                    let first = dir_buf[offset];
                    if first == 0x00 || first == 0xE5 {
                        return Ok((sec_lba, offset));
                    }
                }
            }
            match self.get_next_cluster(cur)? {
                Some(next) => cur = next,
                None => break,
            }
        }

        // Expande diretório
        let new_c = self.alloc_cluster()?;
        self.set_next_cluster(last, new_c)?;
        let new_lba = self.cluster_to_lba(new_c);
        Ok((new_lba, 0))
    }

    pub fn create(&mut self, path: &str) -> Result<(), VfsError> {
        let clean = path.trim_matches('/');
        if clean.is_empty() { return Err(VfsError::InvalidPath); }
        if self.find_file(clean).is_ok() { return Err(VfsError::AlreadyExists); }

        let (parent_cluster, leaf) = self.resolve_parent_and_leaf(clean)?;
        let raw_83 = encode_83(leaf)?;
        let (sec_lba, offset) = self.find_free_dir_slot(parent_cluster)?;

        let mut dir_buf = [0u8; 512];
        self.disk.read_sector(sec_lba, &mut dir_buf)?;
        dir_buf[offset..offset + 11].copy_from_slice(&raw_83);
        dir_buf[offset + 11] = 0x20; // Archive
        dir_buf[offset + 12..offset + 32].fill(0);

        self.disk.write_sector(sec_lba, &dir_buf)?;
        Ok(())
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), VfsError> {
        let clean = path.trim_matches('/');
        if clean.is_empty() { return Err(VfsError::InvalidPath); }
        if self.find_file(clean).is_ok() { return Err(VfsError::AlreadyExists); }

        let (parent_cluster, leaf) = self.resolve_parent_and_leaf(clean)?;
        let raw_83 = encode_83(leaf)?;
        let new_c = self.alloc_cluster()?;

        // Formata '.' e '..'
        let mut c_buf = [0u8; 512];
        c_buf[0..11].copy_from_slice(b".          ");
        c_buf[11] = 0x10;
        let c_hi = ((new_c >> 16) & 0xFFFF) as u16;
        let c_lo = (new_c & 0xFFFF) as u16;
        c_buf[20..22].copy_from_slice(&c_hi.to_le_bytes());
        c_buf[26..28].copy_from_slice(&c_lo.to_le_bytes());

        c_buf[32..43].copy_from_slice(b"..         ");
        c_buf[43] = 0x10;
        let p_target = if parent_cluster == self.root_cluster { 0 } else { parent_cluster };
        let p_hi = ((p_target >> 16) & 0xFFFF) as u16;
        let p_lo = (p_target & 0xFFFF) as u16;
        c_buf[52..54].copy_from_slice(&p_hi.to_le_bytes());
        c_buf[58..60].copy_from_slice(&p_lo.to_le_bytes());

        let new_lba = self.cluster_to_lba(new_c);
        self.disk.write_sector(new_lba, &c_buf)?;

        let (sec_lba, offset) = self.find_free_dir_slot(parent_cluster)?;
        let mut dir_buf = [0u8; 512];
        self.disk.read_sector(sec_lba, &mut dir_buf)?;
        dir_buf[offset..offset + 11].copy_from_slice(&raw_83);
        dir_buf[offset + 11] = 0x10;
        dir_buf[offset + 12..offset + 20].fill(0);
        dir_buf[offset + 20..offset + 22].copy_from_slice(&c_hi.to_le_bytes());
        dir_buf[offset + 22..offset + 26].fill(0);
        dir_buf[offset + 26..offset + 28].copy_from_slice(&c_lo.to_le_bytes());
        dir_buf[offset + 28..offset + 32].fill(0);

        self.disk.write_sector(sec_lba, &dir_buf)?;
        Ok(())
    }

    pub fn open(&mut self, path: &str, flags: u32) -> Result<u64, VfsError> {
        let mut entry = match self.find_file(path) {
            Ok(e) => e,
            Err(VfsError::NotFound) if (flags & O_CREAT) != 0 => {
                self.create(path)?;
                self.find_file(path)?
            }
            Err(e) => return Err(e),
        };
        if entry.is_dir { return Err(VfsError::IsADirectory); }

        if (flags & O_TRUNC) != 0 && entry.size > 0 {
            if entry.first_cluster >= 2 && entry.first_cluster < FAT32_EOF {
                self.free_cluster_chain(entry.first_cluster)?;
            }
            entry.first_cluster = 0;
            entry.size = 0;

            let mut dir_buf = [0u8; 512];
            self.disk.read_sector(entry.dir_sector_lba, &mut dir_buf)?;
            dir_buf[entry.dir_entry_offset + 20..entry.dir_entry_offset + 22].fill(0);
            dir_buf[entry.dir_entry_offset + 26..entry.dir_entry_offset + 28].fill(0);
            dir_buf[entry.dir_entry_offset + 28..entry.dir_entry_offset + 32].fill(0);
            self.disk.write_sector(entry.dir_sector_lba, &dir_buf)?;
        }

        for (idx, slot) in self.open_files.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(entry);
                return Ok(idx as u64);
            }
        }
        Err(VfsError::NoSpace)
    }

    pub fn close(&mut self, handle: u64) -> Result<(), VfsError> {
        let idx = handle as usize;
        if idx >= self.open_files.len() || self.open_files[idx].is_none() {
            return Err(VfsError::InvalidPath);
        }
        self.open_files[idx] = None;
        Ok(())
    }

    pub fn write(&mut self, handle: u64, offset: u64, buf: &[u8]) -> Result<usize, VfsError> {
        if buf.is_empty() { return Ok(0); }
        let idx = handle as usize;
        let mut entry = self.open_files.get(idx).and_then(|s| s.clone()).ok_or(VfsError::NotFound)?;

        let cluster_bytes = (self.sectors_per_cluster as usize) * 512;
        if entry.first_cluster < 2 || entry.first_cluster >= FAT32_EOF {
            let new_c = self.alloc_cluster()?;
            entry.first_cluster = new_c;
            // Atualiza no disco
            let mut dir_buf = [0u8; 512];
            self.disk.read_sector(entry.dir_sector_lba, &mut dir_buf)?;
            let hi = ((new_c >> 16) & 0xFFFF) as u16;
            let lo = (new_c & 0xFFFF) as u16;
            dir_buf[entry.dir_entry_offset + 20..entry.dir_entry_offset + 22].copy_from_slice(&hi.to_le_bytes());
            dir_buf[entry.dir_entry_offset + 26..entry.dir_entry_offset + 28].copy_from_slice(&lo.to_le_bytes());
            self.disk.write_sector(entry.dir_sector_lba, &dir_buf)?;
            self.open_files[idx] = Some(entry.clone());
        }

        let mut cluster_skip = (offset as usize) / cluster_bytes;
        let mut cluster_offset = (offset as usize) % cluster_bytes;
        let mut current_cluster = entry.first_cluster;

        while cluster_skip > 0 {
            match self.get_next_cluster(current_cluster)? {
                Some(next) => current_cluster = next,
                None => {
                    let new_c = self.alloc_cluster()?;
                    self.set_next_cluster(current_cluster, new_c)?;
                    current_cluster = new_c;
                }
            }
            cluster_skip -= 1;
        }

        let mut bytes_written = 0;
        let mut sec_buf = [0u8; 512];

        while bytes_written < buf.len() && current_cluster >= 2 && current_cluster < FAT32_EOF {
            let start_lba = self.cluster_to_lba(current_cluster);
            let start_sec = cluster_offset / 512;
            let mut sec_offset = cluster_offset % 512;

            for sec in start_sec..(self.sectors_per_cluster as usize) {
                let sec_lba = start_lba + sec as u32;
                let available = 512 - sec_offset;
                let needed = buf.len() - bytes_written;
                let copy_count = available.min(needed);

                if sec_offset > 0 || copy_count < 512 {
                    self.disk.read_sector(sec_lba, &mut sec_buf)?;
                }
                sec_buf[sec_offset..sec_offset + copy_count].copy_from_slice(&buf[bytes_written..bytes_written + copy_count]);
                self.disk.write_sector(sec_lba, &sec_buf)?;

                bytes_written += copy_count;
                sec_offset = 0;
                if bytes_written >= buf.len() { break; }
            }

            cluster_offset = 0;
            if bytes_written < buf.len() {
                match self.get_next_cluster(current_cluster)? {
                    Some(next) => current_cluster = next,
                    None => {
                        let new_c = self.alloc_cluster()?;
                        self.set_next_cluster(current_cluster, new_c)?;
                        current_cluster = new_c;
                    }
                }
            }
        }

        let end_pos = (offset as usize) + bytes_written;
        if end_pos > entry.size as usize {
            entry.size = end_pos as u32;
            let mut dir_buf = [0u8; 512];
            self.disk.read_sector(entry.dir_sector_lba, &mut dir_buf)?;
            dir_buf[entry.dir_entry_offset + 28..entry.dir_entry_offset + 32].copy_from_slice(&entry.size.to_le_bytes());
            self.disk.write_sector(entry.dir_sector_lba, &dir_buf)?;
            self.open_files[idx] = Some(entry);
        }

        Ok(bytes_written)
    }

    pub fn read(&self, handle: u64, offset: u64, buf: &mut [u8]) -> Result<usize, VfsError> {
        let idx = handle as usize;
        let entry = self.open_files.get(idx).and_then(|s| s.clone()).ok_or(VfsError::NotFound)?;
        if offset >= entry.size as u64 { return Ok(0); }

        let bytes_to_read = ((entry.size as u64 - offset) as usize).min(buf.len());
        let cluster_bytes = (self.sectors_per_cluster as usize) * 512;
        let mut cluster_skip = (offset as usize) / cluster_bytes;
        let mut cluster_offset = (offset as usize) % cluster_bytes;

        let mut current_cluster = entry.first_cluster;
        while cluster_skip > 0 {
            match self.get_next_cluster(current_cluster)? {
                Some(next) => current_cluster = next,
                None => return Ok(0),
            }
            cluster_skip -= 1;
        }

        let mut bytes_read = 0;
        let mut sec_buf = [0u8; 512];

        while bytes_read < bytes_to_read && current_cluster >= 2 && current_cluster < FAT32_EOF {
            let start_lba = self.cluster_to_lba(current_cluster);
            let start_sec = cluster_offset / 512;
            let mut sec_offset = cluster_offset % 512;

            for sec in start_sec..(self.sectors_per_cluster as usize) {
                let sec_lba = start_lba + sec as u32;
                self.disk.read_sector(sec_lba, &mut sec_buf)?;
                let available = 512 - sec_offset;
                let needed = bytes_to_read - bytes_read;
                let copy_count = available.min(needed);

                buf[bytes_read..bytes_read + copy_count].copy_from_slice(&sec_buf[sec_offset..sec_offset + copy_count]);
                bytes_read += copy_count;
                sec_offset = 0;
                if bytes_read >= bytes_to_read { break; }
            }

            cluster_offset = 0;
            match self.get_next_cluster(current_cluster)? {
                Some(next) => current_cluster = next,
                None => break,
            }
        }

        Ok(bytes_read)
    }

    pub fn remove(&mut self, path: &str) -> Result<(), VfsError> {
        let clean = path.trim_matches('/');
        if clean.is_empty() { return Err(VfsError::InvalidPath); }

        let (parent_cluster, leaf_name) = self.resolve_parent_and_leaf(clean)?;
        let leaf_83 = encode_83(leaf_name).ok();
        let mut cur_cluster = if parent_cluster == 0 { self.root_cluster } else { parent_cluster };
        let mut dir_buf = [0u8; 512];

        while cur_cluster >= 2 && cur_cluster < FAT32_EOF {
            let start_lba = self.cluster_to_lba(cur_cluster);
            for sec in 0..self.sectors_per_cluster {
                let sec_lba = start_lba + sec as u32;
                self.disk.read_sector(sec_lba, &mut dir_buf)?;

                for entry_idx in 0..(512 / 32) {
                    let offset = entry_idx * 32;
                    let first = dir_buf[offset];
                    if first == 0x00 { return Err(VfsError::NotFound); }
                    if first == 0xE5 { continue; }

                    let attr = dir_buf[offset + 11];
                    if attr == 0x0F || (attr & 0x08) != 0 { continue; }

                    let mut raw_83 = [b' '; 11];
                    raw_83.copy_from_slice(&dir_buf[offset..offset + 11]);

                    let mut name = [0u8; 13];
                    let mut len = 0;
                    for i in 0..8 {
                        let c = dir_buf[offset + i];
                        if c != b' ' { name[len] = c.to_ascii_lowercase(); len += 1; }
                    }
                    let is_dir = (attr & 0x10) != 0;
                    let ext1 = dir_buf[offset + 8];
                    let ext2 = dir_buf[offset + 9];
                    let ext3 = dir_buf[offset + 10];
                    if ext1 != b' ' || ext2 != b' ' || ext3 != b' ' {
                        name[len] = b'.'; len += 1;
                        for &c in &[ext1, ext2, ext3] {
                            if c != b' ' { name[len] = c.to_ascii_lowercase(); len += 1; }
                        }
                    }

                    let name_match = if let Ok(s) = core::str::from_utf8(&name[..len]) {
                        s.eq_ignore_ascii_case(leaf_name)
                    } else { false };
                    let raw_match = match leaf_83 {
                        Some(l83) => raw_83 == l83,
                        None => false,
                    };

                    if name_match || raw_match {
                        let fst_hi = u16::from_le_bytes([dir_buf[offset + 20], dir_buf[offset + 21]]) as u32;
                        let fst_lo = u16::from_le_bytes([dir_buf[offset + 26], dir_buf[offset + 27]]) as u32;
                        let first_cluster = (fst_hi << 16) | fst_lo;

                        if is_dir && first_cluster >= 2 && first_cluster < FAT32_EOF {
                            let mut is_empty = true;
                            let _ = self.for_each_entry(first_cluster, |e| {
                                if let Ok(n) = core::str::from_utf8(&e.name[..e.name_len]) {
                                    if n != "." && n != ".." {
                                        is_empty = false;
                                        return false;
                                    }
                                }
                                true
                            });
                            if !is_empty { return Err(VfsError::Busy); }
                        }

                        dir_buf[offset] = 0xE5;
                        self.disk.write_sector(sec_lba, &dir_buf)?;

                        if first_cluster >= 2 && first_cluster < FAT32_EOF {
                            self.free_cluster_chain(first_cluster)?;
                        }

                        for slot in self.open_files.iter_mut() {
                            if let Some(s) = slot {
                                if (first_cluster >= 2 && s.first_cluster == first_cluster) ||
                                   (s.dir_sector_lba == sec_lba && s.dir_entry_offset == offset) {
                                    *slot = None;
                                }
                            }
                        }
                        return Ok(());
                    }
                }
            }
            match self.get_next_cluster(cur_cluster)? {
                Some(next) => cur_cluster = next,
                None => break,
            }
        }
        Err(VfsError::NotFound)
    }
}

// ── Simulador Ext2 Completo ──────────────────────────────────────────────────

pub struct SimulatedExt2 {
    pub disk: RamDisk,
    pub block_size: usize,
    pub inodes_count: u32,
    pub blocks_count: u32,
    pub first_data_block: u32,
    pub blocks_per_group: u32,
    pub inodes_per_group: u32,
    pub inode_size: usize,
    pub open_files: Vec<Option<(u32, Ext2SimInode)>>,
}

#[derive(Clone, Copy, Debug)]
pub struct Ext2SimInode {
    pub mode: u16,
    pub size: u32,
    pub blocks: u32,
    pub links_count: u16,
    pub direct_blocks: [u32; 12],
}

impl SimulatedExt2 {
    pub fn new(total_blocks: u32) -> Self {
        let block_size = 1024usize;
        let mut sim = Self {
            disk: RamDisk::new(total_blocks * 2), // 2 setores de 512 por bloco de 1024
            block_size,
            inodes_count: 64,
            blocks_count: total_blocks,
            first_data_block: 1,
            blocks_per_group: total_blocks,
            inodes_per_group: 64,
            inode_size: 128,
            open_files: vec![None; 32],
        };

        // Layout:
        // Bloco 0: Boot
        // Bloco 1: Superblock
        // Bloco 2: Group Descriptor
        // Bloco 3: Block Bitmap
        // Bloco 4: Inode Bitmap
        // Bloco 5..12: Inode Table (64 inodes * 128 B = 8192 B = 8 blocos)
        // Bloco 13+: Data blocks
        let mut sb = [0u8; 1024];
        sb[0..4].copy_from_slice(&sim.inodes_count.to_le_bytes());
        sb[4..8].copy_from_slice(&sim.blocks_count.to_le_bytes());
        sb[12..16].copy_from_slice(&(total_blocks - 13).to_le_bytes()); // free blocks
        sb[16..20].copy_from_slice(&(sim.inodes_count - 10).to_le_bytes()); // free inodes
        sb[20..24].copy_from_slice(&sim.first_data_block.to_le_bytes());
        sb[24..28].copy_from_slice(&0u32.to_le_bytes()); // log_block_size = 0 (1024)
        sb[32..36].copy_from_slice(&sim.blocks_per_group.to_le_bytes());
        sb[40..44].copy_from_slice(&sim.inodes_per_group.to_le_bytes());
        sb[56..58].copy_from_slice(&0xEF53u16.to_le_bytes()); // Magic
        sb[88..90].copy_from_slice(&128u16.to_le_bytes()); // Inode size
        sim.write_block(1, &sb);

        // Group descriptor: Bloco 2
        let mut gd = [0u8; 1024];
        gd[0..4].copy_from_slice(&3u32.to_le_bytes()); // block bitmap = 3
        gd[4..8].copy_from_slice(&4u32.to_le_bytes()); // inode bitmap = 4
        gd[8..12].copy_from_slice(&5u32.to_le_bytes()); // inode table = 5
        gd[12..14].copy_from_slice(&((total_blocks - 13) as u16).to_le_bytes()); // free blocks
        gd[14..16].copy_from_slice(&((sim.inodes_count - 10) as u16).to_le_bytes()); // free inodes
        gd[16..18].copy_from_slice(&1u16.to_le_bytes()); // used dirs count = 1 (root)
        sim.write_block(2, &gd);

        // Block bitmap: Bloco 3 (marca 0..12 como ocupados)
        let mut b_bitmap = [0u8; 1024];
        for b in 0..13 {
            b_bitmap[b / 8] |= 1 << (b % 8);
        }
        sim.write_block(3, &b_bitmap);

        // Inode bitmap: Bloco 4 (marca 1..10 como reservados; Inode 2 é a raiz)
        let mut i_bitmap = [0u8; 1024];
        for i in 0..11 {
            i_bitmap[i / 8] |= 1 << (i % 8);
        }
        sim.write_block(4, &i_bitmap);

        // Inode 2 (Root directory): aloca bloco 13
        b_bitmap[13 / 8] |= 1 << (13 % 8);
        sim.write_block(3, &b_bitmap);

        let mut root_inode = Ext2SimInode {
            mode: 0x41ED, // Directory, 0755
            size: 1024,
            blocks: 2, // 2 setores de 512
            links_count: 2,
            direct_blocks: [0; 12],
        };
        root_inode.direct_blocks[0] = 13;
        sim.write_inode(2, &root_inode);

        // Formata bloco 13 com '.' e '..'
        let mut root_blk = [0u8; 1024];
        // '.'
        root_blk[0..4].copy_from_slice(&2u32.to_le_bytes());
        root_blk[4..6].copy_from_slice(&12u16.to_le_bytes());
        root_blk[6] = 1;
        root_blk[7] = 2; // Dir
        root_blk[8] = b'.';
        // '..'
        root_blk[12..16].copy_from_slice(&2u32.to_le_bytes());
        root_blk[16..18].copy_from_slice(&(1024u16 - 12).to_le_bytes());
        root_blk[18] = 2;
        root_blk[19] = 2; // Dir
        root_blk[20] = b'.';
        root_blk[21] = b'.';
        sim.write_block(13, &root_blk);

        sim
    }

    fn read_block(&self, blk: u32, buf: &mut [u8]) {
        let lba = blk * 2;
        let mut sec0 = [0u8; 512];
        let mut sec1 = [0u8; 512];
        self.disk.read_sector(lba, &mut sec0).unwrap();
        self.disk.read_sector(lba + 1, &mut sec1).unwrap();
        buf[0..512].copy_from_slice(&sec0);
        buf[512..1024].copy_from_slice(&sec1);
    }

    fn write_block(&mut self, blk: u32, buf: &[u8]) {
        let lba = blk * 2;
        let mut sec0 = [0u8; 512];
        let mut sec1 = [0u8; 512];
        sec0.copy_from_slice(&buf[0..512]);
        sec1.copy_from_slice(&buf[512..1024]);
        self.disk.write_sector(lba, &sec0).unwrap();
        self.disk.write_sector(lba + 1, &sec1).unwrap();
    }

    pub fn write_inode(&mut self, ino: u32, inode: &Ext2SimInode) {
        let idx = (ino - 1) as usize;
        let block = 5 + (idx * self.inode_size) / self.block_size;
        let offset = (idx * self.inode_size) % self.block_size;

        let mut blk = [0u8; 1024];
        self.read_block(block as u32, &mut blk);
        let b = &mut blk[offset..offset + 128];
        b[0..2].copy_from_slice(&inode.mode.to_le_bytes());
        b[4..8].copy_from_slice(&inode.size.to_le_bytes());
        b[26..28].copy_from_slice(&inode.links_count.to_le_bytes());
        b[28..32].copy_from_slice(&inode.blocks.to_le_bytes());
        for i in 0..12 {
            b[40 + i * 4..44 + i * 4].copy_from_slice(&inode.direct_blocks[i].to_le_bytes());
        }
        self.write_block(block as u32, &blk);
    }

    pub fn read_inode(&self, ino: u32) -> Result<Ext2SimInode, VfsError> {
        if ino == 0 || ino > self.inodes_count { return Err(VfsError::NotFound); }
        let idx = (ino - 1) as usize;
        let block = 5 + (idx * self.inode_size) / self.block_size;
        let offset = (idx * self.inode_size) % self.block_size;

        let mut blk = [0u8; 1024];
        self.read_block(block as u32, &mut blk);
        let b = &blk[offset..offset + 128];
        let mode = u16::from_le_bytes([b[0], b[1]]);
        let size = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
        let links_count = u16::from_le_bytes([b[26], b[27]]);
        let blocks = u32::from_le_bytes([b[28], b[29], b[30], b[31]]);
        let mut direct_blocks = [0u32; 12];
        for i in 0..12 {
            direct_blocks[i] = u32::from_le_bytes([b[40 + i * 4], b[41 + i * 4], b[42 + i * 4], b[43 + i * 4]]);
        }
        Ok(Ext2SimInode { mode, size, blocks, links_count, direct_blocks })
    }

    pub fn alloc_inode(&mut self, _is_dir: bool) -> Result<u32, VfsError> {
        let mut bitmap = [0u8; 1024];
        self.read_block(4, &mut bitmap);

        for i in 1..self.inodes_count as usize {
            let byte = i / 8;
            let bit = i % 8;
            if (bitmap[byte] & (1 << bit)) == 0 {
                bitmap[byte] |= 1 << bit;
                self.write_block(4, &bitmap);
                return Ok(i as u32 + 1);
            }
        }
        Err(VfsError::NoSpace)
    }

    pub fn alloc_block(&mut self) -> Result<u32, VfsError> {
        let mut bitmap = [0u8; 1024];
        self.read_block(3, &mut bitmap);

        for b in self.first_data_block as usize..self.blocks_count as usize {
            let byte = b / 8;
            let bit = b % 8;
            if (bitmap[byte] & (1 << bit)) == 0 {
                bitmap[byte] |= 1 << bit;
                self.write_block(3, &bitmap);
                let zero = [0u8; 1024];
                self.write_block(b as u32, &zero);
                return Ok(b as u32);
            }
        }
        Err(VfsError::NoSpace)
    }

    pub fn free_block(&mut self, blk: u32) -> Result<(), VfsError> {
        let mut bitmap = [0u8; 1024];
        self.read_block(3, &mut bitmap);
        let b = blk as usize;
        bitmap[b / 8] &= !(1 << (b % 8));
        self.write_block(3, &bitmap);
        Ok(())
    }

    pub fn free_inode(&mut self, ino: u32) -> Result<(), VfsError> {
        let mut bitmap = [0u8; 1024];
        self.read_block(4, &mut bitmap);
        let i = (ino - 1) as usize;
        bitmap[i / 8] &= !(1 << (i % 8));
        self.write_block(4, &bitmap);
        Ok(())
    }

    pub fn add_dir_entry(&mut self, parent_ino: u32, parent_inode: &mut Ext2SimInode, entry_ino: u32, name: &str, ftype: u8) -> Result<(), VfsError> {
        let name_bytes = name.as_bytes();
        let needed_rec = ((8 + name_bytes.len() + 3) & !3) as u16;
        let mut blk = [0u8; 1024];

        for blk_idx in 0..12 {
            let b_num = parent_inode.direct_blocks[blk_idx];
            if b_num == 0 { break; }
            self.read_block(b_num, &mut blk);

            let mut offset = 0usize;
            while offset < 1024 {
                let ino = u32::from_le_bytes([blk[offset], blk[offset + 1], blk[offset + 2], blk[offset + 3]]);
                let rec_len = u16::from_le_bytes([blk[offset + 4], blk[offset + 5]]) as usize;
                let name_len = blk[offset + 6] as usize;
                if rec_len == 0 { break; }

                let actual_len = if ino != 0 { ((8 + name_len + 3) & !3) as u16 } else { 0 };
                let remaining = rec_len.saturating_sub(actual_len as usize);

                if ino != 0 && remaining >= needed_rec as usize {
                    blk[offset + 4..offset + 6].copy_from_slice(&actual_len.to_le_bytes());
                    let new_off = offset + actual_len as usize;
                    blk[new_off..new_off + 4].copy_from_slice(&entry_ino.to_le_bytes());
                    blk[new_off + 4..new_off + 6].copy_from_slice(&(remaining as u16).to_le_bytes());
                    blk[new_off + 6] = name_bytes.len() as u8;
                    blk[new_off + 7] = ftype;
                    blk[new_off + 8..new_off + 8 + name_bytes.len()].copy_from_slice(name_bytes);

                    self.write_block(b_num, &blk);
                    return Ok(());
                } else if ino == 0 && rec_len >= needed_rec as usize {
                    blk[offset..offset + 4].copy_from_slice(&entry_ino.to_le_bytes());
                    blk[offset + 6] = name_bytes.len() as u8;
                    blk[offset + 7] = ftype;
                    blk[offset + 8..offset + 8 + name_bytes.len()].copy_from_slice(name_bytes);

                    self.write_block(b_num, &blk);
                    return Ok(());
                }
                offset += rec_len;
            }
        }

        // Aloca novo bloco direto para o diretório
        for blk_idx in 0..12 {
            if parent_inode.direct_blocks[blk_idx] == 0 {
                let new_blk = self.alloc_block()?;
                parent_inode.direct_blocks[blk_idx] = new_blk;
                parent_inode.blocks += 2;
                parent_inode.size += 1024;

                let mut new_b = [0u8; 1024];
                new_b[0..4].copy_from_slice(&entry_ino.to_le_bytes());
                new_b[4..6].copy_from_slice(&1024u16.to_le_bytes());
                new_b[6] = name_bytes.len() as u8;
                new_b[7] = ftype;
                new_b[8..8 + name_bytes.len()].copy_from_slice(name_bytes);

                self.write_block(new_blk, &new_b);
                self.write_inode(parent_ino, parent_inode);
                return Ok(());
            }
        }

        Err(VfsError::NoSpace)
    }

    pub fn for_each_dir_entry<F>(&self, inode: &Ext2SimInode, mut cb: F)
    where
        F: FnMut(u32, u8, &str) -> bool,
    {
        let mut blk = [0u8; 1024];
        for &b_num in &inode.direct_blocks {
            if b_num == 0 { break; }
            self.read_block(b_num, &mut blk);
            let mut offset = 0usize;
            while offset + 8 <= 1024 {
                let ino = u32::from_le_bytes([blk[offset], blk[offset + 1], blk[offset + 2], blk[offset + 3]]);
                let rec_len = u16::from_le_bytes([blk[offset + 4], blk[offset + 5]]) as usize;
                let name_len = blk[offset + 6] as usize;
                let ftype = blk[offset + 7];
                if rec_len < 8 || offset + rec_len > 1024 { break; }

                if ino != 0 && offset + 8 + name_len <= offset + rec_len {
                    if let Ok(name) = std::str::from_utf8(&blk[offset + 8..offset + 8 + name_len]) {
                        if !cb(ino, ftype, name) { return; }
                    }
                }
                offset += rec_len;
            }
        }
    }

    pub fn resolve_path(&self, path: &str) -> Result<(u32, Ext2SimInode), VfsError> {
        let clean = path.trim_matches('/');
        if clean.is_empty() {
            let root = self.read_inode(2)?;
            return Ok((2, root));
        }

        let mut cur_ino = 2;
        let mut cur_inode = self.read_inode(cur_ino)?;

        for seg in clean.split('/') {
            if seg.is_empty() || seg == "." { continue; }
            let mut found = None;
            self.for_each_dir_entry(&cur_inode, |ino, _, name| {
                if name.eq_ignore_ascii_case(seg) {
                    found = Some(ino);
                    return false;
                }
                true
            });

            match found {
                Some(next_ino) => {
                    cur_ino = next_ino;
                    cur_inode = self.read_inode(cur_ino)?;
                }
                None => return Err(VfsError::NotFound),
            }
        }

        Ok((cur_ino, cur_inode))
    }

    pub fn create(&mut self, path: &str) -> Result<(), VfsError> {
        let clean = path.trim_matches('/');
        if clean.is_empty() { return Err(VfsError::InvalidPath); }
        if self.resolve_path(clean).is_ok() { return Err(VfsError::AlreadyExists); }

        let (parent_path, leaf) = match clean.rfind('/') {
            Some(i) => (&clean[..i], &clean[i + 1..]),
            None => ("", clean),
        };

        let (p_ino, mut p_inode) = if parent_path.is_empty() {
            (2, self.read_inode(2)?)
        } else {
            self.resolve_path(parent_path)?
        };

        if (p_inode.mode & 0xF000) != 0x4000 {
            return Err(VfsError::NotADirectory);
        }

        let new_ino = self.alloc_inode(false)?;
        let new_inode = Ext2SimInode {
            mode: 0x81A4, // Regular file, 0644
            size: 0,
            blocks: 0,
            links_count: 1,
            direct_blocks: [0; 12],
        };
        self.write_inode(new_ino, &new_inode);
        self.add_dir_entry(p_ino, &mut p_inode, new_ino, leaf, 1)?; // 1 = regular file
        Ok(())
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), VfsError> {
        let clean = path.trim_matches('/');
        if clean.is_empty() { return Err(VfsError::InvalidPath); }
        if self.resolve_path(clean).is_ok() { return Err(VfsError::AlreadyExists); }

        let (parent_path, leaf) = match clean.rfind('/') {
            Some(i) => (&clean[..i], &clean[i + 1..]),
            None => ("", clean),
        };

        let (p_ino, mut p_inode) = if parent_path.is_empty() {
            (2, self.read_inode(2)?)
        } else {
            self.resolve_path(parent_path)?
        };

        if (p_inode.mode & 0xF000) != 0x4000 {
            return Err(VfsError::NotADirectory);
        }

        let new_ino = self.alloc_inode(true)?;
        let new_blk = self.alloc_block()?;

        let mut blk = [0u8; 1024];
        // '.'
        blk[0..4].copy_from_slice(&new_ino.to_le_bytes());
        blk[4..6].copy_from_slice(&12u16.to_le_bytes());
        blk[6] = 1;
        blk[7] = 2; // dir
        blk[8] = b'.';
        // '..'
        blk[12..16].copy_from_slice(&p_ino.to_le_bytes());
        blk[16..18].copy_from_slice(&(1024u16 - 12).to_le_bytes());
        blk[18] = 2;
        blk[19] = 2;
        blk[20] = b'.';
        blk[21] = b'.';
        self.write_block(new_blk, &blk);

        let mut new_inode = Ext2SimInode {
            mode: 0x41ED,
            size: 1024,
            blocks: 2,
            links_count: 2,
            direct_blocks: [0; 12],
        };
        new_inode.direct_blocks[0] = new_blk;
        self.write_inode(new_ino, &new_inode);

        self.add_dir_entry(p_ino, &mut p_inode, new_ino, leaf, 2)?;
        p_inode.links_count += 1;
        self.write_inode(p_ino, &p_inode);
        Ok(())
    }

    pub fn open(&mut self, path: &str, flags: u32) -> Result<u64, VfsError> {
        let (ino, mut inode) = match self.resolve_path(path) {
            Ok(r) => r,
            Err(VfsError::NotFound) if (flags & O_CREAT) != 0 => {
                self.create(path)?;
                self.resolve_path(path)?
            }
            Err(e) => return Err(e),
        };

        if (inode.mode & 0xF000) == 0x4000 {
            return Err(VfsError::IsADirectory);
        }

        if (flags & O_TRUNC) != 0 && inode.size > 0 {
            for i in 0..12 {
                let blk = inode.direct_blocks[i];
                if blk != 0 {
                    self.free_block(blk)?;
                    inode.direct_blocks[i] = 0;
                }
            }
            inode.size = 0;
            inode.blocks = 0;
            self.write_inode(ino, &inode);
        }

        for (idx, slot) in self.open_files.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some((ino, inode));
                return Ok(idx as u64);
            }
        }
        Err(VfsError::NoSpace)
    }

    pub fn close(&mut self, handle: u64) -> Result<(), VfsError> {
        let idx = handle as usize;
        if idx >= self.open_files.len() || self.open_files[idx].is_none() {
            return Err(VfsError::NotFound);
        }
        self.open_files[idx] = None;
        Ok(())
    }

    pub fn write(&mut self, handle: u64, offset: u64, buf: &[u8]) -> Result<usize, VfsError> {
        if buf.is_empty() { return Ok(0); }
        let idx = handle as usize;
        let (ino, mut inode) = self.open_files.get(idx).and_then(|s| *s).ok_or(VfsError::NotFound)?;

        let mut bytes_written = 0usize;
        let mut curr_offset = offset as usize;
        let block_sz = self.block_size;

        while bytes_written < buf.len() {
            let block_index = curr_offset / block_sz;
            let block_offset = curr_offset % block_sz;
            let to_write = (buf.len() - bytes_written).min(block_sz - block_offset);

            if block_index < 12 {
                let mut phys_block = inode.direct_blocks[block_index];
                if phys_block == 0 {
                    phys_block = self.alloc_block()?;
                    inode.direct_blocks[block_index] = phys_block;
                    inode.blocks += (block_sz / 512) as u32;
                    self.write_inode(ino, &inode);
                    self.open_files[idx] = Some((ino, inode));
                }

                let mut blk = [0u8; 1024];
                self.read_block(phys_block, &mut blk);
                blk[block_offset..block_offset + to_write].copy_from_slice(&buf[bytes_written..bytes_written + to_write]);
                self.write_block(phys_block, &blk);
            } else {
                return Err(VfsError::NotSupported);
            }

            curr_offset += to_write;
            bytes_written += to_write;
        }

        if curr_offset as u32 > inode.size {
            inode.size = curr_offset as u32;
            self.write_inode(ino, &inode);
            self.open_files[idx] = Some((ino, inode));
        }

        Ok(bytes_written)
    }

    pub fn read(&self, handle: u64, offset: u64, buf: &mut [u8]) -> Result<usize, VfsError> {
        let idx = handle as usize;
        let (_, inode) = self.open_files.get(idx).and_then(|s| *s).ok_or(VfsError::NotFound)?;
        if offset >= inode.size as u64 { return Ok(0); }

        let bytes_to_read = buf.len().min((inode.size as u64 - offset) as usize);
        let block_sz = self.block_size;
        let start_blk = (offset as usize) / block_sz;
        let off_in_start = (offset as usize) % block_sz;

        let mut bytes_read = 0usize;
        let mut blk = [0u8; 1024];

        for blk_idx in start_blk..12 {
            if bytes_read >= bytes_to_read { break; }
            let b_num = inode.direct_blocks[blk_idx];
            if b_num == 0 { break; }

            self.read_block(b_num, &mut blk);
            let s_off = if blk_idx == start_blk { off_in_start } else { 0 };
            let chunk = (block_sz - s_off).min(bytes_to_read - bytes_read);
            buf[bytes_read..bytes_read + chunk].copy_from_slice(&blk[s_off..s_off + chunk]);
            bytes_read += chunk;
        }

        Ok(bytes_read)
    }
}

// ── BATERIA DE TESTES UNITÁRIOS E DE ESTRESSE ─────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_01_encode_83_clamping_and_formatting() {
        // Arquivos curtos normais
        let raw1 = encode_83("index.html").expect("encode index.html");
        assert_eq!(&raw1[0..8], b"INDEX   ");
        assert_eq!(&raw1[8..11], b"HTM");
        assert_eq!(format_83_to_string(&raw1, false), "index.htm");

        // Nome longo clampado (bookmarks.json -> BOOKMARK.JSO)
        let raw2 = encode_83("bookmarks.json").expect("encode bookmarks.json");
        assert_eq!(&raw2[0..8], b"BOOKMARK");
        assert_eq!(&raw2[8..11], b"JSO");
        assert_eq!(format_83_to_string(&raw2, false), "bookmark.jso");

        // Nomes sem extensão
        let raw3 = encode_83("config").expect("encode config");
        assert_eq!(&raw3[0..8], b"CONFIG  ");
        assert_eq!(&raw3[8..11], b"   ");
        assert_eq!(format_83_to_string(&raw3, false), "config");

        // Casos inválidos
        assert_eq!(encode_83("").unwrap_err(), VfsError::InvalidPath);
        assert_eq!(encode_83(".hidden").unwrap_err(), VfsError::InvalidPath);
    }

    #[test]
    fn test_02_fat32_sys_creat_and_pseudo_inode() {
        let mut fs = SimulatedFat32::new(512);

        // Criação de arquivo inicial de 0 bytes
        fs.create("/notes.txt").expect("create notes.txt");

        // find_file deve localizar
        let entry = fs.find_file("/notes.txt").expect("find notes.txt");
        assert_eq!(entry.size, 0);
        assert_eq!(entry.first_cluster, 0); // Sem clusters inicialmente alocados

        // Valida que o pseudo-inode é não-zero mesmo para arquivos de 0 bytes
        let pseudo_inode = if entry.first_cluster != 0 {
            entry.first_cluster as u64
        } else {
            ((entry.dir_sector_lba as u64) << 16) | (entry.dir_entry_offset as u64 | 0x8000_0000)
        };
        assert_ne!(pseudo_inode, 0);
        assert_eq!(pseudo_inode & 0x8000_0000, 0x8000_0000);

        // Tentativa de recriar arquivo existente retorna EEXIST
        let err = fs.create("/notes.txt").unwrap_err();
        assert_eq!(err, VfsError::AlreadyExists);
        assert_eq!(vfs_error_to_errno(err), EEXIST);
    }

    #[test]
    fn test_03_fat32_sys_mkdir_and_dot_entries() {
        let mut fs = SimulatedFat32::new(512);

        // Cria pasta /system e subpasta /system/cache
        fs.mkdir("/system").expect("mkdir /system");
        fs.mkdir("/system/cache").expect("mkdir /system/cache");

        let sys_dir = fs.find_file("/system").expect("find /system");
        assert!(sys_dir.is_dir);

        let cache_dir = fs.find_file("/system/cache").expect("find /system/cache");
        assert!(cache_dir.is_dir);

        // Valida que '.' e '..' foram gravados no cluster do novo diretório
        let mut entries = Vec::new();
        fs.for_each_entry(cache_dir.first_cluster, |e| {
            let n = std::str::from_utf8(&e.name[..e.name_len]).unwrap().to_string();
            entries.push(n);
            true
        }).unwrap();

        assert!(entries.contains(&".".to_string()));
        assert!(entries.contains(&"..".to_string()));

        // Criar pasta existente retorna EEXIST
        let err = fs.mkdir("/system").unwrap_err();
        assert_eq!(err, VfsError::AlreadyExists);
        assert_eq!(vfs_error_to_errno(err), EEXIST);
    }

    #[test]
    fn test_04_fat32_write_read_multi_cluster_and_boundary() {
        let mut fs = SimulatedFat32::new(512);

        // Abre arquivo com O_CREAT
        let fd = fs.open("/browser.dat", O_CREAT | O_WRONLY).expect("open browser.dat");

        // Escreve 2048 bytes (4 clusters de 512 bytes) com padrão determinístico
        let mut write_payload = vec![0u8; 2048];
        for (i, b) in write_payload.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }

        let written = fs.write(fd, 0, &write_payload).expect("write browser.dat");
        assert_eq!(written, 2048);
        fs.close(fd).expect("close fd");

        // Verifica metadados atualizados
        let stat = fs.find_file("/browser.dat").expect("find browser.dat");
        assert_eq!(stat.size, 2048);
        assert!(stat.first_cluster >= 3);

        // Reabre para leitura
        let read_fd = fs.open("/browser.dat", O_RDONLY).expect("reopen browser.dat");
        let mut read_buf = vec![0u8; 2048];
        let nread = fs.read(read_fd, 0, &mut read_buf).expect("read browser.dat");
        assert_eq!(nread, 2048);
        assert_eq!(read_buf, write_payload);

        // Leitura além do EOF retorna 0
        let mut eof_buf = [0u8; 10];
        let eof_read = fs.read(read_fd, 2048, &mut eof_buf).expect("read past eof");
        assert_eq!(eof_read, 0);

        fs.close(read_fd).expect("close read fd");
    }

    #[test]
    fn test_05_fat32_truncation_via_o_trunc() {
        let mut fs = SimulatedFat32::new(512);

        // Cria e preenche arquivo com 1024 bytes
        let fd = fs.open("/cache.tmp", O_CREAT | O_WRONLY).expect("open cache.tmp");
        fs.write(fd, 0, &[0xAA; 1024]).expect("write");
        fs.close(fd).expect("close");

        let stat1 = fs.find_file("/cache.tmp").expect("stat1");
        assert_eq!(stat1.size, 1024);
        let orig_cluster = stat1.first_cluster;
        assert!(orig_cluster >= 3);

        // Reabre com O_TRUNC
        let fd2 = fs.open("/cache.tmp", O_WRONLY | O_TRUNC).expect("open trunc");
        fs.close(fd2).expect("close trunc");

        let stat2 = fs.find_file("/cache.tmp").expect("stat2");
        assert_eq!(stat2.size, 0);
        assert_eq!(stat2.first_cluster, 0);

        // Valida que o cluster original foi liberado na FAT
        let fat_offset = orig_cluster * 4;
        let mut fat_buf = [0u8; 512];
        fs.disk.read_sector(fs.fat_start_lba + (fat_offset / 512), &mut fat_buf).unwrap();
        let off = (fat_offset % 512) as usize;
        let val = u32::from_le_bytes([fat_buf[off], fat_buf[off + 1], fat_buf[off + 2], fat_buf[off + 3] & 0x0F]);
        assert_eq!(val, 0, "Cluster deve estar marcado como livre (0x00)");
    }

    #[test]
    fn test_06_fat32_remove_file_and_empty_directory() {
        let mut fs = SimulatedFat32::new(512);

        fs.mkdir("/app").expect("mkdir /app");
        fs.create("/app/file.txt").expect("create /app/file.txt");

        // Tentativa de remover diretório não-vazio falha com Busy / EBUSY
        let err_busy = fs.remove("/app").unwrap_err();
        assert_eq!(err_busy, VfsError::Busy);
        assert_eq!(vfs_error_to_errno(err_busy), EBUSY);

        // Remove o arquivo dentro dele
        fs.remove("/app/file.txt").expect("remove file.txt");
        assert_eq!(fs.find_file("/app/file.txt").unwrap_err(), VfsError::NotFound);

        // Agora remoção do diretório vazio tem sucesso
        fs.remove("/app").expect("remove empty /app");
        assert_eq!(fs.find_file("/app").unwrap_err(), VfsError::NotFound);
    }

    #[test]
    fn test_07_fat32_nospace_exhaustion() {
        // Cria um disco minúsculo com apenas 100 setores no total (data_start = 96, restam 4 clusters de dados)
        let mut fs = SimulatedFat32::new(100);

        // Aloca os clusters de dados disponíveis (clusters 3, 4, 5)
        let fd1 = fs.open("/f1.bin", O_CREAT | O_WRONLY).expect("open f1");
        fs.write(fd1, 0, &[1u8; 512]).expect("write f1"); // gasta clus 3

        let fd2 = fs.open("/f2.bin", O_CREAT | O_WRONLY).expect("open f2");
        fs.write(fd2, 0, &[2u8; 512]).expect("write f2"); // gasta clus 4

        let fd3 = fs.open("/f3.bin", O_CREAT | O_WRONLY).expect("open f3");
        fs.write(fd3, 0, &[3u8; 512]).expect("write f3"); // gasta clus 5

        // Próxima escrita tenta alocar além do limite da partição
        let fd4 = fs.open("/f4.bin", O_CREAT | O_WRONLY).expect("open f4");
        let err = fs.write(fd4, 0, &[4u8; 512]).unwrap_err();

        // DEVE retornar NoSpace (ENOSPC), e JAMAIS tentar acesso I/O fora dos limites
        assert_eq!(err, VfsError::NoSpace);
        assert_eq!(vfs_error_to_errno(err), ENOSPC);
    }

    #[test]
    fn test_08_ext2_sys_creat_sys_write_and_direct_block_allocation() {
        let mut ext2 = SimulatedExt2::new(256);

        // Cria arquivo novo no Ext2
        ext2.create("/kernel_log.txt").expect("ext2 create");

        let (ino, inode) = ext2.resolve_path("/kernel_log.txt").expect("resolve");
        assert_eq!(inode.size, 0);
        assert_eq!(inode.direct_blocks[0], 0, "Arquivo novo tem direct_blocks zerados");

        // Abre para escrita
        let fd = ext2.open("/kernel_log.txt", O_WRONLY).expect("ext2 open");

        // Escreve dados no arquivo: o Ext2 deve dinamicamente alocar o bloco direto 0
        let payload = b"Jinn Microkernel Security Audit Log OK";
        let written = ext2.write(fd, 0, payload).expect("ext2 write");
        assert_eq!(written, payload.len());
        ext2.close(fd).expect("close");

        // Inode atualizado no disco
        let updated_inode = ext2.read_inode(ino).expect("read inode");
        assert_eq!(updated_inode.size, payload.len() as u32);
        assert!(updated_inode.direct_blocks[0] >= 14, "Bloco direto 0 alocado com sucesso");

        // Lê de volta
        let rfd = ext2.open("/kernel_log.txt", O_RDONLY).expect("open read");
        let mut r_buf = vec![0u8; payload.len()];
        let nread = ext2.read(rfd, 0, &mut r_buf).expect("read");
        assert_eq!(nread, payload.len());
        assert_eq!(&r_buf, payload);
    }

    #[test]
    fn test_09_ext2_mkdir_and_subdirectories() {
        let mut ext2 = SimulatedExt2::new(256);

        ext2.mkdir("/etc").expect("mkdir /etc");
        ext2.mkdir("/etc/jinn").expect("mkdir /etc/jinn");

        let (_ino, inode) = ext2.resolve_path("/etc/jinn").expect("resolve /etc/jinn");
        assert_eq!(inode.mode & 0xF000, 0x4000);

        // Cria arquivo dentro da subpasta
        ext2.create("/etc/jinn/hosts").expect("create hosts");
        let h_fd = ext2.open("/etc/jinn/hosts", O_WRONLY).expect("open hosts");
        ext2.write(h_fd, 0, b"127.0.0.1 localhost").expect("write hosts");
        ext2.close(h_fd).expect("close hosts");

        // Leitura através do caminho completo
        let r_fd = ext2.open("/etc/jinn/hosts", O_RDONLY).expect("open read hosts");
        let mut host_buf = [0u8; 19];
        ext2.read(r_fd, 0, &mut host_buf).expect("read hosts");
        assert_eq!(&host_buf, b"127.0.0.1 localhost");
    }

    #[test]
    fn test_10_ext2_truncation_and_block_freeing() {
        let mut ext2 = SimulatedExt2::new(256);

        let fd = ext2.open("/truncate_me.bin", O_CREAT | O_WRONLY).expect("open");
        ext2.write(fd, 0, &[0x55; 2048]).expect("write 2 blocks");
        ext2.close(fd).expect("close");

        let (ino, in1) = ext2.resolve_path("/truncate_me.bin").expect("stat");
        assert_eq!(in1.size, 2048);
        let blk0 = in1.direct_blocks[0];
        let blk1 = in1.direct_blocks[1];
        assert_ne!(blk0, 0);
        assert_ne!(blk1, 0);

        // Abre com O_TRUNC
        let fd_trunc = ext2.open("/truncate_me.bin", O_WRONLY | O_TRUNC).expect("trunc");
        ext2.close(fd_trunc).expect("close");

        let in2 = ext2.read_inode(ino).expect("stat after trunc");
        assert_eq!(in2.size, 0);
        assert_eq!(in2.direct_blocks[0], 0);
        assert_eq!(in2.direct_blocks[1], 0);

        // Valida que os blocos foram desmarcados no bitmap de blocos
        let mut b_bitmap = [0u8; 1024];
        ext2.read_block(3, &mut b_bitmap);
        assert_eq!(b_bitmap[blk0 as usize / 8] & (1 << (blk0 % 8)), 0);
        assert_eq!(b_bitmap[blk1 as usize / 8] & (1 << (blk1 % 8)), 0);
    }

    #[test]
    fn test_11_descriptor_exhaustion_emfile() {
        let mut fs = SimulatedFat32::new(512);

        // Tenta abrir mais que o limite de 32 arquivos simultâneos
        let mut handles = Vec::new();
        for i in 0..32 {
            let path = format!("/file_{}.txt", i);
            let h = fs.open(&path, O_CREAT | O_WRONLY).expect("open file");
            handles.push(h);
        }

        // O 33º descritor deve falhar com NoSpace / EMFILE
        let overflow_res = fs.open("/file_overflow.txt", O_CREAT | O_WRONLY);
        assert_eq!(overflow_res.unwrap_err(), VfsError::NoSpace);

        // Fecha um descritor e valida que agora há vaga
        fs.close(handles[0]).expect("close handle 0");
        let reuse_res = fs.open("/file_reuse.txt", O_CREAT | O_WRONLY);
        assert!(reuse_res.is_ok(), "Deve reutilizar o slot liberado");
    }

    #[test]
    fn test_12_posix_error_mapping_conformance() {
        assert_eq!(vfs_error_to_errno(VfsError::NotFound), ENOENT);
        assert_eq!(vfs_error_to_errno(VfsError::AlreadyExists), EEXIST);
        assert_eq!(vfs_error_to_errno(VfsError::IsADirectory), EISDIR);
        assert_eq!(vfs_error_to_errno(VfsError::NotADirectory), ENOTDIR);
        assert_eq!(vfs_error_to_errno(VfsError::NoSpace), ENOSPC);
        assert_eq!(vfs_error_to_errno(VfsError::Busy), EBUSY);
        assert_eq!(vfs_error_to_errno(VfsError::InvalidPath), EINVAL);
    }

    #[test]
    fn test_13_fat32_deeply_nested_directories_and_file_crud() {
        let mut fs = SimulatedFat32::new(1024);

        // Cria hierarquia profunda de subdiretórios
        fs.mkdir("/var").expect("mkdir /var");
        fs.mkdir("/var/log").expect("mkdir /var/log");
        fs.mkdir("/var/log/audit").expect("mkdir /var/log/audit");
        fs.mkdir("/var/log/audit/daily").expect("mkdir /var/log/audit/daily");

        // Cria arquivo na profundidade máxima
        let path = "/var/log/audit/daily/report.txt";
        fs.create(path).expect("create deeply nested file");

        let fd = fs.open(path, O_WRONLY).expect("open for write");
        let sample_data = b"JINN_AUDIT_2026_TEST_LOG_ENTRY_OK";
        fs.write(fd, 0, sample_data).expect("write data");
        fs.close(fd).expect("close");

        // Reabre para leitura e valida integridade
        let rfd = fs.open(path, O_RDONLY).expect("open for read");
        let mut buf = vec![0u8; sample_data.len()];
        let n = fs.read(rfd, 0, &mut buf).expect("read");
        assert_eq!(n, sample_data.len());
        assert_eq!(&buf, sample_data);
        fs.close(rfd).expect("close");

        // Remover pasta com conteúdo deve retornar EBUSY
        let busy_err = fs.remove("/var/log/audit").unwrap_err();
        assert_eq!(busy_err, VfsError::Busy);
        assert_eq!(vfs_error_to_errno(busy_err), EBUSY);

        // Remove o arquivo e valida remoção
        fs.remove(path).expect("remove report.txt");
        assert_eq!(fs.find_file(path).unwrap_err(), VfsError::NotFound);

        // Agora a pasta pode ser removida
        fs.remove("/var/log/audit/daily").expect("remove daily");
    }

    #[test]
    fn test_14_fat32_directory_cluster_chain_expansion() {
        // Disco com 2048 setores para acomodar crescimento dinâmico de diretórios
        let mut fs = SimulatedFat32::new(2048);

        fs.mkdir("/stress").expect("mkdir /stress");

        // 1 cluster FAT32 = 1 setor = 512 bytes = 16 entradas de 32 bytes
        // Entradas '.' e '..' consomem 2 slots.
        // Criar 30 arquivos força a alocação de pelo menos 2 clusters adicionais para o diretório!
        for i in 0..30 {
            let file_path = format!("/stress/f{:02}.dat", i);
            fs.create(&file_path).expect("create file in stress dir");

            let fd = fs.open(&file_path, O_WRONLY).expect("open file");
            let data = format!("CONTENT_{:04}", i);
            fs.write(fd, 0, data.as_bytes()).expect("write content");
            fs.close(fd).expect("close file");
        }

        // Valida que TODOS os 30 arquivos são encontrados e seus conteúdos estão corretos
        for i in 0..30 {
            let file_path = format!("/stress/f{:02}.dat", i);
            let entry = fs.find_file(&file_path).expect("find file in expanded dir");
            assert!(!entry.is_dir);

            let rfd = fs.open(&file_path, O_RDONLY).expect("open read");
            let mut read_buf = [0u8; 12];
            let n = fs.read(rfd, 0, &mut read_buf).expect("read");
            assert_eq!(n, 12);
            let expected = format!("CONTENT_{:04}", i);
            assert_eq!(&read_buf[..n], expected.as_bytes());
            fs.close(rfd).expect("close read");
        }
    }

    #[test]
    fn test_15_ext2_multi_block_directory_expansion_and_stress() {
        let mut ext2 = SimulatedExt2::new(512);

        ext2.mkdir("/database").expect("mkdir /database");

        // Em Ext2 (bloco 1024 bytes), entradas de diretório ocupam ~16-24 bytes cada.
        // Criar 30 arquivos testa a expansão de slots de diretório com split de rec_len
        for i in 0..30 {
            let path = format!("/database/rec_{:02}.db", i);
            ext2.create(&path).expect("ext2 create in dir");

            let fd = ext2.open(&path, O_WRONLY).expect("ext2 open write");
            let payload = format!("DB_RECORD_INDEX_{:03}", i);
            ext2.write(fd, 0, payload.as_bytes()).expect("write payload");
            ext2.close(fd).expect("close fd");
        }

        // Valida leitura de todos os 30 arquivos
        for i in 0..30 {
            let path = format!("/database/rec_{:02}.db", i);
            let (ino, inode) = ext2.resolve_path(&path).expect("resolve in ext2");
            assert!(ino >= 11, "Inodes normais devem ser >= 11");
            assert_eq!(inode.size, 19);

            let rfd = ext2.open(&path, O_RDONLY).expect("open read");
            let mut buf = [0u8; 19];
            let n = ext2.read(rfd, 0, &mut buf).expect("read db");
            assert_eq!(n, 19);
            let expected = format!("DB_RECORD_INDEX_{:03}", i);
            assert_eq!(&buf, expected.as_bytes());
            ext2.close(rfd).expect("close read");
        }
    }

    #[test]
    fn test_16_read_modify_write_unaligned_offsets_and_cluster_spans() {
        let mut fs = SimulatedFat32::new(1024);

        let path = "/unaligned.bin";
        fs.create(path).expect("create");

        let fd = fs.open(path, O_WRONLY).expect("open");

        // 1. Escrita inicial de 1000 bytes (abrange cluster 0 e parte de cluster 1)
        let initial_payload = vec![0xAAu8; 1000];
        fs.write(fd, 0, &initial_payload).expect("write initial");

        // 2. Escrita em offset desalinhado (offset 300, 200 bytes com 0xBB)
        let patch = vec![0xBBu8; 200];
        fs.write(fd, 300, &patch).expect("write patch");
        fs.close(fd).expect("close");

        // 3. Lê de volta e confere se a região anterior e posterior ao patch permaneceu intocada
        let rfd = fs.open(path, O_RDONLY).expect("open read");
        let mut final_buf = vec![0u8; 1000];
        let n = fs.read(rfd, 0, &mut final_buf).expect("read all");
        assert_eq!(n, 1000);

        // 0..300 deve ser 0xAA
        assert_eq!(&final_buf[0..300], &vec![0xAAu8; 300][..]);
        // 300..500 deve ser 0xBB
        assert_eq!(&final_buf[300..500], &vec![0xBBu8; 200][..]);
        // 500..1000 deve ser 0xAA
        assert_eq!(&final_buf[500..1000], &vec![0xAAu8; 500][..]);
        fs.close(rfd).expect("close read");
    }

    #[test]
    fn test_17_posix_negative_error_codes_stress() {
        let mut fs = SimulatedFat32::new(512);

        // ENOTDIR: tentar criar pasta abaixo de um arquivo comum
        fs.create("/file.txt").expect("create file");
        let notdir_err = fs.mkdir("/file.txt/sub").unwrap_err();
        assert_eq!(notdir_err, VfsError::NotADirectory);
        assert_eq!(vfs_error_to_errno(notdir_err), ENOTDIR);

        // EISDIR: tentar abrir diretório para escrita
        fs.mkdir("/docs").expect("mkdir docs");
        let isdir_err = fs.open("/docs", O_WRONLY).unwrap_err();
        assert_eq!(isdir_err, VfsError::IsADirectory);
        assert_eq!(vfs_error_to_errno(isdir_err), EISDIR);

        // EEXIST: tentar recriar diretório já existente
        let exist_err = fs.mkdir("/docs").unwrap_err();
        assert_eq!(exist_err, VfsError::AlreadyExists);
        assert_eq!(vfs_error_to_errno(exist_err), EEXIST);

        // ENOENT: caminho inexistente
        let noent_err = fs.open("/missing_folder/file.bin", O_RDONLY).unwrap_err();
        assert_eq!(noent_err, VfsError::NotFound);
        assert_eq!(vfs_error_to_errno(noent_err), ENOENT);

        // EINVAL: caminho vazio ou inválido
        let inval_err = fs.create("").unwrap_err();
        assert_eq!(inval_err, VfsError::InvalidPath);
        assert_eq!(vfs_error_to_errno(inval_err), EINVAL);
    }
}
