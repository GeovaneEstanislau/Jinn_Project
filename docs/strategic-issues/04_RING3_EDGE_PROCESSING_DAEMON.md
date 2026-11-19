# [JHCS / Ring 3] Real-Time Edge Signal Processing & Inference Daemon Showcase

**Repository:** `GeovaneEstanislau/Jinn_Project`  
**Labels:** `help wanted`, `jhcs`, `embedded`, `ring3`, `showcase`

---

## 📖 Architectural Context

The **Jinn OS microkernel** operates under a strict **Zero-Idle Resource Consumption** policy: background daemons, drivers, and user applications in Ring 3 must consume 0% CPU cycles when idle and immediately yield buffers back to the system.

To demonstrate Jinn OS as a superior, lightweight platform for edge computing and embedded AI, we need a reference userspace daemon application (`apps/edge-compute`) that executes real-time digital signal processing or computer vision filtering (e.g., 2D Sobel edge detection or 1D audio convolution/FFT) on demand using SIMD intrinsics, while maintaining a microscopic memory footprint.

---

## 🎯 Scope of Work

1. Create a Ring 3 ELF binary in `apps/edge-compute/`:
   - Allocates input and output buffers via Jinn OS memory syscalls (`sys_alloc_page`).
   - Executes 2D image filtering (Sobel filter / Gaussian blur) or 1D audio convolution using `conv2d_f32` and `reduce_sum_f32`.
   - Cooperatively yields or waits for IPC events using `sys_yield()` or blocking event polling.
2. Integrate the binary into the build pipeline:
   - Add build rules in `scripts/build.ps1`.
   - Package `EDGE.ELF` into the FAT32 disk image via `scripts/make_disk.py`.
3. Provide automated telemetry output over the serial port (`COM1` / `stdio`) displaying:
   - Kernel memory consumed (< 32 MiB).
   - Execution time per filter pass.

---

## 📂 Files Involved

- `apps/edge-compute/Cargo.toml`
- `apps/edge-compute/src/main.rs`
- `scripts/build.ps1`
- `scripts/make_disk.py`
- `src/main.rs` (task spawning / shell command)

---

## 🧪 How to Test Locally

```powershell
# 1. Build the microkernel, userspace apps, and FAT32 disk image
.\scripts\build.ps1

# 2. Build bootable ISO and launch in QEMU
.\scripts\run.ps1

# 3. Check serial log for daemon output and memory assertions
Get-Content qemu_serial.log | Select-String -Pattern "\[EDGE-COMPUTE\]"
```

---

## ✅ Acceptance Criteria

- [ ] `EDGE.ELF` compiles cleanly for `x86_64-unknown-none` with `#![no_std]`.
- [ ] Daemon runs successfully in Ring 3 without triggering page faults (`#PF`) or General Protection Faults (`#GP`).
- [ ] Total memory footprint of Jinn OS remains strictly below 32 MiB of RAM.
- [ ] Demonstrates 0% CPU usage while awaiting triggers.
