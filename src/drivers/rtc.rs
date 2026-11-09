//! Jinn Microkernel — Driver RTC (Real-Time Clock / CMOS)
//!
//! Fornece leitura de data/hora UTC e geração de timestamp UNIX (segundos desde 1970-01-01)
//! através do chip CMOS (portas I/O 0x70 e 0x71) com interpolação de alta precisão
//! baseada no timer PIT do microkernel, suporte a fuso horário configurável e tempo monotônico.

use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};

const CMOS_ADDR: u16 = 0x70;
const CMOS_DATA: u16 = 0x71;

// ── Identificadores POSIX de Relógio (Syscall 70) ─────────────────────────────
pub const CLOCK_REALTIME: u32 = 0;
pub const CLOCK_MONOTONIC: u32 = 1;
pub const CLOCK_PROCESS_CPUTIME_ID: u32 = 2;
pub const CLOCK_THREAD_CPUTIME_ID: u32 = 3;

static BOOT_UNIX_SECONDS: AtomicU64 = AtomicU64::new(0);
static BOOT_TICKS: AtomicU64 = AtomicU64::new(0);
static TIMEZONE_OFFSET_SECONDS: AtomicI32 = AtomicI32::new(0);
static RTC_IS_LOCAL_TIME: AtomicBool = AtomicBool::new(false);
static RTC_IS_CALIBRATED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtcDateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    core::arch::asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack));
    value
}

#[inline]
unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack));
}

unsafe fn cmos_read(reg: u8) -> u8 {
    // Preserva máscara de interrupção NMI se necessário
    outb(CMOS_ADDR, reg);
    inb(CMOS_DATA)
}

fn is_update_in_progress() -> bool {
    unsafe { (cmos_read(0x0A) & 0x80) != 0 }
}

fn bcd_to_bin(val: u8) -> u8 {
    (val & 0x0F) + ((val / 16) * 10)
}

fn is_leap_year(year: u64) -> bool {
    (year % 4 == 0) && (year % 100 != 0 || year % 400 == 0)
}

const DAYS_IN_MONTH: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

/// Valida se os campos de data e hora lidos do CMOS são cronologicamente consistentes
pub fn is_valid_datetime(dt: &RtcDateTime) -> bool {
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

pub fn datetime_to_unix(dt: &RtcDateTime) -> u64 {
    let year = dt.year as u64;
    if year < 1970 {
        return 0;
    }

    let mut days: u64 = 0;
    // Dias dos anos completos entre 1970 e year - 1
    for y in 1970..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }

    // Dias dos meses completos no ano atual
    let m = dt.month.max(1).min(12) as usize;
    for (idx, &d) in DAYS_IN_MONTH.iter().enumerate().take(m - 1) {
        if idx == 1 && is_leap_year(year) {
            days += 29;
        } else {
            days += d;
        }
    }

    // Dia do mês (1-based)
    days += (dt.day.max(1) as u64) - 1;

    let seconds = days * 86400
        + (dt.hour as u64) * 3600
        + (dt.minute as u64) * 60
        + (dt.second as u64);

    seconds
}

/// Lê a data e hora do CMOS RTC, evitando condições de corrida de UIP
pub fn read_cmos_raw() -> RtcDateTime {
    // Aguarda conclusão de eventual atualização em progresso
    let mut spins = 0;
    while is_update_in_progress() && spins < 10000 {
        spins += 1;
    }

    let mut s1;
    let mut m1;
    let mut h1;
    let mut d1;
    let mut mo1;
    let mut y1;
    let mut c1;
    let mut reg_b;

    // Loop de consistência: lê duas vezes até que todos os registradores coincidam
    loop {
        s1 = unsafe { cmos_read(0x00) };
        m1 = unsafe { cmos_read(0x02) };
        h1 = unsafe { cmos_read(0x04) };
        d1 = unsafe { cmos_read(0x07) };
        mo1 = unsafe { cmos_read(0x08) };
        y1 = unsafe { cmos_read(0x09) };
        c1 = unsafe { cmos_read(0x32) };
        reg_b = unsafe { cmos_read(0x0B) };

        if !is_update_in_progress() {
            let s2 = unsafe { cmos_read(0x00) };
            let m2 = unsafe { cmos_read(0x02) };
            let h2 = unsafe { cmos_read(0x04) };
            let d2 = unsafe { cmos_read(0x07) };
            let mo2 = unsafe { cmos_read(0x08) };
            let y2 = unsafe { cmos_read(0x09) };
            let c2 = unsafe { cmos_read(0x32) };
            if s1 == s2 && m1 == m2 && h1 == h2 && d1 == d2 && mo1 == mo2 && y1 == y2 && c1 == c2 {
                break;
            }
        }
    }

    let is_binary = (reg_b & 0x04) != 0;
    let is_24h = (reg_b & 0x02) != 0;

    let second = if is_binary { s1 } else { bcd_to_bin(s1) };
    let minute = if is_binary { m1 } else { bcd_to_bin(m1) };
    let mut hour = if is_binary { h1 } else { bcd_to_bin(h1 & 0x7F) };
    if !is_24h && (h1 & 0x80) != 0 {
        hour = (hour + 12) % 24;
    }
    let day = if is_binary { d1 } else { bcd_to_bin(d1) };
    let month = if is_binary { mo1 } else { bcd_to_bin(mo1) };
    let year_val = if is_binary { y1 } else { bcd_to_bin(y1) };
    let century_val = if is_binary { c1 } else { bcd_to_bin(c1) };

    let full_year = if century_val >= 19 && century_val <= 21 {
        (century_val as u16) * 100 + (year_val as u16)
    } else if year_val < 70 {
        2000 + (year_val as u16)
    } else {
        1900 + (year_val as u16)
    };

    RtcDateTime {
        year: full_year,
        month,
        day,
        hour,
        minute,
        second,
    }
}

/// Configura o deslocamento de fuso horário e a base do relógio CMOS (UTC vs Hora Local)
pub fn set_timezone(offset_seconds: i32, is_local_time: bool) {
    TIMEZONE_OFFSET_SECONDS.store(offset_seconds, Ordering::SeqCst);
    RTC_IS_LOCAL_TIME.store(is_local_time, Ordering::SeqCst);
}

/// Retorna a configuração de fuso horário atual (offset em segundos, flag de hora local)
pub fn get_timezone() -> (i32, bool) {
    (
        TIMEZONE_OFFSET_SECONDS.load(Ordering::Relaxed),
        RTC_IS_LOCAL_TIME.load(Ordering::Relaxed),
    )
}

/// Informa se o relógio RTC foi lido com sucesso e calibrado
pub fn is_calibrated() -> bool {
    RTC_IS_CALIBRATED.load(Ordering::Relaxed)
}

/// Inicializa o driver RTC capturando a linha de base de tempo e verificando consistência
pub fn init() {
    let dt = read_cmos_raw();
    if is_valid_datetime(&dt) {
        let unix = datetime_to_unix(&dt);
        let current_ticks = crate::timer::ticks();

        BOOT_UNIX_SECONDS.store(unix, Ordering::SeqCst);
        BOOT_TICKS.store(current_ticks, Ordering::SeqCst);
        RTC_IS_CALIBRATED.store(true, Ordering::SeqCst);
    } else {
        // Alerta de falha do RTC CMOS / bateria esgotada: define linha de base mínima segura
        BOOT_UNIX_SECONDS.store(1704067200, Ordering::SeqCst); // 2024-01-01 00:00:00 UTC
        BOOT_TICKS.store(crate::timer::ticks(), Ordering::SeqCst);
        RTC_IS_CALIBRATED.store(false, Ordering::SeqCst);
    }
}

/// Retorna o tempo UNIX atual UTC (segundos, nanossegundos) com ajuste de fuso e interpolação PIT
pub fn current_unix_time() -> (u64, u32) {
    let raw_boot_unix = BOOT_UNIX_SECONDS.load(Ordering::Relaxed);
    let boot_ticks = BOOT_TICKS.load(Ordering::Relaxed);
    let now_ticks = crate::timer::ticks();

    let hz = crate::timer::HZ.max(1);
    let elapsed_ticks = now_ticks.saturating_sub(boot_ticks);
    let elapsed_secs = elapsed_ticks / hz;
    let elapsed_nanos = ((elapsed_ticks % hz) * (1_000_000_000 / hz)) as u32;

    // Se o CMOS opera em hora local, converte para UTC descontando o offset
    let boot_unix = if RTC_IS_LOCAL_TIME.load(Ordering::Relaxed) {
        let offset = TIMEZONE_OFFSET_SECONDS.load(Ordering::Relaxed) as i64;
        if offset >= 0 {
            raw_boot_unix.saturating_sub(offset as u64)
        } else {
            raw_boot_unix.saturating_add((-offset) as u64)
        }
    } else {
        raw_boot_unix
    };

    (boot_unix.saturating_add(elapsed_secs), elapsed_nanos)
}

/// Retorna o tempo monotônico estritamente crescente desde o boot do microkernel (segundos, nanos)
pub fn monotonic_time() -> (u64, u32) {
    let now_ticks = crate::timer::ticks();
    let hz = crate::timer::HZ.max(1);
    let secs = now_ticks / hz;
    let nanos = ((now_ticks % hz) * (1_000_000_000 / hz)) as u32;
    (secs, nanos)
}

/// Retorna timestamp UNIX atual em segundos
pub fn current_unix_timestamp() -> u64 {
    current_unix_time().0
}
