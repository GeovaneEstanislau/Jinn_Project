//! Aladdin Desktop SDK — Criação e Gerenciamento de Janelas

use crate::events::ClientWindowEvent;
use crate::syscall::*;

/// Configuração declarativa para criação de janela
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowConfig<'a> {
    pub title: &'a str,
    pub width: u32,
    pub height: u32,
}

impl<'a> WindowConfig<'a> {
    pub const fn new(title: &'a str, width: u32, height: u32) -> Self {
        Self { title, width, height }
    }
}

/// Identificador seguro e manipulador de janela alocada no Aladdin Desktop
#[derive(Debug)]
pub struct WindowHandle {
    pub id: usize,
    pub width: u32,
    pub height: u32,
}

impl WindowHandle {
    /// Apresenta o buffer de pixels na janela do compositor (sys_window_blit)
    pub fn blit(&self, buffer: &[u32]) -> Result<(), &'static str> {
        let expected_len = (self.width as usize) * (self.height as usize);
        if buffer.len() != expected_len {
            return Err("Tamanho do buffer incompativel com dimensoes da janela");
        }
        let size_bytes = buffer.len() * 4;
        let ret = unsafe {
            syscall3(
                SYS_WINDOW_BLIT,
                self.id as u64,
                buffer.as_ptr() as u64,
                size_bytes as u64,
            )
        };
        if ret >= 0 {
            Ok(())
        } else {
            Err("sys_window_blit falhou")
        }
    }

    /// Coleta o próximo evento de entrada pendente para a janela (sys_window_poll_event)
    pub fn poll_event(&self) -> Option<ClientWindowEvent> {
        let mut ev = ClientWindowEvent::empty();
        let ret = unsafe {
            syscall3(
                SYS_WINDOW_POLL_EVENT,
                self.id as u64,
                (&mut ev as *mut ClientWindowEvent) as u64,
                0,
            )
        };
        if ret == 1 {
            Some(ev)
        } else {
            None
        }
    }

    /// Encerra a janela e libera todos os buffers no compositor (sys_window_close)
    pub fn close(self) -> Result<(), &'static str> {
        let ret = unsafe { syscall1(SYS_WINDOW_CLOSE, self.id as u64) };
        if ret >= 0 {
            Ok(())
        } else {
            Err("sys_window_close falhou")
        }
    }
}

/// Cria uma nova janela de cliente no Aladdin Desktop
pub fn open_window(title: &str, width: u32, height: u32) -> Result<WindowHandle, &'static str> {
    if width == 0 || height == 0 || width > 1024 || height > 768 {
        return Err("Dimensoes invalidas (maximo suportado 1024x768)");
    }
    let title_bytes = title.as_bytes();
    let ret = unsafe {
        syscall4(
            SYS_WINDOW_CREATE,
            title_bytes.as_ptr() as u64,
            title_bytes.len() as u64,
            width as u64,
            height as u64,
        )
    };
    if ret >= 0 {
        Ok(WindowHandle {
            id: ret as usize,
            width,
            height,
        })
    } else {
        Err("Compositor rejeitou a alocacao da janela (limite atingido ou erro interno)")
    }
}
