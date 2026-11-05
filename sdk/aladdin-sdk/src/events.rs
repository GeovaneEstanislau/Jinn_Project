//! Aladdin Desktop SDK — Eventos de Entrada e Notificações de Janela

pub const EVENT_NONE:          u32 = 0;
pub const EVENT_MOUSE_DOWN:    u32 = 1;
pub const EVENT_MOUSE_UP:      u32 = 2;
pub const EVENT_MOUSE_MOVE:    u32 = 3;
pub const EVENT_KEY_DOWN:      u32 = 4;
pub const EVENT_KEY_UP:        u32 = 5;
pub const EVENT_SCROLL:        u32 = 6;
pub const EVENT_WINDOW_CLOSE:  u32 = 7;
pub const EVENT_WINDOW_RESIZE: u32 = 8;

/// Estrutura de evento transmitida entre o compositor do Aladdin e o processo Ring 3 (32 bytes).
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

impl ClientWindowEvent {
    pub const fn empty() -> Self {
        Self {
            event_type: EVENT_NONE,
            param1: 0,
            param2: 0,
            param3: 0,
            timestamp_ms: 0,
            reserved: 0,
        }
    }

    #[inline]
    pub fn is_close(&self) -> bool {
        self.event_type == EVENT_WINDOW_CLOSE
    }

    #[inline]
    pub fn is_key_down(&self) -> bool {
        self.event_type == EVENT_KEY_DOWN
    }

    #[inline]
    pub fn is_key_up(&self) -> bool {
        self.event_type == EVENT_KEY_UP
    }

    #[inline]
    pub fn is_mouse_down(&self) -> bool {
        self.event_type == EVENT_MOUSE_DOWN
    }

    #[inline]
    pub fn is_mouse_up(&self) -> bool {
        self.event_type == EVENT_MOUSE_UP
    }

    #[inline]
    pub fn is_mouse_move(&self) -> bool {
        self.event_type == EVENT_MOUSE_MOVE
    }

    #[inline]
    pub fn key_code(&self) -> Option<u32> {
        if self.is_key_down() || self.is_key_up() {
            Some(self.param1)
        } else {
            None
        }
    }

    #[inline]
    pub fn mouse_coords(&self) -> Option<(u32, u32)> {
        if self.is_mouse_down() || self.is_mouse_up() || self.is_mouse_move() {
            Some((self.param1, self.param2))
        } else {
            None
        }
    }

    #[inline]
    pub fn mouse_button(&self) -> Option<u32> {
        if self.is_mouse_down() || self.is_mouse_up() {
            Some(self.param3)
        } else {
            None
        }
    }
}
