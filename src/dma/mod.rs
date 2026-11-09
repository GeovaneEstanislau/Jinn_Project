//! Jinn OS — Kernel DMA Manager (KDM)
//!
//! Subsistema responsável por gerenciar acessos e buffers de DMA seguro para drivers
//! de dispositivos em Ring 3, aplicando isolamento estrito via tokens opacos, contadores
//! de geração monotônicos, buffers intermediários de kernel (Backend B) ou IOMMU (Backend A),
//! e quarentena permanente de memória em caso de reset de hardware não confirmado.

pub mod types;
pub mod token;
pub mod state;
pub mod buffer;
pub mod pool;
pub mod bounce;
pub mod manager;
pub mod tests;

pub use types::*;
pub use token::*;
pub use state::*;
pub use buffer::*;
pub use pool::*;
pub use bounce::*;
pub use manager::*;
pub use tests::run_dma_manager_selftests;

#[cfg(not(test))]
use crate::memory::SpinLock;

#[cfg(not(test))]
/// Tabela global de gerenciamento de pools DMA
pub static DMA_POOL_MANAGER: SpinLock<DmaPoolManager> = SpinLock::new(DmaPoolManager::new());

#[cfg(not(test))]
/// Tabela global de gerenciamento de buffers e handles DMA
pub static DMA_BUFFER_REGISTRY: SpinLock<DmaBufferRegistry> = SpinLock::new(DmaBufferRegistry::new());

#[cfg(not(test))]
/// Registro global de páginas físicas em quarentena permanente
pub static QUARANTINE_REGISTRY: SpinLock<QuarantineRegistry> = SpinLock::new(QuarantineRegistry::new());

/// Inicializa o subsistema Kernel DMA Manager (KDM)
pub fn init() {
    // Fase 1 e 2: Inicialização estática dos tipos, pools e mediador de bounce buffers
}

