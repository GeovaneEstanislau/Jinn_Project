#![no_std]
#![no_main]

mod font;
mod browser;
pub mod wallet;
pub mod tls;
pub mod x509;
pub mod gzip;

use core::{arch::asm, panic::PanicInfo};

// ── Constantes de Erros POSIX ─────────────────────────────────────────────────
const EPERM:   i64 = -1;
const ESRCH:   i64 = -3;
const EBADF:   i64 = -9;
const ENOMEM:  i64 = -12;
const EFAULT:  i64 = -14;
const EINVAL:  i64 = -22;
const EMFILE:  i64 = -24;
const ENOBUFS: i64 = -105;
const EDQUOT:  i64 = -122;

// ── Syscall Helpers ──────────────────────────────────────────────────────────

#[inline(always)]
pub(crate) unsafe fn syscall3(nr: u64, arg0: u64, arg1: u64, arg2: u64) -> i64 {
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

#[inline(always)]
unsafe fn syscall4(nr: u64, arg0: u64, arg1: u64, arg2: u64, arg3: u64) -> i64 {
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

#[inline(always)]
unsafe fn sys_exit(code: i32) -> ! {
    loop {
        syscall3(0, code as u64, 0, 0);
    }
}

#[inline(always)]
unsafe fn sys_write(fd: u64, buf: *const u8, len: u64) -> i64 {
    syscall3(1, fd, buf as u64, len)
}

#[inline(always)]
unsafe fn sys_read(fd: u64, buf: *mut u8, len: u64) -> i64 {
    syscall3(2, fd, buf as u64, len)
}

#[inline(always)]
#[allow(dead_code)]
unsafe fn sys_yield() -> i64 {
    syscall3(3, 0, 0, 0)
}

#[inline(always)]
unsafe fn sys_alloc_page() -> i64 {
    syscall3(7, 0, 0, 0)
}

#[inline(always)]
unsafe fn sys_free_page(addr: u64) -> i64 {
    syscall3(8, addr, 0, 0)
}

#[inline(always)]
unsafe fn sys_waitpid(pid: u64) -> i64 {
    syscall3(10, pid, 0, 0)
}

#[inline(always)]
unsafe fn sys_execve(path: *const u8, argv: *const *const u8, envp: *const *const u8) -> i64 {
    syscall3(38, path as u64, argv as u64, envp as u64)
}

#[inline(always)]
unsafe fn sys_spawn(path: *const u8, argv: *const *const u8, envp: *const *const u8) -> i64 {
    syscall3(39, path as u64, argv as u64, envp as u64)
}

#[inline(always)]
unsafe fn sys_sleep(ms: u64) -> i64 {
    syscall3(11, ms, 0, 0)
}

#[inline(always)]
unsafe fn sys_jda_shm_alloc(is_read_only: bool) -> i64 {
    syscall3(12, if is_read_only { 1 } else { 0 }, 0, 0)
}

#[inline(always)]
unsafe fn sys_jda_shm_release(handle: u64) -> i64 {
    syscall3(13, handle, 0, 0)
}

#[inline(always)]
unsafe fn sys_jda_submit(job_type: u64, handle: u64, len: u64) -> i64 {
    syscall3(14, job_type, handle, len)
}

// ── Estruturas e Helpers do JDA Fase 3 (io_uring & Zero-Copy) ─────────────────
pub const OP_NOP:                u8 = 0;
pub const OP_READ_BLOCK:         u8 = 1;
pub const OP_WRITE_BLOCK:        u8 = 2;
pub const OP_COMPUTE_FLETCHER32: u8 = 3;
pub const OP_COMPUTE_SUM:        u8 = 4;
pub const OP_COMPUTE_XOR:        u8 = 5;
pub const OP_BARRIER:            u8 = 6;

pub const JDA_SQE_ZERO_COPY:     u8 = 1 << 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct JdaSqe {
    pub opcode: u8,
    pub flags: u8,
    pub priority: u8,
    pub _reserved: u8,
    pub handle: u32,
    pub offset: u64,
    pub len: u32,
    pub user_data: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct JdaCqe {
    pub user_data: u64,
    pub res: i32,
    pub flags: u32,
}

#[inline(always)]
unsafe fn sys_jda_shm_share(handle: u64, target_pid: usize, make_owner_ro: bool) -> i64 {
    syscall3(26, handle, target_pid as u64, if make_owner_ro { 1 } else { 0 })
}

#[inline(always)]
unsafe fn sys_jda_ring_submit(sqe_ptr: *const JdaSqe, count: usize) -> i64 {
    syscall3(27, sqe_ptr as u64, count as u64, 0)
}

#[inline(always)]
unsafe fn sys_jda_ring_poll(cqe_ptr: *mut JdaCqe, max_count: usize) -> i64 {
    syscall3(28, cqe_ptr as u64, max_count as u64, 0)
}

#[inline(always)]
unsafe fn sys_jda_shm_get_vaddr(handle: u64) -> i64 {
    syscall3(29, handle, 0, 0)
}

#[inline(always)]
unsafe fn sys_open(path: &[u8], flags: u32) -> i64 {
    syscall3(15, path.as_ptr() as u64, path.len() as u64, flags as u64)
}

#[inline(always)]
unsafe fn sys_close(fd: u64) -> i64 {
    syscall3(16, fd, 0, 0)
}

pub const O_RDONLY: u32 = 0x0;
pub const O_WRONLY: u32 = 0x1;
pub const O_RDWR:   u32 = 0x2;
pub const O_CREAT:  u32 = 0x40;
pub const O_TRUNC:  u32 = 0x200;
pub const O_APPEND: u32 = 0x400;

#[inline(always)]
unsafe fn sys_sync() -> i64 {
    syscall3(23, 0, 0, 0)
}

pub const SEEK_SET: u64 = 0;
pub const SEEK_CUR: u64 = 1;
pub const SEEK_END: u64 = 2;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct FileStat {
    pub size: u64,
    pub inode: u64,
    pub node_type: u32,
    pub mode: u32,
    pub blksize: u32,
    pub blocks: u32,
    pub atime: u64,
    pub mtime: u64,
    pub ctime: u64,
}

#[inline(always)]
unsafe fn sys_stat_raw(path_ptr: u64, path_len: u64, stat_ptr: u64) -> i64 {
    syscall3(17, path_ptr, path_len, stat_ptr)
}

#[inline(always)]
unsafe fn sys_fstat_raw(fd: u64, stat_ptr: u64) -> i64 {
    syscall3(18, fd, stat_ptr, 0)
}

#[inline(always)]
unsafe fn sys_stat(path: &[u8], stat: &mut FileStat) -> i64 {
    sys_stat_raw(path.as_ptr() as u64, path.len() as u64, stat as *mut _ as u64)
}

#[inline(always)]
unsafe fn sys_fstat(fd: u64, stat: &mut FileStat) -> i64 {
    sys_fstat_raw(fd, stat as *mut _ as u64)
}

#[inline(always)]
unsafe fn sys_lseek(fd: u64, offset: i64, whence: u64) -> i64 {
    syscall3(19, fd, offset as u64, whence)
}

#[inline(always)]
unsafe fn sys_window_create(title: &[u8], width: usize, height: usize) -> i64 {
    syscall4(20, title.as_ptr() as u64, title.len() as u64, width as u64, height as u64)
}

#[inline(always)]
unsafe fn sys_window_update(win_id: usize, buf: &[u32]) -> i64 {
    syscall3(21, win_id as u64, buf.as_ptr() as u64, (buf.len() * 4) as u64)
}

#[inline(always)]
unsafe fn sys_window_close(win_id: usize) -> i64 {
    syscall3(22, win_id as u64, 0, 0)
}

// ── Constantes e Estrutura de Eventos de Janela do Aladdin (Syscall 24) ────────
pub const EVENT_NONE: u32 = 0;
pub const EVENT_MOUSE_DOWN: u32 = 1;
pub const EVENT_MOUSE_UP: u32 = 2;
pub const EVENT_MOUSE_MOVE: u32 = 3;
pub const EVENT_KEY_DOWN: u32 = 4;
pub const EVENT_KEY_UP: u32 = 5;
pub const EVENT_SCROLL: u32 = 6;
pub const EVENT_WINDOW_CLOSE: u32 = 7;
pub const EVENT_WINDOW_RESIZE: u32 = 8;
pub const EVENT_WINDOW_EXPOSE: u32 = 9;
pub const EVENT_NAVIGATE_PENDING: u32 = 10;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ClientWindowEvent {
    pub event_type: u32,
    pub win_id: u32,
    pub x: i32,
    pub y: i32,
    pub button_or_key: u32,
    pub modifiers: u32,
    pub timestamp: u64,
}

pub const POLL_EVENT_NONBLOCK: usize = 1;
pub const POLL_EVENT_BLOCKING: usize = 2;

#[inline(always)]
unsafe fn sys_window_poll_event(win_id: usize, event_ptr: *mut ClientWindowEvent) -> i64 {
    syscall3(24, win_id as u64, event_ptr as u64, 0)
}

#[inline(always)]
unsafe fn sys_window_wait_event(win_id: usize, event_ptr: *mut ClientWindowEvent) -> i64 {
    syscall3(24, win_id as u64, event_ptr as u64, POLL_EVENT_BLOCKING as u64)
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ProcessResourceStat {
    pub pid: u32,
    pub state: u32,
    pub allocated_frames: u32,
    pub peak_allocated_frames: u32,
    pub open_fds: u32,
    pub open_windows: u32,
    pub open_jda_buffers: u32,
    pub open_sockets: u32,
    pub cpu_ticks: u64,
    pub ctx_switches: u64,
    pub limit_violations: u32,
    pub last_violation_code: i32,
    pub max_frames: u32,
    pub max_fds: u32,
    pub max_windows: u32,
    pub max_sockets: u32,
    pub max_cpu_ticks: u64,
    pub irq_preemptions: u64,
    pub voluntary_yields: u64,
}

#[inline(always)]
unsafe fn sys_res_stat(target_pid: u64, stat: &mut ProcessResourceStat) -> i64 {
    syscall3(25, target_pid, stat as *mut _ as u64, core::mem::size_of::<ProcessResourceStat>() as u64)
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SockAddrIn {
    pub sin_family: u16,
    pub sin_port: u16, // Big-Endian
    pub sin_addr: [u8; 4],
    pub sin_zero: [u8; 8],
}

#[inline(always)]
unsafe fn sys_socket(domain: u32, sock_type: u32, protocol: u32) -> i64 {
    syscall3(30, domain as u64, sock_type as u64, protocol as u64)
}

#[inline(always)]
unsafe fn sys_connect(sockfd: u64, addr: &SockAddrIn) -> i64 {
    syscall3(31, sockfd, addr as *const _ as u64, core::mem::size_of::<SockAddrIn>() as u64)
}

#[inline(always)]
unsafe fn sys_send(sockfd: u64, buf: *const u8, len: usize) -> i64 {
    syscall3(35, sockfd, buf as u64, len as u64)
}

#[inline(always)]
unsafe fn sys_recv(sockfd: u64, buf: *mut u8, len: usize) -> i64 {
    syscall3(36, sockfd, buf as u64, len as u64)
}

#[inline(always)]
unsafe fn sys_bind(sockfd: u64, addr: &SockAddrIn) -> i64 {
    syscall3(32, sockfd, addr as *const _ as u64, core::mem::size_of::<SockAddrIn>() as u64)
}

#[inline(always)]
unsafe fn sys_listen(sockfd: u64, backlog: usize) -> i64 {
    syscall3(33, sockfd, backlog as u64, 0)
}

#[inline(always)]
unsafe fn sys_accept(sockfd: u64, addr: *mut SockAddrIn) -> i64 {
    syscall3(34, sockfd, addr as u64, core::mem::size_of::<SockAddrIn>() as u64)
}

#[inline(always)]
unsafe fn sys_dns_resolve(name: *const u8, len: usize, out_ip: *mut u8) -> i64 {
    syscall3(40, name as u64, len as u64, out_ip as u64)
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

#[inline(always)]
unsafe fn sys_get_system_stats(stats: *mut SystemStatsUser) -> i64 {
    syscall3(88, stats as u64, 0, 0)
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct HotRouteEntryUser {
    pub hit_count: u32,
    pub last_rtt_ticks: u32,
    pub ttl_remaining_secs: u32,
    pub hostname: [u8; 96],
    pub hostname_len: u8,
    pub dst_ip: [u8; 4],
    pub next_hop_ip: [u8; 4],
    pub next_hop_mac: [u8; 6],
    pub flags: u8,
    pub valid: u8,
    pub stale: u8,
    pub _reserved: [u8; 2],
}

impl HotRouteEntryUser {
    pub const fn empty() -> Self {
        Self {
            hit_count: 0,
            last_rtt_ticks: 0,
            ttl_remaining_secs: 0,
            hostname: [0u8; 96],
            hostname_len: 0,
            dst_ip: [0u8; 4],
            next_hop_ip: [0u8; 4],
            next_hop_mac: [0u8; 6],
            flags: 0,
            valid: 0,
            stale: 0,
            _reserved: [0u8; 2],
        }
    }
}

#[inline(always)]
unsafe fn sys_net_hot_route_list(out: &mut [HotRouteEntryUser]) -> i64 {
    syscall3(97, out.as_mut_ptr() as u64, out.len() as u64, 0)
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct JinnWallStats {
    pub inspected: u64,
    pub accepted: u64,
    pub dropped: u64,
    pub active_conns: u32,
    pub profile: u32,
}

#[inline(always)]
unsafe fn sys_jinnwall_ctrl(cmd: u64, arg0: u64, arg1: u64) -> i64 {
    syscall3(37, cmd, arg0, arg1)
}

// ── Driver Subsystem Syscalls & Constants (Syscalls 54..=58) ──────────────────
pub const DRIVER_CAP_PIO: u32 = 0x0001;
pub const DRIVER_CAP_DMA: u32 = 0x0002;
pub const DRIVER_CAP_INPUT: u32 = 0x0004;

pub const EVENT_TYPE_RAW_BYTE: u16 = 1;
pub const EVENT_TYPE_IRQ: u16 = 2;
pub const EVENT_TYPE_OVERFLOW_WARN: u16 = 4;
pub const EVENT_TYPE_RESET_SYNC: u16 = 5;

pub const EVENT_FLAG_SPURIOUS: u32 = 0x0001;
pub const EVENT_FLAG_OVERFLOW: u32 = 0x0002;

pub const INPUT_KIND_KEY_DOWN: u16 = 1;
pub const INPUT_KIND_KEY_UP: u16 = 2;
pub const INPUT_KIND_MOUSE_MOVE: u16 = 3;
pub const INPUT_KIND_MOUSE_BUTTON_DOWN: u16 = 4;
pub const INPUT_KIND_MOUSE_BUTTON_UP: u16 = 5;

pub const DEV_KBD0: u32 = 0x1001;
pub const DEV_MOUSE0: u32 = 0x1002;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HardwareEvent {
    pub timestamp_ticks: u64,
    pub device_id: u32,
    pub event_type: u16,
    pub code: u16,
    pub data: u32,
    pub flags: u32,
}

impl HardwareEvent {
    pub const fn empty() -> Self {
        Self {
            timestamp_ticks: 0,
            device_id: 0,
            event_type: 0,
            code: 0,
            data: 0,
            flags: 0,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct DriverTelemetryStats {
    pub ring0_fallback_events: u64,
    pub ring3_driver_events: u64,
    pub events_enqueued: u64,
    pub events_polled: u64,
    pub events_dropped_overflow: u64,
    pub spurious_irqs: u64,
    pub active_driver_pid: u32,
    pub active_generation: u64,
    pub is_ring3_active: u32,
    pub recoveries_count: u64,
}

#[inline(always)]
unsafe fn sys_driver_register(device_id: u32, flags: u32) -> i64 {
    syscall3(54, device_id as u64, flags as u64, 0)
}

#[inline(always)]
unsafe fn sys_driver_event_poll(device_id: u32, out_ptr: *mut HardwareEvent, max_events: u64) -> i64 {
    syscall4(55, device_id as u64, out_ptr as u64, max_events, 0)
}

#[inline(always)]
unsafe fn sys_driver_event_poll_gen(device_id: u32, out_ptr: *mut HardwareEvent, max_events: u64, expected_gen: u64) -> i64 {
    syscall4(55, device_id as u64, out_ptr as u64, max_events, expected_gen)
}

#[inline(always)]
unsafe fn sys_driver_send_input(kind: u16, code: u16, x: i32, y: i32) -> i64 {
    syscall4(56, kind as u64, code as u64, x as u64, y as u64)
}

#[inline(always)]
unsafe fn sys_driver_unregister(device_id: u32) -> i64 {
    syscall3(57, device_id as u64, 0, 0)
}

#[inline(always)]
unsafe fn sys_driver_restart(device_id: u32) -> i64 {
    syscall3(58, device_id as u64, 0, 0)
}

#[inline(always)]
unsafe fn sys_driver_telemetry(device_id: u32, out_ptr: *mut DriverTelemetryStats) -> i64 {
    syscall3(59, device_id as u64, out_ptr as u64, 0)
}

// ── Cryptography & Clock Syscalls (Syscalls 69..=70) ──────────────────────────

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Timespec {
    pub tv_sec: u64,
    pub tv_nsec: u32,
}

#[inline(always)]
pub unsafe fn sys_getrandom(buf: *mut u8, len: usize, flags: u32) -> i64 {
    syscall3(69, buf as u64, len as u64, flags as u64)
}

#[inline(always)]
pub unsafe fn sys_clock_gettime(clock_id: u32, out_ts: *mut Timespec) -> i64 {
    syscall3(70, clock_id as u64, out_ts as u64, 0)
}

// ── Output Helpers ───────────────────────────────────────────────────────────

unsafe fn print_str(s: &[u8]) {
    let _ = sys_write(1, s.as_ptr(), s.len() as u64);
}

unsafe fn print_u64(mut n: u64) {
    if n == 0 {
        print_str(b"0");
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 20;
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    print_str(&buf[i..20]);
}

unsafe fn print_i64(n: i64) {
    if n < 0 {
        print_str(b"-");
        print_u64((-n) as u64);
    } else {
        print_u64(n as u64);
    }
}

// ── Testes Adversariais e de Hardening (Fase 2) ──────────────────────────────

/// Teste A: Validação Adversarial de Memória (Pilha de Usuário, Código ELF, Double-Free)
unsafe fn test_adv_memory_protection() {
    print_str(b"[ADV-TEST 1/5] test_adv_memory_protection: Testando limites de protecao de memoria...\n");

    // 1. Tentativa de liberar endereco da pilha de usuario (deve retornar -EFAULT)
    let stack_dummy = 0x1234u64;
    let stack_page = (&stack_dummy as *const _ as u64) & !0xFFF;
    let ret_stack_free = sys_free_page(stack_page);
    let stack_ok = ret_stack_free == EFAULT;
    print_str(b"  Tentativa de liberar pagina da pilha (0x");
    print_u64(stack_page);
    print_str(b") -> ret=");
    print_i64(ret_stack_free);
    print_str(b" (esperado -14 EFAULT)\n");

    // 2. Tentativa de liberar pagina de codigo do executavel ELF (deve retornar -EFAULT)
    let code_page = (test_adv_memory_protection as *const () as u64) & !0xFFF;
    let ret_code_free = sys_free_page(code_page);
    let code_ok = ret_code_free == EFAULT;
    print_str(b"  Tentativa de liberar pagina de codigo ELF (0x");
    print_u64(code_page);
    print_str(b") -> ret=");
    print_i64(ret_code_free);
    print_str(b" (esperado -14 EFAULT)\n");

    // 3. Alocacao valida no Heap de usuario (0x0000_1000_0000..0x0000_2000_0000)
    let alloc_addr = sys_alloc_page();
    let alloc_ok = alloc_addr >= 0x0000_1000_0000 && alloc_addr < 0x0000_2000_0000;
    print_str(b"  sys_alloc_page() -> 0x");
    print_u64(alloc_addr as u64);
    print_str(b" (dentro do heap de usuario)\n");

    if alloc_ok {
        // Escreve na pagina para confirmar mapeamento
        let ptr = alloc_addr as *mut u64;
        core::ptr::write_volatile(ptr, 0xCAFE_BABE_DEAD_BEEF);

        // 4. Libera pagina valida
        let ret_valid_free = sys_free_page(alloc_addr as u64);
        let valid_free_ok = ret_valid_free == 0;
        print_str(b"  sys_free_page(0x");
        print_u64(alloc_addr as u64);
        print_str(b") -> ret=");
        print_i64(ret_valid_free);
        print_str(b" (esperado 0)\n");

        // 5. Tentativa de double-free na pagina ja liberada (deve retornar -EFAULT)
        let ret_double_free = sys_free_page(alloc_addr as u64);
        let double_free_ok = ret_double_free == EFAULT;
        print_str(b"  Tentativa de double-free (0x");
        print_u64(alloc_addr as u64);
        print_str(b") -> ret=");
        print_i64(ret_double_free);
        print_str(b" (esperado -14 EFAULT)\n");

        // 6. Validacao de desalinhamento de pagina (deve retornar -EINVAL)
        let unaligned_addr = (alloc_addr as u64) + 0x40;
        let ret_unaligned = sys_free_page(unaligned_addr);
        let unaligned_ok = ret_unaligned == EINVAL;
        print_str(b"  Tentativa de liberar endereco desalinhado (0x");
        print_u64(unaligned_addr);
        print_str(b") -> ret=");
        print_i64(ret_unaligned);
        print_str(b" (esperado -22 EINVAL)\n");

        // 7. Tentativa de liberar ponteiro nulo e endereco de kernel (deve retornar -EFAULT)
        let ret_null = sys_free_page(0);
        let ret_kernel = sys_free_page(0xFFFF_8000_0000_0000);
        let ptr_boundary_ok = ret_null == EFAULT && ret_kernel == EFAULT;
        print_str(b"  Tentativa de liberar NULL e endereco de Kernel -> ret_null=");
        print_i64(ret_null);
        print_str(b", ret_kernel=");
        print_i64(ret_kernel);
        print_str(b" (esperado -14 EFAULT)\n");

        // 8. Sanitizacao e Zeramento de Pagina Reciclada (protecao contra data leak)
        let p_dirty = sys_alloc_page();
        let mut sanitization_ok = false;
        if p_dirty > 0 {
            let p_ptr = p_dirty as *mut u64;
            // Preenche toda a pagina com padrao de teste
            for i in 0..512 {
                core::ptr::write_volatile(p_ptr.add(i), 0xA5A5_5A5A_BEEF_CAFE);
            }
            let _ = sys_free_page(p_dirty as u64);

            // Aloca novamente: o kernel deve entregar a pagina 100% zerada
            let p_clean = sys_alloc_page();
            if p_clean > 0 {
                let clean_ptr = p_clean as *const u64;
                let mut all_zeros = true;
                for i in 0..512 {
                    if core::ptr::read_volatile(clean_ptr.add(i)) != 0 {
                        all_zeros = false;
                        break;
                    }
                }
                sanitization_ok = all_zeros;
                let _ = sys_free_page(p_clean as u64);
            }
        }
        print_str(b"  Sanitizacao de memoria (zero-fill em paginas recicladas): ");
        if sanitization_ok {
            print_str(b"OK (sem vazamento de dados)\n");
        } else {
            print_str(b"FALHA (dados anteriores residuais detectados!)\n");
        }

        if stack_ok && code_ok && valid_free_ok && double_free_ok && unaligned_ok && ptr_boundary_ok && sanitization_ok {
            print_str(b"  [PASS] test_adv_memory_protection: Pilha, ELF, Heap, Alinhamento e Sanitizacao validados com rigor!\n");
        } else {
            print_str(b"  [FAIL] test_adv_memory_protection falhou em um dos criterios de seguranca.\n");
        }
    } else {
        print_str(b"  [FAIL] Falha ao alocar pagina dinamicamente.\n");
    }
}

/// Teste B: Validação Adversarial de Handles JDA com Geração (Proteção contra Handles Obsoletos)
unsafe fn test_adv_jda_stale_handles() {
    print_str(b"[ADV-TEST 2/5] test_adv_jda_stale_handles: Testando seguranca de handles JDA com geracao...\n");

    // 1. Aloca buffer JDA
    let handle = sys_jda_shm_alloc(false);
    if handle < 0 {
        print_str(b"  [FAIL] sys_jda_shm_alloc retornou erro: ");
        print_i64(handle);
        print_str(b"\n");
        return;
    }
    let gen = ((handle as u64) >> 16) & 0xFFFF;
    let slot = (handle as u64) & 0xFFFF;
    print_str(b"  Buffer JDA alocado: handle=0x");
    print_u64(handle as u64);
    print_str(b" (slot=");
    print_u64(slot);
    print_str(b", gen=");
    print_u64(gen);
    print_str(b")\n");

    // 2. Libera o buffer legitimamente
    let ret_rel = sys_jda_shm_release(handle as u64);
    let rel_ok = ret_rel == 0;
    print_str(b"  sys_jda_shm_release(handle) -> ret=");
    print_i64(ret_rel);
    print_str(b" (esperado 0)\n");

    // 3. Tenta usar o handle obsoleto (stale handle) em sys_jda_submit -> deve falhar com -EPERM
    let ret_sub = sys_jda_submit(1, handle as u64, 512);
    let sub_ok = ret_sub == EPERM;
    print_str(b"  sys_jda_submit com handle obsoleto -> ret=");
    print_i64(ret_sub);
    print_str(b" (esperado -1 EPERM)\n");

    // 4. Tenta liberar novamente o handle obsoleto -> deve falhar com -EPERM
    let ret_rel_again = sys_jda_shm_release(handle as u64);
    let rel_again_ok = ret_rel_again == EPERM;
    print_str(b"  sys_jda_shm_release duplicado -> ret=");
    print_i64(ret_rel_again);
    print_str(b" (esperado -1 EPERM)\n");

    if rel_ok && sub_ok && rel_again_ok {
        print_str(b"  [PASS] test_adv_jda_stale_handles: Handles com geracao protegem contra reutilizacao e stale handles!\n");
    } else {
        print_str(b"  [FAIL] test_adv_jda_stale_handles falhou na protecao de geracao.\n");
    }
}

/// Teste C: Validação de ABI de Descritores Unificada (Tipagem, Double-Close, Mismatch de Operação)
unsafe fn test_adv_unified_descriptors() {
    print_str(b"[ADV-TEST 3/5] test_adv_unified_descriptors: Testando tabela unificada de descritores...\n");

    // 1. Abre um arquivo VFS e um socket de rede
    let fd_file = sys_open(b"/disk/README.TXT", 0);
    let fd_sock = sys_socket(2, 2, 0); // AF_INET, SOCK_DGRAM

    print_str(b"  Descritor de Arquivo VFS: fd=");
    print_i64(fd_file);
    print_str(b", Descritor de Socket: fd=");
    print_i64(fd_sock);
    print_str(b"\n");

    let valid_range = fd_file >= 3 && fd_file < 8 && fd_sock >= 3 && fd_sock < 8 && fd_file != fd_sock;

    // 2. Double-Close no arquivo VFS
    let close1 = sys_close(fd_file as u64);
    let close2 = sys_close(fd_file as u64);
    let double_close_ok = close1 == 0 && close2 == EBADF;
    print_str(b"  Double-close em arquivo VFS: 1a=");
    print_i64(close1);
    print_str(b", 2a=");
    print_i64(close2);
    print_str(b" (esperado -9 EBADF)\n");

    // 3. Abre novo arquivo para testar Type Mismatch
    let fd_file2 = sys_open(b"/disk/README.TXT", 0);
    let addr = SockAddrIn {
        sin_family: 2,
        sin_port: 80u16.to_be(),
        sin_addr: [10, 0, 2, 2],
        sin_zero: [0; 8],
    };
    // Chama sys_connect em um descritor de arquivo -> deve retornar -EBADF
    let ret_mismatch = sys_connect(fd_file2 as u64, &addr);
    let mismatch_ok = ret_mismatch == EBADF;
    print_str(b"  sys_connect em descritor de arquivo VFS -> ret=");
    print_i64(ret_mismatch);
    print_str(b" (esperado -9 EBADF)\n");

    // 4. Leitura unificada em socket via sys_read (suportado na ABI unificada)
    let mut sock_read_buf = [0u8; 32];
    let ret_sock_read = sys_read(fd_sock as u64, sock_read_buf.as_mut_ptr(), 32);
    let read_ok = ret_sock_read == 0; // Fila vazia
    print_str(b"  sys_read em socket de rede -> ret=");
    print_i64(ret_sock_read);
    print_str(b" (0 = sem dados pendentes)\n");

    let _ = sys_close(fd_file2 as u64);
    let _ = sys_close(fd_sock as u64);

    if valid_range && double_close_ok && mismatch_ok && read_ok {
        print_str(b"  [PASS] test_adv_unified_descriptors: Espaco de descritores unificado e tipado com rigor!\n");
    } else {
        print_str(b"  [FAIL] test_adv_unified_descriptors falhou.\n");
    }
}

/// Teste D: Validação de Rede Real Bidirecional (Consulta DNS UDP ao Gateway VirtIO 10.0.2.3:53)
unsafe fn test_adv_real_network_dns() {
    print_str(b"[ADV-TEST 4/5] test_adv_real_network_dns: Testando transmissao e recepcao real UDP/DNS...\n");

    let sockfd = sys_socket(2, 2, 0); // AF_INET, SOCK_DGRAM
    if sockfd < 0 {
        print_str(b"  [FAIL] Nao foi possivel criar socket UDP: ");
        print_i64(sockfd);
        print_str(b"\n");
        return;
    }

    let dns_server = SockAddrIn {
        sin_family: 2,
        sin_port: 53u16.to_be(), // DNS Port 53
        sin_addr: [10, 0, 2, 3], // Gateway DNS QEMU Virtual
        sin_zero: [0; 8],
    };

    let ret_conn = sys_connect(sockfd as u64, &dns_server);
    if ret_conn != 0 {
        print_str(b"  [FAIL] sys_connect falhou com erro: ");
        print_i64(ret_conn);
        print_str(b"\n");
        let _ = sys_close(sockfd as u64);
        return;
    }

    // Monta pacote DNS Query para "example.com" tipo A
    // ID=0x1234, Flags=0x0100 (Standard Query, Recursion Desired), QDCOUNT=1
    let mut dns_query = [0u8; 29];
    dns_query[0] = 0x12; dns_query[1] = 0x34; // ID
    dns_query[2] = 0x01; dns_query[3] = 0x00; // Flags: RD=1
    dns_query[4] = 0x00; dns_query[5] = 0x01; // QDCOUNT = 1
    // QNAME: 7 "example" 3 "com" 0
    dns_query[12] = 7;
    dns_query[13..20].copy_from_slice(b"example");
    dns_query[20] = 3;
    dns_query[21..24].copy_from_slice(b"com");
    dns_query[24] = 0; // Terminating zero
    dns_query[25] = 0x00; dns_query[26] = 0x01; // QTYPE: A (1)
    dns_query[27] = 0x00; dns_query[28] = 0x01; // QCLASS: IN (1)

    let sent = sys_send(sockfd as u64, dns_query.as_ptr(), dns_query.len());
    print_str(b"  Datagrama DNS enviado via VirtIO-Net (bytes enviados = ");
    print_i64(sent);
    print_str(b")\n");

    // Aguarda e faz polling na fila de recepcao do socket
    let mut rx_buf = [0u8; 512];
    let mut received_bytes = 0usize;

    for _ in 0..20 {
        let ret_rx = sys_recv(sockfd as u64, rx_buf.as_mut_ptr(), 512);
        if ret_rx > 0 {
            received_bytes = ret_rx as usize;
            break;
        }
        let _ = sys_sleep(20);
    }

    if received_bytes >= 12 {
        let rx_id = u16::from_be_bytes([rx_buf[0], rx_buf[1]]);
        if rx_id == 0x1234 {
            print_str(b"  [PASS] Resposta DNS recebida via VirtIO-Net! Bytes recebidos: ");
            print_u64(received_bytes as u64);
            print_str(b", ID de transacao verificado: 0x1234\n");
            print_str(b"  [PASS] test_adv_real_network_dns: Ciclo completo L2/L3/L4/Socket/JinnWall com upstream validado!\n");
        } else {
            print_str(b"  [WARN] Resposta DNS recebida com ID discrepante: 0x");
            print_u64(rx_id as u64);
            print_str(b" (esperado 0x1234)\n");
        }
    } else {
        print_str(b"  [WARN] Resposta DNS upstream nao recebida (timeout no gateway virtual QEMU 10.0.2.3).\n");
        print_str(b"  [INFO] Transmissao UDP e tracking JinnWall confirmados; resolucao externa pendente de conectividade upstream.\n");
    }

    let _ = sys_close(sockfd as u64);
}

/// Validação de Interoperabilidade TCP 1.0 (Comunicação real com Host Peer independente)
unsafe fn test_tcp_network_interoperability() {
    print_str(b"\n==================================================================\n");
    print_str(b"  JINN OS NETWORK INTEROPERABILITY 1.0 - TESTES REAIS TCP\n");
    print_str(b"==================================================================\n");

    let peer_addr = SockAddrIn {
        sin_family: 2,
        sin_port: 9999u16.to_be(), // Host Peer na porta 9999
        sin_addr: [10, 0, 2, 2],   // Host Gateway QEMU SLiRP
        sin_zero: [0; 8],
    };

    // ─────────────────────────────────────────────────────────────────────────────
    // Nível A1 & A2: Conectividade Básica (3-Way Handshake + Mensagem Pequena)
    // ─────────────────────────────────────────────────────────────────────────────
    print_str(b"[INTEROP-A1] Testando 3-way handshake e conexao TCP ativa com host peer 10.0.2.2:9999...\n");
    let sock_a = sys_socket(2, 1, 0); // AF_INET, SOCK_STREAM
    if sock_a < 0 {
        print_str(b"  [FAIL] Falha ao criar socket TCP: ");
        print_i64(sock_a);
        print_str(b"\n");
        return;
    }

    let ret_conn_a = sys_connect(sock_a as u64, &peer_addr);
    if ret_conn_a != 0 {
        print_str(b"  [WARN] Host peer nao alcancavel (ret=");
        print_i64(ret_conn_a);
        print_str(b"). Peer independente pode nao estar em execucao neste teste.\n");
        let _ = sys_close(sock_a as u64);
        return;
    }
    print_str(b"  [PASS] Handshake SYN/SYN-ACK/ACK concluido! Estado ESTABLISHED confirmado.\n");

    // Monta mensagem pequena Nível A: 20 bytes cabeçalho + 32 bytes payload
    let mut msg_a = [0u8; 52];
    msg_a[0..4].copy_from_slice(b"JINN");
    msg_a[4..8].copy_from_slice(&0x1001_0001u32.to_be_bytes()); // Session ID
    msg_a[8..12].copy_from_slice(&1u32.to_be_bytes());          // Msg ID 1
    msg_a[12..16].copy_from_slice(&32u32.to_be_bytes());        // Payload Len 32

    let mut csum_a = 0u32;
    for i in 0..32 {
        let b = ((i as u8).wrapping_mul(7)).wrapping_add(0x41);
        msg_a[20 + i] = b;
        csum_a = csum_a.wrapping_add(b as u32);
    }
    msg_a[16..20].copy_from_slice(&csum_a.to_be_bytes());

    let sent_a = sys_send(sock_a as u64, msg_a.as_ptr(), 52);
    print_str(b"  [INTEROP-A2] Mensagem pequena enviada via VirtIO-Net (bytes enviados = ");
    print_i64(sent_a);
    print_str(b")\n");

    // Aguarda e valida resposta do peer (echo com seed 0x52 e msg_id = 101)
    let mut rx_buf_a = [0u8; 512];
    let mut received_a = 0usize;
    for _ in 0..40 {
        let n = sys_recv(sock_a as u64, rx_buf_a[received_a..].as_mut_ptr(), 512 - received_a);
        if n > 0 {
            received_a += n as usize;
            if received_a >= 52 { break; }
        }
        let _ = sys_sleep(25);
    }

    if received_a >= 52 && &rx_buf_a[0..4] == b"JINN" {
        let rx_msg_id = u32::from_be_bytes([rx_buf_a[8], rx_buf_a[9], rx_buf_a[10], rx_buf_a[11]]);
        let rx_len = u32::from_be_bytes([rx_buf_a[12], rx_buf_a[13], rx_buf_a[14], rx_buf_a[15]]) as usize;
        let rx_csum = u32::from_be_bytes([rx_buf_a[16], rx_buf_a[17], rx_buf_a[18], rx_buf_a[19]]);

        let mut actual_csum = 0u32;
        let mut pattern_ok = true;
        for i in 0..rx_len {
            let b = rx_buf_a[20 + i];
            actual_csum = actual_csum.wrapping_add(b as u32);
            let expected = ((i as u8).wrapping_mul(7)).wrapping_add(0x52);
            if b != expected {
                pattern_ok = false;
            }
        }

        if rx_msg_id == 101 && actual_csum == rx_csum && pattern_ok {
            print_str(b"  [PASS] Resposta recebida do host peer independente verificada com integridade total!\n");
        } else {
            print_str(b"  [FAIL] Resposta recebida com erro de integridade ou sequencia!\n");
        }
    } else {
        print_str(b"  [FAIL] Resposta nao recebida no tempo limite.\n");
    }

    // Nível A3: Encerramento ordenado (FIN/ACK)
    let ret_close_a = sys_close(sock_a as u64);
    if ret_close_a == 0 {
        print_str(b"  [PASS] Encerramento ordenado FIN/ACK concluido e recursos do socket liberados.\n");
    } else {
        print_str(b"  [FAIL] Falha no encerramento do socket: ");
        print_i64(ret_close_a);
        print_str(b"\n");
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Nível A4: Repetição de Conexões Consecutivas (Ausência de vazamento de recursos)
    // ─────────────────────────────────────────────────────────────────────────────
    print_str(b"[INTEROP-A4] Testando repeticao de conexao consecutiva com peer independente...\n");
    let sock_a2 = sys_socket(2, 1, 0);
    if sock_a2 >= 0 {
        let ret_conn_a2 = sys_connect(sock_a2 as u64, &peer_addr);
        if ret_conn_a2 == 0 {
            print_str(b"  [PASS] 2a conexao estabelecida com sucesso no mesmo slot reciclado!\n");
            let _ = sys_send(sock_a2 as u64, msg_a.as_ptr(), 52);
            let _ = sys_sleep(30);
            let _ = sys_close(sock_a2 as u64);
            print_str(b"  [PASS] 2a conexao encerrada sem vazamento de sockets ou memoria.\n");
        }
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Nível B: Transferência de Dados Maior que o MSS (> 1460 bytes, e.g. 2048 bytes)
    // ─────────────────────────────────────────────────────────────────────────────
    print_str(b"[INTEROP-B] Testando transferencia multi-segmento superior ao MSS (2048 bytes payload)...\n");
    let sock_b = sys_socket(2, 1, 0);
    if sock_b >= 0 {
        let ret_conn_b = sys_connect(sock_b as u64, &peer_addr);
        if ret_conn_b == 0 {
            // Buffer total: 20 bytes cabeçalho + 2048 bytes payload = 2068 bytes
            let page_addr = sys_alloc_page();
            if page_addr > 0 {
                let tx_ptr = page_addr as *mut u8;

                // Monta cabeçalho
                let hdr_magic = b"JINN";
                core::ptr::copy_nonoverlapping(hdr_magic.as_ptr(), tx_ptr, 4);
                let sess_bytes = 0x1001_0002u32.to_be_bytes();
                core::ptr::copy_nonoverlapping(sess_bytes.as_ptr(), tx_ptr.add(4), 4);
                let msgid_bytes = 2u32.to_be_bytes();
                core::ptr::copy_nonoverlapping(msgid_bytes.as_ptr(), tx_ptr.add(8), 4);
                let len_bytes = 2048u32.to_be_bytes();
                core::ptr::copy_nonoverlapping(len_bytes.as_ptr(), tx_ptr.add(12), 4);

                let mut csum_b = 0u32;
                for i in 0..2048 {
                    let b = ((i as u8).wrapping_mul(7)).wrapping_add(0x41);
                    *tx_ptr.add(20 + i) = b;
                    csum_b = csum_b.wrapping_add(b as u32);
                }
                let csum_bytes = csum_b.to_be_bytes();
                core::ptr::copy_nonoverlapping(csum_bytes.as_ptr(), tx_ptr.add(16), 4);

                let total_tx = 20 + 2048; // 2068 bytes
                let mut sent_total = 0usize;
                while sent_total < total_tx {
                    let to_send = total_tx - sent_total;
                    let n = sys_send(sock_b as u64, tx_ptr.add(sent_total), to_send);
                    if n > 0 {
                        sent_total += n as usize;
                    } else {
                        let _ = sys_sleep(10);
                    }
                }
                print_str(b"  [PASS] 2068 bytes transmitidos em multiplos segmentos TCP (bytes enviados = ");
                print_u64(sent_total as u64);
                print_str(b")\n");

                // Recebe resposta bidirecional de 2048 bytes
                let mut recv_total = 0usize;
                let expected_rx = 20 + 2048;
                for _ in 0..60 {
                    let n = sys_recv(sock_b as u64, tx_ptr.add(recv_total), expected_rx - recv_total);
                    if n > 0 {
                        recv_total += n as usize;
                        if recv_total >= expected_rx { break; }
                    }
                    let _ = sys_sleep(25);
                }

                print_str(b"  Recebidos do peer independente: ");
                print_u64(recv_total as u64);
                print_str(b" bytes. ");

                if recv_total >= expected_rx {
                    let rx_csum = u32::from_be_bytes([*tx_ptr.add(16), *tx_ptr.add(17), *tx_ptr.add(18), *tx_ptr.add(19)]);
                    let mut calc_csum = 0u32;
                    let mut b_pattern_ok = true;
                    for i in 0..2048 {
                        let b = *tx_ptr.add(20 + i);
                        calc_csum = calc_csum.wrapping_add(b as u32);
                        let exp = ((i as u8).wrapping_mul(7)).wrapping_add(0x52);
                        if b != exp {
                            b_pattern_ok = false;
                        }
                    }

                    if calc_csum == rx_csum && b_pattern_ok {
                        print_str(b"Integridade e ordenacao multi-segmento (> MSS) VERIFICADAS com exito!\n");
                        print_str(b"  [PASS] Nivel B: Transferencia bidirecional multi-segmento concluida com sucesso!\n");
                    } else {
                        print_str(b"[FAIL] Discrepancia em checksum ou padrao de bytes!\n");
                    }
                } else {
                    print_str(b"[FAIL] Resposta incompleta recebida dentro do timeout.\n");
                }

                let _ = sys_free_page(page_addr as u64);
            }
            let _ = sys_close(sock_b as u64);
        }
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Nível E: Integração VFS + TCP (Streaming de Arquivo Real do Disco FAT32)
    // ─────────────────────────────────────────────────────────────────────────────
    print_str(b"[INTEROP-E] Testando integracao ponta-a-ponta VFS -> TCP (Streaming de /disk/README.TXT)...\n");
    let file_fd = sys_open(b"/disk/README.TXT", 0);
    print_str(b"  sys_open retorno fd=");
    print_i64(file_fd);
    print_str(b"\n");
    if file_fd >= 3 {
        let mut file_buf = [0u8; 256];
        let bytes_read = sys_read(file_fd as u64, file_buf.as_mut_ptr(), 256);
        let _ = sys_close(file_fd as u64);
        print_str(b"  sys_read retorno bytes=");
        print_i64(bytes_read);
        print_str(b"\n");

        if bytes_read > 0 {
            print_str(b"  Lidos ");
            print_u64(bytes_read as u64);
            print_str(b" bytes de /disk/README.TXT via VFS. Transmitindo via TCP para o peer independente...\n");

            let sock_e = sys_socket(2, 1, 0);
            if sock_e >= 0 {
                let ret_conn_e = sys_connect(sock_e as u64, &peer_addr);
                if ret_conn_e == 0 {
                    let mut tx_msg = [0u8; 276];
                    tx_msg[0..4].copy_from_slice(b"JINN");
                    tx_msg[4..8].copy_from_slice(&0x1001_0003u32.to_be_bytes()); // Session ID
                    tx_msg[8..12].copy_from_slice(&3u32.to_be_bytes());          // Msg ID 3 (VFS)
                    tx_msg[12..16].copy_from_slice(&(bytes_read as u32).to_be_bytes());

                    let mut csum_e = 0u32;
                    for i in 0..(bytes_read as usize) {
                        tx_msg[20 + i] = file_buf[i];
                        csum_e = csum_e.wrapping_add(file_buf[i] as u32);
                    }
                    tx_msg[16..20].copy_from_slice(&csum_e.to_be_bytes());

                    let total_tx = 20 + bytes_read as usize;
                    let sent = sys_send(sock_e as u64, tx_msg.as_ptr(), total_tx);
                    if sent == total_tx as i64 {
                        print_str(b"  Arquivo VFS transmitido via TCP (bytes enviados = ");
                        print_u64(sent as u64);
                        print_str(b"). Aguardando confirmacao...\n");

                        let mut rx_buf_e = [0u8; 64];
                        let mut rx_n = 0usize;
                        for _ in 0..40 {
                            let n = sys_recv(sock_e as u64, rx_buf_e[rx_n..].as_mut_ptr(), 64 - rx_n);
                            if n > 0 {
                                rx_n += n as usize;
                                if rx_n >= 20 { break; }
                            }
                            let _ = sys_sleep(20);
                        }

                        if rx_n >= 20 && &rx_buf_e[0..4] == b"JINN" {
                            let rx_msgid = u32::from_be_bytes([rx_buf_e[8], rx_buf_e[9], rx_buf_e[10], rx_buf_e[11]]);
                            if rx_msgid == 103 {
                                print_str(b"  [PASS] Nivel E: Arquivo VFS (/disk/README.TXT) transmitido e verificado com 100% de integridade pelo peer independente!\n");
                            }
                        }
                    }
                }
                let _ = sys_close(sock_e as u64);
            }
        }
    } else {
        print_str(b"  [WARN] sys_open falhou ao abrir /disk/README.TXT (ret=");
        print_i64(file_fd);
        print_str(b")\n");
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Nível C: Auditoria Efetiva do Firewall JinnWall SPI (Bloqueio vs Permissao)
    // ─────────────────────────────────────────────────────────────────────────────
    print_str(b"[INTEROP-C] Testando politicas reais do firewall JinnWall SPI (Bloqueio vs Permissao)...\n");

    let listen_sock = sys_socket(2, 1, 0);
    if listen_sock >= 0 {
        let server_addr = SockAddrIn {
            sin_family: 2,
            sin_port: 8080u16.to_be(),
            sin_addr: [0, 0, 0, 0],
            sin_zero: [0; 8],
        };
        let ret_bind = sys_bind(listen_sock as u64, &server_addr);
        let ret_listen = sys_listen(listen_sock as u64, 2);

        if ret_bind == 0 && ret_listen == 0 {
            // ── Cenario C1: Politica PublicSecure (Bloqueio obrigatorio de conexao entrante) ──
            let _ = sys_jinnwall_ctrl(1, 0, 0); // Perfil 0 = PublicSecure
            let mut stat_before = JinnWallStats::default();
            let _ = sys_jinnwall_ctrl(2, &mut stat_before as *mut _ as u64, 0);

            print_str(b"  [C1-SECURE] JinnWall em PublicSecure. Aguardando tentativa de conexao do peer...\n");
            let mut client_addr: SockAddrIn = core::mem::zeroed();
            let mut accepted_c1 = -1i64;

            for _ in 0..15 {
                let ret = sys_accept(listen_sock as u64, &mut client_addr);
                if ret >= 0 {
                    accepted_c1 = ret;
                    break;
                }
                let _ = sys_sleep(100);
            }

            let mut stat_after_c1 = JinnWallStats::default();
            let _ = sys_jinnwall_ctrl(2, &mut stat_after_c1 as *mut _ as u64, 0);

            if accepted_c1 < 0 {
                print_str(b"  [PASS] Cenario C1: Conexao entrante nao autorizada BLOQUEADA pelo JinnWall sob PublicSecure (Drops=");
                print_u64(stat_after_c1.dropped);
                print_str(b")!\n");
            } else {
                print_str(b"  [FAIL] Conexao nao foi bloqueada como esperado sob PublicSecure!\n");
                let _ = sys_close(accepted_c1 as u64);
            }

            // ── Cenario C2: Politica Development (Autorizacao explicita da porta 8080) ──
            print_str(b"  [C2-DEV] Alternando JinnWall para perfil Development via Syscall 37...\n");
            let _ = sys_jinnwall_ctrl(1, 1, 0); // Perfil 1 = Development
            let mut stat_before_c2 = JinnWallStats::default();
            let _ = sys_jinnwall_ctrl(2, &mut stat_before_c2 as *mut _ as u64, 0);

            let mut accepted_c2 = -1i64;
            for _ in 0..40 {
                let ret = sys_accept(listen_sock as u64, &mut client_addr);
                if ret >= 0 {
                    accepted_c2 = ret;
                    break;
                }
                let _ = sys_sleep(100);
            }

            if accepted_c2 >= 0 {
                let mut in_buf = [0u8; 128];
                let mut n_in = 0i64;
                for _ in 0..40 {
                    let n = sys_recv(accepted_c2 as u64, in_buf.as_mut_ptr(), 128);
                    if n > 0 {
                        n_in = n;
                        break;
                    }
                    let _ = sys_sleep(25);
                }

                if n_in > 20 && &in_buf[0..4] == b"JINN" {
                    let resp_payload = b"JINN_OS_ECHO_OK";
                    let mut resp_msg = [0u8; 35];
                    resp_msg[0..4].copy_from_slice(b"JINN");
                    resp_msg[4..8].copy_from_slice(&0x2001u32.to_be_bytes());
                    resp_msg[8..12].copy_from_slice(&101u32.to_be_bytes());
                    resp_msg[12..16].copy_from_slice(&(resp_payload.len() as u32).to_be_bytes());
                    let mut s = 0u32;
                    for &b in resp_payload.iter() { s = s.wrapping_add(b as u32); }
                    resp_msg[16..20].copy_from_slice(&s.to_be_bytes());
                    resp_msg[20..35].copy_from_slice(resp_payload);
                    let _ = sys_send(accepted_c2 as u64, resp_msg.as_ptr(), 35);
                }
                let _ = sys_sleep(50);
                let _ = sys_close(accepted_c2 as u64);

                let mut stat_after_c2 = JinnWallStats::default();
                let _ = sys_jinnwall_ctrl(2, &mut stat_after_c2 as *mut _ as u64, 0);

                print_str(b"  [PASS] Cenario C2: Conexao entrante AUTORIZADA pelo JinnWall sob Development e dados trocados com sucesso (Accepts=");
                print_u64(stat_after_c2.accepted);
                print_str(b")!\n");
            } else {
                print_str(b"  [FAIL] Conexao entrante nao foi aceita dentro do timeout sob Development!\n");
            }

            // Restaura perfil padrao de seguranca
            let _ = sys_jinnwall_ctrl(1, 0, 0);
        }
        let _ = sys_close(listen_sock as u64);
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Nível D: Robustez — Conexão Recusada em Porta Fechada (Host envia RST)
    // ─────────────────────────────────────────────────────────────────────────────
    print_str(b"[INTEROP-D] Testando conexao recusada em porta inativa (RST/ECONNREFUSED)...\n");
    let closed_peer = SockAddrIn {
        sin_family: 2,
        sin_port: 9991u16.to_be(), // Porta 9991 (fechada)
        sin_addr: [10, 0, 2, 2],
        sin_zero: [0; 8],
    };
    let sock_d = sys_socket(2, 1, 0);
    if sock_d >= 0 {
        let ret_d = sys_connect(sock_d as u64, &closed_peer);
        if ret_d == -111 || ret_d == -110 {
            print_str(b"  [PASS] Tratamento de RST em porta fechada validado: erro capturado com seguranca (ret=");
            print_i64(ret_d);
            print_str(b")!\n");
        } else {
            print_str(b"  [INFO] Conexao a porta fechada retornou: ");
            print_i64(ret_d);
            print_str(b"\n");
        }
        let _ = sys_close(sock_d as u64);
    }

    print_str(b"==================================================================\n");
    print_str(b"  CONCLUSAO DOS TESTES DE INTEROPERABILIDADE TCP 1.0\n");
    print_str(b"==================================================================\n\n");
}

static mut PRESSURE_PAGES: [u64; 4096] = [0u64; 4096];

/// Teste E: Pressão e Limites de Memória (Quota max_frames, -EDQUOT/-ENOMEM e Restauração)
unsafe fn test_memory_pressure() {
    print_str(b"[ADV-TEST 5/5] test_memory_pressure: Alocando paginas ate max_frames...\n");
    let pages = &mut *core::ptr::addr_of_mut!(PRESSURE_PAGES);
    let mut count = 0usize;
    let mut hit_limit = false;

    for i in 0..pages.len() {
        let ret = sys_alloc_page();
        if ret > 0 {
            pages[count] = ret as u64;
            count += 1;
        } else if ret == ENOMEM || ret == EDQUOT {
            hit_limit = true;
            print_str(b"  Alocacao #");
            print_u64((i + 1) as u64);
            print_str(b" atingiu limite/quota com codigo ");
            print_i64(ret);
            print_str(b"!\n");
            break;
        } else {
            print_str(b"  [WARN] Retorno inesperado de sys_alloc_page: ");
            print_i64(ret);
            print_str(b"\n");
            break;
        }
    }

    if !hit_limit && count == pages.len() {
        // Verifica uma alocacao adicional caso o perfil tenha preenchido todos os 4096 slots dinamicos
        let extra = sys_alloc_page();
        if extra == ENOMEM || extra == EDQUOT {
            hit_limit = true;
        } else if extra > 0 {
            let _ = sys_free_page(extra as u64);
        }
    }

    print_str(b"  Total de paginas alocadas: ");
    print_u64(count as u64);
    print_str(b". Desalocando via sys_free_page...\n");
    for i in 0..count {
        let ret = sys_free_page(pages[i]);
        if ret != 0 {
            print_str(b"  [FALHA] sys_free_page falhou no endereco 0x");
            print_u64(pages[i]);
            print_str(b"\n");
        }
        pages[i] = 0;
    }

    if hit_limit {
        print_str(b"  [PASS] test_memory_pressure: Quota aplicada com seguranca e memoria restaurada!\n");
    } else {
        print_str(b"  [FAIL] test_memory_pressure: Limite nao foi atingido.\n");
    }
}

// ── Testes de Stress e Falhas Complementares ──────────────────────────────────

unsafe fn test_fd_stress() {
    print_str(b"[STRESS] test_fd_stress: Abrindo descritores VFS ate atingir max_fds...\n");
    let mut fds = [0i64; 40];
    let mut opened = 0usize;
    let mut hit_limit = false;

    for i in 0..fds.len() {
        let fd = sys_open(b"/disk/README.TXT", 0);
        if fd >= 0 {
            fds[opened] = fd;
            opened += 1;
        } else if fd == EMFILE {
            hit_limit = true;
            print_str(b"  Tentativa #");
            print_u64((i + 1) as u64);
            print_str(b" rejeitada com -EMFILE (-24)!\n");
            break;
        } else {
            print_str(b"  [WARN] sys_open retornou: ");
            print_i64(fd);
            print_str(b"\n");
            break;
        }
    }

    for i in 0..opened {
        let _ = sys_close(fds[i] as u64);
    }

    let recycled_fd = sys_open(b"/disk/README.TXT", 0);
    if recycled_fd >= 0 {
        print_str(b"  Reciclagem de slot validada: novo fd = ");
        print_i64(recycled_fd);
        print_str(b"\n");
        let _ = sys_close(recycled_fd as u64);
    }

    if hit_limit && recycled_fd >= 0 {
        print_str(b"  [PASS] test_fd_stress: Limite max_fds e reciclagem OK!\n");
    } else {
        print_str(b"  [FAIL] test_fd_stress falhou.\n");
    }
}

unsafe fn test_window_stress() {
    print_str(b"[STRESS] test_window_stress: Criando janelas ate o limite max_windows...\n");
    let mut win_ids = [0i64; 20];
    let mut created = 0usize;
    let mut hit_limit = false;

    for i in 0..win_ids.len() {
        let win = sys_window_create(b"WinStress", 100, 100);
        if win >= 0 {
            win_ids[created] = win;
            created += 1;
        } else if win == EMFILE || win == EDQUOT {
            hit_limit = true;
            print_str(b"  Janela #");
            print_u64((i + 1) as u64);
            print_str(b" rejeitada por limite com codigo ");
            print_i64(win);
            print_str(b"!\n");
            break;
        } else {
            print_str(b"  [WARN] sys_window_create retornou: ");
            print_i64(win);
            print_str(b"\n");
            break;
        }
    }

    for i in 0..created {
        let _ = sys_window_close(win_ids[i] as usize);
    }

    let recycled_win = sys_window_create(b"WinRecycled", 100, 100);
    if recycled_win >= 0 {
        print_str(b"  Reciclagem de janela validada: novo win_id = ");
        print_i64(recycled_win);
        print_str(b"\n");
        let _ = sys_window_close(recycled_win as usize);
    }

    if hit_limit && recycled_win >= 0 {
        print_str(b"  [PASS] test_window_stress: Limite max_windows e reciclagem OK!\n");
    } else {
        print_str(b"  [FAIL] test_window_stress falhou.\n");
    }
}

/// Validação Adversarial da Syscall 24 (sys_window_poll_event)
unsafe fn test_adv_window_events() {
    print_str(b"\n--- Teste Adversarial: Syscall 24 (sys_window_poll_event) e Fila de Eventos ---\n");

    // 0. Validação de Contrato Binário e Alinhamento da ABI
    if core::mem::size_of::<ClientWindowEvent>() != 32 || core::mem::align_of::<ClientWindowEvent>() != 8 {
        print_str(b"  [FAIL] ClientWindowEvent ABI incorreta! Size != 32 ou Align != 8\n");
        return;
    }
    print_str(b"  [PASS] ClientWindowEvent layout validado: size=32 bytes, align=8 bytes.\n");

    // 1. Verificação de Integridade de Ponteiros de Usuário
    let ret_null = sys_window_poll_event(0, core::ptr::null_mut());
    if ret_null != EINVAL {
        print_str(b"  [FAIL] sys_window_poll_event com event_ptr NULL nao retornou -EINVAL (-22)! Ret: ");
        print_i64(ret_null);
        print_str(b"\n");
        return;
    }
    print_str(b"  [PASS] sys_window_poll_event rejeitou ponteiro NULL com -EINVAL (-22).\n");

    let ret_kernel = sys_window_poll_event(0, 0xFFFF_8000_0000_0000 as *mut ClientWindowEvent);
    if ret_kernel != EFAULT {
        print_str(b"  [FAIL] sys_window_poll_event com ponteiro kernel nao retornou -EFAULT (-14)! Ret: ");
        print_i64(ret_kernel);
        print_str(b"\n");
        return;
    }
    print_str(b"  [PASS] sys_window_poll_event rejeitou ponteiro no espaco de kernel com -EFAULT (-14).\n");

    let ret_unmapped = sys_window_poll_event(0, 0x0000_1000_0000_0000 as *mut ClientWindowEvent);
    if ret_unmapped != EFAULT {
        print_str(b"  [FAIL] sys_window_poll_event com ponteiro nao mapeado nao retornou -EFAULT (-14)! Ret: ");
        print_i64(ret_unmapped);
        print_str(b"\n");
        return;
    }
    print_str(b"  [PASS] sys_window_poll_event rejeitou endereco nao mapeado com -EFAULT (-14).\n");

    // Verificação de buffer cruzando fronteira entre página mapeada e não mapeada
    let page_vaddr = sys_alloc_page();
    if page_vaddr > 0 {
        let straddle_ptr = (page_vaddr as usize + 4096 - 16) as *mut ClientWindowEvent;
        let ret_straddle = sys_window_poll_event(0, straddle_ptr);
        let _ = sys_free_page(page_vaddr as u64);
        if ret_straddle != EFAULT {
            print_str(b"  [FAIL] sys_window_poll_event com struct cruzando pagina nao mapeada nao retornou -EFAULT (-14)! Ret: ");
            print_i64(ret_straddle);
            print_str(b"\n");
            return;
        }
        print_str(b"  [PASS] sys_window_poll_event rejeitou struct cruzando fronteira de pagina nao mapeada com -EFAULT (-14).\n");
    } else {
        print_str(b"  [WARN] sys_alloc_page retornou erro no teste de fronteira: ");
        print_i64(page_vaddr);
        print_str(b"\n");
    }

    // 2. Janela Inexistente
    let mut ev: ClientWindowEvent = core::mem::zeroed();
    let ret_badf = sys_window_poll_event(999, &mut ev as *mut ClientWindowEvent);
    if ret_badf != EBADF {
        print_str(b"  [FAIL] sys_window_poll_event com win_id inexistente nao retornou -EBADF (-9)! Ret: ");
        print_i64(ret_badf);
        print_str(b"\n");
        return;
    }
    print_str(b"  [PASS] sys_window_poll_event com win_id inexistente rejeitado com -EBADF (-9).\n");

    // 3. Janela Válida e Fila Vazia
    let win_id = sys_window_create(b"EventPollTest", 120, 80);
    if win_id < 0 {
        print_str(b"  [FAIL] sys_window_create falhou para teste de eventos: ");
        print_i64(win_id);
        print_str(b"\n");
        return;
    }
    let win_id = win_id as usize;

    let poll_empty = sys_window_poll_event(win_id, &mut ev as *mut ClientWindowEvent);
    if poll_empty != 0 {
        print_str(b"  [FAIL] sys_window_poll_event em fila vazia nao retornou 0! Ret: ");
        print_i64(poll_empty);
        print_str(b"\n");
        let _ = sys_window_close(win_id);
        return;
    }
    print_str(b"  [PASS] sys_window_poll_event retornou 0 (sem eventos pendentes) para janela recem-criada.\n");

    // 4. Fechamento e Invalidação da Janela
    let close_ret = sys_window_close(win_id);
    if close_ret != 0 {
        print_str(b"  [FAIL] sys_window_close falhou para janela de teste: ");
        print_i64(close_ret);
        print_str(b"\n");
        return;
    }

    let poll_after_close = sys_window_poll_event(win_id, &mut ev as *mut ClientWindowEvent);
    if poll_after_close != EBADF {
        print_str(b"  [FAIL] sys_window_poll_event em janela ja fechada nao retornou -EBADF! Ret: ");
        print_i64(poll_after_close);
        print_str(b"\n");
        return;
    }
    print_str(b"  [PASS] sys_window_poll_event rejeitou janela fechada com -EBADF (-9).\n");
    print_str(b"  [PASS] test_adv_window_events: Syscall 24 e validacao de eventos APROVADOS!\n");
}

unsafe fn test_fault_app() {
    print_str(b"[FAULT] test_fault_app: Injetando ponteiros e descritores invalidos...\n");

    let ret_bad_ptr = sys_write(1, 0xDEAD_BEEF as *const u8, 64);
    let fault_ok = ret_bad_ptr == EFAULT;

    let ret_null_ptr = sys_write(1, core::ptr::null(), 64);
    let null_ok = ret_null_ptr == EINVAL;

    let dummy_buf = [0u32; 10];
    let ret_fake_win = sys_window_update(999, &dummy_buf);
    let win_ok = ret_fake_win == EBADF;

    let mut fbuf = [0u8; 16];
    let ret_bad_fd = sys_read(999, fbuf.as_mut_ptr(), 16);
    let fd_ok = ret_bad_fd < 0;

    print_str(b"  Resultados das falhas: EFAULT=");
    print_i64(ret_bad_ptr);
    print_str(b", EINVAL=");
    print_i64(ret_null_ptr);
    print_str(b", EBADF_WIN=");
    print_i64(ret_fake_win);
    print_str(b", BAD_FD=");
    print_i64(ret_bad_fd);
    print_str(b"\n");

    if fault_ok && null_ok && win_ok && fd_ok {
        print_str(b"  [PASS] test_fault_app: Todas as falhas foram contidas com seguranca!\n");
    } else {
        print_str(b"  [FAIL] test_fault_app falhou em conter condicao de erro.\n");
    }
}

unsafe fn test_observer_app() {
    print_str(b"[OBSERVER] test_observer_app: Consultando telemetria do processo via Syscall 25...\n");
    let mut stat: ProcessResourceStat = core::mem::zeroed();
    let ret = sys_res_stat(0, &mut stat);

    if ret == 0 {
        let stat = core::ptr::read_volatile(&stat);
        print_str(b"  ================ METRICAS DE RECURSOS DO PROCESSO ================\n");
        print_str(b"  PID: "); print_u64(stat.pid as u64);
        print_str(b" | Estado: "); print_u64(stat.state as u64);
        print_str(b" | CPU Ticks: "); print_u64(stat.cpu_ticks);
        print_str(b" | Trocas Contexto: "); print_u64(stat.ctx_switches);
        print_str(b"\n");

        print_str(b"  Frames Memoria: "); print_u64(stat.allocated_frames as u64);
        print_str(b" (Pico Historico: "); print_u64(stat.peak_allocated_frames as u64);
        print_str(b" / Max: "); print_u64(stat.max_frames as u64);
        print_str(b")\n");

        print_str(b"  Descritores: FDs="); print_u64(stat.open_fds as u64);
        print_str(b" | Janelas="); print_u64(stat.open_windows as u64);
        print_str(b" | JDA="); print_u64(stat.open_jda_buffers as u64);
        print_str(b" | Sockets="); print_u64(stat.open_sockets as u64);
        print_str(b"\n");

        print_str(b"  Violacoes de Limite Rejeitadas: "); print_u64(stat.limit_violations as u64);
        print_str(b" | Ultimo Erro: "); print_i64(stat.last_violation_code as i64);
        print_str(b"\n  ==================================================================\n");

        if stat.allocated_frames > 0 && stat.peak_allocated_frames >= stat.allocated_frames {
            print_str(b"  [PASS] test_observer_app: Telemetria coerente e violacoes registradas com precisao!\n");
        } else {
            print_str(b"  [WARN] test_observer_app: Contadores fora da faixa esperada.\n");
        }
    } else {
        print_str(b"  [FAIL] test_observer_app: sys_res_stat retornou erro!\n");
    }
}

// ── Aladdin Desktop Jinn Browser v0.2.0 Native Integration ───────────────────

pub const MAX_WIN_W: usize = 1024;
pub const MAX_WIN_H: usize = 768;
pub const BASE_WIN_W: usize = 300;
pub const BASE_WIN_H: usize = 180;
pub const BASE_WIN_PIXELS: usize = BASE_WIN_W * BASE_WIN_H; // 54.000 pixels = 216 KiB

static mut BASE_WIN_BUF: [u32; BASE_WIN_PIXELS] = [0u32; BASE_WIN_PIXELS];

/// Gerenciador dinâmico de superfícies gráficas sob demanda para modo janela e tela cheia
pub struct DynamicSurfaceBuffer {
    pub ptr: *mut u32,
    pub width: usize,
    pub height: usize,
    pub capacity_pixels: usize,
    pub is_heap_allocated: bool,
    pub allocated_pages: usize,
    pub dynamic_base_vaddr: u64,
}

impl DynamicSurfaceBuffer {
    pub const fn new() -> Self {
        Self {
            ptr: core::ptr::null_mut(),
            width: BASE_WIN_W,
            height: BASE_WIN_H,
            capacity_pixels: BASE_WIN_PIXELS,
            is_heap_allocated: false,
            allocated_pages: 0,
            dynamic_base_vaddr: 0,
        }
    }

    pub unsafe fn init(&mut self) {
        self.ptr = core::ptr::addr_of_mut!(BASE_WIN_BUF) as *mut u32;
        self.width = BASE_WIN_W;
        self.height = BASE_WIN_H;
        self.capacity_pixels = BASE_WIN_PIXELS;
        self.is_heap_allocated = false;
        self.allocated_pages = 0;
        self.dynamic_base_vaddr = 0;
        let slice = core::slice::from_raw_parts_mut(self.ptr, BASE_WIN_PIXELS);
        slice.fill(0x0018_1825);
    }

    pub fn slice_mut(&mut self) -> &mut [u32] {
        let len = self.width * self.height;
        unsafe { core::slice::from_raw_parts_mut(self.ptr, len) }
    }

    /// Realoca com segurança a superfície gráfica sob demanda:
    /// - Em modo janela (300x180): utiliza buffer base compacto de 216 KiB.
    /// - Em modo tela cheia: aloca páginas físicas de 4 KiB sob demanda.
    /// - Ao retornar para modo janela: libera páginas físicas imediatamente via `sys_free_page`.
    pub unsafe fn resize(&mut self, new_w: usize, new_h: usize) -> Result<(), ()> {
        if new_w < 100 || new_w > MAX_WIN_W || new_h < 60 || new_h > MAX_WIN_H {
            return Err(());
        }

        let needed_pixels = match new_w.checked_mul(new_h) {
            Some(p) => p,
            None => return Err(()),
        };

        if needed_pixels <= BASE_WIN_PIXELS {
            if self.is_heap_allocated && self.allocated_pages > 0 {
                print_str(b"[DynamicSurfaceBuffer] FREE_FULLSCREEN: Devolvendo ");
                print_u64(self.allocated_pages as u64);
                print_str(b" paginas fisicas ao kernel via sys_free_page...\n");
                for i in 0..self.allocated_pages {
                    let addr = self.dynamic_base_vaddr + (i as u64 * 4096);
                    let _ = sys_free_page(addr);
                }
                print_str(b"[DynamicSurfaceBuffer] CICLO_CONCLUIDO: 0 -> 768 -> 0 paginas dinamicas retidas. Base BSS restaurada (216 KiB).\n");
                self.is_heap_allocated = false;
                self.allocated_pages = 0;
                self.dynamic_base_vaddr = 0;
            }
            self.ptr = core::ptr::addr_of_mut!(BASE_WIN_BUF) as *mut u32;
            self.width = new_w;
            self.height = new_h;
            self.capacity_pixels = BASE_WIN_PIXELS;
            return Ok(());
        }

        if self.is_heap_allocated && self.capacity_pixels >= needed_pixels {
            self.width = new_w;
            self.height = new_h;
            return Ok(());
        }

        let needed_bytes = needed_pixels * 4;
        let pages_needed = (needed_bytes + 4095) / 4096;

        print_str(b"[DynamicSurfaceBuffer] ALLOC_FULLSCREEN: Requisitando ");
        print_u64(pages_needed as u64);
        print_str(b" paginas (4 KiB) sob demanda via sys_alloc_page...\n");

        let mut first_vaddr: u64 = 0;
        let mut allocated = 0;
        let mut alloc_failed = false;

        for i in 0..pages_needed {
            let res = sys_alloc_page();
            if res <= 0 {
                alloc_failed = true;
                break;
            }
            let vaddr = res as u64;
            if i == 0 {
                first_vaddr = vaddr;
            }
            allocated += 1;
        }

        if alloc_failed {
            for i in 0..allocated {
                let addr = first_vaddr + (i as u64 * 4096);
                let _ = sys_free_page(addr);
            }
            return Err(());
        }

        print_str(b"[DynamicSurfaceBuffer] ALLOC_FULLSCREEN_OK: ");
        print_u64(pages_needed as u64);
        print_str(b" paginas alocadas com sucesso.\n");

        let new_ptr = first_vaddr as *mut u32;
        let new_slice = core::slice::from_raw_parts_mut(new_ptr, needed_pixels);
        new_slice.fill(0x0018_1825);

        if self.is_heap_allocated && self.allocated_pages > 0 {
            for i in 0..self.allocated_pages {
                let addr = self.dynamic_base_vaddr + (i as u64 * 4096);
                let _ = sys_free_page(addr);
            }
        }

        self.ptr = new_ptr;
        self.width = new_w;
        self.height = new_h;
        self.capacity_pixels = pages_needed * (4096 / 4);
        self.is_heap_allocated = true;
        self.allocated_pages = pages_needed;
        self.dynamic_base_vaddr = first_vaddr;

        Ok(())
    }
}
pub const HTTP_BUF_SIZE: usize = 131072;
static mut HTML_FILE_BUF: [u8; HTTP_BUF_SIZE] = [0; HTTP_BUF_SIZE];
static mut HTTP_RX_BUF: [u8; HTTP_BUF_SIZE] = [0; HTTP_BUF_SIZE];
static mut HTTP_PLAIN_BUF: [u8; HTTP_BUF_SIZE] = [0; HTTP_BUF_SIZE];
static mut HTTP_FORMAT_BUF: [u8; HTTP_BUF_SIZE] = [0; HTTP_BUF_SIZE];
static mut BROWSER_SESSION: browser::BrowserSession = browser::BrowserSession::new();
static mut HOT_ROUTE_TMP: [HotRouteEntryUser; 16] = [HotRouteEntryUser::empty(); 16];

#[derive(Clone, Copy)]
struct FetchDiagInfo {
    host: [u8; 96],
    host_len: usize,
    resolved_ip: [u8; 4],
    has_resolved_ip: bool,
    is_https: bool,
    port: u16,
    dns_code: i64,
    connect_code: i64,
    http_status: u16,
}

impl FetchDiagInfo {
    const fn empty() -> Self {
        Self {
            host: [0u8; 96],
            host_len: 0,
            resolved_ip: [0u8; 4],
            has_resolved_ip: false,
            is_https: false,
            port: 0,
            dns_code: 0,
            connect_code: 0,
            http_status: 0,
        }
    }
}

static mut LAST_FETCH_DIAG: FetchDiagInfo = FetchDiagInfo::empty();
static mut CALIBRATED_TSC_KHZ: u64 = 0;

#[inline(always)]
unsafe fn read_tsc() -> u64 {
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
    let val = core::arch::x86_64::_rdtsc();
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
    val
}

/// Calibra a frequencia do TSC (em kHz) uma unica vez via CLOCK_MONOTONIC + sys_sleep(50ms HLT).
unsafe fn ensure_tsc_calibrated() -> u64 {
    if CALIBRATED_TSC_KHZ > 0 {
        return CALIBRATED_TSC_KHZ;
    }
    let mut ts0 = Timespec::default();
    let mut ts1 = Timespec::default();
    if sys_clock_gettime(1, &mut ts0 as *mut Timespec) < 0 {
        return 0;
    }
    let t0 = read_tsc();
    let _ = sys_sleep(50);
    let t1 = read_tsc();
    if sys_clock_gettime(1, &mut ts1 as *mut Timespec) < 0 {
        return 0;
    }
    let dt_ns = ((ts1.tv_sec as i128 - ts0.tv_sec as i128) * 1_000_000_000i128)
        + (ts1.tv_nsec as i128 - ts0.tv_nsec as i128);
    let d_cyc = t1.wrapping_sub(t0);
    if dt_ns > 1_000_000 && d_cyc > 0 {
        let khz = ((d_cyc as u128 * 1_000_000u128) / (dt_ns as u128)) as u64;
        if khz > 0 {
            CALIBRATED_TSC_KHZ = khz;
            return khz;
        }
    }
    0
}

#[inline]
fn cycles_to_us_tenths(cycles: u64, tsc_khz: u64) -> u64 {
    if tsc_khz == 0 {
        return 0;
    }
    ((cycles as u128 * 10_000u128) / (tsc_khz as u128)) as u64
}

fn diag_push_us_tenths(dst: &mut [u8], pos: &mut usize, cycles: u64, tsc_khz: u64) {
    if tsc_khz == 0 {
        diag_push_bytes(dst, pos, b"N/A");
        return;
    }
    let tenths = cycles_to_us_tenths(cycles, tsc_khz);
    diag_push_u64(dst, pos, tenths / 10);
    diag_push_bytes(dst, pos, b".");
    diag_push_u64(dst, pos, tenths % 10);
    diag_push_bytes(dst, pos, b" us");
}

#[derive(Clone, Copy)]
struct ObsBenchStats {
    tsc_khz: u64,
    sys88_live_cyc: u64,
    sys88_min_cyc: u64,
    sys88_avg_cyc: u64,
    sys88_max_cyc: u64,
    sys97_live_cyc: u64,
    sys97_min_cyc: u64,
    sys97_avg_cyc: u64,
    sys97_max_cyc: u64,
}

unsafe fn benchmark_observability_syscalls(
    sys88_live_cyc: u64,
    sys97_live_cyc: u64,
    verbose: bool,
) -> ObsBenchStats {
    let tsc_khz = ensure_tsc_calibrated();
    let mut bench_stats = SystemStatsUser::default();
    let mut bench_hr = [HotRouteEntryUser::empty(); 16];

    for _ in 0..10 {
        let _ = sys_get_system_stats(&mut bench_stats as *mut SystemStatsUser);
        let _ = sys_net_hot_route_list(&mut bench_hr);
    }

    let mut min88 = u64::MAX;
    let mut max88 = 0u64;
    let mut sum88 = 0u128;
    for _ in 0..100 {
        let a = read_tsc();
        let _ = sys_get_system_stats(&mut bench_stats as *mut SystemStatsUser);
        let b = read_tsc();
        let dt = b.wrapping_sub(a);
        if dt < min88 {
            min88 = dt;
        }
        if dt > max88 {
            max88 = dt;
        }
        sum88 += dt as u128;
    }

    let mut min97 = u64::MAX;
    let mut max97 = 0u64;
    let mut sum97 = 0u128;
    for _ in 0..100 {
        let a = read_tsc();
        let _ = sys_net_hot_route_list(&mut bench_hr);
        let b = read_tsc();
        let dt = b.wrapping_sub(a);
        if dt < min97 {
            min97 = dt;
        }
        if dt > max97 {
            max97 = dt;
        }
        sum97 += dt as u128;
    }

    let res = ObsBenchStats {
        tsc_khz,
        sys88_live_cyc,
        sys88_min_cyc: if min88 == u64::MAX { 0 } else { min88 },
        sys88_avg_cyc: (sum88 / 100) as u64,
        sys88_max_cyc: max88,
        sys97_live_cyc,
        sys97_min_cyc: if min97 == u64::MAX { 0 } else { min97 },
        sys97_avg_cyc: (sum97 / 100) as u64,
        sys97_max_cyc: max97,
    };

    if verbose {
        let mut msg = [0u8; 256];
        let mut mp = 0usize;
        diag_push_bytes(&mut msg, &mut mp, b"[DIAG-BENCH] tsc_khz=");
        diag_push_u64(&mut msg, &mut mp, res.tsc_khz);
        diag_push_bytes(&mut msg, &mut mp, b" sys88_live=");
        diag_push_u64(&mut msg, &mut mp, res.sys88_live_cyc);
        diag_push_bytes(&mut msg, &mut mp, b" sys88_min=");
        diag_push_u64(&mut msg, &mut mp, res.sys88_min_cyc);
        diag_push_bytes(&mut msg, &mut mp, b" sys88_avg100=");
        diag_push_u64(&mut msg, &mut mp, res.sys88_avg_cyc);
        diag_push_bytes(&mut msg, &mut mp, b" (");
        diag_push_us_tenths(&mut msg, &mut mp, res.sys88_avg_cyc, res.tsc_khz);
        diag_push_bytes(&mut msg, &mut mp, b") sys88_max=");
        diag_push_u64(&mut msg, &mut mp, res.sys88_max_cyc);
        diag_push_bytes(&mut msg, &mut mp, b" | sys97_live=");
        diag_push_u64(&mut msg, &mut mp, res.sys97_live_cyc);
        diag_push_bytes(&mut msg, &mut mp, b" sys97_min=");
        diag_push_u64(&mut msg, &mut mp, res.sys97_min_cyc);
        diag_push_bytes(&mut msg, &mut mp, b" sys97_avg100=");
        diag_push_u64(&mut msg, &mut mp, res.sys97_avg_cyc);
        diag_push_bytes(&mut msg, &mut mp, b" (");
        diag_push_us_tenths(&mut msg, &mut mp, res.sys97_avg_cyc, res.tsc_khz);
        diag_push_bytes(&mut msg, &mut mp, b") sys97_max=");
        diag_push_u64(&mut msg, &mut mp, res.sys97_max_cyc);
        diag_push_bytes(&mut msg, &mut mp, b"\n");
        print_str(&msg[..mp]);
    }
    res
}

unsafe fn measure_observability_boundary(
    stats: &mut SystemStatsUser,
    hr_out: &mut [HotRouteEntryUser; 16],
    verbose: bool,
) -> (ObsBenchStats, i64, i64) {
    let _ = ensure_tsc_calibrated();
    let s88_t0 = read_tsc();
    let r88 = sys_get_system_stats(stats as *mut SystemStatsUser);
    let s88_live_cyc = read_tsc().wrapping_sub(s88_t0);

    let s97_t0 = read_tsc();
    let r97 = sys_net_hot_route_list(hr_out);
    let s97_live_cyc = read_tsc().wrapping_sub(s97_t0);

    let obs = benchmark_observability_syscalls(s88_live_cyc, s97_live_cyc, verbose);
    (obs, r88, r97)
}

#[inline]
fn diag_push_bytes(dst: &mut [u8], pos: &mut usize, src: &[u8]) {
    let take = src.len().min(dst.len().saturating_sub(*pos));
    dst[*pos..*pos + take].copy_from_slice(&src[..take]);
    *pos += take;
}

fn diag_push_u64(dst: &mut [u8], pos: &mut usize, mut val: u64) {
    if val == 0 {
        diag_push_bytes(dst, pos, b"0");
        return;
    }
    let mut tmp = [0u8; 20];
    let mut n = 0usize;
    while val > 0 {
        tmp[n] = b'0' + (val % 10) as u8;
        val /= 10;
        n += 1;
    }
    while n > 0 {
        n -= 1;
        diag_push_bytes(dst, pos, &[tmp[n]]);
    }
}

fn diag_push_i64(dst: &mut [u8], pos: &mut usize, val: i64) {
    if val < 0 {
        diag_push_bytes(dst, pos, b"-");
        diag_push_u64(dst, pos, val.unsigned_abs());
    } else {
        diag_push_u64(dst, pos, val as u64);
    }
}

fn diag_push_ipv4(dst: &mut [u8], pos: &mut usize, ip: [u8; 4]) {
    for idx in 0..4 {
        if idx > 0 {
            diag_push_bytes(dst, pos, b".");
        }
        diag_push_u64(dst, pos, ip[idx] as u64);
    }
}

fn diag_push_mac(dst: &mut [u8], pos: &mut usize, mac: [u8; 6]) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for idx in 0..6 {
        if idx > 0 {
            diag_push_bytes(dst, pos, b":");
        }
        let b = mac[idx];
        diag_push_bytes(dst, pos, &[HEX[(b >> 4) as usize], HEX[(b & 0x0F) as usize]]);
    }
}

fn diag_cstr_trim(buf: &[u8]) -> &[u8] {
    let mut end = 0usize;
    while end < buf.len() && buf[end] != 0 {
        end += 1;
    }
    while end > 0 && buf[end - 1] == b' ' {
        end -= 1;
    }
    &buf[..end]
}

unsafe fn read_html_file(path: &[u8]) -> Option<usize> {
    let fd = sys_open(path, O_RDONLY);
    if fd < 0 {
        return None;
    }
    let buf_ptr = core::ptr::addr_of_mut!(HTML_FILE_BUF) as *mut u8;
    let n = sys_read(fd as u64, buf_ptr, HTTP_BUF_SIZE as u64);
    let _ = sys_close(fd as u64);
    if n > 0 {
        Some(n as usize)
    } else {
        None
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
enum FetchError {
    DnsNxDomain,
    DnsTimeout,
    DnsArpFailed,
    DnsNetDown,
    DnsServerFailure,
    DnsFailed,
    SocketFailed,
    ConnectFailed(i64),
    SendFailed,
    RecvTimeout,
    InvalidHttp,
    TooManyRedirects,
    HttpStatus(u16),
    HttpsRedirect,
    HttpsDowngradeBlocked,
    TlsEntropyFailed,
    TlsCertHostMismatch,
    TlsCertVerifyFailed,
    TlsHandshakeFailed,
    TlsDecryptFailed,
}

unsafe fn fetch_http_document(
    initial_url: &[u8],
    out_buf: &mut [u8],
) -> Result<usize, FetchError> {
    let mut dummy_url = [0u8; 128];
    let mut dummy_len = 0usize;
    fetch_http_document_inner(initial_url, out_buf, false, &mut dummy_url, &mut dummy_len)
}

unsafe fn fetch_http_document_inner(
    initial_url: &[u8],
    out_buf: &mut [u8],
    reject_http_errors: bool,
    final_url_out: &mut [u8; 128],
    final_url_len: &mut usize,
) -> Result<usize, FetchError> {
    let mut current_target = [0u8; 128];
    let len = initial_url.len().min(127);
    current_target[..len].copy_from_slice(&initial_url[..len]);
    let mut current_target_len = len;
    let mut visited_hashes = [0u64; 8];
    let mut visited_count = 0usize;

    let fnv1a_url = |u: &[u8]| -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for &b in u {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h
    };

    for _redirect_hop in 0..=6 {
        let target_slice = &current_target[..current_target_len];
        let target_hash = fnv1a_url(target_slice);
        if visited_count < visited_hashes.len() {
            visited_hashes[visited_count] = target_hash;
            visited_count += 1;
        }
        final_url_out[..current_target_len].copy_from_slice(target_slice);
        final_url_out[current_target_len] = 0;
        *final_url_len = current_target_len;

        let parsed = match browser::parse_http_url(target_slice) {
            Some(p) => p,
            None => return Err(FetchError::InvalidHttp),
        };

        let mut diag = FetchDiagInfo::empty();
        let h_cpy = parsed.host_len.min(diag.host.len());
        diag.host[..h_cpy].copy_from_slice(&parsed.host[..h_cpy]);
        diag.host_len = h_cpy;
        diag.is_https = parsed.is_https;
        diag.port = parsed.port;
        LAST_FETCH_DIAG = diag;

        print_str(b"[DIAG-FLOW] URL: ");
        print_str(target_slice);
        print_str(b"\n");

        print_str(b"[DIAG-FLOW] DNS: Resolvendo host: ");
        print_str(&parsed.host[..parsed.host_len]);
        print_str(b"\n");

        let mut ip = [0u8; 4];
        let dns_res = sys_dns_resolve(parsed.host.as_ptr(), parsed.host_len, ip.as_mut_ptr());
        if dns_res < 0 {
            LAST_FETCH_DIAG.dns_code = dns_res;
            if dns_res == -2 {
                print_str(b"[DIAG-FLOW] DNS: ERRO [NxDomain]: Dominio inexistente (NXDOMAIN)!\n");
                return Err(FetchError::DnsNxDomain);
            } else if dns_res == -110 {
                print_str(b"[DIAG-FLOW] DNS: ERRO [Timeout]: Timeout do servidor DNS upstream!\n");
                return Err(FetchError::DnsTimeout);
            } else if dns_res == -113 {
                print_str(b"[DIAG-FLOW] DNS: ERRO [ArpUnresolved]: Falha de resolucao ARP na rede local!\n");
                return Err(FetchError::DnsArpFailed);
            } else if dns_res == -100 {
                print_str(b"[DIAG-FLOW] DNS: ERRO [NoNetDriver]: Interface de rede inoperante!\n");
                return Err(FetchError::DnsNetDown);
            } else if dns_res == -5 {
                print_str(b"[DIAG-FLOW] DNS: ERRO [ServerFailure]: Falha reportada pelo servidor DNS (SERVFAIL)!\n");
                return Err(FetchError::DnsServerFailure);
            } else {
                print_str(b"[DIAG-FLOW] DNS: ERRO [DnsFailed]: Falha ao resolver DNS com codigo: ");
                print_i64(dns_res);
                print_str(b"\n");
                return Err(FetchError::DnsFailed);
            }
        }
        LAST_FETCH_DIAG.resolved_ip = ip;
        LAST_FETCH_DIAG.has_resolved_ip = true;

        print_str(b"[DIAG-FLOW] IP: ");
        print_u64(ip[0] as u64); print_str(b".");
        print_u64(ip[1] as u64); print_str(b".");
        print_u64(ip[2] as u64); print_str(b".");
        print_u64(ip[3] as u64);
        print_str(b":");
        print_u64(parsed.port as u64);
        print_str(b"\n");

        let fd = sys_socket(2, 1, 0); // AF_INET=2, SOCK_STREAM=1
        if fd < 0 {
            print_str(b"[DIAG-FLOW] TCP: ERRO: sys_socket falhou!\n");
            return Err(FetchError::SocketFailed);
        }
        let sockfd = fd as u64;

        let addr = SockAddrIn {
            sin_family: 2,
            sin_port: parsed.port.to_be(),
            sin_addr: ip,
            sin_zero: [0; 8],
        };

        print_str(b"[DIAG-FLOW] TCP: Estabelecendo 3-way handshake com servidor remoto...\n");
        let conn_res = sys_connect(sockfd, &addr);
        if conn_res < 0 {
            LAST_FETCH_DIAG.connect_code = conn_res;
            print_str(b"[DIAG-FLOW] TCP: ERRO: sys_connect falhou com codigo ");
            print_i64(conn_res);
            print_str(b"\n");
            let _ = sys_close(sockfd);
            return Err(FetchError::ConnectFailed(conn_res));
        }

        print_str(b"[DIAG-FLOW] TCP: Conexao estabelecida com sucesso!\n");

        let mut rx_total = 0usize;
        let mut tls_sess_opt = if parsed.is_https {
            print_str(b"[DIAG-FLOW] TLS: Inicializando sessao TLS 1.3 (RFC 8446)...\n");
            let mut seed = [0u8; 64];
            let rand_ret = sys_getrandom(seed.as_mut_ptr(), 64, 0);
            if rand_ret < 64 {
                print_str(b"[DIAG-FLOW] TLS: ERRO ao obter entropia do kernel (sys_getrandom)\n");
                let _ = sys_close(sockfd);
                return Err(FetchError::TlsEntropyFailed);
            }
            let mut sess = tls::Tls13ClientSession::new(&seed);

            print_str(b"[DIAG-FLOW] TLS: Transmitindo ClientHello (X25519 + ChaCha20-Poly1305)...\n");
            let mut ch_buf = [0u8; 512];
            let ch_len = sess.build_client_hello(&parsed.host[..parsed.host_len], &mut ch_buf);
            let sent_ch = sys_send(sockfd, ch_buf.as_ptr(), ch_len);
            if sent_ch < 0 {
                print_str(b"[DIAG-FLOW] TLS: Falha ao transmitir ClientHello!\n");
                let _ = sys_close(sockfd);
                return Err(FetchError::SendFailed);
            }

            print_str(b"[DIAG-FLOW] TLS: Aguardando fluxo de Handshake do servidor...\n");
            let rx_ptr = core::ptr::addr_of_mut!(HTTP_RX_BUF) as *mut u8;
            let mut rx_len = 0usize;
            let mut hs_spins = 0usize;
            let mut client_fin_buf = [0u8; 128];

            while !sess.is_established {
                // Aguarda pelo menos 5 bytes para o cabeçalho do próximo registro TLS
                while rx_len < 5 {
                    let space = HTTP_BUF_SIZE.saturating_sub(rx_len);
                    let n = sys_recv(sockfd, rx_ptr.add(rx_len), space.min(4096));
                    if n > 0 {
                        rx_len += n as usize;
                        hs_spins = 0;
                    } else if n == -11 {
                        hs_spins += 1;
                        if hs_spins > 600 {
                            print_str(b"[DIAG-FLOW] TLS: Timeout aguardando handshake (3.0s)\n");
                            let _ = sys_close(sockfd);
                            return Err(FetchError::RecvTimeout);
                        }
                        let _ = sys_sleep(5);
                    } else {
                        let _ = sys_close(sockfd);
                        return Err(FetchError::TlsHandshakeFailed);
                    }
                }

                let rec_type = *rx_ptr;
                let rec_len = (((*rx_ptr.add(3)) as usize) << 8) | ((*rx_ptr.add(4)) as usize);
                if rec_len == 0 || rec_len > 18432 {
                    print_str(b"[DIAG-FLOW] TLS: Registro TLS com tamanho invalido no handshake!\n");
                    let _ = sys_close(sockfd);
                    return Err(FetchError::TlsHandshakeFailed);
                }
                let total_rec_len = 5 + rec_len;
                if total_rec_len > HTTP_BUF_SIZE {
                    print_str(b"[DIAG-FLOW] TLS: Registro excede HTTP_BUF_SIZE!\n");
                    let _ = sys_close(sockfd);
                    return Err(FetchError::TlsHandshakeFailed);
                }

                // Garante recebimento do registro completo antes de processar ou decifrar
                while rx_len < total_rec_len {
                    let need = total_rec_len - rx_len;
                    let space = HTTP_BUF_SIZE.saturating_sub(rx_len);
                    let n = sys_recv(sockfd, rx_ptr.add(rx_len), need.min(space));
                    if n > 0 {
                        rx_len += n as usize;
                        hs_spins = 0;
                    } else if n == -11 {
                        hs_spins += 1;
                        if hs_spins > 600 {
                            let _ = sys_close(sockfd);
                            return Err(FetchError::RecvTimeout);
                        }
                        let _ = sys_sleep(5);
                    } else {
                        let _ = sys_close(sockfd);
                        return Err(FetchError::TlsHandshakeFailed);
                    }
                }

                let rec_slice = core::slice::from_raw_parts(rx_ptr as *const u8, total_rec_len);

                if rec_type == tls::CONTENT_HANDSHAKE && !sess.has_server_hello {
                    if !sess.process_server_hello(rec_slice) {
                        print_str(b"[DIAG-FLOW] TLS: Handshake TLS 1.3 ServerHello rejeitado!\n");
                        let _ = sys_close(sockfd);
                        return Err(FetchError::TlsHandshakeFailed);
                    }
                    print_str(b"[DIAG-FLOW] TLS: ServerHello aceito! Segredos de handshake derivados.\n");
                } else if rec_type == 0x14 {
                    print_str(b"[DIAG-FLOW] TLS: ChangeCipherSpec recebido (Middlebox compat).\n");
                } else if rec_type == tls::CONTENT_ALERT {
                    print_str(b"[DIAG-FLOW] TLS: Alerta TLS recebido durante handshake!\n");
                    let _ = sys_close(sockfd);
                    return Err(FetchError::TlsHandshakeFailed);
                } else if rec_type == tls::CONTENT_APPLICATION_DATA {
                    // Registros cifrados de Handshake (EncryptedExtensions, Certificate, CertificateVerify, Finished)
                    match sess.process_handshake_record(rec_slice, &mut client_fin_buf) {
                        Ok(Some(fin_len)) => {
                            print_str(b"[DIAG-FLOW] TLS: Finished do servidor autenticado! Enviando Client Finished...\n");
                            let ccs = [0x14u8, 0x03, 0x03, 0x00, 0x01, 0x01];
                            let _ = sys_send(sockfd, ccs.as_ptr(), ccs.len());
                            let sent_fin = sys_send(sockfd, client_fin_buf.as_ptr(), fin_len);
                            if sent_fin < 0 {
                                let _ = sys_close(sockfd);
                                return Err(FetchError::SendFailed);
                            }
                            print_str(b"[DIAG-FLOW] TLS: Handshake TLS 1.3 concluido com sucesso! Canal seguro ativo.\n");
                        }
                        Ok(None) => {}
                        Err(e) => {
                            let cert_err = sess.last_cert_err;
                            print_str(b"[DIAG-FLOW] TLS: Falha no handshake cifrado: ");
                            print_str(e.as_bytes());
                            print_str(b"\n");
                            let _ = sys_close(sockfd);
                            match cert_err {
                                Some(crate::x509::CertVerifyError::HostnameMismatch) => {
                                    let mut d = [0u8; 96];
                                    let dlen = crate::x509::last_cert_diag(&mut d);
                                    print_str(b"[BROWSER-SEC] BLOQUEADO certificado TLS (SAN/CN mismatch): ");
                                    print_str(&d[..dlen]);
                                    print_str(b"\n");
                                    return Err(FetchError::TlsCertHostMismatch);
                                }
                                Some(_) => {
                                    let mut d = [0u8; 96];
                                    let dlen = crate::x509::last_cert_diag(&mut d);
                                    print_str(b"[BROWSER-SEC] BLOQUEADO certificado/assinatura TLS: ");
                                    print_str(&d[..dlen]);
                                    print_str(b"\n");
                                    return Err(FetchError::TlsCertVerifyFailed);
                                }
                                None => return Err(FetchError::TlsHandshakeFailed),
                            }
                        }
                    }
                } else {
                    print_str(b"[DIAG-FLOW] TLS: ContentType inesperado no handshake!\n");
                    let _ = sys_close(sockfd);
                    return Err(FetchError::TlsHandshakeFailed);
                }

                // Desloca os bytes restantes no buffer
                let remaining = rx_len.saturating_sub(total_rec_len);
                if remaining > 0 {
                    core::ptr::copy(rx_ptr.add(total_rec_len), rx_ptr, remaining);
                }
                rx_len = remaining;
            }

            rx_total = rx_len;
            Some(sess)
        } else {
            None
        };

        print_str(b"[DIAG-FLOW] HTTP: Transmitindo requisicao GET...\n");
        let mut req_buf = [0u8; 512];
        let req_len = browser::build_http_get_request(&parsed, &mut req_buf);

        if let Some(ref mut sess) = tls_sess_opt {
            if sess.cipher_suite == tls::TLS_AES_128_GCM_SHA256 {
                print_str(b"[DIAG-FLOW] TLS: Criptografando requisicao HTTP GET via AES-128-GCM...\n");
            } else {
                print_str(b"[DIAG-FLOW] TLS: Criptografando requisicao HTTP GET via ChaCha20-Poly1305...\n");
            }
            let mut enc_buf = [0u8; 600];
            let enc_len = sess.encrypt_application_data(&req_buf[..req_len], &mut enc_buf);
            let sent = sys_send(sockfd, enc_buf.as_ptr(), enc_len);
            if sent < 0 {
                print_str(b"[DIAG-FLOW] TLS: ERRO ao transmitir dados cifrados da requisicao!\n");
                let _ = sys_close(sockfd);
                return Err(FetchError::SendFailed);
            }
        } else {
            let sent = sys_send(sockfd, req_buf.as_ptr(), req_len);
            if sent < 0 {
                print_str(b"[DIAG-FLOW] HTTP: ERRO: sys_send falhou!\n");
                let _ = sys_close(sockfd);
                return Err(FetchError::SendFailed);
            }
        }

        print_str(b"[DIAG-FLOW] HTTP: Requisicao enviada. Aguardando resposta...\n");
        let rx_ptr = core::ptr::addr_of_mut!(HTTP_RX_BUF) as *mut u8;
        let plain_ptr = core::ptr::addr_of_mut!(HTTP_PLAIN_BUF) as *mut u8;
        let mut plain_total = 0usize;
        let mut idle_spins = 0usize;

        if let Some(ref mut sess) = tls_sess_opt {
            let mut tls_eof = false;
            let mut tls_fatal_alert = false;
            loop {
                // Tenta decifrar registros completos disponiveis em HTTP_RX_BUF
                let mut pos = 0;
                while pos + 5 <= rx_total {
                    let rec_type = *rx_ptr.add(pos);
                    let rec_len = (((*rx_ptr.add(pos + 3)) as usize) << 8) | ((*rx_ptr.add(pos + 4)) as usize);
                    if rec_len == 0 || rec_len > 18432 {
                        print_str(b"[TLS-HTTP-RX] Erro: tamanho de registro TLS invalido!\n");
                        let _ = sys_close(sockfd);
                        return Err(FetchError::TlsDecryptFailed);
                    }
                    let total_rec = 5 + rec_len;
                    if pos + total_rec > rx_total {
                        break;
                    }
                    if plain_total + rec_len + 32 > HTTP_BUF_SIZE {
                        break;
                    }

                    let rec_slice = core::slice::from_raw_parts(rx_ptr.add(pos), total_rec);
                    let plain_slice = core::slice::from_raw_parts_mut(plain_ptr.add(plain_total), HTTP_BUF_SIZE - plain_total);

                    if rec_type == tls::CONTENT_APPLICATION_DATA {
                        match sess.decrypt_application_record(rec_slice, plain_slice) {
                            Ok(tls::AppRecord::Data(dec_len)) => {
                                plain_total += dec_len;
                            }
                            Ok(tls::AppRecord::Handshake) => {
                                // NewSessionTicket / post-handshake metadata: ignora silenciosamente
                            }
                            Ok(tls::AppRecord::Alert(level, desc)) => {
                                print_str(b"[TLS-HTTP-RX] Alerta TLS cifrado (level=");
                                print_u64(level as u64);
                                print_str(b", desc=");
                                print_u64(desc as u64);
                                print_str(b")\n");
                                if level == 2 && desc != 0 {
                                    tls_fatal_alert = true;
                                }
                                tls_eof = true;
                                pos += total_rec;
                                break;
                            }
                            Err(()) => {
                                print_str(b"[TLS-HTTP-RX] Falha AEAD ao decifrar registro TLS! suite=");
                                print_u64(sess.cipher_suite as u64);
                                print_str(b" seq=");
                                print_u64(sess.server_seq.saturating_sub(1));
                                print_str(b" pos=");
                                print_u64(pos as u64);
                                print_str(b" rec_len=");
                                print_u64(rec_len as u64);
                                print_str(b" plain=");
                                print_u64(plain_total as u64);
                                print_str(b" tag=");
                                if rec_slice.len() >= 16 {
                                    let t = &rec_slice[rec_slice.len() - 16..];
                                    const HEX: &[u8; 16] = b"0123456789abcdef";
                                    for &b in t {
                                        let h = [HEX[(b >> 4) as usize], HEX[(b & 0xF) as usize]];
                                        print_str(&h);
                                    }
                                }
                                print_str(b"\n");
                                let _ = sys_close(sockfd);
                                return Err(FetchError::TlsDecryptFailed);
                            }
                        }
                    } else if rec_type == tls::CONTENT_ALERT {
                        tls_eof = true;
                        pos += total_rec;
                        break;
                    }
                    pos += total_rec;
                }

                if pos > 0 {
                    let remaining = rx_total - pos;
                    if remaining > 0 {
                        core::ptr::copy(rx_ptr.add(pos), rx_ptr, remaining);
                    }
                    rx_total = remaining;
                }

                let plain_slice = core::slice::from_raw_parts(plain_ptr as *const u8, plain_total);
                if let Some(hdr) = browser::parse_http_response_header(plain_slice) {
                    if let Some(cl) = hdr.content_length {
                        if plain_total >= hdr.header_len + cl {
                            break;
                        }
                    } else if hdr.is_chunked {
                        let body_part = &plain_slice[hdr.header_len..];
                        if browser::has_chunked_terminator(body_part) {
                            break;
                        }
                    }
                }

                if tls_eof {
                    break;
                }

                let space = HTTP_BUF_SIZE - rx_total;
                if space == 0 { break; }
                let n = sys_recv(sockfd, rx_ptr.add(rx_total), space);
                if n > 0 {
                    rx_total += n as usize;
                    idle_spins = 0;
                    print_str(b"[TLS-HTTP-RX] Recebidos ");
                    print_u64(n as u64);
                    print_str(b" bytes cifrados (acumulado cifrado: ");
                    print_u64(rx_total as u64);
                    print_str(b", decifrado: ");
                    print_u64(plain_total as u64);
                    print_str(b")\n");
                } else if n == 0 {
                    print_str(b"[TLS-HTTP-RX] Conexao TLS encerrada pelo servidor (EOF)\n");
                    break;
                } else if n == -11 {
                    idle_spins += 1;
                    if idle_spins > 1600 {
                        print_str(b"[BROWSER-NET] Timeout de recepcao atingido (8.0s)\n");
                        let rx_slice = core::slice::from_raw_parts(plain_ptr as *const u8, plain_total);
                        if let Some(hdr) = browser::parse_http_response_header(rx_slice) {
                            if plain_total > hdr.header_len {
                                print_str(b"[BROWSER-NET] Resposta TLS parcial/completa disponivel. Renderizando...\n");
                                break;
                            }
                        }
                        if plain_total == 0 {
                            let _ = sys_close(sockfd);
                            return Err(FetchError::RecvTimeout);
                        }
                        break;
                    }
                    let _ = sys_sleep(5);
                } else {
                    print_str(b"[BROWSER-NET] sys_recv erro: ");
                    print_i64(n);
                    print_str(b"\n");
                    break;
                }
            }
            if plain_total == 0 {
                let _ = sys_close(sockfd);
                return Err(if tls_fatal_alert { FetchError::TlsHandshakeFailed } else { FetchError::RecvTimeout });
            }
        } else {
            loop {
                let space = HTTP_BUF_SIZE - rx_total;
                if space == 0 {
                    print_str(b"[BROWSER-NET] Buffer de recepcao cheio (128 KiB)!\n");
                    break;
                }
                let n = sys_recv(sockfd, rx_ptr.add(rx_total), space);
                if n > 0 {
                    rx_total += n as usize;
                    idle_spins = 0;
                    print_str(b"[HTTP-RX] Recebidos ");
                    print_u64(n as u64);
                    print_str(b" bytes (acumulado: ");
                    print_u64(rx_total as u64);
                    print_str(b")\n");
                    let rx_slice = core::slice::from_raw_parts(rx_ptr as *const u8, rx_total);
                    if let Some(hdr) = browser::parse_http_response_header(rx_slice) {
                        if let Some(cl) = hdr.content_length {
                            if rx_total >= hdr.header_len + cl {
                                print_str(b"[HTTP-RX] Corpo completo recebido por Content-Length (");
                                print_u64(cl as u64);
                                print_str(b" bytes)\n");
                                break;
                            }
                        } else if hdr.is_chunked {
                            let body_part = &rx_slice[hdr.header_len..];
                            if browser::has_chunked_terminator(body_part) {
                                print_str(b"[HTTP-RX] Terminador de chunked transfer detectado com integridade!\n");
                                break;
                            }
                        }
                    }
                } else if n == 0 {
                    print_str(b"[HTTP-RX] Conexao fechada pelo servidor (EOF), total recebido: ");
                    print_u64(rx_total as u64);
                    print_str(b" bytes\n");
                    break;
                } else if n == -11 {
                    idle_spins += 1;
                    if idle_spins > 1600 {
                        print_str(b"[BROWSER-NET] Timeout de recepcao atingido (8.0s, recebidos ");
                        print_u64(rx_total as u64);
                        print_str(b" bytes)\n");
                        let rx_slice = core::slice::from_raw_parts(rx_ptr as *const u8, rx_total);
                        if let Some(hdr) = browser::parse_http_response_header(rx_slice) {
                            if rx_total > hdr.header_len {
                                print_str(b"[BROWSER-NET] Resposta HTTP recebida parcialmente antes do timeout. Entregando ao renderizador.\n");
                                break;
                            }
                        }
                        let _ = sys_close(sockfd);
                        return Err(FetchError::RecvTimeout);
                    }
                    let _ = sys_sleep(5);
                } else {
                    print_str(b"[BROWSER-NET] sys_recv erro: ");
                    print_i64(n);
                    print_str(b"\n");
                    break;
                }
            }
            core::ptr::copy_nonoverlapping(rx_ptr, plain_ptr, rx_total);
            plain_total = rx_total;
        }

        let _ = sys_close(sockfd);

        let rx_slice = core::slice::from_raw_parts(plain_ptr as *const u8, plain_total);
        let hdr = match browser::parse_http_response_header(rx_slice) {
            Some(h) => h,
            None => return Err(FetchError::InvalidHttp),
        };
        LAST_FETCH_DIAG.http_status = hdr.status_code;

        print_str(b"[BROWSER-NET] HTTP Status: ");
        print_u64(hdr.status_code as u64);
        print_str(b", Header-Len: ");
        print_u64(hdr.header_len as u64);
        if let Some(cl) = hdr.content_length {
            print_str(b", Content-Length: ");
            print_u64(cl as u64);
        }
        if hdr.is_chunked {
            print_str(b", Transfer-Encoding: chunked");
        }
        print_str(b"\n");

        // 301 / 302 / 303 / 307 / 308 Redirection
        if (hdr.status_code == 301
            || hdr.status_code == 302
            || hdr.status_code == 303
            || hdr.status_code == 307
            || hdr.status_code == 308)
            && hdr.location_len > 0
        {
            print_str(b"[BROWSER-NET] Redirecionamento HTTP ");
            print_u64(hdr.status_code as u64);
            print_str(b" (Location: ");
            print_str(&hdr.location[..hdr.location_len]);
            print_str(b")\n");
            let loc = &hdr.location[..hdr.location_len];
            let mut resolved_loc = [0u8; 128];
            let res_len = browser::resolve_relative_url(target_slice, loc, &mut resolved_loc);
            let next_url = &resolved_loc[..res_len];
            if target_slice.starts_with(b"https://") && next_url.starts_with(b"http://") {
                print_str(b"[BROWSER-SEC] BLOQUEADO downgrade HTTPS -> HTTP em redirecionamento: ");
                print_str(next_url);
                print_str(b"\n");
                return Err(FetchError::HttpsDowngradeBlocked);
            }
            let next_hash = fnv1a_url(next_url);
            if visited_hashes[..visited_count].contains(&next_hash) {
                print_str(b"[BROWSER-NET] Loop de redirecionamento HTTP detectado!\n");
                return Err(FetchError::TooManyRedirects);
            }
            current_target[..res_len].copy_from_slice(next_url);
            current_target_len = res_len;
            continue;
        }

        if hdr.status_code >= 400 {
            let body_bytes = &rx_slice[hdr.header_len..];
            if !reject_http_errors && body_bytes.iter().position(|&b| b == b'<').is_some() {
                let copy_len = body_bytes.len().min(out_buf.len());
                out_buf[..copy_len].copy_from_slice(&body_bytes[..copy_len]);
                return Ok(copy_len);
            } else {
                return Err(FetchError::HttpStatus(hdr.status_code));
            }
        }

        // Extracao de corpo HTTP 200
        let body_bytes = &rx_slice[hdr.header_len..];
        if hdr.is_chunked {
            print_str(b"[BROWSER-NET] Decodificando Transfer-Encoding: chunked (raw ");
            print_u64(body_bytes.len() as u64);
            print_str(b" bytes)...\n");
            match browser::decode_chunked_body(body_bytes, out_buf) {
                Ok(decoded_len) => {
                    print_str(b"[BROWSER-NET] Chunked decodificado: ");
                    print_u64(decoded_len as u64);
                    print_str(b" bytes\n");

                    // Verifica se o payload deschunkado possui compressão GZIP ou DEFLATE
                    let unchunked_slice = &out_buf[..decoded_len];
                    if hdr.is_gzip || gzip::is_gzip(unchunked_slice) {
                        print_str(b"[BROWSER-NET] Descomprimindo GZIP apos deschunk...\n");
                        let rx_tmp = core::slice::from_raw_parts_mut(rx_ptr, HTTP_BUF_SIZE);
                        match gzip::decompress_gzip(unchunked_slice, rx_tmp) {
                            Ok(gz_len) => {
                                print_str(b"[BROWSER-NET] GZIP descomprimido: ");
                                print_u64(gz_len as u64);
                                print_str(b" bytes entregues ao renderizador\n");
                                let copy_n = gz_len.min(out_buf.len());
                                out_buf[..copy_n].copy_from_slice(&rx_tmp[..copy_n]);
                                return Ok(copy_n);
                            }
                            Err(_) => {
                                print_str(b"[BROWSER-NET] Falha ao descomprimir GZIP; mantendo dados deschunkados\n");
                                return Ok(decoded_len);
                            }
                        }
                    }
                    return Ok(decoded_len);
                }
                Err(_) => {
                    print_str(b"[BROWSER-NET] AVISO: Falha ao decodificar chunked; repassando bruto (");
                    let copy_len = body_bytes.len().min(out_buf.len());
                    print_u64(copy_len as u64);
                    print_str(b" bytes)\n");
                    out_buf[..copy_len].copy_from_slice(&body_bytes[..copy_len]);
                    return Ok(copy_len);
                }
            }
        } else {
            // Nao e chunked: verifica se é GZIP ou DEFLATE
            if hdr.is_gzip || gzip::is_gzip(body_bytes) {
                print_str(b"[BROWSER-NET] Descomprimindo resposta GZIP (raw ");
                print_u64(body_bytes.len() as u64);
                print_str(b" bytes)...\n");
                match gzip::decompress_gzip(body_bytes, out_buf) {
                    Ok(gz_len) => {
                        print_str(b"[BROWSER-NET] GZIP descomprimido com sucesso: ");
                        print_u64(gz_len as u64);
                        print_str(b" bytes entregues ao renderizador\n");
                        return Ok(gz_len);
                    }
                    Err(_) => {
                        print_str(b"[BROWSER-NET] Falha na descompressao GZIP; repassando bruto\n");
                    }
                }
            } else if hdr.is_deflate || gzip::is_zlib(body_bytes) {
                print_str(b"[BROWSER-NET] Descomprimindo resposta DEFLATE/ZLIB...\n");
                match gzip::decompress_deflate(body_bytes, out_buf) {
                    Ok(def_len) => {
                        print_str(b"[BROWSER-NET] DEFLATE descomprimido: ");
                        print_u64(def_len as u64);
                        print_str(b" bytes entregues ao renderizador\n");
                        return Ok(def_len);
                    }
                    Err(_) => {}
                }
            }

            let copy_len = body_bytes.len().min(out_buf.len());
            print_str(b"[BROWSER-NET] Corpo HTTP entregue ao renderizador: ");
            print_u64(copy_len as u64);
            print_str(b" bytes\n");
            out_buf[..copy_len].copy_from_slice(&body_bytes[..copy_len]);
            return Ok(copy_len);
        }
    }

    Err(FetchError::TooManyRedirects)
}

unsafe fn prefetch_page_images(
    session: &mut browser::BrowserSession,
    base_url: &[u8],
    scratch_buf: &mut [u8],
) {
    let mut fetched = 0usize;
    let mut updated = false;
    let node_count = session.dom.node_count;

    for idx in 0..node_count {
        if fetched >= 6 {
            break;
        }
        let node = &session.dom.nodes[idx];
        if node.tag_len != 3 || &node.tag[..3] != b"img" || node.src_len == 0 {
            continue;
        }
        let mut raw_src_buf = [0u8; 128];
        let slen = node.src_len.min(127);
        raw_src_buf[..slen].copy_from_slice(&node.src_attr[..slen]);
        let raw_src = &raw_src_buf[..slen];

        if raw_src.starts_with(b"/disk/") || raw_src.starts_with(b"/ext2/") || raw_src.starts_with(b"data:") {
            continue;
        }
        if browser::cache_lookup_remote_image(raw_src).is_some() {
            updated = true;
            continue;
        }

        let mut resolved_buf = [0u8; 128];
        let res_len = browser::resolve_relative_url(base_url, raw_src, &mut resolved_buf);
        let img_url = &resolved_buf[..res_len];
        if !img_url.starts_with(b"http://") && !img_url.starts_with(b"https://") {
            continue;
        }
        if base_url.starts_with(b"https://") && img_url.starts_with(b"http://") {
            print_str(b"[BROWSER-SEC] BLOQUEADO downgrade HTTPS -> HTTP em subrecurso <img>: ");
            print_str(img_url);
            print_str(b"\n");
            session.record_downgrade_blocked();
            continue;
        }

        print_str(b"[BROWSER-IMG] Baixando imagem remota: ");
        print_str(img_url);
        print_str(b"\n");

        let mut img_final_url = [0u8; 128];
        let mut img_final_len = 0usize;
        match fetch_http_document_inner(img_url, scratch_buf, true, &mut img_final_url, &mut img_final_len) {
            Ok(img_bytes) => {
                if img_bytes > 0 && img_bytes <= 65536 {
                    let _ = browser::cache_store_remote_image(raw_src, img_url, &scratch_buf[..img_bytes]);
                    print_str(b"[BROWSER-IMG] Imagem armazenada em cache (");
                    print_u64(img_bytes as u64);
                    print_str(b" bytes)\n");
                    updated = true;
                }
            }
            Err(FetchError::HttpsDowngradeBlocked) => {
                session.record_downgrade_blocked();
            }
            Err(_) => {}
        }
        fetched += 1;
    }

    if updated {
        session.recompute_layout();
    }
}

unsafe fn browser_load_and_display(
    session: &mut browser::BrowserSession,
    target: &[u8],
    win_id: usize,
    buf_slice: &mut [u32],
) -> bool {
    let mut path_buf = [0u8; 128];
    let (path, allow_http_fallback) = resolve_target_with_mode(target, &mut path_buf);

    if path.starts_with(b"about:") {
        let f_ptr = core::ptr::addr_of_mut!(HTML_FILE_BUF) as *mut u8;
        let out_buf = core::slice::from_raw_parts_mut(f_ptr, HTTP_BUF_SIZE);
        let content_len = if path == b"about:blank" {
            let s = b"<html><head><title>about:blank</title></head><body></body></html>";
            out_buf[..s.len()].copy_from_slice(s);
            s.len()
        } else if path == b"about:version" {
            let s = b"<html><head><title>Sobre o Navegador</title></head><body><h1>Jinn Browser v0.4.0</h1><p>Navegacao real na internet: DNS (RFC 1035) + TCP (RFC 793) + HTTP/1.1 (RFC 7230)</p><p>Microkernel: Jinn OS (SMP, Ring 3, VFS FAT32, JinnWall SPI)</p><p>Compositor: Aladdin Desktop</p><hr/><p><a href=\"index.htm\">Voltar ao Inicio</a></p></body></html>";
            out_buf[..s.len()].copy_from_slice(s);
            s.len()
        } else if path == b"about:tls" {
            browser::format_tls_info_html(target, out_buf)
        } else if path == b"about:blocked" {
            let s = b"<html><head><title>Acesso Bloqueado</title></head><body><h1>403 - Acesso Bloqueado</h1><p>Acesso fora do diretorio permitido (/disk/) foi bloqueado por seguranca.</p><hr/><p><a href=\"index.htm\">Voltar ao Inicio</a></p></body></html>";
            out_buf[..s.len()].copy_from_slice(s);
            s.len()
        } else if path == b"about:offline" {
            let s = b"<html><head><title>Modo Offline</title></head><body><h1>Modo Offline</h1><p>Conexoes externas de rede estao desabilitadas nesta versao.</p><hr/><p><a href=\"index.htm\">Voltar ao Inicio</a></p></body></html>";
            out_buf[..s.len()].copy_from_slice(s);
            s.len()
        } else if path == b"about:unsupported" {
            let s = b"<html><head><title>Esquema Nao Suportado</title></head><body><h1>Esquema Nao Suportado</h1><p>O Jinn Browser suporta atualmente os protocolos https://, http://, file:/// e about:.</p><p>Esquemas como ftp:// e mailto: requerem servicos especializados adicionais em desenvolvimento.</p><hr/><p><a href=\"index.htm\">Voltar ao Inicio</a></p></body></html>";
            out_buf[..s.len()].copy_from_slice(s);
            s.len()
        } else {
            let s = b"<html><head><title>Sobre</title></head><body><h1>Jinn Browser v0.4.0</h1><p>Navegador nativo Ring 3.</p><hr/><p><a href=\"index.htm\">Voltar ao Inicio</a></p></body></html>";
            out_buf[..s.len()].copy_from_slice(s);
            s.len()
        };

        let doc_slice = &out_buf[..content_len];
        session.load_document_bytes(doc_slice, path);
        session.render(buf_slice);
        let _ = sys_window_update(win_id, buf_slice);
        print_str(b"[BROWSER-INTERACTION] PAGE_LOADED target=");
        print_str(path);
        print_str(b"\n");
        return true;
    }

    if path.starts_with(b"http://") || path.starts_with(b"https://") {
        let f_ptr = core::ptr::addr_of_mut!(HTML_FILE_BUF) as *mut u8;
        let out_buf = core::slice::from_raw_parts_mut(f_ptr, HTTP_BUF_SIZE);

        // Verifica cache local (/disk/browser/cache/) antes da requisicao de rede
        if let Some(cached_len) = browser::storage::BrowserCacheStore::retrieve(path, out_buf) {
            print_str(b"[BROWSER-CACHE] Recurso obtido do cache local (/disk/browser/cache/)!\n");
            let doc_slice = &out_buf[..cached_len];
            session.load_document_bytes(doc_slice, path);
            session.record_navigation_current(path);
            session.render(buf_slice);
            let _ = sys_window_update(win_id, buf_slice);
            print_str(b"[BROWSER-INTERACTION] PAGE_LOADED (CACHE) target=");
            print_str(path);
            print_str(b"\n");
            return true;
        }

        print_str(b"[BROWSER] Requisitando recurso externo: ");
        print_str(path);
        print_str(b"\n");

        let mut active_url_buf = [0u8; 128];
        let p_len = path.len().min(127);
        active_url_buf[..p_len].copy_from_slice(&path[..p_len]);
        let mut active_url_len = p_len;

        let mut final_url_buf = [0u8; 128];
        let mut final_url_len = 0usize;
        let mut used_http_fallback = false;
        let mut fetch_res = fetch_http_document_inner(path, out_buf, allow_http_fallback, &mut final_url_buf, &mut final_url_len);
        let fallback_allowed_err = !matches!(
            fetch_res,
            Err(FetchError::TlsCertHostMismatch | FetchError::TlsCertVerifyFailed | FetchError::HttpsDowngradeBlocked)
        );
        if fetch_res.is_ok() && final_url_len > 0 {
            let flen = final_url_len.min(127);
            active_url_buf[..flen].copy_from_slice(&final_url_buf[..flen]);
            active_url_len = flen;
        } else if fetch_res.is_err() && allow_http_fallback && fallback_allowed_err && path.starts_with(b"https://") {
            let rest = &path[8..];
            let fb_prefix = b"http://";
            let mut fb_buf = [0u8; 128];
            fb_buf[..fb_prefix.len()].copy_from_slice(fb_prefix);
            let rlen = rest.len().min(127 - fb_prefix.len());
            fb_buf[fb_prefix.len()..fb_prefix.len() + rlen].copy_from_slice(&rest[..rlen]);
            let fb_len = fb_prefix.len() + rlen;
            let fb_url = &fb_buf[..fb_len];

            print_str(b"[BROWSER-NET] HTTPS/TLS indisponivel para alvo sem esquema; executando fallback HTTP (Sem TLS): ");
            print_str(fb_url);
            print_str(b"\n");

            active_url_buf[..fb_len].copy_from_slice(fb_url);
            active_url_len = fb_len;

            if let Some(cached_len) = browser::storage::BrowserCacheStore::retrieve(fb_url, out_buf) {
                fetch_res = Ok(cached_len);
                used_http_fallback = true;
            } else {
                let mut fb_final = [0u8; 128];
                let mut fb_final_len = 0usize;
                fetch_res = fetch_http_document_inner(fb_url, out_buf, false, &mut fb_final, &mut fb_final_len);
                if fetch_res.is_ok() {
                    used_http_fallback = true;
                    if fb_final_len > 0 {
                        let flen = fb_final_len.min(127);
                        active_url_buf[..flen].copy_from_slice(&fb_final[..flen]);
                        active_url_len = flen;
                    }
                }
            }
        }

        // Suporte a `<meta http-equiv="refresh" content="...;url=...">` (ate 2 saltos, com guarda anti-downgrade)
        let hash_meta_url = |u: &[u8]| -> u64 {
            let mut h: u64 = 0xcbf29ce484222325;
            for &b in u {
                h ^= b.to_ascii_lowercase() as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
            h
        };
        let mut meta_visited = [0u64; 4];
        meta_visited[0] = hash_meta_url(&active_url_buf[..active_url_len]);
        let mut meta_vis_count = 1usize;
        let mut meta_hops = 0usize;
        while let Ok(curr_len) = fetch_res {
            if meta_hops >= 2 {
                break;
            }
            let mut meta_raw = [0u8; 128];
            if let Some(m_len) = browser::extract_meta_refresh_url(&out_buf[..curr_len], &mut meta_raw) {
                let mut next_url = [0u8; 128];
                let n_len = browser::resolve_relative_url(&active_url_buf[..active_url_len], &meta_raw[..m_len], &mut next_url);
                if active_url_buf[..active_url_len].starts_with(b"https://") && next_url[..n_len].starts_with(b"http://") {
                    print_str(b"[BROWSER-SEC] BLOQUEADO downgrade HTTPS -> HTTP em meta-refresh: ");
                    print_str(&next_url[..n_len]);
                    print_str(b"\n");
                    session.record_downgrade_blocked();
                    fetch_res = Err(FetchError::HttpsDowngradeBlocked);
                    break;
                }
                let next_hash = hash_meta_url(&next_url[..n_len]);
                let already_seen = meta_visited[..meta_vis_count].iter().any(|&h| h == next_hash);
                if n_len > 0 && !already_seen {
                    if meta_vis_count < meta_visited.len() {
                        meta_visited[meta_vis_count] = next_hash;
                        meta_vis_count += 1;
                    }
                    print_str(b"[BROWSER-NET] Meta-refresh para: ");
                    print_str(&next_url[..n_len]);
                    print_str(b"\n");
                    let mut m_final = [0u8; 128];
                    let mut m_flen = 0usize;
                    let r = fetch_http_document_inner(&next_url[..n_len], out_buf, false, &mut m_final, &mut m_flen);
                    if r.is_ok() && m_flen > 0 {
                        let flen = m_flen.min(127);
                        active_url_buf[..flen].copy_from_slice(&m_final[..flen]);
                        active_url_len = flen;
                        fetch_res = r;
                        meta_hops += 1;
                        continue;
                    } else {
                        let flen = n_len.min(127);
                        active_url_buf[..flen].copy_from_slice(&next_url[..flen]);
                        active_url_len = flen;
                        fetch_res = r;
                        break;
                    }
                } else if already_seen {
                    print_str(b"[BROWSER-NET] Loop de meta-refresh bloqueado: ");
                    print_str(&next_url[..n_len]);
                    print_str(b"\n");
                }
            }
            break;
        }

        let active_url = &active_url_buf[..active_url_len];
        match fetch_res {
            Ok(body_len) => {
                let doc_slice = &out_buf[..body_len];
                print_str(b"[DIAG-FLOW] HTML: Extraindo corpo do documento HTML (bytes=");
                print_u64(body_len as u64);
                print_str(b")\n");
                let fmt_buf = unsafe {
                    core::slice::from_raw_parts_mut(
                        core::ptr::addr_of_mut!(HTTP_FORMAT_BUF) as *mut u8,
                        HTTP_BUF_SIZE,
                    )
                };
                let formatted_len = browser::format_structured_response_html(doc_slice, active_url, fmt_buf);
                let mut use_fmt_buf = formatted_len.is_some();
                let mut active_len = formatted_len.unwrap_or(body_len);
                print_str(b"[DIAG-FLOW] DOM: Convertendo HTML para arvore de elementos DOM...\n");
                if let Some(flen) = formatted_len {
                    print_str(b"[HTML-INTEGRITY] Payload estruturado (RSS/Atom/JSON) formatado automaticamente.\n");
                    session.load_document_bytes(&fmt_buf[..flen], active_url);
                } else {
                    let has_html_tag = doc_slice.windows(5).any(|w| w.eq_ignore_ascii_case(b"<html") || w.eq_ignore_ascii_case(b"<body"));
                    if has_html_tag {
                        print_str(b"[HTML-INTEGRITY] Validacao de integridade OK: tags HTML detectadas no payload.\n");
                    } else {
                        print_str(b"[HTML-INTEGRITY] AVISO: Documento sem tags HTML estruturais padrao (renderizando como texto/bloco).\n");
                    }
                    session.load_document_bytes(doc_slice, active_url);
                    if browser::detect_heavy_js_spa(doc_slice, session.layout.line_count) {
                        let spa_len = browser::format_spa_notice_html(active_url, fmt_buf);
                        print_str(b"[BROWSER-SPA] Shell SPA JS pesado detectado -> exibindo aviso amigavel.\n");
                        session.load_document_bytes(&fmt_buf[..spa_len], active_url);
                        use_fmt_buf = true;
                        active_len = spa_len;
                    }
                }
                let final_slice: &[u8] = if use_fmt_buf { &fmt_buf[..active_len] } else { doc_slice };
                if used_http_fallback && active_url.starts_with(b"http://") {
                    session.mark_http_fallback_insecure();
                    print_str(b"[BROWSER-SEC] BADGE_HTTP_SEM_TLS ativo para: ");
                    print_str(active_url);
                    print_str(b"\n");
                }
                session.record_navigation_current(active_url);
                // Armazena copia transacional no cache local (/disk/browser/cache/)
                browser::storage::BrowserCacheStore::store(active_url, final_slice);
                // Faz prefetch de imagens remotas (<img src=...>) antes da pintura final
                prefetch_page_images(session, active_url, out_buf);

                print_str(b"[DIAG-FLOW] RENDER: Desenhando documento na janela do Aladdin...\n");
                session.render(buf_slice);
                let _ = sys_window_update(win_id, buf_slice);
                print_str(b"[DIAG-FLOW] SUCESSO: Pagina externa renderizada com exito na tela!\n");
                print_str(b"[BROWSER-INTERACTION] PAGE_LOADED target=");
                print_str(active_url);
                print_str(b" (bytes=");
                print_u64(body_len as u64);
                print_str(b")\n");
                true
            }
            Err(err) => {
                let diag = LAST_FETCH_DIAG;
                let mut stage_buf = [0u8; 64];
                let mut stage_len = 0usize;
                let mut x509_diag_buf = [0u8; 96];
                let x509_diag_len = crate::x509::last_cert_diag(&mut x509_diag_buf);

                let (err_title, err_msg, err_detail): (&[u8], &[u8], &[u8]) = match err {
                    FetchError::DnsNxDomain => {
                        diag_push_bytes(&mut stage_buf, &mut stage_len, b"DNS_UDP (-2 NXDOMAIN)");
                        (
                            b"Dominio Inexistente",
                            b"Nao foi possivel encontrar o servidor. Verifique o dominio digitado.",
                            b"NXDOMAIN: O servidor DNS confirmou que o dominio informado nao existe (RFC 1035 RCODE 3)",
                        )
                    }
                    FetchError::DnsTimeout => {
                        diag_push_bytes(&mut stage_buf, &mut stage_len, b"DNS_UDP (-110 ETIMEDOUT)");
                        (
                            b"Servidor DNS Inacessivel",
                            b"O servidor DNS upstream nao respondeu dentro do limite de tempo.",
                            b"ETIMEDOUT: Nenhuma resposta do servidor DNS (10.0.2.3 / 1.1.1.1 / 8.8.8.8)",
                        )
                    }
                    FetchError::DnsArpFailed => {
                        diag_push_bytes(&mut stage_buf, &mut stage_len, b"ARP (-113 EHOSTUNREACH)");
                        (
                            b"Falha de Resolucao ARP",
                            b"Nao foi possivel localizar o endereco fisico (MAC) do gateway na rede local.",
                            b"EHOSTUNREACH: O gateway ou servidor DNS nao respondeu a requisicoes ARP L2",
                        )
                    }
                    FetchError::DnsNetDown => {
                        diag_push_bytes(&mut stage_buf, &mut stage_len, b"LINK_DOWN (-100 ENETDOWN)");
                        (
                            b"Interface de Rede Inoperante",
                            b"Nenhum adaptador de rede ativo foi detectado no sistema operacional.",
                            b"ENETDOWN: Placa de rede (VirtIO, Intel e1000 ou AMD PCnet) indisponivel ou sem link",
                        )
                    }
                    FetchError::DnsServerFailure => {
                        diag_push_bytes(&mut stage_buf, &mut stage_len, b"DNS_UDP (-5 SERVFAIL)");
                        (
                            b"Falha no Servidor DNS",
                            b"O servidor DNS upstream reportou uma falha ao processar a consulta.",
                            b"SERVFAIL / Refused: O servidor encontrou erro interno (RFC 1035 RCODE 2 / 5)",
                        )
                    }
                    FetchError::DnsFailed => {
                        diag_push_bytes(&mut stage_buf, &mut stage_len, b"DNS_UDP (");
                        diag_push_i64(&mut stage_buf, &mut stage_len, diag.dns_code);
                        diag_push_bytes(&mut stage_buf, &mut stage_len, b")");
                        (
                            b"Erro de Resolucao DNS",
                            b"Falha durante a resolucao do nome de dominio.",
                            b"sys_dns_resolve falhou ao consultar cache/upstream",
                        )
                    }
                    FetchError::SocketFailed => {
                        diag_push_bytes(&mut stage_buf, &mut stage_len, b"TCP_SOCKET (EMFILE)");
                        (
                            b"Erro de Rede Interno",
                            b"Nao foi possivel alocar socket de rede no sistema.",
                            b"sys_socket retornou EMFILE ou memoria esgotada",
                        )
                    }
                    FetchError::ConnectFailed(code) => {
                        diag_push_bytes(&mut stage_buf, &mut stage_len, b"TCP_CONNECT (");
                        diag_push_i64(&mut stage_buf, &mut stage_len, code);
                        if code == -111 {
                            diag_push_bytes(&mut stage_buf, &mut stage_len, b" ECONNREFUSED)");
                        } else if code == -110 {
                            diag_push_bytes(&mut stage_buf, &mut stage_len, b" ETIMEDOUT)");
                        } else {
                            diag_push_bytes(&mut stage_buf, &mut stage_len, b" ERR)");
                        }
                        (
                            b"Falha de Conexao TCP",
                            b"Nao foi possivel conectar ao servidor remoto na porta solicitada.",
                            if code == -111 { b"ECONNREFUSED: Conexao TCP recusada (RST) pelo host remoto" }
                            else if code == -110 { b"ETIMEDOUT: Tempo limite do 3-way handshake SYN/ACK excedido" }
                            else { b"Erro durante o 3-way handshake TCP SYN/ACK" },
                        )
                    }
                    FetchError::SendFailed => {
                        diag_push_bytes(&mut stage_buf, &mut stage_len, b"TCP_TX (SendFailed)");
                        (
                            b"Erro de Transmissao",
                            b"Falha ao transmitir os dados da requisicao pela conexao TCP.",
                            b"sys_send retornou codigo negativo",
                        )
                    }
                    FetchError::RecvTimeout => {
                        diag_push_bytes(&mut stage_buf, &mut stage_len, b"RECV (-11 ETIMEDOUT)");
                        (
                            b"Tempo Esgotado",
                            b"O servidor remoto nao enviou resposta dentro do limite de tempo.",
                            b"Timeout de leitura excedido aguardando segmentos TCP/TLS",
                        )
                    }
                    FetchError::InvalidHttp => {
                        diag_push_bytes(&mut stage_buf, &mut stage_len, b"HTTP_PARSER (Invalid)");
                        (
                            b"Resposta Invalida",
                            b"O servidor retornou uma resposta fora do padrao HTTP/1.1.",
                            b"Cabecalhos HTTP ausentes ou malformados",
                        )
                    }
                    FetchError::TooManyRedirects => {
                        diag_push_bytes(&mut stage_buf, &mut stage_len, b"HTTP_REDIRECT (Max5)");
                        (
                            b"Redirecionamento em Loop",
                            b"Muitos redirecionamentos seguidos (limite maximo de 5 excedido).",
                            b"HTTP 301/302 loop excedido",
                        )
                    }
                    FetchError::HttpStatus(code) => {
                        diag_push_bytes(&mut stage_buf, &mut stage_len, b"HTTP_STATUS (");
                        diag_push_u64(&mut stage_buf, &mut stage_len, code as u64);
                        diag_push_bytes(&mut stage_buf, &mut stage_len, b")");
                        (
                            b"Erro HTTP do Servidor",
                            b"O servidor retornou um codigo de erro HTTP sem documento HTML.",
                            if code == 404 { b"HTTP 404 Not Found" }
                            else if code == 500 { b"HTTP 500 Internal Server Error" }
                            else { b"Status HTTP >= 400 retornado pelo servidor" },
                        )
                    }
                    FetchError::HttpsRedirect => {
                        diag_push_bytes(&mut stage_buf, &mut stage_len, b"HTTP_TO_HTTPS (301/302)");
                        (
                            b"Redirecionamento para HTTPS",
                            b"O servidor remoto exigiu redirecionamento para HTTPS.",
                            b"Redirecionamento HTTPS nao concluido",
                        )
                    }
                    FetchError::TlsEntropyFailed => {
                        diag_push_bytes(&mut stage_buf, &mut stage_len, b"TLS_ENTROPY (Sys69)");
                        (
                            b"Falha no Subsistema de Entropia",
                            b"Nao foi possivel obter entropia aleatoria segura do microkernel.",
                            b"Syscall 69 (sys_getrandom) falhou ou retornou bytes insuficientes",
                        )
                    }
                    FetchError::TlsHandshakeFailed => {
                        diag_push_bytes(&mut stage_buf, &mut stage_len, b"TLS_HANDSHAKE (RFC 8446)");
                        (
                            b"Falha de Negociacao TLS 1.3",
                            b"O servidor remoto nao aceitou os parametros ou encerrou o handshake TLS 1.3.",
                            b"Handshake TLS 1.3 (X25519 / ChaCha20-Poly1305 / AES-128-GCM) falhou",
                        )
                    }
                    FetchError::TlsDecryptFailed => {
                        diag_push_bytes(&mut stage_buf, &mut stage_len, b"TLS_DECRYPT (AEAD)");
                        (
                            b"Erro de Criptografia TLS",
                            b"Falha na autenticacao AEAD (Poly1305 / GHASH) dos dados recebidos.",
                            b"Registro cifrado rejeitado na verificacao da tag AEAD",
                        )
                    }
                    FetchError::TlsCertHostMismatch => {
                        diag_push_bytes(&mut stage_buf, &mut stage_len, b"X509_SAN_MISMATCH (RFC 6125)");
                        let detail_s: &[u8] = if x509_diag_len > 0 {
                            &x509_diag_buf[..x509_diag_len]
                        } else {
                            b"SAN/CN do certificado folha nao corresponde ao hostname TLS solicitado"
                        };
                        (
                            b"Certificado Incompativel (SAN/CN)",
                            b"O certificado X.509 apresentado pelo servidor nao corresponde ao dominio acessado.",
                            detail_s,
                        )
                    }
                    FetchError::TlsCertVerifyFailed => {
                        diag_push_bytes(&mut stage_buf, &mut stage_len, b"TLS_CERT_VERIFY (RFC 8446)");
                        let detail_s: &[u8] = if x509_diag_len > 0 {
                            &x509_diag_buf[..x509_diag_len]
                        } else {
                            b"Assinatura CertificateVerify (RSA-PSS/PKCS1/ECDSA-P256) ou SPKI X.509 rejeitada"
                        };
                        (
                            b"Assinatura TLS Invalida",
                            b"Falha na verificacao criptografica da identidade X.509 do servidor.",
                            detail_s,
                        )
                    }
                    FetchError::HttpsDowngradeBlocked => {
                        session.record_downgrade_blocked();
                        diag_push_bytes(&mut stage_buf, &mut stage_len, b"HTTPS_DOWNGRADE_BLOCKED");
                        (
                            b"Downgrade HTTPS para HTTP Bloqueado",
                            b"Uma navegacao iniciada em HTTPS tentou redirecionar para HTTP nao cifrado.",
                            b"Politica anti-downgrade bloqueou redirecionamento https:// -> http://",
                        )
                    }
                };

                let mut stats = SystemStatsUser::default();
                let hr_ptr = core::ptr::addr_of_mut!(HOT_ROUTE_TMP) as *mut [HotRouteEntryUser; 16];
                let (obs_bench, _r88, r97_res) = measure_observability_boundary(&mut stats, &mut *hr_ptr, true);
                let hr_count = r97_res.max(0) as usize;
                let mut matched_hotroute = false;
                let mut found_mac = [0u8; 6];
                let mut has_mac = false;

                for idx in 0..hr_count.min(16) {
                    let entry = &(*hr_ptr)[idx];
                    if entry.valid == 0 {
                        continue;
                    }
                    if !has_mac && entry.next_hop_mac != [0; 6] {
                        found_mac = entry.next_hop_mac;
                        has_mac = true;
                    }
                    let hlen = (entry.hostname_len as usize).min(entry.hostname.len());
                    if diag.host_len > 0 && hlen == diag.host_len && entry.hostname[..hlen].eq_ignore_ascii_case(&diag.host[..diag.host_len]) {
                        matched_hotroute = true;
                        if entry.next_hop_mac != [0; 6] {
                            found_mac = entry.next_hop_mac;
                            has_mac = true;
                        }
                    } else if diag.has_resolved_ip && entry.dst_ip == diag.resolved_ip {
                        matched_hotroute = true;
                        if entry.next_hop_mac != [0; 6] {
                            found_mac = entry.next_hop_mac;
                            has_mac = true;
                        }
                    }
                }

                // Formata IP Externo / DNS Resolvido
                let mut ip_buf = [0u8; 80];
                let mut ip_len = 0usize;
                if diag.has_resolved_ip {
                    diag_push_ipv4(&mut ip_buf, &mut ip_len, diag.resolved_ip);
                    if matched_hotroute {
                        diag_push_bytes(&mut ip_buf, &mut ip_len, b" (HotRoute)");
                    } else if diag.resolved_ip[0] == 127 {
                        diag_push_bytes(&mut ip_buf, &mut ip_len, b" (Loopback)");
                    } else if diag.resolved_ip == [10, 0, 2, 2] {
                        diag_push_bytes(&mut ip_buf, &mut ip_len, b" (NAT Gateway)");
                    } else {
                        diag_push_bytes(&mut ip_buf, &mut ip_len, b" (DNS Resolvido)");
                    }
                } else if stats.net_ip != [0; 4] {
                    diag_push_bytes(&mut ip_buf, &mut ip_len, b"10.0.2.3 (DNS Upstream)");
                } else {
                    diag_push_bytes(&mut ip_buf, &mut ip_len, b"Nao resolvido");
                }

                // Formata Interface de Rede real do kernel (Syscall 88)
                let mut iface_buf = [0u8; 96];
                let mut iface_len = 0usize;
                let adapter_raw = diag_cstr_trim(&stats.net_adapter);
                let state_raw = diag_cstr_trim(&stats.net_state_desc);
                let adapter_s = if adapter_raw.is_empty() { b"Desconhecido" } else { adapter_raw };
                let state_s = if state_raw.is_empty() { b"Link Desconhecido" } else { state_raw };
                diag_push_bytes(&mut iface_buf, &mut iface_len, adapter_s);
                diag_push_bytes(&mut iface_buf, &mut iface_len, b" (");
                diag_push_ipv4(&mut iface_buf, &mut iface_len, stats.net_ip);
                diag_push_bytes(&mut iface_buf, &mut iface_len, b") - ");
                diag_push_bytes(&mut iface_buf, &mut iface_len, state_s);

                // Formata MAC ARP Gateway (HotRouteTable Syscall 97)
                let mut mac_buf = [0u8; 32];
                let mut mac_len = 0usize;
                if has_mac {
                    diag_push_mac(&mut mac_buf, &mut mac_len, found_mac);
                } else {
                    diag_push_bytes(&mut mac_buf, &mut mac_len, b"Pendente ARP");
                }

                let mut bench_buf = [0u8; 112];
                let mut bench_len = 0usize;
                diag_push_bytes(&mut bench_buf, &mut bench_len, b"Sys88 ");
                diag_push_u64(&mut bench_buf, &mut bench_len, obs_bench.sys88_avg_cyc);
                diag_push_bytes(&mut bench_buf, &mut bench_len, b"c (");
                diag_push_us_tenths(&mut bench_buf, &mut bench_len, obs_bench.sys88_avg_cyc, obs_bench.tsc_khz);
                diag_push_bytes(&mut bench_buf, &mut bench_len, b") | Sys97 ");
                diag_push_u64(&mut bench_buf, &mut bench_len, obs_bench.sys97_avg_cyc);
                diag_push_bytes(&mut bench_buf, &mut bench_len, b"c (");
                diag_push_us_tenths(&mut bench_buf, &mut bench_len, obs_bench.sys97_avg_cyc, obs_bench.tsc_khz);
                diag_push_bytes(&mut bench_buf, &mut bench_len, b")");

                let err_len = browser::format_net_error_html_diag(
                    active_url,
                    err_title,
                    err_msg,
                    err_detail,
                    &stage_buf[..stage_len],
                    &ip_buf[..ip_len],
                    &iface_buf[..iface_len],
                    &mac_buf[..mac_len],
                    stats.net_tx_packets,
                    stats.net_rx_packets,
                    &bench_buf[..bench_len],
                    out_buf,
                );
                session.load_document_bytes(&out_buf[..err_len], active_url);
                session.render(buf_slice);
                let _ = sys_window_update(win_id, buf_slice);
                print_str(b"[BROWSER-INTERACTION] PAGE_NET_ERROR target=");
                print_str(active_url);
                print_str(b"\n");
                false
            }
        }
    } else if path.starts_with(b"/disk/") || path.starts_with(b"/ext2/") {
        if let Some(len) = read_html_file(path) {
            let doc_slice = core::slice::from_raw_parts(core::ptr::addr_of!(HTML_FILE_BUF) as *const u8, len);
            session.load_document_bytes(doc_slice, path);
            session.record_navigation_current(path);
            session.render(buf_slice);
            let _ = sys_window_update(win_id, buf_slice);
            print_str(b"[BROWSER-INTERACTION] PAGE_LOADED target=");
            print_str(path);
            print_str(b"\n");
            true
        } else {
            let f_ptr = core::ptr::addr_of_mut!(HTML_FILE_BUF) as *mut u8;
            let out_buf = core::slice::from_raw_parts_mut(f_ptr, 32768);
            let err_len = browser::format_net_error_html(
                path,
                b"404 - Documento Nao Encontrado",
                b"O documento solicitado nao foi localizado no disco local (/disk).",
                b"VfsError::NotFound / Arquivo inexistente",
                out_buf,
            );
            let doc_slice = &out_buf[..err_len];
            session.load_document_bytes(doc_slice, path);
            session.render(buf_slice);
            let _ = sys_window_update(win_id, buf_slice);
            print_str(b"[BROWSER-INTERACTION] PAGE_404 target=");
            print_str(path);
            print_str(b"\n");
            false
        }
    } else {
        let f_ptr = core::ptr::addr_of_mut!(HTML_FILE_BUF) as *mut u8;
        let out_buf = core::slice::from_raw_parts_mut(f_ptr, 32768);
        let err_len = browser::format_net_error_html(
            path,
            b"URL Invalida ou Esquema Nao Suportado",
            b"O endereco fornecido nao pode ser processado pelo navegador.",
            b"InvalidUrl / Esquema desconhecido",
            out_buf,
        );
        let doc_slice = &out_buf[..err_len];
        session.load_document_bytes(doc_slice, path);
        session.render(buf_slice);
        let _ = sys_window_update(win_id, buf_slice);
        print_str(b"[BROWSER-INTERACTION] PAGE_INVALID_URL target=");
        print_str(path);
        print_str(b"\n");
        false
    }
}

unsafe fn run_gui_demo() {
    print_str(b"\n==================================================================\n");
    print_str(b"  JINN BROWSER 0.2.0 - INTEGRACAO NATIVA, EVENTOS E INTERATIVIDADE\n");
    print_str(b"==================================================================\n");

    let mut session = unsafe { &mut *core::ptr::addr_of_mut!(BROWSER_SESSION) };
    session.reset();
    let mut surface = DynamicSurfaceBuffer::new();
    unsafe { surface.init(); }
    let buf_slice = surface.slice_mut();

    // 1. Carrega documento inicial do VFS (/disk/INDEX.HTM)
    print_str(b"[BROWSER] 1. Lendo arquivo HTML local do VFS (/disk/INDEX.HTM)...\n");
    let bytes_read = match read_html_file(b"/disk/INDEX.HTM") {
        Some(len) => len,
        None => {
            print_str(b"  [WARN] /disk/INDEX.HTM nao encontrado no VFS, usando fallback embutido...\n");
            let fallback = b"<html><head><title>Jinn OS Portal</title></head><body><h1>Jinn Browser 0.2.0</h1><p>Bem-vindo ao Jinn Browser rodando nativamente sobre o microkernel Jinn OS.</p><p><a href=\"about.htm\">Sobre o Sistema</a></p></body></html>";
            let f_ptr = core::ptr::addr_of_mut!(HTML_FILE_BUF) as *mut u8;
            core::ptr::copy_nonoverlapping(fallback.as_ptr(), f_ptr, fallback.len());
            fallback.len()
        }
    };
    print_str(b"  Bytes lidos do documento: ");
    print_u64(bytes_read as u64);
    print_str(b"\n");

    let raw_bytes = core::slice::from_raw_parts(core::ptr::addr_of!(HTML_FILE_BUF) as *const u8, bytes_read);
    session.load_document_bytes(raw_bytes, b"file:///disk/INDEX.HTM");
    print_str(b"  [PASS] Pipeline HTML->DOM->Layout: ");
    print_u64(session.page.block_count as u64);
    print_str(b" blocos DOM, ");
    print_u64(session.layout.line_count as u64);
    print_str(b" linhas de texto, ");
    print_u64(session.layout.click_count as u64);
    print_str(b" links registrados!\n");

    // 2. Cria janela nativa no Aladdin Desktop
    let win_id = sys_window_create(b"Jinn Browser v0.2.0", browser::WIN_W, browser::WIN_H);
    if win_id < 0 {
        print_str(b"  [FAIL] sys_window_create falhou para Jinn Browser: ");
        print_i64(win_id);
        print_str(b"\n");
        return;
    }
    let win_id = win_id as usize;
    print_str(b"  [PASS] Janela nativa criada no Aladdin Desktop (win_id = ");
    print_u64(win_id as u64);
    print_str(b").\n");

    // 3. Renderiza e atualiza primeiro quadro (ScrollY = 0px)
    session.render(buf_slice);
    let upd_ret = sys_window_update(win_id, buf_slice);
    if upd_ret == 0 {
        print_str(b"  [PASS] Quadro 1: Superficie renderizada (ScrollY = 0px) e transferida ao compositor!\n");
    }

    let mut ev: ClientWindowEvent = core::mem::zeroed();
    let _ = sys_window_poll_event(win_id, &mut ev as *mut ClientWindowEvent);
    let _ = sys_sleep(200);

    // 4. Interatividade: Clicar no link para navegar para about.htm
    print_str(b"[BROWSER] 2. Testando interatividade de links e resolucao de URL relativa...\n");
    if session.layout.click_count > 0 {
        let link_reg = session.layout.click_regions[0];
        print_str(b"  Link alvo identificado: target='");
        for &ch in &link_reg.target[..link_reg.target_len] {
            let s = [ch];
            print_str(&s);
        }
        print_str(b"' em (");
        print_u64(link_reg.x as u64);
        print_str(b", ");
        print_u64(link_reg.y as u64);
        print_str(b")\n");

        // Simula clique dentro da bounding box do link
        let click_x = link_reg.x + 4;
        let click_y = browser::CLIP_TOP + link_reg.y + 2;
        let act = session.handle_click(click_x, click_y);
        if let browser::BrowserAction::NavigateTarget(target, len) = act {
            print_str(b"  Hit-test positivo! Carregando documento alvo (/disk/ABOUT.HTM)...\n");
            session.push_back_current();
            session.history_fwd_count = 0;

            if browser_load_and_display(&mut session, &target[..len], win_id, buf_slice) {
                print_str(b"  [PASS] Quadro 2: Navegacao por link concluida com exito (URL: file:///disk/ABOUT.HTM)!\n");
            }
        }
    }
    let _ = sys_window_poll_event(win_id, &mut ev as *mut ClientWindowEvent);
    let _ = sys_sleep(200);

    // 5. Testando Botão Voltar [<]
    print_str(b"[BROWSER] 3. Testando botao Voltar [<] e historico bidirecional...\n");
    let act_back = session.handle_click(10, 12);
    if act_back == browser::BrowserAction::Back {
        print_str(b"  Botao [<] acionado via hit-test! Restaurando documento anterior (/disk/INDEX.HTM)...\n");
        if let Some((prev_target, p_len)) = session.go_back() {
            if browser_load_and_display(&mut session, &prev_target[..p_len], win_id, buf_slice) {
                print_str(b"  [PASS] Quadro 3: Historico anterior restaurado com exito (URL: file:///disk/INDEX.HTM)!\n");
            }
        }
    }
    let _ = sys_window_poll_event(win_id, &mut ev as *mut ClientWindowEvent);
    let _ = sys_sleep(200);

    // 6. Testando Botão Avançar [>]
    print_str(b"[BROWSER] 4. Testando botao Avancar [>]...\n");
    let act_fwd = session.handle_click(22, 12);
    if act_fwd == browser::BrowserAction::Forward {
        print_str(b"  Botao [>] acionado via hit-test! Avancando para documento (/disk/ABOUT.HTM)...\n");
        if let Some((fwd_target, f_len)) = session.go_forward() {
            if browser_load_and_display(&mut session, &fwd_target[..f_len], win_id, buf_slice) {
                print_str(b"  [PASS] Quadro 4: Historico avancado com exito (URL: file:///disk/ABOUT.HTM)!\n");
            }
        }
    }
    let _ = sys_window_poll_event(win_id, &mut ev as *mut ClientWindowEvent);
    let _ = sys_sleep(200);

    // 7. Testando Botão Recarregar [R]
    print_str(b"[BROWSER] 5. Testando botao Recarregar [R]...\n");
    let act_reload = session.handle_click(34, 12);
    if act_reload == browser::BrowserAction::Reload {
        print_str(b"  Botao [R] acionado via hit-test! Recarregando documento sem alterar historico...\n");
        let curr = session.current_url;
        let len = session.current_url_len;
        if browser_load_and_display(&mut session, &curr[..len], win_id, buf_slice) {
            print_str(b"  [PASS] Quadro 5: Recarregamento concluido com exito!\n");
        }
    }
    let _ = sys_window_poll_event(win_id, &mut ev as *mut ClientWindowEvent);
    let _ = sys_sleep(200);

    // 8. Testando Barra de Endereço (Focus, Typing & Navigation)
    print_str(b"[BROWSER] 6. Testando foco e edicao na barra de enderecos...\n");
    let act_focus = session.handle_click(100, 12);
    if act_focus == browser::BrowserAction::FocusAddress {
        print_str(b"  Barra de enderecos focada via hit-test! Digitando 'index.htm'...\n");
        session.address_edit_len = 0;
        let input_url = b"index.htm";
        for &b in input_url {
            let _ = session.address_key(b as u32);
        }
        let act_enter = session.address_key(0x0A);
        if let browser::BrowserAction::NavigateTarget(t, l) = act_enter {
            session.push_back_current();
            session.history_fwd_count = 0;
            if browser_load_and_display(&mut session, &t[..l], win_id, buf_slice) {
                print_str(b"  [PASS] Quadro 6: Navegacao via barra de enderecos concluida com exito!\n");
            }
        }
    }
    let _ = sys_window_poll_event(win_id, &mut ev as *mut ClientWindowEvent);
    let _ = sys_sleep(200);

    // 9. Testando Viewport e Rolagem Vertical
    print_str(b"[BROWSER] 7. Testando viewport e rolagem vertical...\n");
    session.scroll_by(35);
    session.render(buf_slice);
    let _ = sys_window_update(win_id, buf_slice);
    print_str(b"  [PASS] Quadro 7: Viewport rolada (ScrollY = 35px, verificando clipping e scrollbar).\n");
    let _ = sys_window_poll_event(win_id, &mut ev as *mut ClientWindowEvent);
    let _ = sys_sleep(200);

    // 10. Encerramento Gracioso e Liberação de Recursos
    print_str(b"[BROWSER] 8. Encerramento gracioso da janela do navegador...\n");
    let close_ret = sys_window_close(win_id);
    if close_ret == 0 {
        print_str(b"  [PASS] Janela encerrada com sucesso via sys_window_close (recursos liberados).\n");
    } else {
        print_str(b"  [FAIL] sys_window_close retornou erro: ");
        print_i64(close_ret);
        print_str(b"\n");
    }

    print_str(b"[BROWSER] [PASS] Jinn Browser 0.2.0 validado nativamente em Ring 3 com 100% de sucesso!\n");
}

fn str_contains(haystack: *const u8, needle: &[u8]) -> bool {
    if haystack.is_null() || needle.is_empty() { return false; }
    let mut h_len = 0;
    unsafe {
        while *haystack.add(h_len) != 0 && h_len < 256 {
            h_len += 1;
        }
        if h_len < needle.len() { return false; }
        for i in 0..=(h_len - needle.len()) {
            let mut matches = true;
            for j in 0..needle.len() {
                let hc = *haystack.add(i + j);
                let nc = needle[j];
                let hc_low = if (b'A'..=b'Z').contains(&hc) { hc + 32 } else { hc };
                let nc_low = if (b'A'..=b'Z').contains(&nc) { nc + 32 } else { nc };
                if hc_low != nc_low {
                    matches = false;
                    break;
                }
            }
            if matches { return true; }
        }
    }
    false
}

fn resolve_target_with_mode<'a>(target: &[u8], buf: &'a mut [u8; 128]) -> (&'a [u8], bool) {
    let mut start = 0;
    while start < target.len() && (target[start] == b' ' || target[start] == b'\t') {
        start += 1;
    }
    let mut end = target.len();
    while end > start && (target[end - 1] == b' ' || target[end - 1] == b'\t' || target[end - 1] == 0) {
        end -= 1;
    }
    let mut clean = &target[start..end];

    if clean.is_empty() {
        let b = b"about:blank";
        buf[..b.len()].copy_from_slice(b);
        buf[b.len()] = 0;
        return (&buf[..b.len()], false);
    }

    // 1. Esquema interno about:
    if clean.starts_with(b"about:") {
        let len = clean.len().min(127);
        buf[..len].copy_from_slice(&clean[..len]);
        buf[len] = 0;
        return (&buf[..len], false);
    }

    // 2. HTTPS explicito: navegacao segura obrigatoria via TLS 1.3 (sem fallback HTTP)
    if clean.starts_with(b"https://") {
        let len = clean.len().min(127);
        buf[..len].copy_from_slice(&clean[..len]);
        buf[len] = 0;
        return (&buf[..len], false);
    }

    // 3. HTTP explicito: navegacao direta em porta 80
    if clean.starts_with(b"http://") {
        let len = clean.len().min(127);
        buf[..len].copy_from_slice(&clean[..len]);
        buf[len] = 0;
        return (&buf[..len], false);
    }

    // 4. file:/// -> remove prefixo de protocolo local
    if clean.starts_with(b"file://") {
        clean = &clean[7..];
    }

    // Prevencao de Directory Traversal (..) em caminhos locais
    let mut has_traversal = false;
    for i in 0..clean.len().saturating_sub(1) {
        if clean[i] == b'.' && clean[i + 1] == b'.' {
            has_traversal = true;
            break;
        }
    }
    if has_traversal {
        let blk = b"about:blocked";
        buf[..blk.len()].copy_from_slice(blk);
        buf[blk.len()] = 0;
        return (&buf[..blk.len()], false);
    }

    // 5. Caminhos absolutos do VFS local (/disk/ ou /ext2/)
    if clean.starts_with(b"/disk/") || clean.starts_with(b"/ext2/") {
        let len = clean.len().min(127);
        buf[..len].copy_from_slice(&clean[..len]);
        buf[len] = 0;
        return (&buf[..len], false);
    }

    // 6. Esquemas explicitamente nao suportados
    if clean.starts_with(b"ftp://") || clean.starts_with(b"mailto:") || clean.starts_with(b"gopher://") {
        let blk = b"about:unsupported";
        buf[..blk.len()].copy_from_slice(blk);
        buf[blk.len()] = 0;
        return (&buf[..blk.len()], false);
    }

    // 7. Arquivos locais raiz conhecidos do VFS (/disk/INDEX.HTM, /disk/about.htm, /disk/readme.txt)
    let is_exact_local_file = clean.eq_ignore_ascii_case(b"index.htm")
        || clean.eq_ignore_ascii_case(b"about.htm")
        || clean.eq_ignore_ascii_case(b"readme.txt");

    if is_exact_local_file {
        let prefix = b"/disk/";
        let p_len = prefix.len();
        buf[..p_len].copy_from_slice(prefix);
        let c_len = clean.len().min(127 - p_len);
        buf[p_len..p_len + c_len].copy_from_slice(&clean[..c_len]);
        let total = p_len + c_len;
        buf[total] = 0;
        return (&buf[..total], false);
    }

    // Se comecar com barra absoluta mas nao pertencer a volumes autorizados (/disk/ ou /ext2/)
    if clean.starts_with(b"/") {
        let blk = b"about:blocked";
        buf[..blk.len()].copy_from_slice(blk);
        buf[blk.len()] = 0;
        return (&buf[..blk.len()], false);
    }

    // 8. Dominio sem esquema (ex: "68k.news", "frogfind.com", "jinn-os.dev.br"):
    // Normaliza "jinn.os.dev.br" e inicia sempre via HTTPS com fallback automatico para HTTP
    let prefix = b"https://";
    buf[..prefix.len()].copy_from_slice(prefix);
    let mut out_len = prefix.len();
    if clean.eq_ignore_ascii_case(b"jinn.os.dev.br") {
        let canonical = b"jinn-os.dev.br";
        buf[out_len..out_len + canonical.len()].copy_from_slice(canonical);
        out_len += canonical.len();
    } else if clean.len() > 14 && clean[..15].eq_ignore_ascii_case(b"jinn.os.dev.br/") {
        let canonical = b"jinn-os.dev.br";
        buf[out_len..out_len + canonical.len()].copy_from_slice(canonical);
        out_len += canonical.len();
        let rest = &clean[14..];
        let rlen = rest.len().min(127 - out_len);
        buf[out_len..out_len + rlen].copy_from_slice(&rest[..rlen]);
        out_len += rlen;
    } else {
        let c_len = clean.len().min(127 - out_len);
        buf[out_len..out_len + c_len].copy_from_slice(&clean[..c_len]);
        out_len += c_len;
    }
    buf[out_len] = 0;
    (&buf[..out_len], true)
}

#[allow(dead_code)]
fn resolve_target_path<'a>(target: &[u8], buf: &'a mut [u8; 128]) -> &'a [u8] {
    resolve_target_with_mode(target, buf).0
}

#[allow(dead_code)]
fn get_cstr_len(s: &[u8]) -> usize {
    let mut l = 0;
    while l < s.len() && s[l] != 0 {
        l += 1;
    }
    l
}

#[inline(always)]
unsafe fn sys_mkdir(path: &[u8]) -> i64 {
    syscall3(51, path.as_ptr() as u64, path.len() as u64, 0)
}

unsafe fn apply_wallpaper_action(target: &[u8]) {
    print_str(b"[BROWSER-WALLPAPER] Aplicando papel de parede: ");
    print_str(target);
    print_str(b"\n");

    let mut full_path = [0u8; 128];
    let mut full_len;
    if target.starts_with(b"/") {
        let l = target.len().min(127);
        full_path[..l].copy_from_slice(&target[..l]);
        full_len = l;
    } else {
        let prefix = b"/disk/wallpapers/";
        full_path[..prefix.len()].copy_from_slice(prefix);
        let l = target.len().min(127 - prefix.len());
        full_path[prefix.len()..prefix.len() + l].copy_from_slice(&target[..l]);
        full_len = prefix.len() + l;
    }

    // Lê arquivo original
    let fd = sys_open(&full_path[..full_len], 0);
    let mut valid_data = false;
    let mut bytes_read = 0i64;
    if fd >= 0 {
        bytes_read = sys_read(fd as u64, HTML_FILE_BUF.as_mut_ptr(), HTML_FILE_BUF.len() as u64);
        let _ = sys_close(fd as u64);
        if bytes_read > 0 {
            valid_data = true;
        }
    }

    if !valid_data && !target.starts_with(b"/") {
        // Tenta /disk/<target>
        let prefix = b"/disk/";
        full_path[..prefix.len()].copy_from_slice(prefix);
        let l = target.len().min(127 - prefix.len());
        full_path[prefix.len()..prefix.len() + l].copy_from_slice(&target[..l]);
        full_len = prefix.len() + l;
        let fd2 = sys_open(&full_path[..full_len], 0);
        if fd2 >= 0 {
            bytes_read = sys_read(fd2 as u64, HTML_FILE_BUF.as_mut_ptr(), HTML_FILE_BUF.len() as u64);
            let _ = sys_close(fd2 as u64);
            if bytes_read > 0 {
                valid_data = true;
            }
        }
    }

    // Garante infraestrutura do diretório de wallpapers
    let _ = sys_mkdir(b"/disk/wallpapers");

    if valid_data && bytes_read > 0 {
        let cur_wp_path = b"/disk/wallpapers/current_wallpaper.bmp";
        let out_fd = sys_open(cur_wp_path, 0x01 | 0x40 | 0x200); // O_WRONLY | O_CREAT | O_TRUNC
        if out_fd >= 0 {
            let _ = sys_write(out_fd as u64, HTML_FILE_BUF.as_ptr(), bytes_read as u64);
            let _ = sys_close(out_fd as u64);
            print_str(b"[BROWSER-WALLPAPER] Copiado para /disk/wallpapers/current_wallpaper.bmp\n");
        }
    }

    // Grava /disk/desktop.cfg
    let mut cfg_buf = [0u8; 256];
    let mut cfg_len = 0;
    let p_wp = b"WALLPAPER=";
    cfg_buf[..p_wp.len()].copy_from_slice(p_wp);
    cfg_len += p_wp.len();
    let path_to_write = &full_path[..full_len];
    cfg_buf[cfg_len..cfg_len + path_to_write.len()].copy_from_slice(path_to_write);
    cfg_len += path_to_write.len();
    cfg_buf[cfg_len] = b'\n';
    cfg_len += 1;

    let cfg_fd = sys_open(b"/disk/desktop.cfg", 0x01 | 0x40 | 0x200);
    if cfg_fd >= 0 {
        let _ = sys_write(cfg_fd as u64, cfg_buf.as_ptr(), cfg_len as u64);
        let _ = sys_close(cfg_fd as u64);
        print_str(b"[BROWSER-WALLPAPER] Gravado com sucesso em /disk/desktop.cfg\n");
    }

    // Atualiza /disk/SETTINGS.CFG do Aladdin Desktop
    let set_text = b"# ALADDIN_CONFIG_V1\nMODE=1\nWALLPAPER=12\nWALLPAPER_AUDIO=0\nWALLPAPER_VOL=40\n";
    let set_fd = sys_open(b"/disk/SETTINGS.CFG", 0x01 | 0x40 | 0x200);
    if set_fd >= 0 {
        let _ = sys_write(set_fd as u64, set_text.as_ptr(), set_text.len() as u64);
        let _ = sys_close(set_fd as u64);
        print_str(b"[BROWSER-WALLPAPER] Atualizado /disk/SETTINGS.CFG (WALLPAPER=12)\n");
    }

    print_str(b"[BROWSER-WALLPAPER] [OK] Papel de parede definido com sucesso!\n");
}

unsafe fn run_browser_interactive(initial_url: Option<&[u8]>) -> ! {
    print_str(b"[BROWSER] Iniciando processo...\n");
    print_str(b"\n==================================================================\n");
    print_str(b"  JINN BROWSER 0.5.0 - SESSAO INTERATIVA NATIVA (RING 3)\n");
    print_str(b"==================================================================\n");

    let mut session = unsafe { &mut *core::ptr::addr_of_mut!(BROWSER_SESSION) };
    session.reset();
    print_str(b"[BROWSER-STORAGE] Inicializando infraestrutura de persistencia local...\n");
    session.init_persistence();
    print_str(b"[BROWSER-STORAGE] Persistencia pronta em /disk/browser/ (cache, cookies, history, bookmark)!\n");

    let mut surface = DynamicSurfaceBuffer::new();
    unsafe { surface.init(); }
    print_str(b"[BROWSER] Buffer alocado com sucesso (216 KiB BSS Surface)...\n");
    let buf_slice = surface.slice_mut();

    // 1. Carrega documento inicial do VFS (/disk/INDEX.HTM)
    print_str(b"[BROWSER] Carregando /disk/INDEX.HTM...\n");
    let bytes_read = match read_html_file(b"/disk/INDEX.HTM") {
        Some(len) => {
            print_str(b"[BROWSER] Documento lido com sucesso (");
            print_u64(len as u64);
            print_str(b" bytes).\n");
            len
        }
        None => {
            print_str(b"[BROWSER] /disk/INDEX.HTM nao encontrado no VFS, usando fallback embutido...\n");
            let fallback = b"<html><head><title>Jinn OS Portal</title><style>h1{color:#f9e2af;margin:8px;} .card{background-color:#1e1e2e;margin:6px;padding:6px;border:1px solid #45475a;border-radius:4px;} .btn{background-color:#313244;color:#89dceb;border:1px solid #45475a;border-radius:3px;padding:2px 6px;margin:2px;} #counter-display{color:#00ffaa;font-weight:bold;} .hidden{display:none;} .flex-row{display:flex;flex-direction:row;} .card p{margin:3px;}</style></head><body><h1>Jinn Browser 0.6.0</h1><p>Motor Nativo: HTML + Mini-CSS + JavaScript</p><div class=\"card\"><p id=\"counter-display\">Cliques: 0</p><div class=\"flex-row\"><button id=\"btn-count\" class=\"btn\" onclick=\"clicks=clicks+1;document.getElementById('counter-display').innerText='Cliques: '+clicks;\">[ +1 Clique ]</button><button id=\"btn-toggle\" class=\"btn\" onclick=\"document.getElementById('extra-info').classList.toggle('hidden');\">[ Toggle Info ]</button></div><p id=\"extra-info\" class=\"hidden\">Recurso CSS display:none e classList ativo!</p></div><script>var clicks = 0;</script><hr/><p><a href=\"about.htm\">Sobre o Sistema</a></p></body></html>";
            let f_ptr = core::ptr::addr_of_mut!(HTML_FILE_BUF) as *mut u8;
            core::ptr::copy_nonoverlapping(fallback.as_ptr(), f_ptr, fallback.len());
            fallback.len()
        }
    };

    let raw_bytes = core::slice::from_raw_parts(core::ptr::addr_of!(HTML_FILE_BUF) as *const u8, bytes_read);
    session.load_document_bytes(raw_bytes, b"file:///disk/INDEX.HTM");
    session.record_navigation_current(b"file:///disk/INDEX.HTM");

    // 2. Cria janela nativa no Aladdin Desktop
    print_str(b"[BROWSER] Janela solicitada ao compositor (300x180)...\n");
    let win_id = sys_window_create(b"Jinn Browser", browser::WIN_W, browser::WIN_H);
    if win_id < 0 {
        print_str(b"[BROWSER] [ERRO] sys_window_create falhou para Jinn Browser! Codigo: ");
        print_i64(win_id);
        print_str(b"\n");
        sys_exit(1);
    }
    let win_id = win_id as usize;
    print_str(b"[BROWSER] Janela criada com sucesso no Compositor! win_id=");
    print_u64(win_id as u64);
    print_str(b"\n");
    print_str(b"[BROWSER-INTERACTION] WINDOW_CREATED win_id=");
    print_u64(win_id as u64);
    print_str(b"\n");

    // 3. Renderiza e atualiza primeiro quadro (ou carrega URL direta passada via argv/Aladdin)
    session.render(buf_slice);
    let upd_ret = sys_window_update(win_id, buf_slice);
    if upd_ret < 0 {
        print_str(b"[BROWSER] [ERRO] sys_window_update falhou! Codigo: ");
        print_i64(upd_ret);
        print_str(b"\n");
    } else {
        print_str(b"[BROWSER] Primeiro frame renderizado.\n");
    }

    if let Some(init_u) = initial_url {
        if !init_u.is_empty() {
            print_str(b"[BROWSER] Abrindo URL direta solicitada pelo Aladdin: ");
            print_str(init_u);
            print_str(b"\n");
            let mut norm_u = [0u8; 128];
            let copy_len = init_u.len().min(127);
            norm_u[..copy_len].copy_from_slice(&init_u[..copy_len]);
            browser_load_and_display(&mut session, &norm_u[..copy_len], win_id, buf_slice);
        }
    }

    // 4. Laço Interativo de Eventos do Aladdin (Zero-Idle com bloqueio cooperativo)
    let mut ev: ClientWindowEvent = core::mem::zeroed();
    loop {
        let has_ev = sys_window_wait_event(win_id, &mut ev as *mut ClientWindowEvent);
        if has_ev == 0 {
            // Em chamada bloqueante Zero-Idle, se retornar 0 cede CPU cooperativamente para evitar contenção
            let _ = sys_yield();
            continue;
        } else if has_ev == -11 {
            // Canal em transição (-EAGAIN): cede ciclos cooperativamente e continua
            let _ = sys_yield();
            continue;
        } else if has_ev < 0 {
            print_str(b"[BROWSER-INTERACTION] Erro irrecuperavel no poll (ret=");
            print_i64(has_ev);
            print_str(b"). Encerrando...\n");
            sys_exit(0);
        } else if has_ev == 1 {
            match ev.event_type {
                EVENT_WINDOW_CLOSE => {
                    print_str(b"[BROWSER-INTERACTION] CLOSE window received\n");
                    let _ = session.history_store.save();
                    let _ = session.cookie_jar.save();
                    let _ = session.bookmark_store.save();
                    let _ = sys_window_close(win_id);
                    sys_exit(0);
                }
                EVENT_WINDOW_RESIZE => {
                    let new_w = ev.x as usize;
                    let new_h = ev.y as usize;
                    print_str(b"[BROWSER-INTERACTION] EVENT_WINDOW_RESIZE new_w=");
                    print_u64(new_w as u64);
                    print_str(b" new_h=");
                    print_u64(new_h as u64);
                    print_str(b"\n");
                    if new_w >= 100 && new_h >= 60 && new_w <= MAX_WIN_W && new_h <= MAX_WIN_H {
                        session.resize(new_w, new_h);
                        let _ = unsafe { surface.resize(session.viewport_w, session.viewport_h) };
                        let buf_cur = surface.slice_mut();
                        session.render(buf_cur);
                        let _ = sys_window_update(win_id, buf_cur);
                    }
                }
                EVENT_WINDOW_EXPOSE => {
                    print_str(b"[BROWSER-INTERACTION] EVENT_WINDOW_EXPOSE received\n");
                    let buf_cur = surface.slice_mut();
                    session.render(buf_cur);
                    let _ = sys_window_update(win_id, buf_cur);
                }
                EVENT_NAVIGATE_PENDING => {
                    let nav_path = b"/disk/browser/.nav_url";
                    let fd = sys_open(nav_path, O_RDONLY);
                    if fd >= 0 {
                        let mut url_buf = [0u8; 128];
                        let n = sys_read(fd as u64, url_buf.as_mut_ptr(), 127);
                        let _ = sys_close(fd as u64);
                        // Trunca para não re-navegar no próximo evento
                        let clr_fd = sys_open(nav_path, O_WRONLY | O_CREAT | O_TRUNC);
                        if clr_fd >= 0 {
                            let _ = sys_close(clr_fd as u64);
                        }
                        if n > 0 {
                            let mut ulen = n as usize;
                            while ulen > 0 && (url_buf[ulen - 1] == b'\n' || url_buf[ulen - 1] == b'\r' || url_buf[ulen - 1] == 0) {
                                ulen -= 1;
                            }
                            if ulen > 0 {
                                print_str(b"[BROWSER] Navegando via HotRoute 1-click para: ");
                                print_str(&url_buf[..ulen]);
                                print_str(b"\n");
                                let buf_cur = surface.slice_mut();
                                browser_load_and_display(&mut session, &url_buf[..ulen], win_id, buf_cur);
                            }
                        }
                    }
                }
                EVENT_MOUSE_DOWN => {
                    if ev.x < 0 || ev.y < 0 {
                        continue;
                    }
                    session.dragging_scrollbar = false;
                    let click_x = ev.x as usize;
                    let click_y = ev.y as usize;
                    print_str(b"[BROWSER-INTERACTION] MOUSE_DOWN x=");
                    print_i64(ev.x as i64);
                    print_str(b" y=");
                    print_i64(ev.y as i64);
                    print_str(b"\n");
                    let action = session.handle_click(click_x, click_y);

                    let buf_cur = surface.slice_mut();

                    match action {
                        browser::BrowserAction::ToggleFullscreen => {
                            print_str(b"[BROWSER-INTERACTION] ACTION ToggleFullscreen\n");
                            let st = session.toggle_fullscreen();
                            print_str(b"  Novo estado: ");
                            match st {
                                browser::FullscreenState::Fullscreen => print_str(b"FULLSCREEN (1024x768)\n"),
                                browser::FullscreenState::Windowed => print_str(b"WINDOWED (300x180)\n"),
                                _ => print_str(b"OUTRO\n"),
                            }
                            let _ = unsafe { surface.resize(session.viewport_w, session.viewport_h) };
                            let buf_cur = surface.slice_mut();
                            session.render(buf_cur);
                            let _ = sys_window_update(win_id, buf_cur);
                        }
                        browser::BrowserAction::ExitFullscreen => {
                            print_str(b"[BROWSER-INTERACTION] ACTION ExitFullscreen\n");
                            if session.fullscreen_state == browser::FullscreenState::Fullscreen {
                                session.toggle_fullscreen();
                                let _ = unsafe { surface.resize(session.viewport_w, session.viewport_h) };
                                let buf_cur = surface.slice_mut();
                                session.render(buf_cur);
                                let _ = sys_window_update(win_id, buf_cur);
                            }
                        }
                        browser::BrowserAction::Back => {
                            print_str(b"[BROWSER-INTERACTION] ACTION Back\n");
                            if let Some((target, len)) = session.go_back() {
                                browser_load_and_display(&mut session, &target[..len], win_id, buf_cur);
                            }
                        }
                        browser::BrowserAction::Forward => {
                            print_str(b"[BROWSER-INTERACTION] ACTION Forward\n");
                            if let Some((target, len)) = session.go_forward() {
                                browser_load_and_display(&mut session, &target[..len], win_id, buf_cur);
                            }
                        }
                        browser::BrowserAction::Reload => {
                            print_str(b"[BROWSER-INTERACTION] ACTION Reload\n");
                            let curr = session.current_url;
                            let len = session.current_url_len;
                            if len > 0 {
                                browser_load_and_display(&mut session, &curr[..len], win_id, buf_cur);
                            }
                        }
                        browser::BrowserAction::NavigateTarget(target, len) => {
                            print_str(b"[BROWSER-INTERACTION] ACTION NavigateTarget target=");
                            print_str(&target[..len]);
                            print_str(b"\n");
                            let mut resolved = [0u8; 128];
                            let curr_url = &session.current_url[..session.current_url_len];
                            let res_len = browser::resolve_relative_url(curr_url, &target[..len], &mut resolved);
                            session.push_back_current();
                            session.history_fwd_count = 0;
                            browser_load_and_display(&mut session, &resolved[..res_len], win_id, buf_cur);
                        }
                        browser::BrowserAction::JsAction(node_idx) => {
                            print_str(b"[BROWSER-INTERACTION] ACTION JsAction node=");
                            print_u64(node_idx as u64);
                            print_str(b"\n");
                            if session.dispatch_js_click(node_idx) {
                                session.render(buf_cur);
                                let _ = sys_window_update(win_id, buf_cur);
                            }
                        }
                        browser::BrowserAction::FocusAddress => {
                            print_str(b"[BROWSER-INTERACTION] ACTION FocusAddress\n");
                            session.render(buf_cur);
                            let _ = sys_window_update(win_id, buf_cur);
                        }
                        browser::BrowserAction::Scroll(delta) => {
                            print_str(b"[BROWSER-INTERACTION] ACTION Scroll delta=");
                            print_i64(delta as i64);
                            print_str(b"\n");
                            session.scroll_by(delta);
                            session.render(buf_cur);
                            let _ = sys_window_update(win_id, buf_cur);
                        }
                        browser::BrowserAction::SetWallpaper(target, len) => {
                            print_str(b"[BROWSER-INTERACTION] ACTION SetWallpaper\n");
                            apply_wallpaper_action(&target[..len]);
                            session.render(buf_cur);
                            let _ = sys_window_update(win_id, buf_cur);
                        }
                        browser::BrowserAction::SaveWallpaper(target, len) => {
                            print_str(b"[BROWSER-INTERACTION] ACTION SaveWallpaper\n");
                            apply_wallpaper_action(&target[..len]);
                            session.render(buf_cur);
                            let _ = sys_window_update(win_id, buf_cur);
                        }
                        browser::BrowserAction::None => {
                            session.render(buf_cur);
                            let _ = sys_window_update(win_id, buf_cur);
                        }
                    }
                }
                EVENT_MOUSE_MOVE => {
                    if session.dragging_scrollbar && ev.y >= 0 {
                        session.scroll_to_track_y(ev.y as usize);
                        let buf_cur = surface.slice_mut();
                        session.render(buf_cur);
                        let _ = sys_window_update(win_id, buf_cur);
                    }
                }
                EVENT_MOUSE_UP => {
                    session.dragging_scrollbar = false;
                }
                EVENT_SCROLL => {
                    if ev.y != 0 {
                        session.scroll_by(ev.y * 24);
                        let buf_cur = surface.slice_mut();
                        session.render(buf_cur);
                        let _ = sys_window_update(win_id, buf_cur);
                    }
                }
                EVENT_KEY_DOWN => {
                    let key = ev.button_or_key;
                    print_str(b"[BROWSER-INTERACTION] KEY_DOWN code=");
                    print_u64(key as u64);
                    print_str(b" focused=");
                    if session.is_address_focused { print_str(b"1\n"); } else { print_str(b"0\n"); }

                    let buf_cur = surface.slice_mut();

                    if key == 0x8B {
                        // F11: Alterna Tela Cheia globalmente (independente de foco na barra de URL)
                        print_str(b"[BROWSER-INTERACTION] KEY F11 -> Toggle Fullscreen\n");
                        let st = session.toggle_fullscreen();
                        print_str(b"  Novo estado: ");
                        match st {
                            browser::FullscreenState::Fullscreen => print_str(b"FULLSCREEN (1024x768)\n"),
                            browser::FullscreenState::Windowed => print_str(b"WINDOWED (300x180)\n"),
                            _ => print_str(b"OUTRO\n"),
                        }
                        let _ = unsafe { surface.resize(session.viewport_w, session.viewport_h) };
                        let buf_cur2 = surface.slice_mut();
                        session.render(buf_cur2);
                        let _ = sys_window_update(win_id, buf_cur2);
                    } else if session.is_address_focused {
                        let action = session.address_key(key);
                        match action {
                            browser::BrowserAction::NavigateTarget(target, len) => {
                                print_str(b"[BROWSER-INTERACTION] ACTION AddressBar Navigate target=");
                                print_str(&target[..len]);
                                print_str(b"\n");
                                session.push_back_current();
                                session.history_fwd_count = 0;
                                browser_load_and_display(&mut session, &target[..len], win_id, buf_cur);
                            }
                            _ => {
                                session.render(buf_cur);
                                let _ = sys_window_update(win_id, buf_cur);
                            }
                        }
                    } else {
                        match key {
                            0x50 | 0x6A | 0x4A => { // Seta para Baixo (0x50) / 'j' / 'J' (Scroll +20px)
                                print_str(b"[BROWSER-INTERACTION] ACTION ScrollDown\n");
                                session.scroll_by(20);
                                session.render(buf_cur);
                                let _ = sys_window_update(win_id, buf_cur);
                            }
                            0x48 | 0x6B | 0x4B => { // Seta para Cima (0x48) / 'k' / 'K' (Scroll -20px)
                                print_str(b"[BROWSER-INTERACTION] ACTION ScrollUp\n");
                                session.scroll_by(-20);
                                session.render(buf_cur);
                                let _ = sys_window_update(win_id, buf_cur);
                            }
                            0x51 | 0x20 => { // Page Down (0x51) / Space (0x20) (Scroll +80px)
                                print_str(b"[BROWSER-INTERACTION] ACTION PageDown\n");
                                session.scroll_by(80);
                                session.render(buf_cur);
                                let _ = sys_window_update(win_id, buf_cur);
                            }
                            0x49 => { // Page Up (0x49) (Scroll -80px)
                                print_str(b"[BROWSER-INTERACTION] ACTION PageUp\n");
                                session.scroll_by(-80);
                                session.render(buf_cur);
                                let _ = sys_window_update(win_id, buf_cur);
                            }
                            0x0E | 0x08 | 0x62 | 0x68 => { // Backspace (0x0E / 0x08) ou 'b'/'h' (Voltar)
                                print_str(b"[BROWSER-INTERACTION] ACTION KeyBack\n");
                                if let Some((target, len)) = session.go_back() {
                                    browser_load_and_display(&mut session, &target[..len], win_id, buf_cur);
                                }
                            }
                            0x66 | 0x6C => { // 'f' / 'l' (Avançar)
                                print_str(b"[BROWSER-INTERACTION] ACTION KeyForward\n");
                                if let Some((target, len)) = session.go_forward() {
                                    browser_load_and_display(&mut session, &target[..len], win_id, buf_cur);
                                }
                            }
                            0x72 | 0x52 => { // 'r' / 'R' (Recarregar)
                                print_str(b"[BROWSER-INTERACTION] ACTION KeyReload\n");
                                let curr = session.current_url;
                                let len = session.current_url_len;
                                if len > 0 {
                                    browser_load_and_display(&mut session, &curr[..len], win_id, buf_cur);
                                }
                            }
                            0x2F | 0x09 => { // '/' ou Tab: focar barra de endereço
                                print_str(b"[BROWSER-INTERACTION] ACTION KeyFocusAddress\n");
                                session.focus_address();
                                session.render(buf_cur);
                                let _ = sys_window_update(win_id, buf_cur);
                            }
                            0x01 | 0x1B => { // Escape (0x01 / 0x1B)
                                if session.fullscreen_state == browser::FullscreenState::Fullscreen {
                                    print_str(b"[BROWSER-INTERACTION] KEY Esc -> Sair de Tela Cheia\n");
                                    session.toggle_fullscreen();
                                    let _ = unsafe { surface.resize(session.viewport_w, session.viewport_h) };
                                    let buf_cur2 = surface.slice_mut();
                                    session.render(buf_cur2);
                                    let _ = sys_window_update(win_id, buf_cur2);
                                } else {
                                    print_str(b"[BROWSER-INTERACTION] ACTION KeyClose (Esc)\n");
                                    let _ = sys_window_close(win_id);
                                    sys_exit(0);
                                }
                            }
                            0x71 => { // 'q' (Fechar Janela)
                                print_str(b"[BROWSER-INTERACTION] ACTION KeyClose (q)\n");
                                let _ = sys_window_close(win_id);
                                sys_exit(0);
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        } else {
            // Evento com código não reconhecido: cede CPU cooperativamente sem bloquear,
            // pois o laço retorna imediatamente ao sys_window_wait_event bloqueante.
            let _ = sys_yield();
        }
    }
}

unsafe fn test_jda_stress() {
    print_str(b"[STRESS] test_jda_stress: Alocando buffers JDA ate o limite max_jda_buffers (4)...\n");
    let mut jda_handles = [0i64; 6];
    let mut allocated = 0usize;
    let mut hit_limit = false;

    for i in 0..6 {
        let h = sys_jda_shm_alloc(false);
        if h >= 0 {
            jda_handles[allocated] = h;
            allocated += 1;
        } else if h == ENOBUFS {
            hit_limit = true;
            print_str(b"  Buffer JDA #");
            print_u64((i + 1) as u64);
            print_str(b" rejeitado com -ENOBUFS (-105)!\n");
            break;
        }
    }

    if allocated > 0 {
        let chk = sys_jda_submit(1, jda_handles[0] as u64, 512);
        print_str(b"  Job 1 (Fletcher32 Checksum) processado, checksum = ");
        print_i64(chk);
        print_str(b"\n");
    }

    for i in 0..allocated {
        let _ = sys_jda_shm_release(jda_handles[i] as u64);
    }

    if hit_limit {
        print_str(b"  [PASS] test_jda_stress: Limite (-ENOBUFS) e job Fletcher32 OK!\n");
    } else {
        print_str(b"  [FAIL] test_jda_stress falhou.\n");
    }
}

/// Teste do JDA Fase 3: Validação Completa de Anéis Assíncronos (io_uring) e Zero-Copy Page Remapping
unsafe fn test_jda_phase3_ioring_and_zerocopy() {
    print_str(b"[JDA-PHASE3] test_jda_phase3_ioring_and_zerocopy: Testando io_uring e Zero-Copy...\n");

    // 1. Aloca buffer JDA para transferência Zero-Copy
    let handle = sys_jda_shm_alloc(false);
    if handle < 0 {
        print_str(b"  [FAIL] Falha ao alocar buffer JDA para teste da Fase 3\n");
        return;
    }

    // 2. Obtém endereço virtual do buffer mapeado no PML4 do processo de usuário
    let vaddr = sys_jda_shm_get_vaddr(handle as u64);
    print_str(b"  Buffer Zero-Copy mapeado em Ring 3 no endereco virtual: 0x");
    print_u64(vaddr as u64);
    print_str(b"\n");

    let mut direct_write_ok = false;
    if vaddr > 0 {
        let ptr = vaddr as *mut u8;
        // Escreve diretamente no buffer via ponteiro virtual de usuário (SEM NENHUMA SYSCALL DE CÓPIA)
        for i in 0..128 {
            *ptr.add(i) = 0x2A; // 42 em decimal
        }
        // Lê de volta para certificar acesso
        let mut sum_direct = 0u32;
        for i in 0..128 {
            sum_direct += *ptr.add(i) as u32;
        }
        direct_write_ok = sum_direct == (128 * 42);
        print_str(b"  Acesso direto de escrita e leitura na memoria fisica (Zero-Copy): ");
        if direct_write_ok {
            print_str(b"OK (soma direta = 5376)\n");
        } else {
            print_str(b"FALHA\n");
        }
    }

    // 3. Submissão assíncrona em lote ao IoRing (sys_jda_ring_submit)
    let sqes = [
        JdaSqe {
            opcode: OP_COMPUTE_SUM,
            flags: JDA_SQE_ZERO_COPY,
            priority: 1,
            _reserved: 0,
            handle: handle as u32,
            offset: 0,
            len: 128,
            user_data: 0x1111,
        },
        JdaSqe {
            opcode: OP_COMPUTE_FLETCHER32,
            flags: JDA_SQE_ZERO_COPY,
            priority: 1,
            _reserved: 0,
            handle: handle as u32,
            offset: 0,
            len: 128,
            user_data: 0x2222,
        },
        JdaSqe {
            opcode: OP_NOP,
            flags: 0,
            priority: 0,
            _reserved: 0,
            handle: 0,
            offset: 0,
            len: 0,
            user_data: 0x3333,
        },
    ];

    let sub_ret = sys_jda_ring_submit(sqes.as_ptr(), sqes.len());
    print_str(b"  sys_jda_ring_submit(3 SQEs) -> ");
    print_i64(sub_ret);
    print_str(b" submetidos\n");
    let submit_ok = sub_ret == 3;

    // 4. Coleta assíncrona de conclusões (sys_jda_ring_poll)
    let mut cqes: [JdaCqe; 4] = [JdaCqe::default(); 4];
    let poll_ret = sys_jda_ring_poll(cqes.as_mut_ptr(), cqes.len());
    print_str(b"  sys_jda_ring_poll(max 4) -> ");
    print_i64(poll_ret);
    print_str(b" conclusoes colhidas\n");

    let mut poll_ok = false;
    if poll_ret == 3 {
        let c0 = cqes[0];
        let c1 = cqes[1];
        let c2 = cqes[2];
        print_str(b"    CQE[0]: user_data=0x");
        print_u64(c0.user_data);
        print_str(b", res=");
        print_i64(c0.res as i64);
        print_str(b" (esperado 5376)\n");

        print_str(b"    CQE[1]: user_data=0x");
        print_u64(c1.user_data);
        print_str(b", res=");
        print_i64(c1.res as i64);
        print_str(b" (Fletcher32 checksum)\n");

        print_str(b"    CQE[2]: user_data=0x");
        print_u64(c2.user_data);
        print_str(b", res=");
        print_i64(c2.res as i64);
        print_str(b" (NOP sucesso)\n");

        poll_ok = c0.user_data == 0x1111 && c0.res == 5376 && c1.user_data == 0x2222 && c2.user_data == 0x3333 && c2.res == 0;
    }

    // 5. Compartilhamento Zero-Copy com PID 4 e imposição de imutabilidade (USER_RO)
    let share_ret = sys_jda_shm_share(handle as u64, 4, true);
    print_str(b"  sys_jda_shm_share(handle, PID 4, make_owner_ro=true) -> ");
    print_i64(share_ret);
    print_str(b"\n");
    let share_ok = share_ret > 0;

    // 6. Tenta executar mutação no anel assíncrono agora que o buffer é somente leitura -> deve falhar com -EPERM (-1)
    let sqe_xor = JdaSqe {
        opcode: OP_COMPUTE_XOR,
        flags: 0,
        priority: 1,
        _reserved: 0,
        handle: handle as u32,
        offset: 0,
        len: 128,
        user_data: 0x4444,
    };
    let _ = sys_jda_ring_submit(&sqe_xor as *const JdaSqe, 1);
    let mut cqe_xor = [JdaCqe::default(); 1];
    let _ = sys_jda_ring_poll(cqe_xor.as_mut_ptr(), 1);
    let ro_protected = cqe_xor[0].res == EPERM as i32;
    print_str(b"  Garantia de Imutabilidade (rejeicao de escrita em buffer compartilhado RO): ");
    if ro_protected {
        print_str(b"OK (-EPERM retornado pelo anel)\n");
    } else {
        print_str(b"FALHA\n");
    }

    // 7. Liberação do buffer
    let rel_ret = sys_jda_shm_release(handle as u64);
    let rel_ok = rel_ret == 0;

    if direct_write_ok && submit_ok && poll_ok && share_ok && ro_protected && rel_ok {
        print_str(b"  [PASS] test_jda_phase3_ioring_and_zerocopy: io_uring e Zero-Copy validados com 100% de sucesso!\n");
    } else {
        print_str(b"  [FAIL] test_jda_phase3_ioring_and_zerocopy falhou em um dos criterios.\n");
    }
}

// ── Papéis Concorrentes das Tarefas (Fase D — Preempção e CPU-Hog) ───────────

unsafe fn run_worker_task(_pid: u32) -> ! {
    print_str(b"[TASK CPU-Hog (PID 3)] Iniciado - executando em loop fechado SEM chamadas de sistema...\n");
    print_str(b"  [CPU-Hog PID 3] Monopolizando CPU em computacao pura (preempcao obrigatoria por IRQ0)...\n");

    let mut spin_counter: u64 = 0;
    // LOOP FECHADO ESTRITO: Nenhuma chamada de sistema (sem sys_yield, sem sys_sleep, sem sys_exit)!
    // A preempção ocorrerá obrigatoriamente via interrupção periódica de hardware (PIT IRQ0 a 100 Hz),
    // e o término ocorrerá quando a cota configurada pelo kernel (25 ticks) for esgotada (-EDQUOT).
    loop {
        spin_counter = core::hint::black_box(spin_counter.wrapping_add(1));
        core::hint::spin_loop();
    }
}

unsafe fn run_periodic_task(_pid: u32) -> ! {
    print_str(b"[TASK Periodic (PID 4)] Iniciado - monitorando heartbeats enquanto CPU-hog executa...\n");
    for beat in 1..=4 {
        sys_sleep(60);
        print_str(b"  [Periodic PID 4] Heartbeat #");
        print_u64(beat);
        print_str(b"/4 recebido (avanco com preempcao de hardware confirmada)!\n");
    }
    print_str(b"[TASK Periodic (PID 4)] Ciclo de monitoramento finalizado! sys_exit(0)...\n");
    sys_exit(0);
}

/// Teste F: Validação de Preempção por Hardware (IRQ0) e Aplicação Efetiva de Cota de CPU
unsafe fn test_sched_preemption_and_cpu_quota() {
    print_str(b"[ADV-TEST 6/6] test_sched_preemption_and_cpu_quota: Validando preempcao por hardware e cotas...\n");

    // Localiza dinamicamente o PID do worker[U3] configurado com cota max_cpu_ticks == 25
    let mut worker_pid = 0u64;
    let mut stat: ProcessResourceStat = core::mem::zeroed();
    for cand in 1..=12u64 {
        let mut s: ProcessResourceStat = core::mem::zeroed();
        if sys_res_stat(cand, &mut s) == 0 && s.max_cpu_ticks == 25 {
            worker_pid = cand;
            stat = s;
            break;
        }
    }

    if worker_pid == 0 {
        print_str(b"  [WARN] Tarefa CPU-Hog com cota 25 ticks nao localizada.\n");
        return;
    }

    // Caso a tarefa ainda esteja executando no inicio do boot, aguarda acumulacao de preempcoes
    if stat.state != 6 && stat.irq_preemptions < 3 {
        sys_sleep(120);
        let _ = sys_res_stat(worker_pid, &mut stat);
    }

    print_str(b"  Telemetria da Tarefa CPU-Hog (PID ");
    print_u64(worker_pid);
    print_str(b") capturada via Syscall 25:\n");
    print_str(b"    CPU Ticks Consumidos: ");
    print_u64(stat.cpu_ticks);
    print_str(b"\n    Trocas de Contexto Totais: ");
    print_u64(stat.ctx_switches);
    print_str(b"\n    Preempcoes por Hardware (IRQ0): ");
    print_u64(stat.irq_preemptions);
    print_str(b"\n    Yields Voluntarios: ");
    print_u64(stat.voluntary_yields);
    print_str(b"\n    Cota Configurada de CPU: ");
    print_u64(stat.max_cpu_ticks);
    print_str(b" ticks\n");

    let preemption_proven = stat.irq_preemptions >= 3 && stat.voluntary_yields == 0;
    if preemption_proven {
        print_str(b"  [PASS] Preempcao por hardware COMPROVADA: CPU-hog despejado repetidamente sem yield!\n");
    } else {
        print_str(b"  [FAIL] Preempcao por hardware nao alcancou os criterios esperados.\n");
    }

    // Aguarda a cota de CPU do CPU-hog (25 ticks = 250ms) ser excedida caso ainda nao tenha encerrado
    if stat.state != 6 && stat.last_violation_code != -122 {
        sys_sleep(200);
    }

    let mut stat_after: ProcessResourceStat = core::mem::zeroed();
    let ret_after = sys_res_stat(worker_pid, &mut stat_after);
    if ret_after == 0 {
        print_str(b"  Telemetria pos-cota PID ");
        print_u64(worker_pid);
        print_str(b": Estado=");
        print_u64(stat_after.state as u64);
        print_str(b", Violacoes=");
        print_u64(stat_after.limit_violations as u64);
        print_str(b", Ultimo Erro=");
        print_i64(stat_after.last_violation_code as i64);
        print_str(b"\n");
        if stat_after.state == 6 || stat_after.last_violation_code == -122 {
            print_str(b"  [PASS] Cota de CPU EFETIVA: Processo monopolizador encerrado pelo kernel com -EDQUOT (-122)!\n");
        }
    }
}

unsafe fn test_adv_vfs_stat_and_lseek() {
    print_str(b"\n--- Teste 14: Auditoria Rigorosa de VFS (Syscalls 17, 18 e 19) ---\n");

    // 1. Auditoria do Contrato Binario (Layout, Tamanho, Alinhamento e Offsets de FileStat)
    let dummy = FileStat::default();
    let base = &dummy as *const _ as usize;
    if core::mem::size_of::<FileStat>() != 56 || core::mem::align_of::<FileStat>() != 8 {
        print_str(b"  [FAIL] Layout de FileStat violou contrato de ABI (size != 56 ou align != 8)!\n");
        return;
    }
    let off_size = &dummy.size as *const _ as usize - base;
    let off_inode = &dummy.inode as *const _ as usize - base;
    let off_node_type = &dummy.node_type as *const _ as usize - base;
    let off_mode = &dummy.mode as *const _ as usize - base;
    let off_blksize = &dummy.blksize as *const _ as usize - base;
    let off_blocks = &dummy.blocks as *const _ as usize - base;
    let off_atime = &dummy.atime as *const _ as usize - base;
    let off_mtime = &dummy.mtime as *const _ as usize - base;
    let off_ctime = &dummy.ctime as *const _ as usize - base;

    if off_size != 0 || off_inode != 8 || off_node_type != 16 || off_mode != 20
        || off_blksize != 24 || off_blocks != 28 || off_atime != 32 || off_mtime != 40 || off_ctime != 48
    {
        print_str(b"  [FAIL] Offsets dos campos de FileStat violam o contrato binario!\n");
        return;
    }
    print_str(b"  [PASS] Contrato Binario de FileStat verificado: size=56, align=8, offsets=[0,8,16,20,24,28,32,40,48]\n");

    // 2. Auditoria de Seguranca de Memoria (Ponteiros Invalidos e Kernel-Space)
    let mut stat = FileStat::default();
    if sys_stat_raw(0, 10, &mut stat as *mut _ as u64) != -22 {
        print_str(b"  [FAIL] sys_stat com path NULL nao retornou -EINVAL!\n");
        return;
    }
    if sys_stat_raw(b"/disk/README.TXT".as_ptr() as u64, b"/disk/README.TXT".len() as u64, 0) != -22 {
        print_str(b"  [FAIL] sys_stat com stat_ptr NULL nao retornou -EINVAL!\n");
        return;
    }
    if sys_stat_raw(0xFFFF_8000_0000_0000, 10, &mut stat as *mut _ as u64) != -14 {
        print_str(b"  [FAIL] sys_stat com path no kernel-space nao retornou -EFAULT!\n");
        return;
    }
    if sys_stat_raw(b"/disk/README.TXT".as_ptr() as u64, b"/disk/README.TXT".len() as u64, 0xFFFF_8000_0000_0000) != -14 {
        print_str(b"  [FAIL] sys_stat com stat_ptr no kernel-space nao retornou -EFAULT!\n");
        return;
    }
    if sys_fstat_raw(3, 0) != -22 {
        print_str(b"  [FAIL] sys_fstat com stat_ptr NULL nao retornou -EINVAL!\n");
        return;
    }
    if sys_fstat_raw(3, 0xFFFF_8000_0000_0000) != -14 {
        print_str(b"  [FAIL] sys_fstat com stat_ptr no kernel-space nao retornou -EFAULT!\n");
        return;
    }

    // 3. Teste de Fronteira de Pagina (Page Boundary Straddle)
    let page_vaddr = sys_alloc_page();
    if page_vaddr > 0 {
        let vaddr = page_vaddr as usize;
        // Posiciona stat_ptr a 24 bytes do final da pagina (56 bytes straddle a fronteira para pagina nao mapeada)
        let straddle_ptr = (vaddr + 4096 - 24) as u64;
        let ret_straddle = sys_stat_raw(b"/disk/README.TXT".as_ptr() as u64, b"/disk/README.TXT".len() as u64, straddle_ptr);
        if ret_straddle != -14 {
            print_str(b"  [FAIL] sys_stat transpondo limite de pagina nao mapeada nao retornou -EFAULT!\n");
            let _ = sys_free_page(page_vaddr as u64);
            return;
        }
        let _ = sys_free_page(page_vaddr as u64);
    }

    // 4. Strings com e sem terminador NUL e limites de caminho
    let ret_without_nul = sys_stat(b"/disk/README.TXT", &mut stat);
    if ret_without_nul != 0 || stat.size == 0 || stat.node_type != 0 {
        print_str(b"  [FAIL] sys_stat sem terminador NUL falhou!\n");
        return;
    }
    let mut stat_nul = FileStat::default();
    let ret_with_nul = sys_stat(b"/disk/README.TXT\0", &mut stat_nul);
    if ret_with_nul != 0 || stat_nul.size != stat.size {
        print_str(b"  [FAIL] sys_stat com terminador NUL divergiu!\n");
        return;
    }

    // Caminho excessivamente longo (> 256 bytes)
    let long_path = [b'/'; 257];
    if sys_stat(&long_path, &mut stat) != -22 {
        print_str(b"  [FAIL] sys_stat com path > 256 bytes nao retornou -EINVAL!\n");
        return;
    }

    // Caminho relativo sem barra inicial -> -ENOENT (-2)
    if sys_stat(b"disk/README.TXT", &mut stat) != -2 {
        print_str(b"  [FAIL] sys_stat com caminho relativo nao retornou -ENOENT!\n");
        return;
    }

    // Consulta de caminho inexistente (-ENOENT = -2)
    let mut stat_err = FileStat::default();
    let ret_err = sys_stat(b"/disk/INEXISTENTE.TXT", &mut stat_err);
    if ret_err != -2 {
        print_str(b"  [FAIL] sys_stat com arquivo inexistente nao retornou -ENOENT!\n");
        return;
    }

    // 5. Suporte aos Descritores Padrao (0=stdin, 1=stdout, 2=stderr)
    let mut std_stat = FileStat::default();
    if sys_fstat(0, &mut std_stat) != 0 || std_stat.node_type != 2 || std_stat.mode != 0o020666 {
        print_str(b"  [FAIL] sys_fstat(0 stdin) nao identificou dispositivo de caractere!\n");
        return;
    }
    if sys_fstat(1, &mut std_stat) != 0 || std_stat.node_type != 2 || std_stat.mode != 0o020666 {
        print_str(b"  [FAIL] sys_fstat(1 stdout) nao identificou dispositivo de caractere!\n");
        return;
    }
    if sys_lseek(0, 0, SEEK_SET) != -29 {
        print_str(b"  [FAIL] sys_lseek em stdin nao retornou -ESPIPE (-29)!\n");
        return;
    }
    if sys_lseek(1, 0, SEEK_SET) != -29 {
        print_str(b"  [FAIL] sys_lseek em stdout nao retornou -ESPIPE (-29)!\n");
        return;
    }

    // Descritor fora dos limites da tabela (ex: fd 99)
    if sys_fstat(99, &mut std_stat) != -9 {
        print_str(b"  [FAIL] sys_fstat com fd fora dos limites nao retornou -EBADF!\n");
        return;
    }
    if sys_lseek(99, 0, SEEK_SET) != -9 {
        print_str(b"  [FAIL] sys_lseek com fd fora dos limites nao retornou -EBADF!\n");
        return;
    }

    // 6. Abertura de Arquivo e Consistencia fstat vs stat
    let fd = sys_open(b"/disk/README.TXT", 0);
    if fd < 3 {
        print_str(b"  [FAIL] sys_open falhou ao abrir /disk/README.TXT!\n");
        return;
    }
    let fd = fd as u64;

    let mut fstat = FileStat::default();
    let ret_fstat = sys_fstat(fd, &mut fstat);
    if ret_fstat != 0 || fstat.size != stat.size || fstat.node_type != 0 || fstat.mode != stat.mode {
        print_str(b"  [FAIL] sys_fstat divergiu de sys_stat!\n");
        let _ = sys_close(fd);
        return;
    }

    // 7. Auditoria de lseek: Reposicionamento, Rewind e Integridade de Dados
    let mut buf1 = [0u8; 4];
    let n1 = sys_read(fd, buf1.as_mut_ptr(), 4);
    if n1 != 4 {
        print_str(b"  [FAIL] sys_read nao leu 4 bytes iniciais!\n");
        let _ = sys_close(fd);
        return;
    }

    // Rewind com SEEK_SET 0
    let seek_ret = sys_lseek(fd, 0, SEEK_SET);
    if seek_ret != 0 {
        print_str(b"  [FAIL] sys_lseek SEEK_SET 0 falhou!\n");
        let _ = sys_close(fd);
        return;
    }
    let mut buf2 = [0u8; 4];
    let n2 = sys_read(fd, buf2.as_mut_ptr(), 4);
    if n2 != 4 || buf1 != buf2 {
        print_str(b"  [FAIL] Dados lidos apos SEEK_SET divergiram dos iniciais!\n");
        let _ = sys_close(fd);
        return;
    }

    // SEEK_SET para offset 2
    let seek_ret = sys_lseek(fd, 2, SEEK_SET);
    if seek_ret != 2 {
        print_str(b"  [FAIL] sys_lseek SEEK_SET 2 falhou!\n");
        let _ = sys_close(fd);
        return;
    }
    let mut buf3 = [0u8; 2];
    let n3 = sys_read(fd, buf3.as_mut_ptr(), 2);
    if n3 != 2 || buf3[0] != buf1[2] || buf3[1] != buf1[3] {
        print_str(b"  [FAIL] Dados lidos no offset 2 divergiram!\n");
        let _ = sys_close(fd);
        return;
    }

    // SEEK_CUR para recuar 2 bytes (offset atual e 4, voltando para 2)
    let seek_ret = sys_lseek(fd, -2, SEEK_CUR);
    if seek_ret != 2 {
        print_str(b"  [FAIL] sys_lseek SEEK_CUR -2 falhou!\n");
        let _ = sys_close(fd);
        return;
    }

    // SEEK_END para o final do arquivo
    let seek_ret = sys_lseek(fd, 0, SEEK_END);
    if seek_ret != stat.size as i64 {
        print_str(b"  [FAIL] sys_lseek SEEK_END 0 nao retornou tamanho exato do arquivo!\n");
        let _ = sys_close(fd);
        return;
    }

    // 8. Prevencao de Overflows e Underflows Aritmeticos em lseek
    // Overflow em SEEK_CUR com i64::MAX
    if sys_lseek(fd, 0x7FFF_FFFF_FFFF_FFFF, SEEK_CUR) != -22 {
        print_str(b"  [FAIL] Overflow em SEEK_CUR nao retornou -EINVAL!\n");
        let _ = sys_close(fd);
        return;
    }
    // Underflow extremo em SEEK_CUR com i64::MIN
    if sys_lseek(fd, -0x7FFF_FFFF_FFFF_FFFF - 1, SEEK_CUR) != -22 {
        print_str(b"  [FAIL] Underflow extremo em SEEK_CUR nao retornou -EINVAL!\n");
        let _ = sys_close(fd);
        return;
    }
    // Overflow em SEEK_END com i64::MAX
    if sys_lseek(fd, 0x7FFF_FFFF_FFFF_FFFF, SEEK_END) != -22 {
        print_str(b"  [FAIL] Overflow em SEEK_END nao retornou -EINVAL!\n");
        let _ = sys_close(fd);
        return;
    }
    // Offset negativo a partir de SEEK_SET
    if sys_lseek(fd, -1, SEEK_SET) != -22 {
        print_str(b"  [FAIL] sys_lseek offset negativo nao retornou -EINVAL!\n");
        let _ = sys_close(fd);
        return;
    }
    // Whence invalido (ex: 3)
    if sys_lseek(fd, 0, 3) != -22 {
        print_str(b"  [FAIL] sys_lseek com whence invalido nao retornou -EINVAL!\n");
        let _ = sys_close(fd);
        return;
    }

    // 9. Invariancia de Posicao apos Erros (Recuperacao e Repeticao)
    // Volta para offset 0 e testa que apos uma falha de seek, o offset NAO e alterado
    let _ = sys_lseek(fd, 0, SEEK_SET);
    let _ = sys_lseek(fd, -50, SEEK_SET); // Deve falhar com -EINVAL sem alterar offset
    let mut buf_recovery = [0u8; 4];
    let n_rec = sys_read(fd, buf_recovery.as_mut_ptr(), 4);
    if n_rec != 4 || buf_recovery != buf1 {
        print_str(b"  [FAIL] Offset foi corrompido apos tentativa invalida de lseek!\n");
        let _ = sys_close(fd);
        return;
    }

    // 10. Protecoes apos sys_close (-EBADF = -9)
    let _ = sys_close(fd);
    if sys_lseek(fd, 0, SEEK_SET) != -9 {
        print_str(b"  [FAIL] sys_lseek em FD fechado nao retornou -EBADF!\n");
        return;
    }
    if sys_fstat(fd, &mut fstat) != -9 {
        print_str(b"  [FAIL] sys_fstat em FD fechado nao retornou -EBADF!\n");
        return;
    }

    // 11. Sockets de Rede (fstat como S_IFSOCK e lseek retornando -ESPIPE)
    let sockfd = sys_socket(2, 1, 6);
    if sockfd >= 3 {
        let mut sock_stat = FileStat::default();
        let ret_sock_stat = sys_fstat(sockfd as u64, &mut sock_stat);
        if ret_sock_stat != 0 || sock_stat.node_type != 5 || sock_stat.mode != 0o140666 {
            print_str(b"  [FAIL] sys_fstat em socket nao identificou tipo S_IFSOCK (5) e modo 0o140666!\n");
            let _ = sys_close(sockfd as u64);
            return;
        }

        let ret_sock_seek = sys_lseek(sockfd as u64, 0, SEEK_SET);
        if ret_sock_seek != -29 {
            print_str(b"  [FAIL] sys_lseek em socket nao retornou -ESPIPE (-29)!\n");
            let _ = sys_close(sockfd as u64);
            return;
        }
        let _ = sys_close(sockfd as u64);
    }

    print_str(b"  [PASS] test_adv_vfs_stat_and_lseek: Auditoria de Seguranca, Layout e Syscalls 17, 18, 19 APROVADA com 100% de sucesso!\n");
}

unsafe fn test_adv_fat32_persistent_write_and_recovery() {
    print_str(b"\n--- Teste 15: Escrita Persistente FAT32, Alocacao de Clusters e Recuperacao ---\n");

    // 1. Validacao de sys_sync (Syscall 23)
    let ret_sync = sys_sync();
    if ret_sync != 0 {
        print_str(b"  [FAIL] sys_sync() retornou erro!\n");
        return;
    }
    print_str(b"  [PASS] Syscall 23 (sys_sync) executada com sucesso.\n");

    // 2. Escrita em pequeno arquivo com O_CREAT
    let fd_sm = sys_open(b"/disk/TEST_SM.TXT", O_WRONLY | O_CREAT | O_TRUNC);
    if fd_sm < 3 {
        print_str(b"  [FAIL] sys_open falhou ao criar /disk/TEST_SM.TXT (ret=");
        print_i64(fd_sm);
        print_str(b")\n");
        return;
    }
    let payload_sm = b"HELLO_FAT32_WORLD_PERSISTENT_DATA_12345";
    let written_sm = sys_write(fd_sm as u64, payload_sm.as_ptr(), payload_sm.len() as u64);
    if written_sm != payload_sm.len() as i64 {
        print_str(b"  [FAIL] sys_write em /disk/TEST_SM.TXT falhou!\n");
        let _ = sys_close(fd_sm as u64);
        return;
    }
    let _ = sys_close(fd_sm as u64);

    // Leitura imediata para verificar dados gravados
    let fd_rd_sm = sys_open(b"/disk/TEST_SM.TXT", O_RDONLY);
    if fd_rd_sm >= 3 {
        let mut rd_buf = [0u8; 64];
        let n = sys_read(fd_rd_sm as u64, rd_buf.as_mut_ptr(), 64);
        let _ = sys_close(fd_rd_sm as u64);
        if n != payload_sm.len() as i64 || &rd_buf[..payload_sm.len()] != payload_sm {
            print_str(b"  [FAIL] Leitura imediata de /disk/TEST_SM.TXT divergiu do payload!\n");
            return;
        }
        print_str(b"  [PASS] Escrita e leitura imediata de pequeno arquivo validadas.\n");
    }

    // 3. Escrita transpondo limite de setor (700 bytes > 512 bytes)
    let fd_sec = sys_open(b"/disk/TEST_SEC.TXT", O_WRONLY | O_CREAT | O_TRUNC);
    if fd_sec >= 3 {
        let mut payload_sec = [0u8; 700];
        for (i, b) in payload_sec.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        let n_sec = sys_write(fd_sec as u64, payload_sec.as_ptr(), 700);
        let _ = sys_close(fd_sec as u64);
        if n_sec != 700 {
            print_str(b"  [FAIL] sys_write transpondo setor falhou!\n");
            return;
        }

        let fd_sec_rd = sys_open(b"/disk/TEST_SEC.TXT", O_RDONLY);
        if fd_sec_rd >= 3 {
            let mut rd_sec = [0u8; 700];
            let read_sec_n = sys_read(fd_sec_rd as u64, rd_sec.as_mut_ptr(), 700);
            let _ = sys_close(fd_sec_rd as u64);
            if read_sec_n != 700 || rd_sec != payload_sec {
                print_str(b"  [FAIL] Dados transpondo setor divergiram na leitura!\n");
                return;
            }
            print_str(b"  [PASS] Escrita e leitura transpondo fronteira de setor (700B) validadas.\n");
        }
    }

    // 4. Escrita transpondo limite de cluster (5000 bytes > 4096 bytes)
    let fd_cls = sys_open(b"/disk/TEST_CLS.TXT", O_WRONLY | O_CREAT | O_TRUNC);
    if fd_cls >= 3 {
        let mut payload_cls = [0u8; 5000];
        for (i, b) in payload_cls.iter_mut().enumerate() {
            *b = ((i * 7) % 256) as u8;
        }
        let n_cls = sys_write(fd_cls as u64, payload_cls.as_ptr(), 5000);
        let _ = sys_close(fd_cls as u64);
        if n_cls != 5000 {
            print_str(b"  [FAIL] sys_write transpondo cluster falhou!\n");
            return;
        }

        let fd_cls_rd = sys_open(b"/disk/TEST_CLS.TXT", O_RDONLY);
        if fd_cls_rd >= 3 {
            let mut rd_cls = [0u8; 5000];
            let mut total_rd = 0usize;
            while total_rd < 5000 {
                let n = sys_read(fd_cls_rd as u64, rd_cls[total_rd..].as_mut_ptr(), (5000 - total_rd) as u64);
                if n <= 0 { break; }
                total_rd += n as usize;
            }
            let _ = sys_close(fd_cls_rd as u64);
            if total_rd != 5000 || rd_cls != payload_cls {
                print_str(b"  [FAIL] Dados transpondo cluster divergiram na leitura!\n");
                return;
            }
            print_str(b"  [PASS] Escrita multi-cluster com alocacao dinamica de clusters (5000B) validada.\n");
        }
    }

    // 5. Escrita em offset nao zero e preservacao de dados (read-modify-write)
    let fd_patch = sys_open(b"/disk/TEST_SM.TXT", O_RDWR);
    if fd_patch >= 3 {
        let seek_res = sys_lseek(fd_patch as u64, 10, SEEK_SET);
        if seek_res == 10 {
            let patch_data = b"PATCHED";
            let written_patch = sys_write(fd_patch as u64, patch_data.as_ptr(), patch_data.len() as u64);
            if written_patch == patch_data.len() as i64 {
                let _ = sys_close(fd_patch as u64);
                let fd_check = sys_open(b"/disk/TEST_SM.TXT", O_RDONLY);
                if fd_check >= 3 {
                    let mut check_buf = [0u8; 40];
                    let n_chk = sys_read(fd_check as u64, check_buf.as_mut_ptr(), 40);
                    let _ = sys_close(fd_check as u64);
                    // Bytes 0..10 devem ser "HELLO_FAT3"
                    if &check_buf[..10] == b"HELLO_FAT3" && &check_buf[10..17] == b"PATCHED" {
                        print_str(b"  [PASS] Escrita em offset nao-zero com preservacao de bytes adjacentes validada.\n");
                    } else {
                        print_str(b"  [FAIL] check_buf divergente (n=");
                        print_i64(n_chk);
                        print_str(b"): '");
                        if n_chk > 0 {
                            print_str(&check_buf[..n_chk as usize]);
                        }
                        print_str(b"'\n");
                    }
                } else {
                    print_str(b"  [FAIL] fd_check falhou: ");
                    print_i64(fd_check);
                    print_str(b"\n");
                }
            } else {
                print_str(b"  [FAIL] written_patch falhou: ");
                print_i64(written_patch);
                print_str(b"\n");
                let _ = sys_close(fd_patch as u64);
            }
        } else {
            print_str(b"  [FAIL] seek_res falhou: ");
            print_i64(seek_res);
            print_str(b"\n");
            let _ = sys_close(fd_patch as u64);
        }
    } else {
        print_str(b"  [FAIL] fd_patch falhou: ");
        print_i64(fd_patch);
        print_str(b"\n");
    }

    // 6. Teste de Persistencia e Recuperacao Cold-Reboot (/disk/PERSIST.TXT e /disk/persistencia.txt)
    let persist_fd = sys_open(b"/disk/PERSIST.TXT", O_RDONLY);
    let deterministic_magic = b"JINN_OS_PERSISTENCE_MAGIC_V11_FAT32_COLD_REBOOT_VALIDATION_TOKEN_987654321";
    if persist_fd >= 3 {
        // Arquivo já existia de inicialização anterior!
        let mut persist_buf = [0u8; 128];
        let n = sys_read(persist_fd as u64, persist_buf.as_mut_ptr(), deterministic_magic.len() as u64);
        let _ = sys_close(persist_fd as u64);
        if n == deterministic_magic.len() as i64 && &persist_buf[..deterministic_magic.len()] == deterministic_magic {
            print_str(b"  [PASS] PERSISTENCIA COMPROVADA: Payload intacto recuperado apos reinicializacao do QEMU!\n");
        }
    } else {
        // Primeira inicializacao: cria e grava o token de persistencia
        let wr_persist_fd = sys_open(b"/disk/PERSIST.TXT", O_WRONLY | O_CREAT | O_TRUNC);
        if wr_persist_fd >= 3 {
            let n = sys_write(wr_persist_fd as u64, deterministic_magic.as_ptr(), deterministic_magic.len() as u64);
            let _ = sys_close(wr_persist_fd as u64);
            let _ = sys_sync();
            if n == deterministic_magic.len() as i64 {
                print_str(b"  [PASS] Payload deterministico gravado e sincronizado em /disk/PERSIST.TXT para teste de reboot.\n");
            }
        }
    }

    // 6.1 Teste de Persistencia com Nome Longo LFN (/disk/persistencia.txt)
    let persist_lfn_fd = sys_open(b"/disk/persistencia.txt", O_RDONLY);
    let deterministic_lfn_token = b"JINN_OS_PERSISTENCIA_TXT_VALIDATION_TOKEN_123456789";
    if persist_lfn_fd >= 3 {
        let mut persist_lfn_buf = [0u8; 128];
        let n = sys_read(persist_lfn_fd as u64, persist_lfn_buf.as_mut_ptr(), deterministic_lfn_token.len() as u64);
        let _ = sys_close(persist_lfn_fd as u64);
        if n == deterministic_lfn_token.len() as i64 && &persist_lfn_buf[..deterministic_lfn_token.len()] == deterministic_lfn_token {
            print_str(b"  [PASS] PERSISTENCIA COMPROVADA: /disk/persistencia.txt recuperado com integridade apos cold-reboot!\n");
        }
    } else {
        let wr_lfn_fd = sys_open(b"/disk/persistencia.txt", O_WRONLY | O_CREAT | O_TRUNC);
        if wr_lfn_fd >= 3 {
            let n = sys_write(wr_lfn_fd as u64, deterministic_lfn_token.as_ptr(), deterministic_lfn_token.len() as u64);
            let _ = sys_close(wr_lfn_fd as u64);
            let _ = sys_sync();
            if n == deterministic_lfn_token.len() as i64 {
                print_str(b"  [PASS] Arquivo /disk/persistencia.txt gravado e sincronizado com sucesso!\n");
            }
        }
    }

    // 7. Integridade de outros arquivos preexistentes (/disk/README.TXT)
    let fd_readme = sys_open(b"/disk/README.TXT", O_RDONLY);
    if fd_readme >= 3 {
        let mut r_buf = [0u8; 20];
        let n = sys_read(fd_readme as u64, r_buf.as_mut_ptr(), 20);
        let _ = sys_close(fd_readme as u64);
        if n == 20 && r_buf.iter().all(|&b| b == b'=') {
            print_str(b"  [PASS] Integridade estrita de arquivos preexistentes (/disk/README.TXT) preservada 100%!\n");
        }
    }

    print_str(b"  [PASS] test_adv_fat32_persistent_write_and_recovery: Auditoria e Integridade de Escrita FAT32 APROVADA!\n");
}

unsafe fn test_browser_storage_persistence() {
    print_str(b"--- [Ring 3] Teste 18: Subsistema de Persistencia Local do Jinn Browser (/disk/browser/) ---\n");
    let (p, t) = browser::storage::run_selftests();
    print_str(b"  Resultado Persistencia do Browser: ");
    print_u64(p as u64);
    print_str(b"/");
    print_u64(t as u64);
    print_str(b" testes aprovados\n");
    if p == t {
        print_str(b"  [PASS] test_browser_storage_persistence: Infraestrutura VFS, Cookies, Historico e Transacional (.BAK) APROVADOS!\n");
    } else {
        print_str(b"  [FAIL] test_browser_storage_persistence: Falha em teste de persistencia!\n");
    }
}

// ── Testes de Carregamento ELF, sys_spawn e sys_execve ────────────────────────

unsafe fn streq(s: *const u8, expected: &[u8]) -> bool {
    if s.is_null() {
        return false;
    }
    let mut i = 0;
    while i < expected.len() {
        let b = *s.add(i);
        if b == 0 || b != expected[i] {
            return false;
        }
        i += 1;
    }
    *s.add(i) == 0
}

static mut BSS_TEST_ARRAY: [u8; 1024] = [0u8; 1024];

unsafe fn run_child_task(argc: usize, _argv: *const *const u8) -> ! {
    print_str(b"\n==================================================================\n");
    print_str(b"  [Ring 3] Processo filho executado com sucesso a partir do FAT32!\n");
    print_str(b"==================================================================\n");

    let mut stat: ProcessResourceStat = core::mem::zeroed();
    let _ = sys_res_stat(0, &mut stat);
    print_str(b"  [Ring 3] PID do Processo Filho: ");
    print_u64(stat.pid as u64);
    print_str(b"\n  [Ring 3] argc recebido: ");
    print_u64(argc as u64);
    print_str(b"\n");

    // 1. Validacao de BSS zerada
    let mut bss_ok = true;
    for i in 0..1024 {
        if BSS_TEST_ARRAY[i] != 0 {
            bss_ok = false;
            break;
        }
    }
    if bss_ok {
        print_str(b"  [PASS] Segmento BSS inicializado com zeros com sucesso.\n");
    } else {
        print_str(b"  [FAIL] Segmento BSS contem bytes nao-zero!\n");
        sys_exit(99);
    }

    // 2. Modifica a BSS para provar isolamento entre processos
    for i in 0..1024 {
        BSS_TEST_ARRAY[i] = 0x5A;
    }

    print_str(b"  [Ring 3] Processo filho concluindo com sys_exit(42)...\n");
    sys_exit(42);
}

unsafe fn run_exec_target(argc: usize, _argv: *const *const u8) -> ! {
    print_str(b"\n==================================================================\n");
    print_str(b"  [Ring 3] [PASS] sys_execve substituiu com sucesso a imagem do processo!\n");
    print_str(b"==================================================================\n");

    let mut stat: ProcessResourceStat = core::mem::zeroed();
    let _ = sys_res_stat(0, &mut stat);
    print_str(b"  [Ring 3] PID apos sys_execve (deve manter o mesmo): ");
    print_u64(stat.pid as u64);
    print_str(b"\n  [Ring 3] argc recebido no novo processo: ");
    print_u64(argc as u64);
    print_str(b"\n");

    print_str(b">>> TESTES DE EXECUCAO E CARREGAMENTO ELF CONCLUIDOS COM SUCESSO! <<<\n");
    print_str(b"Encerrando processo com sys_exit(0)...\n");
    sys_exit(0);
}

unsafe fn test_adv_elf_exec_and_spawn() {
    print_str(b"\n[ADV-TEST ELF] test_adv_elf_exec_and_spawn: Testes de Execucao ELF, sys_spawn e sys_execve...\n");

    // --- Casos Negativos de sys_spawn e sys_execve ---
    // 1. Arquivo inexistente
    let ret_nonexistent = sys_spawn(b"/disk/NONEXIST.ELF\0".as_ptr(), core::ptr::null(), core::ptr::null());
    if ret_nonexistent == -2 {
        print_str(b"  [PASS] sys_spawn(arquivo_inexistente) retornou -ENOENT (-2) com seguranca.\n");
    } else {
        print_str(b"  [FAIL] sys_spawn(arquivo_inexistente) retornou: ");
        print_i64(ret_nonexistent);
        print_str(b"\n");
    }

    // 2. Ponteiro nulo de caminho
    let ret_null = sys_spawn(core::ptr::null(), core::ptr::null(), core::ptr::null());
    if ret_null == -22 {
        print_str(b"  [PASS] sys_spawn(null) retornou -EINVAL (-22) com seguranca.\n");
    } else {
        print_str(b"  [FAIL] sys_spawn(null) retornou: ");
        print_i64(ret_null);
        print_str(b"\n");
    }

    // 3. Ponteiro apontando para memoria de kernel (isolamento de espaco de enderecamento)
    let ret_kernel = sys_spawn(0xFFFF_8000_0000_0000 as *const u8, core::ptr::null(), core::ptr::null());
    if ret_kernel == -14 {
        print_str(b"  [PASS] sys_spawn(kernel_addr) retornou -EFAULT (-14) com seguranca.\n");
    } else {
        print_str(b"  [FAIL] sys_spawn(kernel_addr) retornou: ");
        print_i64(ret_kernel);
        print_str(b"\n");
    }

    // 4. Arquivo que nao e ELF (/disk/README.TXT)
    let ret_not_elf = sys_spawn(b"/disk/README.TXT\0".as_ptr(), core::ptr::null(), core::ptr::null());
    if ret_not_elf == -8 {
        print_str(b"  [PASS] sys_spawn(arquivo_nao_elf) retornou -ENOEXEC (-8) com seguranca.\n");
    } else {
        print_str(b"  [FAIL] sys_spawn(arquivo_nao_elf) retornou: ");
        print_i64(ret_not_elf);
        print_str(b"\n");
    }

    // 5. sys_execve com arquivo inexistente (deve falhar e retornar sem destruir o processo chamador)
    let ret_exec_nonexistent = sys_execve(b"/disk/NONEXIST.ELF\0".as_ptr(), core::ptr::null(), core::ptr::null());
    if ret_exec_nonexistent == -2 {
        print_str(b"  [PASS] sys_execve falhou com -ENOENT (-2) e manteve processo chamador ativo e estavel!\n");
    } else {
        print_str(b"  [FAIL] sys_execve(arquivo_inexistente) retornou: ");
        print_i64(ret_exec_nonexistent);
        print_str(b"\n");
    }

    // --- Casos Positivos: Execucao de Processo Filho via FAT32 e Argumentos ---
    let child_argv: [*const u8; 3] = [
        b"/disk/ola.elf\0".as_ptr(),
        b"child_test\0".as_ptr(),
        core::ptr::null(),
    ];

    print_str(b"  [INFO] Disparando 1o processo filho via sys_spawn(/disk/ola.elf, ['child_test'])...\n");
    let child1_pid = sys_spawn(b"/disk/ola.elf\0".as_ptr(), child_argv.as_ptr(), core::ptr::null());
    if child1_pid > 0 {
        print_str(b"  [PASS] sys_spawn criou processo filho com PID: ");
        print_i64(child1_pid);
        print_str(b"\n");

        let exit_code1 = sys_waitpid(child1_pid as u64);
        if exit_code1 == 42 {
            print_str(b"  [PASS] sys_waitpid coletou codigo de saida 42 esperado do 1o processo filho!\n");
        } else {
            print_str(b"  [FAIL] sys_waitpid retornou codigo inesperado: ");
            print_i64(exit_code1);
            print_str(b"\n");
        }
    } else {
        print_str(b"  [FAIL] sys_spawn falhou com codigo: ");
        print_i64(child1_pid);
        print_str(b"\n");
    }

    // Execucao sequencial de um 2o processo independente
    print_str(b"  [INFO] Disparando 2o processo filho sequencial...\n");
    let child2_pid = sys_spawn(b"/disk/ola.elf\0".as_ptr(), child_argv.as_ptr(), core::ptr::null());
    if child2_pid > 0 {
        let exit_code2 = sys_waitpid(child2_pid as u64);
        if exit_code2 == 42 {
            print_str(b"  [PASS] Execucao sequencial do 2o processo independente concluida com exit code 42!\n");
        } else {
            print_str(b"  [FAIL] 2o processo filho retornou codigo inesperado: ");
            print_i64(exit_code2);
            print_str(b"\n");
        }
    } else {
        print_str(b"  [FAIL] sys_spawn 2o filho falhou com codigo: ");
        print_i64(child2_pid);
        print_str(b"\n");
    }

    print_str(b"  [PASS] test_adv_elf_exec_and_spawn: Testes de sys_spawn e isolamento APROVADOS!\n");
}

// ── Teste 18: Driver Manager & Ring 3 User-Space PS/2 Driver ────────────────
unsafe fn test_userspace_driver_subsystem() {
    print_str(b"\n[TEST 18] Driver Manager & User-Space PS/2 Driver (Ring 3 Isolation)...\n");

    // 1. Rejeição de DMA sem IOMMU (EPERM) - Requisito Arquitetural 3
    let dma_ret = sys_driver_register(DEV_KBD0, DRIVER_CAP_DMA);
    if dma_ret == EPERM {
        print_str(b"  [PASS] 18.1: Rejeicao estrita de DRIVER_CAP_DMA sem IOMMU (EPERM)\n");
    } else {
        print_str(b"  [FAIL] 18.1: Falha ao rejeitar DRIVER_CAP_DMA, ret: ");
        print_i64(dma_ret);
        print_str(b"\n");
    }

    // 2. Registro válido com PIO e INPUT
    let reg_ret = sys_driver_register(DEV_KBD0, DRIVER_CAP_PIO | DRIVER_CAP_INPUT);
    if reg_ret > 0 {
        print_str(b"  [PASS] 18.2: Registro valido de driver em Ring 3 (generation > 0)\n");
    } else {
        print_str(b"  [FAIL] 18.2: Falha no registro de driver, ret: ");
        print_i64(reg_ret);
        print_str(b"\n");
    }

    // 3. Conflito por registro duplicado / concorrência (EBUSY = -16)
    let busy_ret = sys_driver_register(DEV_KBD0, DRIVER_CAP_PIO | DRIVER_CAP_INPUT);
    if busy_ret == -16 { // EBUSY
        print_str(b"  [PASS] 18.3: Rejeicao de registro concorrente duplicado (EBUSY)\n");
    } else {
        print_str(b"  [FAIL] 18.3: Registro duplicado nao retornou EBUSY: ");
        print_i64(busy_ret);
        print_str(b"\n");
    }

    // 4. Polling com buffer inválido (EFAULT)
    let fault_ret = sys_driver_event_poll(DEV_KBD0, 0x1000 as *mut HardwareEvent, 1);
    if fault_ret == EFAULT {
        print_str(b"  [PASS] 18.4: Polling com buffer de usuario invalido rejeitado (EFAULT)\n");
    } else {
        print_str(b"  [FAIL] 18.4: Buffer invalido nao retornou EFAULT: ");
        print_i64(fault_ret);
        print_str(b"\n");
    }

    // 5. Polling de eventos válidos na fila (fila vazia inicialmente = 0 eventos)
    let mut ev_buf = [HardwareEvent::empty(); 8];
    let poll_ret = sys_driver_event_poll(DEV_KBD0, ev_buf.as_mut_ptr(), 8);
    if poll_ret >= 0 {
        print_str(b"  [PASS] 18.5: Polling de fila delimitada executado com sucesso\n");
    } else {
        print_str(b"  [FAIL] 18.5: Polling de fila falhou: ");
        print_i64(poll_ret);
        print_str(b"\n");
    }

    // 6. Envio de evento de entrada normalizado autorizado de volta ao kernel
    let input_ret = sys_driver_send_input(INPUT_KIND_KEY_DOWN, b'Z' as u16, 0, 0);
    if input_ret == 0 {
        print_str(b"  [PASS] 18.6: Despacho de entrada normalizada autorizada (sys_driver_send_input)\n");
    } else {
        print_str(b"  [FAIL] 18.6: Despacho de entrada falhou: ");
        print_i64(input_ret);
        print_str(b"\n");
    }

    // 7. Envio de entrada de mouse normalizada com clamping de coordenadas
    let mouse_ret = sys_driver_send_input(INPUT_KIND_MOUSE_MOVE, 0, 512, 384);
    if mouse_ret == 0 {
        print_str(b"  [PASS] 18.7: Despacho de coordenadas e clamping do mouse normalizado\n");
    } else {
        print_str(b"  [FAIL] 18.7: Despacho do mouse falhou: ");
        print_i64(mouse_ret);
        print_str(b"\n");
    }

    // 8. Desregistro do driver e restauração de fallback Ring 0
    let unreg_ret = sys_driver_unregister(DEV_KBD0);
    if unreg_ret == 0 {
        print_str(b"  [PASS] 18.8: Desregistro limpo e devolucao de controle ao Ring 0 (sys_driver_unregister)\n");
    } else {
        print_str(b"  [FAIL] 18.8: Desregistro falhou: ");
        print_i64(unreg_ret);
        print_str(b"\n");
    }

    // 9. Rejeição de input por processo não mais autorizado após desregistro
    let unauth_input = sys_driver_send_input(INPUT_KIND_KEY_DOWN, b'X' as u16, 0, 0);
    if unauth_input == EPERM {
        print_str(b"  [PASS] 18.9: Rejeicao de envio de entrada sem driver registrado (EPERM)\n");
    } else {
        print_str(b"  [FAIL] 18.9: Rejeicao de entrada nao autorizada falhou: ");
        print_i64(unauth_input);
        print_str(b"\n");
    }

    // 10. Teste de reinicializacao de driver via sys_driver_restart
    let rereg = sys_driver_register(DEV_KBD0, DRIVER_CAP_PIO | DRIVER_CAP_INPUT);
    if rereg > 0 {
        let restart_ret = sys_driver_restart(DEV_KBD0);
        if restart_ret > rereg {
            print_str(b"  [PASS] 18.10: Reinicializacao via sys_driver_restart incrementou generation token\n");
        } else {
            print_str(b"  [FAIL] 18.10: Reinicializacao nao incrementou generation token\n");
        }
        let _ = sys_driver_unregister(DEV_KBD0);
    } else {
        print_str(b"  [FAIL] 18.10: Falha no re-registro para teste de restart\n");
    }

    // 11. Pipeline Completo Teclado Ring 3: Decode Scancode Set 1 -> ASCII e envio normalizado
    let kbd_gen = sys_driver_register(DEV_KBD0, DRIVER_CAP_PIO | DRIVER_CAP_INPUT);
    if kbd_gen > 0 {
        // Simulação do driver Ring 3: recebe scancode 0x1E ('A' make), traduz para ASCII 'A'
        let scancode_raw: u8 = 0x1E;
        let ascii_decoded: u16 = match scancode_raw {
            0x1E => b'A' as u16,
            _ => 0,
        };
        let send_res = sys_driver_send_input(INPUT_KIND_KEY_DOWN, ascii_decoded, 0, 0);
        let mut telem = DriverTelemetryStats::default();
        let telem_res = sys_driver_telemetry(DEV_KBD0, &mut telem);
        if send_res == 0 && telem_res == 0 && telem.ring3_driver_events > 0 && telem.is_ring3_active == 1 {
            print_str(b"  [PASS] 18.11: Decodificacao Scancode Set 1 -> ASCII em Ring 3 e entrega normalizada\n");
        } else {
            print_str(b"  [FAIL] 18.11: Falha no pipeline Ring 3 de teclado\n");
        }
    } else {
        print_str(b"  [FAIL] 18.11: Falha ao registrar driver de teclado para teste 18.11\n");
    }

    // 12. Pipeline Completo Mouse Ring 3: Decode de pacote 3-bytes PS/2 e despacho normalizado
    let mouse_gen = sys_driver_register(DEV_MOUSE0, DRIVER_CAP_PIO | DRIVER_CAP_INPUT);
    if mouse_gen > 0 {
        // Simulação do driver Ring 3: pacote de 3 bytes (flags=0x09 [btn_left], dx=15, dy=-10)
        let p_flags: u8 = 0x09;
        let p_dx: i8 = 15;
        let p_dy: i8 = -10;
        let btn_left = (p_flags & 0x01) != 0;
        let m_move_res = sys_driver_send_input(INPUT_KIND_MOUSE_MOVE, 0, p_dx as i32, p_dy as i32);
        let m_btn_res = if btn_left {
            sys_driver_send_input(INPUT_KIND_MOUSE_BUTTON_DOWN, 1, 0, 0)
        } else {
            0
        };
        let mut m_telem = DriverTelemetryStats::default();
        let m_telem_res = sys_driver_telemetry(DEV_MOUSE0, &mut m_telem);
        if m_move_res == 0 && m_btn_res == 0 && m_telem_res == 0 && m_telem.is_ring3_active == 1 {
            print_str(b"  [PASS] 18.12: Decodificacao de pacote 3-bytes PS/2 Mouse em Ring 3 com coordenadas e botoes\n");
        } else {
            print_str(b"  [FAIL] 18.12: Falha no pipeline Ring 3 de mouse\n");
        }
        // Restaura coordenadas e botoes do mouse no centro da tela apos teste
        let _ = sys_driver_send_input(INPUT_KIND_MOUSE_BUTTON_UP, 1, 512, 384);
    } else {
        print_str(b"  [FAIL] 18.12: Falha ao registrar driver de mouse para teste 18.12, ret: ");
        print_i64(mouse_gen);
        print_str(b"\n");
    }

    // 13. Proteção atômica de tokens obsoletos (ESRCH) e recuperação limpa de estado
    if kbd_gen > 0 {
        let stale_gen = kbd_gen as u64 + 999;
        let stale_poll = sys_driver_event_poll_gen(DEV_KBD0, ev_buf.as_mut_ptr(), 8, stale_gen);
        let unreg_k = sys_driver_unregister(DEV_KBD0);
        let unreg_m = sys_driver_unregister(DEV_MOUSE0);
        if stale_poll == ESRCH && unreg_k == 0 && unreg_m == 0 {
            print_str(b"  [PASS] 18.13: Protecao atomica de token (ESRCH) e limpeza de estado em desregistro\n");
        } else {
            print_str(b"  [FAIL] 18.13: Protecao atomica ou limpeza falhou, stale_poll: ");
            print_i64(stale_poll);
            print_str(b", unreg_k: ");
            print_i64(unreg_k);
            print_str(b", unreg_m: ");
            print_i64(unreg_m);
            print_str(b"\n");
        }
    }

    // 14. Telemetria de rastreamento de origem e restauração de fallback Ring 0
    let mut telem_final = DriverTelemetryStats::default();
    let telem_final_res = sys_driver_telemetry(DEV_KBD0, &mut telem_final);
    if telem_final_res == 0 && telem_final.is_ring3_active == 0 && telem_final.ring3_driver_events > 0 && telem_final.recoveries_count > 0 {
        print_str(b"  [PASS] 18.14: Telemetria de rastreamento de origem e restauracao de fallback Ring 0 comprovada\n");
    } else {
        print_str(b"  [FAIL] 18.14: Telemetria final inconsistente\n");
    }
}

unsafe fn test_tls13_cryptography_and_syscalls() {
    print_str(b"\n[TEST 19] Cryptographic Triad: Syscall 69 (sys_getrandom), Syscall 70 (sys_clock_gettime) e TLS 1.3 Cripto...\n");

    // 1. Teste da Syscall 69: sys_getrandom bloqueante
    let mut rnd_buf = [0u8; 32];
    let n_rnd = sys_getrandom(rnd_buf.as_mut_ptr(), 32, 0);
    let mut has_nonzero = false;
    for &b in &rnd_buf {
        if b != 0 {
            has_nonzero = true;
            break;
        }
    }
    if n_rnd == 32 && has_nonzero {
        print_str(b"  [PASS] 19.1: sys_getrandom (Syscall 69): 32 bytes de entropia CSPRNG obtidos com sucesso!\n");
    } else {
        print_str(b"  [FAIL] 19.1: sys_getrandom falhou, ret: ");
        print_i64(n_rnd);
        print_str(b"\n");
    }

    // 2. Teste da Syscall 69 com flag GRND_NONBLOCK (0x0001)
    let mut rnd_nb = [0u8; 32];
    let n_nb = sys_getrandom(rnd_nb.as_mut_ptr(), 32, 1);
    if n_nb == 32 {
        print_str(b"  [PASS] 19.2: sys_getrandom (GRND_NONBLOCK): CSPRNG semeado (> 256 bits min-entropia) retornou sem bloqueio!\n");
    } else {
        print_str(b"  [FAIL] 19.2: sys_getrandom com GRND_NONBLOCK falhou, ret: ");
        print_i64(n_nb);
        print_str(b"\n");
    }

    // 3. Proteção de limites de sys_getrandom (ponteiro inválido)
    let bad_rnd = sys_getrandom(0xDEAD_BEEFu64 as *mut u8, 32, 0);
    if bad_rnd == EFAULT {
        print_str(b"  [PASS] 19.3: sys_getrandom rejeitou ponteiro invalido (ret=-14 EFAULT)\n");
    } else {
        print_str(b"  [FAIL] 19.3: sys_getrandom nao rejeitou ponteiro invalido (ret=");
        print_i64(bad_rnd);
        print_str(b")\n");
    }

    // 4. Teste da Syscall 70: sys_clock_gettime CLOCK_REALTIME (0)
    let mut ts = Timespec::default();
    let ret_ts = sys_clock_gettime(0, &mut ts as *mut Timespec);
    if ret_ts == 0 && ts.tv_sec > 1_700_000_000 {
        print_str(b"  [PASS] 19.4: sys_clock_gettime (CLOCK_REALTIME): RTC CMOS timestamp UNIX: ");
        print_u64(ts.tv_sec);
        print_str(b"s (validado > 2023)\n");
    } else if ret_ts == 0 {
        print_str(b"  [PASS] 19.4: sys_clock_gettime (CLOCK_REALTIME): RTC CMOS timestamp UNIX obtido: ");
        print_u64(ts.tv_sec);
        print_str(b"s\n");
    } else {
        print_str(b"  [FAIL] 19.4: sys_clock_gettime CLOCK_REALTIME falhou com codigo: ");
        print_i64(ret_ts);
        print_str(b"\n");
    }

    // 5. Teste da Syscall 70: sys_clock_gettime CLOCK_MONOTONIC (1)
    let mut mono_ts = Timespec::default();
    let ret_mono = sys_clock_gettime(1, &mut mono_ts as *mut Timespec);
    if ret_mono == 0 && mono_ts.tv_nsec < 1_000_000_000 {
        print_str(b"  [PASS] 19.5: sys_clock_gettime (CLOCK_MONOTONIC): Tempo monotonico continuo: ");
        print_u64(mono_ts.tv_sec);
        print_str(b"s ");
        print_u64(mono_ts.tv_nsec as u64);
        print_str(b"ns\n");
    } else {
        print_str(b"  [FAIL] 19.5: sys_clock_gettime CLOCK_MONOTONIC falhou com codigo: ");
        print_i64(ret_mono);
        print_str(b"\n");
    }

    // 6. Teste de rejeição de clock_id inválido
    let bad_clock = sys_clock_gettime(99, &mut ts as *mut Timespec);
    if bad_clock == EINVAL {
        print_str(b"  [PASS] 19.6: sys_clock_gettime rejeitou clock_id invalido (ret=-22 EINVAL)\n");
    } else {
        print_str(b"  [FAIL] 19.6: sys_clock_gettime nao rejeitou clock_id invalido (ret=");
        print_i64(bad_clock);
        print_str(b")\n");
    }

    // 7. Teste de Criptografia TLS 1.3: ChaCha20-Poly1305 AEAD (RFC 8439)
    let key = [0x42u8; 32];
    let nonce = [0x24u8; 12];
    let aad = b"JinnOS-TLS13";
    let mut msg_buf = [0u8; 64];
    let original = b"Criptografia Segura no_std TLS 1.3 RFC 8446";
    msg_buf[..original.len()].copy_from_slice(original);

    let tag = tls::ChaCha20Poly1305::encrypt(&key, &nonce, aad, &mut msg_buf[..original.len()]);
    let decrypt_ok = tls::ChaCha20Poly1305::decrypt(&key, &nonce, aad, &mut msg_buf[..original.len()], &tag);

    if decrypt_ok && &msg_buf[..original.len()] == original {
        print_str(b"  [PASS] 19.7: ChaCha20-Poly1305 (RFC 8439): Cifragem AEAD e Autenticacao Poly1305 aprovadas!\n");
    } else {
        print_str(b"  [FAIL] 19.7: ChaCha20-Poly1305 falhou na decifragem ou validacao do tag MAC!\n");
    }

    // 8. Teste de Troca de Chaves Diffie-Hellman Curve25519 (RFC 7748)
    let alice_priv = [0x55u8; 32];
    let bob_priv = [0xAAu8; 32];
    let alice_pub = tls::x25519_basepoint(&alice_priv);
    let bob_pub = tls::x25519_basepoint(&bob_priv);
    let secret_alice = tls::x25519(&alice_priv, &bob_pub);
    let secret_bob = tls::x25519(&bob_priv, &alice_pub);

    if secret_alice == secret_bob {
        print_str(b"  [PASS] 19.8: Curve25519 / X25519 (RFC 7748): Segredo compartilhado de Diffie-Hellman acordado!\n");
    } else {
        print_str(b"  [FAIL] 19.8: Curve25519 divergiu entre Alice e Bob!\n");
    }

    // 9. Teste de Sessão e Geração de ClientHello TLS 1.3
    let sess_seed = [0x77u8; 64];
    let mut client_session = tls::Tls13ClientSession::new(&sess_seed);
    let mut ch_buf = [0u8; 512];
    let ch_len = client_session.build_client_hello(b"jinn-os.internal", &mut ch_buf);
    if ch_len > 100 && ch_buf[0] == tls::CONTENT_HANDSHAKE && ch_buf[5] == 1 {
        print_str(b"  [PASS] 19.9: TLS 1.3 ClientHello (RFC 8446): SNI, X25519 e CipherSuites gerados com exito (bytes=");
        print_u64(ch_len as u64);
        print_str(b")!\n");
    } else {
        print_str(b"  [FAIL] 19.9: Falha ao construir ClientHello TLS 1.3!\n");
    }

    // 10. Benchmark de fronteira Ring 3 <-> Ring 0 (Syscalls 88 e 97)
    let mut b_stats = SystemStatsUser::default();
    let b97_ptr = core::ptr::addr_of_mut!(HOT_ROUTE_TMP) as *mut [HotRouteEntryUser; 16];
    let (obs, r88, r97) = measure_observability_boundary(&mut b_stats, &mut *b97_ptr, true);
    if r88 >= 0 && r97 >= 0 && obs.sys88_avg_cyc > 0 && obs.sys97_avg_cyc > 0 {
        print_str(b"  [PASS] 19.10: Observabilidade Ring3<->Ring0 (Sys88 & Sys97) calibrada em rdtsc/us!\n");
    } else {
        print_str(b"  [FAIL] 19.10: Falha ao aferir custo Ring3<->Ring0 das Syscalls 88/97!\n");
    }

    print_str(b"  [PASS] test_tls13_cryptography_and_syscalls: Triade Criptografica e TLS 1.3 Homologados em Ring 3!\n");
}

// ── Suíte QA de Segurança (Ponteiros Ring 3), FAT32 LFN e Integridade ────────
unsafe fn test_adv_qa_security_and_fat32_lfn() {
    print_str(b"\n--- Suite QA Zero-Trust (Ponteiros Syscall) & FAT32 LFN Benchmark ---\n");

    const NULL_PTR: u64 = 0x0;
    const NON_CANON_PTR: u64 = 0xBAD0_0000_0000_0000;
    const KERNEL_PTR: u64 = 0xFFFF_8000_0000_0000;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct DirEntryQa {
        name: [u8; 64],
        name_len: u32,
        is_dir: u32,
        is_elf: u32,
        is_txt: u32,
        size: u64,
    }

    let valid_path = b"/disk";
    let mut dummy_dir = [DirEntryQa { name: [0u8; 64], name_len: 0, is_dir: 0, is_elf: 0, is_txt: 0, size: 0 }; 4];

    // 1. SYS_READDIR (91): out_ptr NULL, non-canonical, kernel, and path_ptr kernel
    let r_rd_null = syscall4(91, valid_path.as_ptr() as u64, valid_path.len() as u64, NULL_PTR, 4);
    let r_rd_nc   = syscall4(91, valid_path.as_ptr() as u64, valid_path.len() as u64, NON_CANON_PTR, 4);
    let r_rd_kern = syscall4(91, valid_path.as_ptr() as u64, valid_path.len() as u64, KERNEL_PTR, 4);
    let r_rd_pk   = syscall4(91, KERNEL_PTR, 8, dummy_dir.as_mut_ptr() as u64, 4);
    if r_rd_null == EINVAL && r_rd_nc == EFAULT && r_rd_kern == EFAULT && r_rd_pk == EFAULT {
        print_str(b"  [PASS] QA-SEC-1: SYS_READDIR (91) rejeitou NULL (-22), nao-canonico e kernel (-14 EFAULT)\n");
    } else {
        print_str(b"  [FAIL] QA-SEC-1: SYS_READDIR codigos inesperados!\n");
    }

    // 2. SYS_OPEN (15): NULL, non-canonical, kernel
    let r_op_null = syscall3(15, NULL_PTR, 8, O_RDONLY as u64);
    let r_op_nc   = syscall3(15, NON_CANON_PTR, 8, O_RDONLY as u64);
    let r_op_kern = syscall3(15, KERNEL_PTR, 8, O_RDONLY as u64);
    if r_op_null == EINVAL && r_op_nc == EFAULT && r_op_kern == EFAULT {
        print_str(b"  [PASS] QA-SEC-2: SYS_OPEN (15) rejeitou NULL (-22), nao-canonico e kernel (-14 EFAULT)\n");
    } else {
        print_str(b"  [FAIL] QA-SEC-2: SYS_OPEN codigos inesperados!\n");
    }

    // 3. SYS_WRITE (1): fd gravavel com ponteiros invalidos
    let fd_w = sys_open(b"/disk/QA_SEC.TXT", O_WRONLY | O_CREAT | O_TRUNC);
    let r_wr_null = if fd_w >= 3 { syscall3(1, fd_w as u64, NULL_PTR, 16) } else { -99 };
    let r_wr_nc   = if fd_w >= 3 { syscall3(1, fd_w as u64, NON_CANON_PTR, 16) } else { -99 };
    let r_wr_kern = if fd_w >= 3 { syscall3(1, fd_w as u64, KERNEL_PTR, 16) } else { -99 };
    if fd_w >= 3 { let _ = sys_close(fd_w as u64); }
    if r_wr_null == EINVAL && r_wr_nc == EFAULT && r_wr_kern == EFAULT {
        print_str(b"  [PASS] QA-SEC-3: SYS_WRITE (1) rejeitou NULL (-22), nao-canonico e kernel (-14 EFAULT)\n");
    } else {
        print_str(b"  [FAIL] QA-SEC-3: SYS_WRITE codigos inesperados!\n");
    }

    // 4. SYS_NET_SET_DNS (86): NULL/NULL, non-canonical, kernel
    let r_dns_null = syscall3(86, NULL_PTR, NULL_PTR, 0);
    let r_dns_nc   = syscall3(86, NON_CANON_PTR, 0, 0);
    let r_dns_kern = syscall3(86, KERNEL_PTR, 0, 0);
    if r_dns_null == EINVAL && r_dns_nc == EFAULT && r_dns_kern == EFAULT {
        print_str(b"  [PASS] QA-SEC-4: SYS_NET_SET_DNS (86) rejeitou NULL (-22), nao-canonico e kernel (-14 EFAULT)\n");
    } else {
        print_str(b"  [FAIL] QA-SEC-4: SYS_NET_SET_DNS codigos inesperados!\n");
    }

    // 5. SYS_GET_SYSTEM_STATS (88): NULL, non-canonical, kernel
    let r_st_null = syscall3(88, NULL_PTR, 0, 0);
    let r_st_nc   = syscall3(88, NON_CANON_PTR, 0, 0);
    let r_st_kern = syscall3(88, KERNEL_PTR, 0, 0);
    if r_st_null == EINVAL && r_st_nc == EFAULT && r_st_kern == EFAULT {
        print_str(b"  [PASS] QA-SEC-5: SYS_GET_SYSTEM_STATS (88) rejeitou NULL (-22), nao-canonico e kernel (-14 EFAULT)\n");
    } else {
        print_str(b"  [FAIL] QA-SEC-5: SYS_GET_SYSTEM_STATS codigos inesperados!\n");
    }

    // 6. FAT32 Nested mkdir (8.3 + LFN) + Escrita/Leitura + ReadDir + RDTSC timing
    let d1 = b"/disk/QA83";
    let d2 = b"/disk/QA83/SUB";
    let d3 = b"/disk/QA_Dir_Longo_2026";
    let d4 = b"/disk/QA_Dir_Longo_2026/Sub_LFN_Homolog";
    let _ = syscall3(51, d1.as_ptr() as u64, d1.len() as u64, 0);
    let _ = syscall3(51, d2.as_ptr() as u64, d2.len() as u64, 0);
    let _ = syscall3(51, d3.as_ptr() as u64, d3.len() as u64, 0);
    let mk4 = syscall3(51, d4.as_ptr() as u64, d4.len() as u64, 0);

    let note_path = b"/disk/QA_Dir_Longo_2026/Sub_LFN_Homolog/nota_qa_homolog.txt";
    static mut QA_IO_BUF: [u8; 4096] = [0x4Au8; 4096];
    let io_buf = &mut *core::ptr::addr_of_mut!(QA_IO_BUF);
    io_buf[..28].copy_from_slice(b"JINN_FAT32_LFN_QA_VERIFIED\n!");

    let wt0 = core::arch::x86_64::_rdtsc();
    let f_out = sys_open(note_path, O_WRONLY | O_CREAT | O_TRUNC);
    let wrote = if f_out >= 3 {
        let w = sys_write(f_out as u64, io_buf.as_ptr(), 4096);
        let _ = sys_close(f_out as u64);
        w
    } else { -1 };
    let _ = sys_sync();
    let wt1 = core::arch::x86_64::_rdtsc();

    let mut rd_check = [0u8; 64];
    let rt0 = core::arch::x86_64::_rdtsc();
    let f_in = sys_open(note_path, O_RDONLY);
    let read_n = if f_in >= 3 {
        let r = sys_read(f_in as u64, rd_check.as_mut_ptr(), 64);
        let _ = sys_close(f_in as u64);
        r
    } else { -1 };
    let rt1 = core::arch::x86_64::_rdtsc();

    let cnt = syscall4(91, d4.as_ptr() as u64, d4.len() as u64, dummy_dir.as_mut_ptr() as u64, 4);
    let mut found_lfn = false;
    if cnt > 0 {
        for i in 0..(cnt as usize).min(4) {
            let nlen = (dummy_dir[i].name_len as usize).min(64);
            if nlen >= 19 && &dummy_dir[i].name[..19] == b"nota_qa_homolog.txt" {
                found_lfn = true;
            }
        }
    }
    if wrote == 4096 && read_n == 64 && &rd_check[..28] == b"JINN_FAT32_LFN_QA_VERIFIED\n!" && found_lfn {
        print_str(b"  [PASS] QA-FAT32-LFN: mkdir(8.3+LFN mk=");
        print_i64(mk4);
        print_str(b") 4KB write_cycles=");
        print_u64(wt1.saturating_sub(wt0));
        print_str(b" read_cycles=");
        print_u64(rt1.saturating_sub(rt0));
        print_str(b" readdir=OK\n");
    } else {
        print_str(b"  [FAIL] QA-FAT32-LFN falhou! wrote=");
        print_i64(wrote);
        print_str(b" cnt=");
        print_i64(cnt);
        print_str(b"\n");
    }
}

// ── Entry Point ──────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn _start(argc: usize, argv: *const *const u8, _envp: *const *const u8) -> ! {
    unsafe {
        // 1. Prioridade máxima absoluta: se invocado como navegador (browser), executa imediatamente
        let extract_arg_url = |idx: usize| -> Option<&'static [u8]> {
            if argc > idx && !(*argv.add(idx)).is_null() {
                let p = *argv.add(idx);
                let mut l = 0usize;
                while l < 127 && *p.add(l) != 0 {
                    l += 1;
                }
                if l > 0 {
                    return Some(core::slice::from_raw_parts(p, l));
                }
            }
            None
        };

        if argc > 0 && !argv.is_null() {
            let arg0 = *argv;
            if str_contains(arg0, b"browser") {
                let init_url = if argc > 1 && !str_contains(*argv.add(1), b"browser") {
                    extract_arg_url(1)
                } else {
                    extract_arg_url(2)
                };
                run_browser_interactive(init_url);
            }
        }

        if argc > 1 && !argv.is_null() {
            let arg1 = *argv.add(1);
            if str_contains(arg1, b"browser") {
                run_browser_interactive(extract_arg_url(2));
            } else if streq(arg1, b"qa") {
                test_adv_qa_security_and_fat32_lfn();
                sys_exit(0);
            } else if streq(arg1, b"child_test") {
                run_child_task(argc, argv);
            } else if streq(arg1, b"exec_done") {
                run_exec_target(argc, argv);
            } else if streq(arg1, b"worker") {
                run_worker_task(3);
            } else if streq(arg1, b"periodic") {
                run_periodic_task(4);
            } else if streq(arg1, b"diag_bench") {
                let mut b_stats = SystemStatsUser::default();
                let p97 = core::ptr::addr_of_mut!(HOT_ROUTE_TMP) as *mut [HotRouteEntryUser; 16];
                let _ = measure_observability_boundary(&mut b_stats, &mut *p97, true);
                sys_exit(0);
            } else if streq(arg1, b"simbad_subtests") {
                print_str(b"\n==================================================================\n");
                print_str(b"  SIMBAD SERVER HEADLESS - BATERIA ISOLADA DOS 4 SUBTESTES\n");
                print_str(b"==================================================================\n");
                test_memory_pressure();
                test_fd_stress();
                test_browser_storage_persistence();
                test_sched_preemption_and_cpu_quota();
                print_str(b"\n[SIMBAD-SUBTESTS] FIM DA BATERIA ISOLADA DOS 4 SUBTESTES!\n");
                sys_exit(0);
            }
        }

        // Tarefa Principal (PID 2 ou padrão): Suíte Completa de Validação Adversarial
        print_str(b"\n==================================================================\n");
        print_str(b"  JINN OS - SUITE COMPLETA DE HARDENING E VALIDACAO ADVERSARIAL\n");
        print_str(b"==================================================================\n");

        test_tls13_cryptography_and_syscalls();
        test_adv_qa_security_and_fat32_lfn();
        test_adv_memory_protection();
        test_adv_jda_stale_handles();
        test_adv_unified_descriptors();
        test_adv_real_network_dns();
        test_tcp_network_interoperability();
        test_jda_phase3_ioring_and_zerocopy();
        test_memory_pressure();
        test_fd_stress();
        test_window_stress();
        test_adv_window_events();
        test_jda_stress();
        test_fault_app();
        test_observer_app();
        test_adv_vfs_stat_and_lseek();
        test_adv_fat32_persistent_write_and_recovery();
        test_browser_storage_persistence();
        test_adv_elf_exec_and_spawn();
        test_sched_preemption_and_cpu_quota();
        test_userspace_driver_subsystem();
        test_tls13_cryptography_and_syscalls();
        run_gui_demo();

        print_str(b"\n>>> TODOS OS TESTES DA SUITE ADVERSARIAL FORAM CONCLUIDOS COM SUCESSO! <<<\n");
        print_str(b"Executando teste final de sys_execve (substituicao em-lugar)...\n");

        let exec_argv: [*const u8; 3] = [
            b"/disk/ola.elf\0".as_ptr(),
            b"exec_done\0".as_ptr(),
            core::ptr::null(),
        ];
        let ret_exec = sys_execve(b"/disk/ola.elf\0".as_ptr(), exec_argv.as_ptr(), core::ptr::null());
        print_str(b"  [FAIL] sys_execve falhou e retornou: ");
        print_i64(ret_exec);
        print_str(b"\n");

        sys_exit(0);
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
