/// Jinn OS — Subsistema de Multiprocessamento Simétrico (SMP)
///
/// Gerencia a inicialização, topologia e sincronização de múltiplos núcleos (CPUs)
/// utilizando o protocolo oficial Limine SMP e Local APIC x86_64.

use core::sync::atomic::{AtomicI32, AtomicU64, AtomicUsize, Ordering};
use crate::limine::{self, LimineSmpInfo};
use crate::memory::IrqSafeSpinLock;
use crate::interrupts::apic;
use crate::gdt;

pub const MAX_CPUS: usize = 8;
const AP_STACK_SIZE: usize = 32768; // 32 KiB por núcleo

#[repr(align(16))]
struct ApStack([u8; AP_STACK_SIZE]);

static mut AP_STACKS: [ApStack; MAX_CPUS] = [
    ApStack([0u8; AP_STACK_SIZE]),
    ApStack([0u8; AP_STACK_SIZE]),
    ApStack([0u8; AP_STACK_SIZE]),
    ApStack([0u8; AP_STACK_SIZE]),
    ApStack([0u8; AP_STACK_SIZE]),
    ApStack([0u8; AP_STACK_SIZE]),
    ApStack([0u8; AP_STACK_SIZE]),
    ApStack([0u8; AP_STACK_SIZE]),
];

#[derive(Debug, Clone, Copy)]
pub struct CpuCore {
    pub cpu_id: usize,
    pub lapic_id: u32,
    pub is_bsp: bool,
    pub is_online: bool,
    pub current_pid: Option<usize>,
    pub ticks: u64,
    pub ctx_switches: u64,
}

impl CpuCore {
    pub const fn empty(cpu_id: usize) -> Self {
        Self {
            cpu_id,
            lapic_id: 0,
            is_bsp: false,
            is_online: false,
            current_pid: None,
            ticks: 0,
            ctx_switches: 0,
        }
    }
}

static CPUS: IrqSafeSpinLock<[CpuCore; MAX_CPUS]> = IrqSafeSpinLock::new([
    CpuCore::empty(0),
    CpuCore::empty(1),
    CpuCore::empty(2),
    CpuCore::empty(3),
    CpuCore::empty(4),
    CpuCore::empty(5),
    CpuCore::empty(6),
    CpuCore::empty(7),
]);

// Mapeamento lock-free O(1) de Local APIC ID (0..255) para ID lógico de CPU (0..MAX_CPUS-1)
static LAPIC_TO_CPUID: [AtomicI32; 256] = {
    const EMPTY: AtomicI32 = AtomicI32::new(-1);
    [EMPTY; 256]
};

// Telemetria por núcleo lock-free (imune a deadlocks em ISR e preempção)
static CURRENT_PID_PER_CPU: [AtomicI32; MAX_CPUS] = {
    const EMPTY: AtomicI32 = AtomicI32::new(-1);
    [EMPTY; MAX_CPUS]
};

static CORE_TICKS: [AtomicU64; MAX_CPUS] = {
    const ZERO: AtomicU64 = AtomicU64::new(0);
    [ZERO; MAX_CPUS]
};

static CORE_CTX_SWITCHES: [AtomicU64; MAX_CPUS] = {
    const ZERO: AtomicU64 = AtomicU64::new(0);
    [ZERO; MAX_CPUS]
};

static ONLINE_CPUS: AtomicUsize = AtomicUsize::new(0);

/// Retorna o ID lógico (0..MAX_CPUS-1) do núcleo de CPU em execução.
pub fn current_cpu_id() -> usize {
    let lapic = apic::lapic_id() as usize;
    if lapic < 256 {
        let cid = LAPIC_TO_CPUID[lapic].load(Ordering::Relaxed);
        if cid >= 0 && (cid as usize) < MAX_CPUS {
            return cid as usize;
        }
    }
    0
}

/// Retorna o total de núcleos de CPU atualmente inicializados e operantes.
pub fn online_cpus() -> usize {
    ONLINE_CPUS.load(Ordering::Relaxed)
}

/// Retorna uma cópia do estado do núcleo especificado.
pub fn get_cpu(cpu_id: usize) -> Option<CpuCore> {
    if cpu_id < MAX_CPUS {
        let guard = CPUS.lock();
        let mut core = guard[cpu_id];
        core.current_pid = get_current_pid(cpu_id);
        core.ticks = CORE_TICKS[cpu_id].load(Ordering::Relaxed);
        core.ctx_switches = CORE_CTX_SWITCHES[cpu_id].load(Ordering::Relaxed);
        Some(core)
    } else {
        None
    }
}

/// Define o PID do processo em execução no núcleo informado (Lock-Free).
pub fn set_current_pid(cpu_id: usize, pid: Option<usize>) {
    if cpu_id < MAX_CPUS {
        let val = match pid {
            Some(p) => p as i32,
            None => -1,
        };
        CURRENT_PID_PER_CPU[cpu_id].store(val, Ordering::Release);
        let mut guard = CPUS.lock();
        guard[cpu_id].current_pid = pid;
    }
}

/// Retorna o PID do processo que está executando no núcleo informado (Lock-Free).
pub fn get_current_pid(cpu_id: usize) -> Option<usize> {
    if cpu_id < MAX_CPUS {
        let val = CURRENT_PID_PER_CPU[cpu_id].load(Ordering::Acquire);
        if val >= 0 {
            Some(val as usize)
        } else {
            None
        }
    } else {
        None
    }
}

// ── Contabilização Lock-Free Por Núcleo (Zero-Idle Audit SMP) ────────────────

const ACCT_STATE_TASK: u8 = 0;
const ACCT_STATE_IDLE_LOOP: u8 = 1;
const ACCT_STATE_HLT: u8 = 2;
const ACCT_STATE_IRQ: u8 = 3;

#[repr(align(64))]
pub struct PerCpuAccounting {
    seq: core::sync::atomic::AtomicU64,
    online: core::sync::atomic::AtomicBool,
    on_idle: core::sync::atomic::AtomicBool,
    state: core::sync::atomic::AtomicU8,
    last_tsc: core::sync::atomic::AtomicU64,
    task_cycles: core::sync::atomic::AtomicU64,
    hlt_cycles: core::sync::atomic::AtomicU64,
    irq_cycles: core::sync::atomic::AtomicU64,
    idle_transitions: core::sync::atomic::AtomicU64,
    wakeups: core::sync::atomic::AtomicU64,
}

impl PerCpuAccounting {
    pub const fn new() -> Self {
        Self {
            seq: core::sync::atomic::AtomicU64::new(0),
            online: core::sync::atomic::AtomicBool::new(false),
            on_idle: core::sync::atomic::AtomicBool::new(false),
            state: core::sync::atomic::AtomicU8::new(ACCT_STATE_TASK),
            last_tsc: core::sync::atomic::AtomicU64::new(0),
            task_cycles: core::sync::atomic::AtomicU64::new(0),
            hlt_cycles: core::sync::atomic::AtomicU64::new(0),
            irq_cycles: core::sync::atomic::AtomicU64::new(0),
            idle_transitions: core::sync::atomic::AtomicU64::new(0),
            wakeups: core::sync::atomic::AtomicU64::new(0),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PerCpuCoreSnapshot {
    pub cpu_id: u32,
    pub online: bool,
    pub total_cycles: u64,
    pub hlt_cycles: u64,
    pub task_cycles: u64,
    pub irq_cycles: u64,
    pub idle_transitions: u64,
    pub wakeups: u64,
}

static PER_CPU_ACCT: [PerCpuAccounting; MAX_CPUS] = [
    PerCpuAccounting::new(), PerCpuAccounting::new(), PerCpuAccounting::new(), PerCpuAccounting::new(),
    PerCpuAccounting::new(), PerCpuAccounting::new(), PerCpuAccounting::new(), PerCpuAccounting::new(),
];

static CPUID_TO_LAPIC: [core::sync::atomic::AtomicU32; MAX_CPUS] = [
    core::sync::atomic::AtomicU32::new(0), core::sync::atomic::AtomicU32::new(0),
    core::sync::atomic::AtomicU32::new(0), core::sync::atomic::AtomicU32::new(0),
    core::sync::atomic::AtomicU32::new(0), core::sync::atomic::AtomicU32::new(0),
    core::sync::atomic::AtomicU32::new(0), core::sync::atomic::AtomicU32::new(0),
];

/// Registra o LAPIC ID correspondente a `cpu_id` (Lock-Free).
pub fn set_cpu_lapic_id(cpu_id: usize, lapic_id: u32) {
    if cpu_id < MAX_CPUS {
        CPUID_TO_LAPIC[cpu_id].store(lapic_id, Ordering::Release);
    }
}

/// Retorna o LAPIC ID correspondente a `cpu_id` (Lock-Free).
pub fn cpu_lapic_id(cpu_id: usize) -> Option<u32> {
    if cpu_id < MAX_CPUS {
        Some(CPUID_TO_LAPIC[cpu_id].load(Ordering::Acquire))
    } else {
        None
    }
}

/// Inicializa a contabilização por núcleo com ponto zero consistente no TSC.
pub fn init_cpu_accounting(cpu_id: usize, lapic_id: u32, start_on_idle: bool) {
    if cpu_id >= MAX_CPUS {
        return;
    }
    set_cpu_lapic_id(cpu_id, lapic_id);
    let c = &PER_CPU_ACCT[cpu_id];
    let now = crate::scheduler::read_tsc();
    c.seq.fetch_add(1, Ordering::Release);
    c.on_idle.store(start_on_idle, Ordering::Relaxed);
    c.state.store(if start_on_idle { ACCT_STATE_IDLE_LOOP } else { ACCT_STATE_TASK }, Ordering::Relaxed);
    c.last_tsc.store(now, Ordering::Relaxed);
    c.task_cycles.store(0, Ordering::Relaxed);
    c.hlt_cycles.store(0, Ordering::Relaxed);
    c.irq_cycles.store(0, Ordering::Relaxed);
    c.idle_transitions.store(0, Ordering::Relaxed);
    c.wakeups.store(0, Ordering::Relaxed);
    c.online.store(true, Ordering::Release);
    c.seq.fetch_add(1, Ordering::Release);
}

/// Atualiza o indicador se o núcleo está atualmente despachado em sua thread Idle.
#[inline]
pub fn set_cpu_on_idle(cpu_id: usize, is_idle: bool) {
    if cpu_id < MAX_CPUS {
        PER_CPU_ACCT[cpu_id].on_idle.store(is_idle, Ordering::Relaxed);
    }
}

/// Fecha o intervalo anterior antes de entrar em HLT (chamar SOMENTE com IF=0 imediatamente antes de `sti; hlt`).
#[inline]
pub fn account_enter_hlt(cpu_id: usize) {
    if cpu_id >= MAX_CPUS {
        return;
    }
    let c = &PER_CPU_ACCT[cpu_id];
    if !c.online.load(Ordering::Relaxed) {
        return;
    }
    c.seq.fetch_add(1, Ordering::Release);
    let now = crate::scheduler::read_tsc();
    let last = c.last_tsc.load(Ordering::Relaxed);
    let delta = now.saturating_sub(last);
    match c.state.load(Ordering::Relaxed) {
        ACCT_STATE_HLT => { c.hlt_cycles.fetch_add(delta, Ordering::Relaxed); }
        ACCT_STATE_IDLE_LOOP | ACCT_STATE_IRQ => { c.irq_cycles.fetch_add(delta, Ordering::Relaxed); }
        _ => { c.task_cycles.fetch_add(delta, Ordering::Relaxed); }
    }
    c.last_tsc.store(now, Ordering::Relaxed);
    c.state.store(ACCT_STATE_HLT, Ordering::Relaxed);
    c.idle_transitions.fetch_add(1, Ordering::Relaxed);
    c.seq.fetch_add(1, Ordering::Release);
}

/// Contabiliza a saída do estado anterior na entrada de uma interrupção de hardware (`IF=0`).
#[inline]
pub fn account_exit_on_irq(cpu_id: usize) {
    if cpu_id >= MAX_CPUS {
        return;
    }
    let c = &PER_CPU_ACCT[cpu_id];
    if !c.online.load(Ordering::Relaxed) {
        return;
    }
    c.seq.fetch_add(1, Ordering::Release);
    let now = crate::scheduler::read_tsc();
    let last = c.last_tsc.load(Ordering::Relaxed);
    let delta = now.saturating_sub(last);
    match c.state.load(Ordering::Relaxed) {
        ACCT_STATE_HLT => {
            c.hlt_cycles.fetch_add(delta, Ordering::Relaxed);
            c.wakeups.fetch_add(1, Ordering::Relaxed);
        }
        ACCT_STATE_IDLE_LOOP | ACCT_STATE_IRQ => {
            c.irq_cycles.fetch_add(delta, Ordering::Relaxed);
        }
        _ => {
            c.task_cycles.fetch_add(delta, Ordering::Relaxed);
        }
    }
    c.last_tsc.store(now, Ordering::Relaxed);
    c.state.store(ACCT_STATE_IRQ, Ordering::Relaxed);
    c.seq.fetch_add(1, Ordering::Release);
}

/// Fecha o intervalo de execução de ISR antes de retornar via `iretq` (ou trocar de tarefa).
#[inline]
pub fn account_irq_end(cpu_id: usize) {
    if cpu_id >= MAX_CPUS {
        return;
    }
    let c = &PER_CPU_ACCT[cpu_id];
    if !c.online.load(Ordering::Relaxed) {
        return;
    }
    c.seq.fetch_add(1, Ordering::Release);
    let now = crate::scheduler::read_tsc();
    let last = c.last_tsc.load(Ordering::Relaxed);
    let delta = now.saturating_sub(last);
    c.irq_cycles.fetch_add(delta, Ordering::Relaxed);
    c.last_tsc.store(now, Ordering::Relaxed);
    let next_state = if c.on_idle.load(Ordering::Relaxed) {
        ACCT_STATE_IDLE_LOOP
    } else {
        ACCT_STATE_TASK
    };
    c.state.store(next_state, Ordering::Relaxed);
    c.seq.fetch_add(1, Ordering::Release);
}

/// Captura snapshot consistente e lock-free de um núcleo (incluindo tempo corrente no estado atual).
pub fn snapshot_percpu_core(cpu_id: usize) -> PerCpuCoreSnapshot {
    if cpu_id >= MAX_CPUS {
        return PerCpuCoreSnapshot::default();
    }
    let c = &PER_CPU_ACCT[cpu_id];
    loop {
        let s1 = c.seq.load(Ordering::Acquire);
        if (s1 & 1) != 0 {
            core::hint::spin_loop();
            continue;
        }
        let online = c.online.load(Ordering::Relaxed);
        let mut task = c.task_cycles.load(Ordering::Relaxed);
        let mut hlt = c.hlt_cycles.load(Ordering::Relaxed);
        let mut irq = c.irq_cycles.load(Ordering::Relaxed);
        let last_tsc = c.last_tsc.load(Ordering::Relaxed);
        let state = c.state.load(Ordering::Relaxed);
        let idle_transitions = c.idle_transitions.load(Ordering::Relaxed);
        let wakeups = c.wakeups.load(Ordering::Relaxed);
        let s2 = c.seq.load(Ordering::Acquire);
        if s1 == s2 {
            if online {
                let now = crate::scheduler::read_tsc();
                let pending = now.saturating_sub(last_tsc);
                match state {
                    ACCT_STATE_HLT => hlt = hlt.saturating_add(pending),
                    ACCT_STATE_IDLE_LOOP | ACCT_STATE_IRQ => irq = irq.saturating_add(pending),
                    _ => task = task.saturating_add(pending),
                }
            }
            let total = task.saturating_add(hlt).saturating_add(irq);
            return PerCpuCoreSnapshot {
                cpu_id: cpu_id as u32,
                online,
                total_cycles: total,
                hlt_cycles: hlt,
                task_cycles: task,
                irq_cycles: irq,
                idle_transitions,
                wakeups,
            };
        }
    }
}

/// Agrega (ciclos_ativos = task + irq, ciclos_em_hlt) entre todos os núcleos online.
pub fn aggregated_cycle_stats() -> (u64, u64) {
    let n = online_cpus().clamp(1, MAX_CPUS);
    let mut active = 0u64;
    let mut idle = 0u64;
    for cpu in 0..n {
        let s = snapshot_percpu_core(cpu);
        if s.online {
            active = active.saturating_add(s.task_cycles).saturating_add(s.irq_cycles);
            idle = idle.saturating_add(s.hlt_cycles);
        }
    }
    (active, idle)
}

/// Contabiliza um tick de timer no núcleo informado (Lock-Free, seguro em ISR).
pub fn record_tick(cpu_id: usize) {
    if cpu_id < MAX_CPUS {
        CORE_TICKS[cpu_id].fetch_add(1, Ordering::Relaxed);
    }
}

/// Contabiliza uma troca de contexto (context switch) no núcleo informado (Lock-Free, seguro em ISR).
pub fn record_ctx_switch(cpu_id: usize) {
    if cpu_id < MAX_CPUS {
        CORE_CTX_SWITCHES[cpu_id].fetch_add(1, Ordering::Relaxed);
    }
}

/// Ponto de entrada de 64-bits para núcleos secundários (APs).
/// O Limine salta para este endereço com o ponteiro LimineSmpInfo em RDI.
pub unsafe extern "C" fn ap_entry(info: *const LimineSmpInfo) -> ! {
    let lapic_id = (*info).lapic_id;
    let cpu_id = (*info).extra_argument as usize;

    // 1. Troca a pilha para o stack dedicado de kernel do núcleo
    let stack_top = (AP_STACKS[cpu_id].0.as_ptr() as usize + AP_STACK_SIZE) & !15;
    core::arch::asm!(
        "mov rsp, {0}",
        in(reg) stack_top,
        options(nostack)
    );

    // 2. Configura a GDT e TSS exclusiva deste núcleo (evita GP por TSS busy)
    gdt::init_cpu(cpu_id);

    // 3. Carrega a tabela IDT (compartilhada)
    crate::interrupts::idt::load();

    // 4. Habilita extensões SSE/AVX e NX (No-Execute)
    crate::cpu::enable_sse();
    crate::cpu::enable_nxe();

    // 5. Inicializa o Local APIC do núcleo
    apic::init_ap();

    // 6. Configura o timer Local APIC deste núcleo no vetor 32 para preempção a 100 Hz
    apic::init_timer(32, 100);

    // 7. Registra estado online e inicializa contabilização lock-free por núcleo
    {
        let mut guard = CPUS.lock();
        guard[cpu_id].is_online = true;
        guard[cpu_id].lapic_id = lapic_id;
    }
    init_cpu_accounting(cpu_id, lapic_id, true);
    ONLINE_CPUS.fetch_add(1, Ordering::SeqCst);

    // Mensagem de diagnóstico serial
    crate::serial_write(b"[SMP] AP CPU ");
    crate::serial_write_byte(b'0' + (cpu_id as u8));
    crate::serial_write(b" online (LAPIC ");
    crate::serial_write_byte(b'0' + (lapic_id as u8));
    crate::serial_write(b")\n");

    // 8. Habilita interrupções. O timer disparará no vetor 32 e acionará o escalonador
    core::arch::asm!("sti");

    // 9. Loop de espera (Idle Loop do AP enquanto aguarda ou executa tarefas)
    loop {
        core::arch::asm!("cli", options(nomem, nostack));
        account_enter_hlt(cpu_id);
        core::arch::asm!("sti; hlt");
    }
}

/// Inicializa o subsistema SMP, registra o BSP e acorda todos os APs detectados.
pub fn init() -> usize {
    // 1. Inicializa o Local APIC no BSP (CPU 0) e ativa o timer periódico a 100 Hz (vetor 32)
    // Garante preempção e suporte universal ao escalonador tanto em Uniprocessador (1 core) quanto SMP.
    crate::interrupts::apic::init();
    crate::interrupts::apic::init_timer(32, 100);

    // 2. Inicializa tabela de mapeamento
    for slot in LAPIC_TO_CPUID.iter() {
        slot.store(-1, Ordering::Relaxed);
    }

    // 3. Registra o Bootstrap Processor (BSP = CPU 0)
    let bsp_lapic = apic::lapic_id();
    if (bsp_lapic as usize) < 256 {
        LAPIC_TO_CPUID[bsp_lapic as usize].store(0, Ordering::SeqCst);
    }

    {
        let mut guard = CPUS.lock();
        guard[0].is_bsp = true;
        guard[0].is_online = true;
        guard[0].lapic_id = bsp_lapic;
    }
    init_cpu_accounting(0, bsp_lapic, false);
    ONLINE_CPUS.store(1, Ordering::SeqCst);

    // 3. Lê contagem e descritores do Limine
    let detected_cpus = limine::smp_cpu_count();
    if detected_cpus <= 1 {
        return 1;
    }

    let mut next_ap_id = 1;
    for i in 0..detected_cpus {
        if next_ap_id >= MAX_CPUS {
            break;
        }

        if let Some(info_ptr) = limine::smp_get_cpu(i) {
            unsafe {
                let lapic = (*info_ptr).lapic_id;
                // Não inicializa o BSP novamente
                if lapic == bsp_lapic {
                    continue;
                }

                let ap_id = next_ap_id;
                next_ap_id += 1;

                if (lapic as usize) < 256 {
                    LAPIC_TO_CPUID[lapic as usize].store(ap_id as i32, Ordering::SeqCst);
                }

                {
                    let mut guard = CPUS.lock();
                    guard[ap_id].cpu_id = ap_id;
                    guard[ap_id].lapic_id = lapic;
                    guard[ap_id].is_bsp = false;
                    guard[ap_id].is_online = false;
                }

                // Passa o ID lógico via extra_argument
                core::ptr::write_volatile(&mut (*info_ptr).extra_argument, ap_id as u64);

                // Acorda o AP definindo goto_address
                core::ptr::write_volatile(&mut (*info_ptr).goto_address, ap_entry);
            }
        }
    }

    // 4. Aguarda brevemente a ativação dos APs com timeout
    let target = next_ap_id;
    for _ in 0..100_000 {
        if ONLINE_CPUS.load(Ordering::SeqCst) >= target {
            break;
        }
        core::hint::spin_loop();
    }

    ONLINE_CPUS.load(Ordering::SeqCst)
}
