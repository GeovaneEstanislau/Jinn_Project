use core::sync::atomic::{AtomicU64, Ordering};

static TICK_COUNT: AtomicU64 = AtomicU64::new(0);

#[inline]
unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack));
}

pub fn init() {
    TICK_COUNT.store(0, Ordering::SeqCst);
    
    // Configura o PIT (Programmable Interval Timer) para disparar IRQ0
    let hz = 100;
    let divisor = 1193182 / hz;
    unsafe {
        outb(0x43, 0x36);
        outb(0x40, (divisor & 0xFF) as u8);
        outb(0x40, ((divisor >> 8) & 0xFF) as u8);
    }
}

pub fn tick() {
    TICK_COUNT.fetch_add(1, Ordering::Relaxed);
}

pub fn ticks() -> u64 {
    TICK_COUNT.load(Ordering::Relaxed)
}
