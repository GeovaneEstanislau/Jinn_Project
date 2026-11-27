use core::arch::asm;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use crate::limine::hhdm_offset;

pub const LAPIC_TIMER_VECTOR: u8 = 48;
pub const RESCHEDULE_IPI_VECTOR: u8 = 49;
const MAX_CPUS: usize = 8;
const MAX_LAPIC_ONESHOT_CNT: u32 = 0x7FFF_FFFF; // Guarda segura anti-rollover 32-bit

const IA32_APIC_BASE_MSR: u32 = 0x1B;
const APIC_BASE_MASK: u64 = 0xFFFF_FFFF_FFFF_F000;
const APIC_GLOBAL_ENABLE: u64 = 1 << 11;

// APIC Registers (Offsets from APIC Base)
#[allow(dead_code)]
const APIC_ID: u32 = 0x020;
const APIC_TPR: u32 = 0x080;
const APIC_EOI: u32 = 0x0B0;
const APIC_SPURIOUS: u32 = 0x0F0;
const APIC_LVT_LINT0: u32 = 0x350;
const APIC_LVT_LINT1: u32 = 0x360;
const APIC_TIMER: u32 = 0x320;
const APIC_TIMER_INIT_CNT: u32 = 0x380;
const APIC_TIMER_CUR_CNT: u32 = 0x390;
const APIC_TIMER_DIV: u32 = 0x3E0;

const LVT_MASKED: u32 = 1 << 16;
const LVT_MODE_ONESHOT: u32 = 0b00 << 17;
const LVT_MODE_PERIODIC: u32 = 0b01 << 17;

#[repr(C, align(64))]
pub struct LapicCalibrationData {
    pub lapic_ticks_per_ms: u64,
    pub tsc_ticks_per_ms: u64,
    pub tsc_ticks_per_10ms: u64,
    pub divisor: u32,
    pub calibrated: bool,
    _pad: [u8; 32],
}

static mut LAPIC_CAL: LapicCalibrationData = LapicCalibrationData {
    lapic_ticks_per_ms: 62_500,
    tsc_ticks_per_ms: 1_500_000,
    tsc_ticks_per_10ms: 15_000_000,
    divisor: 16,
    calibrated: false,
    _pad: [0u8; 32],
};

static TICKLESS_ENABLED: AtomicBool = AtomicBool::new(false);
static TOTAL_LAPIC_TIMER_IRQS: AtomicU64 = AtomicU64::new(0);
static TOTAL_ONESHOT_ARMS: AtomicU64 = AtomicU64::new(0);
static CORE_LAPIC_TIMER_IRQS: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];

static mut APIC_BASE_VADDR: u64 = 0;

#[inline(always)]
unsafe fn outb(port: u16, val: u8) {
    asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack));
}

#[inline(always)]
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    asm!("in al, dx", out("al") val, in("dx") port, options(nomem, nostack));
    val
}

#[inline(always)]
unsafe fn read_tsc_raw() -> u64 {
    let lo: u32;
    let hi: u32;
    asm!("rdtsc", out("eax") lo, out("edx") hi, options(nomem, nostack));
    ((hi as u64) << 32) | (lo as u64)
}

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

#[allow(dead_code)]
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

        // Aceita todas as prioridades de interrupção (TPR = 0)
        write_apic(APIC_TPR, 0);

        // Configura LVT LINT0 como ExtINT (0x700), sem máscara, para entregar interrupções do PIC 8259A ao BSP
        write_apic(APIC_LVT_LINT0, 0x0000_0700);

        // Configura LVT LINT1 como NMI (0x400)
        write_apic(APIC_LVT_LINT1, 0x0000_0400);

        // Enable APIC by setting bit 8 in the Spurious Interrupt Vector Register.
        // We map the spurious interrupt to vector 255.
        write_apic(APIC_SPURIOUS, 0x100 | 255);
    }
}

/// Calibra determinísticamente a frequência do Local APIC Timer (divisor 16) e do TSC no BSP usando PIT Canal 2 (20 ms).
pub fn calibrate_timer_with_pit() {
    unsafe {
        if APIC_BASE_VADDR == 0 {
            return;
        }
        // Garante PIT Canal 2 desligado do alto-falante antes de carregar modo 0 (one-shot terminal count)
        let orig_61 = inb(0x61);
        outb(0x61, (orig_61 & !0x03) | 0x00);

        // 1_193_182 Hz / 50 = 23_864 ticks (~20.000 ms exatos)
        let pit_count: u16 = 23_864;
        outb(0x43, 0xB0); // Ch2, lobyte/hibyte, mode 0
        outb(0x42, (pit_count & 0xFF) as u8);
        outb(0x42, (pit_count >> 8) as u8);

        write_apic(APIC_TIMER_DIV, 0x3); // Divisor 16
        write_apic(APIC_TIMER, LVT_MASKED | (LAPIC_TIMER_VECTOR as u32));

        let tsc0 = read_tsc_raw();
        write_apic(APIC_TIMER_INIT_CNT, u32::MAX);
        outb(0x61, (orig_61 & !0x02) | 0x01); // Gate Ch2 HIGH

        let mut spins = 0u32;
        while (inb(0x61) & 0x20) == 0 && spins < 80_000_000 {
            core::hint::spin_loop();
            spins += 1;
        }

        let cur_cnt = read_apic(APIC_TIMER_CUR_CNT);
        let tsc1 = read_tsc_raw();

        // Desarma APIC e restaura gate do PIT Canal 2
        write_apic(APIC_TIMER_INIT_CNT, 0);
        outb(0x61, orig_61 & !0x03);

        let lapic_elapsed = (u32::MAX.saturating_sub(cur_cnt)) as u64;
        let tsc_elapsed = tsc1.saturating_sub(tsc0);

        let cal_ptr = core::ptr::addr_of_mut!(LAPIC_CAL);
        if spins < 80_000_000 && lapic_elapsed >= 1_000 {
            (*cal_ptr).lapic_ticks_per_ms = ((lapic_elapsed + 10) / 20).clamp(1_000, 500_000_000);
            (*cal_ptr).tsc_ticks_per_ms = ((tsc_elapsed + 10) / 20).clamp(10_000, 10_000_000_000);
            (*cal_ptr).tsc_ticks_per_10ms = ((*cal_ptr).tsc_ticks_per_ms * 10).max(100_000);
            (*cal_ptr).calibrated = true;
        } else {
            (*cal_ptr).lapic_ticks_per_ms = 62_500;
            (*cal_ptr).tsc_ticks_per_ms = 1_500_000;
            (*cal_ptr).tsc_ticks_per_10ms = 15_000_000;
            (*cal_ptr).calibrated = false;
        }
    }
}

pub fn init_timer(vector: u8, _freq_hz: u32) {
    unsafe {
        if APIC_BASE_VADDR == 0 {
            return;
        }
        write_apic(APIC_TIMER_DIV, 0x3);
        let cal = &*core::ptr::addr_of!(LAPIC_CAL);
        let initial_count = ((cal.lapic_ticks_per_ms * 10) as u64).clamp(1, MAX_LAPIC_ONESHOT_CNT as u64) as u32;
        if TICKLESS_ENABLED.load(Ordering::Acquire) {
            write_apic(APIC_TIMER, LVT_MODE_ONESHOT | (vector as u32));
        } else {
            write_apic(APIC_TIMER, LVT_MODE_PERIODIC | (vector as u32));
        }
        write_apic(APIC_TIMER_INIT_CNT, initial_count);
    }
}

/// Mascara o Local APIC Timer do núcleo atual (zero ticks periódicos quando ocioso em `HLT`).
pub fn mask_timer() {
    unsafe {
        if APIC_BASE_VADDR != 0 {
            write_apic(APIC_TIMER, LVT_MASKED | LVT_MODE_ONESHOT | (LAPIC_TIMER_VECTOR as u32));
            write_apic(APIC_TIMER_INIT_CNT, 0);
        }
    }
}

/// Programa o Local APIC Timer em modo *One-Shot* (`0b00 << 17`) para o deadline informado (ou mascara se `None`).
pub fn arm_lapic_oneshot_deadline(vector: u8, delay_us: Option<u64>) {
    unsafe {
        if APIC_BASE_VADDR == 0 {
            return;
        }
        match delay_us {
            Some(us) => {
                let cal = &*core::ptr::addr_of!(LAPIC_CAL);
                let raw_ticks = ((us as u128 * cal.lapic_ticks_per_ms as u128) + 999) / 1000;
                let cnt = raw_ticks.clamp(64, MAX_LAPIC_ONESHOT_CNT as u128) as u32;
                write_apic(APIC_TIMER_DIV, 0x3);
                write_apic(APIC_TIMER, LVT_MODE_ONESHOT | (vector as u32));
                write_apic(APIC_TIMER_INIT_CNT, cnt);
                TOTAL_ONESHOT_ARMS.fetch_add(1, Ordering::Relaxed);
            }
            None => {
                mask_timer();
            }
        }
    }
}

/// Reativa o Local APIC Timer (mantido para compatibilidade; em modo tickless arma One-Shot de 10 ms).
pub fn unmask_timer(vector: u8, initial_count: u32) {
    unsafe {
        if APIC_BASE_VADDR != 0 {
            write_apic(APIC_TIMER_DIV, 0x3);
            if TICKLESS_ENABLED.load(Ordering::Acquire) {
                let cal = &*core::ptr::addr_of!(LAPIC_CAL);
                let cnt = ((cal.lapic_ticks_per_ms * 10) as u64).clamp(64, MAX_LAPIC_ONESHOT_CNT as u64) as u32;
                write_apic(APIC_TIMER, LVT_MODE_ONESHOT | (vector as u32));
                write_apic(APIC_TIMER_INIT_CNT, cnt);
                TOTAL_ONESHOT_ARMS.fetch_add(1, Ordering::Relaxed);
            } else {
                write_apic(APIC_TIMER, LVT_MODE_PERIODIC | (vector as u32));
                write_apic(APIC_TIMER_INIT_CNT, initial_count);
            }
        }
    }
}

pub fn set_tickless_enabled(enabled: bool) {
    TICKLESS_ENABLED.store(enabled, Ordering::Release);
}

pub fn is_tickless_enabled() -> bool {
    TICKLESS_ENABLED.load(Ordering::Acquire)
}

pub fn calibration_stats() -> (u64, u64, bool) {
    unsafe {
        let cal = &*core::ptr::addr_of!(LAPIC_CAL);
        (cal.lapic_ticks_per_ms, cal.tsc_ticks_per_ms, cal.calibrated)
    }
}

pub fn tsc_ticks_per_10ms() -> u64 {
    unsafe { (*core::ptr::addr_of!(LAPIC_CAL)).tsc_ticks_per_10ms }
}

#[inline]
pub fn record_lapic_timer_irq(cpu_id: usize) {
    TOTAL_LAPIC_TIMER_IRQS.fetch_add(1, Ordering::Relaxed);
    if cpu_id < MAX_CPUS {
        CORE_LAPIC_TIMER_IRQS[cpu_id].fetch_add(1, Ordering::Relaxed);
    }
}

pub fn timer_irq_totals() -> (u64, u64) {
    (
        TOTAL_LAPIC_TIMER_IRQS.load(Ordering::Relaxed),
        TOTAL_ONESHOT_ARMS.load(Ordering::Relaxed),
    )
}

pub fn core_timer_irqs(cpu_id: usize) -> u64 {
    if cpu_id < MAX_CPUS {
        CORE_LAPIC_TIMER_IRQS[cpu_id].load(Ordering::Relaxed)
    } else {
        0
    }
}

const APIC_ICR_LOW: u32 = 0x300;
const APIC_ICR_HIGH: u32 = 0x310;

pub fn send_eoi() {
    unsafe {
        if APIC_BASE_VADDR != 0 {
            write_apic(APIC_EOI, 0);
        }
    }
}

/// Retorna o Local APIC ID do núcleo que está executando a chamada.
pub fn lapic_id() -> u32 {
    unsafe {
        if APIC_BASE_VADDR != 0 {
            (read_apic(APIC_ID) >> 24) & 0xFF
        } else {
            0
        }
    }
}

/// Inicializa o subsistema Local APIC para um núcleo secundário (AP).
pub fn init_ap() {
    unsafe {
        let mut apic_base = rdmsr(IA32_APIC_BASE_MSR);
        apic_base |= APIC_GLOBAL_ENABLE;
        wrmsr(IA32_APIC_BASE_MSR, apic_base);

        let phys_base = apic_base & APIC_BASE_MASK;
        if APIC_BASE_VADDR == 0 {
            APIC_BASE_VADDR = phys_base + hhdm_offset();
        }

        // Spurious interrupt vector 255 com APIC habilitado (bit 8)
        write_apic(APIC_SPURIOUS, 0x100 | 255);
    }
}

/// Envia um Inter-Processor Interrupt (IPI) direcionado a um APIC ID específico.
pub fn send_ipi(dest_lapic_id: u32, vector: u8) {
    unsafe {
        if APIC_BASE_VADDR == 0 { return; }
        // Escreve o destino nos bits 24..31 do ICR High
        write_apic(APIC_ICR_HIGH, (dest_lapic_id & 0xFF) << 24);
        // Escreve o vetor e modo Fixed (0) no ICR Low (dispara o IPI)
        write_apic(APIC_ICR_LOW, vector as u32);
    }
}

/// Envia um IPI para todos os outros núcleos (Broadcast All Excluding Self).
pub fn send_all_ipi(vector: u8) {
    unsafe {
        if APIC_BASE_VADDR == 0 { return; }
        // Shorthand: bits 18..19 = 11b (All Excluding Self)
        write_apic(APIC_ICR_LOW, (vector as u32) | (3 << 18));
    }
}

