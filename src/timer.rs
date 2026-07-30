use core::sync::atomic::{AtomicU64, Ordering};

static TICK_COUNT: AtomicU64 = AtomicU64::new(0);

pub fn init() {
    TICK_COUNT.store(0, Ordering::SeqCst);
}

pub fn tick() {
    TICK_COUNT.fetch_add(1, Ordering::SeqCst);
}

pub fn ticks() -> u64 {
    TICK_COUNT.load(Ordering::SeqCst)
}
