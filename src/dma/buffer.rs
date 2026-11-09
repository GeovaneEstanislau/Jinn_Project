//! Jinn OS — Kernel DMA Manager (KDM)
//! Tipos de Controle, Descritores e Registro de Quarentena Permanente (Fase 1)

use super::types::{
    DmaBackend, DmaDirection, DmaError, MAX_QUARANTINED_FRAMES, PAGE_SIZE,
};
use super::state::DmaBufferState;

/// Descritor de transferência DMA submetido pelo driver.
/// Define uma fatia de dados dentro do buffer alocado.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmaDescriptor {
    /// Deslocamento inicial dentro do buffer (bytes)
    pub offset: usize,
    /// Comprimento dos dados a transferir (bytes)
    pub length: usize,
}

impl DmaDescriptor {
    /// Cria um novo descritor de transferência
    pub const fn new(offset: usize, length: usize) -> Self {
        Self { offset, length }
    }

    /// Valida matematicamente os limites do descritor contra a capacidade do buffer.
    ///
    /// Garante que:
    /// 1. `length > 0` (tamanho não nulo)
    /// 2. `offset + length` não cause overflow aritmético de inteiros.
    /// 3. `offset + length <= buffer_capacity` (sem vazamento de limites).
    ///
    /// Retorna `Ok(end_offset)` em caso de sucesso ou `Err(DmaError)` específico.
    pub fn validate_bounds(&self, buffer_capacity: usize) -> Result<usize, DmaError> {
        if self.length == 0 {
            return Err(DmaError::InvalidSize);
        }

        let end = match self.offset.checked_add(self.length) {
            Some(sum) => sum,
            None => return Err(DmaError::ArithmeticOverflow),
        };

        if end > buffer_capacity {
            return Err(DmaError::OutOfBounds);
        }

        Ok(end)
    }
}

/// Registro de Quarentena Permanente de Páginas Físicas de Memória.
///
/// Invariante Arquitetural Crítica:
/// Quando um dispositivo falha durante DMA ativo e o reset de hardware não pode ser
/// confirmado pelo barramento com 100% de certeza, todas as páginas físicas envolvidas
/// são permanentemente registradas aqui. O alocador Buddy NUNCA mais poderá reciclar
/// esses frames físicos para novos processos ou para o kernel.
pub struct QuarantineRegistry {
    frames: [u64; MAX_QUARANTINED_FRAMES],
    reasons: [&'static str; MAX_QUARANTINED_FRAMES],
    count: usize,
}

impl QuarantineRegistry {
    /// Cria uma nova instância vazia do registro de quarentena
    pub const fn new() -> Self {
        Self {
            frames: [0; MAX_QUARANTINED_FRAMES],
            reasons: [""; MAX_QUARANTINED_FRAMES],
            count: 0,
        }
    }

    /// Adiciona um frame físico à quarentena permanente com justificativa auditável.
    /// Retorna `Ok(())` se registrado (ou já existente), ou `Err(DmaError::OutOfMemory)` se o registro estiver cheio.
    pub fn quarantine_frame(&mut self, paddr: u64, reason: &'static str) -> Result<(), DmaError> {
        if paddr == 0 {
            return Err(DmaError::InvalidSize);
        }

        // Se já está em quarentena, operação é idempotente
        if self.is_quarantined(paddr) {
            return Ok(());
        }

        if self.count >= MAX_QUARANTINED_FRAMES {
            return Err(DmaError::OutOfMemory);
        }

        self.frames[self.count] = paddr;
        self.reasons[self.count] = reason;
        self.count += 1;
        Ok(())
    }

    /// Retorna verdadeiro se o frame físico está sob quarentena permanente.
    pub fn is_quarantined(&self, paddr: u64) -> bool {
        let mut i = 0;
        while i < self.count {
            if self.frames[i] == paddr {
                return true;
            }
            i += 1;
        }
        false
    }

    /// Retorna se a página física está segura para devolução ao Buddy Allocator.
    pub fn can_release_to_buddy(&self, paddr: u64) -> bool {
        !self.is_quarantined(paddr)
    }

    /// Retorna o número total de frames físicos atualmente em quarentena
    pub const fn count(&self) -> usize {
        self.count
    }

    /// Consulta um registro de quarentena pelo índice
    pub fn get_frame(&self, index: usize) -> Option<(u64, &'static str)> {
        if index < self.count {
            Some((self.frames[index], self.reasons[index]))
        } else {
            None
        }
    }
}

/// Metadados de controle de um buffer DMA gerenciado pelo KDM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmaBuffer {
    /// Identificador único / índice de slot do buffer
    pub id: u64,
    /// Geração do buffer (incrementada a cada ciclo de reutilização)
    pub generation: u64,
    /// PID do processo proprietário
    pub owner_pid: u64,
    /// ID do dispositivo de hardware autorizado
    pub device_id: u32,
    /// Capacidade total alocada (bytes)
    pub capacity: usize,
    /// Alinhamento de memória exigido
    pub alignment: usize,
    /// Direção do fluxo de DMA
    pub direction: DmaDirection,
    /// Backend de isolamento ativo (IOMMU ou KernelBounce)
    pub backend: DmaBackend,
    /// Estado formal na máquina de estados
    pub state: DmaBufferState,
    /// Endereço virtual no espaço do processo Ring 3
    pub user_vaddr: u64,
    /// Endereço físico da memória alocada (visível ao kernel ou hardware)
    pub phys_addr: u64,
    /// Contador de transferências físicas em andamento
    pub active_transfers: u32,
    /// Endereço físico do buffer intermediário (bounce buffer) no Ring 0 (se aplicável)
    pub bounce_buffer_phys: u64,
}

impl DmaBuffer {
    /// Cria um slot de buffer DMA vazio/não utilizado
    pub const fn empty() -> Self {
        Self {
            id: 0,
            generation: 0,
            owner_pid: 0,
            device_id: 0,
            capacity: 0,
            alignment: 0,
            direction: DmaDirection::Bidirectional,
            backend: DmaBackend::KernelBounce,
            state: DmaBufferState::Released,
            user_vaddr: 0,
            phys_addr: 0,
            active_transfers: 0,
            bounce_buffer_phys: 0,
        }
    }

    /// Retorna verdadeiro se o buffer está atualmente alocado e em uso ativo
    pub fn is_in_use(&self) -> bool {
        self.state != DmaBufferState::Released && self.capacity > 0
    }

    /// Verifica autorização estrita por PID, dispositivo e geração
    pub fn can_access(&self, pid: u64, device_id: u32, generation: u64) -> bool {
        self.owner_pid == pid && self.device_id == device_id && self.generation == generation
    }

    /// Submete uma transferência validada, atualizando o estado para `InTransfer`
    pub fn submit_transfer(&mut self, desc: &DmaDescriptor) -> Result<(), DmaError> {
        desc.validate_bounds(self.capacity)?;

        match self.state {
            DmaBufferState::Mapped => {
                self.state.transition_to(DmaBufferState::Prepared)?;
                self.state.transition_to(DmaBufferState::InTransfer)?;
            }
            DmaBufferState::Prepared => {
                self.state.transition_to(DmaBufferState::InTransfer)?;
            }
            _ => return Err(DmaError::InvalidState),
        }

        self.active_transfers = self.active_transfers.saturating_add(1);
        Ok(())
    }

    /// Conclui a transferência de hardware atual, transitando para `Completed`
    pub fn complete_transfer(&mut self) -> Result<(), DmaError> {
        if self.state != DmaBufferState::InTransfer {
            return Err(DmaError::InvalidState);
        }

        self.active_transfers = self.active_transfers.saturating_sub(1);
        self.state.transition_to(DmaBufferState::Completed)
    }

    /// Revoga o buffer com reset de hardware bem-sucedido e confirmado.
    /// Libera o slot com segurança sem quarentena.
    pub fn revoke_and_release(&mut self) -> Result<(), DmaError> {
        if self.state != DmaBufferState::Revoking {
            self.state.transition_to(DmaBufferState::Revoking)?;
        }
        self.state.transition_to(DmaBufferState::Released)?;
        self.active_transfers = 0;
        Ok(())
    }

    /// Revoga o buffer quando o reset de hardware falha ou não é confirmado.
    /// Coloca o buffer em `Quarantined` e registra permanentemente todas as suas páginas no `QuarantineRegistry`.
    pub fn revoke_and_quarantine(
        &mut self,
        registry: &mut QuarantineRegistry,
        reason: &'static str,
    ) -> Result<(), DmaError> {
        if self.state != DmaBufferState::Revoking && self.state != DmaBufferState::Quarantined {
            self.state.transition_to(DmaBufferState::Revoking)?;
        }

        if self.state == DmaBufferState::Revoking {
            self.state.transition_to(DmaBufferState::Quarantined)?;
        }

        // Calcula o número de frames físicos que cobrem a capacidade
        let num_frames = if self.capacity == 0 {
            1
        } else {
            (self.capacity + PAGE_SIZE - 1) / PAGE_SIZE
        };

        // Quarentena as páginas físicas principais
        if self.phys_addr != 0 {
            for i in 0..num_frames {
                let frame = self.phys_addr + (i as u64 * PAGE_SIZE as u64);
                registry.quarantine_frame(frame, reason)?;
            }
        }

        // Quarentena as páginas do bounce buffer, caso existam
        if self.bounce_buffer_phys != 0 {
            for i in 0..num_frames {
                let bounce_frame = self.bounce_buffer_phys + (i as u64 * PAGE_SIZE as u64);
                registry.quarantine_frame(bounce_frame, reason)?;
            }
        }

        self.active_transfers = 0;
        Ok(())
    }
}
