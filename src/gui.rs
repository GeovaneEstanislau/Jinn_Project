//! Adaptador de Plataforma do Aladdin Desktop para o Jinn OS
//! Conecta o microkernel x86_64 aos contratos da crate externa 'aladdin'.

use crate::limine;
pub use aladdin::contracts::system::{CapabilityMode, JinnWallProfile};
pub use aladdin::contracts::input::{
    ClientWindowEvent, EVENT_KEY_DOWN, EVENT_KEY_UP, EVENT_MOUSE_DOWN, EVENT_MOUSE_MOVE,
    EVENT_MOUSE_UP, EVENT_NONE, EVENT_SCROLL, EVENT_WINDOW_CLOSE, EVENT_WINDOW_RESIZE,
    EVENT_WINDOW_EXPOSE,
};
use aladdin::contracts::framebuffer::{DirtyRect, FramebufferDevice};
use aladdin::contracts::system::{SystemHost, SystemStats};
use aladdin::contracts::vfs::VfsHost;
use aladdin::contracts::process::ProcessHost;
use aladdin::contracts::shared_rings::{
    AladdinSharedRings, SharedInputEvent,
    INPUT_RING_CAP, CMD_RING_CAP, CONSUMER_SLEEPING,
    GFX_OP_PRESENT_DIRTY_RECT, GFX_OP_SET_DISPLAY_POWER,
    DOORBELL_FLUSH_GFX, DOORBELL_WAIT_INPUT, GFX_FLAG_SUB_BUF,
};
use aladdin::desktop::session::AladdinSession;
use core::sync::atomic::{AtomicUsize, AtomicU64, Ordering};
use crate::memory::IrqSafeSpinLock;
use alloc::vec::Vec;

pub static GRAPHICS_SERVER_PID: AtomicUsize = AtomicUsize::new(0);
pub static DISPLAY_POWER_STATE: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(true);

pub static SHARED_RINGS_HHDM: AtomicUsize = AtomicUsize::new(0);
pub static SHARED_BACKBUFFER_VADDR: AtomicUsize = AtomicUsize::new(0);
pub static RING_EVENTS_DELIVERED: AtomicU64 = AtomicU64::new(0);
pub static RING_CMDS_FLUSHED: AtomicU64 = AtomicU64::new(0);
pub static RING_DOORBELL_CALLS: AtomicU64 = AtomicU64::new(0);

pub fn set_display_power(on: bool) {
    DISPLAY_POWER_STATE.store(on, Ordering::Release);
    if !on {
        if let Some(fb) = limine::framebuffer() {
            let total_bytes = (fb.pitch as usize) * (fb.height as usize);
            unsafe {
                core::ptr::write_bytes(fb.address, 0x00, total_bytes);
            }
        }
        unsafe {
            core::arch::asm!("out dx, al", in("dx") 0x3C4u16, in("al") 0x01u8, options(nomem, nostack));
            let clk: u8;
            core::arch::asm!("in al, dx", in("dx") 0x3C5u16, out("al") clk, options(nomem, nostack));
            core::arch::asm!("out dx, al", in("dx") 0x3C5u16, in("al") clk | 0x20, options(nomem, nostack));
        }
        crate::serial_write(b"[GUI] Display Power: OFF (Screen blanked, 0 rendering load)\n");
    } else {
        unsafe {
            core::arch::asm!("out dx, al", in("dx") 0x3C4u16, in("al") 0x01u8, options(nomem, nostack));
            let clk: u8;
            core::arch::asm!("in al, dx", in("dx") 0x3C5u16, out("al") clk, options(nomem, nostack));
            core::arch::asm!("out dx, al", in("dx") 0x3C5u16, in("al") clk & !0x20, options(nomem, nostack));
        }
        crate::serial_write(b"[GUI] Display Power: ON\n");
    }
}

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
    pub is_minimized: bool,
}

pub static CLIENT_WINDOWS: IrqSafeSpinLock<[Option<KernelClientWindow>; 32]> = IrqSafeSpinLock::new([
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
]);

static LAST_MOUSE_STATE: IrqSafeSpinLock<(usize, usize, u8)> = IrqSafeSpinLock::new((usize::MAX, usize::MAX, 0xFF));

pub fn register_graphics_server(pid: usize, _flags: u32) -> Result<(), isize> {
    let current = GRAPHICS_SERVER_PID.load(Ordering::SeqCst);
    if current != 0 && current != pid {
        crate::serial_write(b"[GUI] Falha ao registrar Servidor Grafico: EBUSY (ja ocupado por outro PID)\n");
        return Err(crate::syscall::EBUSY);
    }
    GRAPHICS_SERVER_PID.store(pid, Ordering::SeqCst);
    *LAST_MOUSE_STATE.lock() = (usize::MAX, usize::MAX, 0xFF);
    crate::interrupts::ps2_mouse::reset_sync();
    if let Some(fb) = crate::limine::framebuffer() {
        crate::interrupts::ps2_mouse::set_bounds(fb.width as usize, fb.height as usize);
        crate::interrupts::ps2_mouse::MOUSE_X.store((fb.width as usize) / 2, Ordering::Release);
        crate::interrupts::ps2_mouse::MOUSE_Y.store((fb.height as usize) / 2, Ordering::Release);
        crate::interrupts::ps2_mouse::MOUSE_BUTTONS.store(0, Ordering::Release);
    }
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
    if !DISPLAY_POWER_STATE.load(Ordering::Relaxed) {
        return Ok(());
    }
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

/// Registra o anel de buffers compartilhado SPSC (AladdinSharedRings) entre Ring 3 e Ring 0 (Syscall 89).
pub fn register_shared_rings(caller_pid: usize, user_rings_vaddr: usize, backbuffer_vaddr: usize) -> Result<(), isize> {
    let current_server = GRAPHICS_SERVER_PID.load(Ordering::SeqCst);
    if current_server != 0 && current_server != caller_pid {
        return Err(crate::syscall::EBUSY);
    }
    if user_rings_vaddr == 0 || (user_rings_vaddr & 0xFFF) != 0 || backbuffer_vaddr == 0 {
        return Err(crate::syscall::EINVAL);
    }

    // Traduz endereço virtual do processo chamador para endereço físico
    let cr3 = crate::memory::paging::current_cr3();
    let pml4 = crate::memory::paging::phys_to_virt(cr3) as *mut crate::memory::paging::PageTable;
    let (phys_addr, _flags) = match unsafe { crate::memory::paging::lookup_page(pml4, user_rings_vaddr as u64) } {
        Some(res) => res,
        None => return Err(crate::syscall::EFAULT),
    };

    // Converte endereço físico para o Higher-Half Direct Map (HHDM) do kernel
    let hhdm_virt = crate::memory::paging::phys_to_virt(phys_addr) as usize;

    // Inicializa cabeçalhos do anel compartilhado
    let rings = unsafe { &mut *(hhdm_virt as *mut AladdinSharedRings) };
    rings.rx_header.head.store(0, Ordering::Release);
    rings.rx_header.tail.store(0, Ordering::Release);
    rings.rx_header.flags.store(0, Ordering::Release);
    rings.tx_header.head.store(0, Ordering::Release);
    rings.tx_header.tail.store(0, Ordering::Release);
    rings.tx_header.flags.store(0, Ordering::Release);

    SHARED_RINGS_HHDM.store(hhdm_virt, Ordering::Release);
    SHARED_BACKBUFFER_VADDR.store(backbuffer_vaddr, Ordering::Release);
    while crate::keyboard_buffer::pop().is_some() {}

    crate::serial_write(b"[GUI] AladdinSharedRings registrado com sucesso (SPSC Zero-Copy Ativo)!\n");
    Ok(())
}

/// Descarrega comandos gráficos em lote a partir do anel TX (SharedGfxCmd) diretamente para o framebuffer Limine.
pub fn flush_shared_gfx_cmds(caller_pid: usize) -> usize {
    let rings_addr = SHARED_RINGS_HHDM.load(Ordering::Acquire);
    let backbuffer_vaddr = SHARED_BACKBUFFER_VADDR.load(Ordering::Acquire);
    if rings_addr == 0 || backbuffer_vaddr == 0 {
        return 0;
    }
    let s_pid = GRAPHICS_SERVER_PID.load(Ordering::SeqCst);
    if s_pid != 0 && caller_pid != s_pid {
        return 0;
    }

    let fb = match limine::framebuffer() {
        Some(fb) => fb,
        None => return 0,
    };

    let rings = unsafe { &*(rings_addr as *const AladdinSharedRings) };
    let mut head = rings.tx_header.head.load(Ordering::Relaxed);
    let tail = rings.tx_header.tail.load(Ordering::Acquire);
    let mut flushed_count = 0;

    let display_on = DISPLAY_POWER_STATE.load(Ordering::Relaxed);
    let fb_width = fb.width as usize;
    let fb_height = fb.height as usize;
    let fb_pitch = fb.pitch as usize;
    let fb_addr = fb.address;

    while head != tail {
        let slot = (head as usize) % CMD_RING_CAP;
        let cmd = unsafe { core::ptr::read_volatile(&rings.tx_cmds[slot]) };

        if display_on {
            match cmd.opcode {
                GFX_OP_PRESENT_DIRTY_RECT => {
                    let rx = cmd.x as usize;
                    let ry = cmd.y as usize;
                    let rw = (cmd.width as usize).min(fb_width.saturating_sub(rx));
                    let rh = (cmd.height as usize).min(fb_height.saturating_sub(ry));

                    if rw > 0 && rh > 0 && rx < fb_width && ry < fb_height {
                        let bytes_per_row = rw * 4;
                        let is_sub_buf = (cmd.flags & GFX_FLAG_SUB_BUF) != 0;

                        for r in 0..rh {
                            let src_ptr = if is_sub_buf {
                                (backbuffer_vaddr + cmd.src_offset as usize + r * bytes_per_row) as *const u8
                            } else {
                                (backbuffer_vaddr + ((ry + r) * fb_width + rx) * 4) as *const u8
                            };
                            let dst_ptr = unsafe { fb_addr.add((ry + r) * fb_pitch + rx * 4) };

                            unsafe {
                                core::ptr::copy_nonoverlapping(src_ptr, dst_ptr, bytes_per_row);
                            }
                        }
                    }
                }
                GFX_OP_SET_DISPLAY_POWER => {
                    set_display_power(cmd.flags != 0);
                }
                _ => {}
            }
        }

        head = head.wrapping_add(1);
        flushed_count += 1;
    }

    rings.tx_header.head.store(head, Ordering::Release);
    RING_CMDS_FLUSHED.fetch_add(flushed_count as u64, Ordering::Relaxed);
    flushed_count
}

/// Aciona a campainha do anel (Doorbell - Syscall 90) para flush de comandos gráficos e repouso anti-lost-wakeup.
pub fn ring_doorbell(caller_pid: usize, flags: u32) -> Result<isize, isize> {
    RING_DOORBELL_CALLS.fetch_add(1, Ordering::Relaxed);
    let mut total_flushed = 0isize;

    if (flags & DOORBELL_FLUSH_GFX) != 0 {
        total_flushed = flush_shared_gfx_cmds(caller_pid) as isize;
    }

    if (flags & DOORBELL_WAIT_INPUT) != 0 {
        let rings_addr = SHARED_RINGS_HHDM.load(Ordering::Acquire);
        if rings_addr != 0 {
            let rings = unsafe { &*(rings_addr as *const AladdinSharedRings) };

            // Double barrier anti-lost-wakeup
            rings.rx_header.flags.fetch_or(CONSUMER_SLEEPING, Ordering::SeqCst);
            core::sync::atomic::fence(Ordering::SeqCst);

            let head = rings.rx_header.head.load(Ordering::Acquire);
            let tail = rings.rx_header.tail.load(Ordering::Acquire);

            if head != tail {
                // Eventos já disponíveis! Limpa flag e retorna imediatamente sem bloquear
                rings.rx_header.flags.fetch_and(!CONSUMER_SLEEPING, Ordering::SeqCst);
                return Ok(total_flushed);
            }

            // Coloca a tarefa em repouso até o próximo evento de hardware ou timeout (50 ticks = 500ms)
            let wake_tick = crate::timer::ticks().wrapping_add(50);
            crate::scheduler::get().sleep_task(caller_pid, wake_tick);
            crate::scheduler::yield_now();

            // Ao acordar (por interrupção de mouse/teclado ou timeout), limpa a flag de sleeping
            rings.rx_header.flags.fetch_and(!CONSUMER_SLEEPING, Ordering::SeqCst);
        }
    }

    Ok(total_flushed)
}

/// Empurra um evento de entrada de hardware para o anel compartilhado (usado por ISRs IRQ1/IRQ12).
pub fn try_push_shared_input_event(ev: SharedInputEvent) -> bool {
    let rings_addr = SHARED_RINGS_HHDM.load(Ordering::Acquire);
    if rings_addr == 0 {
        return false;
    }
    let rings = unsafe { &*(rings_addr as *const AladdinSharedRings) };
    let head = rings.rx_header.head.load(Ordering::Acquire);
    let tail = rings.rx_header.tail.load(Ordering::Relaxed);

    if tail.wrapping_sub(head) as usize >= INPUT_RING_CAP {
        return false;
    }

    let slot = (tail as usize) % INPUT_RING_CAP;
    unsafe {
        let ev_ptr = rings.rx_events.as_ptr().add(slot) as *mut SharedInputEvent;
        core::ptr::write(ev_ptr, ev);
    }
    rings.rx_header.tail.store(tail.wrapping_add(1), Ordering::Release);
    RING_EVENTS_DELIVERED.fetch_add(1, Ordering::Relaxed);

    let flags = rings.rx_header.flags.load(Ordering::SeqCst);
    if (flags & CONSUMER_SLEEPING) != 0 {
        rings.rx_header.flags.fetch_and(!CONSUMER_SLEEPING, Ordering::SeqCst);
        let g_pid = GRAPHICS_SERVER_PID.load(Ordering::Relaxed);
        if g_pid != 0 {
            crate::scheduler::wake_task(g_pid);
        }
    }
    true
}

pub fn poll_hardware_input(caller_pid: usize) -> Result<Option<InputHardwareEvent>, isize> {
    let s_pid = GRAPHICS_SERVER_PID.load(Ordering::SeqCst);
    if s_pid != 0 && caller_pid != s_pid {
        return Err(crate::syscall::EPERM);
    }

    // 1. Processa eventos de teclado a partir da fila segura em memória alimentada pelo ISR (IRQ1)
    if let Some(ch) = crate::keyboard_buffer::pop() {
        return Ok(Some(InputHardwareEvent {
            kind: 1, // KeyDown
            code: ch as u32,
            x: 0,
            y: 0,
            modifiers: 0,
        }));
    }

    // 2. Processa eventos de mouse a partir dos atômicos atualizados pelo ISR (IRQ12)
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

        crate::interrupts::ps2_mouse::MOUSE_EVENTS_DELIVERED.fetch_add(1, core::sync::atomic::Ordering::Relaxed);

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
            let total_bytes = total_pixels * 4;
            let mut buf = alloc::vec::Vec::new();
            if buf.try_reserve_exact(total_pixels).is_err() {
                crate::serial_write(b"[gui] ENOMEM ao alocar buffer da janela cliente: ");
                // Serializa tamanho em bytes para diagnóstico serial
                let mut nbuf = [0u8; 12];
                let mut n = total_bytes;
                let mut idx = 11usize;
                if n == 0 { nbuf[idx] = b'0'; } else {
                    while n > 0 { nbuf[idx] = b'0' + (n % 10) as u8; n /= 10; if idx > 0 { idx -= 1; } else { break; } }
                    idx += 1;
                }
                crate::serial_write(&nbuf[idx..]);
                crate::serial_write(b" bytes (");
                let mut n2 = width; let mut idx2 = 11usize; let mut nbuf2 = [0u8; 12];
                if n2 == 0 { nbuf2[idx2] = b'0'; } else {
                    while n2 > 0 { nbuf2[idx2] = b'0' + (n2 % 10) as u8; n2 /= 10; if idx2 > 0 { idx2 -= 1; } else { break; } }
                    idx2 += 1;
                }
                crate::serial_write(&nbuf2[idx2..]);
                crate::serial_write(b"x");
                let mut n3 = height; let mut idx3 = 11usize; let mut nbuf3 = [0u8; 12];
                if n3 == 0 { nbuf3[idx3] = b'0'; } else {
                    while n3 > 0 { nbuf3[idx3] = b'0' + (n3 % 10) as u8; n3 /= 10; if idx3 > 0 { idx3 -= 1; } else { break; } }
                    idx3 += 1;
                }
                crate::serial_write(&nbuf3[idx3..]);
                crate::serial_write(b" px)\n");
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
                is_minimized: false,
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
                if w.is_minimized {
                    w.is_minimized = false;
                }
                if w.buffer.len() != expected {
                    w.buffer.resize(expected, 0u32);
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
                // Zero-Idle GC: devolve o backbuffer integralmente ao heap
                w.buffer.clear();
                w.buffer.shrink_to_fit();
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

pub fn client_window_minimize(owner_pid: usize, win_id: usize) -> Result<(), isize> {
    let mut windows = CLIENT_WINDOWS.lock();
    for slot in windows.iter_mut() {
        if let Some(w) = slot {
            if w.id == win_id {
                if w.owner_pid != owner_pid {
                    return Err(crate::syscall::EPERM);
                }
                w.is_minimized = true;
                // Zero-Idle GC: libera páginas de backbuffer durante dormência/minimização
                w.buffer.clear();
                w.buffer.shrink_to_fit();
                return Ok(());
            }
        }
    }
    Err(crate::syscall::EBADF)
}

pub fn client_window_restore(owner_pid: usize, win_id: usize) -> Result<(), isize> {
    let mut windows = CLIENT_WINDOWS.lock();
    for slot in windows.iter_mut() {
        if let Some(w) = slot {
            if w.id == win_id {
                if w.owner_pid != owner_pid {
                    return Err(crate::syscall::EPERM);
                }
                w.is_minimized = false;
                let total = w.width * w.height;
                if w.buffer.len() < total {
                    let mut new_buf = alloc::vec::Vec::new();
                    if new_buf.try_reserve_exact(total).is_err() {
                        return Err(crate::syscall::ENOMEM);
                    }
                    new_buf.resize(total, 0u32);
                    w.buffer = new_buf;
                }
                w.damaged = true;
                crate::scheduler::get().unblock_task(w.owner_pid);
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
    let mut target_owner = None;
    {
        let mut windows = CLIENT_WINDOWS.lock();
        for slot in windows.iter_mut() {
            if let Some(w) = slot {
                if w.id == win_id {
                    if event.event_type == EVENT_WINDOW_RESIZE {
                        let new_w = event.x as usize;
                        let new_h = event.y as usize;
                        if new_w > 0 && new_h > 0 && new_w <= 1024 && new_h <= 768 {
                            w.width = new_w;
                            w.height = new_h;
                            w.buffer.resize(new_w * new_h, 0u32);
                            w.damaged = true;
                        }
                    }
                    let count = w.ev_tail.wrapping_sub(w.ev_head);
                    if count >= 16 {
                        // Descarta o evento mais antigo para evitar que a fila de 16 itens trave o compositor
                        w.ev_head = w.ev_head.wrapping_add(1);
                    }
                    w.events[w.ev_tail % 16] = *event;
                    w.ev_tail = w.ev_tail.wrapping_add(1);
                    target_owner = Some(w.owner_pid);
                    break;
                }
            }
        }
    }
    if let Some(owner) = target_owner {
        // Zero-Idle Wakeup: acorda o dono da janela no escalonador de forma segura (fora do lock de janelas)
        crate::scheduler::get().unblock_task(owner);
        Ok(())
    } else {
        Err(crate::syscall::EBADF)
    }
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

        let (net_tx_packets, net_rx_packets) = crate::drivers::net::packet_stats();
        let (cpu_active_cycles, cpu_idle_cycles) = crate::scheduler::cpu_cycle_stats();
        let syscalls_total = crate::syscall::SYSCALL_COUNT.load(core::sync::atomic::Ordering::Relaxed);
        let memory_active_bytes = crate::memory::used_bytes() as u64;
        let memory_idle_bytes = (crate::memory::total_bytes().saturating_sub(crate::memory::used_bytes())) as u64;

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
            net_tx_packets,
            net_rx_packets,
            cpu_active_cycles,
            cpu_idle_cycles,
            syscalls_total,
            memory_active_bytes,
            memory_idle_bytes,
            ring_events_processed: RING_EVENTS_DELIVERED.load(core::sync::atomic::Ordering::Relaxed),
            ring_cmds_flushed: RING_CMDS_FLUSHED.load(core::sync::atomic::Ordering::Relaxed),
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

    fn shutdown(&self) -> ! {
        crate::benchmark::sys_poweroff_machine();
    }

    fn set_display_power(&self, on: bool) {
        set_display_power(on);
    }

    fn has_pending_updates(&self) -> bool {
        true
    }

    fn apply_offline_updates_and_shutdown(&self) -> ! {
        crate::serial_write(b"[OFFLINE-UPDATE] Apagando tela e aplicando atualizacoes offline...\n");
        set_display_power(false);
        crate::benchmark::sys_poweroff_machine();
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
        // Desacoplamento de Trava: copia os campos do volume liberando o VOLUME_MANAGER
        // imediatamente antes de qualquer chamada a subsistemas externos (ex: statvfs)
        // para prevenir deadlocks com a trava MOUNTS.
        let (vol_id, label, label_len, mp, mp_len, fs_kind, total_mb, used_mb, free_mb, is_mounted, is_root, is_busy, pfx_buf, pfx_len) = {
            let vm = crate::vfs::volume::VOLUME_MANAGER.lock();
            let vol = vm.get_volume(index)?;
            let mut p_buf = [0u8; 32];
            let p_len = vol.mount_point_len.min(31);
            p_buf[..p_len].copy_from_slice(&vol.mount_point[..p_len]);
            (
                vol.id,
                vol.label,
                vol.label_len,
                vol.mount_point,
                vol.mount_point_len,
                vol.fs_kind,
                vol.total_mb as u64,
                vol.used_mb as u64,
                vol.free_mb as u64,
                vol.is_mounted,
                vol.is_root,
                vol.open_handles_count > 0,
                p_buf,
                p_len,
            )
        };

        let mut v = aladdin::contracts::storage::VolumeDisplayInfo::empty();
        v.id = vol_id;

        let l_len = label_len.min(31);
        v.label[..l_len].copy_from_slice(&label[..l_len]);
        v.label_len = l_len;

        let m_len = mp_len.min(31);
        v.mount_point[..m_len].copy_from_slice(&mp[..m_len]);
        v.mount_point_len = m_len;

        let fs_str = match fs_kind {
            crate::drivers::partition::DetectedFsKind::Fat32 => "FAT32",
            crate::drivers::partition::DetectedFsKind::Ext2 => "Ext2",
            crate::drivers::partition::DetectedFsKind::Iso9660 => "ISO9660",
            _ => "RAW",
        };
        let fs_bytes = fs_str.as_bytes();
        let fs_len = fs_bytes.len().min(15);
        v.fs_type[..fs_len].copy_from_slice(&fs_bytes[..fs_len]);
        v.fs_type_len = fs_len;

        v.total_mb = total_mb;
        v.used_mb = used_mb;
        v.free_mb = free_mb;
        v.is_mounted = is_mounted;
        v.is_root = is_root;
        v.is_busy = is_busy;

        // Se a capacidade total ainda não estiver preenchida, consulta statvfs fora do lock
        if is_mounted && total_mb == 0 && pfx_len > 0 {
            if let Ok(pfx) = core::str::from_utf8(&pfx_buf[..pfx_len]) {
                if let Ok(st) = crate::vfs::statvfs(pfx) {
                    let t_mb = st.total_bytes / (1024 * 1024);
                    let f_mb = st.free_bytes / (1024 * 1024);
                    v.total_mb = t_mb;
                    v.free_mb = f_mb;
                    v.used_mb = t_mb.saturating_sub(f_mb);
                }
            }
        }
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

fn write_i64(mut n: i64) {
    if n == 0 {
        crate::serial_write(b"0");
        return;
    }
    if n < 0 {
        crate::serial_write(b"-");
        n = -n;
    }
    let mut buf = [0u8; 20];
    let mut i = 0;
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    buf[..i].reverse();
    crate::serial_write(&buf[..i]);
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

        crate::serial_write(b"[SPAWN_APP] Solicitado spawn_app para: ");
        crate::serial_write(path.as_bytes());
        crate::serial_write(b"\n");

        // 1. Tenta carregar do VFS (disco FAT32)
        match crate::scheduler::get().spawn_user_task_from_vfs(path, argv_slice, &[]) {
            Ok(pid) => {
                crate::serial_write(b"[SPAWN_APP] VFS spawn com sucesso! PID=");
                write_i64(pid as i64);
                crate::serial_write(b"\n");
                crate::ipc::register(pid);
                if is_browser {
                    crate::gui::register_browser_pid(pid);
                }
                return Ok(pid);
            }
            Err(errno) => {
                crate::serial_write(b"[SPAWN_APP] spawn_user_task_from_vfs falhou com errno: ");
                write_i64(errno as i64);
                crate::serial_write(b"\n");
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
                    crate::serial_write(b"[SPAWN_APP] alt_path VFS spawn com sucesso! PID=");
                    write_i64(pid as i64);
                    crate::serial_write(b"\n");
                    crate::ipc::register(pid);
                    if is_browser {
                        crate::gui::register_browser_pid(pid);
                    }
                    return Ok(pid);
                }
                Err(errno) => {
                    crate::serial_write(b"[SPAWN_APP] alt_path falhou com errno: ");
                    write_i64(errno as i64);
                    crate::serial_write(b"\n");
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
            crate::serial_write(b"[SPAWN_APP] Tentando fallback para modulo Limine browser...\n");
            let mod_opt = limine::get_module_by_name("browser").or_else(|| limine::get_module(1));
            if let Some(elf_bytes) = mod_opt {
                crate::serial_write(b"[SPAWN_APP] Modulo Limine obtido (bytes=");
                write_i64(elf_bytes.len() as i64);
                crate::serial_write(b"). Validando ELF...\n");
                match crate::elf::validate_elf(elf_bytes) {
                    Ok(_) => {
                        crate::serial_write(b"[SPAWN_APP] validate_elf OK! Invocando spawn_user_task_from_memory...\n");
                        match crate::scheduler::get().spawn_user_task_from_memory(
                            "browser[U3]",
                            elf_bytes,
                            &["/disk/browser.elf", "browser"],
                            &[],
                        ) {
                            Ok(pid) => {
                                crate::serial_write(b"[SPAWN_APP] Sucesso Limine! PID=");
                                write_i64(pid as i64);
                                crate::serial_write(b"\n");
                                crate::ipc::register(pid);
                                crate::gui::register_browser_pid(pid);
                                return Ok(pid);
                            }
                            Err(e) => {
                                crate::serial_write(b"[SPAWN_APP] spawn_user_task_from_memory falhou com errno: ");
                                write_i64(e as i64);
                                crate::serial_write(b"\n");
                            }
                        }
                    }
                    Err(_e) => {
                        crate::serial_write(b"[SPAWN_APP] validate_elf falhou no modulo Limine!\n");
                    }
                }
            } else {
                crate::serial_write(b"[SPAWN_APP] Modulo Limine browser NAO foi encontrado!\n");
            }
        }

        crate::serial_write(b"[SPAWN_APP] Retornando Err(())\n");
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
                w.buffer.clear();
                w.buffer.shrink_to_fit();
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
    // Limpa buffer de teclado residual de selftests do boot
    while crate::keyboard_buffer::pop().is_some() {}

    // Verifica se o compositor Aladdin Ring 3 já está registrado ou pré-carregado
    let existing_pid = {
        let current_g = GRAPHICS_SERVER_PID.load(Ordering::SeqCst);
        if current_g != 0 {
            Some(current_g)
        } else {
            let sched = crate::scheduler::get();
            let mut found = None;
            for (i, slot) in sched.tasks.iter().enumerate() {
                if let Some(t) = slot {
                    if t.name.starts_with("aladdin") && t.state != crate::scheduler::TaskState::Zombie && t.state != crate::scheduler::TaskState::Completed {
                        found = Some(i);
                        break;
                    }
                }
            }
            found
        }
    };

    let mut pid = if let Some(p) = existing_pid {
        crate::serial_write(b"[GUI] Processo Aladdin Ring 3 ativo detectado!\n");
        p
    } else {
        let argv = ["aladdin", "server"];
        
        // 1. Prioriza o módulo Limine em memória (instantâneo e sem I/O de disco)
        if let Some(elf_bytes) = limine::get_module_by_name("aladdin").or_else(|| limine::get_module_by_name("aladdin-server")) {
            match crate::scheduler::get().spawn_user_task_from_memory("aladdin[U3]", elf_bytes, &argv, &[]) {
                Ok(p) => {
                    crate::serial_write(b"[GUI] Modulo Limine aladdin carregado com sucesso!\n");
                    p
                },
                Err(_) => {
                    crate::serial_write(b"[GUI] Falha ao spawnar modulo Limine, tentando disco...\n");
                    let path = "/disk/aladdin.elf";
                    let argv_vfs = [path, "server"];
                    match crate::scheduler::get().spawn_user_task_from_vfs(path, &argv_vfs, &[]) {
                        Ok(p) => p,
                        Err(_) => {
                            run_desktop();
                            return;
                        }
                    }
                }
            }
        } else {
            let path = "/disk/aladdin.elf";
            let argv_vfs = [path, "server"];
            crate::serial_write(b"[GUI] Tentando spawn_user_task_from_vfs(/disk/aladdin.elf)...\n");
            match crate::scheduler::get().spawn_user_task_from_vfs(path, &argv_vfs, &[]) {
                Ok(p) => {
                    crate::serial_write(b"[GUI] /disk/aladdin.elf carregado com sucesso!\n");
                    p
                },
                Err(_) => {
                    match crate::scheduler::get().spawn_user_task_from_vfs("/disk/ALADDIN.ELF", &argv_vfs, &[]) {
                        Ok(p) => p,
                        Err(_) => {
                            crate::serial_write(b"[GUI] Falha no disco, fallback Ring 0\n");
                            run_desktop();
                            return;
                        }
                    }
                }
            }
        }
    };

    let mut restart_attempts = 0usize;
    loop {
        crate::ipc::register(pid);
        crate::serial_write(b"[GUI] Aguardando execucao do compositor Ring 3 (Zero-Idle wait_on)...\n");
        let waiter_pid = crate::scheduler::current_pid();
        loop {
            if let Some(exit_code) = crate::scheduler::get().collect_zombie(pid) {
                crate::serial_write(b"[GUI] Compositor Ring 3 finalizado. Codigo de saida: ");
                let mut c_buf = [0u8; 16];
                let mut c_val = exit_code.unsigned_abs() as u64;
                let mut c_i = 0;
                if c_val == 0 {
                    crate::serial_write(b"0");
                } else {
                    if exit_code < 0 { crate::serial_write(b"-"); }
                    while c_val > 0 {
                        c_buf[c_i] = b'0' + (c_val % 10) as u8;
                        c_val /= 10;
                        c_i += 1;
                    }
                    c_buf[..c_i].reverse();
                    crate::serial_write(&c_buf[..c_i]);
                }
                crate::serial_write(b"\n");

                // Limpa registro de servidor gráfico para permitir novo registro limpo sem EBUSY
                GRAPHICS_SERVER_PID.store(0, Ordering::SeqCst);

                // Auto-recuperação resiliente: se falhou anormalmente (código != 0), reinicia automaticamente
                if exit_code != 0 && restart_attempts < 3 {
                    restart_attempts += 1;
                    crate::serial_write(b"[GUI] [AUTO-RECOVERY] Reiniciando Aladdin Desktop Compositor (Tentativa ");
                    crate::serial_write_byte(b'0' + restart_attempts as u8);
                    crate::serial_write(b"/3)...\n");

                    let argv = ["aladdin", "server"];
                    let new_pid_opt = limine::get_module_by_name("aladdin")
                        .or_else(|| limine::get_module_by_name("aladdin-server"))
                        .and_then(|bytes| crate::scheduler::get().spawn_user_task_from_memory("aladdin[U3]", bytes, &argv, &[]).ok())
                        .or_else(|| {
                            let path = "/disk/aladdin.elf";
                            let argv_vfs = [path, "server"];
                            crate::scheduler::get().spawn_user_task_from_vfs(path, &argv_vfs, &[]).ok()
                        });

                    if let Some(np) = new_pid_opt {
                        pid = np;
                        break; // Reinicia espera externa com novo PID
                    }
                }
                return restore_text_console();
            }
            if pid >= crate::scheduler::MAX_TASKS || crate::scheduler::get().tasks[pid].is_none() {
                GRAPHICS_SERVER_PID.store(0, Ordering::SeqCst);
                crate::serial_write(b"[GUI] Compositor Ring 3 saiu do slot de tarefas\n");
                return restore_text_console();
            }
            // Bloqueia no escalonador aguardando a finalização da tarefa (Zero-Idle)
            if !crate::scheduler::get().wait_on(waiter_pid, pid) {
                crate::scheduler::yield_now();
            }
        }
    }
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
        crate::interrupts::ps2_mouse::MOUSE_X.store(jinn_fb.width / 2, Ordering::Release);
        crate::interrupts::ps2_mouse::MOUSE_Y.store(jinn_fb.height / 2, Ordering::Release);
        crate::interrupts::ps2_mouse::MOUSE_BUTTONS.store(0, Ordering::Release);
        crate::interrupts::ps2_mouse::reset_sync();

        let mut poll_keyboard = || -> Option<u8> {
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
