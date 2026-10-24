//! Jinn OS — Subsistema de Gerenciamento e Contabilidade de Recursos por Processo
//!
//! Controla e monitora de forma previsível o uso de CPU, memória física,
//! descritores de arquivos, conexões de rede, buffers JDA e janelas do compositor.

use crate::syscall::{EMFILE, ENOBUFS, ENOMEM};

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
            max_frames: 128,      // Permite ~115 frames iniciais (64 stack + 49 ELF/BSS) + heap dinâmico
            max_fds: 8,           // 8 descritores por processo (MAX_TASK_FDS)
            max_windows: 4,       // Até 4 janelas por processo (sistema suporta 8)
            max_jda_buffers: 4,   // Até 4 buffers JDA Zero-Copy
            max_sockets: 4,       // Até 4 sockets de rede
            max_cpu_ticks: 0,     // Padrão: ilimitado (ou atribuído especificamente)
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
