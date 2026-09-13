/// PIC — 8259A Programmable Interrupt Controller
///
/// Remaps the two PICs so that:
///   IRQ 0–7  → CPU vectors 32–39
///   IRQ 8–15 → CPU vectors 40–47
///
/// Also provides per-IRQ masking so the kernel can enable only the
/// hardware lines it actually has drivers for.

#[allow(dead_code)]

// ── Port addresses ────────────────────────────────────────────────────────────
pub const PIC1_CMD:  u16 = 0x20;
pub const PIC1_DATA: u16 = 0x21;
pub const PIC2_CMD:  u16 = 0xA0;
pub const PIC2_DATA: u16 = 0xA1;

// ── EOI command ───────────────────────────────────────────────────────────────
const PIC_EOI: u8 = 0x20;

// ── I/O helpers ───────────────────────────────────────────────────────────────
#[inline]
unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack));
}

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    core::arch::asm!("in al, dx", out("al") val, in("dx") port, options(nomem, nostack));
    val
}

/// Brief I/O delay (one outb to port 0x80, a "dummy" POST port).
#[inline]
unsafe fn io_wait() {
    outb(0x80, 0);
}

/// Remap both PICs and **mask every IRQ**.
/// Call `unmask_irq` afterwards for each IRQ your driver needs.
pub fn remap() {
    unsafe {
        // Save existing masks
        let m1 = inb(PIC1_DATA);
        let m2 = inb(PIC2_DATA);

        // ICW1: start initialisation, ICW4 needed
        outb(PIC1_CMD,  0x11); io_wait();
        outb(PIC2_CMD,  0x11); io_wait();

        // ICW2: vector offsets
        outb(PIC1_DATA, 0x20); io_wait();  // IRQ0 → INT 32
        outb(PIC2_DATA, 0x28); io_wait();  // IRQ8 → INT 40

        // ICW3: cascading wiring
        outb(PIC1_DATA, 0x04); io_wait();  // PIC1 has slave on IRQ2
        outb(PIC2_DATA, 0x02); io_wait();  // PIC2 cascade identity = 2

        // ICW4: 8086 mode
        outb(PIC1_DATA, 0x01); io_wait();
        outb(PIC2_DATA, 0x01); io_wait();

        // MASK ALL IRQs (Disable legacy PIC entirely)
        outb(PIC1_DATA, 0xFF);
        outb(PIC2_DATA, 0xFF);
    }
}

pub fn disable() {
    remap();
}

/// Enable a specific IRQ line (0–15).
pub fn unmask_irq(irq: u8) {
    unsafe {
        if irq < 8 {
            let mask = inb(PIC1_DATA) & !(1 << irq);
            outb(PIC1_DATA, mask);
        } else {
            let mask = inb(PIC2_DATA) & !(1 << (irq - 8));
            outb(PIC2_DATA, mask);
            // Also unmask IRQ2 on PIC1 (the cascade line)
            let m1 = inb(PIC1_DATA) & !(1 << 2);
            outb(PIC1_DATA, m1);
        }
    }
}

/// Disable a specific IRQ line (0–15).
pub fn mask_irq(irq: u8) {
    unsafe {
        if irq < 8 {
            let mask = inb(PIC1_DATA) | (1 << irq);
            outb(PIC1_DATA, mask);
        } else {
            let mask = inb(PIC2_DATA) | (1 << (irq - 8));
            outb(PIC2_DATA, mask);
        }
    }
}

/// Send an End-Of-Interrupt signal.
/// Must be called at the end of every IRQ handler.
pub fn send_eoi(irq: u8) {
    unsafe {
        if irq >= 8 {
            outb(PIC2_CMD, PIC_EOI);
        }
        outb(PIC1_CMD, PIC_EOI);
    }
}
