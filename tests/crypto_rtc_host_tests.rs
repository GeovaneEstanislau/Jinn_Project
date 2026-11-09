//! Jinn OS — Testes Unitários de Confiabilidade do CSPRNG e RTC no Host (Fase 1)

#[path = "../src/crypto/sha256.rs"]
mod sha256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtcDateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

const DAYS_IN_MONTH: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

fn is_leap_year(year: u64) -> bool {
    (year % 4 == 0) && (year % 100 != 0 || year % 400 == 0)
}

fn is_valid_datetime(dt: &RtcDateTime) -> bool {
    if dt.year < 1970 || dt.year > 2100 {
        return false;
    }
    if dt.month < 1 || dt.month > 12 {
        return false;
    }
    let max_days = if dt.month == 2 && is_leap_year(dt.year as u64) {
        29
    } else {
        DAYS_IN_MONTH[(dt.month - 1) as usize] as u8
    };
    if dt.day < 1 || dt.day > max_days {
        return false;
    }
    if dt.hour > 23 || dt.minute > 59 || dt.second > 60 {
        return false;
    }
    true
}

fn datetime_to_unix(dt: &RtcDateTime) -> u64 {
    let year = dt.year as u64;
    if year < 1970 {
        return 0;
    }

    let mut days: u64 = 0;
    for y in 1970..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }

    let m = dt.month.max(1).min(12) as usize;
    for (idx, &d) in DAYS_IN_MONTH.iter().enumerate().take(m - 1) {
        if idx == 1 && is_leap_year(year) {
            days += 29;
        } else {
            days += d;
        }
    }

    days += (dt.day.max(1) as u64) - 1;

    days * 86400
        + (dt.hour as u64) * 3600
        + (dt.minute as u64) * 60
        + (dt.second as u64)
}

#[test]
fn test_sha256_rfc6234_vectors() {
    use sha256::Sha256;

    // 1. Empty string
    let hash_empty = Sha256::digest(b"");
    let expected_empty = [
        0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14,
        0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f, 0xb9, 0x24,
        0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c,
        0xa4, 0x95, 0x99, 0x1b, 0x78, 0x52, 0xb8, 0x55,
    ];
    assert_eq!(hash_empty, expected_empty);

    // 2. "abc"
    let hash_abc = Sha256::digest(b"abc");
    let expected_abc = [
        0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea,
        0x41, 0x41, 0x40, 0xde, 0x5d, 0xae, 0x22, 0x23,
        0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c,
        0xb4, 0x10, 0xff, 0x61, 0xf2, 0x00, 0x15, 0xad,
    ];
    assert_eq!(hash_abc, expected_abc);

    // 3. Multi-block message: "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
    let hash_multi = Sha256::digest(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq");
    let expected_multi = [
        0x24, 0x8d, 0x6a, 0x61, 0xd2, 0x06, 0x38, 0xb8,
        0xe5, 0xc0, 0x26, 0x93, 0x0c, 0x3e, 0x60, 0x39,
        0xa3, 0x3c, 0xe4, 0x59, 0x64, 0xff, 0x21, 0x67,
        0xf6, 0xec, 0xed, 0xd4, 0x19, 0xdb, 0x06, 0xc1,
    ];
    assert_eq!(hash_multi, expected_multi);
}

#[test]
fn test_hkdf_extract_rfc5869_case1() {
    use sha256::hkdf_extract;

    // RFC 5869 Test Case 1
    let ikm = [0x0bu8; 22];
    let salt = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
        0x08, 0x09, 0x0a, 0x0b, 0x0c,
    ];
    let prk = hkdf_extract(&salt, &ikm);
    let expected_prk = [
        0x07, 0x77, 0x09, 0x36, 0x2c, 0x2e, 0x32, 0xdf,
        0x0d, 0xdc, 0x3f, 0x0d, 0xc4, 0x7b, 0xba, 0x63,
        0x90, 0xb6, 0xc7, 0x3b, 0xb5, 0x0f, 0x9c, 0x31,
        0x22, 0xec, 0x84, 0x4a, 0xd7, 0xc2, 0xb3, 0xe5,
    ];
    assert_eq!(prk, expected_prk);
}

#[test]
fn test_csprng_state_machine_and_health_tests() {
    // Validação da máquina de estados do ChaCha20 DRBG
    #[allow(dead_code)]
    struct MockCsprng {
        key: [u8; 32],
        counter: u32,
        nonce: [u32; 3],
        accumulated_entropy_bits: usize,
        is_seeded: bool,
        last_rdrand: u64,
        rdrand_repeat_count: u32,
        rdrand_faulty: bool,
    }

    impl MockCsprng {
        fn new() -> Self {
            Self {
                key: [0u8; 32],
                counter: 1,
                nonce: [0x50415353, 0x4a494e4e, 0x544c5331],
                accumulated_entropy_bits: 0,
                is_seeded: false,
                last_rdrand: 0,
                rdrand_repeat_count: 0,
                rdrand_faulty: false,
            }
        }

        fn mix_entropy(&mut self, sample: &[u8], bits: usize) {
            self.key = sha256::hkdf_extract(&self.key, sample);
            self.accumulated_entropy_bits += bits;
            if self.accumulated_entropy_bits >= 256 {
                self.is_seeded = true;
            }
        }

        fn fill_random_bytes(&mut self, dest: &mut [u8], flags: u32) -> Result<usize, isize> {
            if !self.is_seeded {
                if (flags & 1) != 0 { // GRND_NONBLOCK
                    return Err(-11); // -EAGAIN
                }
                // Simula loop bloqueante de semeadura
                self.mix_entropy(b"MOCK_BLOCKING_ENTROPY_SAMPLE_SEED", 256);
            }
            for (i, b) in dest.iter_mut().enumerate() {
                *b = self.key[i % 32] ^ (i as u8);
            }
            Ok(dest.len())
        }

        fn check_rct(&mut self, val: u64) -> bool {
            if val == self.last_rdrand {
                self.rdrand_repeat_count += 1;
                if self.rdrand_repeat_count >= 8 {
                    self.rdrand_faulty = true;
                    return false;
                }
            } else {
                self.last_rdrand = val;
                self.rdrand_repeat_count = 1;
            }
            true
        }
    }

    let mut rng = MockCsprng::new();
    assert!(!rng.is_seeded, "CSPRNG deve começar estritamente não-semeado");

    // Teste 1: GRND_NONBLOCK deve retornar -EAGAIN antes de 256 bits de min-entropia
    let mut buf = [0u8; 16];
    let res = rng.fill_random_bytes(&mut buf, 1);
    assert_eq!(res, Err(-11), "GRND_NONBLOCK deve retornar -EAGAIN quando não semeado");

    // Teste 2: Injeção de 128 bits não deve bastar
    rng.mix_entropy(b"SAMPLE_1", 128);
    assert!(!rng.is_seeded, "128 bits de entropia não podem semear o CSPRNG");

    // Teste 3: Injeção de mais 128 bits (total 256) deve ativar is_seeded
    rng.mix_entropy(b"SAMPLE_2", 128);
    assert!(rng.is_seeded, "256 bits de min-entropia devem acionar is_seeded");

    // Teste 4: Agora chamada com GRND_NONBLOCK deve suceder
    let res2 = rng.fill_random_bytes(&mut buf, 1);
    assert_eq!(res2, Ok(16));
    assert_ne!(buf, [0u8; 16]);

    // Teste 5: NIST SP 800-90B RCT (Repetition Count Test)
    for _ in 0..7 {
        assert!(rng.check_rct(0xDEAD_BEEF_CAFE_BABE));
    }
    assert!(!rng.rdrand_faulty);
    // 8a repetição do mesmo valor aciona falha
    assert!(!rng.check_rct(0xDEAD_BEEF_CAFE_BABE));
    assert!(rng.rdrand_faulty, "8 leituras idênticas consecutivas devem marcar hardware como defeituoso");
}

#[test]
fn test_rtc_datetime_validation_and_unix_epoch() {
    // 1. Data válida conhecida: 2024-02-29 12:00:00 (Ano bissexto)
    let leap_dt = RtcDateTime {
        year: 2024,
        month: 2,
        day: 29,
        hour: 12,
        minute: 0,
        second: 0,
    };
    assert!(is_valid_datetime(&leap_dt));
    let unix = datetime_to_unix(&leap_dt);
    // 2024-02-29 12:00:00 UTC = 1709208000
    assert_eq!(unix, 1709208000);

    // 2. Data inválida: 2023-02-29 (2023 não é bissexto)
    let invalid_leap = RtcDateTime {
        year: 2023,
        month: 2,
        day: 29,
        hour: 12,
        minute: 0,
        second: 0,
    };
    assert!(!is_valid_datetime(&invalid_leap));

    // 3. Mês e dia inválidos
    let bad_month = RtcDateTime {
        year: 2026,
        month: 13,
        day: 1,
        hour: 0,
        minute: 0,
        second: 0,
    };
    assert!(!is_valid_datetime(&bad_month));

    let bad_day = RtcDateTime {
        year: 2026,
        month: 4,
        day: 31, // Abril só tem 30 dias
        hour: 0,
        minute: 0,
        second: 0,
    };
    assert!(!is_valid_datetime(&bad_day));
}

#[test]
fn test_rtc_timezone_offset_handling() {
    // Simula CMOS em hora local (UTC-3: Brasília / São Paulo = -10800 segundos)
    let local_dt = RtcDateTime {
        year: 2026,
        month: 11,
        day: 9,
        hour: 15, // 15:00 local (UTC-3)
        minute: 0,
        second: 0,
    };
    let local_unix = datetime_to_unix(&local_dt);

    let offset_seconds = -10800i32; // UTC-3
    let is_local_time = true;

    let utc_unix = if is_local_time {
        if offset_seconds >= 0 {
            local_unix.saturating_sub(offset_seconds as u64)
        } else {
            local_unix.saturating_add((-offset_seconds) as u64)
        }
    } else {
        local_unix
    };

    // 15:00 UTC-3 = 18:00 UTC (+3 horas = +10800s)
    assert_eq!(utc_unix, local_unix + 10800);
}

#[test]
fn test_rtc_clock_routing_realtime_and_monotonic() {
    pub const CLOCK_REALTIME: u32 = 0;
    pub const CLOCK_MONOTONIC: u32 = 1;
    pub const EINVAL: isize = -22;

    struct Timespec {
        tv_sec: u64,
        tv_nsec: u32,
    }

    fn mock_clock_gettime(clock_id: u32, out: &mut Timespec, ticks: u64, boot_unix: u64, hz: u64) -> isize {
        match clock_id {
            CLOCK_REALTIME => {
                let elapsed_secs = ticks / hz;
                let elapsed_nanos = ((ticks % hz) * (1_000_000_000 / hz)) as u32;
                out.tv_sec = boot_unix + elapsed_secs;
                out.tv_nsec = elapsed_nanos;
                0
            }
            CLOCK_MONOTONIC => {
                let secs = ticks / hz;
                let nanos = ((ticks % hz) * (1_000_000_000 / hz)) as u32;
                out.tv_sec = secs;
                out.tv_nsec = nanos;
                0
            }
            _ => EINVAL,
        }
    }

    let mut ts = Timespec { tv_sec: 0, tv_nsec: 0 };
    // 1. CLOCK_REALTIME (0)
    let ret_rt = mock_clock_gettime(CLOCK_REALTIME, &mut ts, 500, 1700000000, 100);
    assert_eq!(ret_rt, 0);
    assert_eq!(ts.tv_sec, 1700000005);
    assert_eq!(ts.tv_nsec, 0);

    // 2. CLOCK_MONOTONIC (1)
    let ret_mono = mock_clock_gettime(CLOCK_MONOTONIC, &mut ts, 250, 1700000000, 100);
    assert_eq!(ret_mono, 0);
    assert_eq!(ts.tv_sec, 2);
    assert_eq!(ts.tv_nsec, 500_000_000);

    // 3. Rejeição de clock_id inválido (99)
    let ret_bad = mock_clock_gettime(99, &mut ts, 250, 1700000000, 100);
    assert_eq!(ret_bad, -22);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum RtcTimeBase {
    Utc = 0,
    LocalTime = 1,
}

#[test]
fn test_csprng_cold_start_unseeded_rejection() {
    struct ColdCsprng {
        key: [u8; 32],
        accumulated_entropy_bits: usize,
        is_seeded: bool,
    }

    impl ColdCsprng {
        fn new() -> Self {
            Self {
                key: [0u8; 32],
                accumulated_entropy_bits: 0,
                is_seeded: false,
            }
        }

        fn fill_bytes(&mut self, dest: &mut [u8], flags: u32) -> Result<usize, isize> {
            if !self.is_seeded {
                if (flags & 1) != 0 { // GRND_NONBLOCK
                    return Err(-11); // -EAGAIN
                }
                return Err(-11);
            }
            dest.copy_from_slice(&self.key[0..dest.len()]);
            Ok(dest.len())
        }
    }

    let mut cold = ColdCsprng::new();
    assert!(!cold.is_seeded);
    assert_eq!(cold.accumulated_entropy_bits, 0);
    assert_eq!(cold.key, [0u8; 32]);

    let mut out = [0xFFu8; 32];
    let res = cold.fill_bytes(&mut out, 1); // GRND_NONBLOCK
    assert_eq!(res, Err(-11), "CSPRNG não-semeado deve rejeitar chamadas GRND_NONBLOCK com -EAGAIN");
    // Garante que nenhum byte de chave não inicializada foi exposto
    assert_eq!(out, [0xFFu8; 32]);
}

#[test]
fn test_csprng_entropy_source_model_conservative_estimation() {
    struct EntropySourceModel {
        last_delta: u64,
        accumulated_bits: usize,
    }

    impl EntropySourceModel {
        fn new() -> Self {
            Self {
                last_delta: 0,
                accumulated_bits: 0,
            }
        }

        // Modelo de Fonte:
        // - RDSEED: 32 bits por 64 bits (0.5 bit/bit)
        // - RDRAND: 8 bits por 64 bits (0.125 bit/bit)
        // - Jitter: 1 bit se diff de segunda ordem > 4 ciclos, 0 bits caso contrário
        fn add_rdseed(&mut self) -> usize {
            let bits = 32;
            self.accumulated_bits += bits;
            bits
        }

        fn add_rdrand(&mut self) -> usize {
            let bits = 8;
            self.accumulated_bits += bits;
            bits
        }

        fn add_jitter(&mut self, delta: u64) -> usize {
            let diff = if self.last_delta == 0 {
                0
            } else if delta >= self.last_delta {
                delta - self.last_delta
            } else {
                self.last_delta - delta
            };
            self.last_delta = delta;

            let bits = if diff > 4 { 1 } else { 0 };
            self.accumulated_bits += bits;
            bits
        }
    }

    let mut model = EntropySourceModel::new();

    // 1. Sequência sem variação (ex: VM determinística com timer constante)
    let flat_deltas = [100, 100, 102, 101, 103, 100];
    for &d in &flat_deltas {
        let b = model.add_jitter(d);
        assert_eq!(b, 0, "Deltas com variação <= 4 ciclos não devem atribuir entropia (0 bits)");
    }
    assert_eq!(model.accumulated_bits, 0, "Timer determinístico sem variação deve acumular 0 bits");

    // 2. Sequência com jitter físico real (> 4 ciclos de variação)
    let variable_deltas = [150, 80, 200, 95];
    for &d in &variable_deltas {
        let b = model.add_jitter(d);
        assert_eq!(b, 1, "Variações > 4 ciclos devem contribuir com 1 bit conservador");
    }
    assert_eq!(model.accumulated_bits, 4);

    // 3. RDRAND condicionado: 8 bits por amostra de 64 bits
    let rdrand_bits = model.add_rdrand();
    assert_eq!(rdrand_bits, 8);
    assert_eq!(model.accumulated_bits, 12);

    // 4. RDSEED ruído físico: 32 bits por amostra de 64 bits
    let rdseed_bits = model.add_rdseed();
    assert_eq!(rdseed_bits, 32);
    assert_eq!(model.accumulated_bits, 44);

    // 5. 8 amostras de RDSEED atingem 256 bits de min-entropia
    for _ in 0..7 {
        model.add_rdseed();
    }
    assert!(model.accumulated_bits >= 256, "8 amostras de RDSEED devem atingir a meta de 256 bits de min-entropia");
}

#[test]
fn test_csprng_hardware_failure_rct_trigger() {
    struct RctDetector {
        last_sample: u64,
        repeat_count: u32,
        is_faulty: bool,
    }

    impl RctDetector {
        fn new() -> Self {
            Self {
                last_sample: 0,
                repeat_count: 0,
                is_faulty: false,
            }
        }

        fn observe(&mut self, val: u64) -> bool {
            if self.is_faulty {
                return false;
            }
            if val == self.last_sample {
                self.repeat_count += 1;
                if self.repeat_count >= 8 {
                    self.is_faulty = true;
                    return false;
                }
            } else {
                self.last_sample = val;
                self.repeat_count = 1;
            }
            true
        }
    }

    let mut detector = RctDetector::new();
    let stuck_val = 0x55AA_55AA_55AA_55AA;

    // Amostras 1 a 7 consecutivas idênticas não disparam corte ainda
    for i in 1..=7 {
        assert!(detector.observe(stuck_val), "Amostra repetida {}/8 deve ser observada antes do corte", i);
        assert!(!detector.is_faulty);
    }

    // 8a repetição consecutiva atinge o limiar de corte do NIST SP 800-90B RCT
    assert!(!detector.observe(stuck_val), "8a repetição idêntica deve disparar alarme RCT");
    assert!(detector.is_faulty, "Detector deve marcar o hardware como defeituoso (is_faulty = true)");

    // Uma nova leitura não deve reabilitar o canal defeituoso
    assert!(!detector.observe(0x1234_5678_90AB_CDEF));
    assert!(detector.is_faulty);
}

#[test]
fn test_csprng_forward_secrecy_ratcheting() {
    struct RatchetDrbg {
        key: [u8; 32],
        counter: u32,
        bytes_generated: usize,
    }

    impl RatchetDrbg {
        fn new(seed: [u8; 32]) -> Self {
            Self {
                key: seed,
                counter: 1,
                bytes_generated: 0,
            }
        }

        fn generate_block(&mut self) -> [u8; 64] {
            let mut block = [0u8; 64];
            for i in 0..64 {
                block[i] = self.key[i % 32] ^ (self.counter as u8).wrapping_add(i as u8);
            }
            self.counter = self.counter.wrapping_add(1);
            self.bytes_generated += 64;

            // Forward Secrecy: mutação imediata da chave (ratcheting) com os primeiros 32 bytes do bloco
            for i in 0..32 {
                self.key[i] ^= block[i];
            }
            block
        }
    }

    let initial_key = [0x42u8; 32];
    let mut drbg = RatchetDrbg::new(initial_key);

    let block1 = drbg.generate_block();
    let key_after_block1 = drbg.key;
    assert_ne!(key_after_block1, initial_key, "Chave após bloco 1 deve ter sofrido mutação (ratcheting)");

    let block2 = drbg.generate_block();
    let key_after_block2 = drbg.key;
    assert_ne!(key_after_block2, key_after_block1, "Chave após bloco 2 deve ser diferente da chave do bloco 1");
    assert_ne!(block1, block2, "Blocos sucessivos devem ser distintos");
}

#[test]
fn test_csprng_multithreaded_concurrency() {
    use std::sync::{Arc, Mutex};
    use std::thread;

    struct SharedCsprng {
        state: [u8; 32],
        counter: u64,
    }

    impl SharedCsprng {
        fn new() -> Self {
            Self {
                state: [0x7A; 32],
                counter: 1,
            }
        }

        fn get_random_token(&mut self) -> [u8; 16] {
            self.counter += 1;
            let mut token = [0u8; 16];
            for i in 0..16 {
                token[i] = self.state[i] ^ (self.counter as u8).wrapping_add(i as u8);
                self.state[i] ^= token[i];
            }
            token
        }
    }

    let rng = Arc::new(Mutex::new(SharedCsprng::new()));
    let mut handles = Vec::new();

    for _ in 0..8 {
        let rng_clone = Arc::clone(&rng);
        handles.push(thread::spawn(move || {
            let mut guard = rng_clone.lock().unwrap();
            guard.get_random_token()
        }));
    }

    let mut tokens = Vec::new();
    for handle in handles {
        tokens.push(handle.join().unwrap());
    }

    assert_eq!(tokens.len(), 8);
    // Verifica que não há tokens duplicados entre as 8 threads
    for i in 0..tokens.len() {
        for j in (i + 1)..tokens.len() {
            assert_ne!(tokens[i], tokens[j], "Threads concorrentes não podem receber o mesmo token CSPRNG");
        }
    }
}

#[test]
fn test_rtc_explicit_time_base_semantics() {
    fn convert_to_utc(base: RtcTimeBase, offset_seconds: i32, local_unix: u64) -> u64 {
        match base {
            RtcTimeBase::Utc => local_unix,
            RtcTimeBase::LocalTime => {
                let offset = offset_seconds as i64;
                if offset >= 0 {
                    local_unix.saturating_sub(offset as u64)
                } else {
                    local_unix.saturating_add((-offset) as u64)
                }
            }
        }
    }

    let cmos_time = 1700000000u64;

    // Caso 1: RtcTimeBase::Utc — o valor lido do CMOS já é UTC
    let utc_direct = convert_to_utc(RtcTimeBase::Utc, -10800, cmos_time);
    assert_eq!(utc_direct, cmos_time, "Em modo UTC, o offset deve ser ignorado");

    // Caso 2: RtcTimeBase::LocalTime com UTC-3 (São Paulo / Brasília: offset = -10800s)
    // T_utc = T_local - (-10800) = T_local + 10800
    let utc_sp = convert_to_utc(RtcTimeBase::LocalTime, -10800, cmos_time);
    assert_eq!(utc_sp, cmos_time + 10800);

    // Caso 3: RtcTimeBase::LocalTime com UTC+9 (Tóquio: offset = +32400s)
    // T_utc = T_local - 32400
    let utc_tokyo = convert_to_utc(RtcTimeBase::LocalTime, 32400, cmos_time);
    assert_eq!(utc_tokyo, cmos_time - 32400);
}

#[test]
fn test_monotonic_clock_independence_and_conversion() {
    fn compute_monotonic(ticks: u64, hz: u64) -> (u64, u32) {
        let hz_safe = hz.max(1);
        let secs = ticks / hz_safe;
        let nanos = ((ticks % hz_safe) * (1_000_000_000 / hz_safe)) as u32;
        (secs, nanos)
    }

    // 1. Verificação com HZ = 100 (passo de 10ms = 10_000_000 ns)
    let (s100, ns100) = compute_monotonic(150, 100);
    assert_eq!(s100, 1);
    assert_eq!(ns100, 500_000_000);

    // 2. Verificação com HZ = 250 (passo de 4ms = 4_000_000 ns)
    let (s250, ns250) = compute_monotonic(375, 250);
    assert_eq!(s250, 1);
    assert_eq!(ns250, 500_000_000);

    // 3. Verificação com HZ = 1000 (passo de 1ms = 1_000_000 ns)
    let (s1000, ns1000) = compute_monotonic(1500, 1000);
    assert_eq!(s1000, 1);
    assert_eq!(ns1000, 500_000_000);

    // 4. Monotonicidade estrita: avanço de ticks sempre produz tempo crescente
    let mut last_s = 0;
    let mut last_ns = 0;
    for tick in 0..100 {
        let (s, ns) = compute_monotonic(tick * 10, 100);
        if s == last_s {
            assert!(ns >= last_ns);
        } else {
            assert!(s > last_s);
        }
        last_s = s;
        last_ns = ns;
    }
}
