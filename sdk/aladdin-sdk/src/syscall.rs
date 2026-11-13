//! Aladdin Desktop SDK — Primitivas de Syscall em Baixo Nível (x86-64)

use core::arch::asm;

pub const SYS_EXIT:              u64 = 0;
pub const SYS_WRITE:             u64 = 1;
pub const SYS_READ:              u64 = 2;
pub const SYS_YIELD:             u64 = 3;
pub const SYS_GETPID:            u64 = 4;
pub const SYS_UPTIME:            u64 = 5;
pub const SYS_REBOOT:            u64 = 7;
pub const SYS_SLEEP:             u64 = 11;
pub const SYS_WINDOW_CREATE:     u64 = 20;
pub const SYS_WINDOW_BLIT:       u64 = 21;
pub const SYS_WINDOW_CLOSE:      u64 = 22;
pub const SYS_WINDOW_POLL_EVENT: u64 = 24;
pub const SYS_SPAWN_PROCESS:     u64 = 39;
pub const SYS_CLOCK_GETTIME:      u64 = 70;

pub const SYS_GRAPHICS_REGISTER:              u64 = 72;
pub const SYS_GRAPHICS_GET_FB_INFO:          u64 = 73;
pub const SYS_GRAPHICS_PRESENT:              u64 = 74;
pub const SYS_GRAPHICS_POLL_INPUT:           u64 = 75;
pub const SYS_GRAPHICS_UNREGISTER:           u64 = 76;
pub const SYS_GRAPHICS_READ_CLIENT_SURFACE:   u64 = 77;
pub const SYS_GRAPHICS_DISPATCH_CLIENT_EVENT: u64 = 78;
pub const SYS_GRAPHICS_LIST_CLIENT_WINDOWS:   u64 = 79;

/// Syscall sem argumentos
#[inline(always)]
pub unsafe fn syscall0(nr: u64) -> i64 {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: i64;
        asm!(
            "int 0x80",
            inout("rax") nr => ret,
            options(nostack)
        );
        ret
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = nr;
        0
    }
}

/// Syscall com 1 argumento (rdi)
#[inline(always)]
pub unsafe fn syscall1(nr: u64, arg0: u64) -> i64 {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: i64;
        asm!(
            "int 0x80",
            inout("rax") nr => ret,
            in("rdi") arg0,
            options(nostack)
        );
        ret
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = (nr, arg0);
        0
    }
}

/// Syscall com 2 argumentos (rdi, rsi)
#[inline(always)]
pub unsafe fn syscall2(nr: u64, arg0: u64, arg1: u64) -> i64 {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: i64;
        asm!(
            "int 0x80",
            inout("rax") nr => ret,
            in("rdi") arg0,
            in("rsi") arg1,
            options(nostack)
        );
        ret
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = (nr, arg0, arg1);
        0
    }
}

/// Syscall com 3 argumentos (rdi, rsi, rdx)
#[inline(always)]
pub unsafe fn syscall3(nr: u64, arg0: u64, arg1: u64, arg2: u64) -> i64 {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: i64;
        asm!(
            "int 0x80",
            inout("rax") nr => ret,
            in("rdi") arg0,
            in("rsi") arg1,
            in("rdx") arg2,
            options(nostack)
        );
        ret
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = (nr, arg0, arg1, arg2);
        0
    }
}

/// Syscall com 4 argumentos (rdi, rsi, rdx, r10)
#[inline(always)]
pub unsafe fn syscall4(nr: u64, arg0: u64, arg1: u64, arg2: u64, arg3: u64) -> i64 {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: i64;
        asm!(
            "int 0x80",
            inout("rax") nr => ret,
            in("rdi") arg0,
            in("rsi") arg1,
            in("rdx") arg2,
            in("r10") arg3,
            options(nostack)
        );
        ret
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = (nr, arg0, arg1, arg2, arg3);
        0
    }
}

