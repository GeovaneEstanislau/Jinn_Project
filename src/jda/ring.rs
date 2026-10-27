/// Jinn OS — JDA Asynchronous I/O & Compute Ring (io_uring Pattern)
///
/// Implementa anéis de submissão (SQ) e conclusão (CQ) lock-free para desacoplar
/// operações de armazenamento (disco) e processamento vetorial/stream da CPU,
/// atingindo latência determinística e zero-copy in-kernel.

use core::sync::atomic::{AtomicU32, Ordering};
use crate::memory::SpinLock;
use crate::jda::telemetry;

pub const JDA_RING_SIZE: usize = 32;
pub const JDA_RING_MASK: u32   = (JDA_RING_SIZE - 1) as u32;

// ── Opcodes de Operação ────────────────────────────────────────────────────────
pub const OP_NOP:                u8 = 0;
pub const OP_READ_BLOCK:         u8 = 1;
pub const OP_WRITE_BLOCK:        u8 = 2;
pub const OP_COMPUTE_FLETCHER32: u8 = 3;
pub const OP_COMPUTE_SUM:        u8 = 4;
pub const OP_COMPUTE_XOR:        u8 = 5;
pub const OP_BARRIER:            u8 = 6;

// ── Flags de Submissão ────────────────────────────────────────────────────────
pub const JDA_SQE_DRAIN:         u8 = 1 << 0;
pub const JDA_SQE_ZERO_COPY:     u8 = 1 << 1;

/// Entrada da Fila de Submissão (Submission Queue Entry — SQE)
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JdaSqe {
    pub opcode: u8,
    pub flags: u8,
    pub priority: u8,
    pub _reserved: u8,
    pub handle: u32,
    pub offset: u64,
    pub len: u32,
    pub user_data: u64,
}

/// Entrada da Fila de Conclusão (Completion Queue Entry — CQE)
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JdaCqe {
    pub user_data: u64,
    pub res: i32,
    pub flags: u32,
}

/// Fila circular de submissão (SQ)
pub struct SubmissionQueue {
    pub entries: [JdaSqe; JDA_RING_SIZE],
    pub head: AtomicU32,
    pub tail: AtomicU32,
}

impl SubmissionQueue {
    pub const fn new() -> Self {
        Self {
            entries: [JdaSqe { opcode: 0, flags: 0, priority: 0, _reserved: 0, handle: 0, offset: 0, len: 0, user_data: 0 }; JDA_RING_SIZE],
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
        }
    }

    pub fn push(&mut self, sqe: JdaSqe) -> Result<(), &'static str> {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Relaxed);
        if tail.wrapping_sub(head) >= JDA_RING_SIZE as u32 {
            return Err("Submission queue ring cheia");
        }
        let idx = (tail & JDA_RING_MASK) as usize;
        self.entries[idx] = sqe;
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        Ok(())
    }

    pub fn pop(&mut self) -> Option<JdaSqe> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if head == tail {
            return None;
        }
        let idx = (head & JDA_RING_MASK) as usize;
        let sqe = self.entries[idx];
        self.head.store(head.wrapping_add(1), Ordering::Release);
        Some(sqe)
    }

    #[inline]
    pub fn pending_count(&self) -> u32 {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Relaxed);
        tail.wrapping_sub(head)
    }
}

/// Fila circular de conclusão (CQ)
pub struct CompletionQueue {
    pub entries: [JdaCqe; JDA_RING_SIZE],
    pub head: AtomicU32,
    pub tail: AtomicU32,
}

impl CompletionQueue {
    pub const fn new() -> Self {
        Self {
            entries: [JdaCqe { user_data: 0, res: 0, flags: 0 }; JDA_RING_SIZE],
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
        }
    }

    pub fn push(&mut self, cqe: JdaCqe) -> Result<(), &'static str> {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Relaxed);
        if tail.wrapping_sub(head) >= JDA_RING_SIZE as u32 {
            return Err("Completion queue ring cheia");
        }
        let idx = (tail & JDA_RING_MASK) as usize;
        self.entries[idx] = cqe;
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        Ok(())
    }

    pub fn pop(&mut self) -> Option<JdaCqe> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if head == tail {
            return None;
        }
        let idx = (head & JDA_RING_MASK) as usize;
        let cqe = self.entries[idx];
        self.head.store(head.wrapping_add(1), Ordering::Release);
        Some(cqe)
    }

    #[inline]
    pub fn available_count(&self) -> u32 {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Relaxed);
        tail.wrapping_sub(head)
    }
}

/// Estatísticas consolidadas de operação dos anéis
#[derive(Clone, Copy, Debug, Default)]
pub struct JdaRingStats {
    pub total_submitted: u64,
    pub total_completed: u64,
    pub total_errors: u64,
    pub bytes_processed: u64,
}

/// Motor de anéis assíncronos do JDA
pub struct JdaIoRing {
    pub sq: SubmissionQueue,
    pub cq: CompletionQueue,
    pub stats: JdaRingStats,
}

impl JdaIoRing {
    pub const fn new() -> Self {
        Self {
            sq: SubmissionQueue::new(),
            cq: CompletionQueue::new(),
            stats: JdaRingStats {
                total_submitted: 0,
                total_completed: 0,
                total_errors: 0,
                bytes_processed: 0,
            },
        }
    }

    /// Submete um SQE para a fila
    pub fn submit(&mut self, sqe: JdaSqe) -> Result<(), &'static str> {
        self.sq.push(sqe)?;
        self.stats.total_submitted += 1;
        Ok(())
    }

    /// Processa e despacha todas as operações pendentes na Submission Queue
    pub fn dispatch_pending(&mut self, caller_pid: usize) -> usize {
        let mut processed = 0;

        while let Some(sqe) = self.sq.pop() {
            processed += 1;
            let cqe = self.execute_sqe(caller_pid, sqe);
            if cqe.res < 0 {
                self.stats.total_errors += 1;
            } else {
                self.stats.total_completed += 1;
            }
            let _ = self.cq.push(cqe);
        }

        processed
    }

    /// Executa uma única operação de forma segura e zero-copy
    fn execute_sqe(&mut self, caller_pid: usize, sqe: JdaSqe) -> JdaCqe {
        let start_cycles = telemetry::read_tsc();

        let (res, bytes) = match sqe.opcode {
            OP_NOP => (0, 0),

            OP_READ_BLOCK => {
                let phys_opt = crate::jda::shmem::SHARED_BUFFER_POOL.lock().get_phys_addr(caller_pid, sqe.handle as usize);
                match phys_opt {
                    Some(phys) => {
                        let count = (sqe.len as usize).max(1);
                        let total_bytes = count * 512;
                        if total_bytes > 4096 {
                            (-22, 0) // EINVAL
                        } else {
                            let virt = crate::memory::paging::phys_to_virt(phys) as *mut u8;
                            let slice = unsafe { core::slice::from_raw_parts_mut(virt, total_bytes) };
                            match crate::drivers::block::read_into_buffer(sqe.offset, count, slice) {
                                Ok(()) => (total_bytes as i32, total_bytes),
                                Err(_) => (-5, 0), // EIO
                            }
                        }
                    }
                    None => (-1, 0), // EPERM
                }
            }

            OP_WRITE_BLOCK => {
                let phys_opt = crate::jda::shmem::SHARED_BUFFER_POOL.lock().get_phys_addr(caller_pid, sqe.handle as usize);
                match phys_opt {
                    Some(phys) => {
                        let count = (sqe.len as usize).max(1);
                        let total_bytes = count * 512;
                        if total_bytes > 4096 {
                            (-22, 0)
                        } else {
                            let virt = crate::memory::paging::phys_to_virt(phys) as *const u8;
                            let slice = unsafe { core::slice::from_raw_parts(virt, total_bytes) };
                            match crate::drivers::block::write_from_buffer(sqe.offset, count, slice) {
                                Ok(()) => (total_bytes as i32, total_bytes),
                                Err(_) => (-5, 0),
                            }
                        }
                    }
                    None => (-1, 0),
                }
            }

            OP_COMPUTE_FLETCHER32 => {
                let phys_opt = crate::jda::shmem::SHARED_BUFFER_POOL.lock().get_phys_addr(caller_pid, sqe.handle as usize);
                match phys_opt {
                    Some(phys) => {
                        let data_len = (sqe.len as usize).min(4096);
                        let virt = crate::memory::paging::phys_to_virt(phys) as *const u8;
                        let slice = unsafe { core::slice::from_raw_parts(virt, data_len) };
                        let mut c0 = 0u32;
                        let mut c1 = 0u32;
                        for &b in slice {
                            c0 = (c0 + b as u32) % 255;
                            c1 = (c1 + c0) % 255;
                        }
                        let checksum = ((c1 << 8) | c0) as i32;
                        (checksum, data_len)
                    }
                    None => (-1, 0),
                }
            }

            OP_COMPUTE_SUM => {
                let phys_opt = crate::jda::shmem::SHARED_BUFFER_POOL.lock().get_phys_addr(caller_pid, sqe.handle as usize);
                match phys_opt {
                    Some(phys) => {
                        let data_len = (sqe.len as usize).min(4096);
                        let virt = crate::memory::paging::phys_to_virt(phys) as *const u8;
                        let slice = unsafe { core::slice::from_raw_parts(virt, data_len) };
                        let mut sum: u32 = 0;
                        for &b in slice {
                            sum = sum.wrapping_add(b as u32);
                        }
                        (sum as i32, data_len)
                    }
                    None => (-1, 0),
                }
            }

            OP_COMPUTE_XOR => {
                let pool = crate::jda::shmem::SHARED_BUFFER_POOL.lock();
                let phys_opt = pool.get_phys_addr(caller_pid, sqe.handle as usize);
                let is_ro = pool.is_read_only(sqe.handle as usize);
                drop(pool);

                if is_ro {
                    (-1, 0) // EPERM: buffer é somente leitura
                } else {
                    match phys_opt {
                        Some(phys) => {
                            let data_len = (sqe.len as usize).min(4096);
                            let virt = crate::memory::paging::phys_to_virt(phys) as *mut u8;
                            let slice = unsafe { core::slice::from_raw_parts_mut(virt, data_len) };
                            for b in slice.iter_mut() {
                                *b ^= 0x5A;
                            }
                            (data_len as i32, data_len)
                        }
                        None => (-1, 0),
                    }
                }
            }

            OP_BARRIER => {
                let _ = crate::drivers::block::flush_primary_cache();
                (0, 0)
            }

            _ => (-38, 0), // ENOSYS
        };

        if bytes > 0 {
            self.stats.bytes_processed += bytes as u64;
            telemetry::record_data_transfer(bytes, true);
        }

        let end_cycles = telemetry::read_tsc();
        let elapsed = end_cycles.saturating_sub(start_cycles);
        telemetry::record_operation_completed(elapsed);

        JdaCqe {
            user_data: sqe.user_data,
            res,
            flags: if sqe.flags & JDA_SQE_ZERO_COPY != 0 { 1 } else { 0 },
        }
    }

    /// Retira uma conclusão da Fila de Conclusão (CQ)
    pub fn poll_completion(&mut self) -> Option<JdaCqe> {
        self.cq.pop()
    }
}

pub static JDA_RING: SpinLock<JdaIoRing> = SpinLock::new(JdaIoRing::new());
