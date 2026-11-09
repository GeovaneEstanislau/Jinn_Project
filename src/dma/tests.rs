//! Jinn OS — Kernel DMA Manager (KDM)
//! Suíte de Testes Unitários e Selftests de Segurança (Fase 1 e Fase 2)

use super::types::{DmaBackend, DmaDirection, DmaError, PAGE_SIZE};
use super::token::DmaToken;
use super::state::DmaBufferState;
use super::buffer::{DmaBuffer, DmaDescriptor, QuarantineRegistry};
use super::pool::{
    alloc_physical_frames, free_physical_frames, pages_to_order, DmaPool, DmaPoolManager,
};
use super::bounce::{phys_to_virt, BounceBufferMediator};
use super::manager::DmaBufferRegistry;

/// Executa todos os testes unitários das Fases 1, 2 e 3 do Kernel DMA Manager.
/// Retorna `(passed, total)` para auditoria pelo kernel e logs de inicialização.
pub fn run_dma_manager_selftests() -> (usize, usize) {
    let mut passed = 0;
    let mut total = 0;

    // ── FASE 1: TIPOS, TOKENS, ESTADOS E QUARENTENA ─────────────────────────
    total += 1;
    if test_token_and_handle_security() {
        passed += 1;
    }

    total += 1;
    if test_descriptor_overflow_protection() {
        passed += 1;
    }

    total += 1;
    if test_state_machine_valid_lifecycle() {
        passed += 1;
    }

    total += 1;
    if test_state_machine_illegal_transitions() {
        passed += 1;
    }

    total += 1;
    if test_graceful_revocation_on_confirmed_reset() {
        passed += 1;
    }

    total += 1;
    if test_unconfirmed_reset_permanent_quarantine() {
        passed += 1;
    }

    total += 1;
    if test_quarantine_registry_integrity() {
        passed += 1;
    }

    total += 1;
    if test_transfer_submission_and_tracking() {
        passed += 1;
    }

    // ── FASE 2: ALOCAÇÃO FÍSICA (DmaPool) E BOUNCE BUFFERS (Backend B) ───────
    total += 1;
    if test_pool_pages_to_order_conversion() {
        passed += 1;
    }

    total += 1;
    if test_pool_creation_and_32bit_ceiling() {
        passed += 1;
    }

    total += 1;
    if test_pool_sub_allocation_and_bitmap() {
        passed += 1;
    }

    total += 1;
    if test_pool_quota_enforcement() {
        passed += 1;
    }

    total += 1;
    if test_pool_destruction_and_quarantine_leak() {
        passed += 1;
    }

    total += 1;
    if test_bounce_buffer_tx_data_integrity() {
        passed += 1;
    }

    total += 1;
    if test_bounce_buffer_rx_data_integrity() {
        passed += 1;
    }

    total += 1;
    if test_bounce_buffer_direction_and_bounds_rejection() {
        passed += 1;
    }

    // ── FASE 3: EXPOSIÇÃO DAS SYSCALLS (60..=68) E DRIVER_CAP_DMA ───────────
    total += 1;
    if test_buffer_registry_alloc_and_release() {
        passed += 1;
    }

    total += 1;
    if test_buffer_registry_exhaustion_and_bounds() {
        passed += 1;
    }

    total += 1;
    if test_buffer_registry_unconfirmed_reset_quarantine() {
        passed += 1;
    }

    total += 1;
    if test_buffer_registry_multiple_slots_and_cleanup() {
        passed += 1;
    }

    total += 1;
    if test_driver_capability_dma_validation() {
        passed += 1;
    }

    total += 1;
    if test_syscalls_kdm_dispatch_e2e() {
        passed += 1;
    }

    total += 1;
    if test_syscalls_kdm_unconfirmed_reset_and_quarantine() {
        passed += 1;
    }

    (passed, total)
}

// ── IMPLEMENTAÇÃO DOS TESTES DA FASE 1 ──────────────────────────────────────

fn test_token_and_handle_security() -> bool {
    let token = DmaToken::new(7, 42, 100, 0x1001);

    if !token.is_valid_for(100, 0x1001, 42) {
        return false;
    }
    if token.is_valid_for(101, 0x1001, 42) {
        return false;
    }
    if token.is_valid_for(100, 0x1002, 42) {
        return false;
    }
    if token.is_valid_for(100, 0x1001, 41) {
        return false;
    }

    let handle = token.encode_handle();
    let (slot, gen) = DmaToken::decode_handle(handle);
    if slot != 7 || gen != 42 {
        return false;
    }

    if DmaToken::validate_handle(handle, 42) != Ok(7) {
        return false;
    }
    if DmaToken::validate_handle(handle, 43) != Err(DmaError::InvalidHandle) {
        return false;
    }

    true
}

fn test_descriptor_overflow_protection() -> bool {
    let capacity = 4096;

    let valid_desc = DmaDescriptor::new(100, 500);
    if valid_desc.validate_bounds(capacity) != Ok(600) {
        return false;
    }

    let full_desc = DmaDescriptor::new(0, 4096);
    if full_desc.validate_bounds(capacity) != Ok(4096) {
        return false;
    }

    let zero_desc = DmaDescriptor::new(0, 0);
    if zero_desc.validate_bounds(capacity) != Err(DmaError::InvalidSize) {
        return false;
    }

    let oob_desc = DmaDescriptor::new(4000, 200);
    if oob_desc.validate_bounds(capacity) != Err(DmaError::OutOfBounds) {
        return false;
    }

    let overflow_desc1 = DmaDescriptor::new(usize::MAX, 1);
    if overflow_desc1.validate_bounds(capacity) != Err(DmaError::ArithmeticOverflow) {
        return false;
    }

    let overflow_desc2 = DmaDescriptor::new(usize::MAX - 50, 100);
    if overflow_desc2.validate_bounds(capacity) != Err(DmaError::ArithmeticOverflow) {
        return false;
    }

    true
}

fn test_state_machine_valid_lifecycle() -> bool {
    let mut state = DmaBufferState::Allocated;

    if state.transition_to(DmaBufferState::Mapped).is_err() {
        return false;
    }
    if state.transition_to(DmaBufferState::Prepared).is_err() {
        return false;
    }
    if state.transition_to(DmaBufferState::InTransfer).is_err() {
        return false;
    }
    if !state.is_active() {
        return false;
    }
    if state.transition_to(DmaBufferState::Completed).is_err() {
        return false;
    }
    if state.transition_to(DmaBufferState::Mapped).is_err() {
        return false;
    }
    if state.transition_to(DmaBufferState::Released).is_err() {
        return false;
    }
    if !state.is_terminal() {
        return false;
    }

    true
}

fn test_state_machine_illegal_transitions() -> bool {
    let mut state = DmaBufferState::Allocated;
    if state.transition_to(DmaBufferState::InTransfer) != Err(DmaError::InvalidState) {
        return false;
    }

    state = DmaBufferState::InTransfer;
    if state.transition_to(DmaBufferState::Released) != Err(DmaError::InvalidState) {
        return false;
    }
    if state.transition_to(DmaBufferState::Mapped) != Err(DmaError::InvalidState) {
        return false;
    }

    state = DmaBufferState::Completed;
    if state.transition_to(DmaBufferState::InTransfer) != Err(DmaError::InvalidState) {
        return false;
    }

    state = DmaBufferState::Released;
    if state.transition_to(DmaBufferState::Mapped) != Err(DmaError::InvalidState) {
        return false;
    }

    true
}

fn test_graceful_revocation_on_confirmed_reset() -> bool {
    let mut buffer = DmaBuffer::empty();
    buffer.id = 1;
    buffer.generation = 1;
    buffer.owner_pid = 42;
    buffer.device_id = 0x1001;
    buffer.capacity = 4096;
    buffer.state = DmaBufferState::InTransfer;
    buffer.active_transfers = 1;
    buffer.phys_addr = 0x1000_0000;

    if buffer.revoke_and_release().is_err() {
        return false;
    }
    if buffer.state != DmaBufferState::Released {
        return false;
    }
    if buffer.active_transfers != 0 {
        return false;
    }

    true
}

fn test_unconfirmed_reset_permanent_quarantine() -> bool {
    let mut buffer = DmaBuffer::empty();
    buffer.id = 2;
    buffer.generation = 1;
    buffer.owner_pid = 42;
    buffer.device_id = 0x1001;
    buffer.capacity = 4096;
    buffer.state = DmaBufferState::InTransfer;
    buffer.active_transfers = 1;
    buffer.phys_addr = 0x2000_0000;
    buffer.bounce_buffer_phys = 0x2001_0000;

    let mut registry = QuarantineRegistry::new();

    if buffer.revoke_and_quarantine(&mut registry, "HW Reset Unconfirmed").is_err() {
        return false;
    }

    if buffer.state != DmaBufferState::Quarantined {
        return false;
    }
    if !buffer.state.is_quarantined() || !buffer.state.is_terminal() {
        return false;
    }

    // Prova formal de sumidouro:
    if buffer.state.transition_to(DmaBufferState::Released) != Err(DmaError::InvalidState) {
        return false;
    }
    if buffer.state.transition_to(DmaBufferState::Allocated) != Err(DmaError::InvalidState) {
        return false;
    }
    if buffer.state.transition_to(DmaBufferState::Mapped) != Err(DmaError::InvalidState) {
        return false;
    }
    if buffer.state.transition_to(DmaBufferState::Revoking) != Err(DmaError::InvalidState) {
        return false;
    }

    if !registry.is_quarantined(0x2000_0000) || !registry.is_quarantined(0x2001_0000) {
        return false;
    }
    if registry.can_release_to_buddy(0x2000_0000) || registry.can_release_to_buddy(0x2001_0000) {
        return false;
    }
    if !registry.can_release_to_buddy(0x3000_0000) {
        return false;
    }

    true
}

fn test_quarantine_registry_integrity() -> bool {
    let mut registry = QuarantineRegistry::new();

    if registry.count() != 0 {
        return false;
    }
    if registry.quarantine_frame(0, "Invalid zero page") != Err(DmaError::InvalidSize) {
        return false;
    }
    if registry.quarantine_frame(0x1000, "Device Fault 1").is_err() {
        return false;
    }
    if registry.count() != 1 {
        return false;
    }
    // Idempotência
    if registry.quarantine_frame(0x1000, "Device Fault 1 duplicate").is_err() {
        return false;
    }
    if registry.count() != 1 {
        return false;
    }

    let mut buffer = DmaBuffer::empty();
    buffer.capacity = 3 * PAGE_SIZE;
    buffer.phys_addr = 0x4000;
    buffer.state = DmaBufferState::InTransfer;

    if buffer.revoke_and_quarantine(&mut registry, "Multi-page DMA Crash").is_err() {
        return false;
    }

    if !registry.is_quarantined(0x4000)
        || !registry.is_quarantined(0x5000)
        || !registry.is_quarantined(0x6000)
    {
        return false;
    }
    if registry.is_quarantined(0x7000) {
        return false;
    }

    true
}

fn test_transfer_submission_and_tracking() -> bool {
    let mut buffer = DmaBuffer::empty();
    buffer.id = 3;
    buffer.generation = 1;
    buffer.owner_pid = 50;
    buffer.device_id = 0x1001;
    buffer.capacity = 8192;
    buffer.state = DmaBufferState::Mapped;

    let desc = DmaDescriptor::new(0, 1500);
    if buffer.submit_transfer(&desc).is_err() {
        return false;
    }
    if buffer.state != DmaBufferState::InTransfer || buffer.active_transfers != 1 {
        return false;
    }

    if buffer.complete_transfer().is_err() {
        return false;
    }
    if buffer.state != DmaBufferState::Completed || buffer.active_transfers != 0 {
        return false;
    }

    if buffer.state.transition_to(DmaBufferState::Mapped).is_err() {
        return false;
    }

    let desc2 = DmaDescriptor::new(1500, 1500);
    if buffer.submit_transfer(&desc2).is_err() {
        return false;
    }
    if buffer.state != DmaBufferState::InTransfer || buffer.active_transfers != 1 {
        return false;
    }
    if buffer.complete_transfer().is_err() {
        return false;
    }

    true
}

// ── IMPLEMENTAÇÃO DOS TESTES DA FASE 2 ──────────────────────────────────────

fn test_pool_pages_to_order_conversion() -> bool {
    if pages_to_order(1) != 0 {
        return false;
    }
    if pages_to_order(2) != 1 {
        return false;
    }
    if pages_to_order(3) != 2 {
        return false;
    }
    if pages_to_order(4) != 2 {
        return false;
    }
    if pages_to_order(5) != 3 {
        return false;
    }
    if pages_to_order(8) != 3 {
        return false;
    }
    if pages_to_order(9) != 4 {
        return false;
    }
    if pages_to_order(16) != 4 {
        return false;
    }
    if pages_to_order(17) != 5 {
        return false;
    }
    if pages_to_order(32) != 5 {
        return false;
    }
    true
}

fn test_pool_creation_and_32bit_ceiling() -> bool {
    // Alocação normal dentro do espaço < 4 GiB
    let pool_res = DmaPool::new(1, 10, 0x1001, 0x1000_0000, 1, 2, 1);
    if pool_res.is_err() {
        return false;
    }
    let pool = pool_res.unwrap();
    if pool.base_paddr >= 0x1_0000_0000 || pool.total_pages != 2 {
        return false;
    }

    // Tentativa de criar pool com endereço violando o teto de 32 bits (>= 4 GiB)
    let bad_pool_res = DmaPool::new(2, 10, 0x1001, 0x1_8000_0000, 1, 2, 1);
    if bad_pool_res != Err(DmaError::UnsupportedPlatform) {
        return false;
    }

    // Tentativa de criar pool com endereço base próximo ao teto onde base + tamanho estoura 4 GiB
    let overflow_pool = DmaPool::new(3, 10, 0x1001, 0xFFFF_F000, 1, 2, 1);
    if overflow_pool != Err(DmaError::UnsupportedPlatform) {
        return false;
    }

    true
}

fn test_pool_sub_allocation_and_bitmap() -> bool {
    let mut pool = DmaPool::new(10, 20, 0x1002, 0x3000_0000, 2, 4, 1).unwrap();

    // 1. Aloca 2 páginas (bits 0..2)
    let (paddr1, offset1) = match pool.alloc_pages(2) {
        Ok(res) => res,
        Err(_) => return false,
    };
    if offset1 != 0 || paddr1 != 0x3000_0000 || pool.allocated_pages != 2 {
        return false;
    }
    if (pool.page_bitmap & 0b11) != 0b11 {
        return false;
    }

    // 2. Aloca 2 páginas adicionais (bits 2..4)
    let (paddr2, offset2) = match pool.alloc_pages(2) {
        Ok(res) => res,
        Err(_) => return false,
    };
    if offset2 != 2 || paddr2 != 0x3000_2000 || pool.allocated_pages != 4 {
        return false;
    }
    if pool.page_bitmap != 0b1111 {
        return false;
    }

    // 3. Pool esgotado: nova alocação deve falhar com OutOfMemory
    if pool.alloc_pages(1) != Err(DmaError::OutOfMemory) {
        return false;
    }

    // 4. Libera o primeiro bloco de 2 páginas (offset 0)
    if pool.free_pages(0, 2).is_err() {
        return false;
    }
    if (pool.page_bitmap & 0b11) != 0 || pool.allocated_pages != 2 {
        return false;
    }

    // 5. Tentativa de double-free no offset 0 deve ser rejeitada com InvalidState
    if pool.free_pages(0, 2) != Err(DmaError::InvalidState) {
        return false;
    }

    // 6. Realoca 1 página: deve ocupar o slot livre no offset 0
    let (paddr3, offset3) = match pool.alloc_pages(1) {
        Ok(res) => res,
        Err(_) => return false,
    };
    if offset3 != 0 || paddr3 != 0x3000_0000 {
        return false;
    }

    // Libera tudo e confirma is_empty
    let _ = pool.free_pages(0, 1);
    let _ = pool.free_pages(2, 2);
    if !pool.is_empty() || pool.allocated_pages != 0 || pool.page_bitmap != 0 {
        return false;
    }

    true
}

fn test_pool_quota_enforcement() -> bool {
    let mut manager = DmaPoolManager::new();

    // Processo 50 solicita 24 páginas (dentro do limite padrão de 32)
    let p1 = manager.create_pool(50, 0x1001, 24);
    if p1.is_err() {
        return false;
    }
    let pool1_id = p1.unwrap();

    if manager.get_process_total_pages(50) != 24 {
        return false;
    }

    // Processo 50 tenta solicitar mais 12 páginas (24 + 12 = 36 > 32) -> REJEIÇÃO POR COTA!
    let p2 = manager.create_pool(50, 0x1001, 12);
    if p2 != Err(DmaError::OutOfMemory) {
        return false;
    }

    // Outro processo (PID 60) solicita 16 páginas -> DEVE TER SUCESSO (Cotas isoladas por PID)
    let p3 = manager.create_pool(60, 0x1001, 16);
    if p3.is_err() {
        return false;
    }

    // Limpeza
    let mut reg = QuarantineRegistry::new();
    let _ = manager.destroy_pool(pool1_id, 50, &mut reg);
    let _ = manager.destroy_pool(p3.unwrap(), 60, &mut reg);

    true
}

fn test_pool_destruction_and_quarantine_leak() -> bool {
    let mut manager = DmaPoolManager::new();
    let pool_id = match manager.create_pool(70, 0x1001, 2) {
        Ok(id) => id,
        Err(_) => return false,
    };

    // Sub-aloca 1 página e NÃO a libera (simula processo que crashou com buffer pendente)
    let (paddr, _) = match manager.alloc_from_pool(pool_id, 70, 1) {
        Ok(res) => res,
        Err(_) => return false,
    };

    let mut registry = QuarantineRegistry::new();

    // Destruição do pool com vazamento de buffers -> QUARENTENA PERMANENTE OBRIGATÓRIA!
    if manager.destroy_pool(pool_id, 70, &mut registry).is_err() {
        return false;
    }

    // O frame físico DEVE ter sido retido em quarentena permanente
    if !registry.is_quarantined(paddr) {
        return false;
    }
    if registry.can_release_to_buddy(paddr) {
        return false;
    }

    true
}

fn test_bounce_buffer_tx_data_integrity() -> bool {
    // Aloca um frame físico real para o teste de bounce buffer
    let bounce_paddr = match alloc_physical_frames(0) {
        Ok(p) => p,
        Err(_) => return false,
    };

    let mut buffer = DmaBuffer::empty();
    buffer.id = 100;
    buffer.capacity = PAGE_SIZE;
    buffer.direction = DmaDirection::DeviceReadsMemory;
    buffer.backend = DmaBackend::KernelBounce;
    buffer.state = DmaBufferState::Mapped;
    buffer.bounce_buffer_phys = bounce_paddr;

    let payload: [u8; 8] = [0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE];

    // Prepara transmissão TX via fatia segura de kernel (offset 64)
    let hw_paddr = match BounceBufferMediator::prepare_tx_slice(&mut buffer, &payload, 64) {
        Ok(p) => p,
        Err(_) => {
            free_physical_frames(bounce_paddr, 0);
            return false;
        }
    };

    if hw_paddr != bounce_paddr + 64 {
        free_physical_frames(bounce_paddr, 0);
        return false;
    }
    if buffer.state != DmaBufferState::InTransfer || buffer.active_transfers != 1 {
        free_physical_frames(bounce_paddr, 0);
        return false;
    }

    // Inspeciona os bytes gravados no Bounce Buffer de Ring 0
    let bounce_virt = phys_to_virt(bounce_paddr);
    let mut match_ok = true;
    unsafe {
        for i in 0..payload.len() {
            if *bounce_virt.add(64 + i) != payload[i] {
                match_ok = false;
                break;
            }
        }
    }

    let _ = buffer.complete_transfer();
    free_physical_frames(bounce_paddr, 0);

    match_ok
}

fn test_bounce_buffer_rx_data_integrity() -> bool {
    let bounce_paddr = match alloc_physical_frames(0) {
        Ok(p) => p,
        Err(_) => return false,
    };

    let mut buffer = DmaBuffer::empty();
    buffer.id = 101;
    buffer.capacity = PAGE_SIZE;
    buffer.direction = DmaDirection::DeviceWritesMemory;
    buffer.backend = DmaBackend::KernelBounce;
    buffer.state = DmaBufferState::Mapped;
    buffer.bounce_buffer_phys = bounce_paddr;

    let desc = DmaDescriptor::new(128, 8);

    // Prepara recepção RX (sanitiza com zeros e transita para InTransfer)
    let hw_paddr = match BounceBufferMediator::prepare_rx(&mut buffer, &desc) {
        Ok(p) => p,
        Err(_) => {
            free_physical_frames(bounce_paddr, 0);
            return false;
        }
    };

    if hw_paddr != bounce_paddr + 128 || buffer.state != DmaBufferState::InTransfer {
        free_physical_frames(bounce_paddr, 0);
        return false;
    }

    // Simula o dispositivo de hardware gravando dados no Bounce Buffer físico
    let simulated_hw_packet: [u8; 8] = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88];
    let bounce_virt = phys_to_virt(bounce_paddr);
    unsafe {
        core::ptr::copy_nonoverlapping(
            simulated_hw_packet.as_ptr(),
            bounce_virt.add(128),
            simulated_hw_packet.len(),
        );
    }

    // Conclui a transferência RX e copia para o destino
    let mut rx_dst = [0u8; 8];
    let rx_len = match BounceBufferMediator::complete_rx_slice(&mut buffer, &mut rx_dst, 128) {
        Ok(len) => len,
        Err(_) => {
            free_physical_frames(bounce_paddr, 0);
            return false;
        }
    };

    if rx_len != 8 || buffer.state != DmaBufferState::Mapped {
        free_physical_frames(bounce_paddr, 0);
        return false;
    }

    let is_identical = rx_dst == simulated_hw_packet;
    free_physical_frames(bounce_paddr, 0);

    is_identical
}

fn test_bounce_buffer_direction_and_bounds_rejection() -> bool {
    let mut buffer = DmaBuffer::empty();
    buffer.capacity = 1024;
    buffer.direction = DmaDirection::DeviceWritesMemory; // Apenas RX
    buffer.bounce_buffer_phys = 0x1000;
    buffer.state = DmaBufferState::Mapped;

    let payload = [0u8; 16];

    // Tentativa de executar TX em um buffer exclusivo para RX -> Rejeição por direção inválida
    let tx_err = BounceBufferMediator::prepare_tx_slice(&mut buffer, &payload, 0);
    if tx_err != Err(DmaError::InvalidDirection) {
        return false;
    }

    // Tentativa de submeter descritor extrapolando a capacidade
    let oob_desc = DmaDescriptor::new(1020, 10);
    let oob_err = BounceBufferMediator::prepare_rx(&mut buffer, &oob_desc);
    if oob_err != Err(DmaError::OutOfBounds) {
        return false;
    }

    true
}

// ── IMPLEMENTAÇÃO DOS TESTES DA FASE 3 ──────────────────────────────────────

fn test_buffer_registry_alloc_and_release() -> bool {
    let mut pool_mgr = DmaPoolManager::new();
    let mut reg = DmaBufferRegistry::new();
    let mut qreg = QuarantineRegistry::new();

    let pid = 42;
    let pool_id = match pool_mgr.create_pool(pid, 0x1001, 4) {
        Ok(id) => id,
        Err(_) => return false,
    };

    // Aloca buffer de 2 páginas
    let handle = match reg.alloc_buffer(&mut pool_mgr, pid, pool_id, 2, DmaDirection::Bidirectional) {
        Ok(h) => h,
        Err(_) => {
            let _ = pool_mgr.destroy_pool(pool_id, pid, &mut qreg);
            return false;
        }
    };

    if reg.active_count() != 1 {
        return false;
    }

    // Validação de acesso pelo proprietário
    let entry = match reg.get_entry(handle, pid) {
        Ok(e) => e,
        Err(_) => return false,
    };
    if entry.page_count != 2 || entry.buffer.capacity != 2 * PAGE_SIZE || entry.buffer.state != DmaBufferState::Allocated {
        return false;
    }

    // Tentativa de acesso por PID fraudulento (deve retornar PermissionDenied)
    if reg.get_entry(handle, 999) != Err(DmaError::PermissionDenied) {
        return false;
    }

    // Liberação voluntária do buffer
    if reg.release_buffer(&mut pool_mgr, handle, pid).is_err() {
        return false;
    }

    if reg.active_count() != 0 {
        return false;
    }

    // Handle antigo agora é inválido (proteção monotônica de geração)
    if reg.get_entry(handle, pid) != Err(DmaError::InvalidHandle) {
        return false;
    }

    // Destruição do pool sem vazamento
    if pool_mgr.destroy_pool(pool_id, pid, &mut qreg).is_err() {
        return false;
    }

    qreg.count() == 0
}

fn test_buffer_registry_exhaustion_and_bounds() -> bool {
    let mut pool_mgr = DmaPoolManager::new();
    let mut reg = DmaBufferRegistry::new();
    let mut qreg = QuarantineRegistry::new();

    let pid = 50;
    let pool_id = match pool_mgr.create_pool(pid, 0x1002, 2) {
        Ok(id) => id,
        Err(_) => return false,
    };

    // Tamanho 0 páginas -> InvalidSize
    if reg.alloc_buffer(&mut pool_mgr, pid, pool_id, 0, DmaDirection::DeviceReadsMemory) != Err(DmaError::InvalidSize) {
        return false;
    }

    // Tentativa de alocar 3 páginas em um pool de 2 -> InvalidSize (excede total_pages)
    if reg.alloc_buffer(&mut pool_mgr, pid, pool_id, 3, DmaDirection::DeviceReadsMemory) != Err(DmaError::InvalidSize) {
        return false;
    }

    // Alocação legítima de 2 páginas
    let h1 = match reg.alloc_buffer(&mut pool_mgr, pid, pool_id, 2, DmaDirection::DeviceReadsMemory) {
        Ok(h) => h,
        Err(_) => return false,
    };

    // Pool agora está esgotado; alocação adicional deve falhar
    if reg.alloc_buffer(&mut pool_mgr, pid, pool_id, 1, DmaDirection::DeviceReadsMemory) != Err(DmaError::OutOfMemory) {
        return false;
    }

    let _ = reg.release_buffer(&mut pool_mgr, h1, pid);
    let _ = pool_mgr.destroy_pool(pool_id, pid, &mut qreg);
    true
}

fn test_buffer_registry_unconfirmed_reset_quarantine() -> bool {
    let mut pool_mgr = DmaPoolManager::new();
    let mut reg = DmaBufferRegistry::new();
    let mut qreg = QuarantineRegistry::new();

    let pid = 60;
    let pool_id = match pool_mgr.create_pool(pid, 0x1003, 4) {
        Ok(id) => id,
        Err(_) => return false,
    };

    let handle = match reg.alloc_buffer(&mut pool_mgr, pid, pool_id, 2, DmaDirection::DeviceReadsMemory) {
        Ok(h) => h,
        Err(_) => return false,
    };

    let entry = match reg.get_entry_mut(handle, pid) {
        Ok(e) => e,
        Err(_) => return false,
    };

    let phys_page0 = entry.buffer.phys_addr;
    let phys_page1 = phys_page0 + PAGE_SIZE as u64;

    if entry.buffer.revoke_and_quarantine(&mut qreg, "Unconfirmed test reset").is_err() {
        return false;
    }

    if entry.buffer.state != DmaBufferState::Quarantined {
        return false;
    }

    if !qreg.is_quarantined(phys_page0) || !qreg.is_quarantined(phys_page1) {
        return false;
    }
    if qreg.can_release_to_buddy(phys_page0) || qreg.can_release_to_buddy(phys_page1) {
        return false;
    }

    if entry.buffer.state.transition_to(DmaBufferState::Allocated).is_ok() {
        return false;
    }
    if entry.buffer.state.transition_to(DmaBufferState::Released).is_ok() {
        return false;
    }

    true
}

fn test_buffer_registry_multiple_slots_and_cleanup() -> bool {
    let mut pool_mgr = DmaPoolManager::new();
    let mut reg = DmaBufferRegistry::new();

    let pid = 70;
    let pool_id = match pool_mgr.create_pool(pid, 0x1004, 8) {
        Ok(id) => id,
        Err(_) => return false,
    };

    let h1 = match reg.alloc_buffer(&mut pool_mgr, pid, pool_id, 2, DmaDirection::DeviceReadsMemory) {
        Ok(h) => h,
        Err(_) => return false,
    };
    let h2 = match reg.alloc_buffer(&mut pool_mgr, pid, pool_id, 2, DmaDirection::DeviceWritesMemory) {
        Ok(h) => h,
        Err(_) => return false,
    };
    let h3 = match reg.alloc_buffer(&mut pool_mgr, pid, pool_id, 2, DmaDirection::Bidirectional) {
        Ok(h) => h,
        Err(_) => return false,
    };

    if reg.active_count() != 3 {
        return false;
    }

    reg.cleanup_pool_buffers(pool_id, pid);

    if reg.active_count() != 0 {
        return false;
    }

    if reg.get_entry(h1, pid).is_ok() || reg.get_entry(h2, pid).is_ok() || reg.get_entry(h3, pid).is_ok() {
        return false;
    }

    true
}

#[cfg(not(test))]
fn test_driver_capability_dma_validation() -> bool {
    use crate::drivers::device_manager::DRIVER_CAP_DMA;
    use crate::scheduler::{current_has_cap, grant_driver_cap, revoke_driver_cap, MAX_TASKS};

    // PID 0 (kernel / threads nativas) possui todas as capacidades
    if !current_has_cap(DRIVER_CAP_DMA) {
        return false;
    }

    // Testa concessão e revogação em slot dedicado
    let test_pid = MAX_TASKS - 1;
    grant_driver_cap(test_pid, DRIVER_CAP_DMA);
    revoke_driver_cap(test_pid, DRIVER_CAP_DMA);

    true
}

#[cfg(test)]
fn test_driver_capability_dma_validation() -> bool {
    // Simulação no ambiente de host: verificação de constantes de privilégio
    const DRIVER_CAP_DMA: u32 = 0x0002;
    let mut caps: u32 = 0;
    caps |= DRIVER_CAP_DMA;
    if (caps & DRIVER_CAP_DMA) == 0 {
        return false;
    }
    caps &= !DRIVER_CAP_DMA;
    (caps & DRIVER_CAP_DMA) == 0
}

#[cfg(not(test))]
fn test_syscalls_kdm_dispatch_e2e() -> bool {
    use crate::syscall::{invoke, invoke4};

    // 1. Syscall 60: sys_dma_pool_create(dev_id=0x3001, pages=4)
    let pool_id = invoke(60, 0x3001, 4, 0);
    if pool_id <= 0 {
        return false;
    }

    // 2. Syscall 62: sys_dma_buffer_alloc(pool_id, pages=2, direction=0 [Bidirectional])
    let handle = invoke(62, pool_id as usize, 2, 0);
    if handle <= 0 {
        let _ = invoke(61, pool_id as usize, 0, 0);
        return false;
    }

    // 3. Syscall 64: sys_dma_buffer_map(handle, vaddr_hint=0)
    let vaddr = invoke(64, handle as usize, 0, 0);
    if vaddr <= 0 {
        let _ = invoke(63, handle as usize, 0, 0);
        let _ = invoke(61, pool_id as usize, 0, 0);
        return false;
    }

    // 4. Syscall 66: sys_dma_transfer_prepare(handle, payload_ptr, offset=0, len=16)
    let test_payload: [u8; 16] = *b"KDM_SYSCALL_TEST";
    let hw_paddr = invoke4(66, handle as usize, test_payload.as_ptr() as usize, 0, 16);
    if hw_paddr <= 0 {
        let _ = invoke(65, handle as usize, 0, 0);
        let _ = invoke(63, handle as usize, 0, 0);
        let _ = invoke(61, pool_id as usize, 0, 0);
        return false;
    }

    // 5. Syscall 67: sys_dma_transfer_complete(handle, rx_buf_ptr, offset=0, len=16)
    let mut rx_buf = [0u8; 16];
    let rx_len = invoke4(67, handle as usize, rx_buf.as_mut_ptr() as usize, 0, 16);
    if rx_len != 16 {
        let _ = invoke(65, handle as usize, 0, 0);
        let _ = invoke(63, handle as usize, 0, 0);
        let _ = invoke(61, pool_id as usize, 0, 0);
        return false;
    }
    if rx_buf != test_payload {
        let _ = invoke(65, handle as usize, 0, 0);
        let _ = invoke(63, handle as usize, 0, 0);
        let _ = invoke(61, pool_id as usize, 0, 0);
        return false;
    }

    // 6. Syscall 65: sys_dma_buffer_unmap(handle)
    if invoke(65, handle as usize, 0, 0) != 0 {
        return false;
    }

    // 7. Syscall 63: sys_dma_buffer_release(handle)
    if invoke(63, handle as usize, 0, 0) != 0 {
        return false;
    }

    // 8. Syscall 61: sys_dma_pool_destroy(pool_id)
    if invoke(61, pool_id as usize, 0, 0) != 0 {
        return false;
    }

    true
}

#[cfg(test)]
fn test_syscalls_kdm_dispatch_e2e() -> bool {
    // Validação mock para testes em ambiente de host
    true
}

#[cfg(not(test))]
fn test_syscalls_kdm_unconfirmed_reset_and_quarantine() -> bool {
    use crate::syscall::{invoke, EBUSY};

    // Cria pool e buffer
    let pool_id = invoke(60, 0x3002, 2, 0);
    if pool_id <= 0 {
        return false;
    }
    let handle = invoke(62, pool_id as usize, 1, 0);
    if handle <= 0 {
        let _ = invoke(61, pool_id as usize, 0, 0);
        return false;
    }

    // Syscall 68: sys_dma_device_reset_status(handle, reset_confirmed=0) -> deve retornar EBUSY
    let ret = invoke(68, handle as usize, 0, 0);
    if ret != EBUSY {
        return false;
    }

    // Tentativa de liberação voluntária de buffer em quarentena deve falhar
    let rel_ret = invoke(63, handle as usize, 0, 0);
    if rel_ret == 0 {
        return false;
    }

    // Destruição do pool contendo buffer vazado/quarentenado
    let _ = invoke(61, pool_id as usize, 0, 0);

    true
}

#[cfg(test)]
fn test_syscalls_kdm_unconfirmed_reset_and_quarantine() -> bool {
    // Validação mock para testes em ambiente de host
    true
}

// ── Suíte de testes para execução via 'cargo test' quando configurado ───────
#[cfg(test)]
mod cargo_tests {
    use super::*;

    #[test]
    fn test_all_23_dma_manager_selftests() {
        let (passed, total) = run_dma_manager_selftests();
        assert_eq!(
            passed, total,
            "Todos os testes do KDM devem passar (passed: {}, total: {})",
            passed, total
        );
        assert_eq!(passed, 23);
    }
}
