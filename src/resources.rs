//! Jinn OS — Subsistema de Gerenciamento e Contabilidade de Recursos por Processo
//!
//! Controla e monitora de forma previsível o uso de CPU, memória física,
//! descritores de arquivos, conexões de rede, buffers JDA e janelas do compositor.

use crate::syscall::{EMFILE, ENOBUFS, ENOMEM};
use crate::hardware_profiler::HardwareProfile;
use crate::memory::SpinLock;

/// Limites configuráveis de consumo de recursos por processo.
#[derive(Debug, Clone, Copy)]
pub struct ResourceLimits {
    /// Número máximo de frames físicos (4 KiB) mapeados para a tarefa.
    pub max_frames: usize,
    /// Quantidade máxima de descritores de arquivos abertos simultaneamente.
    pub max_fds: usize,
    /// Quantidade máxima de janelas gráficas no Aladdin Desktop.
    pub max_windows: usize,
    /// Quantidade máxima de buffers compartilhados Zero-Copy JDA.
    pub max_jda_buffers: usize,
    /// Quantidade máxima de sockets de rede ativos.
    pub max_sockets: usize,
    /// Cota máxima de ticks de CPU (0 = ilimitado).
    pub max_cpu_ticks: u64,
}

impl ResourceLimits {
    /// Limites conservadores e seguros para aplicações em Ring 3.
    pub const fn user_default() -> Self {
        Self {
            max_frames: 2048,     // Permite stack, ELF/BSS e superfícies dinâmicas até 8 MiB
            max_fds: 8,           // 8 descritores por processo (MAX_TASK_FDS)
            max_windows: 4,       // Até 4 janelas por processo (sistema suporta 8)
            max_jda_buffers: 4,   // Até 4 buffers JDA Zero-Copy
            max_sockets: 4,       // Até 4 sockets de rede
            max_cpu_ticks: 0,     // Padrão: ilimitado (ou atribuído especificamente)
        }
    }

    /// Limites adaptativos por processo de acordo com o Perfil Oficial de Hardware (JAHA 1.0)
    pub const fn user_for_profile(profile: HardwareProfile) -> Self {
        Self {
            max_frames: profile.max_user_frames(),
            max_fds: match profile {
                HardwareProfile::Legacy => 8,
                HardwareProfile::Compact => 12,
                HardwareProfile::Standard => 16,
                HardwareProfile::Performance => 32,
                HardwareProfile::HighEnd => 64,
            },
            max_windows: match profile {
                HardwareProfile::Legacy => 2,
                HardwareProfile::Compact => 4,
                HardwareProfile::Standard => 6,
                HardwareProfile::Performance => 8,
                HardwareProfile::HighEnd => 16,
            },
            max_jda_buffers: match profile {
                HardwareProfile::Legacy => 2,
                HardwareProfile::Compact => 4,
                HardwareProfile::Standard => 8,
                HardwareProfile::Performance => 16,
                HardwareProfile::HighEnd => 32,
            },
            max_sockets: match profile {
                HardwareProfile::Legacy => 4,
                HardwareProfile::Compact => 8,
                HardwareProfile::Standard => 16,
                HardwareProfile::Performance => 32,
                HardwareProfile::HighEnd => 64,
            },
            max_cpu_ticks: 0,
        }
    }

    /// Limites ampliados para tarefas essenciais do Kernel (Ring 0).
    pub const fn kernel_default() -> Self {
        Self {
            max_frames: 65536,    // 256 MiB
            max_fds: 8,
            max_windows: 8,
            max_jda_buffers: 16,
            max_sockets: 16,
            max_cpu_ticks: 0,
        }
    }
}

/// Contabilidade em tempo real dos recursos alocados para um processo.
#[derive(Debug, Clone, Copy)]
pub struct ProcessResources {
    /// Frames físicos de 4 KiB atualmente mapeados para o processo.
    pub allocated_frames: usize,
    /// Pico histórico de frames físicos alocados.
    pub peak_allocated_frames: usize,
    /// Número atual de descritores de arquivos abertos.
    pub open_fds: usize,
    /// Número atual de janelas gráficas abertas no Aladdin.
    pub open_windows: usize,
    /// Número atual de buffers Zero-Copy JDA em uso.
    pub open_jda_buffers: usize,
    /// Número atual de conexões/sockets de rede abertos.
    pub open_sockets: usize,
    /// Contador de violações de limites rejeitadas pelo kernel.
    pub limit_violations: u32,
    /// Último código de erro gerado por violação de limite.
    pub last_violation_code: i32,
}

impl ProcessResources {
    pub const fn new() -> Self {
        Self {
            allocated_frames: 0,
            peak_allocated_frames: 0,
            open_fds: 0,
            open_windows: 0,
            open_jda_buffers: 0,
            open_sockets: 0,
            limit_violations: 0,
            last_violation_code: 0,
        }
    }

    /// Tenta registrar a alocação de `count` frames de memória.
    pub fn check_and_alloc_frames(&mut self, count: usize, limits: &ResourceLimits) -> Result<(), isize> {
        let new_total = self.allocated_frames.saturating_add(count);
        if new_total > limits.max_frames {
            self.limit_violations = self.limit_violations.saturating_add(1);
            self.last_violation_code = ENOMEM as i32;
            return Err(ENOMEM);
        }
        self.allocated_frames = new_total;
        if self.allocated_frames > self.peak_allocated_frames {
            self.peak_allocated_frames = self.allocated_frames;
        }
        Ok(())
    }

    /// Desaloca `count` frames de memória.
    pub fn free_frames(&mut self, count: usize) {
        self.allocated_frames = self.allocated_frames.saturating_sub(count);
    }

    /// Tenta registrar a abertura de um descritor de arquivo.
    pub fn check_and_open_fd(&mut self, limits: &ResourceLimits) -> Result<(), isize> {
        if self.open_fds >= limits.max_fds {
            self.limit_violations = self.limit_violations.saturating_add(1);
            self.last_violation_code = EMFILE as i32;
            return Err(EMFILE);
        }
        self.open_fds += 1;
        Ok(())
    }

    /// Registra o fechamento de um descritor de arquivo.
    pub fn close_fd(&mut self) {
        self.open_fds = self.open_fds.saturating_sub(1);
    }

    /// Tenta registrar a criação de uma janela no Aladdin Desktop.
    pub fn check_and_open_window(&mut self, limits: &ResourceLimits) -> Result<(), isize> {
        if self.open_windows >= limits.max_windows {
            self.limit_violations = self.limit_violations.saturating_add(1);
            self.last_violation_code = EMFILE as i32;
            return Err(EMFILE);
        }
        self.open_windows += 1;
        Ok(())
    }

    /// Registra o fechamento de uma janela.
    pub fn close_window(&mut self) {
        self.open_windows = self.open_windows.saturating_sub(1);
    }

    /// Tenta registrar a alocação de um buffer Zero-Copy JDA.
    pub fn check_and_alloc_jda(&mut self, limits: &ResourceLimits) -> Result<(), isize> {
        if self.open_jda_buffers >= limits.max_jda_buffers {
            self.limit_violations = self.limit_violations.saturating_add(1);
            self.last_violation_code = ENOBUFS as i32;
            return Err(ENOBUFS);
        }
        self.open_jda_buffers += 1;
        Ok(())
    }

    /// Registra a liberação de um buffer Zero-Copy JDA.
    pub fn release_jda(&mut self) {
        self.open_jda_buffers = self.open_jda_buffers.saturating_sub(1);
    }

    /// Tenta registrar a abertura de um socket de rede.
    pub fn check_and_open_socket(&mut self, limits: &ResourceLimits) -> Result<(), isize> {
        if self.open_sockets >= limits.max_sockets {
            self.limit_violations = self.limit_violations.saturating_add(1);
            self.last_violation_code = EMFILE as i32;
            return Err(EMFILE);
        }
        self.open_sockets += 1;
        Ok(())
    }

    /// Registra o fechamento de um socket de rede.
    pub fn close_socket(&mut self) {
        self.open_sockets = self.open_sockets.saturating_sub(1);
    }
}

/// Estrutura exportada para o espaço de usuário para consulta de métricas (Syscall 25: `sys_res_stat`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ProcessResourceStat {
    pub pid: u32,
    pub state: u32,
    pub allocated_frames: u32,
    pub peak_allocated_frames: u32,
    pub open_fds: u32,
    pub open_windows: u32,
    pub open_jda_buffers: u32,
    pub open_sockets: u32,
    pub cpu_ticks: u64,
    pub ctx_switches: u64,
    pub limit_violations: u32,
    pub last_violation_code: i32,
    pub max_frames: u32,
    pub max_fds: u32,
    pub max_windows: u32,
    pub max_sockets: u32,
    pub max_cpu_ticks: u64,
    pub irq_preemptions: u64,
    pub voluntary_yields: u64,
}

// ── Adaptive Resource Manager (JAHA 1.0) ──────────────────────────────────────

/// Estado dinâmico de pressão de memória do sistema
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryPressureState {
    /// Mais de 25% de RAM utilizável livre: operação plena e caches ativos
    Normal,
    /// Entre 10% e 25% de RAM utilizável: alerta moderado, redução de pré-busca
    Moderate,
    /// Menos de 10% de RAM utilizável: contenção estrita, descarte de caches e hibernação de abas
    Critical,
}

impl MemoryPressureState {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Moderate => "MODERATE",
            Self::Critical => "CRITICAL",
        }
    }
}

/// Gerenciador Adaptativo de Recursos do Microkernel Jinn
#[derive(Debug, Clone, Copy)]
pub struct AdaptiveResourceManager {
    pub profile: HardwareProfile,
    pub pressure_state: MemoryPressureState,
    pub total_ram_bytes: u64,
    pub usable_ram_bytes: u64,
    pub reserved_ram_bytes: u64,
    pub last_free_bytes: u64,
    pub backpressure_events: u64,
    pub cache_reduction_events: u64,
    pub tab_hibernations_triggered: u64,
}

impl AdaptiveResourceManager {
    pub const fn empty() -> Self {
        Self {
            profile: HardwareProfile::Standard,
            pressure_state: MemoryPressureState::Normal,
            total_ram_bytes: 0,
            usable_ram_bytes: 0,
            reserved_ram_bytes: 0,
            last_free_bytes: 0,
            backpressure_events: 0,
            cache_reduction_events: 0,
            tab_hibernations_triggered: 0,
        }
    }

    pub fn new(profile: HardwareProfile, total_ram: u64, usable_ram: u64, reserved_ram: u64) -> Self {
        Self {
            profile,
            pressure_state: MemoryPressureState::Normal,
            total_ram_bytes: total_ram,
            usable_ram_bytes: usable_ram,
            reserved_ram_bytes: reserved_ram,
            last_free_bytes: usable_ram,
            backpressure_events: 0,
            cache_reduction_events: 0,
            tab_hibernations_triggered: 0,
        }
    }

    /// Avalia a pressão atual de memória física livre contra a memória utilizável
    pub fn evaluate_pressure(&mut self, free_bytes: u64) -> MemoryPressureState {
        self.last_free_bytes = free_bytes;
        if self.usable_ram_bytes == 0 {
            self.pressure_state = MemoryPressureState::Normal;
            return MemoryPressureState::Normal;
        }

        let free_pct = (free_bytes.saturating_mul(100)) / self.usable_ram_bytes;
        let old_state = self.pressure_state;

        let new_state = if free_pct < 10 {
            MemoryPressureState::Critical
        } else if free_pct < 25 {
            MemoryPressureState::Moderate
        } else {
            MemoryPressureState::Normal
        };

        if new_state != old_state && (new_state == MemoryPressureState::Critical || new_state == MemoryPressureState::Moderate) {
            self.backpressure_events = self.backpressure_events.saturating_add(1);
        }

        self.pressure_state = new_state;
        new_state
    }

    /// Indica se abas do navegador em segundo plano devem ser forçadas a hibernar
    pub fn should_hibernate_tabs(&self) -> bool {
        match self.profile {
            HardwareProfile::Legacy => true, // No perfil Legacy (256 MB), abas inativas hibernam imediatamente
            _ => self.pressure_state == MemoryPressureState::Critical || self.pressure_state == MemoryPressureState::Moderate,
        }
    }

    /// Indica se o cache LRU de blocos de disco deve esvaziar buffers limpos
    pub fn should_reduce_cache(&self) -> bool {
        self.pressure_state == MemoryPressureState::Critical
    }

    /// Registra evento de backpressure
    pub fn record_backpressure(&mut self) {
        self.backpressure_events = self.backpressure_events.saturating_add(1);
    }

    /// Registra gatilho de hibernação de aba disparado pelo Resource Manager
    pub fn record_tab_hibernation(&mut self) {
        self.tab_hibernations_triggered = self.tab_hibernations_triggered.saturating_add(1);
    }
}

pub static RESOURCE_MANAGER: SpinLock<AdaptiveResourceManager> = SpinLock::new(AdaptiveResourceManager::empty());

pub fn init_resource_manager(profile: HardwareProfile, total_ram: u64, usable_ram: u64, reserved_ram: u64) {
    *RESOURCE_MANAGER.lock() = AdaptiveResourceManager::new(profile, total_ram, usable_ram, reserved_ram);
}

pub fn get_resource_manager() -> AdaptiveResourceManager {
    *RESOURCE_MANAGER.lock()
}

pub fn current_pressure() -> MemoryPressureState {
    let free_bytes = crate::memory::alloc::free_frames() as u64 * 4096;
    RESOURCE_MANAGER.lock().evaluate_pressure(free_bytes)
}
