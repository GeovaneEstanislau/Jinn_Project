/// Jinn OS — JDA: Hardware Topology Model
///
/// Modela a topologia física real identificada via CPUID, tabelas ACPI,
/// mapas de memória do Limine e barramento PCI.
///
/// Cada propriedade é associada a um nível de confiança explícito:
/// - ProbedHardware (100): Leitura direta de registrador ou CPUID do processador.
/// - Estimated (50): Estimado a partir de parâmetros arquiteturais conhecidos.
/// - Unknown (0): Não mensurado ou não reportado pelo hardware.

use crate::cpu::{cpuid, cpuid_count};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Confidence {
    Unknown = 0,
    Estimated = 50,
    ProbedHardware = 100,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheType {
    Data,
    Instruction,
    Unified,
    Unknown,
}

#[derive(Debug, Clone, Copy)]
pub struct CacheDescriptor {
    pub level: u8,
    pub cache_type: CacheType,
    pub size_bytes: usize,
    pub line_size: u16,
    pub ways: u16,
    pub sets: u32,
    pub confidence: Confidence,
}

impl CacheDescriptor {
    pub const fn empty() -> Self {
        Self {
            level: 0,
            cache_type: CacheType::Unknown,
            size_bytes: 0,
            line_size: 0,
            ways: 0,
            sets: 0,
            confidence: Confidence::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CpuCoreTopology {
    pub apic_id: u8,
    pub logical_id: u8,
    pub is_bsp: bool,
    pub has_sse: bool,
    pub has_avx: bool,
    pub has_avx2: bool,
    pub confidence: Confidence,
}

impl CpuCoreTopology {
    pub const fn empty() -> Self {
        Self {
            apic_id: 0,
            logical_id: 0,
            is_bsp: false,
            has_sse: false,
            has_avx: false,
            has_avx2: false,
            confidence: Confidence::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryRegionTopology {
    pub base_paddr: u64,
    pub length: u64,
    pub is_usable_dram: bool,
    pub numa_node: u8,
    pub confidence: Confidence,
}

impl MemoryRegionTopology {
    pub const fn empty() -> Self {
        Self {
            base_paddr: 0,
            length: 0,
            is_usable_dram: false,
            numa_node: 0,
            confidence: Confidence::Unknown,
        }
    }
}

pub const MAX_TOPOLOGY_CORES: usize = 16;
pub const MAX_TOPOLOGY_CACHES: usize = 8;
pub const MAX_TOPOLOGY_MEM_REGIONS: usize = 16;

pub struct HardwareTopologyModel {
    pub cores: [CpuCoreTopology; MAX_TOPOLOGY_CORES],
    pub core_count: usize,

    pub caches: [CacheDescriptor; MAX_TOPOLOGY_CACHES],
    pub cache_count: usize,

    pub memory_regions: [MemoryRegionTopology; MAX_TOPOLOGY_MEM_REGIONS],
    pub mem_region_count: usize,

    pub is_initialized: bool,
}

impl HardwareTopologyModel {
    pub const fn new() -> Self {
        Self {
            cores: [CpuCoreTopology::empty(); MAX_TOPOLOGY_CORES],
            core_count: 0,
            caches: [CacheDescriptor::empty(); MAX_TOPOLOGY_CACHES],
            cache_count: 0,
            memory_regions: [MemoryRegionTopology::empty(); MAX_TOPOLOGY_MEM_REGIONS],
            mem_region_count: 0,
            is_initialized: false,
        }
    }

    /// Executa varredura de hardware real via CPUID e mapas do sistema.
    pub fn probe(&mut self) {
        self.probe_cores();
        self.probe_caches();
        self.probe_memory();
        self.is_initialized = true;
    }

    fn probe_cores(&mut self) {
        self.core_count = 0;
        let (max_basic, _ebx, _, _) = unsafe { cpuid(0) };
        if max_basic >= 1 {
            let (_, ebx_feat, ecx_feat, edx_feat) = unsafe { cpuid(1) };
            let apic_id = ((ebx_feat >> 24) & 0xFF) as u8;

            let has_sse = (edx_feat & (1 << 25)) != 0;
            let has_avx = (ecx_feat & (1 << 28)) != 0;

            let mut has_avx2 = false;
            if max_basic >= 7 {
                let (_, ebx7, _, _) = unsafe { cpuid_count(7, 0) };
                has_avx2 = (ebx7 & (1 << 5)) != 0;
            }

            self.cores[0] = CpuCoreTopology {
                apic_id,
                logical_id: 0,
                is_bsp: true,
                has_sse,
                has_avx,
                has_avx2,
                confidence: Confidence::ProbedHardware,
            };
            self.core_count = 1;
        }
    }

    fn probe_caches(&mut self) {
        self.cache_count = 0;
        let (max_basic, _, _, _) = unsafe { cpuid(0) };

        // Testa suporte à folha determinística de cache (CPUID Leaf 4)
        if max_basic >= 4 {
            let mut subleaf = 0u32;
            while subleaf < (MAX_TOPOLOGY_CACHES as u32) {
                let (eax, ebx, ecx, _) = unsafe { cpuid_count(4, subleaf) };
                let cache_type_raw = eax & 0x1F;
                if cache_type_raw == 0 {
                    break; // Fim dos descritores de cache
                }

                let cache_type = match cache_type_raw {
                    1 => CacheType::Data,
                    2 => CacheType::Instruction,
                    3 => CacheType::Unified,
                    _ => CacheType::Unknown,
                };

                let level = ((eax >> 5) & 0x07) as u8;
                let line_size = ((ebx & 0x0FFF) + 1) as u16;
                let partitions = (((ebx >> 12) & 0x03FF) + 1) as u16;
                let ways = (((ebx >> 22) & 0x03FF) + 1) as u16;
                let sets = (ecx + 1) as u32;

                let size_bytes = (ways as usize)
                    * (partitions as usize)
                    * (line_size as usize)
                    * (sets as usize);

                if self.cache_count < MAX_TOPOLOGY_CACHES {
                    self.caches[self.cache_count] = CacheDescriptor {
                        level,
                        cache_type,
                        size_bytes,
                        line_size,
                        ways,
                        sets,
                        confidence: Confidence::ProbedHardware,
                    };
                    self.cache_count += 1;
                }
                subleaf += 1;
            }
        }

        // Se a Folha 4 não retornou dados (ex: processador AMD clássico ou VM simples),
        // consulta a folha estendida 0x80000006 (L2/L3)
        if self.cache_count == 0 {
            let (max_ext, _, _, _) = unsafe { cpuid(0x80000000) };
            if max_ext >= 0x80000006 {
                let (_, _, ecx, edx) = unsafe { cpuid(0x80000006) };
                let l2_size_kb = (ecx >> 16) as usize;
                let l2_line = (ecx & 0xFF) as u16;
                if l2_size_kb > 0 {
                    self.caches[0] = CacheDescriptor {
                        level: 2,
                        cache_type: CacheType::Unified,
                        size_bytes: l2_size_kb * 1024,
                        line_size: l2_line,
                        ways: ((ecx >> 12) & 0x0F) as u16,
                        sets: 0,
                        confidence: Confidence::ProbedHardware,
                    };
                    self.cache_count = 1;
                }

                let l3_size_512kb = (edx >> 18) as usize;
                if l3_size_512kb > 0 && self.cache_count < MAX_TOPOLOGY_CACHES {
                    self.caches[self.cache_count] = CacheDescriptor {
                        level: 3,
                        cache_type: CacheType::Unified,
                        size_bytes: l3_size_512kb * 512 * 1024,
                        line_size: (edx & 0xFF) as u16,
                        ways: ((edx >> 12) & 0x0F) as u16,
                        sets: 0,
                        confidence: Confidence::ProbedHardware,
                    };
                    self.cache_count += 1;
                }
            }
        }
    }

    fn probe_memory(&mut self) {
        self.mem_region_count = 0;
        unsafe {
            let resp = core::ptr::read_volatile(&crate::limine::LIMINE_MEMMAP_REQUEST.response);
            if resp.is_null() {
                return;
            }
            let count = core::ptr::read_volatile(&(*resp).entry_count) as usize;
            let entries = core::ptr::read_volatile(&(*resp).entries);
            if entries.is_null() {
                return;
            }

            for i in 0..count.min(MAX_TOPOLOGY_MEM_REGIONS) {
                let entry_ptr = core::ptr::read_volatile(entries.add(i));
                if entry_ptr.is_null() {
                    continue;
                }
                let entry = core::ptr::read_volatile(entry_ptr);
                let is_usable = entry.typ == crate::limine::LIMINE_MEMMAP_USABLE;

                self.memory_regions[self.mem_region_count] = MemoryRegionTopology {
                    base_paddr: entry.base,
                    length: entry.length,
                    is_usable_dram: is_usable,
                    numa_node: 0, // Single-node default até parser SRAT/SLIT ACPI
                    confidence: Confidence::ProbedHardware,
                };
                self.mem_region_count += 1;
            }
        }
    }

    /// Retorna a soma de toda a DRAM utilizável física detectada em bytes.
    pub fn total_usable_dram(&self) -> u64 {
        let mut total = 0u64;
        for i in 0..self.mem_region_count {
            if self.memory_regions[i].is_usable_dram {
                total += self.memory_regions[i].length;
            }
        }
        total
    }

    /// Localiza o primeiro descritor de cache correspondente ao nível solicitado.
    pub fn find_cache(&self, level: u8) -> Option<&CacheDescriptor> {
        for i in 0..self.cache_count {
            if self.caches[i].level == level {
                return Some(&self.caches[i]);
            }
        }
        None
    }
}
