/// Jinn OS — Jinn Dataflow Architecture (JDA)
///
/// Arquitetura experimental orientada a fluxo de dados (Dataflow), redução
/// de movimentação desnecessária de memória, I/O assíncrono e observabilidade.
///
/// Este módulo é concebido com chave mestra de desativação (JDA_ENABLED)
/// para garantir reversibilidade e preservação integral dos serviços clássicos do microkernel.

pub mod graph;
pub mod policy;
pub mod shmem;
pub mod telemetry;
pub mod topology;

use core::sync::atomic::{AtomicBool, Ordering};
use crate::memory::SpinLock;
use topology::HardwareTopologyModel;

/// Chave mestra em tempo de execução: permite desativar completamente o subsistema JDA
pub static JDA_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Modelo de topologia de hardware descoberto
pub static HARDWARE_TOPOLOGY: SpinLock<HardwareTopologyModel> =
    SpinLock::new(HardwareTopologyModel::new());

/// Inicializa o subsistema JDA descobrindo a topologia física real.
pub fn init() -> Result<(), &'static str> {
    // 1. Sondagem e montagem do modelo de topologia física
    let mut topo = HARDWARE_TOPOLOGY.lock();
    topo.probe();

    // 2. Reinicia contadores de telemetria
    telemetry::reset_metrics();

    // 3. Ativa o subsistema JDA
    JDA_ACTIVE.store(true, Ordering::SeqCst);

    Ok(())
}

/// Consulta se o JDA está ativo no sistema
#[inline]
pub fn is_active() -> bool {
    JDA_ACTIVE.load(Ordering::Relaxed)
}

/// Desativa o JDA retornando o sistema à política de referência padrão
pub fn disable() {
    JDA_ACTIVE.store(false, Ordering::SeqCst);
}

/// Ativa o JDA
pub fn enable() {
    JDA_ACTIVE.store(true, Ordering::SeqCst);
}
