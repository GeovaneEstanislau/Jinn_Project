//! Jinn OS — Kernel DMA Manager (KDM)
//! Tokens Opacos de Acesso e Contadores de Geração (Fase 1)

use super::types::DmaError;

/// Token de autorização concedido a um processo de driver para acesso a buffers DMA.
///
/// Proteção estrita contra:
/// - Acesso cruzado entre processos (owner_pid).
/// - Acesso cruzado entre dispositivos periféricos (device_id).
/// - Uso de handles obsoletos/defasados após reinicialização ou revogação (generation).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmaToken {
    /// Identificador sequencial ou índice de slot do buffer
    pub id: u64,
    /// Contador monotônico de geração (incrementado a cada ciclo/revogação)
    pub generation: u64,
    /// PID do processo do driver proprietário deste token
    pub owner_pid: u64,
    /// Identificador do dispositivo PCI/periférico autorizado
    pub device_id: u32,
}

impl DmaToken {
    /// Cria uma nova instância de DmaToken
    pub const fn new(id: u64, generation: u64, owner_pid: u64, device_id: u32) -> Self {
        Self {
            id,
            generation,
            owner_pid,
            device_id,
        }
    }

    /// Valida se o token pertence exatamente ao PID, dispositivo e geração esperados.
    pub fn is_valid_for(&self, expected_pid: u64, expected_device: u32, expected_gen: u64) -> bool {
        self.owner_pid == expected_pid && self.device_id == expected_device && self.generation == expected_gen
    }

    /// Codifica o identificador do buffer e a geração em um handle opaco de 64 bits para Ring 3.
    /// Formato: [32 bits: buffer_id/slot] | [32 bits: generation]
    pub const fn encode_handle(&self) -> u64 {
        Self::encode_raw_handle(self.id as u32, self.generation as u32)
    }

    /// Helper estático para codificar diretamente (slot_id, generation) em handle opaco de 64 bits.
    pub const fn encode_raw_handle(slot_id: u32, generation: u32) -> u64 {
        ((slot_id as u64) << 32) | (generation as u64)
    }

    /// Decodifica um handle opaco de 64 bits retornando (slot_id, generation).
    pub const fn decode_handle(handle: u64) -> (u32, u32) {
        let slot_id = (handle >> 32) as u32;
        let generation = (handle & 0xFFFF_FFFF) as u32;
        (slot_id, generation)
    }

    /// Valida o handle opaco contra a geração atual esperada no slot do kernel.
    /// Retorna `Ok(slot_id)` se a geração conferir exatamente; caso contrário `Err(DmaError::InvalidHandle)`.
    pub fn validate_handle(handle: u64, expected_generation: u64) -> Result<u32, DmaError> {
        let (slot_id, gen) = Self::decode_handle(handle);
        if gen as u64 == expected_generation {
            Ok(slot_id)
        } else {
            Err(DmaError::InvalidHandle)
        }
    }
}
