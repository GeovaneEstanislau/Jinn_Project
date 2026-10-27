/// Jinn OS — Jinn Dataflow Architecture (JDA)
///
/// Arquitetura experimental orientada a fluxo de dados (Dataflow), redução
/// de movimentação desnecessária de memória, I/O assíncrono e observabilidade.
///
/// Este módulo é concebido com chave mestra de desativação (JDA_ENABLED)
/// para garantir reversibilidade e preservação integral dos serviços clássicos do microkernel.

pub mod graph;
pub mod policy;
pub mod ring;
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

/// Executa bateria de auto-testes de corretude e anéis assíncronos do JDA na inicialização
pub fn run_jda_selftests() -> (usize, usize) {
    let mut passed = 0;
    let mut total = 0;

    // Teste 1: Alocação e liberação de buffer no pool JDA
    total += 1;
    let mut pool = shmem::SHARED_BUFFER_POOL.lock();
    let handle_res = pool.allocate(0, false);
    if let Ok(handle) = handle_res {
        let phys_opt = pool.get_phys_addr(0, handle);
        if phys_opt.is_some() {
            passed += 1;
        }

        // Teste 2: Submissão e conclusão no anel assíncrono (OP_COMPUTE_SUM)
        total += 1;
        let phys = phys_opt.unwrap();
        let virt = crate::memory::paging::phys_to_virt(phys) as *mut u8;
        unsafe {
            for i in 0..100 {
                *virt.add(i) = 1;
            }
        }

        let sqe = ring::JdaSqe {
            opcode: ring::OP_COMPUTE_SUM,
            flags: ring::JDA_SQE_ZERO_COPY,
            priority: 1,
            _reserved: 0,
            handle: handle as u32,
            offset: 0,
            len: 100,
            user_data: 0xCAFE_BABE,
        };

        drop(pool);
        let mut ring = ring::JDA_RING.lock();
        if ring.submit(sqe).is_ok() {
            let processed = ring.dispatch_pending(0);
            if processed == 1 {
                if let Some(cqe) = ring.poll_completion() {
                    if cqe.user_data == 0xCAFE_BABE && cqe.res == 100 {
                        passed += 1;
                    }
                }
            }
        }
        drop(ring);

        // Teste 3: Liberação limpa de buffer físico
        total += 1;
        let mut pool = shmem::SHARED_BUFFER_POOL.lock();
        if pool.release(0, handle).is_ok() {
            passed += 1;
        }
    } else {
        total += 2;
    }

    (passed, total)
}

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
