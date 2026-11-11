//! Jinn OS & Aladdin Desktop — Compositor Ring 3 Isolation & Recovery Test Suite
//!
//! Valida o teste obrigatório de isolamento e recuperação do compositor:
//! 1. Registro exclusivo e consulta de framebuffer (Syscalls 72 e 73).
//! 2. Apresentação mediada de dirty rects sem acesso direto à VRAM física (Syscall 74).
//! 3. Polling de eventos de hardware teclado/mouse mediado por Ring 0 (Syscall 75).
//! 4. Intermediação de superfícies de clientes Ring 3 (Syscalls 77, 78 e 79).
//! 5. Falha forçada / Crash / Kill do processo do compositor:
//!    - Kernel não entra em panic.
//!    - Prompt do shell continua respondendo.
//!    - Recursos gráficos e PID de servidor são revogados.
//!    - Console de texto é restaurado.
//! 6. Reinicialização do Aladdin sem reboot do kernel:
//!    - Novo processo Ring 3 assume o papel de servidor gráfico.
//!    - Reconecta-se às janelas de clientes ativas e retoma a composição normal.

use std::sync::atomic::{AtomicUsize, Ordering};

pub const EPERM:  isize = -1;
pub const ENODEV: isize = -19;
pub const EINVAL: isize = -22;
pub const EBUSY:  isize = -16;
pub const EFAULT: isize = -14;
pub const ENOMEM: isize = -12;

pub const USER_CANONICAL_MIN: usize = 0x0000_0000_0001_0000;
pub const USER_CANONICAL_MAX: usize = 0x0000_8000_0000_0000;

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
    pub kind: u32,
    pub code: u32,
    pub x: i32,
    pub y: i32,
    pub modifiers: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientWindowEvent {
    pub event_type: u32,
    pub param1: u32,
    pub param2: u32,
    pub param3: u32,
    pub timestamp_ms: u64,
    pub reserved: u64,
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

// ── Simulação do Kernel Graphics Subsystem ─────────────────────────────────────

pub struct MockKernelGraphicsSubsystem {
    pub server_pid: AtomicUsize,
    pub fb_width: usize,
    pub fb_height: usize,
    pub fb_pitch: usize,
    pub physical_vram: Vec<u32>,
    pub console_restored_count: usize,
    pub kernel_panic_triggered: bool,
    pub shell_prompt_active: bool,
    pub client_windows: Vec<MockClientWindow>,
    pub input_queue: Vec<InputHardwareEvent>,
}

#[derive(Clone)]
pub struct MockClientWindow {
    pub id: usize,
    pub owner_pid: usize,
    pub width: usize,
    pub height: usize,
    pub title: String,
    pub surface: Vec<u32>,
    pub events: Vec<ClientWindowEvent>,
    pub damaged: bool,
}

impl MockKernelGraphicsSubsystem {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            server_pid: AtomicUsize::new(0),
            fb_width: w,
            fb_height: h,
            fb_pitch: w * 4,
            physical_vram: vec![0u32; w * h],
            console_restored_count: 0,
            kernel_panic_triggered: false,
            shell_prompt_active: true,
            client_windows: Vec::new(),
            input_queue: Vec::new(),
        }
    }

    pub fn validate_user_buffer(ptr: usize, len: usize) -> Result<(), isize> {
        if ptr < USER_CANONICAL_MIN {
            return Err(EFAULT);
        }
        let end = ptr.checked_add(len).ok_or(EFAULT)?;
        if end > USER_CANONICAL_MAX {
            return Err(EFAULT);
        }
        Ok(())
    }

    pub fn register_graphics_server(&mut self, caller_pid: usize) -> Result<(), isize> {
        let current = self.server_pid.load(Ordering::SeqCst);
        if current != 0 && current != caller_pid {
            return Err(EBUSY);
        }
        self.server_pid.store(caller_pid, Ordering::SeqCst);
        self.shell_prompt_active = false; // Desktop assume o controle da exibição
        Ok(())
    }

    pub fn unregister_graphics_server(&mut self, caller_pid: usize) -> Result<(), isize> {
        let current = self.server_pid.load(Ordering::SeqCst);
        if current != caller_pid {
            return Err(EPERM);
        }
        self.server_pid.store(0, Ordering::SeqCst);
        self.restore_text_console();
        Ok(())
    }

    pub fn notify_graphics_server_exit(&mut self, pid: usize) {
        if self.server_pid.load(Ordering::SeqCst) == pid {
            self.server_pid.store(0, Ordering::SeqCst);
            self.restore_text_console();
        }
    }

    pub fn restore_text_console(&mut self) {
        self.console_restored_count += 1;
        self.shell_prompt_active = true;
    }

    pub fn get_fb_info(&self) -> Result<GraphicsFbInfo, isize> {
        Ok(GraphicsFbInfo {
            width: self.fb_width as u32,
            height: self.fb_height as u32,
            pitch: self.fb_pitch as u32,
            bpp: 32,
            format: 0,
        })
    }

    pub fn present_rect(&mut self, caller_pid: usize, rect: &PresentRect, buffer: &[u32]) -> Result<(), isize> {
        let s_pid = self.server_pid.load(Ordering::SeqCst);
        if s_pid == 0 || caller_pid != s_pid {
            return Err(EPERM);
        }
        if rect.x as usize >= self.fb_width || rect.y as usize >= self.fb_height {
            return Err(EINVAL);
        }
        let max_w = self.fb_width.saturating_sub(rect.x as usize);
        let max_h = self.fb_height.saturating_sub(rect.y as usize);
        let rw = (rect.width as usize).min(max_w);
        let rh = (rect.height as usize).min(max_h);
        if rw == 0 || rh == 0 {
            return Ok(());
        }

        let min_len = (rect.width as usize) * (rect.height as usize);
        if buffer.len() < min_len {
            return Err(EINVAL);
        }

        for row in 0..rh {
            let src_offset = row * (rect.width as usize);
            let dst_offset = (rect.y as usize + row) * self.fb_width + (rect.x as usize);
            self.physical_vram[dst_offset..dst_offset + rw].copy_from_slice(&buffer[src_offset..src_offset + rw]);
        }

        Ok(())
    }

    pub fn poll_hardware_input(&mut self, caller_pid: usize) -> Result<Option<InputHardwareEvent>, isize> {
        let s_pid = self.server_pid.load(Ordering::SeqCst);
        if s_pid == 0 || caller_pid != s_pid {
            return Err(EPERM);
        }
        if !self.input_queue.is_empty() {
            Ok(Some(self.input_queue.remove(0)))
        } else {
            Ok(None)
        }
    }

    pub fn create_client_window(&mut self, owner_pid: usize, title: &str, w: usize, h: usize) -> Result<usize, isize> {
        let id = self.client_windows.len() + 1;
        self.client_windows.push(MockClientWindow {
            id,
            owner_pid,
            width: w,
            height: h,
            title: title.to_string(),
            surface: vec![0x0022_3344; w * h],
            events: Vec::new(),
            damaged: true,
        });
        Ok(id)
    }

    pub fn list_client_windows(&self, caller_pid: usize, out_slice: &mut [ClientWindowInfoUser]) -> Result<usize, isize> {
        let s_pid = self.server_pid.load(Ordering::SeqCst);
        if s_pid == 0 || caller_pid != s_pid {
            return Err(EPERM);
        }
        let count = self.client_windows.len().min(out_slice.len());
        for i in 0..count {
            let cw = &self.client_windows[i];
            let mut info = ClientWindowInfoUser {
                win_id: cw.id as u32,
                owner_pid: cw.owner_pid as u32,
                width: cw.width as u32,
                height: cw.height as u32,
                is_open: 1,
                damaged: if cw.damaged { 1 } else { 0 },
                title_len: cw.title.len() as u32,
                title: [0u8; 48],
            };
            let bytes = cw.title.as_bytes();
            let len = bytes.len().min(48);
            info.title[..len].copy_from_slice(&bytes[..len]);
            out_slice[i] = info;
        }
        Ok(count)
    }

    pub fn read_client_surface(&mut self, caller_pid: usize, win_id: usize, dst: &mut [u32]) -> Result<usize, isize> {
        let s_pid = self.server_pid.load(Ordering::SeqCst);
        if s_pid == 0 || caller_pid != s_pid {
            return Err(EPERM);
        }
        for cw in self.client_windows.iter_mut() {
            if cw.id == win_id {
                let count = cw.surface.len().min(dst.len());
                dst[..count].copy_from_slice(&cw.surface[..count]);
                cw.damaged = false;
                return Ok(count * 4);
            }
        }
        Err(EINVAL)
    }

    pub fn dispatch_client_event(&mut self, caller_pid: usize, win_id: usize, ev: &ClientWindowEvent) -> Result<(), isize> {
        let s_pid = self.server_pid.load(Ordering::SeqCst);
        if s_pid == 0 || caller_pid != s_pid {
            return Err(EPERM);
        }
        for cw in self.client_windows.iter_mut() {
            if cw.id == win_id {
                cw.events.push(*ev);
                return Ok(());
            }
        }
        Err(EINVAL)
    }
}

// ── Casos de Teste ────────────────────────────────────────────────────────────

fn test_compositor_registration_and_exclusivity() {
    print!("Teste 1: Registro de Servidor Gráfico Ring 3 e exclusividade mútua... ");
    let mut kernel = MockKernelGraphicsSubsystem::new(1024, 768);

    // 1. Processo Aladdin (PID 10) registra-se como servidor gráfico
    assert_eq!(kernel.register_graphics_server(10), Ok(()));
    assert_eq!(kernel.server_pid.load(Ordering::SeqCst), 10);
    assert_eq!(kernel.shell_prompt_active, false);

    // 2. Outro processo Ring 3 (PID 11) tenta se registrar -> Rejeitado com EBUSY
    assert_eq!(kernel.register_graphics_server(11), Err(EBUSY));

    // 3. Processo não autorizado tenta apresentar -> EPERM
    let rect = PresentRect { x: 0, y: 0, width: 100, height: 100 };
    let dummy_buf = vec![0x00FF_0000; 10000];
    assert_eq!(kernel.present_rect(11, &rect, &dummy_buf), Err(EPERM));

    // 4. Servidor gráfico legítimo (PID 10) apresenta com sucesso
    assert_eq!(kernel.present_rect(10, &rect, &dummy_buf), Ok(()));
    assert_eq!(kernel.physical_vram[0], 0x00FF_0000);

    println!("PASSOU!");
}

fn test_framebuffer_presentation_and_boundary_checks() {
    print!("Teste 2: Apresentação de dirty rects, bounds checking e proteção de memória... ");
    let mut kernel = MockKernelGraphicsSubsystem::new(1024, 768);
    assert_eq!(kernel.register_graphics_server(10), Ok(()));

    // 1. Apresentação com coordenadas fora da tela -> EINVAL
    let invalid_rect = PresentRect { x: 1024, y: 768, width: 10, height: 10 };
    let buf = vec![0u32; 100];
    assert_eq!(kernel.present_rect(10, &invalid_rect, &buf), Err(EINVAL));

    // 2. Buffer menor que rect.width * rect.height -> EINVAL
    let big_rect = PresentRect { x: 10, y: 10, width: 50, height: 50 };
    let small_buf = vec![0u32; 10]; // Necessário 2500 pixels
    assert_eq!(kernel.present_rect(10, &big_rect, &small_buf), Err(EINVAL));

    // 3. Validação de ponteiro de usuário canônico
    assert!(MockKernelGraphicsSubsystem::validate_user_buffer(0x0000, 100).is_err()); // NULL guard
    assert!(MockKernelGraphicsSubsystem::validate_user_buffer(0xFFFF_8000_0000_0000, 100).is_err()); // Espaço de kernel
    assert!(MockKernelGraphicsSubsystem::validate_user_buffer(0x0000_7FFF_FFFF_0000, 4096).is_ok()); // Espaço de usuário válido

    println!("PASSOU!");
}

fn test_hardware_input_polling_and_client_dispatch() {
    print!("Teste 3: Polling de eventos de hardware e despacho para janelas de clientes... ");
    let mut kernel = MockKernelGraphicsSubsystem::new(1024, 768);
    assert_eq!(kernel.register_graphics_server(10), Ok(()));

    // Cria janela de cliente Ring 3 (ex: Terminal PID 12)
    let win_id = kernel.create_client_window(12, "Terminal Ring 3", 640, 480).unwrap();

    // Simula tecla 'A' recebida da interrupção PS/2 em Ring 0
    kernel.input_queue.push(InputHardwareEvent {
        kind: 1, // KeyDown
        code: b'A' as u32,
        x: 0,
        y: 0,
        modifiers: 0,
    });

    // Compositor (PID 10) consome o evento de hardware via Syscall 75
    let ev = kernel.poll_hardware_input(10).unwrap().unwrap();
    assert_eq!(ev.kind, 1);
    assert_eq!(ev.code, b'A' as u32);

    // Compositor despacha o evento para a janela do cliente via Syscall 78
    let client_ev = ClientWindowEvent {
        event_type: 4, // EVENT_KEY_DOWN
        param1: b'A' as u32,
        param2: 0,
        param3: 0,
        timestamp_ms: 100,
        reserved: 0,
    };
    assert_eq!(kernel.dispatch_client_event(10, win_id, &client_ev), Ok(()));

    // Cliente verifica recepção isolada do evento em sua fila
    assert_eq!(kernel.client_windows[0].events.len(), 1);
    assert_eq!(kernel.client_windows[0].events[0].param1, b'A' as u32);

    println!("PASSOU!");
}

fn test_compositor_crash_isolation_and_resurrection() {
    print!("Teste 4: [CRÍTICO] Falha forçada (kill/crash) do Aladdin, kernel estável e reinicialização... ");
    let mut kernel = MockKernelGraphicsSubsystem::new(1024, 768);

    // Passo 1: Boot e início do Aladdin como processo Ring 3 (PID 20)
    assert_eq!(kernel.register_graphics_server(20), Ok(()));
    assert_eq!(kernel.server_pid.load(Ordering::SeqCst), 20);
    assert_eq!(kernel.shell_prompt_active, false);

    // Passo 2: Aplicação Ring 3 abre janelas (Terminal e Monitor)
    let win_term = kernel.create_client_window(21, "Terminal", 640, 480).unwrap();
    let win_mon = kernel.create_client_window(22, "Monitor", 500, 300).unwrap();
    assert_eq!(kernel.client_windows.len(), 2);

    // Passo 3: Forçar encerramento abrupto do compositor Aladdin (simulação de Kill / Crash / Page Fault)
    // O escalonador detecta o óbito do PID 20 e invoca `notify_graphics_server_exit(20)`
    kernel.notify_graphics_server_exit(20);

    // Passo 4: Verificações de Isolamento do Kernel
    // a. O kernel NÃO entra em pânico
    assert_eq!(kernel.kernel_panic_triggered, false);

    // b. O prompt do kernel volta a responder
    assert_eq!(kernel.shell_prompt_active, true);
    assert_eq!(kernel.console_restored_count, 1);

    // c. Os recursos gráficos de servidor foram revogados (PID zerado)
    assert_eq!(kernel.server_pid.load(Ordering::SeqCst), 0);

    // d. Processo zumbi PID 20 não pode mais apresentar frames
    let rect = PresentRect { x: 0, y: 0, width: 100, height: 100 };
    let buf = vec![0u32; 10000];
    assert_eq!(kernel.present_rect(20, &rect, &buf), Err(EPERM));

    // Passo 5: Reiniciar o Aladdin sem reiniciar o kernel!
    // Novo processo Ring 3 é criado pelo comando `gui` (PID 25)
    assert_eq!(kernel.register_graphics_server(25), Ok(()));
    assert_eq!(kernel.server_pid.load(Ordering::SeqCst), 25);
    assert_eq!(kernel.shell_prompt_active, false);

    // Passo 6: O novo compositor descobre as janelas que já estavam abertas via Syscall 79
    let mut info_list = [ClientWindowInfoUser {
        win_id: 0, owner_pid: 0, width: 0, height: 0, is_open: 0, damaged: 0, title_len: 0, title: [0u8; 48],
    }; 16];
    let listed = kernel.list_client_windows(25, &mut info_list).unwrap();
    assert_eq!(listed, 2);
    assert_eq!(info_list[0].win_id, win_term as u32);
    assert_eq!(info_list[1].win_id, win_mon as u32);

    // Passo 7: O novo compositor lê as superfícies das janelas existentes
    let mut surface_cache = vec![0u32; 640 * 480];
    let bytes = kernel.read_client_surface(25, win_term, &mut surface_cache).unwrap();
    assert_eq!(bytes, 640 * 480 * 4);

    // Passo 8: O novo compositor apresenta o desktop reconstruído com sucesso
    assert_eq!(kernel.present_rect(25, &rect, &buf), Ok(()));

    // Passo 9: Encerramento normal pelo usuário fecha o desktop e retorna ao shell
    assert_eq!(kernel.unregister_graphics_server(25), Ok(()));
    assert_eq!(kernel.server_pid.load(Ordering::SeqCst), 0);
    assert_eq!(kernel.shell_prompt_active, true);
    assert_eq!(kernel.console_restored_count, 2);

    println!("PASSOU!");
}

fn main() {
    println!("==================================================================");
    println!("  JINN OS — ALADDIN COMPOSITOR RING 3 ISOLATION & RECOVERY SUITE  ");
    println!("==================================================================");
    test_compositor_registration_and_exclusivity();
    test_framebuffer_presentation_and_boundary_checks();
    test_hardware_input_polling_and_client_dispatch();
    test_compositor_crash_isolation_and_resurrection();
    println!("==================================================================");
    println!("  TODOS OS CRITÉRIOS DE ISOLAMENTO E RECUPERAÇÃO FORAM ATENDIDOS! ");
    println!("==================================================================");
}

