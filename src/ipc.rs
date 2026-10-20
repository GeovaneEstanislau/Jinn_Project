/// Jinn OS — Inter-Process Communication (IPC)
///
/// Lock-free, fixed-size message passing between kernel tasks.
///
/// Design:
///   - Each process/task owns a private `MessageQueue` (ring buffer of `Message`s).
///   - `send(pid, msg)` writes directly into the target's queue.
///   - `recv()` pops from the calling task's queue.
///   - No kernel allocation on the hot path — all queues are statically sized.
///   - The queue uses an atomic head/tail pair for lock-free single-producer,
///     single-consumer usage. Multi-producer safety is ensured via a SpinLock.
///
/// Message format: 80 bytes total
///   pid_from (4 B) + pid_to (4 B) + tag (4 B) + _pad (4 B) + payload (64 B)

use core::sync::atomic::{AtomicUsize, Ordering};
use crate::memory::SpinLock;

// ── Constants ─────────────────────────────────────────────────────────────────
pub const MAX_PROCS:    usize = 64;
pub const QUEUE_DEPTH:  usize = 16;   // messages per queue

pub const TAG_DATA_INLINE:     u32 = 0x0000_0000;
pub const TAG_JDA_SHM_BUFFER:  u32 = 0x4A44_4101; // Marcador 'JDA\1' para Zero-Copy Frame

// ── Message ───────────────────────────────────────────────────────────────────
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Message {
    /// PID of the sender (0 = kernel)
    pub pid_from: u32,
    /// PID of the intended recipient
    pub pid_to:   u32,
    /// Application-defined message type tag
    pub tag:      u32,
    pub _pad:     u32,
    /// Up to 64 bytes of inline payload
    pub payload:  [u8; 64],
}

impl Message {
    pub const fn zero() -> Self {
        Self { pid_from: 0, pid_to: 0, tag: 0, _pad: 0, payload: [0u8; 64] }
    }

    /// Create a message with a raw payload slice (truncated/zero-padded to 64 B).
    pub fn new(from: u32, to: u32, tag: u32, data: &[u8]) -> Self {
        let mut m = Self::zero();
        m.pid_from = from;
        m.pid_to   = to;
        m.tag      = tag;
        let len = data.len().min(64);
        m.payload[..len].copy_from_slice(&data[..len]);
        m
    }

    /// Cria uma mensagem IPC que transporta uma referência de buffer físico Zero-Copy do JDA
    pub fn new_jda_zero_copy(from: u32, to: u32, shm_handle: usize, data_len: usize) -> Self {
        let mut m = Self::zero();
        m.pid_from = from;
        m.pid_to   = to;
        m.tag      = TAG_JDA_SHM_BUFFER;
        m.payload[0..8].copy_from_slice(&(shm_handle as u64).to_le_bytes());
        m.payload[8..16].copy_from_slice(&(data_len as u64).to_le_bytes());
        m
    }

    /// Se a mensagem for TAG_JDA_SHM_BUFFER, extrai o (handle, data_len)
    pub fn as_jda_zero_copy(&self) -> Option<(usize, usize)> {
        if self.tag == TAG_JDA_SHM_BUFFER {
            let mut handle_bytes = [0u8; 8];
            let mut len_bytes = [0u8; 8];
            handle_bytes.copy_from_slice(&self.payload[0..8]);
            len_bytes.copy_from_slice(&self.payload[8..16]);
            let handle = u64::from_le_bytes(handle_bytes) as usize;
            let len = u64::from_le_bytes(len_bytes) as usize;
            Some((handle, len))
        } else {
            None
        }
    }
}

// ── Per-Process Message Queue ─────────────────────────────────────────────────
struct RawQueue {
    buf:  [Message; QUEUE_DEPTH],
    head: AtomicUsize,   // consumer reads here
    tail: AtomicUsize,   // producer writes here
}

impl RawQueue {
    const fn new() -> Self {
        Self {
            buf:  [Message { pid_from: 0, pid_to: 0, tag: 0, _pad: 0, payload: [0u8; 64] }; QUEUE_DEPTH],
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    fn is_empty(&self) -> bool {
        self.head.load(Ordering::Acquire) == self.tail.load(Ordering::Acquire)
    }

    fn is_full(&self) -> bool {
        let tail = self.tail.load(Ordering::Acquire);
        let next = (tail + 1) % QUEUE_DEPTH;
        next == self.head.load(Ordering::Acquire)
    }

    /// Push a message. Returns `false` if the queue is full.
    fn push(&mut self, msg: Message) -> bool {
        if self.is_full() { return false; }
        let tail = self.tail.load(Ordering::Relaxed);
        self.buf[tail] = msg;
        self.tail.store((tail + 1) % QUEUE_DEPTH, Ordering::Release);
        true
    }

    /// Pop a message. Returns `None` if the queue is empty.
    fn pop(&mut self) -> Option<Message> {
        if self.is_empty() { return None; }
        let head = self.head.load(Ordering::Relaxed);
        let msg  = self.buf[head];
        self.head.store((head + 1) % QUEUE_DEPTH, Ordering::Release);
        Some(msg)
    }
}

// ── IPC Table ─────────────────────────────────────────────────────────────────
struct IpcTable {
    queues: [RawQueue; MAX_PROCS],
    active: [bool; MAX_PROCS],
}

impl IpcTable {
    const fn new() -> Self {
        const Q: RawQueue = RawQueue::new();
        Self {
            queues: [Q; MAX_PROCS],
            active: [false; MAX_PROCS],
        }
    }
}

static IPC: SpinLock<IpcTable> = SpinLock::new(IpcTable::new());

// ── Public API ────────────────────────────────────────────────────────────────

/// Register a PID so it can receive messages.
pub fn register(pid: usize) {
    assert!(pid < MAX_PROCS, "ipc: pid out of range");
    let mut table = IPC.lock();
    table.active[pid] = true;
    table.queues[pid].head.store(0, Ordering::Relaxed);
    table.queues[pid].tail.store(0, Ordering::Relaxed);
}

/// Unregister a PID (called on task exit).
pub fn unregister(pid: usize) {
    if pid < MAX_PROCS {
        IPC.lock().active[pid] = false;
    }
}

/// Send a message to a registered PID.
///
/// Returns `Ok(())` on success, `Err` if the recipient doesn't exist or
/// their queue is full.
pub fn send(msg: Message) -> Result<(), &'static str> {
    let pid = msg.pid_to as usize;
    if pid >= MAX_PROCS { return Err("ipc: invalid pid"); }
    let mut table = IPC.lock();
    if !table.active[pid] { return Err("ipc: recipient not registered"); }
    if !table.queues[pid].push(msg) { return Err("ipc: queue full"); }
    Ok(())
}

/// Receive the next message for `pid`. Non-blocking.
pub fn recv(pid: usize) -> Option<Message> {
    if pid >= MAX_PROCS { return None; }
    let mut table = IPC.lock();
    table.queues[pid].pop()
}

/// Returns `true` if there are pending messages for `pid`.
pub fn has_message(pid: usize) -> bool {
    if pid >= MAX_PROCS { return false; }
    !IPC.lock().queues[pid].is_empty()
}
