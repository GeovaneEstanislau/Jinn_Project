/// Jinn OS — JDA: Observability & Telemetry Engine
///
/// Instrumentação precisa de baixo overhead utilizando contadores de ciclos TSC (rdtsc),
/// rastreando operações, transferências de dados, contenção e cópias evitadas.

use crate::memory::SpinLock;

/// Lê o registrador de timestamp da CPU (TSC) retornando o número de ciclos decorridos.
#[inline]
pub fn read_tsc() -> u64 {
    let rax: u64;
    let rdx: u64;
    unsafe {
        core::arch::asm!(
            "rdtsc",
            out("rax") rax,
            out("rdx") rdx,
            options(nomem, nostack)
        );
    }
    (rdx << 32) | rax
}

#[derive(Debug, Clone, Copy, Default)]
pub struct JdaMetrics {
    /// Total de operações submetidas ao motor
    pub total_operations_submitted: u64,
    /// Total de operações concluídas com sucesso
    pub total_operations_completed: u64,
    /// Total de operações com falha
    pub total_operations_failed: u64,

    /// Número de transferências realizadas via Zero-Copy (mapeamento de ponteiros)
    pub zero_copy_transfers: u64,
    /// Número de transferências realizadas via cópia de memória convencional (fallback)
    pub redundant_copies: u64,

    /// Volume em bytes transmitido via Zero-Copy
    pub bytes_transferred_zero_copy: u64,
    /// Volume em bytes copiado na RAM
    pub bytes_copied_memory: u64,

    /// Total de ciclos de CPU gastos em computação útil
    pub cycles_compute: u64,
    /// Total de ciclos de CPU gastos em espera (wait/idle)
    pub cycles_waiting: u64,

    /// Predições de dados assertivas (cache hit / prefetch hit)
    pub prefetch_hits: u64,
    /// Predições erradas de dados (prefetch inútil / desperdiçado)
    pub prefetch_misses: u64,
}

pub static GLOBAL_METRICS: SpinLock<JdaMetrics> = SpinLock::new(JdaMetrics {
    total_operations_submitted: 0,
    total_operations_completed: 0,
    total_operations_failed: 0,
    zero_copy_transfers: 0,
    redundant_copies: 0,
    bytes_transferred_zero_copy: 0,
    bytes_copied_memory: 0,
    cycles_compute: 0,
    cycles_waiting: 0,
    prefetch_hits: 0,
    prefetch_misses: 0,
});

/// Registra uma operação concluída com métricas de tempo
pub fn record_operation_completed(cycles_duration: u64) {
    let mut m = GLOBAL_METRICS.lock();
    m.total_operations_completed += 1;
    m.cycles_compute += cycles_duration;
}

/// Registra uma transferência de dados
pub fn record_data_transfer(bytes: usize, is_zero_copy: bool) {
    let mut m = GLOBAL_METRICS.lock();
    if is_zero_copy {
        m.zero_copy_transfers += 1;
        m.bytes_transferred_zero_copy += bytes as u64;
    } else {
        m.redundant_copies += 1;
        m.bytes_copied_memory += bytes as u64;
    }
}

/// Retorna uma cópia snapshot das métricas globais
pub fn get_metrics_snapshot() -> JdaMetrics {
    *GLOBAL_METRICS.lock()
}

/// Zera os contadores de telemetria
pub fn reset_metrics() {
    let mut m = GLOBAL_METRICS.lock();
    *m = JdaMetrics::default();
}
