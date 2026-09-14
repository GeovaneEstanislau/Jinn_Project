/// IDT — Interrupt Descriptor Table
///
/// Registers all 48 interrupt handlers (exceptions 0–31 + IRQs 32–47).
/// Uses the stubs defined in stubs.s, which funnel into `irq_dispatch`
/// and `exception_dispatch` in interrupts/mod.rs.

#[allow(dead_code)]

#[derive(Copy, Clone)]
#[repr(C, packed)]
struct IdtEntry {
    offset_low:  u16,
    selector:    u16,
    ist:         u8,
    type_attr:   u8,
    offset_mid:  u16,
    offset_high: u32,
    zero:        u32,
}

impl IdtEntry {
    const fn missing() -> IdtEntry {
        IdtEntry {
            offset_low: 0, selector: 0, ist: 0, type_attr: 0,
            offset_mid: 0, offset_high: 0, zero: 0,
        }
    }

    fn set(&mut self, handler: usize, selector: u16, type_attr: u8) {
        self.offset_low  = handler as u16;
        self.selector    = selector;
        self.ist         = 0;
        self.type_attr   = type_attr;
        self.offset_mid  = ((handler >> 16) & 0xffff) as u16;
        self.offset_high = ((handler >> 32) & 0xffffffff) as u32;
        self.zero        = 0;
    }
}

#[repr(C, packed)]
struct IdtDescriptor {
    limit: u16,
    base:  u64,
}

static mut IDT: [IdtEntry; 256] = [IdtEntry::missing(); 256];

// Include all stub symbols from stubs.s
core::arch::global_asm!(include_str!("stubs.s"));

// ── Exception stubs ───────────────────────────────────────────────────────────
extern "C" {
    fn exc0_stub();  fn exc1_stub();  fn exc2_stub();  fn exc3_stub();
    fn exc4_stub();  fn exc5_stub();  fn exc6_stub();  fn exc7_stub();
    fn exc8_stub();  fn exc9_stub();  fn exc10_stub(); fn exc11_stub();
    fn exc12_stub(); fn exc13_stub(); fn exc14_stub(); fn exc15_stub();
    fn exc16_stub(); fn exc17_stub(); fn exc18_stub(); fn exc19_stub();
    fn exc20_stub(); fn exc21_stub(); fn exc22_stub(); fn exc23_stub();
    fn exc24_stub(); fn exc25_stub(); fn exc26_stub(); fn exc27_stub();
    fn exc28_stub(); fn exc29_stub(); fn exc30_stub(); fn exc31_stub();
}

// ── IRQ stubs (vectors 32–47) ─────────────────────────────────────────────────
extern "C" {
    fn irq32_stub(); fn irq33_stub(); fn irq34_stub(); fn irq35_stub();
    fn irq36_stub(); fn irq37_stub(); fn irq38_stub(); fn irq39_stub();
    fn irq40_stub(); fn irq41_stub(); fn irq42_stub(); fn irq43_stub();
    fn irq44_stub(); fn irq45_stub(); fn irq46_stub(); fn irq47_stub();
    fn isr128_stub();
}

/// Install all stubs into the IDT and load it with `lidt`.
pub fn init() {
    let exc_stubs: [usize; 32] = [
        exc0_stub  as usize, exc1_stub  as usize, exc2_stub  as usize, exc3_stub  as usize,
        exc4_stub  as usize, exc5_stub  as usize, exc6_stub  as usize, exc7_stub  as usize,
        exc8_stub  as usize, exc9_stub  as usize, exc10_stub as usize, exc11_stub as usize,
        exc12_stub as usize, exc13_stub as usize, exc14_stub as usize, exc15_stub as usize,
        exc16_stub as usize, exc17_stub as usize, exc18_stub as usize, exc19_stub as usize,
        exc20_stub as usize, exc21_stub as usize, exc22_stub as usize, exc23_stub as usize,
        exc24_stub as usize, exc25_stub as usize, exc26_stub as usize, exc27_stub as usize,
        exc28_stub as usize, exc29_stub as usize, exc30_stub as usize, exc31_stub as usize,
    ];

    let irq_stubs: [usize; 16] = [
        irq32_stub as usize, irq33_stub as usize, irq34_stub as usize, irq35_stub as usize,
        irq36_stub as usize, irq37_stub as usize, irq38_stub as usize, irq39_stub as usize,
        irq40_stub as usize, irq41_stub as usize, irq42_stub as usize, irq43_stub as usize,
        irq44_stub as usize, irq45_stub as usize, irq46_stub as usize, irq47_stub as usize,
    ];

    let mut cs_sel: u16;
    unsafe {
        core::arch::asm!("mov {:x}, cs", out(reg) cs_sel, options(nomem, nostack));
    }
    let gate: u8 = 0x8E;

    unsafe {
        let idt_ptr = core::ptr::addr_of_mut!(IDT);

        for (i, &addr) in exc_stubs.iter().enumerate() {
            (*idt_ptr)[i].set(addr, cs_sel, gate);
        }

        for (i, &addr) in irq_stubs.iter().enumerate() {
            (*idt_ptr)[32 + i].set(addr, cs_sel, gate);
        }

        // ── Syscall handler ───────────────────────────────────────────────────────
        (*idt_ptr)[128].set(isr128_stub as usize, cs_sel, gate | 0x60); // 0x60 = DPL 3 (userspace)

        let idt_desc = IdtDescriptor {
            limit: (core::mem::size_of_val(&IDT) - 1) as u16,
            base:  idt_ptr as u64,
        };
        core::arch::asm!("lidt [{}]", in(reg) &idt_desc, options(readonly, nostack, preserves_flags));
    }
}
