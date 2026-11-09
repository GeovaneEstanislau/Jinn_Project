//! Jinn OS — Kernel DMA Manager (KDM)
//! Máquina de Estados Formal do Ciclo de Vida de Buffers e Transferências DMA (Fase 1)

use super::types::DmaError;

/// Estados do ciclo de vida de um buffer e transferência DMA gerenciados pelo KDM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmaBufferState {
    /// Buffer alocado fisicamente, ainda não mapeado no espaço de endereçamento do usuário.
    Allocated,
    /// Mapeado no espaço virtual do processo do driver (Ring 3).
    Mapped,
    /// Descritores submetidos, limites validados e bounce buffer sincronizado.
    Prepared,
    /// Transferência ativamente em execução pelo dispositivo no barramento (Bus Master).
    InTransfer,
    /// Dispositivo concluiu a transferência física; aguardando sincronização/retomada.
    Completed,
    /// Falha/crash detectada; reset do dispositivo em andamento para parar DMA no barramento.
    Revoking,
    /// Transferência finalizada com sucesso e buffer liberado para reciclagem normal.
    Released,
    /// ESTADO SUMIDOURO TERMINAL: Falha ao confirmar parada do DMA via reset de hardware.
    /// As páginas físicas associadas NUNCA serão devolvidas ao alocador Buddy.
    Quarantined,
}

impl DmaBufferState {
    /// Retorna verdadeiro se a transição de `self` para `next` é permitida pela máquina de estados.
    pub fn can_transition_to(&self, next: DmaBufferState) -> bool {
        match (*self, next) {
            // Buffer em Quarentena é TERMINAL SUMIDOURO: nenhuma transição é permitida.
            (DmaBufferState::Quarantined, _) => false,

            // Buffer liberado é terminal para esta geração do slot.
            (DmaBufferState::Released, _) => false,

            // De Allocated:
            // - Mapeamento para Ring 3 (Mapped)
            // - Falha de mapeamento / desalocação prematura (Released)
            // - Término prematuro do processo durante alocação (Revoking)
            (DmaBufferState::Allocated, DmaBufferState::Mapped) => true,
            (DmaBufferState::Allocated, DmaBufferState::Released) => true,
            (DmaBufferState::Allocated, DmaBufferState::Revoking) => true,

            // De Mapped:
            // - Preparação de descritor para transferência (Prepared)
            // - Desmapeamento / liberação voluntária sem transferir (Released)
            // - Falha do driver / reset (Revoking)
            (DmaBufferState::Mapped, DmaBufferState::Prepared) => true,
            (DmaBufferState::Mapped, DmaBufferState::Released) => true,
            (DmaBufferState::Mapped, DmaBufferState::Revoking) => true,

            // De Prepared:
            // - Submissão ao hardware (InTransfer)
            // - Cancelamento antes de enviar ao hardware (Mapped)
            // - Falha do driver / aborto (Revoking)
            (DmaBufferState::Prepared, DmaBufferState::InTransfer) => true,
            (DmaBufferState::Prepared, DmaBufferState::Mapped) => true,
            (DmaBufferState::Prepared, DmaBufferState::Revoking) => true,

            // De InTransfer:
            // - Hardware concluiu normalmente via IRQ/notificação (Completed)
            // - Falha, timeout ou crash do driver durante transferência ativa (Revoking)
            // NOTA DE SEGURANÇA: InTransfer NÃO PODE ir direto para Released ou Mapped!
            (DmaBufferState::InTransfer, DmaBufferState::Completed) => true,
            (DmaBufferState::InTransfer, DmaBufferState::Revoking) => true,

            // De Completed:
            // - Sincronização concluída, buffer retorna para Mapped para próximo ciclo (Mapped)
            // - Liberação definitiva do buffer (Released)
            // - Reset de emergência (Revoking)
            (DmaBufferState::Completed, DmaBufferState::Mapped) => true,
            (DmaBufferState::Completed, DmaBufferState::Released) => true,
            (DmaBufferState::Completed, DmaBufferState::Revoking) => true,

            // De Revoking:
            // - Reset do hardware confirmado com sucesso: DMA totalmente cessado (Released)
            // - Reset do hardware falhou ou não pôde ser confirmado: QUARENTENA PERMANENTE (Quarantined)
            (DmaBufferState::Revoking, DmaBufferState::Released) => true,
            (DmaBufferState::Revoking, DmaBufferState::Quarantined) => true,

            // Qualquer outra transição não especificada é terminantemente proibida
            _ => false,
        }
    }

    /// Executa a transição de estado, retornando erro caso a transição seja inválida.
    pub fn transition_to(&mut self, next: DmaBufferState) -> Result<(), DmaError> {
        if self.can_transition_to(next) {
            *self = next;
            Ok(())
        } else {
            Err(DmaError::InvalidState)
        }
    }

    /// Retorna verdadeiro se o buffer está com transferência física ou revogação ativa no barramento.
    pub fn is_active(&self) -> bool {
        matches!(self, DmaBufferState::InTransfer | DmaBufferState::Revoking)
    }

    /// Retorna verdadeiro se o buffer atingiu um estado terminal.
    pub fn is_terminal(&self) -> bool {
        matches!(self, DmaBufferState::Released | DmaBufferState::Quarantined)
    }

    /// Retorna verdadeiro se o buffer está em quarentena permanente.
    pub fn is_quarantined(&self) -> bool {
        matches!(self, DmaBufferState::Quarantined)
    }
}
