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

// ── Perfis de Armazenamento Adaptativo (JSA Storage Profiles) ─────────────────

/// Perfis de configuração adaptativa de armazenamento da JSA
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageProfile {
    /// Aladdin (Doméstico): Foco em usabilidade, write-through balanceado, prefetch moderado e recuperação simples
    Aladdin,
    /// Simbad (Servidor): Máxima integridade, write-back com diário e barreiras, telemetria detalhada de I/O
    Simbad,
    /// Workstation: Estações de trabalho, cargas mistas, readahead agressivo e prioridade a baixa latência
    Workstation,
    /// Terminal / POS: Computadores dedicados/embarcados, cache mínimo, write-through estrito e imediato
    TerminalPos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteStrategy {
    WriteThrough,
    WriteBack,
}

#[derive(Debug, Clone, Copy)]
pub struct StoragePolicy {
    pub profile: StorageProfile,
    pub max_cache_slots: usize,
    pub write_strategy: WriteStrategy,
    pub prefetch_sectors: usize,
    pub barrier_required: bool,
}

impl StorageProfile {
    pub const fn policy(&self) -> StoragePolicy {
        match self {
            StorageProfile::Aladdin => StoragePolicy {
                profile: StorageProfile::Aladdin,
                max_cache_slots: 16,
                write_strategy: WriteStrategy::WriteThrough,
                prefetch_sectors: 2,
                barrier_required: false,
            },
            StorageProfile::Simbad => StoragePolicy {
                profile: StorageProfile::Simbad,
                max_cache_slots: 32,
                write_strategy: WriteStrategy::WriteBack,
                prefetch_sectors: 4,
                barrier_required: true,
            },
            StorageProfile::Workstation => StoragePolicy {
                profile: StorageProfile::Workstation,
                max_cache_slots: 32,
                write_strategy: WriteStrategy::WriteThrough,
                prefetch_sectors: 8,
                barrier_required: false,
            },
            StorageProfile::TerminalPos => StoragePolicy {
                profile: StorageProfile::TerminalPos,
                max_cache_slots: 8,
                write_strategy: WriteStrategy::WriteThrough,
                prefetch_sectors: 0,
                barrier_required: true,
            },
        }
    }

    pub const fn name(&self) -> &'static str {
        match self {
            StorageProfile::Aladdin => "Aladdin (Domestico)",
            StorageProfile::Simbad => "Simbad (Servidor)",
            StorageProfile::Workstation => "Workstation (Engenharia)",
            StorageProfile::TerminalPos => "Terminal/POS (Resiliente)",
        }
    }
}

static mut ACTIVE_STORAGE_PROFILE: StorageProfile = StorageProfile::Aladdin;

pub fn get_storage_profile() -> StorageProfile {
    unsafe { *core::ptr::addr_of!(ACTIVE_STORAGE_PROFILE) }
}

pub fn set_storage_profile(profile: StorageProfile) {
    let old_policy = get_storage_policy();
    let new_policy = profile.policy();

    // Transição de segurança: se mudar de WriteBack para WriteThrough ou alterar limites, descarrega a cache
    if old_policy.write_strategy == WriteStrategy::WriteBack || new_policy.write_strategy == WriteStrategy::WriteThrough {
        let _ = flush_primary_cache();
    }

    unsafe { *core::ptr::addr_of_mut!(ACTIVE_STORAGE_PROFILE) = profile; }
}

pub fn get_storage_policy() -> StoragePolicy {
    get_storage_profile().policy()
}

// ── Cache de Blocos na Memória RAM (Block Cache) ──────────────────────────────

pub const CACHE_SLOTS: usize = 8;

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

    /// Retorna o número de slots válidos atualmente modificados (dirty)
    pub fn dirty_count(&self) -> usize {
        let mut count = 0;
        for s in self.slots.iter() {
            if s.is_valid && s.is_dirty {
                count += 1;
            }
        }
        count
    }

    /// Descarrega todos os slots dirty para o dispositivo subjacente
    pub fn flush_dirty<D: BlockDevice>(&mut self, dev: &D) -> Result<(), &'static str> {
        for slot in self.slots.iter_mut() {
            if slot.is_valid && slot.is_dirty {
                dev.write_blocks(slot.lba, 1, &slot.data)?;
                slot.is_dirty = false;
            }
        }
        Ok(())
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

    /// Insere ou atualiza um setor na cache, garantindo que se o slot LRU a ser evictado
    /// for sujo (dirty), ele seja descarregado para o dispositivo subjacente antes da substituição.
    /// Isso previne perda catastrófica de dados em cargas intensivas com WriteBack.
    pub fn insert_evicting<D: BlockDevice>(&mut self, dev: &D, lba: u64, data: &[u8; DEFAULT_SECTOR_SIZE], is_dirty: bool) -> Result<(), &'static str> {
        self.clock = self.clock.wrapping_add(1);
        // 1. Atualiza se já presente
        for slot in self.slots.iter_mut() {
            if slot.is_valid && slot.lba == lba {
                slot.data.copy_from_slice(data);
                slot.is_dirty = is_dirty;
                slot.access_counter = self.clock;
                return Ok(());
            }
        }

        // 2. Utiliza slot vazio se disponível
        for slot in self.slots.iter_mut() {
            if !slot.is_valid {
                slot.lba = lba;
                slot.data.copy_from_slice(data);
                slot.is_dirty = is_dirty;
                slot.is_valid = true;
                slot.access_counter = self.clock;
                return Ok(());
            }
        }

        // 3. Política de substituição LRU (menor contador de acesso)
        let mut min_idx = 0;
        let mut min_val = u32::MAX;
        for (i, slot) in self.slots.iter().enumerate() {
            if slot.access_counter < min_val {
                min_val = slot.access_counter;
                min_idx = i;
            }
        }

        // Proteção de Integridade: se o slot evictado for dirty, persiste no disco antes!
        let slot = &mut self.slots[min_idx];
        if slot.is_valid && slot.is_dirty {
            dev.write_blocks(slot.lba, 1, &slot.data)?;
            slot.is_dirty = false;
        }

        slot.lba = lba;
        slot.data.copy_from_slice(data);
        slot.is_dirty = is_dirty;
        slot.is_valid = true;
        slot.access_counter = self.clock;
        Ok(())
    }
}

/// Dispositivo de blocos com cache LRU na RAM para amortizar acessos físicos ao disco
pub struct CachedBlockDevice<D: BlockDevice> {
    pub inner: D,
    pub cache: crate::memory::SpinLock<BlockCache>,
    pub hits: core::sync::atomic::AtomicU64,
    pub misses: core::sync::atomic::AtomicU64,
}

impl<D: BlockDevice> CachedBlockDevice<D> {
    pub fn new(inner: D) -> Self {
        Self {
            inner,
            cache: crate::memory::SpinLock::new(BlockCache::new()),
            hits: core::sync::atomic::AtomicU64::new(0),
            misses: core::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn stats(&self) -> (u64, u64) {
        (
            self.hits.load(core::sync::atomic::Ordering::Relaxed),
            self.misses.load(core::sync::atomic::Ordering::Relaxed),
        )
    }

    pub fn dirty_count(&self) -> usize {
        self.cache.lock().dirty_count()
    }
}

impl<D: BlockDevice> BlockDevice for CachedBlockDevice<D> {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn block_size(&self) -> usize {
        self.inner.block_size()
    }

    fn total_blocks(&self) -> u64 {
        self.inner.total_blocks()
    }

    fn read_blocks(&self, start_block: u64, count: usize, buf: &mut [u8]) -> Result<(), &'static str> {
        let block_sz = self.block_size();
        let mut cache = self.cache.lock();

        for i in 0..count {
            let lba = start_block + i as u64;
            let offset = i * block_sz;

            if let Some(cached_data) = cache.lookup(lba) {
                buf[offset..offset + block_sz].copy_from_slice(cached_data);
                self.hits.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            } else {
                let mut sector = [0u8; DEFAULT_SECTOR_SIZE];
                self.inner.read_blocks(lba, 1, &mut sector)?;
                let _ = cache.insert_evicting(&self.inner, lba, &sector, false);
                buf[offset..offset + block_sz].copy_from_slice(&sector);
                self.misses.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            }
        }

        Ok(())
    }

    fn write_blocks(&self, start_block: u64, count: usize, buf: &[u8]) -> Result<(), &'static str> {
        let block_sz = self.block_size();
        let policy = get_storage_policy();
        let mut cache = self.cache.lock();

        match policy.write_strategy {
            WriteStrategy::WriteThrough => {
                // Atualiza cache (com evicção segura) e escreve imediatamente no disco físico
                for i in 0..count {
                    let lba = start_block + i as u64;
                    let offset = i * block_sz;
                    let mut sector = [0u8; DEFAULT_SECTOR_SIZE];
                    sector.copy_from_slice(&buf[offset..offset + block_sz]);
                    let _ = cache.insert_evicting(&self.inner, lba, &sector, false);
                }
                self.inner.write_blocks(start_block, count, buf)
            }
            WriteStrategy::WriteBack => {
                // Marca como dirty no cache com evicção segura de dirty anterior para consolidação posterior
                for i in 0..count {
                    let lba = start_block + i as u64;
                    let offset = i * block_sz;
                    let mut sector = [0u8; DEFAULT_SECTOR_SIZE];
                    sector.copy_from_slice(&buf[offset..offset + block_sz]);
                    cache.insert_evicting(&self.inner, lba, &sector, true)?;
                }
                Ok(())
            }
        }
    }

    fn flush(&self) -> Result<(), &'static str> {
        let mut cache = self.cache.lock();
        cache.flush_dirty(&self.inner)?;
        self.inner.flush()
    }
}

// ── Instância Global do Dispositivo Primário com Cache e Flusher Periódico ────

/// Dispositivo de blocos IDE com cache LRU primário compartilhado globalmente
pub static PRIMARY_CACHED_DEV: crate::memory::SpinLock<Option<CachedBlockDevice<IdeBlockDevice>>> =
    crate::memory::SpinLock::new(None);

/// Inicializa o dispositivo de blocos primário com cache LRU
pub fn init_primary_cache() {
    if let Ok(dev) = IdeBlockDevice::new(0) {
        let mut lock = PRIMARY_CACHED_DEV.lock();
        *lock = Some(CachedBlockDevice::new(dev));
    }
}

/// Lê um setor do disco primário utilizando a camada de cache LRU
pub fn cached_read_sector(lba: u64, buf: &mut [u8; DEFAULT_SECTOR_SIZE]) -> Result<(), &'static str> {
    let mut dev_opt = PRIMARY_CACHED_DEV.lock();
    if let Some(ref mut cdev) = *dev_opt {
        cdev.read_blocks(lba, 1, buf)
    } else {
        crate::drivers::ide::read_sector(0, lba as u32, buf)
    }
}

/// Escreve um setor no disco primário utilizando a camada de cache LRU e a estratégia ativa (WriteThrough ou WriteBack)
pub fn cached_write_sector(lba: u64, buf: &[u8; DEFAULT_SECTOR_SIZE]) -> Result<(), &'static str> {
    let mut dev_opt = PRIMARY_CACHED_DEV.lock();
    if let Some(ref mut cdev) = *dev_opt {
        cdev.write_blocks(lba, 1, buf)
    } else {
        crate::drivers::ide::write_sector(0, lba as u32, buf)
    }
}

/// Lê múltiplos setores contíguos do disco primário utilizando a camada de cache
pub fn read_into_buffer(start_block: u64, count: usize, buf: &mut [u8]) -> Result<(), &'static str> {
    let mut dev_opt = PRIMARY_CACHED_DEV.lock();
    if let Some(ref mut cdev) = *dev_opt {
        cdev.read_blocks(start_block, count, buf)
    } else {
        Err("Dispositivo de blocos primario nao inicializado")
    }
}

/// Escreve múltiplos setores contíguos no disco primário utilizando a camada de cache
pub fn write_from_buffer(start_block: u64, count: usize, buf: &[u8]) -> Result<(), &'static str> {
    let mut dev_opt = PRIMARY_CACHED_DEV.lock();
    if let Some(ref mut cdev) = *dev_opt {
        cdev.write_blocks(start_block, count, buf)
    } else {
        Err("Dispositivo de blocos primario nao inicializado")
    }
}

/// Executa a sincronização (FLUSH) de todos os blocos marcados como dirty na memória para o disco
pub fn flush_primary_cache() -> Result<usize, &'static str> {
    let mut dev_opt = PRIMARY_CACHED_DEV.lock();
    if let Some(ref mut cdev) = *dev_opt {
        let dirty = cdev.dirty_count();
        if dirty > 0 {
            cdev.flush()?;
        }
        Ok(dirty)
    } else {
        Ok(0)
    }
}

/// Retorna estatísticas da cache primária: (hits, misses, dirty_count)
pub fn primary_cache_stats() -> (u64, u64, usize) {
    let dev_opt = PRIMARY_CACHED_DEV.lock();
    if let Some(ref cdev) = *dev_opt {
        let (hits, misses) = cdev.stats();
        let dirty = cdev.dirty_count();
        (hits, misses, dirty)
    } else {
        (0, 0, 0)
    }
}

/// Verificação periódica de descarregamento de cache para o perfil Simbad (WriteBack)
pub fn periodic_flush_check() {
    let policy = get_storage_policy();
    if policy.write_strategy == WriteStrategy::WriteBack {
        let _ = flush_primary_cache();
    }
}
