//! Jinn OS — Kernel DMA Manager (KDM)
//! Gerenciador Central de Buffers DMA e Registrador de Slots (Fase 3)

use super::types::{DmaDirection, DmaError, MAX_DMA_BUFFERS, PAGE_SIZE};
use super::buffer::DmaBuffer;
use super::state::DmaBufferState;
use super::token::DmaToken;
use super::pool::DmaPoolManager;

/// Entrada de metadados de controle de um buffer DMA alocado
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmaBufferEntry {
    /// Flag indicando se este slot está em uso ativo
    pub is_active: bool,
    /// Identificador do DmaPool de onde o buffer foi alocado
    pub pool_id: u32,
    /// Deslocamento em páginas físicas dentro do DmaPool
    pub page_offset: usize,
    /// Quantidade de páginas contíguas alocadas
    pub page_count: usize,
    /// Endereço virtual mapeado no espaço do processo de usuário (0 se não mapeado)
    pub user_vaddr: u64,
    /// Metadados formais do buffer e máquina de estados KDM
    pub buffer: DmaBuffer,
}

impl DmaBufferEntry {
    /// Cria uma entrada vazia e inativa
    pub const fn empty() -> Self {
        Self {
            is_active: false,
            pool_id: 0,
            page_offset: 0,
            page_count: 0,
            user_vaddr: 0,
            buffer: DmaBuffer::empty(),
        }
    }
}

/// Registro Central de Buffers DMA do Kernel
pub struct DmaBufferRegistry {
    entries: [DmaBufferEntry; MAX_DMA_BUFFERS],
}

impl DmaBufferRegistry {
    /// Cria uma nova tabela de registro vazia
    pub const fn new() -> Self {
        Self {
            entries: [DmaBufferEntry::empty(); MAX_DMA_BUFFERS],
        }
    }

    /// Aloca um slot de buffer no registro e consome páginas do DmaPool especificado.
    /// Retorna um handle opaco de 64 bits codificado com `(slot_id, generation)`.
    pub fn alloc_buffer(
        &mut self,
        pool_manager: &mut DmaPoolManager,
        caller_pid: u64,
        pool_id: u32,
        pages: usize,
        direction: DmaDirection,
    ) -> Result<u64, DmaError> {
        if pages == 0 {
            return Err(DmaError::InvalidSize);
        }

        // Localiza slot livre no registro
        let mut free_slot = None;
        for i in 0..MAX_DMA_BUFFERS {
            if !self.entries[i].is_active {
                free_slot = Some(i);
                break;
            }
        }
        let slot_idx = free_slot.ok_or(DmaError::OutOfMemory)?;

        // Aloca páginas contíguas dentro do pool do processo
        let (phys_addr, page_offset) = pool_manager.alloc_from_pool(pool_id, caller_pid, pages)?;

        let generation = self.entries[slot_idx].buffer.generation.wrapping_add(1).max(1);

        let mut buf = DmaBuffer::empty();
        buf.id = slot_idx as u64;
        buf.generation = generation;
        buf.owner_pid = caller_pid;
        buf.device_id = 0;
        buf.capacity = match pages.checked_mul(PAGE_SIZE) {
            Some(cap) => cap,
            None => {
                let _ = pool_manager.free_to_pool(pool_id, caller_pid, page_offset, pages);
                return Err(DmaError::ArithmeticOverflow);
            }
        };
        buf.alignment = PAGE_SIZE;
        buf.direction = direction;
        buf.backend = super::types::DmaBackend::KernelBounce;
        buf.state = DmaBufferState::Allocated;
        buf.phys_addr = phys_addr;
        buf.bounce_buffer_phys = phys_addr;
        buf.active_transfers = 0;
        buf.user_vaddr = 0;

        self.entries[slot_idx] = DmaBufferEntry {
            is_active: true,
            pool_id,
            page_offset,
            page_count: pages,
            user_vaddr: 0,
            buffer: buf,
        };

        let handle = DmaToken::encode_raw_handle(slot_idx as u32, generation as u32);
        Ok(handle)
    }

    /// Obtém referência imutável a um slot, validando o handle e autorização por PID.
    pub fn get_entry(&self, handle: u64, caller_pid: u64) -> Result<&DmaBufferEntry, DmaError> {
        let (slot_id, gen) = DmaToken::decode_handle(handle);
        let idx = slot_id as usize;
        if idx >= MAX_DMA_BUFFERS {
            return Err(DmaError::InvalidHandle);
        }
        let entry = &self.entries[idx];
        if !entry.is_active || entry.buffer.generation as u32 != gen {
            return Err(DmaError::InvalidHandle);
        }
        if entry.buffer.owner_pid != caller_pid {
            return Err(DmaError::PermissionDenied);
        }
        Ok(entry)
    }

    /// Obtém referência mutável a um slot, validando o handle e autorização por PID.
    pub fn get_entry_mut(&mut self, handle: u64, caller_pid: u64) -> Result<&mut DmaBufferEntry, DmaError> {
        let (slot_id, gen) = DmaToken::decode_handle(handle);
        let idx = slot_id as usize;
        if idx >= MAX_DMA_BUFFERS {
            return Err(DmaError::InvalidHandle);
        }
        let entry = &mut self.entries[idx];
        if !entry.is_active || entry.buffer.generation as u32 != gen {
            return Err(DmaError::InvalidHandle);
        }
        if entry.buffer.owner_pid != caller_pid {
            return Err(DmaError::PermissionDenied);
        }
        Ok(entry)
    }

    /// Libera voluntariamente um buffer, devolvendo suas páginas ao DmaPool.
    pub fn release_buffer(
        &mut self,
        pool_manager: &mut DmaPoolManager,
        handle: u64,
        caller_pid: u64,
    ) -> Result<(), DmaError> {
        let (slot_id, gen) = DmaToken::decode_handle(handle);
        let idx = slot_id as usize;
        if idx >= MAX_DMA_BUFFERS {
            return Err(DmaError::InvalidHandle);
        }
        let entry = &mut self.entries[idx];
        if !entry.is_active || entry.buffer.generation as u32 != gen {
            return Err(DmaError::InvalidHandle);
        }
        if entry.buffer.owner_pid != caller_pid {
            return Err(DmaError::PermissionDenied);
        }

        if entry.buffer.state == DmaBufferState::Quarantined {
            return Err(DmaError::QuarantinedMemory);
        }

        if entry.buffer.state == DmaBufferState::InTransfer {
            return Err(DmaError::DeviceBusy);
        }

        if entry.buffer.state != DmaBufferState::Allocated && entry.buffer.state != DmaBufferState::Mapped {
            return Err(DmaError::InvalidState);
        }

        // Devolve páginas físicas ao DmaPool
        pool_manager.free_to_pool(entry.pool_id, caller_pid, entry.page_offset, entry.page_count)?;

        entry.buffer.state = DmaBufferState::Released;
        entry.is_active = false;
        entry.buffer.generation = entry.buffer.generation.wrapping_add(1).max(1);
        Ok(())
    }

    /// Limpa ou revoga todos os buffers associados a um determinado DmaPool
    pub fn cleanup_pool_buffers(&mut self, pool_id: u32, caller_pid: u64) {
        for entry in self.entries.iter_mut() {
            if entry.is_active && entry.pool_id == pool_id && entry.buffer.owner_pid == caller_pid {
                entry.is_active = false;
                entry.buffer.state = DmaBufferState::Released;
                entry.buffer.generation = entry.buffer.generation.wrapping_add(1).max(1);
            }
        }
    }

    /// Retorna a contagem de buffers atualmente ativos
    pub fn active_count(&self) -> usize {
        let mut count = 0;
        for entry in self.entries.iter() {
            if entry.is_active {
                count += 1;
            }
        }
        count
    }
}
