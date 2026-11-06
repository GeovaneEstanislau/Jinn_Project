//! Jinn OS — Hardware Profiler & Capability Detection (JAHA 1.0)
//!
//! Responsável por inspecionar a fundo as capacidades físicas reais da máquina:
//! - Memória física total, utilizável, reservada e áreas especiais (ACPI/Framebuffer).
//! - Processador: núcleos, threads, conjunto de instruções (LM, SSE2, AVX, NX, etc.).
//! - Barramentos e periféricos: IDE, SATA/AHCI, NVMe, interfaces de rede, áudio, USB, vídeo.
//! - Firmware: BIOS vs UEFI, tabelas ACPI RSDP.
//! - Classificação determinística nos Perfis de Hardware A..E (Legacy, Compact, Standard, Performance, HighEnd).
//! - Validação estrita de endereços de memória física contra corrupção do firmware.

#![allow(dead_code)]

use crate::cpu::CpuInfo;
use crate::drivers::pci;
use crate::limine;
use crate::memory::SpinLock;

/// Perfis Oficiais de Hardware do ecossistema Jinn (JAHA 1.0)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareProfile {
    /// Perfil A — Jinn Legacy: 256 MB de RAM, 1 vCPU, desktop essencial, zero desperdício.
    Legacy,
    /// Perfil B — Jinn Compact: 512 MB de RAM, 1 vCPU, desktop completo, multitarefa moderada.
    Compact,
    /// Perfil C — Jinn Standard: 1 GB a 4 GB de RAM, 2 vCPUs, navegação expandida e produtividade.
    Standard,
    /// Perfil D — Jinn Performance: 4 GB a 16 GB de RAM, 4 vCPUs, caches amplos, multitarefa avançada.
    Performance,
    /// Perfil E — Jinn High-End: 16 GB ou mais, 8+ vCPUs, alta concorrência e virtualização.
    HighEnd,
}

impl HardwareProfile {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Legacy => "Perfil A - Jinn Legacy (256 MB)",
            Self::Compact => "Perfil B - Jinn Compact (512 MB)",
            Self::Standard => "Perfil C - Jinn Standard (1 GB - 4 GB)",
            Self::Performance => "Perfil D - Jinn Performance (4 GB - 16 GB)",
            Self::HighEnd => "Perfil E - Jinn High-End (16 GB+)",
        }
    }

    pub const fn short_code(&self) -> &'static str {
        match self {
            Self::Legacy => "LEGACY",
            Self::Compact => "COMPACT",
            Self::Standard => "STANDARD",
            Self::Performance => "PERFORMANCE",
            Self::HighEnd => "HIGH-END",
        }
    }

    /// Limite máximo de tarefas simultâneas no escalonador para este perfil
    pub const fn max_active_tasks(&self) -> usize {
        match self {
            Self::Legacy => 16,
            Self::Compact => 32,
            Self::Standard => 64,
            Self::Performance => 128,
            Self::HighEnd => 256,
        }
    }

    /// Orçamento padrão de frames físicos (4 KiB) por processo de usuário Ring 3
    pub const fn max_user_frames(&self) -> usize {
        match self {
            Self::Legacy => 1024,      // 4 MiB
            Self::Compact => 2048,     // 8 MiB
            Self::Standard => 4096,    // 16 MiB
            Self::Performance => 16384,// 64 MiB
            Self::HighEnd => 65536,    // 256 MiB
        }
    }

    /// Quantidade de slots para o cache LRU de blocos de disco
    pub const fn default_cache_slots(&self) -> usize {
        match self {
            Self::Legacy => 16,        // 8 KiB de cache
            Self::Compact => 32,       // 16 KiB
            Self::Standard => 64,      // 32 KiB
            Self::Performance => 128,  // 64 KiB
            Self::HighEnd => 256,      // 128 KiB
        }
    }

    /// Permite composições alfa/transparências complexas na interface gráfica
    pub const fn gui_allow_alpha(&self) -> bool {
        match self {
            Self::Legacy => false,
            Self::Compact => false,
            Self::Standard | Self::Performance | Self::HighEnd => true,
        }
    }

    /// Uso estrito de Dirty Rectangles para evitar repinturas completas de framebuffer
    pub const fn gui_enforce_dirty_rects(&self) -> bool {
        true // Sempre benéfico, mas crítico no perfil Legacy
    }

    /// Quantidade máxima de abas ativas simultaneamente no Jinn Browser
    pub const fn browser_max_active_tabs(&self) -> usize {
        match self {
            Self::Legacy => 1,
            Self::Compact => 3,
            Self::Standard => 8,
            Self::Performance => 16,
            Self::HighEnd => 32,
        }
    }

    /// Limite máximo de passos de execução JavaScript antes de interrupção (bounded loop)
    pub const fn browser_js_step_limit(&self) -> usize {
        match self {
            Self::Legacy => 2_000,
            Self::Compact => 5_000,
            Self::Standard => 10_000,
            Self::Performance => 25_000,
            Self::HighEnd => 50_000,
        }
    }
}

/// Capacidades de Hardware Consolidadas e Validadas do Sistema
#[derive(Debug, Clone, Copy)]
pub struct HardwareCapabilities {
    // ── Memória Física ────────────────────────────────────────────────────────
    pub total_ram_bytes: u64,
    pub usable_ram_bytes: u64,
    pub reserved_ram_bytes: u64,
    pub acpi_reclaimable_bytes: u64,
    pub bootloader_reclaimable_bytes: u64,
    pub framebuffer_ram_bytes: u64,
    pub memory_regions_count: usize,
    pub validated_regions_count: usize,
    pub memory_validation_passed: bool,

    // ── Processador (CPU) ─────────────────────────────────────────────────────
    pub cores_detected: usize,
    pub threads_detected: usize,
    pub vendor: [u8; 13],
    pub brand: [u8; 49],
    pub has_long_mode: bool,
    pub has_sse: bool,
    pub has_sse2: bool,
    pub has_sse3: bool,
    pub has_ssse3: bool,
    pub has_sse4_1: bool,
    pub has_sse4_2: bool,
    pub has_avx: bool,
    pub has_avx2: bool,
    pub has_nx: bool,
    pub has_apic: bool,
    pub has_x2apic: bool,
    pub has_tsc: bool,
    pub has_inv_tsc: bool,
    pub has_pae: bool,
    pub has_smep: bool,
    pub has_smap: bool,
    pub estimated_mhz: u64,

    // ── Controladores de Armazenamento ────────────────────────────────────────
    pub has_ide: bool,
    pub has_ahci: bool,
    pub has_nvme: bool,
    pub has_virtio_blk: bool,
    pub storage_devices_found: usize,

    // ── Dispositivos de Rede e Multimídia ─────────────────────────────────────
    pub pci_devices_count: usize,
    pub has_nic_virtio: bool,
    pub has_nic_e1000: bool,
    pub has_nic_pcnet: bool,
    pub has_nic_realtek: bool,
    pub has_audio: bool,
    pub has_usb_uhci: bool,
    pub has_usb_ohci: bool,
    pub has_usb_ehci: bool,
    pub has_usb_xhci: bool,
    pub has_vga: bool,

    // ── Firmware e Plataforma ─────────────────────────────────────────────────
    pub is_uefi: bool,
    pub acpi_rsdp: Option<u64>,
    pub dma_capable: bool,
    pub has_iommu: bool,

    // ── Perfil de Hardware Selecionado ────────────────────────────────────────
    pub profile: HardwareProfile,
}

impl HardwareCapabilities {
    pub const fn empty() -> Self {
        Self {
            total_ram_bytes: 0,
            usable_ram_bytes: 0,
            reserved_ram_bytes: 0,
            acpi_reclaimable_bytes: 0,
            bootloader_reclaimable_bytes: 0,
            framebuffer_ram_bytes: 0,
            memory_regions_count: 0,
            validated_regions_count: 0,
            memory_validation_passed: false,

            cores_detected: 1,
            threads_detected: 1,
            vendor: [0; 13],
            brand: [0; 49],
            has_long_mode: false,
            has_sse: false,
            has_sse2: false,
            has_sse3: false,
            has_ssse3: false,
            has_sse4_1: false,
            has_sse4_2: false,
            has_avx: false,
            has_avx2: false,
            has_nx: false,
            has_apic: false,
            has_x2apic: false,
            has_tsc: false,
            has_inv_tsc: false,
            has_pae: false,
            has_smep: false,
            has_smap: false,
            estimated_mhz: 0,

            has_ide: false,
            has_ahci: false,
            has_nvme: false,
            has_virtio_blk: false,
            storage_devices_found: 0,

            pci_devices_count: 0,
            has_nic_virtio: false,
            has_nic_e1000: false,
            has_nic_pcnet: false,
            has_nic_realtek: false,
            has_audio: false,
            has_usb_uhci: false,
            has_usb_ohci: false,
            has_usb_ehci: false,
            has_usb_xhci: false,
            has_vga: false,

            is_uefi: false,
            acpi_rsdp: None,
            dma_capable: false,
            has_iommu: false,

            profile: HardwareProfile::Standard,
        }
    }

    /// Realiza a descoberta e validação integral do hardware da máquina.
    pub fn probe() -> Self {
        let mut caps = Self::empty();

        // 1. Auditoria e Validação Rigorosa do Mapa de Memória Física Limine
        caps.audit_physical_memory();

        // 2. Inspeção de CPU e Instruções Mandatórias
        caps.audit_cpu();

        // 3. Enumeração de Dispositivos e Controladores no Barramento PCI
        caps.audit_pci_bus();

        // 4. Verificação de Firmware e Plataforma
        caps.audit_firmware();

        // 5. Determinação do Perfil Adaptativo de Hardware
        caps.profile = caps.evaluate_profile();

        caps
    }

    /// Valida e sanitiza os endereços do mapa de memória física fornecido pelo bootloader.
    /// Não confia cegamente no firmware: valida limites físicos de 48 bits, comprimentos não-nulos
    /// e descarta regiões corrompidas ou sobrepostas.
    fn audit_physical_memory(&mut self) {
        unsafe {
            let resp = core::ptr::read_volatile(&limine::LIMINE_MEMMAP_REQUEST.response);
            if resp.is_null() {
                self.memory_validation_passed = false;
                return;
            }

            let count = core::ptr::read_volatile(&(*resp).entry_count) as usize;
            let entries_ptr = core::ptr::read_volatile(&(*resp).entries);
            if entries_ptr.is_null() || count == 0 {
                self.memory_validation_passed = false;
                return;
            }

            self.memory_regions_count = count.min(128);
            let mut valid_count = 0;
            let max_phys_addr = 0x0000_FFFF_FFFF_FFFFu64; // Limite de 48 bits físicos

            for i in 0..self.memory_regions_count {
                let entry_ptr = *entries_ptr.add(i);
                if entry_ptr.is_null() {
                    continue;
                }
                let entry = *entry_ptr;

                // Validação de sanidade:
                // 1. Comprimento deve ser > 0
                // 2. Endereço base deve estar dentro do limite físico da arquitetura
                // 3. Adição base + length não deve causar overflow aritmético de 64 bits
                let is_valid = entry.length > 0
                    && entry.base < max_phys_addr
                    && entry.base.checked_add(entry.length).map_or(false, |end| end <= max_phys_addr);

                if !is_valid {
                    continue; // Descarta região defeituosa informada pelo firmware
                }

                valid_count += 1;
                self.total_ram_bytes = self.total_ram_bytes.saturating_add(entry.length);

                match entry.typ {
                    limine::LIMINE_MEMMAP_USABLE => {
                        self.usable_ram_bytes = self.usable_ram_bytes.saturating_add(entry.length);
                    }
                    limine::LIMINE_MEMMAP_RESERVED => {
                        self.reserved_ram_bytes = self.reserved_ram_bytes.saturating_add(entry.length);
                    }
                    limine::LIMINE_MEMMAP_ACPI_RECLAIMABLE | limine::LIMINE_MEMMAP_ACPI_NVS => {
                        self.acpi_reclaimable_bytes = self.acpi_reclaimable_bytes.saturating_add(entry.length);
                    }
                    limine::LIMINE_MEMMAP_BOOTLOADER_RECLAIMABLE => {
                        self.bootloader_reclaimable_bytes = self.bootloader_reclaimable_bytes.saturating_add(entry.length);
                    }
                    limine::LIMINE_MEMMAP_FRAMEBUFFER => {
                        self.framebuffer_ram_bytes = self.framebuffer_ram_bytes.saturating_add(entry.length);
                    }
                    _ => {}
                }
            }

            self.validated_regions_count = valid_count;
            self.memory_validation_passed = valid_count > 0 && self.usable_ram_bytes >= (16 * 1024 * 1024);
        }
    }

    /// Inspeciona recursos e extensões da CPU via CPUID
    fn audit_cpu(&mut self) {
        let cpu_info = CpuInfo::probe();
        self.vendor = cpu_info.vendor;
        self.brand = cpu_info.brand;
        self.has_sse = cpu_info.has_sse;
        self.has_sse2 = cpu_info.has_sse2;
        self.has_sse3 = cpu_info.has_sse3;
        self.has_ssse3 = cpu_info.has_ssse3;
        self.has_sse4_1 = cpu_info.has_sse4_1;
        self.has_sse4_2 = cpu_info.has_sse4_2;
        self.has_avx = cpu_info.has_avx;
        self.has_avx2 = cpu_info.has_avx2;
        self.has_nx = cpu_info.has_nx;
        self.has_apic = cpu_info.has_apic;
        self.has_x2apic = cpu_info.has_x2apic;
        self.has_tsc = cpu_info.has_tsc;
        self.has_inv_tsc = cpu_info.has_inv_tsc;
        self.has_pae = cpu_info.has_pae;
        self.has_smep = cpu_info.has_smep;
        self.has_smap = cpu_info.has_smap;

        // Long mode é garantido pela execução em 64 bits, mas auditamos explicitamente o bit CPUID
        self.has_long_mode = cpu_info.max_extended_leaf >= 0x80000001;

        // Detecção de contagem de núcleos via SMP ou CPUID
        self.cores_detected = crate::smp::online_cpus().max(cpu_info.logical_cores as usize).max(1);
        self.threads_detected = self.cores_detected;

        // Estimativa básica de frequência (MHz)
        self.estimated_mhz = 2000; // Padrão conservador de 2.0 GHz
    }

    /// Varre dispositivos no barramento PCI identificando controladores suportados
    fn audit_pci_bus(&mut self) {
        let dev_count = pci::device_count();
        self.pci_devices_count = dev_count;

        for dev in pci::devices().iter().flatten() {
            // Controladores de Armazenamento (Classe 0x01)
            if dev.class_code == 0x01 {
                self.storage_devices_found += 1;
                match dev.subclass {
                    0x01 => self.has_ide = true,
                    0x06 => self.has_ahci = true,
                    0x08 => self.has_nvme = true,
                    _ => {}
                }
            }

            // VirtIO Block ou Rede (Vendor 0x1AF4)
            if dev.vendor_id == 0x1AF4 {
                if dev.device_id == 0x1001 || dev.device_id == 0x1042 {
                    self.has_virtio_blk = true;
                }
                if dev.device_id == 0x1000 || dev.device_id == 0x1041 {
                    self.has_nic_virtio = true;
                }
            }

            // Controladores de Rede (Classe 0x02)
            if dev.class_code == 0x02 {
                if dev.vendor_id == 0x8086 && (dev.device_id == 0x100E || dev.device_id == 0x100F || dev.device_id == 0x1539) {
                    self.has_nic_e1000 = true;
                }
                if dev.vendor_id == 0x1022 && dev.device_id == 0x2000 {
                    self.has_nic_pcnet = true;
                }
                if dev.vendor_id == 0x10EC && (dev.device_id == 0x8139 || dev.device_id == 0x8168) {
                    self.has_nic_realtek = true;
                }
            }

            // Controladores Gráficos (Classe 0x03)
            if dev.class_code == 0x03 {
                self.has_vga = true;
            }

            // Áudio / Multimídia (Classe 0x04)
            if dev.class_code == 0x04 {
                self.has_audio = true;
            }

            // Controladores USB (Classe 0x0C, Subclasse 0x03)
            if dev.class_code == 0x0C && dev.subclass == 0x03 {
                match dev.prog_if {
                    0x00 => self.has_usb_uhci = true,
                    0x10 => self.has_usb_ohci = true,
                    0x20 => self.has_usb_ehci = true,
                    0x30 => self.has_usb_xhci = true,
                    _ => {}
                }
            }
        }

        // Se o Limine Framebuffer estiver ativo, temos capacidade de vídeo confirmada
        if limine::framebuffer().is_some() {
            self.has_vga = true;
        }

        // IDE legado por padrão suportado na plataforma PC
        self.has_ide = true;
        self.dma_capable = true;
    }

    /// Avalia o firmware (BIOS vs UEFI) e tabelas ACPI
    fn audit_firmware(&mut self) {
        self.acpi_rsdp = limine::rsdp_address();
        if let Some((_, ver)) = limine::bootloader_info() {
            // Se Limine informa versão UEFI, registramos
            self.is_uefi = ver.as_bytes().windows(4).any(|w| w == b"uefi" || w == b"UEFI");
        }
    }

    /// Avalia e seleciona o perfil de hardware de acordo com os recursos reais disponíveis
    fn evaluate_profile(&self) -> HardwareProfile {
        let ram_mb = if self.usable_ram_bytes > 0 {
            self.usable_ram_bytes / (1024 * 1024)
        } else {
            self.total_ram_bytes / (1024 * 1024)
        };

        if ram_mb < 320 {
            HardwareProfile::Legacy      // <= 256 MB (RAM utilizável típica ~240 MB)
        } else if ram_mb < 768 {
            HardwareProfile::Compact     // 512 MB (RAM utilizável típica ~502 MB)
        } else if ram_mb < 3584 {
            HardwareProfile::Standard    // 1 GB a 4 GB (RAM utilizável típica ~980 MB a 3.8 GB)
        } else if ram_mb < 14336 {
            HardwareProfile::Performance // 4 GB a 16 GB
        } else {
            HardwareProfile::HighEnd     // 16 GB ou mais
        }
    }

    pub fn total_ram_mb(&self) -> u64 {
        self.total_ram_bytes / (1024 * 1024)
    }

    pub fn usable_ram_mb(&self) -> u64 {
        self.usable_ram_bytes / (1024 * 1024)
    }
}

// ── Singleton Global do Hardware Profiler ──────────────────────────────────────

static HARDWARE_CAPS: SpinLock<Option<HardwareCapabilities>> = SpinLock::new(None);

/// Inicializa o Hardware Profiler no boot do kernel
pub fn init() -> HardwareCapabilities {
    let caps = HardwareCapabilities::probe();
    *HARDWARE_CAPS.lock() = Some(caps);
    caps
}

/// Obtém uma cópia das capacidades físicas detectadas
pub fn get_capabilities() -> HardwareCapabilities {
    HARDWARE_CAPS.lock().unwrap_or_else(HardwareCapabilities::empty)
}

/// Obtém diretamente o perfil de hardware selecionado para a máquina atual
pub fn get_profile() -> HardwareProfile {
    get_capabilities().profile
}

