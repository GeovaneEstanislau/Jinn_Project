/// Jinn OS — Driver de Sistema de Arquivos Ext2 (Protótipo Read-Only)
///
/// Implementa a leitura de volumes formatados em Ext2 (Second Extended Filesystem),
/// fornecendo compatibilidade nativa com partições Linux/POSIX no Jinn OS.
///
/// Arquitetura do Volume Ext2:
///   [ Boot Record (1024 B) ] -> [ Superblock (1024 B) ] -> [ Block Group Descriptors ]
///   -> [ Block Bitmaps / Inode Bitmaps ] -> [ Inode Tables ] -> [ Data Blocks ]

use crate::vfs::{self, FileSystem, NodeType, Stat, VfsError, VfsResult};
use crate::drivers::ide;

pub const EXT2_SUPER_MAGIC: u16 = 0xEF53;
pub const EXT2_ROOT_INO: u32    = 2;
pub const EXT2_S_IFDIR: u16     = 0x4000;
pub const EXT2_S_IFREG: u16     = 0x8000;
pub const EXT2_FT_REG_FILE: u8  = 1;
pub const EXT2_FT_DIR: u8       = 2;

const MAX_OPEN_FILES: usize = 8;

#[derive(Clone, Copy)]
pub struct Ext2Inode {
    pub mode: u16,
    pub uid: u16,
    pub size: u32,
    pub atime: u32,
    pub ctime: u32,
    pub mtime: u32,
    pub dtime: u32,
    pub gid: u16,
    pub links_count: u16,
    pub blocks: u32,
    pub flags: u32,
    pub direct_blocks: [u32; 12],
    pub indirect_block: u32,
}

impl Ext2Inode {
    pub const fn empty() -> Self {
        Self {
            mode: 0,
            uid: 0,
            size: 0,
            atime: 0,
            ctime: 0,
            mtime: 0,
            dtime: 0,
            gid: 0,
            links_count: 0,
            blocks: 0,
            flags: 0,
            direct_blocks: [0; 12],
            indirect_block: 0,
        }
    }

    pub fn is_dir(&self) -> bool {
        (self.mode & 0xF000) == EXT2_S_IFDIR
    }

    pub fn is_file(&self) -> bool {
        (self.mode & 0xF000) == EXT2_S_IFREG
    }
}

#[derive(Clone, Copy)]
struct OpenExt2File {
    inode_num: u32,
    inode: Ext2Inode,
    is_active: bool,
}

pub struct Ext2Fs {
    pub drive: u8,
    pub partition_lba: u32,
    pub block_size: usize,
    pub inodes_count: u32,
    pub blocks_count: u32,
    pub first_data_block: u32,
    pub blocks_per_group: u32,
    pub inodes_per_group: u32,
    pub inode_size: u16,
    pub is_mounted: bool,
    open_files: [OpenExt2File; MAX_OPEN_FILES],
}

impl Ext2Fs {
    pub const fn new() -> Self {
        const EMPTY_OPEN: OpenExt2File = OpenExt2File {
            inode_num: 0,
            inode: Ext2Inode::empty(),
            is_active: false,
        };
        Self {
            drive: 0,
            partition_lba: 0,
            block_size: 1024,
            inodes_count: 0,
            blocks_count: 0,
            first_data_block: 1,
            blocks_per_group: 0,
            inodes_per_group: 0,
            inode_size: 128,
            is_mounted: false,
            open_files: [EMPTY_OPEN; MAX_OPEN_FILES],
        }
    }

    /// Inicializa e valida o Superblock Ext2 na partição especificada
    pub fn init_from_partition(&mut self, drive: u8, partition_lba: u32) -> Result<(), &'static str> {
        // O Superblock Ext2 fica localizado no byte 1024 (LBA offset 2 de 512 bytes)
        let mut sb_buf = [0u8; 512];
        ide::read_sector(drive, partition_lba + 2, &mut sb_buf)?;

        // Valida Magic Number Ext2: 0xEF53 (offset 56 no superblock)
        let magic = u16::from_le_bytes([sb_buf[56], sb_buf[57]]);
        if magic != EXT2_SUPER_MAGIC {
            return Err("Assinatura Ext2 invalida (magic != 0xEF53)");
        }

        self.drive = drive;
        self.partition_lba = partition_lba;
        self.inodes_count = u32::from_le_bytes([sb_buf[0], sb_buf[1], sb_buf[2], sb_buf[3]]);
        self.blocks_count = u32::from_le_bytes([sb_buf[4], sb_buf[5], sb_buf[6], sb_buf[7]]);
        self.first_data_block = u32::from_le_bytes([sb_buf[20], sb_buf[21], sb_buf[22], sb_buf[23]]);

        let log_block_size = u32::from_le_bytes([sb_buf[24], sb_buf[25], sb_buf[26], sb_buf[27]]);
        self.block_size = 1024usize << log_block_size;
        self.blocks_per_group = u32::from_le_bytes([sb_buf[32], sb_buf[33], sb_buf[34], sb_buf[35]]);
        self.inodes_per_group = u32::from_le_bytes([sb_buf[40], sb_buf[41], sb_buf[42], sb_buf[43]]);

        // Inode size (para revisões >= 1 fica em offset 88; default 128)
        let ino_sz = u16::from_le_bytes([sb_buf[88], sb_buf[89]]);
        self.inode_size = if ino_sz >= 128 { ino_sz } else { 128 };

        self.is_mounted = true;
        Ok(())
    }

    /// Lê um bloco de dados do sistema de arquivos para o buffer especificado
    fn read_block(&self, block_num: u32, buf: &mut [u8]) -> Result<(), &'static str> {
        let sectors_per_block = self.block_size / 512;
        let start_lba = self.partition_lba + (block_num * sectors_per_block as u32);

        for s in 0..sectors_per_block {
            let offset = s * 512;
            let mut sec_buf = [0u8; 512];
            ide::read_sector(self.drive, start_lba + s as u32, &mut sec_buf)?;
            let copy_len = (buf.len().saturating_sub(offset)).min(512);
            buf[offset..offset + copy_len].copy_from_slice(&sec_buf[..copy_len]);
        }
        Ok(())
    }

    /// Obtém a tabela de descritores de grupo de blocos e localiza a Inode Table
    fn get_inode_table_block(&self, group: u32) -> Result<u32, &'static str> {
        // Se block_size == 1024, superblock está no bloco 1 e descritores começam no bloco 2.
        // Se block_size > 1024, superblock está no bloco 0 e descritores no bloco 1.
        let desc_start_block = if self.block_size == 1024 { 2u32 } else { 1u32 };
        let descs_per_block = (self.block_size / 32) as u32;
        let desc_block = desc_start_block + (group / descs_per_block);
        let desc_index = (group % descs_per_block) as usize;

        let mut block_buf = [0u8; 1024];
        self.read_block(desc_block, &mut block_buf)?;

        let entry_offset = desc_index * 32;
        // Inode table LBA fica nos bytes 8..12 do descritor de grupo
        let ino_table = u32::from_le_bytes([
            block_buf[entry_offset + 8],
            block_buf[entry_offset + 9],
            block_buf[entry_offset + 10],
            block_buf[entry_offset + 11],
        ]);
        Ok(ino_table)
    }

    /// Lê a estrutura do Inode a partir de seu número (indexação 1-based, root = 2)
    pub fn read_inode(&self, inode_num: u32) -> Result<Ext2Inode, &'static str> {
        if inode_num == 0 || inode_num > self.inodes_count {
            return Err("Numero de Inode Ext2 fora dos limites");
        }

        let group = (inode_num - 1) / self.inodes_per_group;
        let index = (inode_num - 1) % self.inodes_per_group;
        let inode_table_block = self.get_inode_table_block(group)?;

        let byte_offset = index * self.inode_size as u32;
        let block_in_table = inode_table_block + (byte_offset / self.block_size as u32);
        let offset_within_block = (byte_offset % self.block_size as u32) as usize;

        let mut block_buf = [0u8; 1024];
        self.read_block(block_in_table, &mut block_buf)?;

        let b = &block_buf[offset_within_block..offset_within_block + self.inode_size as usize];
        let mode = u16::from_le_bytes([b[0], b[1]]);
        let uid = u16::from_le_bytes([b[2], b[3]]);
        let size = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
        let atime = u32::from_le_bytes([b[8], b[9], b[10], b[11]]);
        let ctime = u32::from_le_bytes([b[12], b[13], b[14], b[15]]);
        let mtime = u32::from_le_bytes([b[16], b[17], b[18], b[19]]);
        let dtime = u32::from_le_bytes([b[20], b[21], b[22], b[23]]);
        let gid = u16::from_le_bytes([b[24], b[25]]);
        let links_count = u16::from_le_bytes([b[26], b[27]]);
        let blocks = u32::from_le_bytes([b[28], b[29], b[30], b[31]]);
        let flags = u32::from_le_bytes([b[32], b[33], b[34], b[35]]);

        let mut direct_blocks = [0u32; 12];
        for i in 0..12 {
            let off = 40 + (i * 4);
            direct_blocks[i] = u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]]);
        }

        let indirect_block = u32::from_le_bytes([b[88], b[89], b[90], b[91]]);

        Ok(Ext2Inode {
            mode,
            uid,
            size,
            atime,
            ctime,
            mtime,
            dtime,
            gid,
            links_count,
            blocks,
            flags,
            direct_blocks,
            indirect_block,
        })
    }

    /// Itera sobre as entradas de diretório contidas no Inode especificado
    pub fn for_each_dir_entry<F>(&self, dir_inode: &Ext2Inode, mut callback: F) -> Result<(), &'static str>
    where
        F: FnMut(u32, u8, &str) -> bool,
    {
        if !dir_inode.is_dir() {
            return Err("O Inode alvo nao e um diretorio Ext2");
        }

        let mut block_buf = [0u8; 1024];

        for &block_num in dir_inode.direct_blocks.iter() {
            if block_num == 0 { break; }
            self.read_block(block_num, &mut block_buf)?;

            let mut offset = 0usize;
            while offset + 8 <= self.block_size {
                let ino = u32::from_le_bytes([
                    block_buf[offset],
                    block_buf[offset + 1],
                    block_buf[offset + 2],
                    block_buf[offset + 3],
                ]);
                let rec_len = u16::from_le_bytes([block_buf[offset + 4], block_buf[offset + 5]]) as usize;
                let name_len = block_buf[offset + 6] as usize;
                let file_type = block_buf[offset + 7];

                if rec_len == 0 { break; }

                if ino != 0 && offset + 8 + name_len <= self.block_size {
                    if let Ok(name_str) = core::str::from_utf8(&block_buf[offset + 8..offset + 8 + name_len]) {
                        if !callback(ino, file_type, name_str) {
                            return Ok(());
                        }
                    }
                }

                offset += rec_len;
            }
        }
        Ok(())
    }

    /// Resolve um caminho de arquivo relativo à raiz do Ext2 para seu Inode correspondente
    pub fn resolve_path(&self, path: &str) -> Result<(u32, Ext2Inode), VfsError> {
        let trimmed = path.trim_matches('/');
        if trimmed.is_empty() {
            let root_inode = self.read_inode(EXT2_ROOT_INO).map_err(|_| VfsError::IoError)?;
            return Ok((EXT2_ROOT_INO, root_inode));
        }

        let mut current_ino = EXT2_ROOT_INO;
        let mut current_inode = self.read_inode(current_ino).map_err(|_| VfsError::IoError)?;

        for component in trimmed.split('/') {
            if component.is_empty() || component == "." { continue; }

            let mut found_ino = None;
            let _ = self.for_each_dir_entry(&current_inode, |ino, _, name| {
                if name.eq_ignore_ascii_case(component) {
                    found_ino = Some(ino);
                    return false;
                }
                true
            });

            match found_ino {
                Some(next_ino) => {
                    current_ino = next_ino;
                    current_inode = self.read_inode(current_ino).map_err(|_| VfsError::IoError)?;
                }
                None => return Err(VfsError::NotFound),
            }
        }

        Ok((current_ino, current_inode))
    }
}

// ── Implementação da Trait FileSystem para o VFS Unificado do Jinn OS ──────────

impl FileSystem for Ext2Fs {
    fn name(&self) -> &'static str {
        "ext2"
    }

    fn lookup(&self, path: &str) -> VfsResult<Stat> {
        if !self.is_mounted { return Err(VfsError::IoError); }
        let (ino, inode) = self.resolve_path(path)?;
        let node_type = if inode.is_dir() {
            NodeType::Directory
        } else {
            NodeType::File
        };

        Ok(Stat {
            node_type,
            size: inode.size as u64,
            inode: ino as u64,
        })
    }

    fn open(&self, path: &str, _flags: u32) -> VfsResult<u64> {
        if !self.is_mounted { return Err(VfsError::IoError); }
        let (ino, inode) = self.resolve_path(path)?;

        unsafe {
            let fs_ptr = core::ptr::addr_of_mut!(EXT2_GLOBAL);
            let fs = &mut (*fs_ptr);
            for (idx, slot) in fs.open_files.iter_mut().enumerate() {
                if !slot.is_active {
                    slot.inode_num = ino;
                    slot.inode = inode;
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
            return Err(VfsError::NotFound);
        }
        unsafe {
            let fs_ptr = core::ptr::addr_of_mut!(EXT2_GLOBAL);
            (*fs_ptr).open_files[idx].is_active = false;
        }
        Ok(())
    }

    fn read(&self, handle: u64, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let idx = handle as usize;
        if idx >= MAX_OPEN_FILES || !self.open_files[idx].is_active {
            return Err(VfsError::NotFound);
        }

        let inode = &self.open_files[idx].inode;
        if offset >= inode.size as u64 {
            return Ok(0);
        }

        let bytes_to_read = buf.len().min((inode.size as u64 - offset) as usize);
        let block_sz = self.block_size;
        let start_block_idx = (offset as usize) / block_sz;
        let offset_in_start = (offset as usize) % block_sz;

        let mut bytes_read = 0usize;
        let mut block_buf = [0u8; 1024];

        for blk_idx in start_block_idx..12 {
            if bytes_read >= bytes_to_read { break; }
            let blk_num = inode.direct_blocks[blk_idx];
            if blk_num == 0 { break; }

            if self.read_block(blk_num, &mut block_buf).is_err() {
                return Err(VfsError::IoError);
            }

            let start_off = if blk_idx == start_block_idx { offset_in_start } else { 0 };
            let chunk = (block_sz - start_off).min(bytes_to_read - bytes_read);
            buf[bytes_read..bytes_read + chunk].copy_from_slice(&block_buf[start_off..start_off + chunk]);
            bytes_read += chunk;
        }

        Ok(bytes_read)
    }

    fn write(&self, _handle: u64, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        // Protótipo Fase 2: Somente Leitura por segurança contra corrupção
        Err(VfsError::NotSupported)
    }

    fn create(&self, _path: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn remove(&self, _path: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn readdir(&self, path: &str, cb: &mut dyn FnMut(&str, NodeType)) {
        if !self.is_mounted { return; }
        if let Ok((_, dir_inode)) = self.resolve_path(path) {
            let _ = self.for_each_dir_entry(&dir_inode, |_, ftype, name| {
                let node_type = if ftype == EXT2_FT_DIR {
                    NodeType::Directory
                } else {
                    NodeType::File
                };
                cb(name, node_type);
                true
            });
        }
    }
}

// ── Instância Global e API de Montagem ─────────────────────────────────────────

pub static mut EXT2_GLOBAL: Ext2Fs = Ext2Fs::new();

/// Monta uma partição Ext2 no ponto de montagem VFS indicado (ex: `"/ext2"` ou `"/mnt"`)
pub fn mount_ext2_partition(prefix: &'static str, drive: u8, partition_lba: u32) -> Result<(), &'static str> {
    unsafe {
        let fs = &mut *core::ptr::addr_of_mut!(EXT2_GLOBAL);
        fs.init_from_partition(drive, partition_lba)?;
        vfs::mount(prefix, fs).map_err(|_| "Falha ao registrar ponto de montagem Ext2 no VFS")?;
    }
    Ok(())
}
