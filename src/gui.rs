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
use core::sync::atomic::{AtomicUsize, Ordering};
use crate::memory::SpinLock;
use alloc::vec::Vec;

pub static GRAPHICS_SERVER_PID: AtomicUsize = AtomicUsize::new(0);

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphicsFbInfo {
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub bpp: u32,
    pub format: u32, // 0 = 0x00RRGGBB / BGRX
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
    pub kind: u32, // 0 = None, 1 = KeyDown, 2 = KeyUp, 3 = MouseMove, 4 = MouseDown, 5 = MouseUp, 6 = Scroll
    pub code: u32, // ASCII / Scancode / Button
    pub x: i32,    // Coordenada absoluta X
    pub y: i32,    // Coordenada absoluta Y
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

pub struct KernelClientWindow {
    pub id: usize,
    pub owner_pid: usize,
    pub width: usize,
    pub height: usize,
    pub title: [u8; 48],
    pub title_len: usize,
    pub buffer: Vec<u32>,
    pub events: [ClientWindowEvent; 16],
    pub ev_head: usize,
    pub ev_tail: usize,
    pub damaged: bool,
    pub is_open: bool,
}

pub static CLIENT_WINDOWS: SpinLock<[Option<KernelClientWindow>; 32]> = SpinLock::new([
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
]);

static LAST_MOUSE_STATE: SpinLock<(usize, usize, u8)> = SpinLock::new((512, 384, 0));

pub fn register_graphics_server(pid: usize, _flags: u32) -> Result<(), isize> {
    let current = GRAPHICS_SERVER_PID.load(Ordering::SeqCst);
    if current != 0 && current != pid {
        crate::serial_write(b"[GUI] Falha ao registrar Servidor Grafico: EBUSY (ja ocupado por outro PID)\n");
        return Err(crate::syscall::EBUSY);
    }
    GRAPHICS_SERVER_PID.store(pid, Ordering::SeqCst);
    crate::interrupts::ps2_mouse::init();
    crate::serial_write(b"[GUI] Servidor Grafico Ring 3 registrado com sucesso! GRAPHICS_SERVER_PID ativo.\n");
    Ok(())
}

pub fn unregister_graphics_server(pid: usize) -> Result<(), isize> {
    let current = GRAPHICS_SERVER_PID.load(Ordering::SeqCst);
    if current != pid {
        return Err(crate::syscall::EPERM);
    }
    GRAPHICS_SERVER_PID.store(0, Ordering::SeqCst);
    crate::serial_write(b"[GUI] Servidor Grafico Ring 3 desregistrado. Restaurando console de texto.\n");
    restore_text_console();
    Ok(())
}

pub fn notify_graphics_server_exit(pid: usize) {
    if GRAPHICS_SERVER_PID.load(Ordering::SeqCst) == pid {
        GRAPHICS_SERVER_PID.store(0, Ordering::SeqCst);
        restore_text_console();
    }
}

pub fn restore_text_console() {
    let mut writer = crate::vga::Writer::new();
    writer.clear_screen();
    writer.set_color(crate::vga::Color::LightCyan, crate::vga::Color::Black);
    writer.write_line("Retornou ao Shell do Kernel [Ring 0].");
    writer.set_color(crate::vga::Color::LightGray, crate::vga::Color::Black);
}

pub fn get_fb_info() -> Result<GraphicsFbInfo, isize> {
    if let Some(fb) = limine::framebuffer() {
        Ok(GraphicsFbInfo {
            width: fb.width as u32,
            height: fb.height as u32,
            pitch: fb.pitch as u32,
            bpp: fb.bpp as u32,
            format: 0,
        })
    } else {
        Err(crate::syscall::ENODEV)
    }
}

pub fn present_rect(caller_pid: usize, rect: &PresentRect, buffer: &[u32]) -> Result<(), isize> {
    let s_pid = GRAPHICS_SERVER_PID.load(Ordering::SeqCst);
    if s_pid != 0 && caller_pid != s_pid {
        return Err(crate::syscall::EPERM);
    }
    let fb = match limine::framebuffer() {
        Some(fb) => fb,
        None => return Err(crate::syscall::ENODEV),
    };

    if rect.x as usize >= fb.width as usize || rect.y as usize >= fb.height as usize {
        return Err(crate::syscall::EINVAL);
    }
    let max_w = (fb.width as usize).saturating_sub(rect.x as usize);
    let max_h = (fb.height as usize).saturating_sub(rect.y as usize);
    let rw = (rect.width as usize).min(max_w);
    let rh = (rect.height as usize).min(max_h);
    if rw == 0 || rh == 0 {
        return Ok(());
    }

    let min_len = (rect.width as usize) * (rect.height as usize);
    if buffer.len() < min_len {
        return Err(crate::syscall::EINVAL);
    }

    let bytes_to_copy = rw * 4;
    let pitch = fb.pitch as usize;
    let fb_addr = fb.address;

    for row in 0..rh {
        let src_offset = row * (rect.width as usize);
        let dst_offset = (rect.y as usize + row) * pitch + (rect.x as usize) * 4;
        unsafe {
            core::ptr::copy_nonoverlapping(
                buffer.as_ptr().add(src_offset) as *const u8,
                fb_addr.add(dst_offset),
                bytes_to_copy,
            );
        }
    }

    Ok(())
}

pub fn poll_hardware_input(caller_pid: usize) -> Result<Option<InputHardwareEvent>, isize> {
    let s_pid = GRAPHICS_SERVER_PID.load(Ordering::SeqCst);
    if s_pid != 0 && caller_pid != s_pid {
        return Err(crate::syscall::EPERM);
    }

    // 1. Prioriza eventos de teclado
    let status: u8;
    unsafe {
        core::arch::asm!("in al, dx", out("al") status, in("dx") 0x64u16, options(nomem, nostack));
    }
    if (status & 1) != 0 && (status & 0x20) == 0 {
        let sc: u8;
        unsafe {
            core::arch::asm!("in al, dx", out("al") sc, in("dx") 0x60u16, options(nomem, nostack));
        }
        if sc == 0x01 { // ESC
            return Ok(Some(InputHardwareEvent {
                kind: 1, // KeyDown
                code: 0x1B,
                x: 0,
                y: 0,
                modifiers: 0,
            }));
        }
        if !crate::drivers::device_manager::post_raw_hardware_byte(crate::drivers::device_manager::DEV_KBD0, 1, sc) {
            crate::interrupts::ps2_keyboard::process_scancode(sc);
        }
    }
    if let Some(ch) = crate::keyboard_buffer::pop() {
        return Ok(Some(InputHardwareEvent {
            kind: 1, // KeyDown
            code: ch as u32,
            x: 0,
            y: 0,
            modifiers: 0,
        }));
    }

    // 2. Eventos de mouse
    crate::interrupts::ps2_mouse::poll();
    let (cur_x, cur_y, cur_btn) = crate::interrupts::ps2_mouse::get_state();
    let mut last = LAST_MOUSE_STATE.lock();
    if (cur_x, cur_y, cur_btn) != *last {
        let (_lx, _ly, lbtn) = *last;
        *last = (cur_x, cur_y, cur_btn);

        let kind = if (cur_btn & 1) != (lbtn & 1) {
            if (cur_btn & 1) != 0 { 4 } else { 5 } // MouseDown / MouseUp
        } else if (cur_btn & 2) != (lbtn & 2) {
            if (cur_btn & 2) != 0 { 4 } else { 5 }
        } else {
            3 // MouseMove
        };

        return Ok(Some(InputHardwareEvent {
            kind,
            code: cur_btn as u32,
            x: cur_x as i32,
            y: cur_y as i32,
            modifiers: 0,
        }));
    }

    Ok(None)
}

pub fn client_window_create(owner_pid: usize, title: &str, width: usize, height: usize) -> Result<usize, isize> {
    if width == 0 || height == 0 || width > 1024 || height > 768 {
        return Err(crate::syscall::EINVAL);
    }
    if let Some(Some(task)) = crate::scheduler::get().tasks.get_mut(owner_pid) {
        if let Err(e) = task.resources.check_and_open_window(&task.limits) {
            return Err(e);
        }
    }

    let mut windows = CLIENT_WINDOWS.lock();
    for (idx, slot) in windows.iter_mut().enumerate() {
        if slot.is_none() {
            let win_id = idx + 1;
            let mut title_buf = [0u8; 48];
            let t_bytes = title.as_bytes();
            let t_len = t_bytes.len().min(48);
            title_buf[..t_len].copy_from_slice(&t_bytes[..t_len]);

            let total_pixels = width * height;
            let mut buf = alloc::vec::Vec::new();
            if buf.try_reserve_exact(total_pixels).is_err() {
                if let Some(Some(task)) = crate::scheduler::get().tasks.get_mut(owner_pid) {
                    task.resources.close_window();
                }
                return Err(crate::syscall::ENOMEM);
            }
            buf.resize(total_pixels, 0u32);

            *slot = Some(KernelClientWindow {
                id: win_id,
                owner_pid,
                width,
                height,
                title: title_buf,
                title_len: t_len,
                buffer: buf,
                events: [ClientWindowEvent::empty(); 16],
                ev_head: 0,
                ev_tail: 0,
                damaged: true,
                is_open: true,
            });

            // Se o SESSION em kernel estiver ativo, também registra para compatibilidade
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
                    let _ = session.create_client_window(owner_pid, title, width, height);
                }
            }

            return Ok(win_id);
        }
    }
    if let Some(Some(task)) = crate::scheduler::get().tasks.get_mut(owner_pid) {
        task.resources.close_window();
    }
    Err(crate::syscall::EMFILE)
}

pub fn client_window_update(owner_pid: usize, win_id: usize, buffer: &[u32]) -> Result<(), isize> {
    let mut windows = CLIENT_WINDOWS.lock();
    for slot in windows.iter_mut() {
        if let Some(w) = slot {
            if w.id == win_id {
                if w.owner_pid != owner_pid {
                    return Err(crate::syscall::EPERM);
                }
                let expected = w.width * w.height;
                if buffer.len() != expected {
                    return Err(crate::syscall::EINVAL);
                }
                w.buffer.copy_from_slice(buffer);
                w.damaged = true;

                unsafe {
                    if let Some(session) = (*core::ptr::addr_of_mut!(SESSION)).as_mut() {
                        let _ = session.update_client_buffer(owner_pid, win_id, buffer);
                    }
                }
                return Ok(());
            }
        }
    }
    Err(crate::syscall::EBADF)
}

pub fn client_window_close(owner_pid: usize, win_id: usize) -> Result<(), isize> {
    let mut windows = CLIENT_WINDOWS.lock();
    for slot in windows.iter_mut() {
        if let Some(w) = slot {
            if w.id == win_id {
                if w.owner_pid != owner_pid {
                    return Err(crate::syscall::EPERM);
                }
                *slot = None;
                if let Some(Some(task)) = crate::scheduler::get().tasks.get_mut(owner_pid) {
                    task.resources.close_window();
                }
                unsafe {
                    if let Some(session) = (*core::ptr::addr_of_mut!(SESSION)).as_mut() {
                        let _ = session.close_client_window(owner_pid, win_id);
                    }
                }
                return Ok(());
            }
        }
    }
    Err(crate::syscall::EBADF)
}

pub fn client_window_poll_event(owner_pid: usize, win_id: usize) -> Result<Option<ClientWindowEvent>, isize> {
    let mut windows = CLIENT_WINDOWS.lock();
    for slot in windows.iter_mut() {
        if let Some(w) = slot {
            if w.id == win_id {
                if w.owner_pid != owner_pid {
                    return Err(crate::syscall::EPERM);
                }
                if w.ev_head != w.ev_tail {
                    let ev = w.events[w.ev_head % 16];
                    w.ev_head = w.ev_head.wrapping_add(1);
                    return Ok(Some(ev));
                }
                unsafe {
                    if let Some(session) = (*core::ptr::addr_of_mut!(SESSION)).as_mut() {
                        if let Ok(ev_opt) = session.poll_client_event(owner_pid, win_id) {
                            if ev_opt.is_some() {
                                return Ok(ev_opt);
                            }
                        }
                    }
                }
                return Ok(None);
            }
        }
    }
    Err(crate::syscall::EBADF)
}

pub fn graphics_read_client_surface(caller_pid: usize, win_id: usize, dst_buf: &mut [u32]) -> Result<usize, isize> {
    let s_pid = GRAPHICS_SERVER_PID.load(Ordering::SeqCst);
    if s_pid != 0 && caller_pid != s_pid {
        return Err(crate::syscall::EPERM);
    }
    let mut windows = CLIENT_WINDOWS.lock();
    for slot in windows.iter_mut() {
        if let Some(w) = slot {
            if w.id == win_id {
                let to_copy = w.buffer.len().min(dst_buf.len());
                dst_buf[..to_copy].copy_from_slice(&w.buffer[..to_copy]);
                w.damaged = false;
                return Ok(to_copy * 4);
            }
        }
    }
    Err(crate::syscall::EBADF)
}

pub fn graphics_dispatch_client_event(caller_pid: usize, win_id: usize, event: &ClientWindowEvent) -> Result<(), isize> {
    let s_pid = GRAPHICS_SERVER_PID.load(Ordering::SeqCst);
    if s_pid != 0 && caller_pid != s_pid {
        return Err(crate::syscall::EPERM);
    }
    let mut windows = CLIENT_WINDOWS.lock();
    for slot in windows.iter_mut() {
        if let Some(w) = slot {
            if w.id == win_id {
                let next_tail = w.ev_tail.wrapping_add(1);
                if (next_tail - w.ev_head) <= 16 {
                    w.events[w.ev_tail % 16] = *event;
                    w.ev_tail = next_tail;
                }
                return Ok(());
            }
        }
    }
    Err(crate::syscall::EBADF)
}

pub fn graphics_list_client_windows(caller_pid: usize, out_slice: &mut [ClientWindowInfoUser]) -> Result<usize, isize> {
    let s_pid = GRAPHICS_SERVER_PID.load(Ordering::SeqCst);
    if s_pid != 0 && caller_pid != s_pid {
        return Err(crate::syscall::EPERM);
    }
    let windows = CLIENT_WINDOWS.lock();
    let mut count = 0usize;
    for slot in windows.iter() {
        if let Some(w) = slot {
            if count < out_slice.len() {
                out_slice[count] = ClientWindowInfoUser {
                    win_id: w.id as u32,
                    owner_pid: w.owner_pid as u32,
                    width: w.width as u32,
                    height: w.height as u32,
                    is_open: if w.is_open { 1 } else { 0 },
                    damaged: if w.damaged { 1 } else { 0 },
                    title_len: w.title_len as u32,
                    title: w.title,
                };
                count += 1;
            }
        }
    }
    Ok(count)
}

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
        let net_adapter = crate::drivers::net::active_nic_name();
        let net_state = crate::drivers::net::audit_network_state();
        let net_state_desc = net_state.description();
        let jinnwall_active_conns = unsafe { crate::net::jinnwall::JINNWALL.active_connections() };
        let jinnwall_drops = unsafe { crate::net::jinnwall::JINNWALL.drops() };
        let jinnwall_inspected = unsafe { crate::net::jinnwall::JINNWALL.inspected() };
        let jinnwall_accepted = unsafe { crate::net::jinnwall::JINNWALL.accepted() };
        let jinnwall_profile = match unsafe { crate::net::jinnwall::JINNWALL.profile() } {
            crate::net::jinnwall::FirewallProfile::PublicSecure => JinnWallProfile::PublicSecure,
            crate::net::jinnwall::FirewallProfile::Development => JinnWallProfile::Development,
        };

        let mut total_open_fds = 0usize;
        let sched = crate::scheduler::get();
        for t in sched.tasks.iter().flatten() {
            total_open_fds += t.resources.open_fds;
        }
        let total_vols = crate::vfs::volume::VOLUME_MANAGER.lock().count;

        SystemStats {
            uptime_secs: (ticks / 100) as usize,
            task_count: sched.task_count(),
            sched_ticks: crate::scheduler::sched_ticks(),
            used_ram_mb: crate::memory::used_bytes() / (1024 * 1024),
            total_ram_mb: crate::memory::total_bytes() / (1024 * 1024),
            current_tick: ticks,
            net_ip,
            net_adapter,
            net_state_desc,
            jinnwall_active_conns,
            jinnwall_drops,
            jinnwall_inspected,
            jinnwall_accepted,
            jinnwall_profile,
            physical_frames_used: crate::memory::used_bytes() / 4096,
            physical_frames_total: crate::memory::total_bytes() / 4096,
            dynamic_pages_allocated: crate::memory::allocated_dynamic_pages() as usize,
            dynamic_pages_freed: crate::memory::freed_dynamic_pages() as usize,
            vfs_open_fds: total_open_fds,
            vfs_mounted_volumes: total_vols,
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

    fn storage_disk_count(&self) -> usize {
        crate::drivers::block::BLOCK_DEVICE_MANAGER.lock().device_count()
    }

    fn storage_get_disk(&self, index: usize) -> Option<aladdin::contracts::storage::DiskInfo> {
        let bdm = crate::drivers::block::BLOCK_DEVICE_MANAGER.lock();
        let dev_info = bdm.get_device(index)?;
        let mut d = aladdin::contracts::storage::DiskInfo::empty();

        let n_len = dev_info.name_len.min(31);
        d.name[..n_len].copy_from_slice(&dev_info.name[..n_len]);
        d.name_len = n_len;

        let m_len = 40.min(dev_info.model.len());
        d.model[..m_len].copy_from_slice(&dev_info.model[..m_len]);
        d.model_len = m_len;

        d.bus = match dev_info.bus_type {
            crate::drivers::block::BusType::IdeAta => aladdin::contracts::storage::DiskBus::IdeAta,
            crate::drivers::block::BusType::AhciSata => aladdin::contracts::storage::DiskBus::AhciSata,
            crate::drivers::block::BusType::NvmePci => aladdin::contracts::storage::DiskBus::NvmePci,
            crate::drivers::block::BusType::VirtioBlk => aladdin::contracts::storage::DiskBus::VirtioBlk,
            crate::drivers::block::BusType::RamDisk => aladdin::contracts::storage::DiskBus::RamDisk,
            crate::drivers::block::BusType::Unknown => aladdin::contracts::storage::DiskBus::Unknown,
        };

        d.capacity_mb = dev_info.size_mb();
        d.is_system_disk = dev_info.is_boot_device;
        Some(d)
    }

    fn storage_volume_count(&self) -> usize {
        crate::vfs::volume::VOLUME_MANAGER.lock().volume_count()
    }

    fn storage_get_volume(&self, index: usize) -> Option<aladdin::contracts::storage::VolumeDisplayInfo> {
        let vm = crate::vfs::volume::VOLUME_MANAGER.lock();
        let vol = vm.get_volume(index)?;
        let mut v = aladdin::contracts::storage::VolumeDisplayInfo::empty();
        v.id = vol.id;

        let l_len = vol.label_len.min(31);
        v.label[..l_len].copy_from_slice(&vol.label[..l_len]);
        v.label_len = l_len;

        let m_len = vol.mount_point_len.min(31);
        v.mount_point[..m_len].copy_from_slice(&vol.mount_point[..m_len]);
        v.mount_point_len = m_len;

        let fs_str = match vol.fs_kind {
            crate::drivers::partition::DetectedFsKind::Fat32 => "FAT32",
            crate::drivers::partition::DetectedFsKind::Ext2 => "Ext2",
            crate::drivers::partition::DetectedFsKind::Iso9660 => "ISO9660",
            _ => "RAW",
        };
        let fs_bytes = fs_str.as_bytes();
        let fs_len = fs_bytes.len().min(15);
        v.fs_type[..fs_len].copy_from_slice(&fs_bytes[..fs_len]);
        v.fs_type_len = fs_len;

        v.total_mb = vol.total_mb as u64;
        v.used_mb = vol.used_mb as u64;
        v.free_mb = vol.free_mb as u64;
        if vol.is_mounted {
            let pfx = vol.mount_point_str();
            if let Ok(st) = crate::vfs::statvfs(pfx) {
                let total_mb = st.total_bytes / (1024 * 1024);
                let free_mb = st.free_bytes / (1024 * 1024);
                v.total_mb = total_mb;
                v.free_mb = free_mb;
                v.used_mb = total_mb.saturating_sub(free_mb);
            }
        }
        v.is_mounted = vol.is_mounted;
        v.is_root = vol.is_root;
        v.is_busy = vol.open_handles_count > 0;
        Some(v)
    }

    fn storage_mount_volume(&self, volume_id: usize, mount_point: &str) -> Result<(), &'static str> {
        let mut vm = crate::vfs::volume::VOLUME_MANAGER.lock();
        vm.mount_volume(volume_id, mount_point)
    }

    fn storage_unmount_volume(&self, volume_id: usize) -> Result<(), &'static str> {
        let mut vm = crate::vfs::volume::VOLUME_MANAGER.lock();
        vm.unmount_volume(volume_id)
    }

    fn storage_format_volume(&self, volume_id: usize, fs_type: &str) -> Result<(), &'static str> {
        let mut vm = crate::vfs::volume::VOLUME_MANAGER.lock();
        vm.format_volume(volume_id, fs_type)
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

    fn create_file(&self, path: &str) -> Result<(), ()> {
        crate::vfs::create(path).map_err(|_| ())
    }

    fn create_dir(&self, path: &str) -> Result<(), ()> {
        crate::vfs::mkdir(path).map_err(|_| ())
    }

    fn remove_file(&self, path: &str) -> Result<(), ()> {
        crate::vfs::remove(path).map_err(|_| ())
    }

    fn rename_file(&self, old_path: &str, new_path: &str) -> Result<(), ()> {
        crate::vfs::rename(old_path, new_path).map_err(|_| ())
    }

    fn copy_file(&self, src_path: &str, dst_path: &str) -> Result<(), ()> {
        crate::vfs::copy(src_path, dst_path).map(|_| ()).map_err(|_| ())
    }

    fn file_size(&self, path: &str) -> Result<usize, ()> {
        crate::vfs::stat(path).map(|s| s.size as usize).map_err(|_| ())
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
                // Se a falha foi de formato/integridade interna do binário (ENOEXEC = -8, E2BIG = -7),
                // não podemos mascarar a corrupção do arquivo.
                // Mas se for arquivo não encontrado (-ENOENT = -2) ou erro de VFS/mídia não montada (-EIO = -5),
                // tentamos caminhos alternativos e o fallback resiliente em memória Limine!
                if errno != -2 && errno != -5 {
                    return Err(());
                }
            }
        }

        // 2. Tenta apenas variantes de caixa no FAT32 se o arquivo primário não foi encontrado (-ENOENT / -EIO)
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
                    if errno != -2 && errno != -5 {
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
    // Inicialização leve do subsistema gráfico: valida presença do framebuffer Limine
    // e inicializa subsistemas de entrada. Não aloca redundantemente o backbuffer de 3 MiB
    // no heap do kernel, preservando memória RAM para o compositor independente Ring 3.
}

pub fn is_available() -> bool {
    limine::framebuffer().is_some()
}

/// Cria uma nova janela de cliente para o processo `owner_pid`.
/// `title`: título exibido na barra da janela.
/// `width`, `height`: dimensões da área cliente de desenho em pixels.
pub fn window_create(owner_pid: usize, title: &str, width: usize, height: usize) -> Result<usize, isize> {
    client_window_create(owner_pid, title, width, height)
}

/// Atualiza o conteúdo gráfico da janela `win_id` com os pixels do `buffer` (formato 0x00RRGGBB).
pub fn window_update(owner_pid: usize, win_id: usize, buffer: &[u32]) -> Result<(), isize> {
    client_window_update(owner_pid, win_id, buffer)
}

/// Fecha a janela `win_id` pertencente ao processo `owner_pid`.
pub fn window_close(owner_pid: usize, win_id: usize) -> Result<(), isize> {
    client_window_close(owner_pid, win_id)
}

/// Consulta o próximo evento de entrada pendente para a janela `win_id` do processo `owner_pid`.
pub fn window_poll_event(owner_pid: usize, win_id: usize) -> Result<Option<ClientWindowEvent>, isize> {
    client_window_poll_event(owner_pid, win_id)
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
    let mut windows = CLIENT_WINDOWS.lock();
    for slot in windows.iter_mut() {
        if let Some(w) = slot {
            if w.owner_pid == pid {
                *slot = None;
            }
        }
    }
    unsafe {
        if let Some(session) = (*core::ptr::addr_of_mut!(SESSION)).as_mut() {
            session.close_windows_for_pid(pid);
        }
    }
    if let Some(Some(task)) = crate::scheduler::get().tasks.get_mut(pid) {
        task.resources.open_windows = 0;
    }
}

pub fn open_window(win_id: usize) {
    let mut windows = CLIENT_WINDOWS.lock();
    for slot in windows.iter_mut() {
        if let Some(w) = slot {
            if w.id == win_id {
                w.is_open = true;
                w.damaged = true;
            }
        }
    }
    unsafe {
        if let Some(session) = (*core::ptr::addr_of_mut!(SESSION)).as_mut() {
            if let Some(w) = session.wm.windows.get_mut(win_id).and_then(|w| w.as_mut()) {
                w.is_open = true;
                w.is_minimized = false;
                session.wm.bring_to_front(win_id);
                session.client_damaged = true;
            }
        }
    }
}

/// Inicia o serviço gráfico do Aladdin Desktop em Ring 3 como processo de usuário independente.
pub fn launch_ring3_desktop() {
    crate::serial_write(b"[GUI] launch_ring3_desktop() chamado\n");
    let path = "/disk/aladdin.elf";
    let argv = [path, "server"];
    
    // 1. Tenta carregar do disco FAT32
    crate::serial_write(b"[GUI] Tentando spawn_user_task_from_vfs(/disk/aladdin.elf)...\n");
    let spawn_res = crate::scheduler::get().spawn_user_task_from_vfs(path, &argv, &[]);
    let pid = match spawn_res {
        Ok(p) => {
            crate::serial_write(b"[GUI] /disk/aladdin.elf carregado com sucesso!\n");
            p
        },
        Err(_e) => {
            crate::serial_write(b"[GUI] Falha ao carregar /disk/aladdin.elf, tentando /disk/ALADDIN.ELF...\n");
            // Tenta variante de caixa /disk/ALADDIN.ELF
            match crate::scheduler::get().spawn_user_task_from_vfs("/disk/ALADDIN.ELF", &argv, &[]) {
                Ok(p) => {
                    crate::serial_write(b"[GUI] /disk/ALADDIN.ELF carregado com sucesso!\n");
                    p
                },
                Err(_) => {
                    crate::serial_write(b"[GUI] Falha no disco, tentando modulo Limine 'aladdin'...\n");
                    // Fallback para módulos Limine em memória
                    if let Some(elf_bytes) = limine::get_module_by_name("aladdin").or_else(|| limine::get_module_by_name("aladdin-server")) {
                        match crate::scheduler::get().spawn_user_task_from_memory("aladdin[U3]", elf_bytes, &argv, &[]) {
                            Ok(p) => {
                                crate::serial_write(b"[GUI] Modulo Limine aladdin carregado com sucesso!\n");
                                p
                            },
                            Err(_) => {
                                crate::serial_write(b"[GUI] Falha ao spawnar modulo Limine, fallback Ring 0\n");
                                run_desktop();
                                return;
                            }
                        }
                    } else {
                        crate::serial_write(b"[GUI] Modulo Limine nao encontrado, fallback Ring 0\n");
                        run_desktop();
                        return;
                    }
                }
            }
        }
    };

    crate::ipc::register(pid);
    crate::serial_write(b"[GUI] Aguardando execucao do compositor Ring 3...\n");
    loop {
        if crate::scheduler::get().collect_zombie(pid).is_some() {
            crate::serial_write(b"[GUI] Compositor Ring 3 terminou (zombie coletado)\n");
            break;
        }
        if pid >= crate::scheduler::MAX_TASKS || crate::scheduler::get().tasks[pid].is_none() {
            crate::serial_write(b"[GUI] Compositor Ring 3 saiu do slot de tarefas\n");
            break;
        }
        crate::scheduler::yield_now();
    }
    crate::serial_write(b"[GUI] launch_ring3_desktop finalizado, restaurando console de texto\n");
    restore_text_console();
}

pub fn run_desktop() {
    unsafe {
        let fb_opt = limine::framebuffer();
        if fb_opt.is_none() { return; }
        let fb = fb_opt.unwrap();

        // Aloca backbuffer de fallback no heap do kernel sob demanda apenas se necessário
        let mut bb_ptr = *core::ptr::addr_of!(BACKBUFFER);
        if bb_ptr.is_null() {
            let total_bytes = (fb.width * fb.height * 4) as usize;
            if let Ok(layout) = core::alloc::Layout::from_size_align(total_bytes, 16) {
                bb_ptr = alloc::alloc::alloc_zeroed(layout) as *mut u32;
                *core::ptr::addr_of_mut!(BACKBUFFER) = bb_ptr;
            }
        }
        if bb_ptr.is_null() { return; }

        if (*core::ptr::addr_of!(SESSION)).is_none() {
            *core::ptr::addr_of_mut!(SESSION) = Some(AladdinSession::new());
        }

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

        crate::interrupts::ps2_mouse::init();

        let mut poll_keyboard = || -> Option<u8> {
            let status: u8;
            core::arch::asm!("in al, dx", out("al") status, in("dx") 0x64u16, options(nomem, nostack));
            if (status & 1) != 0 && (status & 0x20) == 0 {
                let sc: u8;
                core::arch::asm!("in al, dx", out("al") sc, in("dx") 0x60u16, options(nomem, nostack));
                if sc == 0x01 { // Scancode ESC
                    return Some(0x1B);
                }
                if !crate::drivers::device_manager::post_raw_hardware_byte(crate::drivers::device_manager::DEV_KBD0, 1, sc) {
                    crate::interrupts::ps2_keyboard::process_scancode(sc);
                }
            }
            crate::keyboard_buffer::pop()
        };

        let mut poll_mouse = || -> (usize, usize, u8) {
            crate::interrupts::ps2_mouse::poll();
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

    restore_text_console();
}
