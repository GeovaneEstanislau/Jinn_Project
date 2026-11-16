//! Jinn OS — FPU / SIMD Context Management & Safe Assembly Primitives
//!
//! Provides strict 16-byte aligned FXSAVE64/FXRSTOR64 state management,
//! MXCSR sanitization, and deterministic architectural defaults.

#![allow(dead_code)]

/// Estado arquitetural da FPU x87, MMX e SSE (512 bytes alinhados a 16 bytes).
/// Definido conforme Intel SDM Vol. 1, Seção 10.5.1 (Layout do FXSAVE de 64 bits).
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct FpuState {
    /// FPU Control Word (FCW) — offset 0x00
    pub fcw: u16,
    /// FPU Status Word (FSW) — offset 0x02
    pub fsw: u16,
    /// FPU Tag Word (FTW / Abridged) — offset 0x04
    pub ftw: u8,
    /// Reservado — offset 0x05
    pub reserved_1: u8,
    /// FPU Opcode (FOP) — offset 0x06
    pub fop: u16,
    /// FPU Instruction Pointer Offset (64-bit em modo longo) — offset 0x08
    pub fpu_ip: u64,
    /// FPU Data Pointer Offset (64-bit em modo longo) — offset 0x10
    pub fpu_dp: u64,
    /// MXCSR Control and Status Register — offset 0x18
    pub mxcsr: u32,
    /// MXCSR Mask (bits implementados pelo processador) — offset 0x1C
    pub mxcsr_mask: u32,
    /// Registradores ST0..ST7 / MMX0..MMX7 (8 x 16 bytes = 128 bytes) — offset 0x20
    pub st_space: [u8; 128],
    /// Registradores XMM0..XMM15 (16 x 16 bytes = 256 bytes) — offset 0xA0
    pub xmm_space: [u8; 256],
    /// Área reservada para extensões de arquitetura — offset 0x1A0 (96 bytes)
    pub padding: [u8; 96],
}

// Asserção estática em tempo de compilação exigida pelas especificações de hardware:
const _: () = assert!(core::mem::size_of::<FpuState>() == 512 && core::mem::align_of::<FpuState>() == 16);

pub const FCW_DEFAULT: u16 = 0x037F; // Round to nearest, 64-bit extended precision, all exceptions masked
pub const MXCSR_DEFAULT: u32 = 0x1F80; // All SIMD exceptions masked, round to nearest, flags clear
pub const MXCSR_VALID_MASK: u32 = 0x0000_FFFF; // Bits 16..31 are strictly reserved on x86_64

impl FpuState {
    /// Retorna um estado limpo e determinístico para novos processos em Ring 3.
    pub const fn new_user_default() -> Self {
        Self {
            fcw: FCW_DEFAULT,
            fsw: 0,
            ftw: 0,
            reserved_1: 0,
            fop: 0,
            fpu_ip: 0,
            fpu_dp: 0,
            mxcsr: MXCSR_DEFAULT,
            mxcsr_mask: 0,
            st_space: [0u8; 128],
            xmm_space: [0u8; 256],
            padding: [0u8; 96],
        }
    }
}

/// Salva o estado atual da FPU/SSE da CPU física na estrutura em memória.
///
/// # Safety
/// O ponteiro `state` deve ser válido e alinhado a 16 bytes.
#[inline(always)]
pub unsafe fn fxsave64(state: *mut FpuState) {
    debug_assert_eq!(state as usize & 0xF, 0, "FpuState must be 16-byte aligned for fxsave64");
    core::arch::asm!(
        "fxsave64 [{0}]",
        in(reg) state,
        options(nostack, preserves_flags)
    );
}

/// Restaura o estado da FPU/SSE a partir da estrutura em memória para a CPU física.
/// Executa sanitização obrigatória de `MXCSR` antes da instrução para prevenir `#GP(0)` em Ring 0.
///
/// # Safety
/// O ponteiro `state` deve ser válido e alinhado a 16 bytes.
#[inline(always)]
pub unsafe fn fxrstor64(state: *const FpuState) {
    debug_assert_eq!(state as usize & 0xF, 0, "FpuState must be 16-byte aligned for fxrstor64");

    // Sanitização de MXCSR: bits 16..31 são estritamente reservados.
    // Qualquer bit reservado carregado via fxrstor dispara General Protection Fault (#GP).
    let raw_mxcsr = (*state).mxcsr;
    let mut safe_mxcsr = raw_mxcsr & MXCSR_VALID_MASK;
    if safe_mxcsr == 0 {
        safe_mxcsr = MXCSR_DEFAULT;
    }
    if safe_mxcsr != raw_mxcsr {
        (*(state as *mut FpuState)).mxcsr = safe_mxcsr;
    }

    core::arch::asm!(
        "fxrstor64 [{0}]",
        in(reg) state,
        options(nostack, preserves_flags)
    );
}
