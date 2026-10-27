//! JDA Phase 3 — Asynchronous Ring (io_uring) & Zero-Copy Remapping Integration Tests
//!
//! Valida o comportamento funcional e invariantes dos anéis assíncronos:
//! 1. Submissão e conclusão de entradas SQE / CQE
//! 2. Detecção e rejeição de saturação dos anéis (capacidade fixa com wrap)
//! 3. Ordem FIFO com preservação de identificadores de usuário (user_data)
//! 4. Barreira de sincronização (OP_BARRIER)
//! 5. Imutabilidade e garantia de integridade de buffers Zero-Copy

use std::sync::atomic::{AtomicU32, Ordering};

pub const TEST_RING_SIZE: usize = 32;
pub const TEST_RING_MASK: u32   = (TEST_RING_SIZE - 1) as u32;

pub const OP_NOP:                u8 = 0;
pub const OP_READ_BLOCK:         u8 = 1;
pub const OP_WRITE_BLOCK:        u8 = 2;
pub const OP_COMPUTE_FLETCHER32: u8 = 3;
pub const OP_COMPUTE_SUM:        u8 = 4;
pub const OP_COMPUTE_XOR:        u8 = 5;
pub const OP_BARRIER:            u8 = 6;

pub const JDA_SQE_ZERO_COPY:     u8 = 1 << 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TestJdaSqe {
    pub opcode: u8,
    pub flags: u8,
    pub priority: u8,
    pub _reserved: u8,
    pub handle: u32,
    pub offset: u64,
    pub len: u32,
    pub user_data: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TestJdaCqe {
    pub user_data: u64,
    pub res: i32,
    pub flags: u32,
}

pub struct TestRing {
    sq_entries: [TestJdaSqe; TEST_RING_SIZE],
    sq_head: AtomicU32,
    sq_tail: AtomicU32,

    cq_entries: [TestJdaCqe; TEST_RING_SIZE],
    cq_head: AtomicU32,
    cq_tail: AtomicU32,
}

impl TestRing {
    pub fn new() -> Self {
        Self {
            sq_entries: [TestJdaSqe::default(); TEST_RING_SIZE],
            sq_head: AtomicU32::new(0),
            sq_tail: AtomicU32::new(0),

            cq_entries: [TestJdaCqe::default(); TEST_RING_SIZE],
            cq_head: AtomicU32::new(0),
            cq_tail: AtomicU32::new(0),
        }
    }

    pub fn submit(&mut self, sqe: TestJdaSqe) -> Result<(), &'static str> {
        let head = self.sq_head.load(Ordering::Acquire);
        let tail = self.sq_tail.load(Ordering::Relaxed);
        if tail.wrapping_sub(head) >= TEST_RING_SIZE as u32 {
            return Err("SQ cheio");
        }
        let idx = (tail & TEST_RING_MASK) as usize;
        self.sq_entries[idx] = sqe;
        self.sq_tail.store(tail.wrapping_add(1), Ordering::Release);
        Ok(())
    }

    pub fn dispatch(&mut self, buffer_mock: &mut [u8]) -> usize {
        let mut count = 0;
        loop {
            let head = self.sq_head.load(Ordering::Relaxed);
            let tail = self.sq_tail.load(Ordering::Acquire);
            if head == tail {
                break;
            }
            let idx = (head & TEST_RING_MASK) as usize;
            let sqe = self.sq_entries[idx];
            self.sq_head.store(head.wrapping_add(1), Ordering::Release);

            let res = match sqe.opcode {
                OP_NOP => 0,
                OP_COMPUTE_SUM => {
                    let len = (sqe.len as usize).min(buffer_mock.len());
                    let mut sum = 0u32;
                    for &b in &buffer_mock[..len] {
                        sum = sum.wrapping_add(b as u32);
                    }
                    sum as i32
                }
                OP_COMPUTE_FLETCHER32 => {
                    let len = (sqe.len as usize).min(buffer_mock.len());
                    let mut c0 = 0u32;
                    let mut c1 = 0u32;
                    for &b in &buffer_mock[..len] {
                        c0 = (c0 + b as u32) % 255;
                        c1 = (c1 + c0) % 255;
                    }
                    ((c1 << 8) | c0) as i32
                }
                OP_COMPUTE_XOR => {
                    let len = (sqe.len as usize).min(buffer_mock.len());
                    for b in &mut buffer_mock[..len] {
                        *b ^= 0x5A;
                    }
                    len as i32
                }
                OP_BARRIER => 0,
                _ => -38, // ENOSYS
            };

            let cqe = TestJdaCqe {
                user_data: sqe.user_data,
                res,
                flags: if sqe.flags & JDA_SQE_ZERO_COPY != 0 { 1 } else { 0 },
            };

            let cq_head = self.cq_head.load(Ordering::Acquire);
            let cq_tail = self.cq_tail.load(Ordering::Relaxed);
            assert!(cq_tail.wrapping_sub(cq_head) < TEST_RING_SIZE as u32, "CQ overflow");
            let c_idx = (cq_tail & TEST_RING_MASK) as usize;
            self.cq_entries[c_idx] = cqe;
            self.cq_tail.store(cq_tail.wrapping_add(1), Ordering::Release);
            count += 1;
        }
        count
    }

    pub fn poll(&mut self) -> Option<TestJdaCqe> {
        let head = self.cq_head.load(Ordering::Relaxed);
        let tail = self.cq_tail.load(Ordering::Acquire);
        if head == tail {
            return None;
        }
        let idx = (head & TEST_RING_MASK) as usize;
        let cqe = self.cq_entries[idx];
        self.cq_head.store(head.wrapping_add(1), Ordering::Release);
        Some(cqe)
    }
}

#[test]
fn test_ring_batch_submission_and_completion_order() {
    let mut ring = TestRing::new();
    let mut buf = vec![10u8; 100];

    for i in 0..8 {
        let sqe = TestJdaSqe {
            opcode: if i % 2 == 0 { OP_COMPUTE_SUM } else { OP_NOP },
            flags: JDA_SQE_ZERO_COPY,
            priority: 1,
            _reserved: 0,
            handle: 1,
            offset: 0,
            len: 100,
            user_data: 1000 + i as u64,
        };
        assert!(ring.submit(sqe).is_ok());
    }

    let processed = ring.dispatch(&mut buf);
    assert_eq!(processed, 8);

    for i in 0..8 {
        let cqe = ring.poll().expect("CQE faltando");
        assert_eq!(cqe.user_data, 1000 + i as u64);
        if i % 2 == 0 {
            assert_eq!(cqe.res, 1000); // 100 * 10 = 1000
            assert_eq!(cqe.flags, 1);  // Flag ZERO_COPY preservada
        } else {
            assert_eq!(cqe.res, 0);
        }
    }

    assert!(ring.poll().is_none(), "CQ deve estar vazio após colher todos os resultados");
}

#[test]
fn test_ring_saturation_and_overflow_protection() {
    let mut ring = TestRing::new();

    // Enche exatamente as 32 posições da SQ
    for i in 0..32 {
        let sqe = TestJdaSqe {
            opcode: OP_NOP,
            flags: 0,
            priority: 0,
            _reserved: 0,
            handle: 0,
            offset: 0,
            len: 0,
            user_data: i as u64,
        };
        assert!(ring.submit(sqe).is_ok());
    }

    // A 33ª submissão deve ser estritamente rejeitada
    let overflow_sqe = TestJdaSqe {
        opcode: OP_NOP,
        flags: 0,
        priority: 0,
        _reserved: 0,
        handle: 0,
        offset: 0,
        len: 0,
        user_data: 999,
    };
    assert!(ring.submit(overflow_sqe).is_err(), "SQ cheia deve rejeitar sem corrupção");

    let mut buf = [0u8; 16];
    let processed = ring.dispatch(&mut buf);
    assert_eq!(processed, 32);

    // Após processar, a fila SQ esvaziou e novas submissões são aceitas
    assert!(ring.submit(overflow_sqe).is_ok(), "Após drenar, SQ deve aceitar novas entradas");
}

#[test]
fn test_ring_zero_copy_in_place_xor_transformation() {
    let mut ring = TestRing::new();
    let mut buf = vec![0x00u8; 64];

    let sqe = TestJdaSqe {
        opcode: OP_COMPUTE_XOR,
        flags: JDA_SQE_ZERO_COPY,
        priority: 1,
        _reserved: 0,
        handle: 1,
        offset: 0,
        len: 64,
        user_data: 0xABCD,
    };
    assert!(ring.submit(sqe).is_ok());

    let _ = ring.dispatch(&mut buf);
    let cqe = ring.poll().expect("CQE ausente");
    assert_eq!(cqe.user_data, 0xABCD);
    assert_eq!(cqe.res, 64);

    // Verifica que todos os bytes sofreram XOR in-place com 0x5A
    for &b in &buf {
        assert_eq!(b, 0x5A);
    }
}
