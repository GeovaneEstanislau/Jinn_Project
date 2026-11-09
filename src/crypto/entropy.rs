//! Jinn Microkernel — Subsistema de Entropia e CSPRNG
//!
//! Implementa gerador de números pseudoaleatórios criptograficamente seguro (CSPRNG)
//! baseado no algoritmo ChaCha20 DRBG, alimentado por instruções de hardware RDRAND/RDSEED
//! e pool de jitter acumulativo (RDTSC, PIT I/O jitter, CMOS e ticks do escalonador).

use core::sync::atomic::{AtomicBool, Ordering};
use crate::memory::SpinLock;

static HAS_RDRAND: AtomicBool = AtomicBool::new(false);
static HAS_RDSEED: AtomicBool = AtomicBool::new(false);

const CHACHA_CONSTANTS: [u32; 4] = [
    0x61707865, // "expa"
    0x3320646e, // "nd 3"
    0x79622d32, // "2-by"
    0x6b206574, // "te k"
];

pub struct CsprngState {
    key: [u32; 8],
    counter: u32,
    nonce: [u32; 3],
    bytes_generated: usize,
}

impl CsprngState {
    pub const fn new() -> Self {
        Self {
            key: [
                0x2b7e1516, 0x28aed2a6, 0xabf71588, 0x09cf4f3c,
                0x8e73bed6, 0x7c2adc21, 0x3e42f6ab, 0x15b2c3d4,
            ],
            counter: 1,
            nonce: [0x50415353, 0x4a494e4e, 0x544c5331],
            bytes_generated: 0,
        }
    }

    /// Mistura 32 bytes de entropia fresca no estado interno
    pub fn mix_entropy(&mut self, seed: &[u8; 32]) {
        for (i, chunk) in seed.chunks_exact(4).enumerate() {
            let val = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            self.key[i % 8] ^= val.rotate_left(i as u32 * 3);
        }
        self.counter = self.counter.wrapping_add(1);
    }

    /// Executa 20 rodadas da permutação ChaCha20 e gera um bloco de 64 bytes
    pub fn generate_block(&mut self, out: &mut [u8; 64]) {
        let mut state = [0u32; 16];
        state[0..4].copy_from_slice(&CHACHA_CONSTANTS);
        state[4..12].copy_from_slice(&self.key);
        state[12] = self.counter;
        state[13..16].copy_from_slice(&self.nonce);

        let mut working = state;

        for _ in 0..10 {
            // Column rounds
            qr(&mut working, 0, 4, 8, 12);
            qr(&mut working, 1, 5, 9, 13);
            qr(&mut working, 2, 6, 10, 14);
            qr(&mut working, 3, 7, 11, 15);
            // Diagonal rounds
            qr(&mut working, 0, 5, 10, 15);
            qr(&mut working, 1, 6, 11, 12);
            qr(&mut working, 2, 7, 8, 13);
            qr(&mut working, 3, 4, 9, 14);
        }

        for i in 0..16 {
            let word = working[i].wrapping_add(state[i]);
            let bytes = word.to_le_bytes();
            out[i * 4..i * 4 + 4].copy_from_slice(&bytes);
        }

        self.counter = self.counter.wrapping_add(1);
        self.bytes_generated += 64;

        // Se counter transbordar ou após 64 KiB, altera o nonce e re-mistura
        if self.counter == 0 || self.bytes_generated > 65536 {
            self.nonce[0] = self.nonce[0].wrapping_add(1);
            self.bytes_generated = 0;
        }
    }
}

#[inline(always)]
fn qr(st: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    st[a] = st[a].wrapping_add(st[b]); st[d] ^= st[a]; st[d] = st[d].rotate_left(16);
    st[c] = st[c].wrapping_add(st[d]); st[b] ^= st[c]; st[b] = st[b].rotate_left(12);
    st[a] = st[a].wrapping_add(st[b]); st[d] ^= st[a]; st[d] = st[d].rotate_left(8);
    st[c] = st[c].wrapping_add(st[d]); st[b] ^= st[c]; st[b] = st[b].rotate_left(7);
}

static CSPRNG: SpinLock<CsprngState> = SpinLock::new(CsprngState::new());

/// Executa RDRAND em hardware x86_64 se suportado
#[inline]
pub fn rdrand64() -> Option<u64> {
    if !HAS_RDRAND.load(Ordering::Relaxed) {
        return None;
    }
    let mut val: u64 = 0;
    let ok: u8;
    unsafe {
        core::arch::asm!(
            "rdrand {0}",
            "setc {1}",
            out(reg) val,
            out(reg_byte) ok,
            options(nomem, nostack)
        );
    }
    if ok != 0 { Some(val) } else { None }
}

/// Executa RDSEED em hardware x86_64 se suportado
#[inline]
pub fn rdseed64() -> Option<u64> {
    if !HAS_RDSEED.load(Ordering::Relaxed) {
        return None;
    }
    let mut val: u64 = 0;
    let ok: u8;
    unsafe {
        core::arch::asm!(
            "rdseed {0}",
            "setc {1}",
            out(reg) val,
            out(reg_byte) ok,
            options(nomem, nostack)
        );
    }
    if ok != 0 { Some(val) } else { None }
}

/// Coleta pool de entropia combinando RDRAND, RDSEED e jitter temporal de periféricos
pub fn gather_entropy_sample() -> [u8; 32] {
    let mut sample = [0u8; 32];

    // 1. Tenta RDRAND / RDSEED de hardware
    for i in 0..4 {
        let hw_rand = rdseed64().or_else(rdrand64).unwrap_or_else(|| {
            // Jitter fallback: RDTSC + PIT tick + endereço de pilha
            let tsc = unsafe { core::arch::x86_64::_rdtsc() };
            let pit = crate::timer::ticks();
            let mut port_jitter: u8 = 0;
            unsafe {
                core::arch::asm!("in al, dx", in("dx") 0x40u16, out("al") port_jitter, options(nomem, nostack));
            }
            tsc ^ (pit << 16) ^ ((port_jitter as u64) << 48) ^ (i as u64 * 0x9e3779b97f4a7c15)
        });
        sample[i * 8..i * 8 + 8].copy_from_slice(&hw_rand.to_le_bytes());
    }

    sample
}

/// Preenche um buffer de bytes com números pseudoaleatórios criptograficamente seguros
pub fn fill_random_bytes(dest: &mut [u8]) {
    let mut guard = CSPRNG.lock();
    let mut offset = 0;
    let mut block = [0u8; 64];

    // Ocasionalmente re-injeta entropia fresca
    let entropy = gather_entropy_sample();
    guard.mix_entropy(&entropy);

    while offset < dest.len() {
        guard.generate_block(&mut block);
        let chunk = (dest.len() - offset).min(64);
        dest[offset..offset + chunk].copy_from_slice(&block[..chunk]);
        offset += chunk;
    }
}

/// Inicializa o subsistema de entropia detectando recursos de CPUID
pub fn init() {
    let cpu = crate::cpu::CpuInfo::probe();
    HAS_RDRAND.store(cpu.has_rdrand, Ordering::SeqCst);
    HAS_RDSEED.store(cpu.has_rdseed, Ordering::SeqCst);

    // Semeia o CSPRNG inicialmente com amostra do pool
    let initial_sample = gather_entropy_sample();
    let mut guard = CSPRNG.lock();
    guard.mix_entropy(&initial_sample);
}

pub fn has_rdrand() -> bool {
    HAS_RDRAND.load(Ordering::Relaxed)
}

pub fn has_rdseed() -> bool {
    HAS_RDSEED.load(Ordering::Relaxed)
}
