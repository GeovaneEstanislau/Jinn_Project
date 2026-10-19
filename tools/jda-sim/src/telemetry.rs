//! JDA Telemetry & Latency Profiling Module
//!
//! Fornece métricas estatísticas rigorosas: mínimo, máximo, média,
//! mediana (p50), percentis p95 e p99, e desvio padrão.

use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct LatencyTracker {
    samples: Vec<Duration>,
}

impl LatencyTracker {
    pub fn new() -> Self {
        Self { samples: Vec::new() }
    }

    pub fn record(&mut self, duration: Duration) {
        self.samples.push(duration);
    }

    pub fn count(&self) -> usize {
        self.samples.len()
    }

    pub fn stats(&self) -> Option<LatencyStats> {
        if self.samples.is_empty() {
            return None;
        }

        let mut sorted = self.samples.clone();
        sorted.sort();

        let count = sorted.len();
        let min = sorted[0];
        let max = sorted[count - 1];

        let total_nanos: u128 = sorted.iter().map(|d| d.as_nanos()).sum();
        let mean_nanos = (total_nanos / count as u128) as u64;
        let mean = Duration::from_nanos(mean_nanos);

        let p50 = sorted[(count as f64 * 0.50).floor() as usize];
        let p95 = sorted[((count as f64 * 0.95).floor() as usize).min(count - 1)];
        let p99 = sorted[((count as f64 * 0.99).floor() as usize).min(count - 1)];

        // Desvio padrão
        let variance = sorted.iter().map(|d| {
            let diff = d.as_nanos() as f64 - mean_nanos as f64;
            diff * diff
        }).sum::<f64>() / count as f64;
        let stddev = Duration::from_nanos(variance.sqrt() as u64);

        Some(LatencyStats {
            count,
            min,
            max,
            mean,
            p50,
            p95,
            p99,
            stddev,
        })
    }
}

#[derive(Debug, Clone)]
pub struct LatencyStats {
    pub count: usize,
    pub min: Duration,
    pub max: Duration,
    pub mean: Duration,
    pub p50: Duration,
    pub p95: Duration,
    pub p99: Duration,
    pub stddev: Duration,
}

#[derive(Debug, Clone, Default)]
pub struct TelemetryReport {
    pub operations_submitted: u64,
    pub operations_completed: u64,
    pub operations_failed: u64,
    pub bytes_copied: u64,
    pub bytes_zero_copy: u64,
    pub copy_operations: u64,
    pub zero_copy_operations: u64,
    pub wall_time: Duration,
    pub latency: LatencyTracker,
}

impl TelemetryReport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_transfer(&mut self, bytes: usize, is_zero_copy: bool) {
        if is_zero_copy {
            self.bytes_zero_copy += bytes as u64;
            self.zero_copy_operations += 1;
        } else {
            self.bytes_copied += bytes as u64;
            self.copy_operations += 1;
        }
    }

    pub fn throughput_mb_per_sec(&self) -> f64 {
        let total_bytes = self.bytes_copied + self.bytes_zero_copy;
        let secs = self.wall_time.as_secs_f64();
        if secs > 0.0 {
            (total_bytes as f64 / (1024.0 * 1024.0)) / secs
        } else {
            0.0
        }
    }
}
