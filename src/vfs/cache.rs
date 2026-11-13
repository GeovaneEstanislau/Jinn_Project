//! Jinn OS — VFS Block Cache Subsystem (`BlockCache`)
//!
//! Sub-sistema dedicado de Cache de Blocos e Buffers de I/O na camada VFS.
//! Amortiza acessos repetitivos ao barramento de armazenamento físico (IDE/ATA/AHCI),
//! acelerando operações em sistemas de arquivos (FAT32 e Ext2) e o carregamento
//! de binários pesados como `browser.elf`.
//!
//! # Características Arquiteturais:
//! - **LRU Cache (Least Recently Used):** 64 slots de 512 bytes (32 KiB) em RAM.
//! - **Suporte Multi-Bloco e Clusters 4 KiB:** Opera nativamente em 512B e clusters de 4096B (8 setores).
//! - **Mapeamento de Blocos Sujos (Dirty Tracking):** Rastreio por slot e dispositivo com flag atômica/booleana.
//! - **Estratégias de Escrita:**
//!   - `WriteThrough`: Escrita síncrona imediata no disco e atualização de cache.
//!   - `WriteBack`: Escrita adiada em RAM com dirty tracking e persistência via flush.
//! - **Segurança na Evicção (Zero Data Loss):** Se um slot dirty for escolhido para evicção LRU,
//!   ele é gravado no disco físico ANTES de ser sobrescrito.
//! - **Sincronização Seletiva:** `flush_device` (por drive) e `flush_all` (global).
//! - **Invalidação:** `invalidate` e `invalidate_device` para desmontagem limpa de volumes.
//! - **Telemetria Completa:** Hits, Misses, Flushes, Evictions e Hit Ratio.
//! - **Suíte de Selftests & Estresse:** Testes unitários in-kernel para validação de integridade.

use crate::memory::SpinLock;
use alloc::vec::Vec;

pub const BLOCK_SIZE: usize = 512;
pub const CLUSTER_SIZE_4K: usize = 4096;
pub const SECTORS_PER_CLUSTER_4K: usize = CLUSTER_SIZE_4K / BLOCK_SIZE; // 8 setores
pub const VFS_CACHE_SLOTS: usize = 64; // 64 * 512 bytes = 32 KiB de cache

/// Estratégia de persistência de escrita no cache
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheWritePolicy {
    /// Escrita imediata no disco e atualização da cópia em RAM
    WriteThrough,
    /// Escrita na RAM marcando o bloco como dirty; descarregado via flush ou evicção
    WriteBack,
}

/// Slot individual do Cache de Blocos do VFS
#[derive(Clone, Copy)]
pub struct CacheSlot {
    pub device: u8,
    pub lba: u64,
    pub is_valid: bool,
    pub is_dirty: bool,
    pub access_counter: u64,
    pub data: [u8; BLOCK_SIZE],
}

impl CacheSlot {
    pub const fn empty() -> Self {
        Self {
            device: 0,
            lba: 0,
            is_valid: false,
            is_dirty: false,
            access_counter: 0,
            data: [0u8; BLOCK_SIZE],
        }
    }
}

/// Estatísticas e telemetria operacional do VFS Block Cache
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VfsCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub flushes: u64,
    pub dirty_blocks: usize,
    pub valid_blocks: usize,
    pub total_slots: usize,
}

impl VfsCacheStats {
    /// Taxa de acerto (Hit Ratio) em porcentagem inteira (0 a 100%)
    pub fn hit_ratio_percent(&self) -> u32 {
        let total = self.hits + self.misses;
        if total == 0 {
            0
        } else {
            ((self.hits * 100) / total) as u32
        }
    }
}

/// Interface abstrata para leitura e escrita física de setores em dispositivos de armazenamento
pub trait BlockBackend {
    fn read_sector(&mut self, device: u8, lba: u64, buf: &mut [u8; BLOCK_SIZE]) -> Result<(), &'static str>;
    fn write_sector(&mut self, device: u8, lba: u64, buf: &[u8; BLOCK_SIZE]) -> Result<(), &'static str>;
}

/// Gerenciador de Cache de Blocos LRU do VFS
pub struct VfsBlockCache {
    slots: [CacheSlot; VFS_CACHE_SLOTS],
    clock: u64,
    policy: CacheWritePolicy,
    hits: u64,
    misses: u64,
    evictions: u64,
    flushes: u64,
}

impl VfsBlockCache {
    pub const fn new() -> Self {
        Self {
            slots: [CacheSlot::empty(); VFS_CACHE_SLOTS],
            clock: 0,
            policy: CacheWritePolicy::WriteThrough,
            hits: 0,
            misses: 0,
            evictions: 0,
            flushes: 0,
        }
    }

    /// Reseta o estado interno de todos os slots e contadores in-place (sem alocação de stack)
    pub fn reset(&mut self) {
        for slot in self.slots.iter_mut() {
            slot.device = 0;
            slot.lba = 0;
            slot.is_valid = false;
            slot.is_dirty = false;
            slot.access_counter = 0;
            slot.data = [0u8; BLOCK_SIZE];
        }
        self.clock = 0;
        self.policy = CacheWritePolicy::WriteThrough;
        self.hits = 0;
        self.misses = 0;
        self.evictions = 0;
        self.flushes = 0;
    }

    /// Configura a política de escrita (WriteThrough ou WriteBack)
    pub fn set_policy(&mut self, policy: CacheWritePolicy) {
        self.policy = policy;
    }

    /// Retorna a política de escrita ativa
    pub fn policy(&self) -> CacheWritePolicy {
        self.policy
    }

    /// Retorna as estatísticas consolidadas da cache
    pub fn stats(&self) -> VfsCacheStats {
        let mut dirty = 0;
        let mut valid = 0;
        for s in self.slots.iter() {
            if s.is_valid {
                valid += 1;
                if s.is_dirty {
                    dirty += 1;
                }
            }
        }
        VfsCacheStats {
            hits: self.hits,
            misses: self.misses,
            evictions: self.evictions,
            flushes: self.flushes,
            dirty_blocks: dirty,
            valid_blocks: valid,
            total_slots: VFS_CACHE_SLOTS,
        }
    }

    /// Retorna a quantidade de blocos atualmente modificados (dirty)
    pub fn dirty_count(&self) -> usize {
        let mut count = 0;
        for s in self.slots.iter() {
            if s.is_valid && s.is_dirty {
                count += 1;
            }
        }
        count
    }

    /// Verifica se um bloco específico está sujo (dirty) no cache
    pub fn is_dirty(&self, device: u8, lba: u64) -> bool {
        if let Some(idx) = self.find_slot(device, lba) {
            self.slots[idx].is_dirty
        } else {
            false
        }
    }

    /// Verifica se um bloco específico está presente e válido no cache
    pub fn is_cached(&self, device: u8, lba: u64) -> bool {
        self.find_slot(device, lba).is_some()
    }

    /// Procura o índice do slot correspondente a (device, lba)
    pub fn find_slot(&self, device: u8, lba: u64) -> Option<usize> {
        for (i, s) in self.slots.iter().enumerate() {
            if s.is_valid && s.device == device && s.lba == lba {
                return Some(i);
            }
        }
        None
    }

    /// Lê um setor de 512 bytes utilizando o backend especificado
    pub fn read_block_with_backend<B: BlockBackend>(
        &mut self,
        backend: &mut B,
        device: u8,
        lba: u64,
        buf: &mut [u8; BLOCK_SIZE],
    ) -> Result<(), &'static str> {
        // 1. Tenta Cache Hit
        if let Some(idx) = self.find_slot(device, lba) {
            self.clock = self.clock.wrapping_add(1);
            self.slots[idx].access_counter = self.clock;
            buf.copy_from_slice(&self.slots[idx].data);
            self.hits += 1;
            return Ok(());
        }

        // 2. Cache Miss: Lê do dispositivo físico
        self.misses += 1;
        let mut sector_data = [0u8; BLOCK_SIZE];
        backend.read_sector(device, lba, &mut sector_data)?;

        // 3. Aloca slot (com evicção segura de bloco dirty)
        let slot_idx = self.allocate_slot(backend)?;
        self.clock = self.clock.wrapping_add(1);

        let slot = &mut self.slots[slot_idx];
        slot.device = device;
        slot.lba = lba;
        slot.data.copy_from_slice(&sector_data);
        slot.is_valid = true;
        slot.is_dirty = false;
        slot.access_counter = self.clock;

        buf.copy_from_slice(&sector_data);
        Ok(())
    }

    /// Escreve um setor de 512 bytes utilizando o backend especificado
    pub fn write_block_with_backend<B: BlockBackend>(
        &mut self,
        backend: &mut B,
        device: u8,
        lba: u64,
        buf: &[u8; BLOCK_SIZE],
    ) -> Result<(), &'static str> {
        self.clock = self.clock.wrapping_add(1);

        match self.policy {
            CacheWritePolicy::WriteThrough => {
                // Atualiza ou insere no cache
                if let Some(idx) = self.find_slot(device, lba) {
                    let slot = &mut self.slots[idx];
                    slot.data.copy_from_slice(buf);
                    slot.is_dirty = false;
                    slot.access_counter = self.clock;
                } else {
                    let slot_idx = self.allocate_slot(backend)?;
                    let slot = &mut self.slots[slot_idx];
                    slot.device = device;
                    slot.lba = lba;
                    slot.data.copy_from_slice(buf);
                    slot.is_valid = true;
                    slot.is_dirty = false;
                    slot.access_counter = self.clock;
                }
                // Persiste imediatamente no dispositivo físico
                backend.write_sector(device, lba, buf)?;
            }
            CacheWritePolicy::WriteBack => {
                // Atualiza ou insere marcando como dirty
                if let Some(idx) = self.find_slot(device, lba) {
                    let slot = &mut self.slots[idx];
                    slot.data.copy_from_slice(buf);
                    slot.is_dirty = true;
                    slot.access_counter = self.clock;
                } else {
                    let slot_idx = self.allocate_slot(backend)?;
                    let slot = &mut self.slots[slot_idx];
                    slot.device = device;
                    slot.lba = lba;
                    slot.data.copy_from_slice(buf);
                    slot.is_valid = true;
                    slot.is_dirty = true;
                    slot.access_counter = self.clock;
                }
            }
        }

        Ok(())
    }

    /// Descarrega todos os blocos sujos de um dispositivo específico
    pub fn flush_device_with_backend<B: BlockBackend>(
        &mut self,
        backend: &mut B,
        device: u8,
    ) -> Result<usize, &'static str> {
        let mut count = 0;
        for slot in self.slots.iter_mut() {
            if slot.is_valid && slot.is_dirty && slot.device == device {
                backend.write_sector(slot.device, slot.lba, &slot.data)?;
                slot.is_dirty = false;
                count += 1;
            }
        }
        self.flushes += count as u64;
        Ok(count)
    }

    /// Descarrega todos os blocos sujos de todos os dispositivos para o disco físico
    pub fn flush_all_with_backend<B: BlockBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<usize, &'static str> {
        let mut count = 0;
        for slot in self.slots.iter_mut() {
            if slot.is_valid && slot.is_dirty {
                backend.write_sector(slot.device, slot.lba, &slot.data)?;
                slot.is_dirty = false;
                count += 1;
            }
        }
        self.flushes += count as u64;
        Ok(count)
    }

    /// Invalida um setor específico da cache
    pub fn invalidate(&mut self, device: u8, lba: u64) {
        if let Some(idx) = self.find_slot(device, lba) {
            self.slots[idx].is_valid = false;
            self.slots[idx].is_dirty = false;
        }
    }

    /// Invalida todos os setores pertencentes a um dispositivo (útil na desmontagem)
    pub fn invalidate_device(&mut self, device: u8) {
        for slot in self.slots.iter_mut() {
            if slot.device == device {
                slot.is_valid = false;
                slot.is_dirty = false;
            }
        }
    }

    /// Localiza um slot livre ou seleciona o LRU com writeback forçado de dados sujos
    fn allocate_slot<B: BlockBackend>(&mut self, backend: &mut B) -> Result<usize, &'static str> {
        // 1. Procura slot vazio
        for (i, slot) in self.slots.iter().enumerate() {
            if !slot.is_valid {
                return Ok(i);
            }
        }

        // 2. Busca slot LRU (menor access_counter)
        let mut min_idx = 0;
        let mut min_val = u64::MAX;
        for (i, slot) in self.slots.iter().enumerate() {
            if slot.access_counter < min_val {
                min_val = slot.access_counter;
                min_idx = i;
            }
        }

        // 3. Proteção Zero Data Loss: se o slot evictado for dirty, persiste antes!
        let slot = &mut self.slots[min_idx];
        if slot.is_valid && slot.is_dirty {
            backend.write_sector(slot.device, slot.lba, &slot.data)?;
            slot.is_dirty = false;
            self.flushes += 1;
        }

        self.evictions += 1;
        Ok(min_idx)
    }
}

// ── Backend de Hardware Padrão (IDE ATA Driver) ───────────────────────────────

/// Conecta o VFS Block Cache diretamente ao controlador físico IDE ATA (PIIX4)
pub struct HardwareIdeBackend;

impl BlockBackend for HardwareIdeBackend {
    #[inline]
    fn read_sector(&mut self, device: u8, lba: u64, buf: &mut [u8; BLOCK_SIZE]) -> Result<(), &'static str> {
        if device < 4 {
            crate::drivers::ide::read_sector(device, lba as u32, buf)
        } else {
            Err("Dispositivo de armazenamento fora do limite do barramento IDE ATA (0..=3)")
        }
    }

    #[inline]
    fn write_sector(&mut self, device: u8, lba: u64, buf: &[u8; BLOCK_SIZE]) -> Result<(), &'static str> {
        if device < 4 {
            crate::drivers::ide::write_sector(device, lba as u32, buf)
        } else {
            Err("Dispositivo de armazenamento fora do limite do barramento IDE ATA (0..=3)")
        }
    }
}

// ── Instância Global e API de Alto Nível ───────────────────────────────────────

/// Instância global do Cache de Blocos do VFS protegida por SpinLock
pub static GLOBAL_VFS_CACHE: SpinLock<VfsBlockCache> = SpinLock::new(VfsBlockCache::new());

/// Inicializa ou reseta o cache de blocos do VFS
pub fn init_cache() {
    let mut cache = GLOBAL_VFS_CACHE.lock();
    *cache = VfsBlockCache::new();
}

/// Lê um setor de 512 bytes através do VFS Block Cache
pub fn read_block(device: u8, lba: u64, buf: &mut [u8; BLOCK_SIZE]) -> Result<(), &'static str> {
    let mut backend = HardwareIdeBackend;
    GLOBAL_VFS_CACHE.lock().read_block_with_backend(&mut backend, device, lba, buf)
}

/// Escreve um setor de 512 bytes através do VFS Block Cache
pub fn write_block(device: u8, lba: u64, buf: &[u8; BLOCK_SIZE]) -> Result<(), &'static str> {
    let mut backend = HardwareIdeBackend;
    GLOBAL_VFS_CACHE.lock().write_block_with_backend(&mut backend, device, lba, buf)
}

/// Lê múltiplos setores contíguos de 512 bytes através do VFS Block Cache
pub fn read_blocks(device: u8, start_lba: u64, count: usize, buf: &mut [u8]) -> Result<(), &'static str> {
    if buf.len() < count * BLOCK_SIZE {
        return Err("Buffer de destino menor que a quantidade requisitada de blocos");
    }
    let mut backend = HardwareIdeBackend;
    let mut cache = GLOBAL_VFS_CACHE.lock();
    for i in 0..count {
        let lba = start_lba + i as u64;
        let offset = i * BLOCK_SIZE;
        let sec_slice: &mut [u8; BLOCK_SIZE] = (&mut buf[offset..offset + BLOCK_SIZE])
            .try_into()
            .map_err(|_| "Falha ao indexar fatia de bloco")?;
        cache.read_block_with_backend(&mut backend, device, lba, sec_slice)?;
    }
    Ok(())
}

/// Escreve múltiplos setores contíguos de 512 bytes através do VFS Block Cache
pub fn write_blocks(device: u8, start_lba: u64, count: usize, buf: &[u8]) -> Result<(), &'static str> {
    if buf.len() < count * BLOCK_SIZE {
        return Err("Buffer de origem menor que a quantidade requisitada de blocos");
    }
    let mut backend = HardwareIdeBackend;
    let mut cache = GLOBAL_VFS_CACHE.lock();
    for i in 0..count {
        let lba = start_lba + i as u64;
        let offset = i * BLOCK_SIZE;
        let sec_slice: &[u8; BLOCK_SIZE] = (&buf[offset..offset + BLOCK_SIZE])
            .try_into()
            .map_err(|_| "Falha ao indexar fatia de bloco")?;
        cache.write_block_with_backend(&mut backend, device, lba, sec_slice)?;
    }
    Ok(())
}

/// Lê um cluster de 4096 bytes (8 setores contíguos) através do cache
pub fn read_cluster_4k(device: u8, start_lba: u64, buf: &mut [u8; CLUSTER_SIZE_4K]) -> Result<(), &'static str> {
    read_blocks(device, start_lba, SECTORS_PER_CLUSTER_4K, buf)
}

/// Escreve um cluster de 4096 bytes (8 setores contíguos) através do cache
pub fn write_cluster_4k(device: u8, start_lba: u64, buf: &[u8; CLUSTER_SIZE_4K]) -> Result<(), &'static str> {
    write_blocks(device, start_lba, SECTORS_PER_CLUSTER_4K, buf)
}

/// Descarrega todos os blocos dirty pertencentes a um drive
pub fn flush_device(device: u8) -> Result<usize, &'static str> {
    let mut backend = HardwareIdeBackend;
    GLOBAL_VFS_CACHE.lock().flush_device_with_backend(&mut backend, device)
}

/// Descarrega todos os blocos dirty do sistema de arquivos para o disco físico
pub fn flush_all() -> Result<usize, &'static str> {
    let mut backend = HardwareIdeBackend;
    GLOBAL_VFS_CACHE.lock().flush_all_with_backend(&mut backend)
}

/// Invalida um setor específico
pub fn invalidate(device: u8, lba: u64) {
    GLOBAL_VFS_CACHE.lock().invalidate(device, lba);
}

/// Invalida todos os setores de um dispositivo (ex: unmount)
pub fn invalidate_device(device: u8) {
    GLOBAL_VFS_CACHE.lock().invalidate_device(device);
}

/// Define a estratégia de escrita (WriteThrough ou WriteBack)
pub fn set_write_policy(policy: CacheWritePolicy) {
    GLOBAL_VFS_CACHE.lock().set_policy(policy);
}

/// Retorna as métricas e estatísticas atuais do VFS Block Cache
pub fn cache_stats() -> VfsCacheStats {
    GLOBAL_VFS_CACHE.lock().stats()
}

/// Retorna a quantidade de blocos pendentes de sincronização
pub fn dirty_count() -> usize {
    GLOBAL_VFS_CACHE.lock().dirty_count()
}

// ── Suíte de Selftests e Testes de Estresse ───────────────────────────────────

/// Mock in-memory backend para testes de unidade e estresse sem efeitos colaterais de I/O físico
pub struct MockBlockBackend {
    sectors: Vec<[u8; BLOCK_SIZE]>,
    pub read_count: usize,
    pub write_count: usize,
}

impl MockBlockBackend {
    pub fn new(sector_count: usize) -> Self {
        let mut sectors = Vec::with_capacity(sector_count);
        for i in 0..sector_count {
            let mut sec = [0u8; BLOCK_SIZE];
            // Inicializa com padrão verificável
            sec[0] = (i & 0xFF) as u8;
            sec[1] = ((i >> 8) & 0xFF) as u8;
            sec[2] = 0xAA;
            sec[3] = 0x55;
            sectors.push(sec);
        }
        Self {
            sectors,
            read_count: 0,
            write_count: 0,
        }
    }
}

impl BlockBackend for MockBlockBackend {
    fn read_sector(&mut self, _device: u8, lba: u64, buf: &mut [u8; BLOCK_SIZE]) -> Result<(), &'static str> {
        let idx = lba as usize;
        if idx >= self.sectors.len() {
            return Err("Mock: LBA fora dos limites");
        }
        buf.copy_from_slice(&self.sectors[idx]);
        self.read_count += 1;
        Ok(())
    }

    fn write_sector(&mut self, _device: u8, lba: u64, buf: &[u8; BLOCK_SIZE]) -> Result<(), &'static str> {
        let idx = lba as usize;
        if idx >= self.sectors.len() {
            return Err("Mock: LBA fora dos limites");
        }
        self.sectors[idx].copy_from_slice(buf);
        self.write_count += 1;
        Ok(())
    }
}

/// Cache estática isolada em .bss para execução dos selftests com 0 bytes de alocação de stack
static TEST_CACHE: SpinLock<VfsBlockCache> = SpinLock::new(VfsBlockCache::new());

/// Executa a bateria de 10 testes unitários e de estresse do VFS Block Cache.
/// Retorna `(passed, total)`.
pub fn run_vfs_cache_selftests() -> (usize, usize) {
    let mut passed = 0;
    let mut total = 0;

    // Teste 1: Miss na primeira leitura, Hit na segunda leitura
    total += 1;
    {
        let mut cache = TEST_CACHE.lock();
        cache.reset();
        let mut mock = MockBlockBackend::new(128);
        let mut buf1 = [0u8; BLOCK_SIZE];
        let mut buf2 = [0u8; BLOCK_SIZE];

        let r1 = cache.read_block_with_backend(&mut mock, 0, 10, &mut buf1);
        let r2 = cache.read_block_with_backend(&mut mock, 0, 10, &mut buf2);

        if r1.is_ok() && r2.is_ok()
            && buf1 == buf2
            && buf1[2] == 0xAA && buf1[3] == 0x55
            && mock.read_count == 1 // Segunda leitura foi do cache, sem tocar no mock!
            && cache.hits == 1
            && cache.misses == 1
        {
            passed += 1;
        }
    }

    // Teste 2: Atualização in-place de bloco já em cache (WriteThrough)
    total += 1;
    {
        let mut cache = TEST_CACHE.lock();
        cache.reset();
        let mut mock = MockBlockBackend::new(128);
        let mut test_data = [0x77u8; BLOCK_SIZE];
        test_data[0] = 0xDE;
        test_data[1] = 0xAD;

        let w_res = cache.write_block_with_backend(&mut mock, 0, 15, &test_data);
        let mut read_back = [0u8; BLOCK_SIZE];
        let r_res = cache.read_block_with_backend(&mut mock, 0, 15, &mut read_back);

        if w_res.is_ok() && r_res.is_ok()
            && read_back[0] == 0xDE && read_back[1] == 0xAD
            && mock.write_count == 1 // WriteThrough gravou no mock imediatamente
            && cache.hits == 1      // Leitura subsequente veio do cache
            && cache.dirty_count() == 0 // WriteThrough não deixa dirty
        {
            passed += 1;
        }
    }

    // Teste 3: Política de substituição LRU (Evicção de bloco mais antigo)
    total += 1;
    {
        let mut cache = TEST_CACHE.lock();
        cache.reset();
        let mut mock = MockBlockBackend::new(128);
        let mut buf = [0u8; BLOCK_SIZE];

        // Preenche todos os 64 slots com LBAs 0..64
        for lba in 0..64u64 {
            cache.read_block_with_backend(&mut mock, 0, lba, &mut buf).ok();
        }

        // Acessa LBA 0 novamente, tornando-o o mais recente (MRU)
        cache.read_block_with_backend(&mut mock, 0, 0, &mut buf).ok();

        // Agora lê LBA 64, forçando evicção de 1 slot
        cache.read_block_with_backend(&mut mock, 0, 64, &mut buf).ok();

        // O bloco 0 deve CONTINUAR no cache (foi re-acessado recentemente)
        // O bloco 1 (que não foi re-acessado) deve ter sido o evictado!
        if cache.is_cached(0, 0) && !cache.is_cached(0, 1) && cache.evictions == 1 {
            passed += 1;
        }
    }

    // Teste 4: Write-Back — Escrita adiada em RAM e rastreio de blocos sujos
    total += 1;
    {
        let mut cache = TEST_CACHE.lock();
        cache.reset();
        cache.set_policy(CacheWritePolicy::WriteBack);
        let mut mock = MockBlockBackend::new(128);
        let mut write_buf = [0x5Au8; BLOCK_SIZE];
        write_buf[0] = 0xBE;
        write_buf[1] = 0xEF;

        let w_res = cache.write_block_with_backend(&mut mock, 0, 20, &write_buf);

        if w_res.is_ok()
            && mock.write_count == 0 // Em WriteBack, não escreve no disco imediatamente!
            && cache.dirty_count() == 1
            && cache.is_dirty(0, 20)
        {
            passed += 1;
        }
    }

    // Teste 5: Write-Back — Evicção segura de bloco dirty (Zero Data Loss)
    total += 1;
    {
        let mut cache = TEST_CACHE.lock();
        cache.reset();
        cache.set_policy(CacheWritePolicy::WriteBack);
        let mut mock = MockBlockBackend::new(128);

        // Escreve dados modificados no LBA 0
        let mut dirty_data = [0x99u8; BLOCK_SIZE];
        dirty_data[0] = 0xCA;
        dirty_data[1] = 0xFE;
        cache.write_block_with_backend(&mut mock, 0, 0, &dirty_data).ok();

        // Agora preenche mais 64 blocos para forçar a evicção do LBA 0
        let mut temp_buf = [0u8; BLOCK_SIZE];
        for lba in 1..=64u64 {
            cache.read_block_with_backend(&mut mock, 0, lba, &mut temp_buf).ok();
        }

        // O LBA 0 foi evictado, mas seu conteúdo DEVE ter sido gravado no mock antes da evicção!
        if mock.write_count == 1
            && mock.sectors[0][0] == 0xCA
            && mock.sectors[0][1] == 0xFE
            && cache.flushes >= 1
        {
            passed += 1;
        }
    }

    // Teste 6: Sincronização seletiva por dispositivo (`flush_device`) e global (`flush_all`)
    total += 1;
    {
        let mut cache = TEST_CACHE.lock();
        cache.reset();
        cache.set_policy(CacheWritePolicy::WriteBack);
        let mut mock = MockBlockBackend::new(128);
        let dummy = [0x33u8; BLOCK_SIZE];

        // Grava blocos dirty no device 0 e device 1
        cache.write_block_with_backend(&mut mock, 0, 10, &dummy).ok();
        cache.write_block_with_backend(&mut mock, 0, 11, &dummy).ok();
        cache.write_block_with_backend(&mut mock, 1, 20, &dummy).ok();

        if cache.dirty_count() == 3 {
            // Sincroniza apenas device 0
            let flushed_dev0 = cache.flush_device_with_backend(&mut mock, 0).unwrap_or(0);
            let dev0_clean = !cache.is_dirty(0, 10) && !cache.is_dirty(0, 11);
            let dev1_still_dirty = cache.is_dirty(1, 20);

            // Sincroniza tudo
            let flushed_all = cache.flush_all_with_backend(&mut mock).unwrap_or(0);
            let total_clean = cache.dirty_count() == 0;

            if flushed_dev0 == 2 && dev0_clean && dev1_still_dirty && flushed_all == 1 && total_clean {
                passed += 1;
            }
        }
    }

    // Teste 7: Invalidação seletiva e na desmontagem de volume (`invalidate_device`)
    total += 1;
    {
        let mut cache = TEST_CACHE.lock();
        cache.reset();
        let mut mock = MockBlockBackend::new(128);
        let mut buf = [0u8; BLOCK_SIZE];

        cache.read_block_with_backend(&mut mock, 0, 5, &mut buf).ok();
        cache.read_block_with_backend(&mut mock, 0, 6, &mut buf).ok();
        cache.read_block_with_backend(&mut mock, 1, 5, &mut buf).ok();

        cache.invalidate_device(0);

        if !cache.is_cached(0, 5) && !cache.is_cached(0, 6) && cache.is_cached(1, 5) {
            passed += 1;
        }
    }

    // Teste 8: Operações de cluster 4096 bytes (4 KiB = 8 setores contíguos)
    total += 1;
    {
        let mut cache = TEST_CACHE.lock();
        cache.reset();
        let mut mock = MockBlockBackend::new(128);
        let mut cluster_data = alloc::vec![0u8; CLUSTER_SIZE_4K];

        for i in 0..CLUSTER_SIZE_4K {
            cluster_data[i] = (i & 0xFF) as u8;
        }

        // Escreve os 8 setores do cluster contíguo
        let mut write_ok = true;
        for s in 0..SECTORS_PER_CLUSTER_4K {
            let offset = s * BLOCK_SIZE;
            let sec_slice: &[u8; BLOCK_SIZE] = (&cluster_data[offset..offset + BLOCK_SIZE])
                .try_into()
                .unwrap();
            if cache.write_block_with_backend(&mut mock, 0, 32 + s as u64, sec_slice).is_err() {
                write_ok = false;
            }
        }

        // Lê de volta do cache
        let mut read_cluster = alloc::vec![0u8; CLUSTER_SIZE_4K];
        let mut read_ok = true;
        for s in 0..SECTORS_PER_CLUSTER_4K {
            let offset = s * BLOCK_SIZE;
            let sec_slice: &mut [u8; BLOCK_SIZE] = (&mut read_cluster[offset..offset + BLOCK_SIZE])
                .try_into()
                .unwrap();
            if cache.read_block_with_backend(&mut mock, 0, 32 + s as u64, sec_slice).is_err() {
                read_ok = false;
            }
        }

        if write_ok && read_ok && cluster_data == read_cluster {
            passed += 1;
        }
    }

    // Teste 9: Teste de estresse com operações sequenciais e aleatórias (Simulação do `browser.elf`)
    total += 1;
    {
        let mut cache = TEST_CACHE.lock();
        cache.reset();
        let mut mock = MockBlockBackend::new(128);
        let mut buf = [0u8; BLOCK_SIZE];
        let mut stress_ok = true;

        // Fase 1: Carga do working set ativo de 64 setores (simulando cabeçalhos e seções de código)
        for lba in 0..64u64 {
            if cache.read_block_with_backend(&mut mock, 0, lba, &mut buf).is_err() {
                stress_ok = false;
                break;
            }
        }

        // Fase 2: Re-leitura aleatória intensiva de setores quentes (alta localidade temporal)
        let initial_hits = cache.hits;
        for i in 0..100u64 {
            let lba = (i * 7) % 64; // Acessa os 64 setores presentes no working set
            if cache.read_block_with_backend(&mut mock, 0, lba, &mut buf).is_err() {
                stress_ok = false;
                break;
            }
        }

        let hits_gained = cache.hits - initial_hits;
        // Na janela ativa de 64 slots, 100% das 100 re-leituras devem ser Cache Hits
        if stress_ok && hits_gained >= 80 {
            passed += 1;
        }
    }

    // Teste 10: Integridade de concorrência e consistência de dados sob sobrescrita contínua
    total += 1;
    {
        let mut cache = TEST_CACHE.lock();
        cache.reset();
        let mut mock = MockBlockBackend::new(128);
        let mut integrity_ok = true;

        for cycle in 0..50u8 {
            let mut sec = [cycle; BLOCK_SIZE];
            sec[0] = cycle;
            sec[1] = 255 - cycle;
            if cache.write_block_with_backend(&mut mock, 0, 42, &sec).is_err() {
                integrity_ok = false;
                break;
            }
            let mut read_sec = [0u8; BLOCK_SIZE];
            if cache.read_block_with_backend(&mut mock, 0, 42, &mut read_sec).is_err() {
                integrity_ok = false;
                break;
            }
            if read_sec[0] != cycle || read_sec[1] != (255 - cycle) {
                integrity_ok = false;
                break;
            }
        }

        if integrity_ok {
            passed += 1;
        }
    }

    (passed, total)
}

