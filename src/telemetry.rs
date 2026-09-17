
use core::sync::atomic::{AtomicU64, Ordering};

pub const MAX_METRICS: usize = 64;

#[derive(Debug)]
pub struct TaskMetrics {
    pub cpu_ticks: AtomicU64,
    pub ipc_tx_count: AtomicU64,
    pub ipc_rx_count: AtomicU64,
    pub page_faults: AtomicU64,
}

impl TaskMetrics {
    const fn new() -> Self {
        Self {
            cpu_ticks: AtomicU64::new(0),
            ipc_tx_count: AtomicU64::new(0),
            ipc_rx_count: AtomicU64::new(0),
            page_faults: AtomicU64::new(0),
        }
    }
}

// Global, lock-free telemetry array
const INIT_METRIC: TaskMetrics = TaskMetrics::new();
static METRICS: [TaskMetrics; MAX_METRICS] = [INIT_METRIC; MAX_METRICS];

#[inline(always)]
pub fn record_cpu_tick(pid: usize) {
    if pid < MAX_METRICS {
        METRICS[pid].cpu_ticks.fetch_add(1, Ordering::Relaxed);
    }
}

#[inline(always)]
pub fn record_ipc_tx(pid: usize) {
    if pid < MAX_METRICS {
        METRICS[pid].ipc_tx_count.fetch_add(1, Ordering::Relaxed);
    }
}

#[inline(always)]
pub fn record_ipc_rx(pid: usize) {
    if pid < MAX_METRICS {
        METRICS[pid].ipc_rx_count.fetch_add(1, Ordering::Relaxed);
    }
}

#[inline(always)]
pub fn record_page_fault(pid: usize) {
    if pid < MAX_METRICS {
        METRICS[pid].page_faults.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn get_metrics(pid: usize) -> Option<(u64, u64, u64, u64)> {
    if pid < MAX_METRICS {
        let ticks = METRICS[pid].cpu_ticks.load(Ordering::Relaxed);
        let tx = METRICS[pid].ipc_tx_count.load(Ordering::Relaxed);
        let rx = METRICS[pid].ipc_rx_count.load(Ordering::Relaxed);
        let pf = METRICS[pid].page_faults.load(Ordering::Relaxed);
        Some((ticks, tx, rx, pf))
    } else {
        None
    }
}
