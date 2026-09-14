use core::arch::asm;
use crate::limine::hhdm_offset;

const IA32_APIC_BASE_MSR: u32 = 0x1B;
const APIC_BASE_MASK: u64 = 0xFFFF_FFFF_FFFF_F000;
const APIC_GLOBAL_ENABLE: u64 = 1 << 11;

// APIC Registers (Offsets from APIC Base)
const APIC_ID: u32 = 0x020;
const APIC_EOI: u32 = 0x0B0;
const APIC_SPURIOUS: u32 = 0x0F0;
const APIC_TIMER: u32 = 0x320;
const APIC_TIMER_INIT_CNT: u32 = 0x380;
const APIC_TIMER_CUR_CNT: u32 = 0x390;
const APIC_TIMER_DIV: u32 = 0x3E0;

static mut APIC_BASE_VADDR: u64 = 0;

unsafe fn rdmsr(msr: u32) -> u64 {
    let mut low: u32;
    let mut high: u32;
    asm!("rdmsr", out("eax") low, out("edx") high, in("ecx") msr, options(nomem, nostack));
    ((high as u64) << 32) | (low as u64)
}

unsafe fn wrmsr(msr: u32, val: u64) {
    let low = val as u32;
    let high = (val >> 32) as u32;
    asm!("wrmsr", in("eax") low, in("edx") high, in("ecx") msr, options(nomem, nostack));
}

unsafe fn read_apic(offset: u32) -> u32 {
    let ptr = (APIC_BASE_VADDR + offset as u64) as *const u32;
    core::ptr::read_volatile(ptr)
}

unsafe fn write_apic(offset: u32, val: u32) {
    let ptr = (APIC_BASE_VADDR + offset as u64) as *mut u32;
    core::ptr::write_volatile(ptr, val);
}

pub fn init() {
    unsafe {
        // Read IA32_APIC_BASE
        let mut apic_base = rdmsr(IA32_APIC_BASE_MSR);
        
        // Ensure APIC is globally enabled
        apic_base |= APIC_GLOBAL_ENABLE;
        wrmsr(IA32_APIC_BASE_MSR, apic_base);

        // Get physical address and convert to virtual address via HHDM
        let phys_base = apic_base & APIC_BASE_MASK;
        APIC_BASE_VADDR = phys_base + hhdm_offset();

        // Enable APIC by setting bit 8 in the Spurious Interrupt Vector Register.
        // We map the spurious interrupt to vector 255.
        write_apic(APIC_SPURIOUS, 0x100 | 255);
    }
}

pub fn init_timer(vector: u8, freq_hz: u32) {
    unsafe {
        // Divider = 16
        write_apic(APIC_TIMER_DIV, 0x3);

        // Calculate ticks (Assuming a generic APIC bus frequency of ~1GHz or 100MHz for emulators, 
        // we use a rough approximation or calibrate via PIT. For now, we hardcode an approximation).
        // A common APIC timer frequency in VirtualBox is 1GHz. 
        // Divider 16 -> 62.5MHz. For 100Hz -> 625,000 ticks.
        let initial_count = 625_000; 

        // Configure timer in Periodic Mode (Bit 17) and set vector
        write_apic(APIC_TIMER, (1 << 17) | vector as u32);
        
        // Start timer
        write_apic(APIC_TIMER_INIT_CNT, initial_count);
    }
}

pub fn send_eoi() {
    unsafe {
        write_apic(APIC_EOI, 0);
    }
}
