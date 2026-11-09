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
pub mod tests;

pub use types::*;
pub use token::*;
pub use state::*;
pub use buffer::*;
pub use tests::run_dma_manager_selftests;

/// Inicializa o subsistema Kernel DMA Manager (KDM)
pub fn init() {
    // Fase 1: Inicialização estática dos tipos puros e estruturas fundamentais
}

