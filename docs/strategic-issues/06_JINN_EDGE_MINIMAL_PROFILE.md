# [Embedded / Boot] "Jinn-Edge" Minimal Footprint Compilation Profile (< 16 MiB RAM Baseline)

**Repository:** `GeovaneEstanislau/Jinn_Project`  
**Labels:** `help wanted`, `embedded`, `kernel`, `footprint`, `optimization`

---

## 📖 Architectural Context

While the full Jinn OS desktop experience (Aladdin Desktop + Browser) runs comfortably within ~19–24 MiB of RAM under the 64 MiB ceiling, headless edge gateways, IoT edge nodes, and industrial telemetry hardware require a deterministic microkernel that boots in under **16 MiB of total system RAM**.

We want to introduce an explicit Cargo compilation profile and feature flag (`edge-minimal`) that trims all non-essential desktop subsystems and tunes the page allocator for low-memory targets.

---

## 🎯 Scope of Work

1. Add a Cargo feature flag `edge-minimal` in `Cargo.toml`.
2. When `edge-minimal` is active:
   - Compile out Limine graphical framebuffer backbuffers (saving ~3–6 MiB of VRAM allocations).
   - Adjust initial kernel heap pool from 4 MiB to 1 MiB.
   - Disable automatic launch of `aladdin.elf` compositor; boot directly into a lightweight serial console / telemetry daemon.
   - Configure page frame allocator (`src/memory/physical.rs`) for 16 MiB physical boundaries.
3. Validate that the microkernel boots cleanly to a ready prompt in < 500 ms in QEMU configured with `-m 16M`.

---

## 📂 Files Involved

- `Cargo.toml`
- `src/main.rs`
- `src/memory/physical.rs`
- `src/scheduler.rs`
- `scripts/run.ps1`

---

## 🧪 How to Test Locally

```powershell
# 1. Build kernel with edge-minimal feature
cargo build --release --no-default-features --features edge-minimal

# 2. Package ISO
.\scripts\run.ps1 -NoRun -SkipBuild

# 3. Launch QEMU with strictly 16 MB of RAM and headless serial
& "D:\Projetos\Jinn\tools\qemu\qemu-system-x86_64.exe" `
  -cdrom jinn.iso `
  -m 16M `
  -display none `
  -serial stdio
```

---

## ✅ Acceptance Criteria

- [ ] Kernel boots to completion in QEMU with `-m 16M` without physical allocator exhaustion.
- [ ] Memory reconciler (`JAHA-AUDIT-MEM`) confirms zero discrepancy and total RAM consumption < 8 MiB.
- [ ] Serial shell responds to interactive commands.
