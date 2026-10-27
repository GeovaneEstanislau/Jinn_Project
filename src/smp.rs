/// Jinn OS — Subsistema de Multiprocessamento Simétrico (SMP)
///
/// Gerencia a inicialização, topologia e sincronização de múltiplos núcleos (CPUs)
/// utilizando o protocolo oficial Limine SMP e Local APIC x86_64.

use core::sync::atomic::{AtomicI32, AtomicUsize, Ordering};
use crate::limine::{self, LimineSmpInfo};
use crate::memory::SpinLock;
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

static CPUS: SpinLock<[CpuCore; MAX_CPUS]> = SpinLock::new([
    CpuCore::empty(0),
    CpuCore::empty(1),
    CpuCore::empty(2),
    CpuCore::empty(3),
    CpuCore::empty(4),
    CpuCore::empty(5),
    CpuCore::empty(6),
    CpuCore::empty(7),
]);

// Mapeamento O(1) de Local APIC ID (0..255) para ID lógico de CPU (0..MAX_CPUS-1)
static LAPIC_TO_CPUID: [AtomicI32; 256] = {
    const EMPTY: AtomicI32 = AtomicI32::new(-1);
    [EMPTY; 256]
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
        Some(guard[cpu_id])
    } else {
        None
    }
}

/// Define o PID do processo em execução no núcleo informado.
pub fn set_current_pid(cpu_id: usize, pid: Option<usize>) {
    if cpu_id < MAX_CPUS {
        let mut guard = CPUS.lock();
        guard[cpu_id].current_pid = pid;
    }
}

/// Retorna o PID do processo que está executando no núcleo informado.
pub fn get_current_pid(cpu_id: usize) -> Option<usize> {
    if cpu_id < MAX_CPUS {
        let guard = CPUS.lock();
        guard[cpu_id].current_pid
    } else {
        None
    }
}

/// Contabiliza um tick de timer no núcleo informado.
pub fn record_tick(cpu_id: usize) {
    if cpu_id < MAX_CPUS {
        let mut guard = CPUS.lock();
        guard[cpu_id].ticks = guard[cpu_id].ticks.wrapping_add(1);
    }
}

/// Contabiliza uma troca de contexto (context switch) no núcleo informado.
pub fn record_ctx_switch(cpu_id: usize) {
    if cpu_id < MAX_CPUS {
        let mut guard = CPUS.lock();
        guard[cpu_id].ctx_switches = guard[cpu_id].ctx_switches.wrapping_add(1);
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

    // 4. Habilita extensões SSE/AVX
    crate::cpu::enable_sse();

    // 5. Inicializa o Local APIC do núcleo
    apic::init_ap();

    // 6. Configura o timer Local APIC deste núcleo no vetor 32 para preempção a 100 Hz
    apic::init_timer(32, 100);

    // 7. Registra estado online
    {
        let mut guard = CPUS.lock();
        guard[cpu_id].is_online = true;
        guard[cpu_id].lapic_id = lapic_id;
    }
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
        core::arch::asm!("hlt");
    }
}

/// Inicializa o subsistema SMP, registra o BSP e acorda todos os APs detectados.
pub fn init() -> usize {
    // 1. Inicializa tabela de mapeamento
    for slot in LAPIC_TO_CPUID.iter() {
        slot.store(-1, Ordering::Relaxed);
    }

    // 2. Registra o Bootstrap Processor (BSP = CPU 0)
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
