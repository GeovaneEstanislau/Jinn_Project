/// Keyboard Ring Buffer
///
/// A lock-free-friendly, interrupt-safe ring buffer of 256 bytes.
/// The PS/2 driver pushes bytes here from IRQ context; the shell pops
/// bytes from the main loop context.
///
/// Implemented as a static SpinLock over a simple ring structure.
/// Using a SpinLock is safe here because:
///   - The kernel is single-core (only one IRQ at a time)
///   - We never hold the lock across an `sti` boundary
///   - The shell holds the lock only briefly to pop one byte

use crate::memory::SpinLock;

const BUF_SIZE: usize = 256;

struct RingBuffer {
    buf:  [u8; BUF_SIZE],
    head: usize,   // next write position
    tail: usize,   // next read position
    len:  usize,
}

impl RingBuffer {
    const fn new() -> Self {
        RingBuffer {
            buf:  [0u8; BUF_SIZE],
            head: 0,
            tail: 0,
            len:  0,
        }
    }

    fn push(&mut self, byte: u8) {
        if self.len < BUF_SIZE {
            self.buf[self.head] = byte;
            self.head = (self.head + 1) % BUF_SIZE;
            self.len += 1;
        }
        // If full, silently drop the byte (keyboard overrun protection)
    }

    fn pop(&mut self) -> Option<u8> {
        if self.len == 0 {
            return None;
        }
        let byte = self.buf[self.tail];
        self.tail = (self.tail + 1) % BUF_SIZE;
        self.len -= 1;
        Some(byte)
    }

    fn len(&self) -> usize {
        self.len
    }
}

static KEYBOARD_BUFFER: SpinLock<RingBuffer> = SpinLock::new(RingBuffer::new());

/// Push a byte into the buffer (called from IRQ context).
pub fn push(byte: u8) {
    KEYBOARD_BUFFER.lock().push(byte);
}

/// Pop a byte from the buffer (called from shell main loop).
pub fn pop() -> Option<u8> {
    KEYBOARD_BUFFER.lock().pop()
}

/// Number of bytes currently in the buffer.
pub fn len() -> usize {
    KEYBOARD_BUFFER.lock().len()
}
