//! Adaptador de Plataforma do Aladdin Desktop para o Jinn OS
//! Conecta o microkernel x86_64 aos contratos da crate externa 'aladdin'.

use crate::limine;
pub use aladdin::contracts::system::{CapabilityMode, JinnWallProfile};
pub use aladdin::contracts::input::{
    ClientWindowEvent, EVENT_KEY_DOWN, EVENT_KEY_UP, EVENT_MOUSE_DOWN, EVENT_MOUSE_MOVE,
    EVENT_MOUSE_UP, EVENT_NONE, EVENT_SCROLL, EVENT_WINDOW_CLOSE,
};
use aladdin::contracts::framebuffer::{DirtyRect, FramebufferDevice};
use aladdin::contracts::system::{SystemHost, SystemStats};
use aladdin::contracts::vfs::VfsHost;
use aladdin::contracts::process::ProcessHost;
use aladdin::desktop::session::AladdinSession;

pub struct JinnFramebuffer {
    pub address: *mut u8,
    pub width: usize,
    pub height: usize,
    pub pitch: usize,
}

impl FramebufferDevice for JinnFramebuffer {
    fn dimensions(&self) -> (usize, usize, usize) {
        (self.width, self.height, self.pitch)
    }

    fn swap_rect(&mut self, rect: &DirtyRect, buffer: &[u32]) {
        let rw = rect.width.min(self.width.saturating_sub(rect.x));
        let rh = rect.height.min(self.height.saturating_sub(rect.y));
        if rw == 0 || rh == 0 { return; }
        let bytes_to_copy = rw * 4;

        for row in 0..rh {
            let src_offset = (rect.y + row) * self.width + rect.x;
            let dst_offset = (rect.y + row) * self.pitch + rect.x * 4;
            unsafe {
                core::ptr::copy_nonoverlapping(
                    buffer.as_ptr().add(src_offset) as *const u8,
                    self.address.add(dst_offset),
                    bytes_to_copy,
                );
            }
        }
    }

    fn swap_full(&mut self, buffer: &[u32]) {
        let bytes_per_row = self.width * 4;
        if self.pitch == bytes_per_row {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    buffer.as_ptr() as *const u8,
                    self.address,
                    self.width * self.height * 4,
                );
            }
        } else {
            for y in 0..self.height {
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        buffer.as_ptr().add(y * self.width) as *const u8,
                        self.address.add(y * self.pitch),
                        bytes_per_row,
                    );
                }
            }
        }
    }
}

pub struct JinnSystem;

impl SystemHost for JinnSystem {
    fn stats(&self) -> SystemStats {
        let ticks = crate::timer::ticks();
        let net_ip = unsafe { crate::net::dhcp::DHCP_CLIENT.ip };
        let jinnwall_active_conns = unsafe { crate::net::jinnwall::JINNWALL.active_connections() };
        let jinnwall_drops = unsafe { crate::net::jinnwall::JINNWALL.drops() };
        let jinnwall_inspected = unsafe { crate::net::jinnwall::JINNWALL.inspected() };
        let jinnwall_accepted = unsafe { crate::net::jinnwall::JINNWALL.accepted() };
        let jinnwall_profile = match unsafe { crate::net::jinnwall::JINNWALL.profile() } {
            crate::net::jinnwall::FirewallProfile::PublicSecure => JinnWallProfile::PublicSecure,
            crate::net::jinnwall::FirewallProfile::Development => JinnWallProfile::Development,
        };

        SystemStats {
            uptime_secs: (ticks / 100) as usize,
            task_count: crate::scheduler::get().task_count(),
            sched_ticks: crate::scheduler::sched_ticks(),
            used_ram_mb: crate::memory::used_bytes() / (1024 * 1024),
            total_ram_mb: crate::memory::total_bytes() / (1024 * 1024),
            current_tick: ticks,
            net_ip,
            jinnwall_active_conns,
            jinnwall_drops,
            jinnwall_inspected,
            jinnwall_accepted,
            jinnwall_profile,
        }
    }

    fn yield_cpu(&self) {
        crate::scheduler::yield_now();
    }

    fn storage_profile_name(&self) -> &'static str {
        crate::drivers::block::get_storage_profile().name()
    }

    fn cycle_storage_profile(&self) {
        let next_prof = match crate::drivers::block::get_storage_profile() {
            crate::drivers::block::StorageProfile::Aladdin => crate::drivers::block::StorageProfile::Simbad,
            crate::drivers::block::StorageProfile::Simbad => crate::drivers::block::StorageProfile::Workstation,
            crate::drivers::block::StorageProfile::Workstation => crate::drivers::block::StorageProfile::TerminalPos,
            crate::drivers::block::StorageProfile::TerminalPos => crate::drivers::block::StorageProfile::Aladdin,
        };
        crate::drivers::block::set_storage_profile(next_prof);
    }

    fn jinnwall_profile(&self) -> JinnWallProfile {
        match unsafe { crate::net::jinnwall::JINNWALL.profile() } {
            crate::net::jinnwall::FirewallProfile::PublicSecure => JinnWallProfile::PublicSecure,
            crate::net::jinnwall::FirewallProfile::Development => JinnWallProfile::Development,
        }
    }

    fn set_jinnwall_profile(&self, profile: JinnWallProfile) {
        unsafe {
            let prof = match profile {
                JinnWallProfile::PublicSecure => crate::net::jinnwall::FirewallProfile::PublicSecure,
                JinnWallProfile::Development => crate::net::jinnwall::FirewallProfile::Development,
            };
            crate::net::jinnwall::JINNWALL.set_profile(prof);
        }
    }

    fn toggle_jinnwall_profile(&self) -> JinnWallProfile {
        unsafe {
            let next = crate::net::jinnwall::JINNWALL.toggle_profile();
            match next {
                crate::net::jinnwall::FirewallProfile::PublicSecure => JinnWallProfile::PublicSecure,
                crate::net::jinnwall::FirewallProfile::Development => JinnWallProfile::Development,
            }
        }
    }

    fn reboot(&self) -> ! {
        unsafe {
            let null_idt: [u8; 10] = [0u8; 10];
            core::arch::asm!("lidt [{}]", "int 3", in(reg) null_idt.as_ptr(), options(nostack));
        }
        loop { core::hint::spin_loop(); }
    }
}

pub struct JinnVfs;

impl VfsHost for JinnVfs {
    fn readdir(&self, path: &str, cb: &mut dyn FnMut(&str, bool, bool, bool)) {
        let _ = crate::vfs::readdir(path, |name, node_type| {
            let is_dir = node_type == crate::vfs::NodeType::Directory;
            let is_elf = name.ends_with(".elf") || name.ends_with(".ELF");
            let is_txt = name.ends_with(".txt") || name.ends_with(".TXT");
            cb(name, is_dir, is_elf, is_txt);
        });
    }

    fn read_file(&self, path: &str, buf: &mut [u8]) -> Result<usize, ()> {
        crate::vfs::read_file(path, buf).map_err(|_| ())
    }

    fn write_file(&self, path: &str, data: &[u8]) -> Result<(), ()> {
        crate::vfs::write_file(path, data).map(|_| ()).map_err(|_| ())
    }

    fn remove_file(&self, path: &str) -> Result<(), ()> {
        crate::vfs::remove(path).map_err(|_| ())
    }
}

pub struct JinnProcess;

impl ProcessHost for JinnProcess {
    fn execute_command(&mut self, cmd: &str, output: &mut dyn FnMut(&str, u32)) {
        crate::shell::execute_string_command(cmd, output);
    }

    fn is_process_alive(&self, pid: usize) -> bool {
        match crate::scheduler::get().task_state(pid) {
            Some(state) => state != crate::scheduler::TaskState::Zombie && state != crate::scheduler::TaskState::Completed,
            None => false,
        }
    }

    fn spawn_app(&mut self, path: &str) -> Result<usize, ()> {
        let is_browser = path.contains("browser") || path.contains("BROWSER");
        let argv: [&str; 2] = [path, if is_browser { "browser" } else { "" }];
        let argv_slice: &[&str] = if is_browser { &argv[..2] } else { &argv[..1] };

        // 1. Tenta carregar do VFS (disco FAT32)
        match crate::scheduler::get().spawn_user_task_from_vfs(path, argv_slice, &[]) {
            Ok(pid) => {
                crate::ipc::register(pid);
                if is_browser {
                    crate::gui::register_browser_pid(pid);
                }
                return Ok(pid);
            }
            Err(errno) => {
                // Se a falha foi de formato/integridade/segurança (ex: ENOEXEC = -8, EIO = -5),
                // NÃO podemos mascarar a corrupção caindo silenciosamente no fallback!
                if errno != -2 /* ENOENT */ {
                    return Err(());
                }
            }
        }

        // 2. Tenta apenas variantes de caixa no FAT32 se o arquivo primário não foi encontrado (-ENOENT)
        let alt_path = if is_browser {
            if path.contains("BROWSER") { "/disk/browser.elf" } else { "/disk/BROWSER.ELF" }
        } else if path.contains("OLA") || path.contains("ola") {
            if path.contains("OLA") { "/disk/ola.elf" } else { "/disk/OLA.ELF" }
        } else {
            ""
        };

        if !alt_path.is_empty() && alt_path != path {
            let alt_argv: [&str; 2] = [alt_path, if is_browser { "browser" } else { "" }];
            let alt_slice = if is_browser { &alt_argv[..2] } else { &alt_argv[..1] };
            match crate::scheduler::get().spawn_user_task_from_vfs(alt_path, alt_slice, &[]) {
                Ok(pid) => {
                    crate::ipc::register(pid);
                    if is_browser {
                        crate::gui::register_browser_pid(pid);
                    }
                    return Ok(pid);
                }
                Err(errno) => {
                    if errno != -2 /* ENOENT */ {
                        return Err(());
                    }
                }
            }
        }

        // 3. Fallback resiliente: se e somente se o disco FAT32 não estiver montado ou o arquivo
        // estiver ausente (-ENOENT), busca o executável estritamente correspondente nos módulos
        // Limine em memória (procura por nome/caminho 'browser', jamais carregando executáveis alheios).
        if is_browser {
            let mod_opt = limine::get_module_by_name("browser").or_else(|| limine::get_module(1));
            if let Some(elf_bytes) = mod_opt {
                // Valida estritamente a integridade do módulo ELF antes de tentar inicializar
                if crate::elf::validate_elf(elf_bytes).is_ok() {
                    if let Ok(pid) = crate::scheduler::get().spawn_user_task_from_memory(
                        "browser[U3]",
                        elf_bytes,
                        &["/disk/browser.elf", "browser"],
                        &[],
                    ) {
                        crate::ipc::register(pid);
                        crate::gui::register_browser_pid(pid);
                        return Ok(pid);
                    }
                }
            }
        }

        Err(())
    }
}

static mut BACKBUFFER: *mut u32 = core::ptr::null_mut();
static mut SESSION: Option<AladdinSession> = None;

pub fn init() {
    unsafe {
        if let Some(fb) = limine::framebuffer() {
            let total_bytes = (fb.width * fb.height * 4) as usize;
            let layout = core::alloc::Layout::from_size_align(total_bytes, 16)
                .expect("Falha no layout do backbuffer");
            let buf_ptr = alloc::alloc::alloc_zeroed(layout) as *mut u32;
            if !buf_ptr.is_null() {
                *core::ptr::addr_of_mut!(BACKBUFFER) = buf_ptr;
            }
            *core::ptr::addr_of_mut!(SESSION) = Some(AladdinSession::new());
        }
    }
}

pub fn is_available() -> bool {
    unsafe {
        let bb_ptr = *core::ptr::addr_of!(BACKBUFFER);
        let fb = limine::framebuffer();
        fb.is_some() && !bb_ptr.is_null()
    }
}

/// Cria uma nova janela de cliente para o processo `owner_pid`.
/// `title`: título exibido na barra da janela.
/// `width`, `height`: dimensões da área cliente de desenho em pixels.
pub fn window_create(owner_pid: usize, title: &str, width: usize, height: usize) -> Result<usize, isize> {
    if width == 0 || height == 0 || width > 1024 || height > 768 {
        return Err(crate::syscall::EINVAL);
    }
    // Verifica limites de recursos configurados para a tarefa
    if let Some(Some(task)) = crate::scheduler::get().tasks.get_mut(owner_pid) {
        if let Err(e) = task.resources.check_and_open_window(&task.limits) {
            return Err(e);
        }
    }
    unsafe {
        if let Some(session) = (*core::ptr::addr_of_mut!(SESSION)).as_mut() {
            let is_browser = if let Some(task) = crate::scheduler::get().tasks.get(owner_pid).and_then(|t| t.as_ref()) {
                task.name.contains("browser")
            } else {
                false
            };
            if is_browser {
                session.register_browser_pid(owner_pid);
            }
            match session.create_client_window(owner_pid, title, width, height) {
                Ok(win_id) => Ok(win_id),
                Err(_) => {
                    if let Some(Some(task)) = crate::scheduler::get().tasks.get_mut(owner_pid) {
                        task.resources.close_window();
                    }
                    Err(crate::syscall::EMFILE)
                }
            }
        } else {
            Err(crate::syscall::ENOSYS)
        }
    }
}

/// Atualiza o conteúdo gráfico da janela `win_id` com os pixels do `buffer` (formato 0x00RRGGBB).
pub fn window_update(owner_pid: usize, win_id: usize, buffer: &[u32]) -> Result<(), isize> {
    unsafe {
        if let Some(session) = (*core::ptr::addr_of_mut!(SESSION)).as_mut() {
            session.update_client_buffer(owner_pid, win_id, buffer)
                .map_err(|e| match e {
                    aladdin::wm::WindowOpError::NotFound => crate::syscall::EBADF,
                    aladdin::wm::WindowOpError::PermissionDenied => crate::syscall::EPERM,
                    aladdin::wm::WindowOpError::InvalidDimensions => crate::syscall::EINVAL,
                })
        } else {
            Err(crate::syscall::ENOSYS)
        }
    }
}

/// Fecha a janela `win_id` pertencente ao processo `owner_pid`.
pub fn window_close(owner_pid: usize, win_id: usize) -> Result<(), isize> {
    unsafe {
        if let Some(session) = (*core::ptr::addr_of_mut!(SESSION)).as_mut() {
            let res = session.close_client_window(owner_pid, win_id)
                .map_err(|e| match e {
                    aladdin::wm::WindowOpError::NotFound => crate::syscall::EBADF,
                    aladdin::wm::WindowOpError::PermissionDenied => crate::syscall::EPERM,
                    aladdin::wm::WindowOpError::InvalidDimensions => crate::syscall::EINVAL,
                });
            if res.is_ok() {
                if let Some(Some(task)) = crate::scheduler::get().tasks.get_mut(owner_pid) {
                    task.resources.close_window();
                }
            }
            res
        } else {
            Err(crate::syscall::ENOSYS)
        }
    }
}

/// Consulta o próximo evento de entrada pendente para a janela `win_id` do processo `owner_pid`.
pub fn window_poll_event(owner_pid: usize, win_id: usize) -> Result<Option<ClientWindowEvent>, isize> {
    unsafe {
        if let Some(session) = (*core::ptr::addr_of_mut!(SESSION)).as_mut() {
            session.poll_client_event(owner_pid, win_id)
                .map_err(|e| match e {
                    aladdin::wm::WindowOpError::NotFound => crate::syscall::EBADF,
                    aladdin::wm::WindowOpError::PermissionDenied => crate::syscall::EPERM,
                    aladdin::wm::WindowOpError::InvalidDimensions => crate::syscall::EINVAL,
                })
        } else {
            Err(crate::syscall::ENOSYS)
        }
    }
}

/// Fecha todas as janelas do processo `pid` (chamado no encerramento da tarefa).
pub fn register_browser_pid(pid: usize) {
    unsafe {
        if let Some(session) = (*core::ptr::addr_of_mut!(SESSION)).as_mut() {
            session.register_browser_pid(pid);
        }
    }
}

pub fn close_windows_for_pid(pid: usize) {
    unsafe {
        if let Some(session) = (*core::ptr::addr_of_mut!(SESSION)).as_mut() {
            session.close_windows_for_pid(pid);
        }
    }
    if let Some(Some(task)) = crate::scheduler::get().tasks.get_mut(pid) {
        task.resources.open_windows = 0;
    }
}

pub fn run_desktop() {
    unsafe {
        let bb_ptr = *core::ptr::addr_of!(BACKBUFFER);
        let fb_opt = limine::framebuffer();
        if bb_ptr.is_null() || fb_opt.is_none() { return; }
        let fb = fb_opt.unwrap();

        let mut jinn_fb = JinnFramebuffer {
            address: fb.address,
            width: fb.width as usize,
            height: fb.height as usize,
            pitch: fb.pitch as usize,
        };
        let jinn_sys = JinnSystem;
        let jinn_vfs = JinnVfs;
        let mut jinn_proc = JinnProcess;

        let total_pixels = jinn_fb.width * jinn_fb.height;
        let backbuffer_slice = core::slice::from_raw_parts_mut(bb_ptr, total_pixels);

        let mut poll_keyboard = || -> Option<u8> {
            let status: u8;
            core::arch::asm!("in al, dx", out("al") status, in("dx") 0x64u16, options(nomem, nostack));
            if (status & 1) != 0 && (status & 0x20) == 0 {
                let sc: u8;
                core::arch::asm!("in al, dx", out("al") sc, in("dx") 0x60u16, options(nomem, nostack));
                if sc == 0x01 { // Scancode ESC
                    return Some(0x1B);
                }
                crate::interrupts::ps2_keyboard::process_scancode(sc);
            }
            crate::keyboard_buffer::pop()
        };

        let mut poll_mouse = || -> (usize, usize, u8) {
            crate::interrupts::ps2_mouse::get_state()
        };

        if let Some(session) = (*core::ptr::addr_of_mut!(SESSION)).as_mut() {
            session.run(
                &mut jinn_fb,
                &jinn_sys,
                &jinn_vfs,
                &mut jinn_proc,
                backbuffer_slice,
                &mut poll_keyboard,
                &mut poll_mouse,
            );
        }
    }

    // Ao sair, limpa a tela para restaurar o console de texto VGA
    let mut writer = crate::vga::Writer::new();
    writer.clear_screen();
    writer.set_color(crate::vga::Color::LightCyan, crate::vga::Color::Black);
    writer.write_line("Retornou ao Shell do Kernel [Ring 0].");
    writer.set_color(crate::vga::Color::LightGray, crate::vga::Color::Black);
}
