//! Aladdin Desktop SDK — Primitivas de Syscall em Baixo Nível (x86-64)

use core::arch::asm;

pub const SYS_EXIT:              u64 = 0;
pub const SYS_WRITE:             u64 = 1;
pub const SYS_READ:              u64 = 2;
pub const SYS_YIELD:             u64 = 3;
pub const SYS_GETPID:            u64 = 4;
pub const SYS_IPC_SEND:          u64 = 5;
pub const SYS_IPC_RECV:          u64 = 6;
pub const SYS_REBOOT:            u64 = 7;
pub const SYS_SLEEP:             u64 = 11;
pub const SYS_JDA_SHM_ALLOC:     u64 = 12;
pub const SYS_JDA_SHM_RELEASE:   u64 = 13;
pub const SYS_JDA_SUBMIT:        u64 = 14;
pub const SYS_WINDOW_CREATE:     u64 = 20;
pub const SYS_WINDOW_BLIT:       u64 = 21;
pub const SYS_WINDOW_CLOSE:      u64 = 22;
pub const SYS_WINDOW_POLL_EVENT: u64 = 24;
pub const SYS_JDA_SHM_SHARE:     u64 = 26;
pub const SYS_JDA_SHM_GET_VADDR: u64 = 29;
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
pub const SYS_POWEROFF:                      u64 = 80;
pub const SYS_GRAPHICS_SET_POWER:            u64 = 81;
pub const SYS_HARDWARE_PROBE:                u64 = 82;
pub const SYS_NET_RESOLVE:                   u64 = 83;
pub const SYS_NET_HTTP_GET:                  u64 = 84;
pub const SYS_NET_SET_DNS:                   u64 = 86;
pub const SYS_NET_GET_DNS:                   u64 = 87;
pub const SYS_GET_SYSTEM_STATS:              u64 = 88;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemStatsUser {
    pub uptime_secs: u64,
    pub task_count: u64,
    pub sched_ticks: u64,
    pub used_ram_mb: u64,
    pub total_ram_mb: u64,
    pub current_tick: u64,
    pub net_ip: [u8; 4],
    pub net_adapter: [u8; 32],
    pub net_state_desc: [u8; 32],
    pub jinnwall_active_conns: u64,
    pub jinnwall_drops: u64,
    pub jinnwall_inspected: u64,
    pub jinnwall_accepted: u64,
    pub jinnwall_profile: u32,
    pub physical_frames_used: u64,
    pub physical_frames_total: u64,
    pub dynamic_pages_allocated: u64,
    pub dynamic_pages_freed: u64,
    pub vfs_open_fds: u64,
    pub vfs_mounted_volumes: u64,
}

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

/// Resolve nome de domínio para endereço IPv4 (Syscall 83)
#[inline(always)]
pub unsafe fn sys_net_resolve(name: &[u8], out_ip: &mut [u8; 4]) -> i64 {
    syscall3(SYS_NET_RESOLVE, name.as_ptr() as u64, name.len() as u64, out_ip.as_mut_ptr() as u64)
}

/// Baixa arquivo via HTTP/1.1 diretamente para caminho no VFS (Syscall 84)
#[inline(always)]
pub unsafe fn sys_net_http_get(url: &[u8], out_path: &[u8]) -> i64 {
    syscall4(SYS_NET_HTTP_GET, url.as_ptr() as u64, url.len() as u64, out_path.as_ptr() as u64, out_path.len() as u64)
}

/// Define servidores DNS primário e secundário (Syscall 86)
#[inline(always)]
pub unsafe fn sys_net_set_dns(primary: &[u8; 4], secondary: &[u8; 4]) -> i64 {
    syscall2(SYS_NET_SET_DNS, primary.as_ptr() as u64, secondary.as_ptr() as u64)
}

/// Obtém servidores DNS primário e secundário configurados (Syscall 87)
#[inline(always)]
pub unsafe fn sys_net_get_dns(primary_out: &mut [u8; 4], secondary_out: &mut [u8; 4]) -> i64 {
    syscall2(SYS_NET_GET_DNS, primary_out.as_mut_ptr() as u64, secondary_out.as_mut_ptr() as u64)
}

