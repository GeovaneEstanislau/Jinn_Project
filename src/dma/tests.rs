//! Jinn OS — Kernel DMA Manager (KDM)
//! Suíte de Testes Unitários e Selftests de Segurança (Fase 1)

use super::types::{DmaError, PAGE_SIZE};
use super::token::DmaToken;
use super::state::DmaBufferState;
use super::buffer::{DmaBuffer, DmaDescriptor, QuarantineRegistry};

/// Executa todos os testes unitários da Fase 1 do Kernel DMA Manager.
/// Retorna `(passed, total)` para auditoria pelo kernel e logs de inicialização.
pub fn run_dma_manager_selftests() -> (usize, usize) {
    let mut passed = 0;
    let mut total = 0;

    // Teste 1: Tokens opacos, decodificação de handle e proteção contra handles defasados
    total += 1;
    if test_token_and_handle_security() {
        passed += 1;
    }

    // Teste 2: Aritmética segura contra integer overflow em descritores
    total += 1;
    if test_descriptor_overflow_protection() {
        passed += 1;
    }

    // Teste 3: Validação do ciclo normal da máquina de estados
    total += 1;
    if test_state_machine_valid_lifecycle() {
        passed += 1;
    }

    // Teste 4: Rejeição estrita de transições de estado ilegais (Memory Safety)
    total += 1;
    if test_state_machine_illegal_transitions() {
        passed += 1;
    }

    // Teste 5: Revogação com confirmação de reset de hardware (Graceful Release)
    total += 1;
    if test_graceful_revocation_on_confirmed_reset() {
        passed += 1;
    }

    // Teste 6: Falha de reset -> Quarentena Permanente e Barreira no Buddy Allocator
    total += 1;
    if test_unconfirmed_reset_permanent_quarantine() {
        passed += 1;
    }

    // Teste 7: Registro de Quarentena: idempotência, multi-páginas e integridade
    total += 1;
    if test_quarantine_registry_integrity() {
        passed += 1;
    }

    // Teste 8: Submissão e conclusão de transferências com rastreamento ativo
    total += 1;
    if test_transfer_submission_and_tracking() {
        passed += 1;
    }

    (passed, total)
}

fn test_token_and_handle_security() -> bool {
    let token = DmaToken::new(7, 42, 100, 0x1001);

    // Verificação de validação com parâmetros exatos
    if !token.is_valid_for(100, 0x1001, 42) {
        return false;
    }

    // Rejeição por PID diferente (cross-process)
    if token.is_valid_for(101, 0x1001, 42) {
        return false;
    }

    // Rejeição por dispositivo diferente (cross-device)
    if token.is_valid_for(100, 0x1002, 42) {
        return false;
    }

    // Rejeição por geração defasada (stale handle)
    if token.is_valid_for(100, 0x1001, 41) {
        return false;
    }

    // Codificação e decodificação do handle opaco de 64 bits
    let handle = token.encode_handle();
    let (slot, gen) = DmaToken::decode_handle(handle);
    if slot != 7 || gen != 42 {
        return false;
    }

    // Validação com geração atual conferindo
    if DmaToken::validate_handle(handle, 42) != Ok(7) {
        return false;
    }

    // Rejeição com geração defasada (retorna InvalidHandle -> ESRCH)
    if DmaToken::validate_handle(handle, 43) != Err(DmaError::InvalidHandle) {
        return false;
    }

    true
}

fn test_descriptor_overflow_protection() -> bool {
    let capacity = 4096;

    // Descritor válido
    let valid_desc = DmaDescriptor::new(100, 500);
    if valid_desc.validate_bounds(capacity) != Ok(600) {
        return false;
    }

    // Descritor cobrindo o buffer inteiro exatamente
    let full_desc = DmaDescriptor::new(0, 4096);
    if full_desc.validate_bounds(capacity) != Ok(4096) {
        return false;
    }

    // Descritor com comprimento zero (rejeição de InvalidSize)
    let zero_desc = DmaDescriptor::new(0, 0);
    if zero_desc.validate_bounds(capacity) != Err(DmaError::InvalidSize) {
        return false;
    }

    // Descritor fora dos limites (OutOfBounds)
    let oob_desc = DmaDescriptor::new(4000, 200);
    if oob_desc.validate_bounds(capacity) != Err(DmaError::OutOfBounds) {
        return false;
    }

    // Estouro aritmético intencional (Integer Overflow)
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

    // Allocated -> Mapped
    if state.transition_to(DmaBufferState::Mapped).is_err() {
        return false;
    }

    // Mapped -> Prepared
    if state.transition_to(DmaBufferState::Prepared).is_err() {
        return false;
    }

    // Prepared -> InTransfer
    if state.transition_to(DmaBufferState::InTransfer).is_err() {
        return false;
    }
    if !state.is_active() {
        return false;
    }

    // InTransfer -> Completed
    if state.transition_to(DmaBufferState::Completed).is_err() {
        return false;
    }

    // Completed -> Mapped (pronto para nova transferência)
    if state.transition_to(DmaBufferState::Mapped).is_err() {
        return false;
    }

    // Mapped -> Released (liberação normal)
    if state.transition_to(DmaBufferState::Released).is_err() {
        return false;
    }
    if !state.is_terminal() {
        return false;
    }

    true
}

fn test_state_machine_illegal_transitions() -> bool {
    // 1. Allocated NÃO pode pular para InTransfer
    let mut state = DmaBufferState::Allocated;
    if state.transition_to(DmaBufferState::InTransfer) != Err(DmaError::InvalidState) {
        return false;
    }

    // 2. InTransfer NÃO PODE pular direto para Released (Hardware ainda escrevendo no barramento!)
    state = DmaBufferState::InTransfer;
    if state.transition_to(DmaBufferState::Released) != Err(DmaError::InvalidState) {
        return false;
    }

    // 3. InTransfer NÃO PODE pular direto para Mapped
    if state.transition_to(DmaBufferState::Mapped) != Err(DmaError::InvalidState) {
        return false;
    }

    // 4. Completed NÃO PODE pular direto para InTransfer sem ser Prepared
    state = DmaBufferState::Completed;
    if state.transition_to(DmaBufferState::InTransfer) != Err(DmaError::InvalidState) {
        return false;
    }

    // 5. Released é terminal para aquela geração
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

    // Reset de hardware executado e confirmado com sucesso: transita para Released
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

    // Reset de hardware falhou ou não pôde ser confirmado: quarentena obrigatória!
    if buffer.revoke_and_quarantine(&mut registry, "HW Reset Unconfirmed").is_err() {
        return false;
    }

    // O buffer DEVE estar no estado terminal Quarantined
    if buffer.state != DmaBufferState::Quarantined {
        return false;
    }
    if !buffer.state.is_quarantined() {
        return false;
    }
    if !buffer.state.is_terminal() {
        return false;
    }

    // PROVA FORMAL DE SUMIDOURO: Nenhuma transição para fora de Quarantined é aceita!
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

    // O frame físico DEVE estar registrado na quarentena
    if !registry.is_quarantined(0x2000_0000) {
        return false;
    }
    if !registry.is_quarantined(0x2001_0000) {
        return false;
    }

    // O alocador Buddy NUNCA pode receber essa página
    if registry.can_release_to_buddy(0x2000_0000) {
        return false;
    }
    if registry.can_release_to_buddy(0x2001_0000) {
        return false;
    }

    // Uma página não quarentenada pode ser liberada normalmente
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

    // Inserção de frame 0 é rejeitada como inválida
    if registry.quarantine_frame(0, "Invalid zero page") != Err(DmaError::InvalidSize) {
        return false;
    }

    // Inserção de frame válido
    if registry.quarantine_frame(0x1000, "Device Fault 1").is_err() {
        return false;
    }
    if registry.count() != 1 {
        return false;
    }

    // Idempotência: reinserir o mesmo frame não incrementa contagem nem gera erro
    if registry.quarantine_frame(0x1000, "Device Fault 1 duplicate").is_err() {
        return false;
    }
    if registry.count() != 1 {
        return false;
    }

    // Verificação de consulta
    if !registry.is_quarantined(0x1000) {
        return false;
    }
    if registry.is_quarantined(0x2000) {
        return false;
    }

    // Buffer de 3 páginas (12 KiB)
    let mut buffer = DmaBuffer::empty();
    buffer.capacity = 3 * PAGE_SIZE;
    buffer.phys_addr = 0x4000;
    buffer.state = DmaBufferState::InTransfer;

    if buffer.revoke_and_quarantine(&mut registry, "Multi-page DMA Crash").is_err() {
        return false;
    }

    // Todas as 3 páginas devem estar em quarentena
    if !registry.is_quarantined(0x4000) {
        return false;
    }
    if !registry.is_quarantined(0x5000) {
        return false;
    }
    if !registry.is_quarantined(0x6000) {
        return false;
    }
    // Próxima página externa não deve estar
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

    // Submissão de descritor válido
    let desc = DmaDescriptor::new(0, 1500);
    if buffer.submit_transfer(&desc).is_err() {
        return false;
    }

    if buffer.state != DmaBufferState::InTransfer {
        return false;
    }
    if buffer.active_transfers != 1 {
        return false;
    }

    // Conclusão da transferência
    if buffer.complete_transfer().is_err() {
        return false;
    }

    if buffer.state != DmaBufferState::Completed {
        return false;
    }
    if buffer.active_transfers != 0 {
        return false;
    }

    // Ciclo de retorno para Mapped e nova transferência
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

// Suíte de testes para execução via 'cargo test --lib' no host
#[cfg(test)]
mod cargo_tests {
    use super::*;

    #[test]
    fn test_token_and_handle_security_unit() {
        assert!(test_token_and_handle_security());
    }

    #[test]
    fn test_descriptor_overflow_protection_unit() {
        assert!(test_descriptor_overflow_protection());
    }

    #[test]
    fn test_state_machine_valid_lifecycle_unit() {
        assert!(test_state_machine_valid_lifecycle());
    }

    #[test]
    fn test_state_machine_illegal_transitions_unit() {
        assert!(test_state_machine_illegal_transitions());
    }

    #[test]
    fn test_graceful_revocation_on_confirmed_reset_unit() {
        assert!(test_graceful_revocation_on_confirmed_reset());
    }

    #[test]
    fn test_unconfirmed_reset_permanent_quarantine_unit() {
        assert!(test_unconfirmed_reset_permanent_quarantine());
    }

    #[test]
    fn test_quarantine_registry_integrity_unit() {
        assert!(test_quarantine_registry_integrity());
    }

    #[test]
    fn test_transfer_submission_and_tracking_unit() {
        assert!(test_transfer_submission_and_tracking());
    }

    #[test]
    fn test_all_dma_manager_selftests() {
        let (passed, total) = run_dma_manager_selftests();
        assert_eq!(passed, total, "Todos os testes do KDM devem passar (passed: {}, total: {})", passed, total);
        assert_eq!(passed, 8);
    }
}
