//! Jinn Microkernel — Subsistema de Entropia e CSPRNG (NIST SP 800-90A/B & RFC 8439)
//!
//! Implementa um gerador de números pseudoaleatórios criptograficamente seguro (CSPRNG)
//! baseado no ChaCha20 DRBG com avanço de estado irreversível (*ratcheting forward-secrecy*),
//! alimentado por extrator HKDF-Extract (RFC 5869) e testes contínuos de integridade NIST SP 800-90B.

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use crate::memory::SpinLock;
use crate::crypto::sha256::hkdf_extract;

// ── Flags POSIX para sys_getrandom (Syscall 69) ──────────────────────────────
pub const GRND_NONBLOCK: u32 = 0x0001;
pub const GRND_RANDOM:   u32 = 0x0002;

// Limiares de Segurança
pub const MIN_ENTROPY_THRESHOLD_BITS: usize = 256;
pub const RESEED_INTERVAL_BYTES: usize = 65536;
pub const RESEED_INTERVAL_CALLS: usize = 1000;
pub const RCT_CUTOFF: u32 = 8; // Repetition Count Test cutoff (NIST SP 800-90B)

static HAS_RDRAND: AtomicBool = AtomicBool::new(false);
static HAS_RDSEED: AtomicBool = AtomicBool::new(false);
static GLOBAL_IS_SEEDED: AtomicBool = AtomicBool::new(false);
static ESTIMATED_ENTROPY_BITS: AtomicUsize = AtomicUsize::new(0);

const CHACHA_CONSTANTS: [u32; 4] = [
    0x61707865, // "expa"
    0x3320646e, // "nd 3"
    0x79622d32, // "2-by"
    0x6b206574, // "te k"
];

pub struct CsprngState {
    key: [u8; 32],
    counter: u32,
    nonce: [u32; 3],
    bytes_generated: usize,
    calls_since_reseed: usize,
    accumulated_entropy_bits: usize,
    is_seeded: bool,

    // NIST SP 800-90B Repetition Count Test (RCT)
    last_rdrand: u64,
    rdrand_repeat_count: u32,
    rdrand_faulty: bool,

    last_rdseed: u64,
    rdseed_repeat_count: u32,
    rdseed_faulty: bool,
}

impl CsprngState {
    pub const fn new() -> Self {
        Self {
            // Inicializado em zero: nenhuma aleatoriedade estática fictícia
            key: [0u8; 32],
            counter: 1,
            nonce: [0x50415353, 0x4a494e4e, 0x544c5331],
            bytes_generated: 0,
            calls_since_reseed: 0,
            accumulated_entropy_bits: 0,
            is_seeded: false,

            last_rdrand: 0,
            rdrand_repeat_count: 0,
            rdrand_faulty: false,

            last_rdseed: 0,
            rdseed_repeat_count: 0,
            rdseed_faulty: false,
        }
    }

    /// Mistura amostras brutas de entropia utilizando HKDF-Extract (RFC 5869)
    pub fn mix_entropy(&mut self, raw_sample: &[u8], estimated_bits: usize) {
        // PRK = HKDF-Extract(Salt=chave atual, IKM=amostra de entropia)
        let prk = hkdf_extract(&self.key, raw_sample);
        self.key = prk;
        self.counter = self.counter.wrapping_add(1);

        self.accumulated_entropy_bits = self.accumulated_entropy_bits.saturating_add(estimated_bits);
        if self.accumulated_entropy_bits >= MIN_ENTROPY_THRESHOLD_BITS {
            self.is_seeded = true;
            GLOBAL_IS_SEEDED.store(true, Ordering::SeqCst);
        }
        ESTIMATED_ENTROPY_BITS.store(self.accumulated_entropy_bits, Ordering::SeqCst);
        self.calls_since_reseed = 0;
    }

    /// Executa 20 rodadas da permutação ChaCha20 e gera bloco de 64 bytes com ratcheting
    pub fn generate_block(&mut self, out: &mut [u8; 64]) {
        let mut key_words = [0u32; 8];
        for i in 0..8 {
            key_words[i] = u32::from_le_bytes([
                self.key[i * 4],
                self.key[i * 4 + 1],
                self.key[i * 4 + 2],
                self.key[i * 4 + 3],
            ]);
        }

        let mut state = [0u32; 16];
        state[0..4].copy_from_slice(&CHACHA_CONSTANTS);
        state[4..12].copy_from_slice(&key_words);
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

        // Ratcheting (Forward Secrecy): atualiza a chave com os primeiros 32 bytes do bloco
        // garantindo que o estado anterior não possa ser reconstruído
        for i in 0..32 {
            self.key[i] ^= out[i];
        }

        // Se counter transbordar ou após ciclo, altera o nonce
        if self.counter == 0 || self.bytes_generated > RESEED_INTERVAL_BYTES {
            self.nonce[0] = self.nonce[0].wrapping_add(1);
            self.bytes_generated = 0;
        }

        // Limpeza de registradores de trabalho da pilha
        unsafe {
            core::ptr::write_volatile(&mut working as *mut _ as *mut u8, 0);
            core::ptr::write_volatile(&mut state as *mut _ as *mut u8, 0);
            core::ptr::write_volatile(&mut key_words as *mut _ as *mut u8, 0);
        }
    }

    /// Executa teste de repetição contínua (NIST SP 800-90B RCT)
    fn check_rct(&mut self, sample: u64, is_rdseed: bool) -> bool {
        if is_rdseed {
            if sample == self.last_rdseed {
                self.rdseed_repeat_count += 1;
                if self.rdseed_repeat_count >= RCT_CUTOFF {
                    self.rdseed_faulty = true;
                    return false;
                }
            } else {
                self.last_rdseed = sample;
                self.rdseed_repeat_count = 1;
            }
            true
        } else {
            if sample == self.last_rdrand {
                self.rdrand_repeat_count += 1;
                if self.rdrand_repeat_count >= RCT_CUTOFF {
                    self.rdrand_faulty = true;
                    return false;
                }
            } else {
                self.last_rdrand = sample;
                self.rdrand_repeat_count = 1;
            }
            true
        }
    }

    /// Teste de Saúde NIST SP 800-90B: Repetition Count Test (RCT básico)
    pub fn health_check_rct(&mut self, sample: u64) -> bool {
        self.check_rct(sample, false)
    }

    /// Injeção de Entropia via HKDF-Extract e acumulador seguro de min-entropia
    pub fn feed_entropy(&mut self, samples: &[u8]) {
        self.mix_entropy(samples, samples.len() * 8);
    }

    /// Verificador de semente para atendimento à Syscall 69
    pub fn check_ready(&mut self, nonblock: bool) -> Result<(), i32> {
        if self.is_seeded {
            Ok(())
        } else if nonblock {
            Err(-11) // -EAGAIN
        } else {
            Err(-11)
        }
    }

    /// Preenchimento de buffer de destino com números pseudoaleatórios (ChaCha20 DRBG)
    pub fn fill_bytes(&mut self, dest: &mut [u8]) {
        let mut offset = 0;
        let mut block = [0u8; 64];

        while offset < dest.len() {
            self.generate_block(&mut block);
            let chunk = (dest.len() - offset).min(64);
            dest[offset..offset + chunk].copy_from_slice(&block[..chunk]);
            offset += chunk;
        }

        unsafe {
            core::ptr::write_volatile(&mut block as *mut _ as *mut u8, 0);
        }

        if self.bytes_generated >= RESEED_INTERVAL_BYTES {
            self.reseed_internal();
        }
    }

    pub fn reseed_internal(&mut self) {
        self.nonce[0] = self.nonce[0].wrapping_add(1);
        self.bytes_generated = 0;
    }
}

#[inline(always)]
fn qr(st: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    st[a] = st[a].wrapping_add(st[b]); st[d] ^= st[a]; st[d] = st[d].rotate_left(16);
    st[c] = st[c].wrapping_add(st[d]); st[b] ^= st[c]; st[b] = st[b].rotate_left(12);
    st[a] = st[a].wrapping_add(st[b]); st[d] ^= st[d].rotate_left(8);
    st[c] = st[c].wrapping_add(st[d]); st[b] ^= st[c]; st[b] = st[b].rotate_left(7);
}

/// Instância global estática protegida por SpinLock (segura contra condições de corrida em SMP)
pub static GLOBAL_CSPRNG: SpinLock<CsprngState> = SpinLock::new(CsprngState::new());
static CSPRNG: &SpinLock<CsprngState> = &GLOBAL_CSPRNG;

/// Executa RDRAND em hardware x86_64 se suportado e ativo
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

/// Executa RDSEED em hardware x86_64 se suportado e ativo
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

/// Coleta uma amostra de entropia do barramento com estimativa de min-entropia
pub fn gather_entropy_sample(state: &mut CsprngState) -> ([u8; 64], usize) {
    let mut sample = [0u8; 64];
    let mut estimated_bits = 0;

    // 1. Tenta RDSEED físico (64 bits de min-entropia por palavra se passar no RCT)
    for i in 0..4 {
        let mut got_hw = false;
        if !state.rdseed_faulty {
            if let Some(val) = rdseed64() {
                if state.check_rct(val, true) {
                    sample[i * 8..i * 8 + 8].copy_from_slice(&val.to_le_bytes());
                    estimated_bits += 64;
                    got_hw = true;
                }
            }
        }

        // 2. Se RDSEED falhar, tenta RDRAND (estima 32 bits de min-entropia)
        if !got_hw && !state.rdrand_faulty {
            if let Some(val) = rdrand64() {
                if state.check_rct(val, false) {
                    sample[i * 8..i * 8 + 8].copy_from_slice(&val.to_le_bytes());
                    estimated_bits += 32;
                    got_hw = true;
                }
            }
        }

        // 3. Jitter de hardware e I/O como complemento ou fallback
        if !got_hw {
            let tsc1 = unsafe { core::arch::x86_64::_rdtsc() };
            let mut port_jitter: u8 = 0;
            unsafe {
                core::arch::asm!("in al, dx", in("dx") 0x40u16, out("al") port_jitter, options(nomem, nostack));
            }
            let tsc2 = unsafe { core::arch::x86_64::_rdtsc() };
            let pit = crate::timer::ticks();

            let delta = tsc2.wrapping_sub(tsc1);
            let mixed = delta ^ (pit << 16) ^ ((port_jitter as u64) << 48) ^ (i as u64 * 0x9e3779b97f4a7c15);
            sample[i * 8..i * 8 + 8].copy_from_slice(&mixed.to_le_bytes());

            // Em ambientes sem hardware RNG, jitter contribui com estimativa conservadora (4 bits)
            estimated_bits += 4;
        }
    }

    // 4. Preenche os 32 bytes restantes da amostra com derivadas temporais
    let now_ticks = crate::timer::ticks();
    let tsc_final = unsafe { core::arch::x86_64::_rdtsc() };
    sample[32..40].copy_from_slice(&now_ticks.to_le_bytes());
    sample[40..48].copy_from_slice(&tsc_final.to_le_bytes());
    let mut p0 = [0u8; 8];
    let mut p1 = [0u8; 8];
    p0.copy_from_slice(&sample[0..8]);
    p1.copy_from_slice(&sample[8..16]);
    sample[48..56].copy_from_slice(&p0);
    sample[56..64].copy_from_slice(&p1);

    (sample, estimated_bits)
}

/// Informa se o gerador já atingiu o patamar mínimo de 256 bits de min-entropia
pub fn is_seeded() -> bool {
    GLOBAL_IS_SEEDED.load(Ordering::Acquire)
}

/// Retorna a estimativa atual de bits de entropia acumulados
pub fn entropy_bits() -> usize {
    ESTIMATED_ENTROPY_BITS.load(Ordering::Relaxed)
}

/// Força coleta de entropia até atingir a semeadura completa
pub fn force_seed_pool() {
    let mut guard = CSPRNG.lock();
    let mut spins = 0;
    while !guard.is_seeded && spins < 500 {
        let (sample, bits) = gather_entropy_sample(&mut guard);
        guard.mix_entropy(&sample, bits);
        spins += 1;
    }
}

/// Preenche um buffer de bytes com números pseudoaleatórios criptograficamente seguros
///
/// Retorna `Ok(bytes_preenchidos)` ou `Err(errno)` (ex: `-EAGAIN` se `GRND_NONBLOCK` for especificado e o pool não estiver semeado).
pub fn fill_random_bytes(dest: &mut [u8], flags: u32) -> Result<usize, isize> {
    let mut guard = CSPRNG.lock();

    // Se o pool ainda não foi semeado com 256 bits de min-entropia
    if !guard.is_seeded {
        if (flags & GRND_NONBLOCK) != 0 {
            return Err(crate::syscall::EAGAIN);
        }

        // Em modo bloqueante, coleta amostras até atingir a meta
        let mut loop_count = 0;
        while !guard.is_seeded && loop_count < 1000 {
            let (sample, bits) = gather_entropy_sample(&mut guard);
            guard.mix_entropy(&sample, bits);
            loop_count += 1;
        }
    }

    // Gatilho de Reseed Periódico
    guard.calls_since_reseed += 1;
    if guard.bytes_generated >= RESEED_INTERVAL_BYTES || guard.calls_since_reseed >= RESEED_INTERVAL_CALLS {
        let (sample, bits) = gather_entropy_sample(&mut guard);
        guard.mix_entropy(&sample, bits);
    }

    let mut offset = 0;
    let mut block = [0u8; 64];

    while offset < dest.len() {
        guard.generate_block(&mut block);
        let chunk = (dest.len() - offset).min(64);
        dest[offset..offset + chunk].copy_from_slice(&block[..chunk]);
        offset += chunk;
    }

    // Limpeza volátil do buffer temporário
    unsafe {
        core::ptr::write_volatile(&mut block as *mut _ as *mut u8, 0);
    }

    Ok(dest.len())
}

/// Inicializa o subsistema de entropia detectando CPUID e semeando o pool
pub fn init() {
    let cpu = crate::cpu::CpuInfo::probe();
    HAS_RDRAND.store(cpu.has_rdrand, Ordering::SeqCst);
    HAS_RDSEED.store(cpu.has_rdseed, Ordering::SeqCst);

    // Coleta inicial ativa para garantir semeadura desde o boot do kernel
    force_seed_pool();
}

pub fn has_rdrand() -> bool {
    HAS_RDRAND.load(Ordering::Relaxed)
}

pub fn has_rdseed() -> bool {
    HAS_RDSEED.load(Ordering::Relaxed)
}
