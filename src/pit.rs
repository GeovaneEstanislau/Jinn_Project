// Simple PIT (programmable interval timer) driver
const PIT_CHANNEL0: u16 = 0x40;
const PIT_COMMAND: u16 = 0x43;
const PIT_FREQ: u32 = 1193182;

unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val);
}

pub fn init_hz(hz: u32) {
    let divisor = (PIT_FREQ / hz) as u16;
    unsafe {
        // mode 2, lobyte/hibyte
        outb(PIT_COMMAND, 0x34);
        outb(PIT_CHANNEL0, (divisor & 0xff) as u8);
        outb(PIT_CHANNEL0, (divisor >> 8) as u8);
    }
}
