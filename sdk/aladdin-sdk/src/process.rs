//! Aladdin Desktop SDK — Gerenciamento e Ciclo de Vida de Processos

use crate::syscall::{syscall0, syscall1, SYS_EXIT, SYS_GETPID, SYS_YIELD};

/// Encerra o processo atual de usuário enviando o código de saída ao kernel (sys_exit).
#[inline(always)]
pub fn exit(code: i32) -> ! {
    unsafe {
        syscall1(SYS_EXIT, code as u64);
    }
    loop {
        core::hint::spin_loop();
    }
}

/// Obtém o identificador do processo atual (PID) (sys_getpid).
#[inline(always)]
pub fn getpid() -> u64 {
    unsafe { syscall0(SYS_GETPID) as u64 }
}

/// Cede voluntariamente a fatia de CPU para outras tarefas do escalonador (sys_yield).
#[inline(always)]
pub fn yield_now() {
    unsafe {
        syscall0(SYS_YIELD);
    }
}
