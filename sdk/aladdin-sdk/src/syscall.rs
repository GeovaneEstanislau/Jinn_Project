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
pub const SYS_REGISTER_SHARED_RINGS:         u64 = 89;
pub const SYS_RING_DOORBELL:                 u64 = 90;
pub const SYS_READDIR:                       u64 = 91;
pub const SYS_GET_PERCPU_STATS:              u64 = 92;
pub const SYS_NET_DNS_CACHE_LIST:            u64 = 93;
pub const SYS_NET_DNS_HOST_ADD:              u64 = 94;
pub const SYS_NET_DNS_CACHE_FLUSH:           u64 = 95;
pub const SYS_NET_DNS_CONFIG:                u64 = 96;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DnsEntryUser {
    pub hostname: [u8; 128],
    pub hostname_len: u32,
    pub ipv4: [u8; 4],
    pub source_upstream: [u8; 4],
    pub ttl_remaining_secs: u32,
    pub hits: u32,
    pub is_static_host: u32,
    pub is_stale: u32,
}

impl Default for DnsEntryUser {
    fn default() -> Self {
        Self {
            hostname: [0u8; 128],
            hostname_len: 0,
            ipv4: [0u8; 4],
            source_upstream: [0u8; 4],
            ttl_remaining_secs: 0,
            hits: 0,
            is_static_host: 0,
            is_stale: 0,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct PerCpuCoreStat {
    pub cpu_id: u32,
    pub online: u32,
    pub total_cycles: u64,
    pub hlt_cycles: u64,
    pub task_cycles: u64,
    pub irq_cycles: u64,
    pub idle_transitions: u64,
    pub wakeups: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct PerCpuStatsUser {
    pub cpu_count: u64,
    pub net_irq_count: u64,
    pub net_irq_verified: u64,
    pub cores: [PerCpuCoreStat; 8],
}

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
    pub net_tx_packets: u64,
    pub net_rx_packets: u64,
    pub cpu_active_cycles: u64,
    pub cpu_idle_cycles: u64,
    pub syscalls_total: u64,
    pub memory_active_bytes: u64,
    pub memory_idle_bytes: u64,
    pub ring_events_processed: u64,
    pub ring_cmds_flushed: u64,
    pub cpu_count: u64,
    pub per_cpu: [PerCpuCoreStat; 8],
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

/// Lista entradas ativas da Tabela Local DNS (`/disk/hosts` + Cache LRU) (Syscall 93)
#[inline(always)]
pub unsafe fn sys_net_dns_cache_list(out: &mut [DnsEntryUser]) -> i64 {
    syscall2(SYS_NET_DNS_CACHE_LIST, out.as_mut_ptr() as u64, out.len() as u64)
}

/// Adiciona/atualiza host estático em `/disk/hosts` (Syscall 94, flags=0)
#[inline(always)]
pub unsafe fn sys_net_dns_host_add(host: &[u8], ip: &[u8; 4]) -> i64 {
    syscall4(SYS_NET_DNS_HOST_ADD, host.as_ptr() as u64, host.len() as u64, ip.as_ptr() as u64, 0)
}

/// Remove host estático de `/disk/hosts` (Syscall 94, flags=1)
#[inline(always)]
pub unsafe fn sys_net_dns_host_remove(host: &[u8]) -> i64 {
    syscall4(SYS_NET_DNS_HOST_ADD, host.as_ptr() as u64, host.len() as u64, 0, 1)
}

/// Limpa entradas dinâmicas do cache DNS em RAM e disco (Syscall 95)
#[inline(always)]
pub unsafe fn sys_net_dns_cache_flush() -> i64 {
    syscall0(SYS_NET_DNS_CACHE_FLUSH)
}

/// Lê os 3 servidores DNS (primário, secundário e fallback) (Syscall 96, set=0)
#[inline(always)]
pub unsafe fn sys_net_dns_get_servers3(primary_out: &mut [u8; 4], secondary_out: &mut [u8; 4], fallback_out: &mut [u8; 4]) -> i64 {
    syscall4(
        SYS_NET_DNS_CONFIG,
        0,
        primary_out.as_mut_ptr() as u64,
        secondary_out.as_mut_ptr() as u64,
        fallback_out.as_mut_ptr() as u64,
    )
}

/// Define os 3 servidores DNS e salva `/disk/net.cfg` (Syscall 96, set=1)
#[inline(always)]
pub unsafe fn sys_net_dns_set_servers3(primary: &[u8; 4], secondary: &[u8; 4], fallback: &[u8; 4]) -> i64 {
    syscall4(
        SYS_NET_DNS_CONFIG,
        1,
        primary.as_ptr() as u64,
        secondary.as_ptr() as u64,
        fallback.as_ptr() as u64,
    )
}

/// Registra o anel de buffers compartilhado SPSC Ring 3 / Ring 0 (Syscall 89)
#[inline(always)]
pub unsafe fn register_shared_rings(rings_vaddr: usize, backbuffer_vaddr: usize) -> i64 {
    syscall2(SYS_REGISTER_SHARED_RINGS, rings_vaddr as u64, backbuffer_vaddr as u64)
}

/// Aciona a campainha (doorbell) dos anéis compartilhados (Syscall 90)
#[inline(always)]
pub unsafe fn ring_doorbell(flags: u32) -> i64 {
    syscall1(SYS_RING_DOORBELL, flags as u64)
}

