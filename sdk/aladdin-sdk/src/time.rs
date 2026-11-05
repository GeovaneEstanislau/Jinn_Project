//! Aladdin Desktop SDK — Temporização e Sincronização

use crate::syscall::{syscall1, SYS_SLEEP};

/// Suspende a execução da tarefa corrente pelo número de milissegundos indicado (sys_sleep).
#[inline(always)]
pub fn sleep(ms: u64) {
    unsafe {
        syscall1(SYS_SLEEP, ms);
    }
}

