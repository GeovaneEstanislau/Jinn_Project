# [Backend SIMD] Implement NEON Vector Acceleration for AArch64 (ARM64)

**Repository:** `GeovaneEstanislau/jhcs-rs` (or `GeovaneEstanislau/Jinn_Project`)  
**Labels:** `good first issue`, `help wanted`, `jhcs`, `simd`, `embedded`, `arm64`

---

## 📖 Architectural Context

The **Jinn Heterogeneous Compute Service (JHCS)** is designed around the principle of cooperative, multi-platform acceleration. We currently provide optimized SIMD math kernels (`matmul_f32`, `conv2d_f32`, and `reduce_sum_f32`) accelerated via x86_64 AVX2/FMA with dynamic runtime CPUID detection and graceful fallback to scalar execution.

However, the embedded computing landscape and edge AI platforms (such as Raspberry Pi 4/5, NVIDIA Jetson, NXP i.MX8, and Rockchip RK3588) primarily run on 64-bit ARM architectures (**AArch64**). 

To make `jhcs-rs` the premier compute runtime for edge devices, we need native **ARM NEON** vector implementations for our core compute kernels.

---

## 🎯 Scope of Work

1. Implement 128-bit NEON vector paths in `crates/jhcs-backend-simd/src/ops.rs` and `matmul.rs` using `core::arch::aarch64::*`:
   - `reduce_sum_f32_neon`: Using vector addition (`vaddq_f32`) across 4-lane 32-bit registers, horizontal pairwise addition (`vpaddq_f32`), and scalar reduction.
   - `conv2d_f32_neon`: Using vector load (`vld1q_f32`), lane duplication (`vdupq_n_f32`), and fused multiply-accumulate (`vfmaq_f32`).
2. Add target architecture gating:
   ```rust
   #[cfg(target_arch = "aarch64")]
   use core::arch::aarch64::*;
   ```
3. Ensure strict `#![no_std]` compliance without heap dependencies.
4. Add parity unit tests comparing NEON outputs against the reference scalar implementation with numerical tolerance $\Delta < 10^{-4}$.

---

## 📂 Files Involved

- `crates/jhcs-backend-simd/src/ops.rs`
- `crates/jhcs-backend-simd/src/matmul.rs`
- `crates/jhcs-backend-simd/src/lib.rs`

---

## 🧪 How to Test Locally

You do **not** need a physical ARM board or QEMU to write and verify this code. You can cross-compile and test using `cross` or QEMU user-mode:

```bash
# 1. Clone the repository
git clone https://github.com/GeovaneEstanislau/jhcs-rs.git
cd jhcs-rs

# 2. Check compilation for ARM64 target
cargo check -p jhcs-backend-simd --target aarch64-unknown-linux-gnu

# 3. Run unit tests under QEMU user emulation (using cross)
cross test -p jhcs-backend-simd --target aarch64-unknown-linux-gnu
```

---

## ✅ Acceptance Criteria

- [ ] Kernels compile cleanly for `aarch64-unknown-linux-gnu` and `aarch64-unknown-none`.
- [ ] Automated tests verify exact numerical parity against scalar fallback.
- [ ] Zero warnings under `cargo clippy`.
- [ ] Safe encapsulation of `unsafe` intrinsic blocks with `# Safety` doc comments.
