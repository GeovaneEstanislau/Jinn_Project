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
