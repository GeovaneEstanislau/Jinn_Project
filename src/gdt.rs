/// Jinn OS — Global Descriptor Table (GDT) e Task State Segment (TSS)
///
/// Em 64-bits (Long Mode), a GDT é majoritariamente ignorada para proteção de memória
/// (feita pela Paginação), mas ainda é estritamente necessária para:
///
/// 1. Transição de privilégios (Ring 0 <-> Ring 3)
/// 2. Carregar o TSS (Task State Segment)
///
/// O TSS no Long Mode não salva mais registradores (como fazia em 32-bits).
/// Sua única função real é dizer à CPU qual é o endereço da Pilha do Kernel (RSP0)
/// quando ocorrer uma interrupção (hardware ou syscall) vinda do Ring 3 (Userspace).

// ── Função de flush do CS (via global_asm para evitar restrições de labels) ───────
//
// O Rust proíbe labels em `asm!` inline. O far return (retfq) precisa de
// um label para poder saltar de volta. A solução é declarar a função em
// `global_asm!` e chamá-la normalmente em Rust.
//
// Convenção de chamada:
//   Empurrar na pilha: [cs_selector, endereço_de_retorno]
//   Chamar gdt_flush_cs via CALL normal.
//   A função troca CS e retorna para o caller.
core::arch::global_asm!(
    ".global gdt_flush_cs",
    "gdt_flush_cs:",
    // Na pilha (de cima para baixo): [retorno_normal, cs_selector]
    // Precisamos fazer: push cs_selector / push rip_apos_retfq / retfq
    // Pegamos o endereço de retorno normal primeiro:
    "pop rax",           // rax = RIP de retorno (endereço após CALL gdt_flush_cs)
    "pop rcx",           // rcx = novo CS selector (2º argumento na pilha)
    "push rcx",          // empurra CS para retfq
    "push rax",          // empurra RIP para retfq
    "retfq",             // far return: atualiza CS e pula para RIP
);

extern "C" {
    /// Flush do registrador CS via far return. Ver `global_asm!` acima.
    fn gdt_flush_cs();
}

// ── Estrutura do descritor de 8 bytes da GDT ──────────────────────────────────

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct GdtEntry(u64);

impl GdtEntry {
    /// Cria um descritor de segmento para Long Mode.
    pub const fn new(limit: u32, base: u32, access: u8, flags: u8) -> Self {
        let mut e = 0u64;
        e |= (limit as u64 & 0xFFFF);
        e |= (base as u64 & 0x00FF_FFFF) << 16;
        e |= (access as u64) << 40;
        e |= ((limit as u64 >> 16) & 0xF) << 48;
        e |= (flags as u64 & 0xF) << 52;
        e |= ((base as u64 >> 24) & 0xFF) << 56;
        Self(e)
    }

    pub const fn null() -> Self { Self(0) }
}

/// Descritor GDTR — passado para `lgdt`.
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
struct GdtDescriptor {
    limit: u16,
    base: u64,
}

// ── Task State Segment (TSS) ──────────────────────────────────────────────────

/// No Long Mode o TSS tem 104 bytes e serve quase exclusivamente para
/// fornecer RSP0 (pilha do kernel usada em transições Ring 3 → Ring 0).
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct TaskStateSegment {
    _reserved1: u32,
    /// RSP0: Pilha utilizada quando ocorre transição Ring 3 -> Ring 0.
    pub rsp0: u64,
    pub rsp1: u64,
    pub rsp2: u64,
    _reserved2: u64,
    /// IST: Interrupt Stack Table (pilhas para Double Fault, NMI, etc.)
    pub ist: [u64; 7],
    _reserved3: u64,
    _reserved4: u16,
    /// IOPB desativado: aponta além do TSS, bloqueando I/O em userspace.
    pub iopb_offset: u16,
}

impl TaskStateSegment {
    pub const fn new() -> Self {
        Self {
            _reserved1: 0,
            rsp0: 0,
            rsp1: 0,
            rsp2: 0,
            _reserved2: 0,
            ist: [0; 7],
            _reserved3: 0,
            _reserved4: 0,
            iopb_offset: core::mem::size_of::<TaskStateSegment>() as u16,
        }
    }
}

/// Descritor de TSS — ocupa 16 bytes na GDT (dois slots).
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
struct TssDescriptor {
    low:  GdtEntry,
    high: u64,
}

impl TssDescriptor {
    pub fn from_tss(tss: *const TaskStateSegment) -> Self {
        let base  = tss as u64;
        let limit = (core::mem::size_of::<TaskStateSegment>() - 1) as u32;
        // Access: Present(1) | DPL=0(00) | Type=TSS64 Available(01001) → 0x89
        let low = GdtEntry::new(limit, base as u32, 0x89, 0x00);
        let high = base >> 32;
        Self { low, high }
    }
}

// ── Seletores de segmento públicos ────────────────────────────────────────────

/// Seletor do segmento de Código do Kernel (Ring 0, DPL=0).
pub const KERNEL_CODE_SEL: u16 = 1 << 3;          // 0x08
/// Seletor do segmento de Dados do Kernel (Ring 0, DPL=0).
pub const KERNEL_DATA_SEL: u16 = 2 << 3;          // 0x10
/// Seletor do segmento de Dados do Usuário (Ring 3, DPL=3).
pub const USER_DATA_SEL:   u16 = (3 << 3) | 3;    // 0x1B
/// Seletor do segmento de Código do Usuário (Ring 3, DPL=3).
pub const USER_CODE_SEL:   u16 = (4 << 3) | 3;    // 0x23
/// Seletor do TSS na GDT.
pub const TSS_SEL:         u16 = 5 << 3;           // 0x28

// ── Tabela GDT ────────────────────────────────────────────────────────────────

/// Layout da GDT completa do Jinn OS.
/// O campo `tss` é preenchido em tempo de execução em `init()`.
#[repr(C, align(16))]
struct Gdt {
    null:        GdtEntry,
    kernel_code: GdtEntry,
    kernel_data: GdtEntry,
    user_data:   GdtEntry,
    user_code:   GdtEntry,
    tss:         TssDescriptor,
}

// ── Instâncias estáticas ──────────────────────────────────────────────────────

/// TSS global, preenchido em `init()` com a pilha do kernel.
pub static mut TSS: TaskStateSegment = TaskStateSegment::new();

static mut GLOBAL_GDT: Gdt = Gdt {
    null:        GdtEntry::null(),
    // Kernel Code: L=1 (Long Mode 64-bit), P=1, DPL=0, S=1, Type=0xA (Exec+Read)
    kernel_code: GdtEntry::new(0, 0, 0b1001_1010, 0b0010),
    // Kernel Data: P=1, DPL=0, S=1, Type=0x2 (Read/Write)
    kernel_data: GdtEntry::new(0, 0, 0b1001_0010, 0b0000),
    // User Data:  P=1, DPL=3, S=1, Type=0x2 (Read/Write)
    user_data:   GdtEntry::new(0, 0, 0b1111_0010, 0b0000),
    // User Code:  L=1, P=1, DPL=3, S=1, Type=0xA (Exec+Read)
    user_code:   GdtEntry::new(0, 0, 0b1111_1010, 0b0010),
    // TSS: preenchido em init()
    tss: TssDescriptor {
        low:  GdtEntry::null(),
        high: 0,
    },
};

// ── Inicialização ─────────────────────────────────────────────────────────────

/// Instala a GDT e o TSS no processador.
///
/// Deve ser chamado uma vez, logo no início do boot, antes da IDT.
pub fn init() {
    unsafe {
        // 1. Preenche o descritor do TSS com o endereço real do nosso TSS estático.
        //    O Rust 2024 proíbe referências compartilhadas a `static mut`; usamos
        //    ponteiros brutos para evitar UB ao configurar o TSS.
        GLOBAL_GDT.tss = TssDescriptor::from_tss(&raw const TSS);

        // 2. Carrega a GDT via instrução LGDT.
        let gdt_desc = GdtDescriptor {
            limit: (core::mem::size_of::<Gdt>() - 1) as u16,
            base:  core::ptr::addr_of!(GLOBAL_GDT) as u64,
        };
        core::arch::asm!(
            "lgdt [{}]",
            in(reg) &gdt_desc,
            options(readonly, nostack, preserves_flags)
        );

        // 3. Recarrega DS, ES, FS, GS, SS com o seletor de dados do kernel.
        core::arch::asm!(
            "mov ds, {0:x}",
            "mov es, {0:x}",
            "mov fs, {0:x}",
            "mov gs, {0:x}",
            "mov ss, {0:x}",
            in(reg) KERNEL_DATA_SEL,
            options(nostack, preserves_flags)
        );

        // 4. Recarrega CS via far return (gdt_flush_cs faz a mágica).
        //    Empurramos o seletor na pilha antes de chamar; a função
        //    consome o seletor e faz um far return com o CS correto.
        core::arch::asm!(
            "push {0:r}",       // empurra o seletor CS como 2º argumento
            "call gdt_flush_cs",
            in(reg) KERNEL_CODE_SEL as u64,
            options(nostack)
        );

        // 5. Carrega o TSS no registrador de tarefa (LTR).
        core::arch::asm!(
            "ltr ax",
            in("ax") TSS_SEL,
            options(nostack, preserves_flags)
        );
    }
}

/// Atualiza RSP0 no TSS para a pilha do kernel da tarefa atual.
///
/// O escalonador deve chamar esta função a cada context switch para uma tarefa
/// de usuário (Ring 3), garantindo que interrupções de hardware e syscalls
/// encontrem a pilha de kernel correta.
pub fn set_kernel_stack(stack_top: u64) {
    // SAFETY: TSS é o único gerenciador de pilhas por nível de privilégio.
    // Escrevemos via ponteiro bruto para evitar UB com `static mut`.
    unsafe {
        core::ptr::write_volatile(&raw mut TSS.rsp0, stack_top);
    }
}
