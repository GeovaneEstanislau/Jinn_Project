//! # Aladdin Desktop SDK
//!
//! Biblioteca de desenvolvimento para aplicativos independentes de usuário (Ring 3)
//! executando sobre o microkernel **Jinn OS** e o ambiente gráfico **Aladdin Desktop**.
//!
//! Projetada para total conformidade `#![no_std]`, zero alocações implícitas,
//! segurança de memória estrita e comunicação exclusiva através de chamadas de sistema (syscalls).
//!
//! ## Módulos Disponíveis
//!
//! - [`window`]: Criação, gerenciamento e fechamento de janelas no compositor.
//! - [`graphics`]: Primitivas de renderização 2D (pixels, retângulos, degradês, linhas).
//! - [`events`]: Protocolo de eventos de entrada (mouse, teclado, fechamento).
//! - [`process`]: Controle de execução (PID, saída limpa de processo, preempção voluntária).
//! - [`console`]: Impressão e depuração em terminal de usuário.
//! - [`time`]: Suspensão temporizada (`sleep`).
//! - [`syscall`]: Primitivas de baixo nível da instrução `int 0x80`.

#![cfg_attr(not(test), no_std)]

pub mod syscall;
pub mod events;
pub mod window;
pub mod graphics;
pub mod process;
pub mod console;
pub mod time;

// ── Re-exportações Globais e Conveniência ─────────────────────────────────────

pub use syscall::{
    SYS_EXIT, SYS_WRITE, SYS_READ, SYS_YIELD, SYS_GETPID, SYS_SLEEP,
    SYS_WINDOW_CREATE, SYS_WINDOW_BLIT, SYS_WINDOW_CLOSE, SYS_WINDOW_POLL_EVENT,
    SYS_GRAPHICS_REGISTER, SYS_GRAPHICS_GET_FB_INFO, SYS_GRAPHICS_PRESENT,
    SYS_GRAPHICS_POLL_INPUT, SYS_GRAPHICS_UNREGISTER, SYS_GRAPHICS_READ_CLIENT_SURFACE,
    SYS_GRAPHICS_DISPATCH_CLIENT_EVENT, SYS_GRAPHICS_LIST_CLIENT_WINDOWS,
};

pub use events::{
    ClientWindowEvent,
    EVENT_NONE, EVENT_MOUSE_DOWN, EVENT_MOUSE_UP, EVENT_MOUSE_MOVE,
    EVENT_KEY_DOWN, EVENT_KEY_UP, EVENT_SCROLL, EVENT_WINDOW_CLOSE, EVENT_WINDOW_RESIZE,
    EVENT_FOCUS_GAINED, EVENT_FOCUS_LOST,
};

pub use window::{WindowHandle, WindowConfig, WindowError, open_window};
pub use graphics::{Color, Canvas};
pub use process::{exit, getpid, yield_now};
pub use console::{print, println, print_num, print_hex};
pub use time::sleep;

/// Prelúdio com tipos e funções essenciais para rápida inicialização
pub mod prelude {
    pub use crate::window::{open_window, WindowHandle, WindowConfig, WindowError};
    pub use crate::graphics::{Color, Canvas};
    pub use crate::events::{
        ClientWindowEvent, EVENT_WINDOW_CLOSE, EVENT_WINDOW_RESIZE,
        EVENT_KEY_DOWN, EVENT_KEY_UP, EVENT_MOUSE_DOWN, EVENT_MOUSE_UP,
        EVENT_FOCUS_GAINED, EVENT_FOCUS_LOST,
    };
    pub use crate::process::{exit, getpid, yield_now};
    pub use crate::console::{print, println, print_num, print_hex};
    pub use crate::time::sleep;
}
