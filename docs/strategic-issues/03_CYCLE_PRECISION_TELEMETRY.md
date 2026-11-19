# [Benchmark / Telemetry] Cycle-Accurate Timers (RDTSC / CNTVCT_EL0) and Energy Efficiency Metrics in ComputeBroker

**Repository:** `GeovaneEstanislau/jhcs-rs`  
**Labels:** `help wanted`, `jhcs`, `embedded`, `telemetry`, `dsp`

---

## 📖 Architectural Context

In edge AI accelerators and embedded digital signal processing (DSP), scheduling decisions cannot rely solely on wall-clock averages. Engineers care intensely about **performance-per-watt**, determinism (tail latency / p99 jitter), and cycle-accurate cache pressure.

The `ComputeBroker` in `jhcs-broker` currently uses empirical time-based thresholds ($\mu s$) obtained via host platform timers to decide between Scalar CPU, SIMD, and GPU acceleration. We want to augment the broker's telemetry engine with hardware cycle-counting primitives and energy-efficiency estimation.

---

## 🎯 Scope of Work

1. Implement architecture-native cycle measurement in `jhcs-platform`:
   - `x86_64`: Serializing cycle counters using `core::arch::x86_64::_rdtsc()` and `_mm_lfence()`.
   - `AArch64`: Virtual timer counter register `CNTVCT_EL0` via inline assembly.
2. Extend `ExecutionMetrics` in `crates/jhcs-broker/src/metrics.rs` to track:
   - `cycles_elapsed: u64`
   - `cycles_per_flop: f32`
   - `estimated_energy_score: f32` (calculated based on active backend power profile and cycle duration).
3. Update `examples/benchmark-breakeven` to output cycle-level statistics alongside nanoseconds.

---

## 📂 Files Involved

- `crates/jhcs-platform/src/lib.rs`
- `crates/jhcs-platform-host/src/lib.rs`
- `crates/jhcs-platform-jinn/src/lib.rs`
- `crates/jhcs-broker/src/metrics.rs`
- `crates/jhcs-broker/src/policy.rs`
- `examples/benchmark-breakeven/src/main.rs`

---

## 🧪 How to Test Locally

```bash
# 1. Run all broker unit tests
cargo test -p jhcs-broker

# 2. Run the breakeven benchmark and observe cycle counts
cargo run --release -p benchmark-breakeven
```

---

## ✅ Acceptance Criteria

- [ ] High-resolution cycle counting operates accurately on both x86_64 and AArch64.
- [ ] Broker records cycle metrics for every task batch executed.
- [ ] Benchmark output reports cycles per operation and break-even points in clock cycles.
