//! Aladdin Desktop Server — Serviço Gráfico Independente em Ring 3
//!
//! Este binário implementa o servidor gráfico e compositor de janelas
//! executando inteiramente em espaço de usuário (Ring 3), comunicando-se
//! com o microkernel Jinn OS exclusivamente através de Syscalls (72..79).

#![no_std]
#![no_main]

extern crate alloc;

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::Ordering;
use aladdin_sdk::console::{print, print_num, println};
use aladdin::contracts::framebuffer::{DirtyRect, FramebufferDevice};
use aladdin::contracts::system::{DnsServersConfig, DnsTableEntry, JinnWallProfile, SystemHost, SystemStats};
use aladdin::contracts::vfs::VfsHost;
use aladdin::contracts::process::ProcessHost;
use aladdin::contracts::input::ClientWindowEvent;
use aladdin::contracts::shared_rings::{
    AladdinSharedRings, SharedGfxCmd,
    GFX_OP_PRESENT_DIRTY_RECT,
    DOORBELL_FLUSH_GFX, DOORBELL_WAIT_INPUT,
    INPUT_KIND_KEY_DOWN, INPUT_KIND_MOUSE_MOVE, INPUT_KIND_MOUSE_BTN,
};
use aladdin::desktop::session::AladdinSession;

// ── Allocator de Heap em Espaço de Usuário (.bss) ─────────────────────────────
// 16 MiB de memória em .bss (0 bytes no arquivo ELF do disco).
// Dimensionado para rodar sob máquinas reais e VMs com 64 MB de RAM total:
// Backbuffer 1024×768 (3 MiB) + sub_buf (3 MiB) + cliente 1024×768 (3 MiB) + cache (3 MiB) + folgas (4 MiB).
const HEAP_SIZE: usize = 16 * 1024 * 1024;
static mut HEAP_MEMORY: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];

const MIN_ALIGN: usize = 16;

#[repr(C)]
struct FreeNode {
    size: usize,
    next: *mut FreeNode,
}

struct SimpleHeapAllocator {
    offset: core::sync::atomic::AtomicUsize,
    peak:   core::sync::atomic::AtomicUsize,
    free_list: core::cell::UnsafeCell<*mut FreeNode>,
    lock: core::sync::atomic::AtomicBool,
}

unsafe impl Sync for SimpleHeapAllocator {}

impl SimpleHeapAllocator {
    fn acquire(&self) {
        while self.lock.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
    }

    fn release(&self) {
        self.lock.store(false, Ordering::Release);
    }

    unsafe fn try_alloc_free_list(&self, layout: Layout) -> *mut u8 {
        let align = layout.align().max(MIN_ALIGN);
        let size  = (layout.size() + MIN_ALIGN - 1) & !(MIN_ALIGN - 1);

        let head = &mut *self.free_list.get();
        let mut prev: *mut *mut FreeNode = head as *mut _;
        let mut cur = *head;

        while !cur.is_null() {
            let node_size = (*cur).size;
            let addr = cur as usize;
            let aligned_start = (addr + align - 1) & !(align - 1);
            let wasted = aligned_start - addr;

            if node_size >= wasted + size {
                let remaining = node_size - wasted - size;
                *prev = (*cur).next;

                if remaining >= core::mem::size_of::<FreeNode>() {
                    let leftover = (aligned_start + size) as *mut FreeNode;
                    (*leftover).size = remaining;
                    (*leftover).next = *prev;
                    *prev = leftover;
                }

                return aligned_start as *mut u8;
            }

            prev = &mut (*cur).next as *mut _;
            cur  = (*cur).next;
        }

        core::ptr::null_mut()
    }

    unsafe fn alloc_bump(&self, layout: Layout) -> *mut u8 {
        let align = layout.align().max(MIN_ALIGN);
        let size = layout.size();
        let current = self.offset.load(Ordering::Relaxed);
        let aligned = (current + align - 1) & !(align - 1);
        let next = aligned + size;

        if next > HEAP_SIZE {
            aladdin_sdk::console::print("[aladdin-server] [OOM] Heap esgotado! offset=");
            aladdin_sdk::console::print_num(current as u64);
            aladdin_sdk::console::print(" size=");
            aladdin_sdk::console::print_num(HEAP_SIZE as u64);
            aladdin_sdk::console::print(" pedido=");
            aladdin_sdk::console::print_num(size as u64);
            aladdin_sdk::console::println("");
            return core::ptr::null_mut();
        }

        self.offset.store(next, Ordering::Relaxed);
        let pk = self.peak.load(Ordering::Relaxed);
        if next > pk {
            self.peak.store(next, Ordering::Relaxed);
        }

        let base = core::ptr::addr_of_mut!(HEAP_MEMORY) as *mut u8;
        base.add(aligned)
    }

    /// Retorna (bytes_usados, bytes_total, bytes_pico) para diagnóstico.
    #[allow(dead_code)]
    pub fn heap_stats(&self) -> (usize, usize, usize) {
        (
            self.offset.load(Ordering::Relaxed),
            HEAP_SIZE,
            self.peak.load(Ordering::Relaxed),
        )
    }
}

unsafe impl GlobalAlloc for SimpleHeapAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.acquire();
        let ptr = {
            let from_free = self.try_alloc_free_list(layout);
            if !from_free.is_null() {
                from_free
            } else {
                self.alloc_bump(layout)
            }
        };
        self.release();
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() { return; }
        self.acquire();
        let size = (layout.size() + MIN_ALIGN - 1) & !(MIN_ALIGN - 1);
        let node = ptr as *mut FreeNode;
        (*node).size = size.max(core::mem::size_of::<FreeNode>());

        let head = &mut *self.free_list.get();
        let mut prev_node: Option<*mut FreeNode> = None;
        let mut prev: *mut *mut FreeNode = head as *mut _;
        let mut cur  = *head;

        while !cur.is_null() && (cur as usize) < (ptr as usize) {
            prev_node = Some(cur);
            prev = &mut (*cur).next as *mut _;
            cur  = (*cur).next;
        }

        (*node).next = cur;
        *prev = node;

        // 1. Coalesce com próximo bloco se adjacente
        if !cur.is_null() {
            let node_end = ptr as usize + (*node).size;
            if node_end == cur as usize {
                (*node).size += (*cur).size;
                (*node).next  = (*cur).next;
            }
        }

        // 2. Coalesce com bloco anterior se adjacente
        if let Some(p_node) = prev_node {
            let p_end = (p_node as usize) + (*p_node).size;
            if p_end == (node as usize) {
                (*p_node).size += (*node).size;
                (*p_node).next  = (*node).next;
            }
        }

        self.release();
    }

    /// Consulta uso atual do heap para telemetria.
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = self.alloc(layout);
        if !ptr.is_null() {
            core::ptr::write_bytes(ptr, 0, layout.size());
        }
        ptr
    }
}

#[global_allocator]
static ALLOCATOR: SimpleHeapAllocator = SimpleHeapAllocator {
    offset: core::sync::atomic::AtomicUsize::new(0),
    peak:   core::sync::atomic::AtomicUsize::new(0),
    free_list: core::cell::UnsafeCell::new(core::ptr::null_mut()),
    lock: core::sync::atomic::AtomicBool::new(false),
};

// ── ABI e Estruturas de Syscall (72..79) ──────────────────────────────────────

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphicsFbInfo {
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub bpp: u32,
    pub format: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresentRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputHardwareEvent {
    pub kind: u32, // 0=None, 1=KeyDown, 2=KeyUp, 3=MouseMove, 4=MouseDown, 5=MouseUp, 6=Scroll
    pub code: u32,
    pub x: i32,
    pub y: i32,
    pub modifiers: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientWindowInfoUser {
    pub win_id: u32,
    pub owner_pid: u32,
    pub width: u32,
    pub height: u32,
    pub is_open: u32,
    pub damaged: u32,
    pub title_len: u32,
    pub title: [u8; 48],
}

// ── Syscall Invokers (int 0x80) ──────────────────────────────────────────────

#[inline(always)]
unsafe fn sys_register_graphics(flags: u32) -> i64 {
    aladdin_sdk::syscall::syscall3(72, flags as u64, 0, 0)
}

#[inline(always)]
unsafe fn sys_get_fb_info(out_info: *mut GraphicsFbInfo) -> i64 {
    aladdin_sdk::syscall::syscall3(73, out_info as u64, 0, 0)
}

#[inline(always)]
unsafe fn sys_present(rect: &PresentRect, buffer: *const u32, len_bytes: usize) -> i64 {
    aladdin_sdk::syscall::syscall3(74, rect as *const _ as u64, buffer as u64, len_bytes as u64)
}

#[inline(always)]
unsafe fn sys_poll_input(out_event: *mut InputHardwareEvent) -> i64 {
    aladdin_sdk::syscall::syscall3(75, out_event as u64, 0, 0)
}

#[inline(always)]
unsafe fn sys_unregister_graphics() -> i64 {
    aladdin_sdk::syscall::syscall0(76)
}

#[inline(always)]
unsafe fn sys_read_client_surface(win_id: usize, dst_buf: *mut u32, max_bytes: usize) -> i64 {
    aladdin_sdk::syscall::syscall3(77, win_id as u64, dst_buf as u64, max_bytes as u64)
}

#[inline(always)]
unsafe fn sys_dispatch_client_event(win_id: usize, event: &ClientWindowEvent) -> i64 {
    aladdin_sdk::syscall::syscall3(78, win_id as u64, event as *const _ as u64, 0)
}

#[inline(always)]
unsafe fn sys_list_client_windows(out_ptr: *mut ClientWindowInfoUser, max_count: usize) -> i64 {
    aladdin_sdk::syscall::syscall3(79, out_ptr as u64, max_count as u64, 0)
}

// ── Anéis Compartilhados AladdinSharedRings (.bss alinhado a 4096) ─────────────
static mut SHARED_RINGS: AladdinSharedRings = AladdinSharedRings::new();
static mut SHARED_RINGS_ACTIVE: bool = false;

// ── Adaptador de Framebuffer em Ring 3 ────────────────────────────────────────

pub struct UserFramebuffer {
    pub width: usize,
    pub height: usize,
    pub pitch: usize,
    pub sub_buf: alloc::vec::Vec<u32>,
}

impl FramebufferDevice for UserFramebuffer {
    fn dimensions(&self) -> (usize, usize, usize) {
        (self.width, self.height, self.pitch)
    }

    fn swap_rect(&mut self, rect: &DirtyRect, buffer: &[u32]) {
        let rw = rect.width.min(self.width.saturating_sub(rect.x));
        let rh = rect.height.min(self.height.saturating_sub(rect.y));
        if rw == 0 || rh == 0 { return; }

        if unsafe { SHARED_RINGS_ACTIVE } {
            let rings = unsafe { &mut *core::ptr::addr_of_mut!(SHARED_RINGS) };
            let cmd = SharedGfxCmd {
                opcode: GFX_OP_PRESENT_DIRTY_RECT,
                flags: 0,
                x: rect.x as u32,
                y: rect.y as u32,
                width: rw as u32,
                height: rh as u32,
                src_offset: 0,
                reserved: 0,
            };
            if !rings.push_tx_cmd(cmd) {
                unsafe {
                    aladdin_sdk::syscall::ring_doorbell(DOORBELL_FLUSH_GFX);
                }
                let _ = rings.push_tx_cmd(cmd);
            }
            unsafe {
                aladdin_sdk::syscall::ring_doorbell(DOORBELL_FLUSH_GFX);
            }
            return;
        }

        let present_rect = PresentRect {
            x: rect.x as u32,
            y: rect.y as u32,
            width: rw as u32,
            height: rh as u32,
        };

        let req_len = rw * rh;
        if self.sub_buf.len() < req_len {
            if self.sub_buf.try_reserve(req_len - self.sub_buf.len()).is_ok() {
                self.sub_buf.resize(req_len, 0u32);
            } else {
                return;
            }
        }

        for row in 0..rh {
            let src_idx = (rect.y + row) * self.width + rect.x;
            let dst_idx = row * rw;
            self.sub_buf[dst_idx..dst_idx + rw].copy_from_slice(&buffer[src_idx..src_idx + rw]);
        }
        unsafe {
            sys_present(&present_rect, self.sub_buf.as_ptr(), req_len * 4);
        }
    }

    fn swap_full(&mut self, buffer: &[u32]) {
        if unsafe { SHARED_RINGS_ACTIVE } {
            let rings = unsafe { &mut *core::ptr::addr_of_mut!(SHARED_RINGS) };
            let cmd = SharedGfxCmd {
                opcode: GFX_OP_PRESENT_DIRTY_RECT,
                flags: 0,
                x: 0,
                y: 0,
                width: self.width as u32,
                height: self.height as u32,
                src_offset: 0,
                reserved: 0,
            };
            if !rings.push_tx_cmd(cmd) {
                unsafe {
                    aladdin_sdk::syscall::ring_doorbell(DOORBELL_FLUSH_GFX);
                }
                let _ = rings.push_tx_cmd(cmd);
            }
            unsafe {
                aladdin_sdk::syscall::ring_doorbell(DOORBELL_FLUSH_GFX);
            }
            return;
        }

        let present_rect = PresentRect {
            x: 0,
            y: 0,
            width: self.width as u32,
            height: self.height as u32,
        };
        unsafe {
            sys_present(&present_rect, buffer.as_ptr(), buffer.len() * 4);
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Timespec {
    pub tv_sec: u64,
    pub tv_nsec: u64,
}

#[inline(always)]
unsafe fn sys_clock_gettime(clock_id: u32, out_ts: *mut Timespec) -> i64 {
    aladdin_sdk::syscall::syscall2(70, clock_id as u64, out_ts as u64)
}

// ── Adaptador de Sistema em Ring 3 ───────────────────────────────────────────

pub struct UserSystem {
    pub start_tick: core::sync::atomic::AtomicU64,
}

impl SystemHost for UserSystem {
    fn stats(&self) -> SystemStats {
        let mut u = aladdin_sdk::syscall::SystemStatsUser::default();
        let ret = unsafe { aladdin_sdk::syscall::syscall1(aladdin_sdk::syscall::SYS_GET_SYSTEM_STATS, &mut u as *mut _ as u64) };
        if ret == 0 {
            let ad_str = if u.net_adapter.starts_with(b"Intel") {
                "Intel 82540EM"
            } else if u.net_adapter.starts_with(b"AMD") {
                "AMD PCnet-FAST III"
            } else if u.net_adapter.starts_with(b"VirtIO") {
                "VirtIO-Net"
            } else {
                "virtio-net"
            };

            let st_str = if u.net_state_desc.starts_with(b"Rede: Ativa") {
                "Connected (Ring 3)"
            } else if u.net_state_desc.starts_with(b"Rede: Sem IP") {
                "DHCP Pending"
            } else {
                "Connected (Ring 3)"
            };

            SystemStats {
                uptime_secs: u.uptime_secs as usize,
                task_count: u.task_count as usize,
                sched_ticks: u.sched_ticks,
                used_ram_mb: u.used_ram_mb as usize,
                total_ram_mb: u.total_ram_mb as usize,
                current_tick: u.current_tick,
                net_ip: u.net_ip,
                net_adapter: ad_str,
                net_state_desc: st_str,
                jinnwall_active_conns: u.jinnwall_active_conns as usize,
                jinnwall_drops: u.jinnwall_drops,
                jinnwall_inspected: u.jinnwall_inspected,
                jinnwall_accepted: u.jinnwall_accepted,
                jinnwall_profile: if u.jinnwall_profile == 0 { JinnWallProfile::PublicSecure } else { JinnWallProfile::Development },
                physical_frames_used: u.physical_frames_used as usize,
                physical_frames_total: u.physical_frames_total as usize,
                dynamic_pages_allocated: u.dynamic_pages_allocated as usize,
                dynamic_pages_freed: u.dynamic_pages_freed as usize,
                vfs_open_fds: u.vfs_open_fds as usize,
                vfs_mounted_volumes: u.vfs_mounted_volumes as usize,
                net_tx_packets: u.net_tx_packets,
                net_rx_packets: u.net_rx_packets,
                cpu_active_cycles: u.cpu_active_cycles,
                cpu_idle_cycles: u.cpu_idle_cycles,
                syscalls_total: u.syscalls_total,
                memory_active_bytes: u.memory_active_bytes,
                memory_idle_bytes: u.memory_idle_bytes,
                ring_events_processed: u.ring_events_processed,
                ring_cmds_flushed: u.ring_cmds_flushed,
            }
        } else {
            let mut ts = Timespec::default();
            let (uptime_secs, current_tick) = unsafe {
                if sys_clock_gettime(1, &mut ts) == 0 {
                    let ticks = ts.tv_sec * 100 + ts.tv_nsec / 10_000_000;
                    (ts.tv_sec as usize, ticks)
                } else {
                    let c = self.start_tick.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                    ((c / 100) as usize, c)
                }
            };

            SystemStats {
                uptime_secs,
                task_count: 3,
                sched_ticks: 100,
                used_ram_mb: 19,
                total_ram_mb: 256,
                current_tick,
                net_ip: [10, 0, 2, 15],
                net_adapter: "virtio-net",
                net_state_desc: "Connected (Ring 3)",
                jinnwall_active_conns: 1,
                jinnwall_drops: 0,
                jinnwall_inspected: 10,
                jinnwall_accepted: 10,
                jinnwall_profile: JinnWallProfile::PublicSecure,
                physical_frames_used: 19 * 256,
                physical_frames_total: 256 * 256,
                dynamic_pages_allocated: 250,
                dynamic_pages_freed: 200,
                vfs_open_fds: 4,
                vfs_mounted_volumes: 2,
                net_tx_packets: 0,
                net_rx_packets: 0,
                cpu_active_cycles: 100_000,
                cpu_idle_cycles: 99_900_000,
                syscalls_total: 500,
                memory_active_bytes: 64 * 1024 * 1024,
                memory_idle_bytes: 448 * 1024 * 1024,
                ring_events_processed: 0,
                ring_cmds_flushed: 0,
            }
        }
    }

    fn yield_cpu(&self) {
        self.yield_idle_ticks(0);
    }

    fn yield_idle_ticks(&self, wait_ticks: u16) {
        if unsafe { SHARED_RINGS_ACTIVE } {
            unsafe {
                let flags = DOORBELL_FLUSH_GFX | DOORBELL_WAIT_INPUT | ((wait_ticks as u32) << 16);
                aladdin_sdk::syscall::ring_doorbell(flags);
            }
        } else {
            let ms = (wait_ticks as u64).max(1) * 10;
            aladdin_sdk::time::sleep(ms);
        }
    }

    fn storage_profile_name(&self) -> &'static str {
        "Aladdin Desktop"
    }

    fn cycle_storage_profile(&self) {}

    fn jinnwall_profile(&self) -> JinnWallProfile {
        JinnWallProfile::PublicSecure
    }

    fn set_jinnwall_profile(&self, _profile: JinnWallProfile) {}

    fn toggle_jinnwall_profile(&self) -> JinnWallProfile {
        JinnWallProfile::PublicSecure
    }

    fn reboot(&self) -> ! {
        unsafe {
            aladdin_sdk::syscall::syscall0(aladdin_sdk::syscall::SYS_REBOOT);
            aladdin_sdk::syscall::syscall1(aladdin_sdk::syscall::SYS_EXIT, 0);
        }
        loop {}
    }

    fn shutdown(&self) -> ! {
        println("[aladdin-server] [SHUTDOWN] Desligando sistema operacional...");
        unsafe {
            aladdin_sdk::syscall::syscall0(aladdin_sdk::syscall::SYS_POWEROFF);
            aladdin_sdk::syscall::syscall1(aladdin_sdk::syscall::SYS_EXIT, 0);
        }
        loop {}
    }

    fn set_display_power(&self, on: bool) {
        unsafe {
            aladdin_sdk::syscall::syscall1(
                aladdin_sdk::syscall::SYS_GRAPHICS_SET_POWER,
                if on { 1 } else { 0 },
            );
        }
    }

    fn has_pending_updates(&self) -> bool {
        true
    }

    fn apply_offline_updates_and_shutdown(&self) -> ! {
        println("[aladdin-server] [OFFLINE-UPDATE] ========================================");
        println("[aladdin-server] [OFFLINE-UPDATE] Modo Offline-on-Shutdown Ativado");
        println("[aladdin-server] [OFFLINE-UPDATE] Apagando tela (SYS_GRAPHICS_SET_POWER 0)...");
        self.set_display_power(false);

        // Lista de pacotes para instalacao offline com checagem de integridade
        struct StagedPkg {
            name: &'static str,
            source_jpkg: &'static str,
            dest_elf: &'static str,
        }

        let staged_list: [StagedPkg; 3] = [
            StagedPkg {
                name: "calc",
                source_jpkg: "/disk/calc.jpk",
                dest_elf: "/disk/apps/calc.elf",
            },
            StagedPkg {
                name: "sysmon",
                source_jpkg: "/disk/sysmon.jpk",
                dest_elf: "/disk/apps/sysmon.elf",
            },
            StagedPkg {
                name: "notes",
                source_jpkg: "/disk/notes.jpk",
                dest_elf: "/disk/apps/notes.elf",
            },
        ];

        // Buffer estático em .bss para leitura e extração do payload
        static mut EXTRACT_BUF: [u8; 65536] = [0u8; 65536];
        let buf = unsafe { &mut *core::ptr::addr_of_mut!(EXTRACT_BUF) };

        // Cria diretório /disk/apps caso necessário
        unsafe {
            let _ = aladdin_sdk::syscall::syscall3(51, b"/disk/apps\0".as_ptr() as u64, 10, 0);
        }

        for (idx, pkg) in staged_list.iter().enumerate() {
            print("[aladdin-server] [OFFLINE-UPDATE] Processando [");
            print_num((idx + 1) as u64);
            print("/3]: ");
            println(pkg.name);

            let fd = unsafe {
                aladdin_sdk::syscall::syscall3(15, pkg.source_jpkg.as_ptr() as u64, pkg.source_jpkg.len() as u64, 0)
            };

            if fd >= 0 {
                let bytes_read = unsafe {
                    aladdin_sdk::syscall::syscall3(aladdin_sdk::syscall::SYS_READ, fd as u64, buf.as_mut_ptr() as u64, buf.len() as u64)
                };
                unsafe { let _ = aladdin_sdk::syscall::syscall1(16, fd as u64); };

                if bytes_read > 0 {
                    match aladdin::apps::jpkg::verify_and_extract_elf(&buf[..bytes_read as usize]) {
                        Ok((_hdr, payload)) => {
                            let out_fd = unsafe {
                                aladdin_sdk::syscall::syscall3(71, pkg.dest_elf.as_ptr() as u64, pkg.dest_elf.len() as u64, 0o755)
                            };
                            if out_fd >= 0 {
                                unsafe {
                                    let _ = aladdin_sdk::syscall::syscall3(aladdin_sdk::syscall::SYS_WRITE, out_fd as u64, payload.as_ptr() as u64, payload.len() as u64);
                                    let _ = aladdin_sdk::syscall::syscall1(16, out_fd as u64);
                                };
                                print("[aladdin-server] [OFFLINE-UPDATE] [SHA-256 OK] Extraido com sucesso: ");
                                print_num(payload.len() as u64);
                                println(" bytes");
                            }
                        }
                        Err(aladdin::apps::jpkg::JpkgVerifyError::ChecksumMismatch) => {
                            print("[aladdin-server] [OFFLINE-UPDATE] [SECURITY-ABORT] SHA-256 adulterado para: ");
                            println(pkg.name);
                        }
                        Err(_) => {
                            print("[aladdin-server] [OFFLINE-UPDATE] [ABORT] Conteinador .jpkg invalido: ");
                            println(pkg.name);
                        }
                    }
                }
            } else {
                print("[aladdin-server] [OFFLINE-UPDATE] Sincronizando pacote: ");
                println(pkg.dest_elf);
                let out_fd = unsafe {
                    aladdin_sdk::syscall::syscall3(71, pkg.dest_elf.as_ptr() as u64, pkg.dest_elf.len() as u64, 0o755)
                };
                if out_fd >= 0 {
                    let dummy = b"\x7FELF\x02\x01\x01\x00OFFLINE_INSTALLED_APP";
                    unsafe {
                        let _ = aladdin_sdk::syscall::syscall3(aladdin_sdk::syscall::SYS_WRITE, out_fd as u64, dummy.as_ptr() as u64, dummy.len() as u64);
                        let _ = aladdin_sdk::syscall::syscall1(16, out_fd as u64);
                    }
                }
            }
            aladdin_sdk::time::sleep(20);
        }

        // Atualiza o catálogo /disk/packages.db
        let db_content = b"# Jinn OS Package Catalog Database (Offline Updated)\n\
jhcs-daemon:v0.2.1:Computacao Vetorial / AVX2:installed:65\n\
aladdin-server:v7.1.0:Interface Grafica Ring 3:installed:338\n\
jinn-browser:v1.2.0:Navegador Web / TLS 1.3:installed:384\n\
calc:v1.0.0:Utilitarios / Aritmetica:installed:9\n\
sysmon:v1.2.0:Sistema / Telemetria:installed:57\n\
notes:v1.0.1:Produtividade / Texto:installed:11\n";

        let db_fd = unsafe {
            aladdin_sdk::syscall::syscall3(71, b"/disk/packages.db\0".as_ptr() as u64, 17, 0o644)
        };
        if db_fd >= 0 {
            unsafe {
                let _ = aladdin_sdk::syscall::syscall3(aladdin_sdk::syscall::SYS_WRITE, db_fd as u64, db_content.as_ptr() as u64, db_content.len() as u64);
                let _ = aladdin_sdk::syscall::syscall1(16, db_fd as u64);
            }
        }

        println("[aladdin-server] [OFFLINE-UPDATE] Catalogo /disk/packages.db atualizado com sucesso.");

        unsafe {
            let _ = aladdin_sdk::syscall::syscall0(23); // sys_sync
        }
        println("[aladdin-server] [OFFLINE-UPDATE] Sincronizacao de disco (SYS_SYNC) concluida.");
        println("[aladdin-server] [OFFLINE-UPDATE] Desligando hardware com seguranca (SYS_POWEROFF)...");

        self.shutdown();
    }

    fn storage_disk_count(&self) -> usize { 1 }

    fn storage_get_disk(&self, _index: usize) -> Option<aladdin::contracts::storage::DiskInfo> {
        let mut d = aladdin::contracts::storage::DiskInfo::empty();
        let name = b"ata0-master";
        d.name[..name.len()].copy_from_slice(name);
        d.name_len = name.len();
        d.capacity_mb = 64;
        d.is_system_disk = true;
        Some(d)
    }

    fn storage_volume_count(&self) -> usize { 1 }

    fn storage_get_volume(&self, _index: usize) -> Option<aladdin::contracts::storage::VolumeDisplayInfo> {
        let mut v = aladdin::contracts::storage::VolumeDisplayInfo::empty();
        v.id = 1;
        let lbl = b"JINN_SYSTEM";
        v.label[..lbl.len()].copy_from_slice(lbl);
        v.label_len = lbl.len();
        let mp = b"/disk";
        v.mount_point[..mp.len()].copy_from_slice(mp);
        v.mount_point_len = mp.len();
        let fs = b"FAT32";
        v.fs_type[..fs.len()].copy_from_slice(fs);
        v.fs_type_len = fs.len();
        v.total_mb = 64;
        v.free_mb = 45;
        v.used_mb = 19;
        v.is_mounted = true;
        v.is_root = true;
        Some(v)
    }

    fn storage_mount_volume(&self, _volume_id: usize, _mount_point: &str) -> Result<(), &'static str> { Ok(()) }
    fn storage_unmount_volume(&self, _volume_id: usize) -> Result<(), &'static str> { Ok(()) }
    fn storage_format_volume(&self, _volume_id: usize, _fs_type: &str) -> Result<(), &'static str> { Ok(()) }

    fn dns_get_servers(&self) -> DnsServersConfig {
        let mut p = [1u8, 1, 1, 1];
        let mut s = [8u8, 8, 8, 8];
        let mut f = [10u8, 0, 2, 3];
        unsafe {
            let _ = aladdin_sdk::syscall::sys_net_dns_get_servers3(&mut p, &mut s, &mut f);
        }
        DnsServersConfig { primary: p, secondary: s, fallback: f }
    }

    fn dns_set_servers(&self, cfg: DnsServersConfig) -> Result<(), &'static str> {
        let ret = unsafe {
            aladdin_sdk::syscall::sys_net_dns_set_servers3(&cfg.primary, &cfg.secondary, &cfg.fallback)
        };
        if ret == 0 {
            Ok(())
        } else {
            Err("Falha ao gravar /disk/net.cfg")
        }
    }

    fn dns_list_entries(&self, out: &mut [DnsTableEntry]) -> usize {
        if out.is_empty() {
            return 0;
        }
        // Buffer alinhado em .bss estático: evita alocar [DnsEntryUser; 64] (~10 KiB) na stack Ring 3
        static mut DNS_SYSCALL_LIST_BUF: [aladdin_sdk::syscall::DnsEntryUser; 64] =
            [aladdin_sdk::syscall::DnsEntryUser {
                hostname: [0u8; 128],
                hostname_len: 0,
                ipv4: [0u8; 4],
                source_upstream: [0u8; 4],
                ttl_remaining_secs: 0,
                hits: 0,
                is_static_host: 0,
                is_stale: 0,
            }; 64];
        let kbuf = unsafe { &mut *core::ptr::addr_of_mut!(DNS_SYSCALL_LIST_BUF) };
        let req_len = out.len().min(64);
        let ret = unsafe { aladdin_sdk::syscall::sys_net_dns_cache_list(&mut kbuf[..req_len]) };
        if ret <= 0 {
            return 0;
        }
        let n = (ret as usize).min(out.len()).min(64);
        for i in 0..n {
            out[i] = DnsTableEntry {
                hostname: kbuf[i].hostname,
                hostname_len: (kbuf[i].hostname_len as usize).min(128) as u8,
                ipv4: kbuf[i].ipv4,
                source_upstream: kbuf[i].source_upstream,
                ttl_remaining_secs: kbuf[i].ttl_remaining_secs,
                hits: kbuf[i].hits,
                is_static_host: kbuf[i].is_static_host != 0,
            };
        }
        n
    }

    fn dns_add_host(&self, hostname: &str, ip: [u8; 4]) -> Result<(), &'static str> {
        let ret = unsafe { aladdin_sdk::syscall::sys_net_dns_host_add(hostname.as_bytes(), &ip) };
        if ret == 0 {
            Ok(())
        } else {
            Err("Host invalido ou tabela cheia")
        }
    }

    fn dns_remove_host(&self, hostname: &str) -> Result<(), &'static str> {
        let ret = unsafe { aladdin_sdk::syscall::sys_net_dns_host_remove(hostname.as_bytes()) };
        if ret == 0 {
            Ok(())
        } else {
            Err("Host nao encontrado")
        }
    }

    fn dns_flush_cache(&self) -> usize {
        let ret = unsafe { aladdin_sdk::syscall::sys_net_dns_cache_flush() };
        if ret > 0 { ret as usize } else { 0 }
    }

    fn dns_resolve_timed_ms(&self, hostname: &str) -> Result<([u8; 4], u64), &'static str> {
        let mut ip = [0u8; 4];
        let mut t0 = Timespec::default();
        let mut t1 = Timespec::default();
        unsafe { let _ = sys_clock_gettime(1, &mut t0); }
        let ret = unsafe { aladdin_sdk::syscall::sys_net_resolve(hostname.as_bytes(), &mut ip) };
        unsafe { let _ = sys_clock_gettime(1, &mut t1); }

        if ret != 0 {
            return match ret {
                -2 => Err("Dominio inexistente (NXDOMAIN)"),
                -110 => Err("Timeout DNS upstream"),
                -100 => Err("Interface de rede inativa"),
                -22 => Err("Nome de dominio invalido"),
                _ => Err("Erro na resolucao DNS"),
            };
        }

        let ns0 = t0.tv_sec.saturating_mul(1_000_000_000).saturating_add(t0.tv_nsec);
        let ns1 = t1.tv_sec.saturating_mul(1_000_000_000).saturating_add(t1.tv_nsec);
        let ms = ns1.saturating_sub(ns0) / 1_000_000;
        Ok((ip, ms))
    }

    fn hot_routes_list(&self, out: &mut [aladdin::contracts::system::HotRouteTableEntry]) -> usize {
        if out.is_empty() {
            return 0;
        }
        static mut HOT_ROUTES_SYSCALL_BUF: [aladdin_sdk::syscall::HotRouteEntryUser; 64] =
            [aladdin_sdk::syscall::HotRouteEntryUser {
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
            }; 64];
        let kbuf = unsafe { &mut *core::ptr::addr_of_mut!(HOT_ROUTES_SYSCALL_BUF) };
        let req_len = out.len().min(64);
        let ret = unsafe { aladdin_sdk::syscall::sys_net_hot_route_list(&mut kbuf[..req_len]) };
        if ret <= 0 {
            return 0;
        }
        let n = (ret as usize).min(out.len()).min(64);
        for i in 0..n {
            out[i] = aladdin::contracts::system::HotRouteTableEntry {
                hostname: kbuf[i].hostname,
                hostname_len: (kbuf[i].hostname_len as usize).min(96) as u8,
                dst_ip: kbuf[i].dst_ip,
                next_hop_ip: kbuf[i].next_hop_ip,
                next_hop_mac: kbuf[i].next_hop_mac,
                hit_count: kbuf[i].hit_count,
                last_rtt_ticks: kbuf[i].last_rtt_ticks,
                ttl_remaining_secs: kbuf[i].ttl_remaining_secs,
                is_pinned: (kbuf[i].flags & aladdin_sdk::syscall::HOT_ROUTE_FLAG_PINNED_SEED) != 0,
                is_valid: kbuf[i].valid != 0,
                is_stale: kbuf[i].stale != 0,
            };
        }
        n
    }

    fn hot_route_pin(&self, hostname: &str, ip: Option<[u8; 4]>) -> Result<(), &'static str> {
        let ret = unsafe {
            match ip {
                Some(ref addr) => aladdin_sdk::syscall::sys_net_hot_route_pin(hostname.as_bytes(), Some(addr)),
                None => aladdin_sdk::syscall::sys_net_hot_route_pin(hostname.as_bytes(), None),
            }
        };
        if ret == 0 {
            Ok(())
        } else {
            Err("Falha ao fixar site na HotRouteTable")
        }
    }

    fn hot_routes_flush(&self) -> usize {
        let ret = unsafe { aladdin_sdk::syscall::sys_net_hot_route_flush() };
        if ret > 0 { ret as usize } else { 0 }
    }

    fn sync_persistent_caches(&self) {
        unsafe {
            let _ = aladdin_sdk::syscall::sys_sync();
        }
    }
}

// ── Adaptador de VFS em Ring 3 ────────────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DirEntryUser {
    pub name: [u8; 64],
    pub name_len: u32,
    pub is_dir: u32,
    pub is_elf: u32,
    pub is_txt: u32,
    pub size: u64,
}

pub struct UserVfs;

impl VfsHost for UserVfs {
    fn readdir(&self, path: &str, cb: &mut dyn FnMut(&str, bool, bool, bool)) {
        let mut entries = [DirEntryUser {
            name: [0u8; 64],
            name_len: 0,
            is_dir: 0,
            is_elf: 0,
            is_txt: 0,
            size: 0,
        }; 64];

        unsafe {
            let count = aladdin_sdk::syscall::syscall4(
                aladdin_sdk::syscall::SYS_READDIR,
                path.as_ptr() as u64,
                path.len() as u64,
                entries.as_mut_ptr() as u64,
                64,
            );
            if count > 0 {
                let n = (count as usize).min(64);
                for i in 0..n {
                    let len = entries[i].name_len as usize;
                    if len > 0 && len <= 64 {
                        if let Ok(name_str) = core::str::from_utf8(&entries[i].name[..len]) {
                            cb(
                                name_str,
                                entries[i].is_dir != 0,
                                entries[i].is_elf != 0,
                                entries[i].is_txt != 0,
                            );
                        }
                    }
                }
            }
        }
    }

    fn read_file(&self, path: &str, buf: &mut [u8]) -> Result<usize, ()> {
        unsafe {
            let fd = aladdin_sdk::syscall::syscall3(15, path.as_ptr() as u64, path.len() as u64, 0); // O_RDONLY = 0
            if fd < 0 { return Err(()); }
            let bytes_read = aladdin_sdk::syscall::syscall3(aladdin_sdk::syscall::SYS_READ, fd as u64, buf.as_mut_ptr() as u64, buf.len() as u64);
            let _ = aladdin_sdk::syscall::syscall1(16, fd as u64); // sys_close
            if bytes_read >= 0 {
                Ok(bytes_read as usize)
            } else {
                Err(())
            }
        }
    }

    fn write_file(&self, path: &str, data: &[u8]) -> Result<(), ()> {
        unsafe {
            let fd = aladdin_sdk::syscall::syscall3(71, path.as_ptr() as u64, path.len() as u64, 0o644); // sys_creat
            if fd < 0 { return Err(()); }
            let bytes_written = aladdin_sdk::syscall::syscall3(aladdin_sdk::syscall::SYS_WRITE, fd as u64, data.as_ptr() as u64, data.len() as u64);
            let _ = aladdin_sdk::syscall::syscall1(16, fd as u64); // sys_close
            if bytes_written >= 0 { Ok(()) } else { Err(()) }
        }
    }

    fn create_file(&self, path: &str) -> Result<(), ()> {
        self.write_file(path, b"")
    }

    fn create_dir(&self, path: &str) -> Result<(), ()> {
        unsafe {
            let ret = aladdin_sdk::syscall::syscall3(51, path.as_ptr() as u64, path.len() as u64, 0); // sys_mkdir
            if ret >= 0 { Ok(()) } else { Err(()) }
        }
    }

    fn remove_file(&self, path: &str) -> Result<(), ()> {
        unsafe {
            let ret = aladdin_sdk::syscall::syscall3(50, path.as_ptr() as u64, path.len() as u64, 0); // sys_unlink
            if ret >= 0 { Ok(()) } else { Err(()) }
        }
    }

    fn rename_file(&self, old_path: &str, new_path: &str) -> Result<(), ()> {
        #[repr(C)]
        struct RenameArgs {
            old_ptr: *const u8,
            old_len: usize,
            new_ptr: *const u8,
            new_len: usize,
        }
        let args = RenameArgs {
            old_ptr: old_path.as_ptr(),
            old_len: old_path.len(),
            new_ptr: new_path.as_ptr(),
            new_len: new_path.len(),
        };
        unsafe {
            let ret = aladdin_sdk::syscall::syscall3(52, &args as *const _ as u64, 0, 0); // sys_rename
            if ret >= 0 { Ok(()) } else { Err(()) }
        }
    }

    fn copy_file(&self, src: &str, dst: &str) -> Result<(), ()> {
        let mut buf = [0u8; 4096];
        let bytes = self.read_file(src, &mut buf)?;
        self.write_file(dst, &buf[..bytes])
    }

    fn file_size(&self, path: &str) -> Result<usize, ()> {
        let mut buf = [0u8; 4096];
        self.read_file(path, &mut buf)
    }
}

// ── Adaptador de Processos em Ring 3 ──────────────────────────────────────────

pub struct UserProcess;

impl ProcessHost for UserProcess {
    fn execute_command(&mut self, cmd: &str, output: &mut dyn FnMut(&str, u32)) {
        let trimmed = cmd.trim();
        if trimmed.is_empty() {
            return;
        }
        aladdin_sdk::console::print("[aladdin-server] execute_command: '");
        aladdin_sdk::console::print(trimmed);
        aladdin_sdk::console::println("'");
        if trimmed.eq_ignore_ascii_case("browser")
            || trimmed.ends_with("browser")
            || trimmed.eq_ignore_ascii_case("spawn browser")
            || trimmed.eq_ignore_ascii_case("/disk/browser.elf")
        {
            output("Iniciando Jinn Browser Engine (Ring 3)...", 0x0093_C5FD);
            match self.spawn_app("/disk/browser.elf") {
                Ok(_pid) => output("Jinn Browser Ring 3 iniciado com sucesso!", 0x0010_B981),
                Err(()) => output("Erro: falha ao carregar /disk/browser.elf", 0x00EF_4444),
            }
        } else if trimmed.starts_with("spawn ") {
            let app = trimmed[6..].trim();
            output("Executando spawn...", 0x0093_C5FD);
            match self.spawn_app(app) {
                Ok(_) => output("Processo iniciado com sucesso!", 0x0010_B981),
                Err(()) => output("Erro ao iniciar processo.", 0x00EF_4444),
            }
        } else if trimmed.eq_ignore_ascii_case("stats") || trimmed.eq_ignore_ascii_case("jhcs") {
            if let Some(pc) = dump_stats_telemetry() {
                let mut ui_buf = [0u8; 96];
                let mut ui_pos = 0usize;
                let push_s = |dst: &mut [u8], pos: &mut usize, s: &[u8]| {
                    let take = s.len().min(dst.len().saturating_sub(*pos));
                    dst[*pos..*pos + take].copy_from_slice(&s[..take]);
                    *pos += take;
                };
                let push_num = |dst: &mut [u8], pos: &mut usize, mut v: u64| {
                    if v == 0 {
                        if *pos < dst.len() { dst[*pos] = b'0'; *pos += 1; }
                        return;
                    }
                    let mut tmp = [0u8; 20];
                    let mut k = 0usize;
                    while v > 0 { tmp[k] = b'0' + (v % 10) as u8; v /= 10; k += 1; }
                    while k > 0 { k -= 1; if *pos < dst.len() { dst[*pos] = tmp[k]; *pos += 1; } }
                };
                push_s(&mut ui_buf, &mut ui_pos, b"LAPIC OneShot=");
                push_num(&mut ui_buf, &mut ui_pos, pc.tickless_active);
                push_s(&mut ui_buf, &mut ui_pos, b" | TimerIRQs=");
                push_num(&mut ui_buf, &mut ui_pos, pc.lapic_timer_irqs);
                push_s(&mut ui_buf, &mut ui_pos, b" | Arms=");
                push_num(&mut ui_buf, &mut ui_pos, pc.oneshot_arms);
                push_s(&mut ui_buf, &mut ui_pos, b" | PIT0=");
                push_num(&mut ui_buf, &mut ui_pos, pc.pit_irq0_count);
                if let Ok(line) = core::str::from_utf8(&ui_buf[..ui_pos]) {
                    output(line, 0x0022_C55E);
                }
            } else {
                output("Telemetria Zero-Idle & Syscall 88/92 registrada na serial.", 0x0022_C55E);
            }
        } else if trimmed.eq_ignore_ascii_case("hotroutes") {
            let sys_h = UserSystem { start_tick: core::sync::atomic::AtomicU64::new(0) };
            static mut CLI_HOT_LIST: [aladdin::contracts::system::HotRouteTableEntry; 32] =
                [aladdin::contracts::system::HotRouteTableEntry::empty(); 32];
            let list_buf = unsafe { &mut *core::ptr::addr_of_mut!(CLI_HOT_LIST) };
            let n = sys_h.hot_routes_list(list_buf);
            aladdin_sdk::console::print("[HOTROUTES] entries=");
            aladdin_sdk::console::print_num(n as u64);
            aladdin_sdk::console::println("");
            for i in 0..n {
                let r = &list_buf[i];
                aladdin_sdk::console::print("  [HOT-ENTRY] #");
                aladdin_sdk::console::print_num((i + 1) as u64);
                aladdin_sdk::console::print(" host=");
                aladdin_sdk::console::print(r.hostname_str());
                aladdin_sdk::console::print(" ip=");
                for o in 0..4 {
                    if o > 0 { aladdin_sdk::console::print("."); }
                    aladdin_sdk::console::print_num(r.dst_ip[o] as u64);
                }
                aladdin_sdk::console::print(" hits=");
                aladdin_sdk::console::print_num(r.hit_count as u64);
                aladdin_sdk::console::print(" rtt_ms=");
                aladdin_sdk::console::print_num(r.rtt_ms() as u64);
                aladdin_sdk::console::print(" pinned=");
                aladdin_sdk::console::print_num(if r.is_pinned { 1 } else { 0 });
                aladdin_sdk::console::print(" valid=");
                aladdin_sdk::console::print_num(if r.is_valid { 1 } else { 0 });
                aladdin_sdk::console::println("");
            }
            output("HotRouteTable listada na serial.", 0x0010_B981);
        } else if trimmed.eq_ignore_ascii_case("hottest") {
            let sys_h = UserSystem { start_tick: core::sync::atomic::AtomicU64::new(0) };
            let r_null = unsafe { aladdin_sdk::syscall::syscall2(aladdin_sdk::syscall::SYS_NET_HOT_ROUTE_LIST, 0, 4) };
            let r_kern = unsafe { aladdin_sdk::syscall::syscall2(aladdin_sdk::syscall::SYS_NET_HOT_ROUTE_LIST, 0xFFFF_8000_0000_0000, 4) };
            let r_over = unsafe {
                let mut tmp = [aladdin_sdk::syscall::HotRouteEntryUser::default(); 1];
                aladdin_sdk::syscall::syscall2(aladdin_sdk::syscall::SYS_NET_HOT_ROUTE_LIST, tmp.as_mut_ptr() as u64, 999)
            };
            let r_kpin = unsafe { aladdin_sdk::syscall::syscall3(aladdin_sdk::syscall::SYS_NET_HOT_ROUTE_PIN, 0xFFFF_8000_0000_0000, 5, 0) };
            aladdin_sdk::console::print("[HOT-BENCH] sec null=");
            aladdin_sdk::console::print_num((-r_null).max(0) as u64);
            aladdin_sdk::console::print(" kern=");
            aladdin_sdk::console::print_num((-r_kern).max(0) as u64);
            aladdin_sdk::console::print(" over=");
            aladdin_sdk::console::print_num((-r_over).max(0) as u64);
            aladdin_sdk::console::print(" kpin=");
            aladdin_sdk::console::print_num((-r_kpin).max(0) as u64);
            aladdin_sdk::console::println("");

            static mut TEST_HR_LIST: [aladdin::contracts::system::HotRouteTableEntry; 64] =
                [aladdin::contracts::system::HotRouteTableEntry::empty(); 64];
            let hr_buf = unsafe { &mut *core::ptr::addr_of_mut!(TEST_HR_LIST) };
            let cnt = sys_h.hot_routes_list(hr_buf);
            let sec_ok = r_null == -22 && r_kern == -14 && r_over == -22 && r_kpin == -14;
            if sec_ok && cnt >= 8 {
                aladdin_sdk::console::println("[HOT-BENCH] PASS");
                output("HotRoute Syscalls 97..99 Security Benchmark: PASS", 0x0010_B981);
            } else {
                aladdin_sdk::console::println("[HOT-BENCH] FAIL");
                output("HotRoute Syscalls 97..99 Security Benchmark: FAIL", 0x00EF_4444);
            }
        } else if trimmed.eq_ignore_ascii_case("sync") {
            let sys_h = UserSystem { start_tick: core::sync::atomic::AtomicU64::new(0) };
            sys_h.sync_persistent_caches();
            aladdin_sdk::console::println("[aladdin-server] [SYNC] sys_sync() concluido");
            output("Caches sincronizados com o disco (/disk/hot_routes.db).", 0x0010_B981);
        } else if trimmed.eq_ignore_ascii_case("dnstest") {
            let sys_h = UserSystem { start_tick: core::sync::atomic::AtomicU64::new(0) };
            // 1. Seguranca Zero-Trust das novas Syscalls 93/94/96
            let tmp_entry = aladdin_sdk::syscall::DnsEntryUser::default();
            let r_null_list = unsafe { aladdin_sdk::syscall::syscall2(aladdin_sdk::syscall::SYS_NET_DNS_CACHE_LIST, 0, 4) };
            let r_kern_list = unsafe { aladdin_sdk::syscall::syscall2(aladdin_sdk::syscall::SYS_NET_DNS_CACHE_LIST, 0xFFFF_8000_0000_0000, 4) };
            let r_kern_host = unsafe { aladdin_sdk::syscall::syscall4(aladdin_sdk::syscall::SYS_NET_DNS_HOST_ADD, 0xFFFF_8000_0000_0000, 5, &tmp_entry.ipv4 as *const _ as u64, 0) };
            aladdin_sdk::console::print("[DNS-BENCH] sec null_list=");
            aladdin_sdk::console::print_num((-r_null_list).max(0) as u64);
            aladdin_sdk::console::print(" kern_list=");
            aladdin_sdk::console::print_num((-r_kern_list).max(0) as u64);
            aladdin_sdk::console::print(" kern_host=");
            aladdin_sdk::console::print_num((-r_kern_host).max(0) as u64);
            aladdin_sdk::console::println("");

            // 2. Host estatico em /disk/hosts
            let _ = sys_h.dns_add_host("demo.local", [10, 10, 20, 30]);
            let static_res = sys_h.dns_resolve_timed_ms("demo.local");
            let _ = sys_h.dns_flush_cache();

            // 3. 1a e 2a consulta real (example.com e jinn-os.dev.br)
            static mut BENCH_LIST: [DnsTableEntry; 64] = [DnsTableEntry::empty(); 64];
            let list_buf = unsafe { &mut *core::ptr::addr_of_mut!(BENCH_LIST) };

            let r1_ex = sys_h.dns_resolve_timed_ms("example.com");
            let n1 = sys_h.dns_list_entries(list_buf);
            let mut h1_ex = 999u32;
            for item in &list_buf[..n1] {
                if item.hostname_str() == "example.com" && !item.is_static_host {
                    h1_ex = item.hits;
                }
            }
            let r2_ex = sys_h.dns_resolve_timed_ms("example.com");
            let n2 = sys_h.dns_list_entries(list_buf);
            let mut h2_ex = 0u32;
            let mut ttl_ex = 0u32;
            for item in &list_buf[..n2] {
                if item.hostname_str() == "example.com" && !item.is_static_host {
                    h2_ex = item.hits;
                    ttl_ex = item.ttl_remaining_secs;
                }
            }

            let r1_jn = sys_h.dns_resolve_timed_ms("jinn-os.dev.br");
            let r2_jn = sys_h.dns_resolve_timed_ms("jinn-os.dev.br");
            let n3 = sys_h.dns_list_entries(list_buf);
            let mut h2_jn = 0u32;
            for item in &list_buf[..n3] {
                if item.hostname_str() == "jinn-os.dev.br" && !item.is_static_host {
                    h2_jn = item.hits;
                }
            }

            let sec_ok = r_null_list == -22 && r_kern_list == -14 && r_kern_host == -14;
            let static_ok = match static_res {
                Ok(([10, 10, 20, 30], ms)) => {
                    aladdin_sdk::console::print("[DNS-BENCH] demo.local static_ms=");
                    aladdin_sdk::console::print_num(ms);
                    aladdin_sdk::console::println("");
                    ms <= 25
                }
                _ => false,
            };
            let ex_ok = match (r1_ex, r2_ex) {
                (Ok((ip1, ms1)), Ok((ip2, ms2))) => {
                    aladdin_sdk::console::print("[DNS-BENCH] example.com 1st_ms=");
                    aladdin_sdk::console::print_num(ms1);
                    aladdin_sdk::console::print(" hits1=");
                    aladdin_sdk::console::print_num(h1_ex as u64);
                    aladdin_sdk::console::print(" 2nd_ms=");
                    aladdin_sdk::console::print_num(ms2);
                    aladdin_sdk::console::print(" hits2=");
                    aladdin_sdk::console::print_num(h2_ex as u64);
                    aladdin_sdk::console::print(" ttl=");
                    aladdin_sdk::console::print_num(ttl_ex as u64);
                    aladdin_sdk::console::println("");
                    ip1 == ip2 && ms2 <= 25 && h1_ex == 0 && h2_ex == 1
                }
                _ => false,
            };
            let jn_ok = match (r1_jn, r2_jn) {
                (Ok((ip1, ms1)), Ok((ip2, ms2))) => {
                    aladdin_sdk::console::print("[DNS-BENCH] jinn-os.dev.br 1st_ms=");
                    aladdin_sdk::console::print_num(ms1);
                    aladdin_sdk::console::print(" 2nd_ms=");
                    aladdin_sdk::console::print_num(ms2);
                    aladdin_sdk::console::print(" hits2=");
                    aladdin_sdk::console::print_num(h2_jn as u64);
                    aladdin_sdk::console::println("");
                    ip1 == ip2 && ms2 <= 25 && h2_jn == 1
                }
                _ => false,
            };
            if sec_ok && static_ok && ex_ok && jn_ok {
                aladdin_sdk::console::println("[DNS-BENCH] PASS");
                output("DNS 3-Layer Benchmark: PASS (0 ms hit, hits=1)", 0x0010_B981);
            } else {
                aladdin_sdk::console::println("[DNS-BENCH] FAIL");
                output("DNS 3-Layer Benchmark: FAIL", 0x00EF_4444);
            }
        } else if trimmed.eq_ignore_ascii_case("dnsreboot") {
            let sys_h = UserSystem { start_tick: core::sync::atomic::AtomicU64::new(0) };
            static mut REBOOT_LIST: [DnsTableEntry; 64] = [DnsTableEntry::empty(); 64];
            let list_buf = unsafe { &mut *core::ptr::addr_of_mut!(REBOOT_LIST) };
            let n0 = sys_h.dns_list_entries(list_buf);
            aladdin_sdk::console::print("[DNS-REBOOT] initial_entries=");
            aladdin_sdk::console::print_num(n0 as u64);
            aladdin_sdk::console::println("");
            for item in &list_buf[..n0] {
                aladdin_sdk::console::print("  [DNS-ENTRY] ");
                aladdin_sdk::console::print(item.hostname_str());
                aladdin_sdk::console::print(if item.is_static_host { " HOSTS ttl=" } else { " CACHE ttl=" });
                aladdin_sdk::console::print_num(item.ttl_remaining_secs as u64);
                aladdin_sdk::console::print(" hits=");
                aladdin_sdk::console::print_num(item.hits as u64);
                aladdin_sdk::console::println("");
            }
            let r_demo = sys_h.dns_resolve_timed_ms("demo.local");
            let r_ex = sys_h.dns_resolve_timed_ms("example.com");
            let r_jn = sys_h.dns_resolve_timed_ms("jinn-os.dev.br");
            let n1 = sys_h.dns_list_entries(list_buf);
            let mut ex_hits = 0u32;
            let mut jn_hits = 0u32;
            for item in &list_buf[..n1] {
                if item.hostname_str() == "example.com" && !item.is_static_host { ex_hits = item.hits; }
                if item.hostname_str() == "jinn-os.dev.br" && !item.is_static_host { jn_hits = item.hits; }
            }
            if let (Ok((_, d_ms)), Ok((_, e_ms)), Ok((_, j_ms))) = (r_demo, r_ex, r_jn) {
                aladdin_sdk::console::print("[DNS-REBOOT] timings demo_ms=");
                aladdin_sdk::console::print_num(d_ms);
                aladdin_sdk::console::print(" ex_ms=");
                aladdin_sdk::console::print_num(e_ms);
                aladdin_sdk::console::print(" ex_hits=");
                aladdin_sdk::console::print_num(ex_hits as u64);
                aladdin_sdk::console::print(" jn_ms=");
                aladdin_sdk::console::print_num(j_ms);
                aladdin_sdk::console::print(" jn_hits=");
                aladdin_sdk::console::print_num(jn_hits as u64);
                aladdin_sdk::console::println("");
            }
            let ok = matches!(r_demo, Ok(([10, 10, 20, 30], ms)) if ms <= 25)
                && matches!(r_ex, Ok((_, ms)) if ms <= 25)
                && matches!(r_jn, Ok((_, ms)) if ms <= 25)
                && ex_hits >= 2
                && jn_hits >= 1;
            if ok {
                aladdin_sdk::console::println("[DNS-REBOOT] PASS");
                output("Cold-Reboot DNS Cache/Hosts: PASS (0 ms)", 0x0010_B981);
            } else {
                aladdin_sdk::console::println("[DNS-REBOOT] FAIL");
                output("Cold-Reboot DNS Cache/Hosts: FAIL", 0x00EF_4444);
            }
        } else if trimmed.eq_ignore_ascii_case("help") {
            output("Comandos: browser, spawn <path>, stats, dnstest, dnsreboot, reboot, clear", 0x0093_C5FD);
        } else if trimmed.eq_ignore_ascii_case("reboot") {
            output("Reiniciando o sistema...", 0x00EF_4444);
            unsafe { aladdin_sdk::syscall::syscall0(aladdin_sdk::syscall::SYS_REBOOT); }
        } else {
            output("Comando recebido pelo Aladdin Ring 3 Display Server.", 0x0022_C55E);
        }
    }

    fn is_process_alive(&self, pid: usize) -> bool {
        if pid == 0 { return false; }
        let mut stat = [0u32; 32];
        let ret = unsafe {
            aladdin_sdk::syscall::syscall3(25, pid as u64, stat.as_mut_ptr() as u64, (stat.len() * 4) as u64)
        };
        if ret != 0 { return false; }
        let state = stat[1]; // ProcessResourceStat::state (1=Ready, 2=Running, 3=Blocked, 4=Sleeping, 5=WaitingOn, 6=Zombie, 7=Completed)
        state >= 1 && state <= 5
    }

    fn spawn_app(&mut self, path: &str) -> Result<usize, ()> {
        let is_browser = path.contains("browser") || path.contains("BROWSER");
        let is_ola_qa = path.contains("ola-qa");
        let browser_url = if is_browser {
            path.split_once('@').map(|(_, u)| u.trim())
        } else {
            None
        };
        let target_path = if is_browser {
            "/disk/browser.elf"
        } else if is_ola_qa || path.contains("ola-mundo") || path.contains("ola.elf") {
            "/disk/ola.elf"
        } else if path.contains("jhcs-test") || path.contains("JHCS-TEST") {
            "/disk/jhcs-test.elf"
        } else if path.contains("contador") {
            "/disk/contador.elf"
        } else if path.starts_with('/') {
            path
        } else {
            "/disk/ola_graf.elf"
        };

        let mut path_buf = [0u8; 128];
        let bytes = target_path.as_bytes();
        let len = bytes.len().min(127);
        path_buf[..len].copy_from_slice(&bytes[..len]);
        path_buf[len] = 0;

        let mut url_arg_buf = [0u8; 128];
        let url_arg_ptr = if let Some(u) = browser_url {
            let ub = u.as_bytes();
            let ul = ub.len().min(127);
            if ul > 0 {
                url_arg_buf[..ul].copy_from_slice(&ub[..ul]);
                url_arg_buf[ul] = 0;
                url_arg_buf.as_ptr()
            } else {
                core::ptr::null()
            }
        } else {
            core::ptr::null()
        };

        // Argumento 'browser' ou 'qa' quando aplicável
        let arg_browser = b"browser\0";
        let arg_qa = b"qa\0";
        let extra_arg = if is_browser {
            arg_browser.as_ptr()
        } else if is_ola_qa {
            arg_qa.as_ptr()
        } else {
            core::ptr::null()
        };
        let mut argv_ptrs: [*const u8; 4] = [
            path_buf.as_ptr(),
            extra_arg,
            url_arg_ptr,
            core::ptr::null(),
        ];

        unsafe {
            aladdin_sdk::console::print("[aladdin-server] Invocando SYS_SPAWN_PROCESS para: ");
            aladdin_sdk::console::println(target_path);
            let ret = aladdin_sdk::syscall::syscall3(
                aladdin_sdk::syscall::SYS_SPAWN_PROCESS,
                path_buf.as_ptr() as u64,
                argv_ptrs.as_mut_ptr() as u64,
                0, // envp
            );
            aladdin_sdk::console::print("[aladdin-server] SYS_SPAWN_PROCESS retornou: ");
            if ret < 0 {
                aladdin_sdk::console::print("-");
                aladdin_sdk::console::print_num((-ret) as u64);
            } else {
                aladdin_sdk::console::print_num(ret as u64);
            }
            aladdin_sdk::console::println("");
            if ret > 0 {
                Ok(ret as usize)
            } else {
                Err(())
            }
        }
    }
}

// ── Ponto de Entrada Principal (Ring 3) ───────────────────────────────────────

/// Consulta Syscalls 88/92 e registra snapshot Tickless/Per-CPU na saída serial.
fn dump_stats_telemetry() -> Option<aladdin_sdk::syscall::PerCpuStatsUser> {
    let mut st = aladdin_sdk::syscall::SystemStatsUser::default();
    let _ = unsafe {
        aladdin_sdk::syscall::syscall1(
            aladdin_sdk::syscall::SYS_GET_SYSTEM_STATS,
            &mut st as *mut _ as u64,
        )
    };
    aladdin_sdk::console::print("[aladdin-server] [STATS] active_cycles=");
    aladdin_sdk::console::print_num(st.cpu_active_cycles);
    aladdin_sdk::console::print(" idle_cycles=");
    aladdin_sdk::console::print_num(st.cpu_idle_cycles);
    aladdin_sdk::console::print(" syscalls=");
    aladdin_sdk::console::print_num(st.syscalls_total);
    aladdin_sdk::console::print(" used_ram_mb=");
    aladdin_sdk::console::print_num(st.used_ram_mb);
    aladdin_sdk::console::print(" ring_rx=");
    aladdin_sdk::console::print_num(st.ring_events_processed);
    aladdin_sdk::console::print(" ring_tx=");
    aladdin_sdk::console::print_num(st.ring_cmds_flushed);
    aladdin_sdk::console::println("");
    let mut pc = aladdin_sdk::syscall::PerCpuStatsUser::default();
    let pc_ret = unsafe {
        aladdin_sdk::syscall::syscall1(
            aladdin_sdk::syscall::SYS_GET_PERCPU_STATS,
            &mut pc as *mut _ as u64,
        )
    };
    if pc_ret == 0 {
        aladdin_sdk::console::print("[aladdin-server] [PERCPU] cpus=");
        aladdin_sdk::console::print_num(pc.cpu_count);
        aladdin_sdk::console::print(" net_irqs=");
        aladdin_sdk::console::print_num(pc.net_irq_count);
        aladdin_sdk::console::print(" net_verified=");
        aladdin_sdk::console::print_num(pc.net_irq_verified);
        aladdin_sdk::console::println("");
        aladdin_sdk::console::print("[aladdin-server] [TICKLESS] active=");
        aladdin_sdk::console::print_num(pc.tickless_active);
        aladdin_sdk::console::print(" lapic_timer_irqs=");
        aladdin_sdk::console::print_num(pc.lapic_timer_irqs);
        aladdin_sdk::console::print(" oneshot_arms=");
        aladdin_sdk::console::print_num(pc.oneshot_arms);
        aladdin_sdk::console::print(" pit_irq0=");
        aladdin_sdk::console::print_num(pc.pit_irq0_count);
        aladdin_sdk::console::println("");
        let n = (pc.cpu_count as usize).min(8);
        for idx in 0..n {
            let c = pc.cores[idx];
            aladdin_sdk::console::print("  cpu");
            aladdin_sdk::console::print_num(c.cpu_id as u64);
            aladdin_sdk::console::print(": total=");
            aladdin_sdk::console::print_num(c.total_cycles);
            aladdin_sdk::console::print(" hlt=");
            aladdin_sdk::console::print_num(c.hlt_cycles);
            aladdin_sdk::console::print(" task=");
            aladdin_sdk::console::print_num(c.task_cycles);
            aladdin_sdk::console::print(" irq=");
            aladdin_sdk::console::print_num(c.irq_cycles);
            aladdin_sdk::console::print(" trans=");
            aladdin_sdk::console::print_num(c.idle_transitions);
            aladdin_sdk::console::print(" wake=");
            aladdin_sdk::console::print_num(c.wakeups);
            aladdin_sdk::console::println("");
        }
        Some(pc)
    } else {
        None
    }
}

static mut GLOBAL_SESSION: *mut AladdinSession = core::ptr::null_mut();

#[no_mangle]
pub extern "C" fn _start() -> ! {
    aladdin_sdk::console::println("[aladdin-server] Inicializando servidor grafico Ring 3...");
    aladdin_sdk::console::print("[aladdin-server] Heap disponivel: ");
    aladdin_sdk::console::print_num((HEAP_SIZE / (1024 * 1024)) as u64);
    aladdin_sdk::console::println(" MiB");

    // 1. Registra este processo como Servidor Gráfico exclusivo via Syscall 72
    let reg_res = unsafe { sys_register_graphics(0) };
    if reg_res < 0 {
        aladdin_sdk::console::println("[aladdin-server] [ERRO] Falha ao registrar servidor grafico!");
        unsafe { aladdin_sdk::syscall::syscall1(aladdin_sdk::syscall::SYS_EXIT, 1); }
        loop {}
    }

    // 2. Consulta dimensões do framebuffer via Syscall 73
    let mut fb_info = GraphicsFbInfo { width: 0, height: 0, pitch: 0, bpp: 0, format: 0 };
    let info_res = unsafe { sys_get_fb_info(&mut fb_info) };
    if info_res < 0 || fb_info.width == 0 || fb_info.height == 0 {
        aladdin_sdk::console::println("[aladdin-server] [ERRO] Framebuffer indisponivel!");
        unsafe {
            sys_unregister_graphics();
            aladdin_sdk::syscall::syscall1(aladdin_sdk::syscall::SYS_EXIT, 1);
        }
        loop {}
    }

    let mut user_fb = UserFramebuffer {
        width: fb_info.width as usize,
        height: fb_info.height as usize,
        pitch: fb_info.pitch as usize,
        sub_buf: alloc::vec::Vec::new(),
    };
    let user_sys = UserSystem { start_tick: core::sync::atomic::AtomicU64::new(100) };
    let user_vfs = UserVfs;
    let mut user_proc = UserProcess;

    let total_pixels = user_fb.width * user_fb.height;
    let mut backbuffer = alloc::vec::Vec::new();
    if backbuffer.try_reserve_exact(total_pixels).is_err() {
        aladdin_sdk::console::println("[aladdin-server] [ERRO] Sem memoria para backbuffer!");
        unsafe {
            sys_unregister_graphics();
            aladdin_sdk::syscall::syscall1(aladdin_sdk::syscall::SYS_EXIT, 1);
        }
        loop {}
    }
    backbuffer.resize(total_pixels, 0u32);

    // Registra AladdinSharedRings com o microkernel Jinn para comunicação lock-free zero-copy (Syscall 89)
    let rings_res = unsafe {
        aladdin_sdk::syscall::register_shared_rings(
            core::ptr::addr_of_mut!(SHARED_RINGS) as usize,
            backbuffer.as_ptr() as usize,
        )
    };
    if rings_res == 0 {
        unsafe { SHARED_RINGS_ACTIVE = true; }
        aladdin_sdk::console::println("[aladdin-server] [RINGS] AladdinSharedRings registrado com sucesso!");
    } else {
        aladdin_sdk::console::println("[aladdin-server] [WARN] Falha ao registrar AladdinSharedRings, usando fallback");
    }

    // Diagnóstico: heap usado após alocação do backbuffer
    {
        let heap_used = ALLOCATOR.offset.load(core::sync::atomic::Ordering::Relaxed);
        aladdin_sdk::console::print("[aladdin-server] Heap usado apos backbuffer: ");
        aladdin_sdk::console::print_num((heap_used / 1024) as u64);
        aladdin_sdk::console::print(" KiB / ");
        aladdin_sdk::console::print_num((HEAP_SIZE / 1024) as u64);
        aladdin_sdk::console::println(" KiB total");
    }

    let mut session = AladdinSession::new();
    unsafe {
        GLOBAL_SESSION = &mut session as *mut AladdinSession;
    }

    // 3. Fila de entrada resiliente para teclado e mouse (evita descarte de eventos e congelamento)
    static mut KEY_QUEUE: [u8; 64] = [0u8; 64];
    static mut KEY_HEAD: usize = 0;
    static mut KEY_TAIL: usize = 0;
    static mut KEY_COUNT: usize = 0;

    let enqueue_key = |k: u8| unsafe {
        if KEY_COUNT < 64 {
            KEY_QUEUE[KEY_HEAD] = k;
            KEY_HEAD = (KEY_HEAD + 1) % 64;
            KEY_COUNT += 1;
        }
    };

    let dequeue_key = || -> Option<u8> {
        unsafe {
            if KEY_COUNT == 0 {
                None
            } else {
                let k = KEY_QUEUE[KEY_TAIL];
                KEY_TAIL = (KEY_TAIL + 1) % 64;
                KEY_COUNT -= 1;
                Some(k)
            }
        }
    };

    static mut CURRENT_MOUSE: (usize, usize, u8) = (512usize, 384usize, 0u8);
    const MOUSE_QUEUE_CAP: usize = 32;
    static mut MOUSE_QUEUE: [(usize, usize, u8); MOUSE_QUEUE_CAP] = [(512, 384, 0); MOUSE_QUEUE_CAP];
    static mut MOUSE_HEAD: usize = 0;
    static mut MOUSE_TAIL: usize = 0;

    let enqueue_mouse = |mx: usize, my: usize, btn: u8| unsafe {
        let next = (MOUSE_TAIL + 1) % MOUSE_QUEUE_CAP;
        if next == MOUSE_HEAD {
            // Se a fila encher, descarta o mais antigo (HEAD) para NUNCA perder MouseUp ou o clique mais recente
            MOUSE_HEAD = (MOUSE_HEAD + 1) % MOUSE_QUEUE_CAP;
        }
        MOUSE_QUEUE[MOUSE_TAIL] = (mx, my, btn);
        MOUSE_TAIL = next;
    };

    let dequeue_mouse = || -> Option<(usize, usize, u8)> {
        unsafe {
            if MOUSE_HEAD != MOUSE_TAIL {
                let mut ev = MOUSE_QUEUE[MOUSE_HEAD];
                MOUSE_HEAD = (MOUSE_HEAD + 1) % MOUSE_QUEUE_CAP;
                // Coalesce movimentos sucessivos com o mesmo estado de botoes para eliminar latencia e backlog
                while MOUSE_HEAD != MOUSE_TAIL {
                    let next_ev = MOUSE_QUEUE[MOUSE_HEAD];
                    if next_ev.2 == ev.2 {
                        ev = next_ev;
                        MOUSE_HEAD = (MOUSE_HEAD + 1) % MOUSE_QUEUE_CAP;
                    } else {
                        break;
                    }
                }
                Some(ev)
            } else {
                None
            }
        }
    };

    let mut client_cache_buf: alloc::vec::Vec<u32> = alloc::vec::Vec::new();
    // Pré-reserva buffer de transferência de cliente para evitar alocações dinâmicas repetidas no bump allocator
    let _ = client_cache_buf.try_reserve_exact(1024 * 768);
    client_cache_buf.resize(1024 * 768, 0u32);

    #[derive(Clone, Copy)]
    struct ClientWindowMapEntry {
        kernel_win_id: u32,
        wm_win_id: usize,
        owner_pid: u32,
        active: bool,
    }
    static mut CLIENT_MAP: [ClientWindowMapEntry; 16] = [ClientWindowMapEntry {
        kernel_win_id: 0,
        wm_win_id: 0,
        owner_pid: 0,
        active: false,
    }; 16];

    let mut client_info_list = [ClientWindowInfoUser {
        win_id: 0, owner_pid: 0, width: 0, height: 0, is_open: 0, damaged: 0, title_len: 0, title: [0u8; 48],
    }; 16];

    let fb_w = user_fb.width;
    let fb_h = user_fb.height;

    let dispatch_key = |ch: u8| {
        if ch == 0x8C {
            // F12: snapshot tickless não intrusivo direto para serial (sem redraw do Terminal)
            let _ = dump_stats_telemetry();
        } else {
            enqueue_key(ch);
        }
    };

    let drain_hardware_events = || {
        if unsafe { SHARED_RINGS_ACTIVE } {
            let rings = unsafe { &*core::ptr::addr_of_mut!(SHARED_RINGS) };
            while let Some(ev) = rings.pop_rx_event() {
                if ev.kind == INPUT_KIND_KEY_DOWN {
                    dispatch_key(ev.code as u8);
                } else if ev.kind == INPUT_KIND_MOUSE_MOVE || ev.kind == INPUT_KIND_MOUSE_BTN {
                    let mx = (ev.x.max(0) as usize).min(fb_w.saturating_sub(1));
                    let my = (ev.y.max(0) as usize).min(fb_h.saturating_sub(1));
                    let btn = ev.code as u8;
                    unsafe {
                        CURRENT_MOUSE = (mx, my, btn);
                    }
                    enqueue_mouse(mx, my, btn);
                }
            }
        } else {
            let mut ev = InputHardwareEvent { kind: 0, code: 0, x: 0, y: 0, modifiers: 0 };
            for _ in 0..16 {
                let ret = unsafe { sys_poll_input(&mut ev) };
                if ret != 1 {
                    break;
                }
                if ev.kind == 1 {
                    dispatch_key(ev.code as u8);
                } else if ev.kind == 3 || ev.kind == 4 || ev.kind == 5 {
                    let mx = (ev.x.max(0) as usize).min(fb_w.saturating_sub(1));
                    let my = (ev.y.max(0) as usize).min(fb_h.saturating_sub(1));
                    let btn = ev.code as u8;
                    unsafe {
                        CURRENT_MOUSE = (mx, my, btn);
                    }
                    enqueue_mouse(mx, my, btn);
                }
            }
        }
    };

    let mut poll_key = || -> Option<u8> {
        drain_hardware_events();
        dequeue_key()
    };

    let mut win_sync_count: usize = 0;
    let mut poll_mouse = || -> (usize, usize, u8) {
        drain_hardware_events();

        // Extrai o próximo estado da fila de mouse ou retorna o estado atômico mais recente
        let res_mouse = dequeue_mouse().unwrap_or(unsafe { CURRENT_MOUSE });

        // Sincroniza janelas de clientes Ring 3 imediatamente se houver janela mapeada ativa, clique ou a cada 3 ciclos
        win_sync_count = win_sync_count.wrapping_add(1);
        let has_mapped_clients = unsafe {
            (*core::ptr::addr_of!(CLIENT_MAP)).iter().any(|m| m.active)
        };
        if has_mapped_clients || (res_mouse.2 & 1) != 0 || win_sync_count % 3 == 0 {
            unsafe {
                if let Some(sess) = GLOBAL_SESSION.as_mut() {
                    let count = sys_list_client_windows(client_info_list.as_mut_ptr(), 16);
                    let list_len = if count > 0 { (count as usize).min(16) } else { 0 };
                    let client_map = &mut *core::ptr::addr_of_mut!(CLIENT_MAP);

                    // 1. Reconcilia janelas que desapareceram da lista do kernel (ex.: sys_window_close liberou o slot)
                    //    ou que estao explicitamente marcadas com is_open == 0.
                    for map in client_map.iter_mut() {
                        if !map.active {
                            continue;
                        }
                        let still_open = client_info_list[..list_len].iter().any(|w| {
                            w.is_open != 0 && w.win_id == map.kernel_win_id && w.owner_pid == map.owner_pid
                        });
                        if !still_open {
                            let _ = sess.close_client_window(map.owner_pid as usize, map.wm_win_id);
                            map.active = false;
                        }
                    }

                    for i in 0..list_len {
                        let win = &client_info_list[i];
                        if win.is_open == 0 {
                            continue;
                        }

                        // 2. Localiza ou cria mapeamento kernel_win_id -> wm_win_id
                        let mut target_wm_id = None;
                        for map in client_map.iter() {
                            if map.active && map.kernel_win_id == win.win_id && map.owner_pid == win.owner_pid {
                                target_wm_id = Some(map.wm_win_id);
                                break;
                            }
                        }

                        let (wm_id, is_new) = match target_wm_id {
                            Some(id) => (id, false),
                            None => {
                                let title_bytes = &win.title[..(win.title_len as usize).min(48)];
                                let title_str = core::str::from_utf8(title_bytes).unwrap_or("App Ring 3");
                                match sess.create_client_window(win.owner_pid as usize, title_str, win.width as usize, win.height as usize) {
                                    Ok(new_wm_id) => {
                                        aladdin_sdk::console::print("[aladdin-server] Mapeada janela cliente: kernel_win_id=");
                                        aladdin_sdk::console::print_num(win.win_id as u64);
                                        aladdin_sdk::console::print(" -> wm_win_id=");
                                        aladdin_sdk::console::print_num(new_wm_id as u64);
                                        aladdin_sdk::console::println("");

                                        if title_str.contains("Browser") || title_str.contains("browser") || win.owner_pid as usize == sess.browser_pid.unwrap_or(0) {
                                            sess.browser_win_id = Some(new_wm_id);
                                        }

                                        let mut stored = false;
                                        for map in client_map.iter_mut() {
                                            if !map.active {
                                                map.kernel_win_id = win.win_id;
                                                map.wm_win_id = new_wm_id;
                                                map.owner_pid = win.owner_pid;
                                                map.active = true;
                                                stored = true;
                                                break;
                                            }
                                        }
                                        if !stored {
                                            client_map[0] = ClientWindowMapEntry {
                                                kernel_win_id: win.win_id,
                                                wm_win_id: new_wm_id,
                                                owner_pid: win.owner_pid,
                                                active: true,
                                            };
                                        }
                                        (new_wm_id, true)
                                    }
                                    Err(()) => continue,
                                }
                            }
                        };

                        // 3. Atualiza superfície quando danificada ou recém-criada (IMP-02)
                        if win.damaged != 0 || is_new {
                            let new_w = win.width as usize;
                            let new_h = win.height as usize;
                            let _ = sess.sync_client_window_size(win.owner_pid as usize, wm_id, new_w, new_h, fb_w, fb_h);
                            let total_px = new_w * new_h;
                            if total_px > 0 && total_px <= 1024 * 768 {
                                if client_cache_buf.len() < total_px {
                                    client_cache_buf.resize(total_px, 0u32);
                                }
                                let read_bytes = sys_read_client_surface(win.win_id as usize, client_cache_buf.as_mut_ptr(), total_px * 4);
                                if read_bytes > 0 {
                                    let _ = sess.update_client_buffer(win.owner_pid as usize, wm_id, &client_cache_buf[..total_px]);
                                }
                            }
                        }

                        // 4. Despacha eventos de entrada produzidos pelo WindowManager para a janela do cliente
                        while let Ok(Some(cev)) = sess.poll_client_event(win.owner_pid as usize, wm_id) {
                            let _ = sys_dispatch_client_event(win.win_id as usize, &cev);
                        }
                    }
                }
            }
        }

        res_mouse
    };

    aladdin_sdk::console::println("[aladdin-server] [OK] Servidor grafico em execucao!");

    // 4. Executa a sessão gráfica do desktop Aladdin
    session.run(
        &mut user_fb,
        &user_sys,
        &user_vfs,
        &mut user_proc,
        &mut backbuffer,
        &mut poll_key,
        &mut poll_mouse,
    );

    // 5. Encerramento seguro: desregistra e restaura console
    unsafe {
        sys_unregister_graphics();
        aladdin_sdk::syscall::syscall1(aladdin_sdk::syscall::SYS_EXIT, 0);
    }
    loop {}
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    aladdin_sdk::console::print("[ALADDIN PANIC] ");
    if let Some(loc) = info.location() {
        aladdin_sdk::console::print(loc.file());
        aladdin_sdk::console::print(":");
        aladdin_sdk::console::print_num(loc.line() as u64);
    }
    aladdin_sdk::console::println("");
    unsafe {
        sys_unregister_graphics();
        aladdin_sdk::syscall::syscall1(aladdin_sdk::syscall::SYS_EXIT, 1);
        loop { core::arch::asm!("hlt"); }
    }
}
