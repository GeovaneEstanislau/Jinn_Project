<div align="center">
  <img src="https://raw.githubusercontent.com/jinn-os/jinn-os/main/assets/jinn-logo.png" alt="Jinn OS Logo" width="200" height="200" />
  <h1>Jinn OS</h1>
  <p><strong>A Next-Generation, Zero-Trust Microkernel OS written in Rust 🦀</strong></p>

  [![Rust](https://img.shields.io/badge/rust-nightly-orange.svg)](https://www.rust-lang.org)
  [![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
  [![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](CONTRIBUTING.md)
  [![Status](https://img.shields.io/badge/Status-Phase%_1%_Complete-blue)](docs/roadmap/roadmap.md)
</div>

<br>

**Jinn OS** is a modern, capability-based microkernel operating system built from scratch. It aims to completely rethink OS security and performance by stripping away legacy monolothic paradigms (like the `root` user) and embracing a pure **Zero-Trust** capability model, zero-copy IPC, and a predictive memory engine.

---

## ⚡ Core Philosophy

1. **Zero-Trust by Default:** No service, not even device drivers, has inherent privileges. All resources (memory, IPC, I/O ports) are strictly mediated by cryptographically verifiable *Capability Tokens*.
2. **Microkernel Architecture:** The kernel does the bare minimum—scheduling, capability management, and IPC. Everything else (VFS, Network, Drivers) runs in isolated user-space sandboxes.
3. **Ultra-Low Latency IPC:** Designed with synchronous rendezvous (*Direct Thread Switching*) and lock-free SPSC ring buffers, achieving `< 500ns` RPC round-trips.
4. **Predictive Memory Engine:** Smart telemetry pre-warms cache and memory pools, avoiding the latency spikes of traditional lazy paging.

---

## 🏗️ Architecture

```text
┌──────────────────────────────────────────────────────────────┐
│ APLICAÇÕES & DRIVERS (User Space Sandboxed)                  │
│ ┌──────────┬──────────┬──────────┬──────────┬──────────┐   │
│ │ VFS Srv  │ Net Srv  │ NVMe Drv │ GPU Drv  │   Apps   │   │
│ └────┬─────┴────┬─────┴─────┬────┴────┬─────┴─────┬────┘   │
└──────┼──────────┼───────────┼────────┼─────────┼───────────┘
       │    IPC   │    IPC    │   IPC  │   IPC  │  (Zero-Copy)
┌──────▼──────────▼───────────▼────────▼─────────▼───────────┐
│                    JINN MICROKERNEL                        │
│  ┌──────────┐ ┌──────────┐ ┌──────────────┐ ┌───────────┐  │
│  │Scheduler │ │  Memory  │ │ Security     │ │ IPC Core  │  │
│  │(Direct   │ │  (Buddy  │ │ (C-Space &   │ │ (Regs &   │  │
│  │ Switch)  │ │  Slab)   │ │  Tokens)     │ │ Buffers)  │  │
│  └──────────┘ └──────────┘ └──────────────┘ └───────────┘  │
└────────────────────────────────────────────────────────────┘
```

👉 **Dive deep into the internals:**
- [Memory Core](docs/kernel/memory-core.md) - NUMA-aware, Predictive Pools, capability-mediated.
- [Security Core](docs/kernel/security-core.md) - Zero-Trust C-Spaces, strict attenuation, cascading revocation.
- [IPC Core](docs/kernel/ipc-core.md) - Direct hand-offs, Lock-free queues, Zero-copy shared pages.
- [Scheduler Core](docs/kernel/scheduler-core.md) - Lock-free queues, predictable latency.

---

## 🚀 Status: Phase 1 Complete

We have just successfully completed **Phase 1** of our roadmap, heavily documenting and specifying the entire core architecture. We are now entering **Phase 2 (Implementation of Basic Services)**.

| Feature | Status |
|---------|--------|
| Limine Bootloader & HHDM | ✅ Done |
| Physical Memory (Buddy/Bitmap) | ✅ Done |
| Preemptive APIC Scheduler | ✅ Done |
| Lock-free IPC Core Spec | ✅ Spec Complete |
| Security C-Space Spec | ✅ Spec Complete |
| User-Space Driver ABI | ✅ Spec Complete |
| VFS / TCP-IP / GUI | 🚧 Planned (Phase 3+) |

---

## 🛠️ Building & Running

Jinn OS uses native PowerShell scripts for a seamless build experience on Windows, but can easily run on Linux/macOS.

### Prerequisites
- [Rust nightly](https://rustup.rs/) (Target: `x86_64-unknown-none`)
- QEMU

### Quick Start

```bash
# 1. Add required targets
rustup target add x86_64-unknown-none --toolchain nightly
rustup component add rust-src --toolchain nightly

# 2. Build the kernel
.\scripts\build.ps1

# 3. Create the ISO image
.\scripts\make_iso.ps1

# 4. Run in QEMU
qemu-system-x86_64 -cdrom jinn.iso -m 128M -serial stdio
```

---

## 🤝 Contributing

We are looking for passionate systems programmers, Rustaceans, and OS dev enthusiasts to help build the future of secure operating systems. Whether you want to write device drivers, improve the scheduler, or work on the network stack, there is a place for you!

Read our [Contribution Guide](CONTRIBUTING.md) to get started.

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
