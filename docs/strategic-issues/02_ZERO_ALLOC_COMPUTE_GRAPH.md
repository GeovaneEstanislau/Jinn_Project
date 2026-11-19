# [Core / Embedded] Zero-Alloc Static Mode for ComputeGraph (no extern crate alloc)

**Repository:** `GeovaneEstanislau/jhcs-rs`  
**Labels:** `help wanted`, `jhcs`, `embedded`, `hard-real-time`, `no-std`

---

## 📖 Architectural Context

In hard real-time, aerospace, and mission-critical embedded systems, dynamic heap allocation (`extern crate alloc`, `malloc`, `Box`, `Vec`) is strictly prohibited. Heap fragmentation can cause non-deterministic latency spikes or catastrophic out-of-memory panics.

The `jhcs-core` crate currently relies on `alloc::vec::Vec` for maintaining compute task lists, dependency graphs, and execution batch schedules. To position JHCS for ultra-reliable microcontroller and edge deployments, we need an optional **Zero-Alloc static arena mode**.

---

## 🎯 Scope of Work

1. Introduce a Cargo feature flag `static-arena` in `crates/jhcs-core/Cargo.toml`.
2. When `default-features = false` and `features = ["static-arena"]` are selected:
   - Eliminate all references to `extern crate alloc`.
   - Implement `ComputeGraph` using static const-generic capacities or `heapless::Vec` (e.g., `ComputeGraph<const MAX_NODES: usize = 64>`).
   - Implement topological sorting and execution batching using stack-allocated bitsets or fixed-size index arrays.
3. Preserve full backwards-compatibility when the default `alloc` feature is enabled.

---

## 📂 Files Involved

- `crates/jhcs-core/Cargo.toml`
- `crates/jhcs-core/src/graph.rs`
- `crates/jhcs-core/src/task.rs`
- `crates/jhcs-core/src/lib.rs`

---

## 🧪 How to Test Locally

```bash
# 1. Run tests with standard dynamic alloc (default)
cargo test -p jhcs-core

# 2. Check compilation for bare-metal target without alloc
cargo check -p jhcs-core --no-default-features --features static-arena --target thumbv7em-none-eabihf

# 3. Run unit tests verifying static graph sorting and cycle detection
cargo test -p jhcs-core --no-default-features --features static-arena
```

---

## ✅ Acceptance Criteria

- [ ] `ComputeGraph` can be created, populated, and executed with zero heap allocations.
- [ ] Compiles successfully for bare-metal targets (`x86_64-unknown-none` and `thumbv7em-none-eabihf`) without linking an allocator.
- [ ] Topological sort and cycle detection pass 100% of existing test cases under the static arena mode.
