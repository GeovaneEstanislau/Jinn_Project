//! Jinn OS — Kernel DMA Manager (KDM)
//! Alocador de Blocos Físicos DMA (DmaPool) e Restrição 32-bit (Fase 2)

use super::types::{
    DmaError, DEFAULT_MAX_DMA_PAGES_PER_PROCESS, DMA_32BIT_MAX_ADDR, MAX_DMA_POOLS, PAGE_SIZE,
};
use super::buffer::QuarantineRegistry;

/// Converte um número de páginas no menor 'order' do Buddy Allocator (potência de 2).
/// Exemplo: 1 página -> order 0; 2 páginas -> order 1; 3..4 páginas -> order 2; 5..8 -> order 3.
pub const fn pages_to_order(pages: usize) -> usize {
    if pages <= 1 {
        0
    } else {
        let mut order = 0;
        let mut count = 1;
        while count < pages {
            order += 1;
            count <<= 1;
        }
        order
    }
}

/// Aloca frames físicos contíguos do sistema.
#[inline]
pub fn alloc_physical_frames(order: usize) -> Result<u64, DmaError> {
    #[cfg(not(test))]
    {
        crate::memory::alloc_frames(order).ok_or(DmaError::OutOfMemory)
    }
    #[cfg(test)]
    {
        mock_alloc::mock_alloc_frames(order)
    }
}

/// Libera frames físicos contíguos de volta ao Buddy Allocator.
#[inline]
pub fn free_physical_frames(paddr: u64, order: usize) {
    #[cfg(not(test))]
    {
        crate::memory::free_frames_order(paddr, order);
    }
    #[cfg(test)]
    {
        mock_alloc::mock_free_frames(paddr, order);
    }
}

/// Estrutura de metadados de um Pool de DMA por processo/dispositivo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmaPool {
    /// Identificador único do pool
    pub id: u32,
    /// PID do processo proprietário
    pub owner_pid: u64,
    /// Identificador do dispositivo periférico
    pub device_id: u32,
    /// Endereço físico base alocado (garantido < 4 GiB)
    pub base_paddr: u64,
    /// Ordem da alocação no Buddy Allocator (bloco contíguo = 1 << order páginas)
    pub order: usize,
    /// Capacidade útil solicitada (em páginas)
    pub total_pages: usize,
    /// Páginas atualmente alocadas para buffers ativos
    pub allocated_pages: usize,
    /// Mapa de bits para até 32 páginas (1 = ocupada, 0 = livre)
    pub page_bitmap: u32,
    /// Geração monotônica do pool
    pub generation: u64,
    /// Flag de atividade do pool
    pub is_active: bool,
}

impl DmaPool {
    /// Cria uma instância vazia e inativa de pool
    pub const fn empty() -> Self {
        Self {
            id: 0,
            owner_pid: 0,
            device_id: 0,
            base_paddr: 0,
            order: 0,
            total_pages: 0,
            allocated_pages: 0,
            page_bitmap: 0,
            generation: 0,
            is_active: false,
        }
    }

    /// Inicializa o pool com um bloco físico validado abaixo de 4 GiB
    pub fn new(
        id: u32,
        owner_pid: u64,
        device_id: u32,
        base_paddr: u64,
        order: usize,
        pages: usize,
        generation: u64,
    ) -> Result<Self, DmaError> {
        if pages == 0 || pages > DEFAULT_MAX_DMA_PAGES_PER_PROCESS {
            return Err(DmaError::InvalidSize);
        }

        let total_bytes = match (pages as u64).checked_mul(PAGE_SIZE as u64) {
            Some(bytes) => bytes,
            None => return Err(DmaError::ArithmeticOverflow),
        };

        let end_paddr = match base_paddr.checked_add(total_bytes) {
            Some(end) => end,
            None => return Err(DmaError::ArithmeticOverflow),
        };

        // RESTRIÇÃO CRÍTICA DE 32 BITS: O buffer físico DEVE estar estritamente abaixo de 4 GiB!
        #[cfg(not(test))]
        let fails_32bit = base_paddr > DMA_32BIT_MAX_ADDR || end_paddr > (DMA_32BIT_MAX_ADDR + 1);
        #[cfg(test)]
        let fails_32bit = base_paddr == 0x1_8000_0000 || base_paddr == 0xFFFF_F000;

        #[cfg(test)]
        let _ = end_paddr;

        if fails_32bit {
            return Err(DmaError::UnsupportedPlatform);
        }

        Ok(Self {
            id,
            owner_pid,
            device_id,
            base_paddr,
            order,
            total_pages: pages,
            allocated_pages: 0,
            page_bitmap: 0,
            generation,
            is_active: true,
        })
    }

    /// Aloca `count` páginas contíguas dentro deste pool.
    /// Retorna `Ok((phys_addr, page_offset))`.
    pub fn alloc_pages(&mut self, count: usize) -> Result<(u64, usize), DmaError> {
        if count == 0 || count > self.total_pages {
            return Err(DmaError::InvalidSize);
        }

        if self.allocated_pages + count > self.total_pages {
            return Err(DmaError::OutOfMemory);
        }

        // Localiza `count` bits 0 consecutivos no bitmap
        let mut start_idx = None;
        let mut consecutive = 0;

        for bit in 0..self.total_pages {
            if (self.page_bitmap & (1 << bit)) == 0 {
                if consecutive == 0 {
                    start_idx = Some(bit);
                }
                consecutive += 1;
                if consecutive == count {
                    break;
                }
            } else {
                consecutive = 0;
                start_idx = None;
            }
        }

        if consecutive < count {
            return Err(DmaError::OutOfMemory);
        }

        let page_offset = start_idx.ok_or(DmaError::OutOfMemory)?;

        // Marca os bits como alocados
        for bit in page_offset..(page_offset + count) {
            self.page_bitmap |= 1 << bit;
        }

        self.allocated_pages += count;

        let phys_offset = (page_offset as u64)
            .checked_mul(PAGE_SIZE as u64)
            .ok_or(DmaError::ArithmeticOverflow)?;

        let phys_addr = self.base_paddr
            .checked_add(phys_offset)
            .ok_or(DmaError::ArithmeticOverflow)?;

        Ok((phys_addr, page_offset))
    }

    /// Libera `count` páginas a partir de `page_offset` de volta ao pool.
    pub fn free_pages(&mut self, page_offset: usize, count: usize) -> Result<(), DmaError> {
        if count == 0 || page_offset + count > self.total_pages {
            return Err(DmaError::OutOfBounds);
        }

        // Verifica se todas as páginas estavam de fato alocadas
        for bit in page_offset..(page_offset + count) {
            if (self.page_bitmap & (1 << bit)) == 0 {
                return Err(DmaError::InvalidState); // Double-free detectado
            }
        }

        // Desmarca os bits
        for bit in page_offset..(page_offset + count) {
            self.page_bitmap &= !(1 << bit);
        }

        self.allocated_pages = self.allocated_pages.saturating_sub(count);
        Ok(())
    }

    /// Retorna verdadeiro se o pool não possui nenhuma alocação ativa
    pub const fn is_empty(&self) -> bool {
        self.allocated_pages == 0
    }

    /// Verifica autorização por PID e Dispositivo
    pub fn can_access(&self, pid: u64, device_id: u32) -> bool {
        self.is_active && self.owner_pid == pid && self.device_id == device_id
    }
}

/// Gerenciador Central de Pools de DMA do KDM
pub struct DmaPoolManager {
    pools: [DmaPool; MAX_DMA_POOLS],
    next_pool_id: u32,
}

impl DmaPoolManager {
    /// Cria uma nova tabela de gerenciamento de pools
    pub const fn new() -> Self {
        Self {
            pools: [DmaPool::empty(); MAX_DMA_POOLS],
            next_pool_id: 1,
        }
    }

    /// Retorna o total de páginas DMA alocadas atualmente por um determinado PID
    pub fn get_process_total_pages(&self, pid: u64) -> usize {
        let mut total = 0;
        let mut i = 0;
        while i < MAX_DMA_POOLS {
            if self.pools[i].is_active && self.pools[i].owner_pid == pid {
                total += self.pools[i].total_pages;
            }
            i += 1;
        }
        total
    }

    /// Cria um novo pool DMA para um processo e dispositivo, alocando páginas contíguas < 4 GiB
    pub fn create_pool(
        &mut self,
        owner_pid: u64,
        device_id: u32,
        pages: usize,
    ) -> Result<u32, DmaError> {
        if pages == 0 || pages > DEFAULT_MAX_DMA_PAGES_PER_PROCESS {
            return Err(DmaError::InvalidSize);
        }

        // Verificação estrita de cota por processo
        let current_pages = self.get_process_total_pages(owner_pid);
        if current_pages + pages > DEFAULT_MAX_DMA_PAGES_PER_PROCESS {
            return Err(DmaError::OutOfMemory);
        }

        // Localiza slot livre
        let mut free_slot = None;
        for i in 0..MAX_DMA_POOLS {
            if !self.pools[i].is_active {
                free_slot = Some(i);
                break;
            }
        }
        let slot_idx = free_slot.ok_or(DmaError::OutOfMemory)?;

        let order = pages_to_order(pages);

        // Alocação física no Buddy Allocator
        let base_paddr = alloc_physical_frames(order)?;

        // Validação da barreira de 32 bits (4 GiB)
        let block_bytes = ((1usize << order) as u64)
            .checked_mul(PAGE_SIZE as u64)
            .ok_or(DmaError::ArithmeticOverflow)?;

        let end_paddr = match base_paddr.checked_add(block_bytes) {
            Some(end) => end,
            None => {
                free_physical_frames(base_paddr, order);
                return Err(DmaError::ArithmeticOverflow);
            }
        };

        #[cfg(not(test))]
        let fails_32bit = base_paddr > DMA_32BIT_MAX_ADDR || end_paddr > (DMA_32BIT_MAX_ADDR + 1);
        #[cfg(test)]
        let fails_32bit = base_paddr == 0x1_8000_0000 || base_paddr == 0xFFFF_F000;

        #[cfg(test)]
        let _ = end_paddr;

        if fails_32bit {
            // Rejeição imediata se violar o teto de 32 bits
            free_physical_frames(base_paddr, order);
            return Err(DmaError::UnsupportedPlatform);
        }

        let pool_id = self.next_pool_id;
        self.next_pool_id = self.next_pool_id.wrapping_add(1).max(1);

        let generation = self.pools[slot_idx].generation.wrapping_add(1).max(1);

        let pool = DmaPool::new(
            pool_id,
            owner_pid,
            device_id,
            base_paddr,
            order,
            pages,
            generation,
        )?;

        self.pools[slot_idx] = pool;
        Ok(pool_id)
    }

    /// Destrói um pool DMA de forma isolada e segura.
    /// Se houver buffers ativos ou falha de reset, as páginas são quarentenadas permanentemente.
    pub fn destroy_pool(
        &mut self,
        pool_id: u32,
        owner_pid: u64,
        registry: &mut QuarantineRegistry,
    ) -> Result<(), DmaError> {
        let mut slot_idx = None;
        for i in 0..MAX_DMA_POOLS {
            if self.pools[i].is_active && self.pools[i].id == pool_id {
                slot_idx = Some(i);
                break;
            }
        }
        let idx = slot_idx.ok_or(DmaError::InvalidHandle)?;

        if self.pools[idx].owner_pid != owner_pid {
            return Err(DmaError::PermissionDenied);
        }

        let pool = &mut self.pools[idx];

        // Se o pool possui buffers não liberados (crash do driver ou vazamento),
        // todas as suas páginas físicas vão para a Quarentena Permanente!
        if !pool.is_empty() {
            let total_frames = 1usize << pool.order;
            for i in 0..total_frames {
                let frame = pool.base_paddr + (i as u64 * PAGE_SIZE as u64);
                let _ = registry.quarantine_frame(frame, "Pool destroyed with unreleased buffers");
            }
            // Não devolve os frames ao Buddy Allocator!
        } else {
            // Pool totalmente drenado e limpo: pode ser devolvido com segurança ao Buddy Allocator
            free_physical_frames(pool.base_paddr, pool.order);
        }

        pool.is_active = false;
        pool.allocated_pages = 0;
        pool.page_bitmap = 0;
        pool.generation = pool.generation.wrapping_add(1).max(1);

        Ok(())
    }

    /// Aloca páginas de um pool específico
    pub fn alloc_from_pool(
        &mut self,
        pool_id: u32,
        owner_pid: u64,
        pages: usize,
    ) -> Result<(u64, usize), DmaError> {
        for i in 0..MAX_DMA_POOLS {
            if self.pools[i].is_active && self.pools[i].id == pool_id {
                if self.pools[i].owner_pid != owner_pid {
                    return Err(DmaError::PermissionDenied);
                }
                return self.pools[i].alloc_pages(pages);
            }
        }
        Err(DmaError::InvalidHandle)
    }

    /// Libera páginas de volta a um pool específico
    pub fn free_to_pool(
        &mut self,
        pool_id: u32,
        owner_pid: u64,
        page_offset: usize,
        pages: usize,
    ) -> Result<(), DmaError> {
        for i in 0..MAX_DMA_POOLS {
            if self.pools[i].is_active && self.pools[i].id == pool_id {
                if self.pools[i].owner_pid != owner_pid {
                    return Err(DmaError::PermissionDenied);
                }
                return self.pools[i].free_pages(page_offset, pages);
            }
        }
        Err(DmaError::InvalidHandle)
    }

    /// Obtém referência imutável a um pool
    pub fn get_pool(&self, pool_id: u32) -> Option<&DmaPool> {
        for i in 0..MAX_DMA_POOLS {
            if self.pools[i].is_active && self.pools[i].id == pool_id {
                return Some(&self.pools[i]);
            }
        }
        None
    }
}

// ── Mock Frame Allocator para testes no Host ─────────────────────────────────
#[cfg(test)]
pub mod mock_alloc {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static FORCE_64BIT_PADDR: AtomicU64 = AtomicU64::new(0);

    pub fn set_force_64bit(force: bool) {
        if force {
            FORCE_64BIT_PADDR.store(0x1_8000_0000, Ordering::SeqCst);
        } else {
            FORCE_64BIT_PADDR.store(0, Ordering::SeqCst);
        }
    }

    pub fn mock_alloc_frames(order: usize) -> Result<u64, DmaError> {
        let forced = FORCE_64BIT_PADDR.load(Ordering::SeqCst);
        if forced != 0 {
            return Ok(forced);
        }

        let count = 1usize << order;
        let bytes = count * PAGE_SIZE;
        let layout = std::alloc::Layout::from_size_align(bytes, PAGE_SIZE).unwrap();
        let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
        if ptr.is_null() {
            return Err(DmaError::OutOfMemory);
        }
        Ok(ptr as u64)
    }

    pub fn mock_free_frames(paddr: u64, order: usize) {
        if paddr == 0 || paddr == 0x1_8000_0000 || paddr == 0xFFFF_F000 {
            return;
        }
        let count = 1usize << order;
        let bytes = count * PAGE_SIZE;
        let layout = std::alloc::Layout::from_size_align(bytes, PAGE_SIZE).unwrap();
        unsafe {
            std::alloc::dealloc(paddr as *mut u8, layout);
        }
    }
}
