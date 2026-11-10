//! Aladdin Desktop SDK — Criação e Gerenciamento de Janelas

use crate::events::ClientWindowEvent;
use crate::syscall::*;

/// Códigos de erro estruturados para operações de janela
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowError {
    /// Dimensões inválidas (largura ou altura zero, ou excedendo 1024x768)
    InvalidDimensions,
    /// Título vazio ou superior a 48 bytes
    InvalidTitle,
    /// Compositor rejeitou a criação (limite de janelas atingido ou falta de memória)
    CompositorRejected,
    /// Buffer fornecido tem tamanho incompatível com a área da janela
    BufferMismatch,
    /// Permissão negada para atualizar ou fechar a janela
    PermissionDenied,
    /// Identificador de janela inválido ou inexistente
    InvalidHandle,
    /// Erro de syscall genérico retornado pelo kernel
    SyscallFailed(i64),
}

impl WindowError {
    pub const fn as_str(&self) -> &'static str {
        match self {
            WindowError::InvalidDimensions => "Dimensoes invalidas (maximo suportado 1024x768)",
            WindowError::InvalidTitle => "Titulo de janela invalido ou muito longo (maximo 48 bytes)",
            WindowError::CompositorRejected => "Compositor rejeitou a alocacao da janela (limite atingido ou erro interno)",
            WindowError::BufferMismatch => "Tamanho do buffer incompativel com dimensoes da janela",
            WindowError::PermissionDenied => "Permissao negada para a janela",
            WindowError::InvalidHandle => "Identificador de janela invalido",
            WindowError::SyscallFailed(_) => "Syscall de janela falhou",
        }
    }
}

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
    pub fn blit(&self, buffer: &[u32]) -> Result<(), WindowError> {
        let expected_len = (self.width as usize) * (self.height as usize);
        if buffer.len() != expected_len {
            return Err(WindowError::BufferMismatch);
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
        } else if ret == -1 {
            Err(WindowError::PermissionDenied)
        } else if ret == -9 {
            Err(WindowError::InvalidHandle)
        } else {
            Err(WindowError::SyscallFailed(ret))
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

    /// Aguarda o próximo evento de entrada, cedendo a CPU voluntariamente (`yield_now`) enquanto a fila estiver vazia.
    pub fn wait_event(&self) -> ClientWindowEvent {
        loop {
            if let Some(ev) = self.poll_event() {
                return ev;
            }
            crate::process::yield_now();
        }
    }

    /// Aguarda o próximo evento de entrada com timeout em milissegundos.
    pub fn wait_event_timeout(&self, timeout_ms: u64) -> Option<ClientWindowEvent> {
        let mut waited = 0u64;
        loop {
            if let Some(ev) = self.poll_event() {
                return Some(ev);
            }
            if waited >= timeout_ms {
                return None;
            }
            crate::time::sleep(5);
            waited = waited.saturating_add(5);
        }
    }

    /// Encerra a janela e libera todos os buffers no compositor (sys_window_close)
    pub fn close(self) -> Result<(), WindowError> {
        let ret = unsafe { syscall1(SYS_WINDOW_CLOSE, self.id as u64) };
        if ret >= 0 {
            Ok(())
        } else if ret == -1 {
            Err(WindowError::PermissionDenied)
        } else if ret == -9 {
            Err(WindowError::InvalidHandle)
        } else {
            Err(WindowError::SyscallFailed(ret))
        }
    }
}

/// Cria uma nova janela de cliente no Aladdin Desktop
pub fn open_window(title: &str, width: u32, height: u32) -> Result<WindowHandle, WindowError> {
    if width == 0 || height == 0 || width > 1024 || height > 768 {
        return Err(WindowError::InvalidDimensions);
    }
    let title_bytes = title.as_bytes();
    if title_bytes.is_empty() || title_bytes.len() > 48 {
        return Err(WindowError::InvalidTitle);
    }
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
        Err(WindowError::CompositorRejected)
    }
}


