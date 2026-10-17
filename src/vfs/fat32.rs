/// Jinn OS — Driver de Sistema de Arquivos FAT32
///
/// Implementa a leitura de volumes formatados em FAT32 diretamente a partir
/// de discos IDE/ATA ou partições MBR.
///
/// Estrutura do Volume FAT32:
///   [ Boot Sector / BPB ] -> [ Setores Reservados ] -> [ FAT 1 ] -> [ FAT 2 ] -> [ Área de Dados (Clusters) ]

use crate::vfs::{self, FileSystem, NodeType, Stat, VfsError, VfsResult};
use crate::drivers::ide;

pub const FAT32_EOF: u32 = 0x0FFF_FFF8;
const MAX_OPEN_FILES: usize = 16;

// ── Estruturas de Dados do FAT32 ──────────────────────────────────────────────

#[derive(Clone, Copy)]
pub struct FatDirEntry {
    pub name: [u8; 13], // Nome formatado 8.3 + terminador nulo
    pub name_len: usize,
    pub is_dir: bool,
    pub first_cluster: u32,
    pub size: u32,
}

impl FatDirEntry {
    pub const fn empty() -> Self {
        Self {
            name: [0; 13],
            name_len: 0,
            is_dir: false,
            first_cluster: 0,
            size: 0,
        }
    }
}

struct OpenFile {
    entry: FatDirEntry,
    is_active: bool,
}

pub struct Fat32Fs {
    drive: u8,
    partition_lba: u32,
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    fat_start_lba: u32,
    data_start_lba: u32,
    root_cluster: u32,
    is_mounted: bool,
    open_files: [OpenFile; MAX_OPEN_FILES],
}

impl Fat32Fs {
    pub const fn new() -> Self {
        const EMPTY_FILE: OpenFile = OpenFile {
            entry: FatDirEntry::empty(),
            is_active: false,
        };
        Self {
            drive: 0,
            partition_lba: 0,
            bytes_per_sector: 512,
            sectors_per_cluster: 1,
            fat_start_lba: 0,
            data_start_lba: 0,
            root_cluster: 2,
            is_mounted: false,
            open_files: [EMPTY_FILE; MAX_OPEN_FILES],
        }
    }

    /// Inicializa e monta o filesystem lendo o BPB (Boot Sector) da partição.
    pub fn init_from_partition(&mut self, drive: u8, partition_lba: u32) -> Result<(), &'static str> {
        let mut bpb = [0u8; 512];
        ide::read_sector(drive, partition_lba, &mut bpb)?;

        // Valida assinatura de boot 0x55AA
        if bpb[510] != 0x55 || bpb[511] != 0xAA {
            return Err("Assinatura 0x55AA invalida no setor de boot FAT32");
        }

        let bytes_per_sector = (bpb[11] as u16) | ((bpb[12] as u16) << 8);
        if bytes_per_sector != 512 {
            return Err("Apenas setores de 512 bytes sao suportados atualmente");
        }

        let sectors_per_cluster = bpb[13];
        if sectors_per_cluster == 0 {
            return Err("Setores por cluster invalido (0)");
        }

        let reserved_sectors = (bpb[14] as u16) | ((bpb[15] as u16) << 8);
        let num_fats = bpb[16];
        if num_fats == 0 {
            return Err("Numero de tabelas FAT invalido (0)");
        }

        let sectors_per_fat32 = (bpb[36] as u32)
            | ((bpb[37] as u32) << 8)
            | ((bpb[38] as u32) << 16)
            | ((bpb[39] as u32) << 24);

        if sectors_per_fat32 == 0 {
            return Err("Setores por FAT32 invalido (0) - volume pode ser FAT12/16");
        }

        let root_cluster = (bpb[44] as u32)
            | ((bpb[45] as u32) << 8)
            | ((bpb[46] as u32) << 16)
            | ((bpb[47] as u32) << 24);

        let fat_start = partition_lba + reserved_sectors as u32;
        let data_start = fat_start + (num_fats as u32 * sectors_per_fat32);

        self.drive = drive;
        self.partition_lba = partition_lba;
        self.bytes_per_sector = bytes_per_sector;
        self.sectors_per_cluster = sectors_per_cluster;
        self.fat_start_lba = fat_start;
        self.data_start_lba = data_start;
        self.root_cluster = root_cluster;
        self.is_mounted = true;

        Ok(())
    }

    /// Converte um número de cluster para seu endereço físico LBA inicial.
    #[inline]
    fn cluster_to_lba(&self, cluster: u32) -> u32 {
        if cluster < 2 {
            self.data_start_lba
        } else {
            self.data_start_lba + (cluster - 2) * (self.sectors_per_cluster as u32)
        }
    }

    /// Lê a tabela FAT para encontrar o próximo cluster na cadeia.
    /// Retorna `Ok(None)` se for o fim do arquivo (EOF).
    pub fn get_next_cluster(&self, cluster: u32) -> Result<Option<u32>, &'static str> {
        let fat_offset = cluster * 4;
        let fat_sector = self.fat_start_lba + (fat_offset / 512);
        let entry_offset = (fat_offset % 512) as usize;

        let mut sector_buf = [0u8; 512];
        ide::read_sector(self.drive, fat_sector, &mut sector_buf)?;

        let next = (sector_buf[entry_offset] as u32)
            | ((sector_buf[entry_offset + 1] as u32) << 8)
            | ((sector_buf[entry_offset + 2] as u32) << 16)
            | (((sector_buf[entry_offset + 3] & 0x0F) as u32) << 24);

        if next >= FAT32_EOF {
            Ok(None)
        } else {
            Ok(Some(next))
        }
    }

    /// Itera sobre os arquivos e pastas de um diretório cujo cluster inicial é `dir_cluster`.
    pub fn for_each_entry<F>(&self, dir_cluster: u32, mut cb: F) -> Result<(), &'static str>
    where
        F: FnMut(FatDirEntry) -> bool, // Retorna true para continuar, false para parar
    {
        let mut cur_cluster = dir_cluster;
        let mut sector_buf = [0u8; 512];

        while cur_cluster >= 2 && cur_cluster < FAT32_EOF {
            let start_lba = self.cluster_to_lba(cur_cluster);

            for sec in 0..self.sectors_per_cluster {
                ide::read_sector(self.drive, start_lba + sec as u32, &mut sector_buf)?;

                for entry_idx in 0..(512 / 32) {
                    let offset = entry_idx * 32;
                    let first_byte = sector_buf[offset];

                    if first_byte == 0x00 {
                        // Fim do diretório
                        return Ok(());
                    }
                    if first_byte == 0xE5 {
                        // Entrada excluída
                        continue;
                    }

                    let attr = sector_buf[offset + 11];
                    // Pula entradas Long File Name (0x0F) e Volume ID (0x08)
                    if attr == 0x0F || (attr & 0x08) != 0 {
                        continue;
                    }

                    // Extrai nome 8.3 formatado
                    let mut name = [0u8; 13];
                    let mut len = 0;

                    // Nome base (até 8 caracteres)
                    for i in 0..8 {
                        let c = sector_buf[offset + i];
                        if c != b' ' {
                            name[len] = c.to_ascii_lowercase();
                            len += 1;
                        }
                    }

                    let is_dir = (attr & 0x10) != 0;

                    // Extensão (até 3 caracteres, se não for diretório)
                    let ext1 = sector_buf[offset + 8];
                    let ext2 = sector_buf[offset + 9];
                    let ext3 = sector_buf[offset + 10];
                    if !is_dir && (ext1 != b' ' || ext2 != b' ' || ext3 != b' ') {
                        name[len] = b'.';
                        len += 1;
                        for &c in &[ext1, ext2, ext3] {
                            if c != b' ' {
                                name[len] = c.to_ascii_lowercase();
                                len += 1;
                            }
                        }
                    }

                    let fst_clus_hi = (sector_buf[offset + 20] as u32) | ((sector_buf[offset + 21] as u32) << 8);
                    let fst_clus_lo = (sector_buf[offset + 26] as u32) | ((sector_buf[offset + 27] as u32) << 8);
                    let first_cluster = (fst_clus_hi << 16) | fst_clus_lo;

                    let size = (sector_buf[offset + 28] as u32)
                        | ((sector_buf[offset + 29] as u32) << 8)
                        | ((sector_buf[offset + 30] as u32) << 16)
                        | ((sector_buf[offset + 31] as u32) << 24);

                    let entry = FatDirEntry {
                        name,
                        name_len: len,
                        is_dir,
                        first_cluster,
                        size,
                    };

                    if !cb(entry) {
                        return Ok(());
                    }
                }
            }

            match self.get_next_cluster(cur_cluster)? {
                Some(next) => cur_cluster = next,
                None => break,
            }
        }

        Ok(())
    }

    /// Procura por um arquivo pelo caminho (ex: `"readme.txt"` ou `"bin/app.elf"`).
    pub fn find_file(&self, path: &str) -> VfsResult<FatDirEntry> {
        let clean_path = path.trim_matches('/');
        if clean_path.is_empty() {
            // Diretório raiz
            return Ok(FatDirEntry {
                name: *b"/            ",
                name_len: 1,
                is_dir: true,
                first_cluster: self.root_cluster,
                size: 0,
            });
        }

        let mut current_cluster = self.root_cluster;
        let segments = clean_path.split('/');

        let mut found_entry: Option<FatDirEntry> = None;

        for seg in segments {
            let mut matched: Option<FatDirEntry> = None;

            let _ = self.for_each_entry(current_cluster, |entry| {
                if let Ok(entry_name) = core::str::from_utf8(&entry.name[..entry.name_len]) {
                    if entry_name.eq_ignore_ascii_case(seg) {
                        matched = Some(entry);
                        return false; // Encontrou, interrompe iteração
                    }
                }
                true
            });

            match matched {
                Some(entry) => {
                    current_cluster = entry.first_cluster;
                    found_entry = Some(entry);
                }
                None => return Err(VfsError::NotFound),
            }
        }

        found_entry.ok_or(VfsError::NotFound)
    }
}

// ── Implementação do FileSystem Trait para VFS ────────────────────────────────

impl FileSystem for Fat32Fs {
    fn name(&self) -> &'static str {
        "fat32"
    }

    fn lookup(&self, path: &str) -> VfsResult<Stat> {
        if !self.is_mounted {
            return Err(VfsError::NotSupported);
        }

        let entry = self.find_file(path)?;
        Ok(Stat {
            node_type: if entry.is_dir { NodeType::Directory } else { NodeType::File },
            size: entry.size as u64,
            inode: entry.first_cluster as u64,
        })
    }

    fn open(&self, path: &str, _flags: u32) -> VfsResult<u64> {
        if !self.is_mounted {
            return Err(VfsError::NotSupported);
        }

        let entry = self.find_file(path)?;
        if entry.is_dir {
            return Err(VfsError::IsADirectory);
        }

        // Armazena na tabela de arquivos abertos (handle index)
        // Como o método recebe &self, usamos mutabilidade controlada por SpinLock global
        unsafe {
            let fs_ptr = core::ptr::addr_of_mut!(FAT32_GLOBAL);
            let fs = &mut (*fs_ptr);
            for (idx, slot) in fs.open_files.iter_mut().enumerate() {
                if !slot.is_active {
                    slot.entry = entry;
                    slot.is_active = true;
                    return Ok(idx as u64);
                }
            }
        }

        Err(VfsError::NoSpace)
    }

    fn close(&self, handle: u64) -> VfsResult<()> {
        let idx = handle as usize;
        if idx >= MAX_OPEN_FILES {
            return Err(VfsError::InvalidPath);
        }

        unsafe {
            let fs_ptr = core::ptr::addr_of_mut!(FAT32_GLOBAL);
            (*fs_ptr).open_files[idx].is_active = false;
        }
        Ok(())
    }

    fn read(&self, handle: u64, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let idx = handle as usize;
        if idx >= MAX_OPEN_FILES {
            return Err(VfsError::InvalidPath);
        }

        let entry = unsafe {
            let fs_ptr = core::ptr::addr_of!(FAT32_GLOBAL);
            let slot = &(*fs_ptr).open_files[idx];
            if !slot.is_active {
                return Err(VfsError::NotFound);
            }
            slot.entry
        };

        if offset >= entry.size as u64 {
            return Ok(0); // EOF
        }

        let bytes_to_read = ((entry.size as u64 - offset) as usize).min(buf.len());
        let cluster_bytes = (self.sectors_per_cluster as usize) * 512;

        let mut cluster_skip = (offset as usize) / cluster_bytes;
        let mut cluster_offset = (offset as usize) % cluster_bytes;

        let mut current_cluster = entry.first_cluster;
        while cluster_skip > 0 {
            match self.get_next_cluster(current_cluster) {
                Ok(Some(next)) => current_cluster = next,
                _ => return Ok(0),
            }
            cluster_skip -= 1;
        }

        let mut bytes_read = 0;
        let mut sector_buf = [0u8; 512];

        while bytes_read < bytes_to_read && current_cluster >= 2 && current_cluster < FAT32_EOF {
            let start_lba = self.cluster_to_lba(current_cluster);

            let start_sec = cluster_offset / 512;
            let mut sec_offset = cluster_offset % 512;

            for sec in start_sec..(self.sectors_per_cluster as usize) {
                if ide::read_sector(self.drive, start_lba + sec as u32, &mut sector_buf).is_err() {
                    return Err(VfsError::IoError);
                }

                let available_in_sec = 512 - sec_offset;
                let needed = bytes_to_read - bytes_read;
                let copy_count = available_in_sec.min(needed);

                buf[bytes_read..bytes_read + copy_count]
                    .copy_from_slice(&sector_buf[sec_offset..sec_offset + copy_count]);

                bytes_read += copy_count;
                sec_offset = 0;

                if bytes_read >= bytes_to_read {
                    break;
                }
            }

            cluster_offset = 0;
            match self.get_next_cluster(current_cluster) {
                Ok(Some(next)) => current_cluster = next,
                _ => break,
            }
        }

        Ok(bytes_read)
    }

    fn write(&self, _handle: u64, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::NotSupported)
    }

    fn create(&self, _path: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn remove(&self, _path: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn readdir(&self, path: &str, cb: &mut dyn FnMut(&str, NodeType)) {
        if !self.is_mounted {
            return;
        }

        let target_cluster = match self.find_file(path) {
            Ok(entry) if entry.is_dir => entry.first_cluster,
            _ => return,
        };

        let _ = self.for_each_entry(target_cluster, |entry| {
            if let Ok(name_str) = core::str::from_utf8(&entry.name[..entry.name_len]) {
                let node_type = if entry.is_dir { NodeType::Directory } else { NodeType::File };
                cb(name_str, node_type);
            }
            true
        });
    }
}

// ── Instância Global e Ponto de Montagem ───────────────────────────────────────

pub static mut FAT32_GLOBAL: Fat32Fs = Fat32Fs::new();

/// Monta a partição FAT32 do disco especificado no ponto de montagem do VFS (ex: `"/disk"`).
pub fn mount_fat32_partition(prefix: &'static str, drive: u8, partition_lba: u32) -> Result<(), &'static str> {
    unsafe {
        let fs = &mut *core::ptr::addr_of_mut!(FAT32_GLOBAL);
        fs.init_from_partition(drive, partition_lba)?;
        vfs::mount(prefix, fs).map_err(|_| "Falha ao registrar ponto de montagem no VFS")?;
    }
    Ok(())
}
