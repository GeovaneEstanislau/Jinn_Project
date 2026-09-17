#![allow(dead_code)]

use crate::scheduler::{self, MAX_TASKS};
use crate::telemetry;

/// Armazena a predição heurística de uma tarefa
#[derive(Debug, Clone, Copy)]
pub struct Prediction {
    pub affinity_pid: usize,
    pub io_bound: bool,
}

impl Prediction {
    const fn new() -> Self {
        Self {
            affinity_pid: usize::MAX, // usize::MAX significa sem afinidade detectada
            io_bound: false,
        }
    }
}

/// Tabela global de predições.
/// Sendo um sistema microkernel focado em latência, e rodando no BSP (Single Core
/// na versão atual), a escrita/leitura atômica via safe pointer seria ideal, mas
/// para a estrutura básica usamos unsafe simplificado, garantindo lock-free total.
static mut PREDICTIONS: [Prediction; MAX_TASKS] = [Prediction::new(); MAX_TASKS];

pub fn get_prediction(pid: usize) -> Option<Prediction> {
    if pid < MAX_TASKS {
        unsafe { Some(PREDICTIONS[pid]) }
    } else {
        None
    }
}

/// O loop principal do Motor Preditivo.
/// Roda em Ring 0 como uma Tarefa de Kernel, sem bloquear a preempção.
pub fn analyze_telemetry_loop() {
    let mut last_run_ticks = scheduler::sched_ticks();

    loop {
        let current_ticks = scheduler::sched_ticks();
        
        // Roda a heurística a cada ~1000 ticks de escalonamento para não sufocar a CPU
        if current_ticks.saturating_sub(last_run_ticks) >= 1000 {
            last_run_ticks = current_ticks;
            run_heuristics();
        }

        // Cede a CPU voluntariamente para a próxima tarefa pronta
        scheduler::yield_now();
    }
}

fn run_heuristics() {
    let sched = scheduler::get();
    
    for pid in 0..MAX_TASKS {
        if sched.tasks[pid].is_none() { continue; }

        if let Some((cpu_ticks, tx, rx, _pf)) = telemetry::get_metrics(pid) {
            let mut io_bound = false;
            let mut best_affinity = usize::MAX;

            // 1. Classificação Heurística de I/O Bound vs CPU Bound
            // Se a tarefa gasta poucos ciclos (alta dormência) mas envia/recebe
            // um volume alto de mensagens IPC
            let ipc_total = tx + rx;
            if ipc_total > 50 && (ipc_total * 10) > cpu_ticks {
                io_bound = true;
            }

            // 2. Classificação de Afinidade IPC (Cache Pre-warming target)
            // Se é IO Bound e essencialmente receptora, ela provavelmete conversa
            // com a maior emissora do sistema. (V0.1 Heurística Simplificada).
            if io_bound && rx > tx {
                let mut max_tx = 0;
                let mut max_tx_pid = usize::MAX;
                for other in 0..MAX_TASKS {
                    if other != pid && sched.tasks[other].is_some() {
                        if let Some((_, o_tx, _, _)) = telemetry::get_metrics(other) {
                            if o_tx > max_tx {
                                max_tx = o_tx;
                                max_tx_pid = other;
                            }
                        }
                    }
                }
                if max_tx_pid != usize::MAX {
                    best_affinity = max_tx_pid;
                }
            } else if io_bound && tx > rx {
                let mut max_rx = 0;
                let mut max_rx_pid = usize::MAX;
                for other in 0..MAX_TASKS {
                    if other != pid && sched.tasks[other].is_some() {
                        if let Some((_, _, o_rx, _)) = telemetry::get_metrics(other) {
                            if o_rx > max_rx {
                                max_rx = o_rx;
                                max_rx_pid = other;
                            }
                        }
                    }
                }
                if max_rx_pid != usize::MAX {
                    best_affinity = max_rx_pid;
                }
            }

            // Atualiza Tabela de Predição para uso pelo Escalonador Adaptativo
            unsafe {
                PREDICTIONS[pid] = Prediction {
                    affinity_pid: best_affinity,
                    io_bound,
                };
            }
        }
    }
}
