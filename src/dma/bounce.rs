//! Jinn OS — Kernel DMA Manager (KDM)
//! Mediador de Bounce Buffers (Backend B) e Prevenção de Race Conditions (Fase 2)

use super::types::{DmaDirection, DmaError};
use super::buffer::{DmaBuffer, DmaDescriptor};
use core::sync::atomic::{fence, Ordering};

/// Converte um endereço físico abaixo de 4 GiB em um ponteiro virtual acessível pelo Ring 0.
#[inline]
pub fn phys_to_virt(paddr: u64) -> *mut u8 {
    if paddr == 0 {
        return core::ptr::null_mut();
    }

    #[cfg(not(test))]
    {
        (crate::limine::hhdm_offset() + paddr) as *mut u8
    }
    #[cfg(test)]
    {
        paddr as *mut u8
    }
}

/// Valida se um ponteiro fornecido pelo espaço de usuário (Ring 3) é seguro para leitura ou escrita.
pub fn validate_user_memory(ptr: *const u8, len: usize, write_access: bool) -> Result<(), DmaError> {
    if ptr.is_null() || len == 0 {
        return Err(DmaError::InvalidSize);
    }

    let addr = ptr as usize;
    if addr.checked_add(len).is_none() {
        return Err(DmaError::ArithmeticOverflow);
    }

    #[cfg(not(test))]
    {
        let pid = crate::scheduler::current_pid();
        if let Some(Some(task)) = crate::scheduler::get().tasks.get(pid) {
            if task.ring == crate::scheduler::Ring::User {
                if crate::syscall::validate_user_buffer(ptr, len, write_access).is_err() {
                    return Err(DmaError::MappingFailed);
                }
            }
        }
    }

    #[cfg(test)]
    let _ = write_access; // Silencia aviso de unused em testes no host

    Ok(())
}

/// Mediador de Bounce Buffers (Backend B): Camada de isolamento estrito entre
/// memória do usuário (Ring 3) e o barramento de hardware físico.
pub struct BounceBufferMediator;

impl BounceBufferMediator {
    /// Prepara uma transmissão DMA (TX - DeviceReadsMemory) a partir de um buffer de usuário (Ring 3).
    ///
    /// Fluxo de Segurança:
    /// 1. Valida limites do descritor com proteção contra estouro aritmético (`checked_add`).
    /// 2. Valida se o ponteiro de Ring 3 é legível e está dentro dos limites de usuário.
    /// 3. Copia os dados do usuário para o buffer intermediário de Ring 0 (Bounce Buffer).
    /// 4. Aplica barreira de memória de barramento (`SeqCst fence`).
    /// 5. Transita o buffer para `InTransfer` e programa o hardware com o endereço físico do Bounce Buffer.
    ///
    /// Garantia Anti-TOCTOU: O hardware NUNCA lê do espaço de usuário; mutações posteriores do processo
    /// de usuário durante o DMA não afetam a transferência em voo.
    pub fn prepare_tx(
        buffer: &mut DmaBuffer,
        user_src: *const u8,
        desc: &DmaDescriptor,
    ) -> Result<u64, DmaError> {
        if buffer.direction != DmaDirection::DeviceReadsMemory
            && buffer.direction != DmaDirection::Bidirectional
        {
            return Err(DmaError::InvalidDirection);
        }

        if buffer.bounce_buffer_phys == 0 {
            return Err(DmaError::MappingFailed);
        }

        desc.validate_bounds(buffer.capacity)?;
        validate_user_memory(user_src, desc.length, false)?;

        let bounce_virt = phys_to_virt(buffer.bounce_buffer_phys);
        if bounce_virt.is_null() {
            return Err(DmaError::MappingFailed);
        }

        // Cópia segura de Ring 3 -> Bounce Buffer Ring 0
        unsafe {
            let dst_ptr = bounce_virt.add(desc.offset);
            core::ptr::copy_nonoverlapping(user_src, dst_ptr, desc.length);
        }

        // Barreira estrita para ordenação de barramento
        fence(Ordering::SeqCst);

        // Submissão na máquina de estados (Mapped -> Prepared -> InTransfer)
        buffer.submit_transfer(desc)?;

        let hw_paddr = buffer.bounce_buffer_phys
            .checked_add(desc.offset as u64)
            .ok_or(DmaError::ArithmeticOverflow)?;

        Ok(hw_paddr)
    }

    /// Prepara uma recepção DMA (RX - DeviceWritesMemory) no buffer intermediário de Ring 0.
    ///
    /// Fluxo de Segurança:
    /// 1. Valida limites do descritor.
    /// 2. Sanitiza o Bounce Buffer com zeros (evita vazamento de dados prévios do kernel).
    /// 3. Aplica barreira de memória.
    /// 4. Transita o buffer para `InTransfer` e retorna o endereço físico do Bounce Buffer para o hardware.
    pub fn prepare_rx(
        buffer: &mut DmaBuffer,
        desc: &DmaDescriptor,
    ) -> Result<u64, DmaError> {
        if buffer.direction != DmaDirection::DeviceWritesMemory
            && buffer.direction != DmaDirection::Bidirectional
        {
            return Err(DmaError::InvalidDirection);
        }

        if buffer.bounce_buffer_phys == 0 {
            return Err(DmaError::MappingFailed);
        }

        desc.validate_bounds(buffer.capacity)?;

        let bounce_virt = phys_to_virt(buffer.bounce_buffer_phys);
        if bounce_virt.is_null() {
            return Err(DmaError::MappingFailed);
        }

        // Sanitização: limpa o buffer com zeros antes de submeter ao HW
        unsafe {
            let dst_ptr = bounce_virt.add(desc.offset);
            core::ptr::write_bytes(dst_ptr, 0, desc.length);
        }

        fence(Ordering::SeqCst);

        buffer.submit_transfer(desc)?;

        let hw_paddr = buffer.bounce_buffer_phys
            .checked_add(desc.offset as u64)
            .ok_or(DmaError::ArithmeticOverflow)?;

        Ok(hw_paddr)
    }

    /// Conclui uma recepção DMA (RX) sincronizando os dados recebidos pelo hardware com o Ring 3.
    ///
    /// Fluxo de Segurança:
    /// 1. Verifica se a transferência de hardware foi ativamente concluída (`InTransfer -> Completed`).
    /// 2. Valida o buffer de destino do usuário para escrita (`validate_user_memory`).
    /// 3. Aplica barreira de memória para garantir que todos os bytes gravados pelo DMA estão visíveis.
    /// 4. Copia os dados do Bounce Buffer de Ring 0 para o espaço de usuário em Ring 3.
    /// 5. Retorna o buffer para o estado `Mapped` (pronto para novos ciclos).
    pub fn complete_rx(
        buffer: &mut DmaBuffer,
        user_dst: *mut u8,
        desc: &DmaDescriptor,
    ) -> Result<usize, DmaError> {
        if buffer.direction != DmaDirection::DeviceWritesMemory
            && buffer.direction != DmaDirection::Bidirectional
        {
            return Err(DmaError::InvalidDirection);
        }

        desc.validate_bounds(buffer.capacity)?;
        validate_user_memory(user_dst, desc.length, true)?;

        let bounce_virt = phys_to_virt(buffer.bounce_buffer_phys);
        if bounce_virt.is_null() {
            return Err(DmaError::MappingFailed);
        }

        // Barreira garantindo visibilidade pós-DMA
        fence(Ordering::SeqCst);

        // Cópia segura: Bounce Buffer Ring 0 -> Buffer de Ring 3
        unsafe {
            let src_ptr = bounce_virt.add(desc.offset);
            core::ptr::copy_nonoverlapping(src_ptr, user_dst, desc.length);
        }

        // Transição: InTransfer -> Completed
        buffer.complete_transfer()?;

        // Ciclo normal: Completed -> Mapped
        buffer.state.transition_to(super::state::DmaBufferState::Mapped)?;

        Ok(desc.length)
    }

    /// Helper para transmissão (TX) a partir de uma fatia segura de memória interna do kernel.
    /// Utilizado por subsistemas internos, testes do kernel e selftests.
    pub fn prepare_tx_slice(
        buffer: &mut DmaBuffer,
        src: &[u8],
        offset: usize,
    ) -> Result<u64, DmaError> {
        let desc = DmaDescriptor::new(offset, src.len());
        desc.validate_bounds(buffer.capacity)?;

        if buffer.direction != DmaDirection::DeviceReadsMemory
            && buffer.direction != DmaDirection::Bidirectional
        {
            return Err(DmaError::InvalidDirection);
        }

        let bounce_virt = phys_to_virt(buffer.bounce_buffer_phys);
        if bounce_virt.is_null() {
            return Err(DmaError::MappingFailed);
        }

        unsafe {
            let dst_ptr = bounce_virt.add(offset);
            core::ptr::copy_nonoverlapping(src.as_ptr(), dst_ptr, src.len());
        }

        fence(Ordering::SeqCst);
        buffer.submit_transfer(&desc)?;

        let hw_paddr = buffer.bounce_buffer_phys
            .checked_add(offset as u64)
            .ok_or(DmaError::ArithmeticOverflow)?;

        Ok(hw_paddr)
    }

    /// Helper para recepção (RX) preenchendo uma fatia de memória interna do kernel.
    /// Utilizado por subsistemas internos, testes do kernel e selftests.
    pub fn complete_rx_slice(
        buffer: &mut DmaBuffer,
        dst: &mut [u8],
        offset: usize,
    ) -> Result<usize, DmaError> {
        let desc = DmaDescriptor::new(offset, dst.len());
        desc.validate_bounds(buffer.capacity)?;

        let bounce_virt = phys_to_virt(buffer.bounce_buffer_phys);
        if bounce_virt.is_null() {
            return Err(DmaError::MappingFailed);
        }

        fence(Ordering::SeqCst);

        unsafe {
            let src_ptr = bounce_virt.add(offset);
            core::ptr::copy_nonoverlapping(src_ptr, dst.as_mut_ptr(), dst.len());
        }

        buffer.complete_transfer()?;
        buffer.state.transition_to(super::state::DmaBufferState::Mapped)?;

        Ok(dst.len())
    }
}

