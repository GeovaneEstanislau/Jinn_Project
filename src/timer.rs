use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub const HZ: u64 = 100;

static TICK_COUNT: AtomicU64 = AtomicU64::new(0);
static PIT_IRQ0_COUNT: AtomicU64 = AtomicU64::new(0);
static TICKLESS_ACTIVE: AtomicBool = AtomicBool::new(false);
static BASE_TICKS: AtomicU64 = AtomicU64::new(0);
static BASE_TSC: AtomicU64 = AtomicU64::new(0);
static TSC_PER_10MS: AtomicU64 = AtomicU64::new(15_000_000);

#[inline]
unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack));
}

pub fn init() {
    TICK_COUNT.store(0, Ordering::SeqCst);
    PIT_IRQ0_COUNT.store(0, Ordering::SeqCst);
    TICKLESS_ACTIVE.store(false, Ordering::SeqCst);

    // Configura o PIT (Programmable Interval Timer) em 100 Hz (usado no boot/calibração antes do LAPIC tickless)
    let hz = 100;
    let divisor = 1193182 / hz;
    unsafe {
        outb(0x43, 0x36);
        outb(0x40, (divisor & 0xFF) as u8);
        outb(0x40, ((divisor >> 8) & 0xFF) as u8);
    }
}

/// Transiciona o relógio monotônico de 10 ms para leitura diferencial contínua via TSC.
pub fn enable_tickless_tsc_clock(tsc_per_10ms: u64, now_tsc: u64) {
    let current = TICK_COUNT.load(Ordering::Acquire);
    BASE_TICKS.store(current, Ordering::Release);
    BASE_TSC.store(now_tsc, Ordering::Release);
    TSC_PER_10MS.store(tsc_per_10ms.max(100_000), Ordering::Release);
    TICKLESS_ACTIVE.store(true, Ordering::Release);
}

#[inline]
pub fn is_tickless_active() -> bool {
    TICKLESS_ACTIVE.load(Ordering::Acquire)
}

pub fn tick() {
    PIT_IRQ0_COUNT.fetch_add(1, Ordering::Relaxed);
    if !TICKLESS_ACTIVE.load(Ordering::Acquire) {
        TICK_COUNT.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn pit_irq0_count() -> u64 {
    PIT_IRQ0_COUNT.load(Ordering::Relaxed)
}

/// Retorna ticks monotônicos de 10 ms sem depender de interrupções periódicas durante `HLT`.
pub fn ticks() -> u64 {
    if TICKLESS_ACTIVE.load(Ordering::Acquire) {
        let per_tick = TSC_PER_10MS.load(Ordering::Relaxed).max(1);
        let base_t = BASE_TICKS.load(Ordering::Relaxed);
        let base_c = BASE_TSC.load(Ordering::Relaxed);
        let elapsed = crate::scheduler::read_tsc().saturating_sub(base_c);
        base_t.saturating_add(elapsed / per_tick)
    } else {
        TICK_COUNT.load(Ordering::Relaxed)
    }
}

/// Retorna `(segundos, nanossegundos)` monotônicos contínuos com granularidade sub-tick via TSC.
pub fn monotonic_secs_nanos() -> (u64, u32) {
    if TICKLESS_ACTIVE.load(Ordering::Acquire) {
        let per_tick = TSC_PER_10MS.load(Ordering::Relaxed).max(1);
        let base_t = BASE_TICKS.load(Ordering::Relaxed);
        let base_c = BASE_TSC.load(Ordering::Relaxed);
        let elapsed = crate::scheduler::read_tsc().saturating_sub(base_c);
        let full_ticks = elapsed / per_tick;
        let rem_cycles = elapsed % per_tick;
        let total_ticks = base_t.saturating_add(full_ticks);
        let rem_ns = ((rem_cycles as u128 * 10_000_000u128) / (per_tick as u128)) as u128;
        let total_ns = (total_ticks as u128 * 10_000_000u128).saturating_add(rem_ns);
        ((total_ns / 1_000_000_000u128) as u64, (total_ns % 1_000_000_000u128) as u32)
    } else {
        let t = TICK_COUNT.load(Ordering::Relaxed);
        let secs = t / HZ;
        let nanos = ((t % HZ) * (1_000_000_000 / HZ)) as u32;
        (secs, nanos)
    }
}
