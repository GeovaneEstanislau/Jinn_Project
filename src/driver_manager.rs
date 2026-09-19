// Jinn OS — Driver Manager (Ring 0)
//
// Gerencia drivers de espaço de usuário e roteamento de IRQ via mensagens IPC.

use core::sync::atomic::{AtomicUsize, Ordering};

pub const MAX_IRQS: usize = 16;

/// Tabela que mapeia o número de uma IRQ para o PID do driver que a gerencia.
/// `usize::MAX` significa que não há driver registrado para a IRQ.
#[allow(clippy::declare_interior_mutable_const)]
const UNINIT: AtomicUsize = AtomicUsize::new(usize::MAX);
static IRQ_ROUTING_TABLE: [AtomicUsize; MAX_IRQS] = [UNINIT; MAX_IRQS];

/// Registra a IRQ `irq_num` para ser roteada para `pid`.
/// Retorna 0 em sucesso, ou negativo em caso de erro.
pub fn register_irq(irq_num: u8, pid: usize) -> isize {
    if (irq_num as usize) >= MAX_IRQS {
        return -22; // EINVAL
    }
    
    // Para simplificar, aceitamos sobrescrever se o driver pedir de novo,
    // ou se outro assumir. (Em um sistema real, exigiria capabilities).
    IRQ_ROUTING_TABLE[irq_num as usize].store(pid, Ordering::SeqCst);
    0
}

/// Consulta o PID registrado para uma determinada IRQ.
pub fn get_irq_pid(irq_num: u8) -> Option<usize> {
    if (irq_num as usize) >= MAX_IRQS {
        return None;
    }
    let pid = IRQ_ROUTING_TABLE[irq_num as usize].load(Ordering::Relaxed);
    if pid == usize::MAX {
        None
    } else {
        Some(pid)
    }
}
