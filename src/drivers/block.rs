/// Jinn OS — Abstração de Dispositivos de Bloco (Block Device Layer)
///
/// Fornece uma interface unificada entre controladores de hardware (IDE, AHCI, NVMe, RamDisk)
/// e camadas superiores (Sistemas de Arquivos e Cache de Blocos).

use crate::drivers::ide;

pub const DEFAULT_SECTOR_SIZE: usize = 512;

/// Tipo de barramento físico de conexão do dispositivo de armazenamento
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusType {
    IdeAta,
    AhciSata,
    NvmePci,
    VirtioBlk,
    RamDisk,
    Unknown,
}

impl BusType {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::IdeAta => "IDE / ATA",
            Self::AhciSata => "SATA (AHCI)",
            Self::NvmePci => "NVMe (PCIe)",
            Self::VirtioBlk => "VirtIO Blk",
            Self::RamDisk => "RAM Disk",
            Self::Unknown => "Desconhecido",
        }
    }
}

/// Estado operacional do dispositivo de blocos
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceStatus {
    Uninitialized,
    Operational,
    Degraded,
    Failed,
    Removed,
}

impl DeviceStatus {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Uninitialized => "Nao Inicializado",
            Self::Operational => "Operacional",
            Self::Degraded => "Degradado",
            Self::Failed => "Falha",
            Self::Removed => "Removido",
        }
    }
}

/// Metadados padronizados para inventário de dispositivos de bloco gerenciados
#[derive(Debug, Clone, Copy)]
pub struct BlockDeviceInfo {
    pub id: usize,
    pub name: [u8; 32],
    pub name_len: usize,
    pub model: [u8; 40],
    pub serial: [u8; 20],
    pub vendor: [u8; 32],
    pub bus_type: BusType,
    pub status: DeviceStatus,
    pub capacity_sectors: u64,
    pub logical_sector_size: usize,
    pub physical_sector_size: usize,
    pub is_read_only: bool,
    pub is_removable: bool,
    pub is_boot_device: bool,
    pub dma_capable: bool,
}

impl BlockDeviceInfo {
    pub const fn empty() -> Self {
        Self {
            id: 0,
            name: [0; 32],
            name_len: 0,
            model: [b' '; 40],
            serial: [b' '; 20],
            vendor: [0; 32],
            bus_type: BusType::Unknown,
            status: DeviceStatus::Uninitialized,
            capacity_sectors: 0,
            logical_sector_size: DEFAULT_SECTOR_SIZE,
            physical_sector_size: DEFAULT_SECTOR_SIZE,
            is_read_only: false,
            is_removable: false,
            is_boot_device: false,
            dma_capable: false,
        }
    }

    pub fn size_mb(&self) -> u64 {
        (self.capacity_sectors * self.logical_sector_size as u64) / (1024 * 1024)
    }

    pub fn name_str(&self) -> &str {
        if self.name_len == 0 { "desconhecido" } else { core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("desconhecido") }
    }

    pub fn model_str(&self) -> &str {
        core::str::from_utf8(&self.model).unwrap_or("").trim()
    }

    pub fn vendor_str(&self) -> &str {
        core::str::from_utf8(&self.vendor).unwrap_or("").trim_matches('\0').trim()
    }

    pub fn serial_str(&self) -> &str {
        core::str::from_utf8(&self.serial).unwrap_or("").trim()
    }
}

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

    /// Consulta metadados estruturados do dispositivo de blocos
    fn device_info(&self) -> BlockDeviceInfo {
        let mut info = BlockDeviceInfo::empty();
        let n = self.name().as_bytes();
        let len = n.len().min(31);
        info.name[..len].copy_from_slice(&n[..len]);
        info.name_len = len;
        info.capacity_sectors = self.total_blocks();
        info.logical_sector_size = self.block_size();
        info.physical_sector_size = self.block_size();
        info.status = DeviceStatus::Operational;
        info
    }
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

// ── Storage Device Manager (L1) ───────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageKind {
    Unknown,
    AtaDisk,
    AtapiCdrom,
    VirtioBlk,
    RamDisk,
    Removable,
}

#[derive(Debug, Clone, Copy)]
pub struct StorageDeviceInfo {
    pub id: usize,
    pub name: [u8; 32],
    pub name_len: usize,
    pub kind: StorageKind,
    pub sector_size: usize,
    pub total_sectors: u64,
    pub size_mb: u64,
    pub is_read_only: bool,
    pub is_removable: bool,
    pub model: [u8; 40],
    pub serial: [u8; 20],
}

impl StorageDeviceInfo {
    pub const fn empty() -> Self {
        Self {
            id: 0,
            name: [0; 32],
            name_len: 0,
            kind: StorageKind::AtaDisk,
            sector_size: 512,
            total_sectors: 0,
            size_mb: 0,
            is_read_only: false,
            is_removable: false,
            model: [b' '; 40],
            serial: [b' '; 20],
        }
    }
}

pub const MAX_STORAGE_DISKS: usize = 8;

pub struct StorageManager {
    pub disks: [Option<StorageDeviceInfo>; MAX_STORAGE_DISKS],
    pub count: usize,
    pub boot_disk_idx: Option<usize>,
}

impl StorageManager {
    pub const fn new() -> Self {
        Self {
            disks: [None; MAX_STORAGE_DISKS],
            count: 0,
            boot_disk_idx: None,
        }
    }

    /// Varre canais IDE primário e secundário (0..2) descobrindo discos e unidades ópticas
    pub fn probe_devices(&mut self) {
        self.count = 0;
        self.boot_disk_idx = None;

        // Procura canais IDE: 0 (Primary Master), 1 (Primary Slave)
        for drive_id in 0..2u8 {
            if let Ok(info) = ide::identify(drive_id) {
                let mut dev = StorageDeviceInfo::empty();
                dev.id = self.count;
                let prefix: &[u8] = if drive_id == 0 { b"ide0 (Master)" } else { b"ide1 (Slave)" };
                dev.name[..prefix.len()].copy_from_slice(prefix);
                dev.name_len = prefix.len();
                dev.kind = if info.is_atapi { StorageKind::AtapiCdrom } else { StorageKind::AtaDisk };
                dev.sector_size = DEFAULT_SECTOR_SIZE;
                dev.total_sectors = info.sectors_lba28 as u64;
                dev.size_mb = (info.sectors_lba28 as u64 * DEFAULT_SECTOR_SIZE as u64) / (1024 * 1024);
                dev.is_read_only = info.is_atapi;
                dev.is_removable = info.is_atapi;
                dev.model = info.model;
                dev.serial = info.serial;

                if self.boot_disk_idx.is_none() && !info.is_atapi && dev.total_sectors > 0 {
                    self.boot_disk_idx = Some(self.count);
                }

                if self.count < MAX_STORAGE_DISKS {
                    self.disks[self.count] = Some(dev);
                    self.count += 1;
                }
            }
        }
    }

    pub fn get_disk(&self, idx: usize) -> Option<&StorageDeviceInfo> {
        if idx < self.count {
            self.disks[idx].as_ref()
        } else {
            None
        }
    }

    pub fn disk_count(&self) -> usize {
        self.count
    }
}

pub static STORAGE_MANAGER: crate::memory::SpinLock<StorageManager> = crate::memory::SpinLock::new(StorageManager::new());

/// Inicializa e realiza a descoberta de dispositivos de armazenamento físicos
pub fn init_storage_manager() {
    let mut mgr = STORAGE_MANAGER.lock();
    mgr.probe_devices();
    init_block_device_manager();
}

// ── Block Device Manager Unificado (L1/L2) ───────────────────────────────────

pub const MAX_BLOCK_DEVICES: usize = 16;

/// Gerenciador unificado de dispositivos de bloco de diferentes barramentos (IDE, AHCI, NVMe, RamDisk)
pub struct BlockDeviceManager {
    pub devices: [Option<BlockDeviceInfo>; MAX_BLOCK_DEVICES],
    pub count: usize,
    pub boot_device_id: Option<usize>,
}

impl BlockDeviceManager {
    pub const fn new() -> Self {
        Self {
            devices: [None; MAX_BLOCK_DEVICES],
            count: 0,
            boot_device_id: None,
        }
    }

    /// Registra um novo dispositivo no catálogo gerenciado
    pub fn register_device(&mut self, mut info: BlockDeviceInfo) -> Result<usize, &'static str> {
        if self.count >= MAX_BLOCK_DEVICES {
            return Err("Limite maximo de dispositivos de bloco atingido (MAX_BLOCK_DEVICES)");
        }
        let id = self.count;
        info.id = id;
        if self.boot_device_id.is_none() && info.status == DeviceStatus::Operational && !info.is_read_only && info.capacity_sectors > 0 {
            self.boot_device_id = Some(id);
            info.is_boot_device = true;
        }
        self.devices[id] = Some(info);
        self.count += 1;
        Ok(id)
    }

    /// Atualiza o estado operacional de um dispositivo (ex: falha de hardware ou remocao)
    pub fn set_device_status(&mut self, id: usize, status: DeviceStatus) -> Result<(), &'static str> {
        if id < self.count {
            if let Some(ref mut dev) = self.devices[id] {
                dev.status = status;
                return Ok(());
            }
        }
        Err("Dispositivo de bloco nao encontrado")
    }

    pub fn get_device(&self, id: usize) -> Option<&BlockDeviceInfo> {
        if id < self.count {
            self.devices[id].as_ref()
        } else {
            None
        }
    }

    pub fn device_count(&self) -> usize {
        self.count
    }

    pub fn boot_device(&self) -> Option<&BlockDeviceInfo> {
        self.boot_device_id.and_then(|id| self.get_device(id))
    }

    /// Realiza a descoberta inicial de todos os dispositivos de bloco nos barramentos suportados
    pub fn probe_all(&mut self) {
        self.count = 0;
        self.boot_device_id = None;

        // 1. Procura discos IDE/ATA
        for drive_id in 0..2u8 {
            if let Ok(info) = ide::identify(drive_id) {
                let mut bdev = BlockDeviceInfo::empty();
                let name: &[u8] = if drive_id == 0 { b"ide0 (Master)" } else { b"ide1 (Slave)" };
                bdev.name[..name.len()].copy_from_slice(name);
                bdev.name_len = name.len();
                bdev.model = info.model;
                bdev.serial = info.serial;
                let v = b"ATA/IDE Generic";
                bdev.vendor[..v.len()].copy_from_slice(v);
                bdev.bus_type = BusType::IdeAta;
                bdev.status = DeviceStatus::Operational;
                bdev.capacity_sectors = info.sectors_lba28 as u64;
                bdev.logical_sector_size = DEFAULT_SECTOR_SIZE;
                bdev.physical_sector_size = DEFAULT_SECTOR_SIZE;
                bdev.is_read_only = info.is_atapi;
                bdev.is_removable = info.is_atapi;
                bdev.is_boot_device = drive_id == 0;
                bdev.dma_capable = false;

                let _ = self.register_device(bdev);
            }
        }
    }
}

pub static BLOCK_DEVICE_MANAGER: crate::memory::SpinLock<BlockDeviceManager> =
    crate::memory::SpinLock::new(BlockDeviceManager::new());

/// Inicializa e popula o catalogo global de dispositivos de blocos
pub fn init_block_device_manager() {
    let mut mgr = BLOCK_DEVICE_MANAGER.lock();
    mgr.probe_all();
}
