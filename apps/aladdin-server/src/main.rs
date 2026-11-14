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
use aladdin::contracts::framebuffer::{DirtyRect, FramebufferDevice};
use aladdin::contracts::system::{JinnWallProfile, SystemHost, SystemStats};
use aladdin::contracts::vfs::VfsHost;
use aladdin::contracts::process::ProcessHost;
use aladdin::contracts::input::ClientWindowEvent;
use aladdin::desktop::session::AladdinSession;

// ── Allocator de Heap em Espaço de Usuário (.bss) ─────────────────────────────
// 14 MiB de memória em .bss (0 bytes no arquivo ELF do disco).
// Dimensionado para rodar sob máquinas reais e VMs com 64 MB de RAM total:
// Backbuffer 1024×768 (3 MiB) + sub_buf (3 MiB) + janelas clientes Ring 3 (3 MiB) + folgas (5 MiB).
const HEAP_SIZE: usize = 14 * 1024 * 1024;
static mut HEAP_MEMORY: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];

struct SimpleHeapAllocator {
    offset: core::sync::atomic::AtomicUsize,
    peak:   core::sync::atomic::AtomicUsize,
}

unsafe impl GlobalAlloc for SimpleHeapAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align();
        let size = layout.size();
        loop {
            let current = self.offset.load(Ordering::Relaxed);
            let aligned = (current + align - 1) & !(align - 1);
            let next = aligned + size;

            if next > HEAP_SIZE {
                // OOM: imprime diagnóstico na porta serial via syscall write
                aladdin_sdk::console::print("[aladdin-server] [OOM] Heap esgotado! offset=");
                aladdin_sdk::console::print_num(current as u64);
                aladdin_sdk::console::print(" size=");
                aladdin_sdk::console::print_num(HEAP_SIZE as u64);
                aladdin_sdk::console::print(" pedido=");
                aladdin_sdk::console::print_num(size as u64);
                aladdin_sdk::console::println("");
                return core::ptr::null_mut();
            }

            // CAS atômico: garante corretude mesmo se IRQ preemptar entre load e store
            if self.offset.compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
                // Atualiza pico histórico para diagnóstico
                let mut pk = self.peak.load(Ordering::Relaxed);
                while next > pk {
                    match self.peak.compare_exchange_weak(pk, next, Ordering::Relaxed, Ordering::Relaxed) {
                        Ok(_) => break,
                        Err(p) => pk = p,
                    }
                }
                let base = core::ptr::addr_of_mut!(HEAP_MEMORY) as *mut u8;
                return base.add(aligned);
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator: frees são ignorados (sem fragmentação, sem double-free).
        // Objetos de longa duração (backbuffer, janelas) nunca são liberados em sessão normal.
        // Para reciclagem explícita (ex.: fechar janela do browser), use reset_session().
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

impl SimpleHeapAllocator {
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

#[global_allocator]
static ALLOCATOR: SimpleHeapAllocator = SimpleHeapAllocator {
    offset: core::sync::atomic::AtomicUsize::new(0),
    peak:   core::sync::atomic::AtomicUsize::new(0),
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
            net_ip: [192, 168, 1, 100],
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
        }
    }

    fn yield_cpu(&self) {
        unsafe { aladdin_sdk::syscall::syscall0(aladdin_sdk::syscall::SYS_YIELD); }
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
        unsafe { aladdin_sdk::syscall::syscall1(aladdin_sdk::syscall::SYS_EXIT, 0); }
        loop {}
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
}

// ── Adaptador de VFS em Ring 3 ────────────────────────────────────────────────

pub struct UserVfs;

impl VfsHost for UserVfs {
    fn readdir(&self, _path: &str, cb: &mut dyn FnMut(&str, bool, bool, bool)) {
        cb("ola.elf", false, true, false);
        cb("ola_graf.elf", false, true, false);
        cb("contador.elf", false, true, false);
        cb("browser.elf", false, true, false);
        cb("README.TXT", false, false, true);
        cb("nebula.ppm", false, false, false);
        cb("landscp.ppm", false, false, false);
        cb("cosmos.ppm", false, false, false);
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
        if trimmed.eq_ignore_ascii_case("browser")
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
        } else if trimmed.eq_ignore_ascii_case("help") {
            output("Comandos: browser, spawn <path>, uptime, reboot, clear", 0x0093_C5FD);
        } else if trimmed.eq_ignore_ascii_case("reboot") {
            output("Reiniciando o sistema...", 0x00EF_4444);
            unsafe { aladdin_sdk::syscall::syscall0(aladdin_sdk::syscall::SYS_REBOOT); }
        } else {
            output("Comando recebido pelo Aladdin Ring 3 Display Server.", 0x0022_C55E);
        }
    }

    fn is_process_alive(&self, pid: usize) -> bool {
        if pid == 0 { return false; }
        let mut stat = [0u8; 128];
        let ret = unsafe {
            aladdin_sdk::syscall::syscall3(25, pid as u64, stat.as_mut_ptr() as u64, stat.len() as u64)
        };
        ret == 0
    }

    fn spawn_app(&mut self, path: &str) -> Result<usize, ()> {
        let is_browser = path.contains("browser") || path.contains("BROWSER");
        let target_path = if is_browser {
            "/disk/browser.elf"
        } else if path.contains("contador") {
            "/disk/contador.elf"
        } else {
            "/disk/ola_graf.elf"
        };

        let mut path_buf = [0u8; 128];
        let bytes = target_path.as_bytes();
        let len = bytes.len().min(127);
        path_buf[..len].copy_from_slice(&bytes[..len]);
        path_buf[len] = 0;

        // Argumento 'browser' para que ola-mundo inicie no modo navegador gráfico
        let arg_browser = b"browser\0";
        let mut argv_ptrs: [*const u8; 3] = [
            path_buf.as_ptr(),
            if is_browser { arg_browser.as_ptr() } else { core::ptr::null() },
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
    let mut client_cache_buf: alloc::vec::Vec<u32> = alloc::vec::Vec::new();
    // Pré-reserva buffer de transferência de cliente para evitar alocações dinâmicas repetidas no bump allocator
    let _ = client_cache_buf.try_reserve_exact(1024 * 768);

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

    let drain_hardware_events = || {
        let mut ev = InputHardwareEvent { kind: 0, code: 0, x: 0, y: 0, modifiers: 0 };
        for _ in 0..16 {
            let ret = unsafe { sys_poll_input(&mut ev) };
            if ret != 1 {
                break;
            }
            if ev.kind == 1 {
                enqueue_key(ev.code as u8);
            } else if ev.kind >= 3 {
                let mx = (ev.x.max(0) as usize).min(fb_w.saturating_sub(1));
                let my = (ev.y.max(0) as usize).min(fb_h.saturating_sub(1));
                let btn = ev.code as u8;
                unsafe {
                    CURRENT_MOUSE = (mx, my, btn);
                }
            }
        }
    };

    let mut poll_key = || -> Option<u8> {
        drain_hardware_events();
        dequeue_key()
    };

    let mut poll_mouse = || -> (usize, usize, u8) {
        drain_hardware_events();

        // Sincroniza janelas de clientes Ring 3 ativas
        unsafe {
            if let Some(sess) = GLOBAL_SESSION.as_mut() {
                let count = sys_list_client_windows(client_info_list.as_mut_ptr(), 16);
                let client_map = &mut *core::ptr::addr_of_mut!(CLIENT_MAP);
                if count > 0 {
                    for i in 0..(count as usize).min(16) {
                        let win = &client_info_list[i];

                        // 1. Trata fechamento de janela
                        if win.is_open == 0 {
                            for map in client_map.iter_mut() {
                                if map.active && map.kernel_win_id == win.win_id {
                                    let _ = sess.close_client_window(map.owner_pid as usize, map.wm_win_id);
                                    map.active = false;
                                }
                            }
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

                        let wm_id = match target_wm_id {
                            Some(id) => id,
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
                                        new_wm_id
                                    }
                                    Err(()) => continue,
                                }
                            }
                        };

                        // 3. Atualiza superfície quando danificada
                        if win.damaged == 1 {
                            let total_px = (win.width as usize) * (win.height as usize);
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

        unsafe { CURRENT_MOUSE }
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
