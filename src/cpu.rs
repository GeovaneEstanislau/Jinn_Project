#![allow(dead_code)]

/// CPU hardware inspection and feature detection for x86_64 architecture.
///
/// Uses CPUID and control registers (CR0, CR4, RFLAGS, EFER) to probe:
/// - CPU Vendor string (Intel, AMD, QEMU TCG/KVM, etc.)
/// - CPU Brand/Model name string
/// - CPU Family, Model, Stepping
/// - Supported instruction set features (SSE, AVX, APIC, TSC, NX, etc.)
/// - Architectural control registers

#[derive(Debug, Clone, Copy)]
pub struct CpuInfo {
    pub vendor: [u8; 13],
    pub brand: [u8; 49],
    pub family: u8,
    pub model: u8,
    pub stepping: u8,
    pub max_basic_leaf: u32,
    pub max_extended_leaf: u32,
    pub logical_cores: u8,
    pub initial_apic_id: u8,

    // Features
    pub has_fpu: bool,
    pub has_tsc: bool,
    pub has_inv_tsc: bool,
    pub has_msr: bool,
    pub has_pae: bool,
    pub has_apic: bool,
    pub has_x2apic: bool,
    pub has_sse: bool,
    pub has_sse2: bool,
    pub has_sse3: bool,
    pub has_ssse3: bool,
    pub has_sse4_1: bool,
    pub has_sse4_2: bool,
    pub has_avx: bool,
    pub has_avx2: bool,
    pub has_fma: bool,
    pub has_aes: bool,
    pub has_rdrand: bool,
    pub has_rdseed: bool,
    pub has_hypervisor: bool,
    pub has_nx: bool,
    pub has_1gb_pages: bool,
    pub has_smep: bool,
    pub has_smap: bool,
    pub has_fsgsbase: bool,

    // Hardware Registers
    pub cr0: u64,
    pub cr4: u64,
    pub rflags: u64,
    pub efer: u64,
}

impl CpuInfo {
    pub fn probe() -> Self {
        let mut info = CpuInfo {
            vendor: [0u8; 13],
            brand: [0u8; 49],
            family: 0,
            model: 0,
            stepping: 0,
            max_basic_leaf: 0,
            max_extended_leaf: 0,
            logical_cores: 1,
            initial_apic_id: 0,

            has_fpu: false,
            has_tsc: false,
            has_inv_tsc: false,
            has_msr: false,
            has_pae: false,
            has_apic: false,
            has_x2apic: false,
            has_sse: false,
            has_sse2: false,
            has_sse3: false,
            has_ssse3: false,
            has_sse4_1: false,
            has_sse4_2: false,
            has_avx: false,
            has_avx2: false,
            has_fma: false,
            has_aes: false,
            has_rdrand: false,
            has_rdseed: false,
            has_hypervisor: false,
            has_nx: false,
            has_1gb_pages: false,
            has_smep: false,
            has_smap: false,
            has_fsgsbase: false,

            cr0: 0,
            cr4: 0,
            rflags: 0,
            efer: 0,
        };

        // 1. Basic CPUID Leaf 0: Vendor String & Max basic leaf
        let (max_basic, ebx, ecx, edx) = unsafe { cpuid(0) };
        info.max_basic_leaf = max_basic;

        let vendor_bytes = [
            (ebx & 0xff) as u8,
            ((ebx >> 8) & 0xff) as u8,
            ((ebx >> 16) & 0xff) as u8,
            ((ebx >> 24) & 0xff) as u8,
            (edx & 0xff) as u8,
            ((edx >> 8) & 0xff) as u8,
            ((edx >> 16) & 0xff) as u8,
            ((edx >> 24) & 0xff) as u8,
            (ecx & 0xff) as u8,
            ((ecx >> 8) & 0xff) as u8,
            ((ecx >> 16) & 0xff) as u8,
            ((ecx >> 24) & 0xff) as u8,
        ];
        info.vendor[..12].copy_from_slice(&vendor_bytes);

        // 2. CPUID Leaf 1: Features & Processor Info
        if max_basic >= 1 {
            let (eax, ebx, ecx, edx) = unsafe { cpuid(1) };
            info.stepping = (eax & 0x0f) as u8;
            let base_model = ((eax >> 4) & 0x0f) as u8;
            let base_family = ((eax >> 8) & 0x0f) as u8;
            let ext_model = ((eax >> 16) & 0x0f) as u8;
            let ext_family = ((eax >> 20) & 0xff) as u8;

            if base_family == 6 || base_family == 15 {
                info.model = (ext_model << 4) | base_model;
            } else {
                info.model = base_model;
            }

            if base_family == 15 {
                info.family = base_family + ext_family;
            } else {
                info.family = base_family;
            }

            info.initial_apic_id = ((ebx >> 24) & 0xff) as u8;
            let logical = ((ebx >> 16) & 0xff) as u8;
            if logical > 0 {
                info.logical_cores = logical;
            }

            // EDX feature flags
            info.has_fpu = (edx & (1 << 0)) != 0;
            info.has_tsc = (edx & (1 << 4)) != 0;
            info.has_msr = (edx & (1 << 5)) != 0;
            info.has_pae = (edx & (1 << 6)) != 0;
            info.has_apic = (edx & (1 << 9)) != 0;
            info.has_sse = (edx & (1 << 25)) != 0;
            info.has_sse2 = (edx & (1 << 26)) != 0;

            // ECX feature flags
            info.has_sse3 = (ecx & (1 << 0)) != 0;
            info.has_ssse3 = (ecx & (1 << 9)) != 0;
            info.has_fma = (ecx & (1 << 12)) != 0;
            info.has_sse4_1 = (ecx & (1 << 19)) != 0;
            info.has_sse4_2 = (ecx & (1 << 20)) != 0;
            info.has_x2apic = (ecx & (1 << 21)) != 0;
            info.has_aes = (ecx & (1 << 25)) != 0;
            info.has_avx = (ecx & (1 << 28)) != 0;
            info.has_rdrand = (ecx & (1 << 30)) != 0;
            info.has_hypervisor = (ecx & (1 << 31)) != 0;
        }

        // 3. CPUID Leaf 7: Structured Extended Features
        if max_basic >= 7 {
            let (_, ebx, _, _) = unsafe { cpuid_count(7, 0) };
            info.has_fsgsbase = (ebx & (1 << 0)) != 0;
            info.has_avx2 = (ebx & (1 << 5)) != 0;
            info.has_smep = (ebx & (1 << 7)) != 0;
            info.has_rdseed = (ebx & (1 << 18)) != 0;
            info.has_smap = (ebx & (1 << 20)) != 0;
        }

        // 4. Extended Leaves (0x80000000+)
        let (max_ext, _, _, _) = unsafe { cpuid(0x80000000) };
        info.max_extended_leaf = max_ext;

        if max_ext >= 0x80000001 {
            let (_, _, _, edx) = unsafe { cpuid(0x80000001) };
            info.has_nx = (edx & (1 << 20)) != 0;
            info.has_1gb_pages = (edx & (1 << 26)) != 0;
        }

        if max_ext >= 0x80000004 {
            let mut brand_idx = 0;
            for leaf in 0x80000002..=0x80000004 {
                let (eax, ebx, ecx, edx) = unsafe { cpuid(leaf) };
                let regs = [eax, ebx, ecx, edx];
                for reg in regs {
                    info.brand[brand_idx] = (reg & 0xff) as u8;
                    info.brand[brand_idx + 1] = ((reg >> 8) & 0xff) as u8;
                    info.brand[brand_idx + 2] = ((reg >> 16) & 0xff) as u8;
                    info.brand[brand_idx + 3] = ((reg >> 24) & 0xff) as u8;
                    brand_idx += 4;
                }
            }
        }

        if max_ext >= 0x80000007 {
            let (_, _, _, edx) = unsafe { cpuid(0x80000007) };
            info.has_inv_tsc = (edx & (1 << 8)) != 0;
        }

        // 5. Control Registers
        unsafe {
            info.cr0 = read_cr0();
            info.cr4 = read_cr4();
            info.rflags = read_rflags();
            info.efer = read_efer();
        }

        info
    }

    pub fn vendor_str(&self) -> &str {
        let len = self.vendor.iter().position(|&b| b == 0).unwrap_or(12);
        core::str::from_utf8(&self.vendor[..len]).unwrap_or("Unknown")
    }

    pub fn brand_str(&self) -> &str {
        // Strip leading whitespace if present
        let mut start = 0;
        while start < 48 && self.brand[start] == b' ' {
            start += 1;
        }
        let end = self.brand[start..].iter().position(|&b| b == 0).map(|p| start + p).unwrap_or(48);
        if start < end {
            core::str::from_utf8(&self.brand[start..end]).unwrap_or("x86_64 Processor")
        } else {
            "x86_64 Processor"
        }
    }
}

// ── CPUID & Register Assembly Helpers ────────────────────────────────────────

#[inline]
pub unsafe fn cpuid(leaf: u32) -> (u32, u32, u32, u32) {
    let eax: u32;
    let ebx: u32;
    let ecx: u32;
    let edx: u32;
    core::arch::asm!(
        "mov r8, rbx",
        "cpuid",
        "xchg rbx, r8",
        inout("eax") leaf => eax,
        out("r8") ebx,
        inout("ecx") 0u32 => ecx,
        out("edx") edx,
        options(nomem, preserves_flags)
    );
    (eax, ebx, ecx, edx)
}

#[inline]
pub unsafe fn cpuid_count(leaf: u32, sub_leaf: u32) -> (u32, u32, u32, u32) {
    let eax: u32;
    let ebx: u32;
    let ecx: u32;
    let edx: u32;
    core::arch::asm!(
        "mov r8, rbx",
        "cpuid",
        "xchg rbx, r8",
        inout("eax") leaf => eax,
        out("r8") ebx,
        inout("ecx") sub_leaf => ecx,
        out("edx") edx,
        options(nomem, preserves_flags)
    );
    (eax, ebx, ecx, edx)
}

#[inline]
pub unsafe fn read_cr0() -> u64 {
    let val: u64;
    core::arch::asm!("mov {}, cr0", out(reg) val, options(nomem, nostack, preserves_flags));
    val
}

#[inline]
pub unsafe fn read_cr4() -> u64 {
    let val: u64;
    core::arch::asm!("mov {}, cr4", out(reg) val, options(nomem, nostack, preserves_flags));
    val
}

#[inline]
pub unsafe fn read_rflags() -> u64 {
    let val: u64;
    core::arch::asm!("pushfq; pop {}", out(reg) val, options(nomem, preserves_flags));
    val
}

#[inline]
pub unsafe fn read_efer() -> u64 {
    let lo: u32;
    let hi: u32;
    core::arch::asm!("rdmsr", in("ecx") 0xC0000080u32, out("eax") lo, out("edx") hi, options(nomem, nostack, preserves_flags));
    ((hi as u64) << 32) | (lo as u64)
}
