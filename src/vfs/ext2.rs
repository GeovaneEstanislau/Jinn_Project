/// Jinn OS — Driver de Sistema de Arquivos Ext2 (Protótipo Read-Only)
///
/// Implementa a leitura de volumes formatados em Ext2 (Second Extended Filesystem),
/// fornecendo compatibilidade nativa com partições Linux/POSIX no Jinn OS.
///
/// Arquitetura do Volume Ext2:
///   [ Boot Record (1024 B) ] -> [ Superblock (1024 B) ] -> [ Block Group Descriptors ]
///   -> [ Block Bitmaps / Inode Bitmaps ] -> [ Inode Tables ] -> [ Data Blocks ]

use crate::vfs::{self, FileSystem, FsCapabilities, NodeType, Stat, VfsError, VfsResult};

pub const EXT2_SUPER_MAGIC: u16 = 0xEF53;
pub const EXT2_ROOT_INO: u32    = 2;
pub const EXT2_S_IFDIR: u16     = 0x4000;
pub const EXT2_S_IFREG: u16     = 0x8000;
pub const EXT2_FT_REG_FILE: u8  = 1;
pub const EXT2_FT_DIR: u8       = 2;

pub const MAX_EXT2_BLOCK_SIZE: usize = 4096;
const MAX_OPEN_FILES: usize = 32;

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
    pub partition_sectors: u64,
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
            partition_sectors: 0,
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

    #[inline]
    fn read_sec(&self, lba: u32, buf: &mut [u8; 512]) -> Result<(), &'static str> {
        if self.partition_sectors > 0 && (lba as u64) >= (self.partition_lba as u64 + self.partition_sectors) {
            return Err("Acesso de leitura fora dos limites da particao Ext2 (Bloqueado)");
        }
        crate::vfs::cache::read_block(self.drive, lba as u64, buf)
    }

    /// Inicializa e valida o Superblock Ext2 na partição especificada
    pub fn init_from_partition(&mut self, drive: u8, partition_lba: u32, partition_sectors: u64) -> Result<(), &'static str> {
        self.drive = drive;
        self.partition_lba = partition_lba;
        self.partition_sectors = partition_sectors;

        // O Superblock Ext2 fica localizado no byte 1024 (LBA offset 2 de 512 bytes)
        let mut sb_buf = [0u8; 512];
        self.read_sec(partition_lba + 2, &mut sb_buf)?;

        // Valida Magic Number Ext2: 0xEF53 (offset 56 no superblock)
        let magic = u16::from_le_bytes([sb_buf[56], sb_buf[57]]);
        if magic != EXT2_SUPER_MAGIC {
            return Err("Assinatura Ext2 invalida (magic != 0xEF53)");
        }

        self.inodes_count = u32::from_le_bytes([sb_buf[0], sb_buf[1], sb_buf[2], sb_buf[3]]);
        self.blocks_count = u32::from_le_bytes([sb_buf[4], sb_buf[5], sb_buf[6], sb_buf[7]]);
        self.first_data_block = u32::from_le_bytes([sb_buf[20], sb_buf[21], sb_buf[22], sb_buf[23]]);

        let log_block_size = u32::from_le_bytes([sb_buf[24], sb_buf[25], sb_buf[26], sb_buf[27]]);
        let computed_block_size = 1024usize << log_block_size;
        if computed_block_size > MAX_EXT2_BLOCK_SIZE {
            return Err("Tamanho de bloco Ext2 excede o limite suportado (4096 bytes)");
        }
        self.block_size = computed_block_size;

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
            self.read_sec(start_lba + s as u32, &mut sec_buf)?;
            let copy_len = (buf.len().saturating_sub(offset)).min(512);
            buf[offset..offset + copy_len].copy_from_slice(&sec_buf[..copy_len]);
        }
        Ok(())
    }

    #[inline]
    fn write_sec(&self, lba: u32, buf: &[u8; 512]) -> Result<(), &'static str> {
        if self.partition_sectors > 0 && (lba as u64) >= (self.partition_lba as u64 + self.partition_sectors) {
            return Err("Acesso de escrita fora dos limites da particao Ext2 (Bloqueado)");
        }
        crate::vfs::cache::write_block(self.drive, lba as u64, buf)
    }

    /// Escreve um bloco de dados no sistema de arquivos Ext2
    fn write_block(&self, block_num: u32, buf: &[u8]) -> Result<(), &'static str> {
        let sectors_per_block = self.block_size / 512;
        let start_lba = self.partition_lba + (block_num * sectors_per_block as u32);

        for s in 0..sectors_per_block {
            let offset = s * 512;
            let mut sec_buf = [0u8; 512];
            let copy_len = (buf.len().saturating_sub(offset)).min(512);
            sec_buf[..copy_len].copy_from_slice(&buf[offset..offset + copy_len]);
            self.write_sec(start_lba + s as u32, &sec_buf)?;
        }
        Ok(())
    }

    /// Serializa e grava as alterações do Inode de volta para a Inode Table
    pub fn write_inode(&self, inode_num: u32, inode: &Ext2Inode) -> Result<(), &'static str> {
        if inode_num == 0 || inode_num > self.inodes_count {
            return Err("Numero de Inode Ext2 fora dos limites");
        }

        let group = (inode_num - 1) / self.inodes_per_group;
        let index = (inode_num - 1) % self.inodes_per_group;
        let table_start_block = self.get_inode_table_block(group)?;

        let block_offset = (index * self.inode_size as u32) / self.block_size as u32;
        let byte_offset = ((index * self.inode_size as u32) % self.block_size as u32) as usize;

        let target_block = table_start_block + block_offset;
        let mut block_buf = [0u8; MAX_EXT2_BLOCK_SIZE];
        self.read_block(target_block, &mut block_buf[..self.block_size])?;

        let b = &mut block_buf[byte_offset..byte_offset + self.inode_size as usize];
        b[0..2].copy_from_slice(&inode.mode.to_le_bytes());
        b[2..4].copy_from_slice(&inode.uid.to_le_bytes());
        b[4..8].copy_from_slice(&inode.size.to_le_bytes());
        b[8..12].copy_from_slice(&inode.atime.to_le_bytes());
        b[12..16].copy_from_slice(&inode.ctime.to_le_bytes());
        b[16..20].copy_from_slice(&inode.mtime.to_le_bytes());
        b[20..24].copy_from_slice(&inode.dtime.to_le_bytes());
        b[24..26].copy_from_slice(&inode.gid.to_le_bytes());
        b[26..28].copy_from_slice(&inode.links_count.to_le_bytes());
        b[28..32].copy_from_slice(&inode.blocks.to_le_bytes());
        b[32..36].copy_from_slice(&inode.flags.to_le_bytes());

        for i in 0..12 {
            let off = 40 + (i * 4);
            b[off..off + 4].copy_from_slice(&inode.direct_blocks[i].to_le_bytes());
        }
        b[88..92].copy_from_slice(&inode.indirect_block.to_le_bytes());

        self.write_block(target_block, &block_buf[..self.block_size])?;
        Ok(())
    }

    /// Obtém a tabela de descritores de grupo de blocos e localiza a Inode Table
    /// Retorna o bloco e o offset dentro do bloco para o descritor do grupo especificado
    fn get_group_desc_location(&self, group: u32) -> (u32, usize) {
        let desc_start_block = if self.block_size == 1024 { 2u32 } else { 1u32 };
        let descs_per_block = (self.block_size / 32) as u32;
        let desc_block = desc_start_block + (group / descs_per_block);
        let desc_index = (group % descs_per_block) as usize;
        (desc_block, desc_index * 32)
    }

    /// Aloca um novo Inode livre no bitmap de Inodes do Ext2
    pub fn alloc_inode(&self, is_dir: bool) -> Result<u32, VfsError> {
        let groups_count = (self.inodes_count + self.inodes_per_group - 1) / self.inodes_per_group;
        let mut desc_buf = [0u8; MAX_EXT2_BLOCK_SIZE];
        let mut bitmap_buf = [0u8; MAX_EXT2_BLOCK_SIZE];

        for group in 0..groups_count {
            let (desc_block, desc_offset) = self.get_group_desc_location(group);
            if self.read_block(desc_block, &mut desc_buf[..self.block_size]).is_err() {
                return Err(VfsError::IoError);
            }

            let free_inodes = u16::from_le_bytes([
                desc_buf[desc_offset + 14],
                desc_buf[desc_offset + 15],
            ]);

            if free_inodes == 0 {
                continue;
            }

            let inode_bitmap_block = u32::from_le_bytes([
                desc_buf[desc_offset + 4],
                desc_buf[desc_offset + 5],
                desc_buf[desc_offset + 6],
                desc_buf[desc_offset + 7],
            ]);

            if self.read_block(inode_bitmap_block, &mut bitmap_buf[..self.block_size]).is_err() {
                return Err(VfsError::IoError);
            }

            let inodes_in_this_group = self.inodes_per_group.min(self.inodes_count.saturating_sub(group * self.inodes_per_group));
            // No Grupo 0, Inodes 1..10 são estritamente reservados pelo padrão Ext2
            // (1: bad blocks, 2: root, 3: ACL idx, 4: ACL data, 5: bootloader, 6: undel dir, 7..10: res)
            let start_i = if group == 0 { 10 } else { 0 };
            for i in start_i..inodes_in_this_group as usize {
                let byte_idx = i / 8;
                let bit_idx = i % 8;
                if (bitmap_buf[byte_idx] & (1 << bit_idx)) == 0 {
                    // Inode livre encontrado! Marca como ocupado
                    bitmap_buf[byte_idx] |= 1 << bit_idx;
                    if self.write_block(inode_bitmap_block, &bitmap_buf[..self.block_size]).is_err() {
                        return Err(VfsError::IoError);
                    }

                    // Atualiza descritor de grupo
                    let new_free_inodes = free_inodes.saturating_sub(1);
                    desc_buf[desc_offset + 14..desc_offset + 16].copy_from_slice(&new_free_inodes.to_le_bytes());
                    if is_dir {
                        let used_dirs = u16::from_le_bytes([desc_buf[desc_offset + 16], desc_buf[desc_offset + 17]]);
                        desc_buf[desc_offset + 16..desc_offset + 18].copy_from_slice(&(used_dirs + 1).to_le_bytes());
                    }
                    if self.write_block(desc_block, &desc_buf[..self.block_size]).is_err() {
                        return Err(VfsError::IoError);
                    }

                    // Atualiza superblock (free_inodes_count fica no offset 16 do superblock)
                    let sb_block = if self.block_size == 1024 { 1 } else { 0 };
                    let mut sb_buf = [0u8; MAX_EXT2_BLOCK_SIZE];
                    if self.read_block(sb_block, &mut sb_buf[..self.block_size]).is_ok() {
                        let sb_free = u32::from_le_bytes([sb_buf[16], sb_buf[17], sb_buf[18], sb_buf[19]]);
                        let new_sb_free = sb_free.saturating_sub(1);
                        sb_buf[16..20].copy_from_slice(&new_sb_free.to_le_bytes());
                        let _ = self.write_block(sb_block, &sb_buf[..self.block_size]);
                    }

                    let inode_num = group * self.inodes_per_group + (i as u32) + 1;
                    return Ok(inode_num);
                }
            }
        }

        Err(VfsError::NoSpace)
    }

    /// Aloca um novo bloco de dados livre no bitmap de blocos do Ext2
    pub fn alloc_block(&self) -> Result<u32, VfsError> {
        let groups_count = (self.blocks_count + self.blocks_per_group - 1) / self.blocks_per_group;
        let mut desc_buf = [0u8; MAX_EXT2_BLOCK_SIZE];
        let mut bitmap_buf = [0u8; MAX_EXT2_BLOCK_SIZE];

        for group in 0..groups_count {
            let (desc_block, desc_offset) = self.get_group_desc_location(group);
            if self.read_block(desc_block, &mut desc_buf[..self.block_size]).is_err() {
                return Err(VfsError::IoError);
            }

            let free_blocks = u16::from_le_bytes([
                desc_buf[desc_offset + 12],
                desc_buf[desc_offset + 13],
            ]);

            if free_blocks == 0 {
                continue;
            }

            let block_bitmap_block = u32::from_le_bytes([
                desc_buf[desc_offset],
                desc_buf[desc_offset + 1],
                desc_buf[desc_offset + 2],
                desc_buf[desc_offset + 3],
            ]);

            if self.read_block(block_bitmap_block, &mut bitmap_buf[..self.block_size]).is_err() {
                return Err(VfsError::IoError);
            }

            let blocks_in_group = self.blocks_per_group.min(self.blocks_count.saturating_sub(group * self.blocks_per_group));
            for i in 0..blocks_in_group as usize {
                let block_num = self.first_data_block + group * self.blocks_per_group + (i as u32);
                if block_num >= self.blocks_count {
                    return Err(VfsError::NoSpace);
                }

                // Protege blocos de metadados no Grupo 0 contra sobrescrita acidental
                // (Blocos de boot, superblock, descritores, bitmaps e tabela de inodes)
                if group == 0 {
                    let itable_blocks = ((self.inodes_per_group as usize * self.inode_size as usize) + self.block_size - 1) / self.block_size;
                    let first_safe_data_block = 5 + itable_blocks as u32;
                    if block_num < first_safe_data_block {
                        continue;
                    }
                }

                let byte_idx = i / 8;
                let bit_idx = i % 8;
                if (bitmap_buf[byte_idx] & (1 << bit_idx)) == 0 {
                    bitmap_buf[byte_idx] |= 1 << bit_idx;
                    if self.write_block(block_bitmap_block, &bitmap_buf[..self.block_size]).is_err() {
                        return Err(VfsError::IoError);
                    }

                    // Atualiza descritor de grupo
                    let new_free_blocks = free_blocks.saturating_sub(1);
                    desc_buf[desc_offset + 12..desc_offset + 14].copy_from_slice(&new_free_blocks.to_le_bytes());
                    if self.write_block(desc_block, &desc_buf[..self.block_size]).is_err() {
                        return Err(VfsError::IoError);
                    }

                    // Atualiza superblock (free_blocks_count fica no offset 12 do superblock)
                    let sb_block = if self.block_size == 1024 { 1 } else { 0 };
                    let mut sb_buf = [0u8; MAX_EXT2_BLOCK_SIZE];
                    if self.read_block(sb_block, &mut sb_buf[..self.block_size]).is_ok() {
                        let sb_free = u32::from_le_bytes([sb_buf[12], sb_buf[13], sb_buf[14], sb_buf[15]]);
                        let new_sb_free = sb_free.saturating_sub(1);
                        sb_buf[12..16].copy_from_slice(&new_sb_free.to_le_bytes());
                        let _ = self.write_block(sb_block, &sb_buf[..self.block_size]);
                    }

                    let block_num = self.first_data_block + group * self.blocks_per_group + (i as u32);
                    // Zera o bloco alocado
                    let zero_buf = [0u8; MAX_EXT2_BLOCK_SIZE];
                    let _ = self.write_block(block_num, &zero_buf[..self.block_size]);

                    return Ok(block_num);
                }
            }
        }

        Err(VfsError::NoSpace)
    }

    /// Libera um Inode no bitmap de Inodes do Ext2
    pub fn free_inode(&self, inode_num: u32, is_dir: bool) -> Result<(), VfsError> {
        if inode_num == 0 || inode_num > self.inodes_count {
            return Err(VfsError::InvalidPath);
        }

        let group = (inode_num - 1) / self.inodes_per_group;
        let index = ((inode_num - 1) % self.inodes_per_group) as usize;

        let (desc_block, desc_offset) = self.get_group_desc_location(group);
        let mut desc_buf = [0u8; MAX_EXT2_BLOCK_SIZE];
        if self.read_block(desc_block, &mut desc_buf[..self.block_size]).is_err() {
            return Err(VfsError::IoError);
        }

        let inode_bitmap_block = u32::from_le_bytes([
            desc_buf[desc_offset + 4],
            desc_buf[desc_offset + 5],
            desc_buf[desc_offset + 6],
            desc_buf[desc_offset + 7],
        ]);

        let mut bitmap_buf = [0u8; MAX_EXT2_BLOCK_SIZE];
        if self.read_block(inode_bitmap_block, &mut bitmap_buf[..self.block_size]).is_err() {
            return Err(VfsError::IoError);
        }

        let byte_idx = index / 8;
        let bit_idx = index % 8;
        bitmap_buf[byte_idx] &= !(1 << bit_idx);

        if self.write_block(inode_bitmap_block, &bitmap_buf[..self.block_size]).is_err() {
            return Err(VfsError::IoError);
        }

        let free_inodes = u16::from_le_bytes([desc_buf[desc_offset + 14], desc_buf[desc_offset + 15]]);
        desc_buf[desc_offset + 14..desc_offset + 16].copy_from_slice(&(free_inodes + 1).to_le_bytes());
        if is_dir {
            let used_dirs = u16::from_le_bytes([desc_buf[desc_offset + 16], desc_buf[desc_offset + 17]]);
            desc_buf[desc_offset + 16..desc_offset + 18].copy_from_slice(&(used_dirs.saturating_sub(1)).to_le_bytes());
        }
        let _ = self.write_block(desc_block, &desc_buf[..self.block_size]);

        // Atualiza superblock
        let sb_block = if self.block_size == 1024 { 1 } else { 0 };
        let mut sb_buf = [0u8; MAX_EXT2_BLOCK_SIZE];
        if self.read_block(sb_block, &mut sb_buf[..self.block_size]).is_ok() {
            let sb_free = u32::from_le_bytes([sb_buf[16], sb_buf[17], sb_buf[18], sb_buf[19]]);
            sb_buf[16..20].copy_from_slice(&(sb_free + 1).to_le_bytes());
            let _ = self.write_block(sb_block, &sb_buf[..self.block_size]);
        }

        Ok(())
    }

    /// Libera um bloco de dados no bitmap de blocos do Ext2
    pub fn free_block(&self, block_num: u32) -> Result<(), VfsError> {
        if block_num < self.first_data_block || block_num >= self.blocks_count {
            return Err(VfsError::InvalidPath);
        }

        let rel_block = block_num - self.first_data_block;
        let group = rel_block / self.blocks_per_group;
        let index = (rel_block % self.blocks_per_group) as usize;

        let (desc_block, desc_offset) = self.get_group_desc_location(group);
        let mut desc_buf = [0u8; MAX_EXT2_BLOCK_SIZE];
        if self.read_block(desc_block, &mut desc_buf[..self.block_size]).is_err() {
            return Err(VfsError::IoError);
        }

        let block_bitmap_block = u32::from_le_bytes([
            desc_buf[desc_offset],
            desc_buf[desc_offset + 1],
            desc_buf[desc_offset + 2],
            desc_buf[desc_offset + 3],
        ]);

        let mut bitmap_buf = [0u8; MAX_EXT2_BLOCK_SIZE];
        if self.read_block(block_bitmap_block, &mut bitmap_buf[..self.block_size]).is_err() {
            return Err(VfsError::IoError);
        }

        let byte_idx = index / 8;
        let bit_idx = index % 8;
        bitmap_buf[byte_idx] &= !(1 << bit_idx);

        if self.write_block(block_bitmap_block, &bitmap_buf[..self.block_size]).is_err() {
            return Err(VfsError::IoError);
        }

        let free_blocks = u16::from_le_bytes([desc_buf[desc_offset + 12], desc_buf[desc_offset + 13]]);
        desc_buf[desc_offset + 12..desc_offset + 14].copy_from_slice(&(free_blocks + 1).to_le_bytes());
        let _ = self.write_block(desc_block, &desc_buf[..self.block_size]);

        // Atualiza superblock
        let sb_block = if self.block_size == 1024 { 1 } else { 0 };
        let mut sb_buf = [0u8; MAX_EXT2_BLOCK_SIZE];
        if self.read_block(sb_block, &mut sb_buf[..self.block_size]).is_ok() {
            let sb_free = u32::from_le_bytes([sb_buf[12], sb_buf[13], sb_buf[14], sb_buf[15]]);
            sb_buf[12..16].copy_from_slice(&(sb_free + 1).to_le_bytes());
            let _ = self.write_block(sb_block, &sb_buf[..self.block_size]);
        }

        Ok(())
    }

    /// Adiciona uma entrada de diretÃ³rio (Ext2DirEntry) a um Inode pai
    pub fn add_dir_entry(
        &self,
        parent_inode_num: u32,
        parent_inode: &mut Ext2Inode,
        entry_inode: u32,
        name: &str,
        file_type: u8,
    ) -> Result<(), VfsError> {
        let name_bytes = name.as_bytes();
        if name_bytes.len() > 255 {
            return Err(VfsError::InvalidPath);
        }
        let needed_rec_len = ((8 + name_bytes.len() + 3) & !3) as u16;

        let mut block_buf = [0u8; MAX_EXT2_BLOCK_SIZE];
        let block_sz = self.block_size;

        // 1. Tenta encontrar espaÃ§o nos blocos de diretÃ³rio existentes
        for blk_idx in 0..12 {
            let blk_num = parent_inode.direct_blocks[blk_idx];
            if blk_num == 0 {
                break;
            }

            if self.read_block(blk_num, &mut block_buf[..block_sz]).is_err() {
                return Err(VfsError::IoError);
            }

            let mut offset = 0usize;
            while offset < block_sz {
                let entry_ino = u32::from_le_bytes([
                    block_buf[offset],
                    block_buf[offset + 1],
                    block_buf[offset + 2],
                    block_buf[offset + 3],
                ]);
                let rec_len = u16::from_le_bytes([
                    block_buf[offset + 4],
                    block_buf[offset + 5],
                ]);
                let name_len = block_buf[offset + 6] as usize;

                if rec_len == 0 {
                    break;
                }

                let actual_len = if entry_ino != 0 {
                    ((8 + name_len + 3) & !3) as u16
                } else {
                    0
                };

                let remaining_space = rec_len.saturating_sub(actual_len);

                if entry_ino != 0 && remaining_space >= needed_rec_len {
                    block_buf[offset + 4..offset + 6].copy_from_slice(&actual_len.to_le_bytes());

                    let new_off = offset + actual_len as usize;
                    block_buf[new_off..new_off + 4].copy_from_slice(&entry_inode.to_le_bytes());
                    block_buf[new_off + 4..new_off + 6].copy_from_slice(&remaining_space.to_le_bytes());
                    block_buf[new_off + 6] = name_bytes.len() as u8;
                    block_buf[new_off + 7] = file_type;
                    block_buf[new_off + 8..new_off + 8 + name_bytes.len()].copy_from_slice(name_bytes);

                    self.write_block(blk_num, &block_buf[..block_sz]).map_err(|_| VfsError::IoError)?;
                    return Ok(());
                } else if entry_ino == 0 && rec_len >= needed_rec_len {
                    block_buf[offset..offset + 4].copy_from_slice(&entry_inode.to_le_bytes());
                    block_buf[offset + 6] = name_bytes.len() as u8;
                    block_buf[offset + 7] = file_type;
                    block_buf[offset + 8..offset + 8 + name_bytes.len()].copy_from_slice(name_bytes);

                    self.write_block(blk_num, &block_buf[..block_sz]).map_err(|_| VfsError::IoError)?;
                    return Ok(());
                }

                offset += rec_len as usize;
            }
        }

        // 2. Aloca novo bloco direto se necessÃ¡rio
        for blk_idx in 0..12 {
            if parent_inode.direct_blocks[blk_idx] == 0 {
                let new_blk = self.alloc_block()?;
                parent_inode.direct_blocks[blk_idx] = new_blk;
                parent_inode.blocks += (block_sz / 512) as u32;
                parent_inode.size += block_sz as u32;

                let mut new_blk_buf = [0u8; MAX_EXT2_BLOCK_SIZE];
                new_blk_buf[0..4].copy_from_slice(&entry_inode.to_le_bytes());
                new_blk_buf[4..6].copy_from_slice(&(block_sz as u16).to_le_bytes());
                new_blk_buf[6] = name_bytes.len() as u8;
                new_blk_buf[7] = file_type;
                new_blk_buf[8..8 + name_bytes.len()].copy_from_slice(name_bytes);

                self.write_block(new_blk, &new_blk_buf[..block_sz]).map_err(|_| VfsError::IoError)?;
                self.write_inode(parent_inode_num, parent_inode).map_err(|_| VfsError::IoError)?;
                return Ok(());
            }
        }

        Err(VfsError::NoSpace)
    }
    fn get_inode_table_block(&self, group: u32) -> Result<u32, &'static str> {
        // Se block_size == 1024, superblock está no bloco 1 e descritores começam no bloco 2.
        // Se block_size > 1024, superblock está no bloco 0 e descritores no bloco 1.
        let desc_start_block = if self.block_size == 1024 { 2u32 } else { 1u32 };
        let descs_per_block = (self.block_size / 32) as u32;
        let desc_block = desc_start_block + (group / descs_per_block);
        let desc_index = (group % descs_per_block) as usize;

        let mut block_buf = [0u8; MAX_EXT2_BLOCK_SIZE];
        self.read_block(desc_block, &mut block_buf[..self.block_size])?;

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

        let mut block_buf = [0u8; MAX_EXT2_BLOCK_SIZE];
        self.read_block(block_in_table, &mut block_buf[..self.block_size])?;

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

    /// Itera sobre as entradas de diretório contidas no Inode especificado com verificação de integridade
    pub fn for_each_dir_entry<F>(&self, dir_inode: &Ext2Inode, mut callback: F) -> Result<(), &'static str>
    where
        F: FnMut(u32, u8, &str) -> bool,
    {
        if !dir_inode.is_dir() {
            return Err("O Inode alvo nao e um diretorio Ext2");
        }

        let mut block_buf = [0u8; MAX_EXT2_BLOCK_SIZE];

        for &block_num in dir_inode.direct_blocks.iter() {
            if block_num == 0 { break; }
            self.read_block(block_num, &mut block_buf[..self.block_size])?;

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

                // Proteção contra metadados malformados / corrompidos (loop infinito ou buffer overflow)
                if rec_len < 8 || offset + rec_len > self.block_size {
                    break;
                }

                if ino != 0 && offset + 8 + name_len <= offset + rec_len {
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
            mode: inode.mode as u32,
            uid: inode.uid as u32,
            gid: inode.gid as u32,
        })
    }

    fn open(&self, path: &str, flags: u32) -> VfsResult<u64> {
        if !self.is_mounted { return Err(VfsError::IoError); }

        let (ino, mut inode) = match self.resolve_path(path) {
            Ok(res) => res,
            Err(VfsError::NotFound) if (flags & crate::vfs::O_CREAT) != 0 => {
                self.create(path)?;
                self.resolve_path(path)?
            }
            Err(e) => return Err(e),
        };

        if inode.is_dir() {
            return Err(VfsError::IsADirectory);
        }

        // Truncamento se solicitado via O_TRUNC
        if (flags & crate::vfs::O_TRUNC) != 0 && inode.size > 0 {
            for i in 0..12 {
                let blk = inode.direct_blocks[i];
                if blk != 0 {
                    let _ = self.free_block(blk);
                    inode.direct_blocks[i] = 0;
                }
            }
            inode.size = 0;
            inode.blocks = 0;
            let _ = self.write_inode(ino, &inode);
        }

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
        let mut block_buf = [0u8; MAX_EXT2_BLOCK_SIZE];

        for blk_idx in start_block_idx..12 {
            if bytes_read >= bytes_to_read { break; }
            let blk_num = inode.direct_blocks[blk_idx];
            if blk_num == 0 { break; }

            if self.read_block(blk_num, &mut block_buf[..block_sz]).is_err() {
                return Err(VfsError::IoError);
            }

            let start_off = if blk_idx == start_block_idx { offset_in_start } else { 0 };
            let chunk = (block_sz - start_off).min(bytes_to_read - bytes_read);
            buf[bytes_read..bytes_read + chunk].copy_from_slice(&block_buf[start_off..start_off + chunk]);
            bytes_read += chunk;
        }

        Ok(bytes_read)
    }

    fn write(&self, handle: u64, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        if !self.is_mounted { return Err(VfsError::NotSupported); }
        if buf.is_empty() { return Ok(0); }

        let (mut inode, inode_num) = unsafe {
            let fs_ptr = core::ptr::addr_of!(EXT2_GLOBAL);
            let slot = (*fs_ptr).open_files.get(handle as usize).ok_or(VfsError::NotFound)?;
            if !slot.is_active { return Err(VfsError::NotFound); }
            (slot.inode, slot.inode_num)
        };

        let mut bytes_written = 0usize;
        let mut curr_offset = offset as usize;
        let block_sz = self.block_size;

        while bytes_written < buf.len() {
            let block_index = curr_offset / block_sz;
            let block_offset = curr_offset % block_sz;
            let to_write = (buf.len() - bytes_written).min(block_sz - block_offset);

            // Suporte para escrita em blocos diretos (0..12)
            if block_index < 12 {
                let mut phys_block = inode.direct_blocks[block_index];
                if phys_block == 0 {
                    phys_block = self.alloc_block()?;
                    inode.direct_blocks[block_index] = phys_block;
                    inode.blocks += (block_sz / 512) as u32;
                    self.write_inode(inode_num, &inode).map_err(|_| VfsError::IoError)?;
                    unsafe {
                        let fs_ptr = core::ptr::addr_of_mut!(EXT2_GLOBAL);
                        if let Some(slot) = (*fs_ptr).open_files.get_mut(handle as usize) {
                            slot.inode = inode;
                        }
                    }
                }
                let mut block_buf = [0u8; MAX_EXT2_BLOCK_SIZE];
                self.read_block(phys_block, &mut block_buf[..block_sz]).map_err(|_| VfsError::IoError)?;
                block_buf[block_offset..block_offset + to_write].copy_from_slice(&buf[bytes_written..bytes_written + to_write]);
                self.write_block(phys_block, &block_buf[..block_sz]).map_err(|_| VfsError::IoError)?;
            } else {
                return Err(VfsError::NotSupported);
            }

            curr_offset += to_write;
            bytes_written += to_write;
        }

        if curr_offset as u32 > inode.size {
            inode.size = curr_offset as u32;
            self.write_inode(inode_num, &inode).map_err(|_| VfsError::IoError)?;
            unsafe {
                let fs_ptr = core::ptr::addr_of_mut!(EXT2_GLOBAL);
                if let Some(slot) = (*fs_ptr).open_files.get_mut(handle as usize) {
                    slot.inode.size = inode.size;
                }
            }
        }

        Ok(bytes_written)
    }

    fn create(&self, path: &str) -> VfsResult<()> {
        if !self.is_mounted { return Err(VfsError::NotSupported); }
        let clean = path.trim_matches('/');
        if clean.is_empty() { return Err(VfsError::InvalidPath); }

        if self.resolve_path(clean).is_ok() {
            return Err(VfsError::AlreadyExists);
        }

        let (parent_path, leaf_name) = match clean.rfind('/') {
            Some(idx) => (&clean[..idx], &clean[idx + 1..]),
            None => ("", clean),
        };

        let (parent_ino_num, mut parent_inode) = if parent_path.is_empty() {
            (EXT2_ROOT_INO, self.read_inode(EXT2_ROOT_INO).map_err(|_| VfsError::IoError)?)
        } else {
            self.resolve_path(parent_path)?
        };

        if (parent_inode.mode & 0xF000) != EXT2_S_IFDIR {
            return Err(VfsError::NotADirectory);
        }

        let new_ino_num = self.alloc_inode(false)?;
        let mut new_inode = Ext2Inode::empty();
        new_inode.mode = EXT2_S_IFREG | 0o644;
        new_inode.size = 0;
        new_inode.links_count = 1;
        new_inode.blocks = 0;

        self.write_inode(new_ino_num, &new_inode).map_err(|_| VfsError::IoError)?;
        self.add_dir_entry(parent_ino_num, &mut parent_inode, new_ino_num, leaf_name, EXT2_FT_REG_FILE)?;
        self.sync()?;

        Ok(())
    }

    fn mkdir(&self, path: &str) -> VfsResult<()> {
        if !self.is_mounted { return Err(VfsError::NotSupported); }
        let clean = path.trim_matches('/');
        if clean.is_empty() { return Err(VfsError::InvalidPath); }

        if self.resolve_path(clean).is_ok() {
            return Err(VfsError::AlreadyExists);
        }

        let (parent_path, leaf_name) = match clean.rfind('/') {
            Some(idx) => (&clean[..idx], &clean[idx + 1..]),
            None => ("", clean),
        };

        let (parent_ino_num, mut parent_inode) = if parent_path.is_empty() {
            (EXT2_ROOT_INO, self.read_inode(EXT2_ROOT_INO).map_err(|_| VfsError::IoError)?)
        } else {
            self.resolve_path(parent_path)?
        };

        if (parent_inode.mode & 0xF000) != EXT2_S_IFDIR {
            return Err(VfsError::NotADirectory);
        }

        let new_ino_num = self.alloc_inode(true)?;
        let new_blk_num = self.alloc_block()?;

        // Formata o novo bloco do diretÃ³rio com as entradas "." e ".."
        let mut blk_buf = [0u8; MAX_EXT2_BLOCK_SIZE];
        let block_sz = self.block_size;

        // Entrada "." (offset 0, rec_len 12)
        blk_buf[0..4].copy_from_slice(&new_ino_num.to_le_bytes());
        blk_buf[4..6].copy_from_slice(&12u16.to_le_bytes());
        blk_buf[6] = 1;
        blk_buf[7] = EXT2_FT_DIR;
        blk_buf[8] = b'.';
        blk_buf[9..12].fill(0);

        // Entrada ".." (offset 12, rec_len = block_sz - 12)
        let dotdot_rec_len = (block_sz - 12) as u16;
        blk_buf[12..16].copy_from_slice(&parent_ino_num.to_le_bytes());
        blk_buf[16..18].copy_from_slice(&dotdot_rec_len.to_le_bytes());
        blk_buf[18] = 2;
        blk_buf[19] = EXT2_FT_DIR;
        blk_buf[20] = b'.';
        blk_buf[21] = b'.';

        self.write_block(new_blk_num, &blk_buf[..block_sz]).map_err(|_| VfsError::IoError)?;

        let mut new_inode = Ext2Inode::empty();
        new_inode.mode = EXT2_S_IFDIR | 0o755;
        new_inode.size = block_sz as u32;
        new_inode.links_count = 2;
        new_inode.blocks = (block_sz / 512) as u32;
        new_inode.direct_blocks[0] = new_blk_num;

        self.write_inode(new_ino_num, &new_inode).map_err(|_| VfsError::IoError)?;
        self.add_dir_entry(parent_ino_num, &mut parent_inode, new_ino_num, leaf_name, EXT2_FT_DIR)?;

        parent_inode.links_count += 1;
        self.write_inode(parent_ino_num, &parent_inode).map_err(|_| VfsError::IoError)?;
        self.sync()?;

        Ok(())
    }

    fn remove(&self, path: &str) -> VfsResult<()> {
        if !self.is_mounted { return Err(VfsError::NotSupported); }
        let clean = path.trim_matches('/');
        if clean.is_empty() { return Err(VfsError::InvalidPath); }

        let (target_ino_num, target_inode) = self.resolve_path(clean)?;
        if target_ino_num == EXT2_ROOT_INO {
            return Err(VfsError::PermissionDenied);
        }

        let is_dir = (target_inode.mode & 0xF000) == EXT2_S_IFDIR;
        if is_dir {
            let mut is_empty = true;
            let _ = self.for_each_dir_entry(&target_inode, |_, _, name| {
                if name != "." && name != ".." {
                    is_empty = false;
                    return false;
                }
                true
            });
            if !is_empty {
                return Err(VfsError::Busy);
            }
        }

        let (parent_path, leaf_name) = match clean.rfind('/') {
            Some(idx) => (&clean[..idx], &clean[idx + 1..]),
            None => ("", clean),
        };

        let (parent_ino_num, mut parent_inode) = if parent_path.is_empty() {
            (EXT2_ROOT_INO, self.read_inode(EXT2_ROOT_INO).map_err(|_| VfsError::IoError)?)
        } else {
            self.resolve_path(parent_path)?
        };

        let block_sz = self.block_size;
        let mut block_buf = [0u8; MAX_EXT2_BLOCK_SIZE];
        let mut removed = false;

        for blk_idx in 0..12 {
            let blk_num = parent_inode.direct_blocks[blk_idx];
            if blk_num == 0 { break; }

            if self.read_block(blk_num, &mut block_buf[..block_sz]).is_err() {
                return Err(VfsError::IoError);
            }

            let mut offset = 0usize;
            let mut prev_offset: Option<usize> = None;

            while offset < block_sz {
                let ino = u32::from_le_bytes([block_buf[offset], block_buf[offset + 1], block_buf[offset + 2], block_buf[offset + 3]]);
                let rec_len = u16::from_le_bytes([block_buf[offset + 4], block_buf[offset + 5]]) as usize;
                let name_len = block_buf[offset + 6] as usize;

                if rec_len == 0 { break; }

                if ino == target_ino_num {
                    if let Ok(n) = core::str::from_utf8(&block_buf[offset + 8..offset + 8 + name_len]) {
                        if n == leaf_name {
                            if let Some(prev_off) = prev_offset {
                                let prev_rec = u16::from_le_bytes([block_buf[prev_off + 4], block_buf[prev_off + 5]]) as usize;
                                let new_prev_rec = (prev_rec + rec_len) as u16;
                                block_buf[prev_off + 4..prev_off + 6].copy_from_slice(&new_prev_rec.to_le_bytes());
                            } else {
                                block_buf[offset..offset + 4].fill(0);
                            }
                            let _ = self.write_block(blk_num, &block_buf[..block_sz]);
                            removed = true;
                            break;
                        }
                    }
                }

                prev_offset = Some(offset);
                offset += rec_len;
            }

            if removed { break; }
        }

        if !removed {
            return Err(VfsError::NotFound);
        }

        for &blk_num in &target_inode.direct_blocks {
            if blk_num != 0 {
                let _ = self.free_block(blk_num);
            }
        }

        let _ = self.free_inode(target_ino_num, is_dir);

        if is_dir && parent_inode.links_count > 2 {
            parent_inode.links_count -= 1;
            let _ = self.write_inode(parent_ino_num, &parent_inode);
        }

        self.sync()?;
        Ok(())
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

    fn capabilities(&self) -> FsCapabilities {
        FsCapabilities {
            read_only: false,
            supports_create: true,
            supports_remove: true,
            supports_write: true,
            supports_directories: true,
            supports_journaling: false,
            supports_permissions: true,
            max_file_size: 2 * 1024 * 1024 * 1024 * 1024,
            name: "Ext2 (Read/Write)",
        }
    }
    fn statvfs(&self) -> VfsResult<crate::vfs::FsStat> {
        if !self.is_mounted {
            return Err(VfsError::NotSupported);
        }
        let total_bytes = (self.blocks_count as u64) * (self.block_size as u64);
        let used_bytes = (self.first_data_block as u64 + 100) * (self.block_size as u64);
        let free_bytes = total_bytes.saturating_sub(used_bytes);

        Ok(crate::vfs::FsStat {
            total_bytes,
            free_bytes,
            block_size: self.block_size,
            total_inodes: self.inodes_count as u64,
            free_inodes: self.inodes_count.saturating_sub(20) as u64,
            fs_type: "Ext2",
            is_read_only: false,
        })
    }

    fn sync(&self) -> VfsResult<()> {
        let _ = crate::vfs::cache::flush_device(self.drive);
        let _ = crate::drivers::block::flush_primary_cache();
        Ok(())
    }

    fn unmount(&self) -> VfsResult<()> {
        unsafe {
            let fs_ptr = core::ptr::addr_of_mut!(EXT2_GLOBAL);
            // Verificação de ocupação (Busy-checking): impede desmontagem se houver arquivos abertos
            for slot in (*fs_ptr).open_files.iter() {
                if slot.is_active {
                    return Err(VfsError::Busy);
                }
            }
            let _ = self.sync();
            crate::vfs::cache::invalidate_device(self.drive);
            (*fs_ptr).is_mounted = false;
        }
        Ok(())
    }
}

// ── Instância Global e API de Montagem ─────────────────────────────────────────

pub static mut EXT2_GLOBAL: Ext2Fs = Ext2Fs::new();

/// Monta uma partição Ext2 no ponto de montagem VFS indicado (ex: `"/ext2"` ou `"/mnt"`)
pub fn mount_ext2_partition(prefix: &str, drive: u8, partition_lba: u32, partition_sectors: u64) -> Result<(), &'static str> {
    unsafe {
        let fs = &mut *core::ptr::addr_of_mut!(EXT2_GLOBAL);
        fs.init_from_partition(drive, partition_lba, partition_sectors)?;
        vfs::mount(prefix, fs).map_err(|_| "Falha ao registrar ponto de montagem Ext2 no VFS")?;
    }
    Ok(())
}
