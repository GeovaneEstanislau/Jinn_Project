//! Aladdin Desktop SDK — Biblioteca de Suporte para Aplicativos de Usuário (Ring 3)
//!
//! Fornece interface idiomática em Rust `no_std` para chamadas de sistema (syscalls)
//! do Jinn OS e integração gráfica com o compositor do Aladdin Desktop.

#![no_std]

use core::arch::asm;

// ── Primitivas de Syscall em x86-64 ──────────────────────────────────────────

#[inline(always)]
pub unsafe fn syscall0(nr: u64) -> i64 {
    let ret: i64;
    asm!(
        "int 0x80",
        inout("rax") nr => ret,
        options(nostack)
    );
    ret
}

#[inline(always)]
pub unsafe fn syscall1(nr: u64, arg0: u64) -> i64 {
    let ret: i64;
    asm!(
        "int 0x80",
        inout("rax") nr => ret,
        in("rdi") arg0,
        options(nostack)
    );
    ret
}

#[inline(always)]
pub unsafe fn syscall2(nr: u64, arg0: u64, arg1: u64) -> i64 {
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

#[inline(always)]
pub unsafe fn syscall3(nr: u64, arg0: u64, arg1: u64, arg2: u64) -> i64 {
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
pub unsafe fn syscall4(nr: u64, arg0: u64, arg1: u64, arg2: u64, arg3: u64) -> i64 {
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

// ── Constantes de Syscalls e Eventos ─────────────────────────────────────────

pub const SYS_EXIT:              u64 = 0;
pub const SYS_WRITE:             u64 = 1;
pub const SYS_READ:              u64 = 2;
pub const SYS_YIELD:             u64 = 3;
pub const SYS_GETPID:            u64 = 4;
pub const SYS_SLEEP:             u64 = 11;
pub const SYS_WINDOW_CREATE:     u64 = 20;
pub const SYS_WINDOW_BLIT:       u64 = 21;
pub const SYS_WINDOW_CLOSE:      u64 = 22;
pub const SYS_WINDOW_POLL_EVENT: u64 = 24;

pub const EVENT_NONE:         u32 = 0;
pub const EVENT_MOUSE_DOWN:   u32 = 1;
pub const EVENT_MOUSE_UP:     u32 = 2;
pub const EVENT_MOUSE_MOVE:   u32 = 3;
pub const EVENT_KEY_DOWN:     u32 = 4;
pub const EVENT_KEY_UP:       u32 = 5;
pub const EVENT_SCROLL:       u32 = 6;
pub const EVENT_WINDOW_CLOSE: u32 = 7;

/// Estrutura de evento transmitida entre o compositor do Aladdin e o processo Ring 3 (32 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClientWindowEvent {
    pub event_type: u32,
    pub win_id: u32,
    pub x: i32,
    pub y: i32,
    pub button_or_key: u32,
    pub modifiers: u32,
    pub timestamp: u64,
}

// ── Funções de Baixo Nível do Sistema ─────────────────────────────────────────

/// Encerra o processo de usuário de forma imediata com o código especificado.
#[inline(always)]
pub fn exit(code: i32) -> ! {
    unsafe {
        loop {
            syscall1(SYS_EXIT, code as u64);
        }
    }
}

/// Cede a CPU voluntariamente para outras tarefas (cooperative yield).
#[inline(always)]
pub fn yield_now() {
    unsafe {
        syscall0(SYS_YIELD);
    }
}

/// Adormece o processo pelo período aproximado em milissegundos sem queimar ciclos de CPU.
#[inline(always)]
pub fn sleep(ms: u64) {
    unsafe {
        syscall1(SYS_SLEEP, ms);
    }
}

/// Obtém o identificador do processo atual (PID).
#[inline(always)]
pub fn get_pid() -> u64 {
    unsafe { syscall0(SYS_GETPID) as u64 }
}

/// Escreve uma string diretamente na saída padrão (stdout - fd 1) do terminal.
pub fn print(s: &str) {
    unsafe {
        syscall3(SYS_WRITE, 1, s.as_ptr() as u64, s.len() as u64);
    }
}

/// Escreve uma string seguida de quebra de linha na saída padrão (stdout).
pub fn println(s: &str) {
    print(s);
    print("\n");
}

/// Imprime um número inteiro formatado na saída padrão.
pub fn print_num(mut n: i64) {
    if n == 0 {
        print("0");
        return;
    }
    if n < 0 {
        print("-");
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
    if let Ok(s) = core::str::from_utf8(&buf[..i]) {
        print(s);
    }
}

// ── Abstração de Janelas Gráficas (Aladdin Windowing) ─────────────────────────

/// Identificador e controlador de uma janela gráfica alocada no compositor do Aladdin Desktop.
pub struct WindowHandle {
    pub id: u32,
    pub width: u32,
    pub height: u32,
}

impl WindowHandle {
    /// Envia e apresenta uma fatia de pixels (0x00RRGGBB) no framebuffer da janela.
    pub fn blit(&self, buffer: &[u32]) -> Result<(), &'static str> {
        let expected_len = (self.width as usize) * (self.height as usize);
        if buffer.len() != expected_len {
            return Err("Buffer de pixels com dimensao diferente da janela");
        }
        let size_bytes = buffer.len() * core::mem::size_of::<u32>();
        let ret = unsafe {
            syscall3(
                SYS_WINDOW_BLIT,
                self.id as u64,
                buffer.as_ptr() as u64,
                size_bytes as u64,
            )
        };
        if ret == 0 {
            Ok(())
        } else {
            Err("Falha na chamada sys_window_blit")
        }
    }

    /// Envia e apresenta um buffer de bytes brutos (4 bytes por pixel: BGRA / 0x00RRGGBB).
    pub fn blit_bytes(&self, buffer: &[u8]) -> Result<(), &'static str> {
        let expected_bytes = (self.width as usize) * (self.height as usize) * 4;
        if buffer.len() != expected_bytes {
            return Err("Buffer de bytes com tamanho incompativel com a resolucao da janela");
        }
        let ret = unsafe {
            syscall3(
                SYS_WINDOW_BLIT,
                self.id as u64,
                buffer.as_ptr() as u64,
                buffer.len() as u64,
            )
        };
        if ret == 0 {
            Ok(())
        } else {
            Err("Falha na chamada sys_window_blit")
        }
    }

    /// Consulta o próximo evento de entrada pendente para esta janela.
    pub fn poll_event(&self) -> Option<ClientWindowEvent> {
        let mut ev = ClientWindowEvent::default();
        let ret = unsafe {
            syscall3(
                SYS_WINDOW_POLL_EVENT,
                self.id as u64,
                &mut ev as *mut ClientWindowEvent as u64,
                0,
            )
        };
        if ret == 1 {
            Some(ev)
        } else {
            None
        }
    }

    /// Fecha a janela e libera todos os recursos gráficos alocados no compositor.
    pub fn close(self) -> Result<(), &'static str> {
        let ret = unsafe {
            syscall1(SYS_WINDOW_CLOSE, self.id as u64)
        };
        if ret == 0 {
            Ok(())
        } else {
            Err("Falha ao fechar janela via sys_window_close")
        }
    }
}

/// Solicita ao compositor do Aladdin a criação de uma nova janela com título e dimensões especificadas.
pub fn open_window(title: &str, width: u32, height: u32) -> Result<WindowHandle, &'static str> {
    if width == 0 || height == 0 || width > 1024 || height > 768 {
        return Err("Dimensoes invalidas para criacao de janela (max 1024x768)");
    }
    let ret = unsafe {
        syscall4(
            SYS_WINDOW_CREATE,
            title.as_ptr() as u64,
            title.len() as u64,
            width as u64,
            height as u64,
        )
    };
    if ret >= 0 {
        Ok(WindowHandle {
            id: ret as u32,
            width,
            height,
        })
    } else {
        Err("Compositor rejeitou a alocacao da janela (limite atingido ou erro interno)")
    }
}
