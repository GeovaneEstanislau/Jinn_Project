/// Jinn OS — Abstração de Dispositivos de Bloco (Block Device Layer)
///
/// Fornece uma interface unificada entre controladores de hardware (IDE, AHCI, NVMe, RamDisk)
/// e camadas superiores (Sistemas de Arquivos e Cache de Blocos).

use crate::drivers::ide;

pub const DEFAULT_SECTOR_SIZE: usize = 512;

/// Contrato obrigatório para qualquer dispositivo de armazenamento estruturado em blocos.
pub trait BlockDevice: Send + Sync {
    /// Nome amigável do dispositivo (ex: "ide0", "ramdisk0")
    fn name(&self) -> &'static str;

    /// Tamanho do bloco físico em bytes (tipicamente 512 ou 4096)
    fn block_size(&self) -> usize;

    /// Número total de blocos disponíveis no volume
    fn total_blocks(&self) -> u64;

    /// Lê múltiplos blocos contíguos a partir de `start_block`
    fn read_blocks(&self, start_block: u64, count: usize, buf: &mut [u8]) -> Result<(), &'static str>;

    /// Escreve múltiplos blocos contíguos a partir de `start_block`
    fn write_blocks(&self, start_block: u64, count: usize, buf: &[u8]) -> Result<(), &'static str>;

    /// Força o descarregamento de caches voláteis para mídia não volátil (FLUSH CACHE)
    fn flush(&self) -> Result<(), &'static str>;
}

/// Implementação do `BlockDevice` para o canal primário do controlador IDE/ATA em modo PIO.
pub struct IdeBlockDevice {
    pub drive: u8,
    pub sectors_total: u64,
}

impl IdeBlockDevice {
    pub fn new(drive: u8) -> Result<Self, &'static str> {
        let info = ide::identify(drive)?;
        Ok(Self {
            drive,
            sectors_total: info.sectors_lba28 as u64,
        })
    }
}

impl BlockDevice for IdeBlockDevice {
    fn name(&self) -> &'static str {
        match self.drive {
            0 => "ide0 (Primary Master)",
            1 => "ide1 (Primary Slave)",
            _ => "ide-unknown",
        }
    }

    fn block_size(&self) -> usize {
        DEFAULT_SECTOR_SIZE
    }

    fn total_blocks(&self) -> u64 {
        self.sectors_total
    }

    fn read_blocks(&self, start_block: u64, count: usize, buf: &mut [u8]) -> Result<(), &'static str> {
        if start_block + (count as u64) > self.sectors_total {
            return Err("Acesso fora dos limites fisicos do disco IDE");
        }
        if buf.len() < count * DEFAULT_SECTOR_SIZE {
            return Err("Buffer fornecido menor que os blocos requisitados");
        }

        let mut sector_buf = [0u8; DEFAULT_SECTOR_SIZE];
        for i in 0..count {
            let lba = (start_block + i as u64) as u32;
            ide::read_sector(self.drive, lba, &mut sector_buf)?;
            let offset = i * DEFAULT_SECTOR_SIZE;
            buf[offset..offset + DEFAULT_SECTOR_SIZE].copy_from_slice(&sector_buf);
        }

        Ok(())
    }

    fn write_blocks(&self, start_block: u64, count: usize, buf: &[u8]) -> Result<(), &'static str> {
        if start_block + (count as u64) > self.sectors_total {
            return Err("Acesso de escrita fora dos limites fisicos do disco IDE");
        }
        if buf.len() < count * DEFAULT_SECTOR_SIZE {
            return Err("Buffer fornecido menor que os blocos a gravar");
        }

        let mut sector_buf = [0u8; DEFAULT_SECTOR_SIZE];
        for i in 0..count {
            let lba = (start_block + i as u64) as u32;
            let offset = i * DEFAULT_SECTOR_SIZE;
            sector_buf.copy_from_slice(&buf[offset..offset + DEFAULT_SECTOR_SIZE]);
            ide::write_sector(self.drive, lba, &sector_buf)?;
        }

        Ok(())
    }

    fn flush(&self) -> Result<(), &'static str> {
        // Modo PIO síncrono com write-through não requer flush complexo no estágio atual
        Ok(())
    }
}

// ── Cache de Blocos na Memória RAM (Block Cache) ──────────────────────────────

pub const CACHE_SLOTS: usize = 16;

#[derive(Clone, Copy)]
pub struct CacheSlot {
    pub lba: u64,
    pub data: [u8; DEFAULT_SECTOR_SIZE],
    pub is_dirty: bool,
    pub is_valid: bool,
    pub access_counter: u32,
}

impl CacheSlot {
    pub const fn empty() -> Self {
        Self {
            lba: 0,
            data: [0u8; DEFAULT_SECTOR_SIZE],
            is_dirty: false,
            is_valid: false,
            access_counter: 0,
        }
    }
}

/// Buffer cache volátil na memória RAM para amortizar acessos repetidos ao disco
pub struct BlockCache {
    slots: [CacheSlot; CACHE_SLOTS],
    clock: u32,
}

impl BlockCache {
    pub const fn new() -> Self {
        Self {
            slots: [CacheSlot::empty(); CACHE_SLOTS],
            clock: 0,
        }
    }

    /// Busca um setor na cache. Retorna Some(&[u8; 512]) em caso de Cache Hit
    pub fn lookup(&mut self, lba: u64) -> Option<&[u8; DEFAULT_SECTOR_SIZE]> {
        self.clock = self.clock.wrapping_add(1);
        for slot in self.slots.iter_mut() {
            if slot.is_valid && slot.lba == lba {
                slot.access_counter = self.clock;
                return Some(&slot.data);
            }
        }
        None
    }

    /// Insere ou atualiza um setor na cache
    pub fn insert(&mut self, lba: u64, data: &[u8; DEFAULT_SECTOR_SIZE], is_dirty: bool) {
        self.clock = self.clock.wrapping_add(1);
        // Atualiza se já presente
        for slot in self.slots.iter_mut() {
            if slot.is_valid && slot.lba == lba {
                slot.data.copy_from_slice(data);
                slot.is_dirty = is_dirty;
                slot.access_counter = self.clock;
                return;
            }
        }

        // Utiliza slot vazio se disponível
        for slot in self.slots.iter_mut() {
            if !slot.is_valid {
                slot.lba = lba;
                slot.data.copy_from_slice(data);
                slot.is_dirty = is_dirty;
                slot.is_valid = true;
                slot.access_counter = self.clock;
                return;
            }
        }

        // Política de substituição LRU (menor contador de acesso)
        let mut min_idx = 0;
        let mut min_val = u32::MAX;
        for (i, slot) in self.slots.iter().enumerate() {
            if slot.access_counter < min_val {
                min_val = slot.access_counter;
                min_idx = i;
            }
        }

        let slot = &mut self.slots[min_idx];
        slot.lba = lba;
        slot.data.copy_from_slice(data);
        slot.is_dirty = is_dirty;
        slot.is_valid = true;
        slot.access_counter = self.clock;
    }
}
