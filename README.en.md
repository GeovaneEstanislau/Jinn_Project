# Jinn OS — Minimalist Preemptive x86_64 Microkernel in Rust

<div align="center">

| Language | Status | File | Language | Status | File |
| :--- | :---: | :---: | :--- | :---: | :---: |
| 🇧🇷 **Português** | 🟢 Concluído | [README.md](README.md) | 🇮🇳 **Hindi (हिन्दी)** | 🟢 Concluído | [README.hi.md](README.hi.md) |
| 🇺🇸 **English** | 🟢 Concluído | [README.en.md](README.en.md) | 🇮🇹 **Italiano** | 🟢 Concluído | [README.it.md](README.it.md) |
| 🇨🇳 **中文 (Chinese)** | 🟢 Concluído | [README.zh.md](README.zh.md) | 🇭🇺 **Magyar (Hungarian)**| 🟢 Concluído | [README.hu.md](README.hu.md) |
| 🇯🇵 **日本語 (Japanese)** | 🟢 Concluído | [README.ja.md](README.ja.md) | 🇭🇷 **Hrvatski (Croatian)** | 🟢 Concluído | [README.hr.md](README.hr.md) |
| 🇪🇸 **Español** | 🟢 Concluído | [README.es.md](README.es.md) | 🇳🇴 **Norsk (Norwegian)** | 🟢 Concluído | [README.no.md](README.no.md) |
| 🇷🇺 **Русский (Russian)**| 🟢 Concluído | [README.ru.md](README.ru.md) | 🇩🇪 **Deutsch (German)** | 🟢 Concluído | [README.de.md](README.de.md) |

</div>

> **Status:** Active Development (Alpha v0.0.3)  
> **Architecture:** `x86_64` | **Language:** Rust (`#![no_std]`, bare-metal, target `x86_64-unknown-none`)  
> **Bootloader:** Limine Boot Protocol v2 (Higher-Half)  
> **Community:** Developed in Brazil with a commitment to computational sovereignty, defense-in-depth security, and radical simplicity.

---

## About the Project

**Jinn OS** is a clean-slate microkernel operating system written entirely in modern Rust. The project was born from the necessity of establishing a reliable, verifiable, and strictly minimalist computing foundation, where only the bare essentials reside in the processor's most privileged execution mode (**Ring 0**).

Unlike conventional monolithic operating systems, all complex subsystems in Jinn OS — including network device drivers, file systems, the **Aladdin Desktop** window server, and applications such as the **Jinn Browser** — run as isolated processes in user space (**Ring 3**). System stability and process boundaries are enforced through hardware-level memory paging, strict capability checks, and high-performance, lock-free inter-process communication (IPC).

---

## Architectural Pillars and Milestones (Alpha v0.0.3)

1. **True Preemption & Multitasking Scheduling:**
   - Round-Robin preemptive scheduler driven by the hardware PIT Timer (100 Hz / 10ms quantum) with local APIC Timer integration.
   - Decoupled interrupt handling (IRQs) and explicit LAPIC End-Of-Interrupt (EOI) signaling to prevent interrupt bus starvation.

2. **Strict Memory Isolation (x86_64 PML4 & W^X Security):**
   - 4-level page tables with strict canonical virtual address management.
   - Ring 3 processes reside in user-space addresses (`>= 0x400000`), with zero access to physical memory or Ring 0 control data.
   - W^X (*Write XOR Execute*) enforcement across all dynamically allocated pages.

3. **Least Privilege & Canonical Syscall Interface:**
   - Ring 0 is restricted to context switching, physical page frame allocation, hardware interrupt dispatching, and a canonical system call interface (Syscalls `0..85` via `syscall`/`sysret` with legacy `int 0x80` fallback).
   - The microkernel never handles, stores, or processes cryptographic private keys or financial credentials.

4. **Lock-Free Message Passing (IPC):**
   - Static, bounded, per-process IPC mailboxes with zero heap allocations on the critical message dispatch path.

5. **Virtual File System (VFS) with Transactional Persistence:**
   - Concurrent native support for **FAT32** (`/disk`) and **Ext2**.
   - Dedicated **BlockCache** subsystem with Least Recently Used (LRU) eviction for high-throughput disk I/O.
   - Standard POSIX access flags (`O_RDONLY`, `O_WRONLY`, `O_RDWR`, `O_CREAT`, `O_APPEND`, `O_TRUNC`).
   - Transactional rotational integrity (`.TMP` -> `.BAK`) preventing corruption of browser caches, cookies, bookmarks, and configuration files during abrupt power-off events.

6. **Aladdin Desktop Graphical Environment in Ring 3:**
   - Independent window compositor and display server running completely in user space (`apps/aladdin-server`).
   - High-resolution Linear Framebuffer rendering provided by Limine.
   - PS/2 mouse driver with decoupled packet ingestion, real-time cursor tracking, and complete window management (`Focus`, `Drag`, `Resize`, `Close`).

7. **Sovereign Network Stack, JinnWall SPI & PKI Cryptography:**
   - Bare-metal IPv4, TCP, UDP, ARP, DHCP, and DNS network stack supporting VirtIO-Net and Intel E1000 NICs.
   - Stateful Packet Inspection firewall (**JinnWall SPI**).
   - Native **PKI X.509** certificate validation and secure **TLS 1.3** Handshake (RFC 8446) with HKDF-SHA256/384 cryptographic key schedules.

8. **Native Ring 3 Applications:**
   - **Jinn Browser:** Native graphical web browser engine with zero C dependencies, featuring an HTML/CSS layout parser, HTTP client, local cache, bookmarks, and persistent history stored under `/disk/browser/`.
   - **Aladdin Shell:** Interactive terminal with diagnostic utilities (`sysinfo`, `vfs`, `net`, `ps`, `mem`).
   - **Aladdin Wallet:** Non-custodial Web3 wallet featuring deterministic BIP-39/BIP-44 mnemonic key derivation, completely isolated in Ring 3.

---

## Repository Structure

```text
jinn/
├── Cargo.toml            # Root workspace manifest for the microkernel crate
├── limine.conf           # Limine v2 bootloader configuration
├── build.rs              # Kernel build and linking script
├── apps/                 # Native user-space applications and services (Ring 3)
│   ├── aladdin-server/   # Aladdin Desktop window server (Ring 3 compositor)
│   └── ola-mundo/        # Jinn Browser (browser.rs) and Aladdin Wallet (wallet.rs)
├── sdk/                  # Aladdin SDK and client libraries for Ring 3
│   └── aladdin-sdk/      # Wrappers for Syscalls, graphics surfaces, events, and IPC
├── contracts/            # BEP-20 smart contracts and Hardhat testing suite
│   ├── contracts/        # JinnToken.sol, JinnVesting.sol, JinnRewardDistributor.sol
│   ├── test/             # Unit tests and formal security invariants on the EVM
│   └── scripts/          # Deployment pipelines for BSC Testnet/Mainnet
├── tools/                # Development utilities and simulation harnesses
│   ├── jinn-web3/        # jinn-web3 (CLI) and jinn-web3d (Ring 3 daemon)
│   └── jda-sim/          # DAG pipeline simulator and SMP benchmarks
├── web/                  # Official Jinn OS web portal (jinn-os.dev.br / GitHub Pages)
├── docs/                 # Technical specifications, ADRs, and audit reports
└── src/                  # Core microkernel source code (Ring 0)
    ├── main.rs           # Kernel entry point (_start) and subsystem bootstrapping
    ├── gdt.rs            # Global Descriptor Table and Task State Segment (Ring 0 / Ring 3)
    ├── idt.rs            # Interrupt Descriptor Table and ISR/IRQ handlers
    ├── syscall.rs        # Canonical system call dispatcher (Syscalls 0..85)
    ├── scheduler.rs      # Preemptive multitasking scheduler
    ├── ipc.rs            # Static lock-free message passing engine
    ├── memory/           # Physical frame allocator (Buddy), Kernel Heap, and PML4
    ├── drivers/          # Drivers (PIT, LAPIC, Keyboard, PS/2 Mouse, IDE, PCI, VirtIO, E1000)
    ├── net/              # TCP/IP stack, JinnWall SPI, TLS 1.3 engine, and X.509 PKI
    └── vfs/              # Virtual File System (FAT32, Ext2, LRU BlockCache)
```

---

## Build and Run

### Prerequisites
- **Rust Toolchain:** `nightly` compiler with the `x86_64-unknown-none` target installed.
- **Emulation & Tooling:** `qemu-system-x86_64`, PowerShell 7+ (Windows/Linux) or Python 3.

```bash
# Add the bare-metal x86_64 target
rustup target add x86_64-unknown-none
```

### 1. Build the Microkernel and User Space Binaries
Run the automated build script to compile the kernel, build the Ring 3 applications, and generate the bootable FAT32 disk image:

```powershell
# From the repository root
.\scripts\build.ps1
```

### 2. Boot in QEMU
Launch the virtual machine with hardware emulation and peripheral support:

```powershell
# Standard execution with Aladdin Desktop and networking
.\scripts\run.ps1

# Homologation under strict memory limits (e.g., 64MB RAM stress testing):
.\scripts\run.ps1 -Memory 64M
```

---

## Web3 Ecosystem & Jinn Open Economy ($JINN)

To ensure long-term sovereign economic sustainability, fund foundational systems research, and reward open-source contributors via *Proof of Useful Contribution* (PoUC), the project maintains an integrated smart contract ecosystem powered by the native **$JINN** token on the BNB Smart Chain (BSC).

> [!IMPORTANT]
> **Inviolable Key Isolation:** The Jinn microkernel (Ring 0) never accesses, processes, or stores private keys, seed phrases, or financial data. All cryptographic operations remain strictly isolated in user space (Ring 3) within the **Aladdin Wallet** application.

- **Economic Proposal & Whitepaper:** [`JINN_TOKEN_PROPOSAL.md`](JINN_TOKEN_PROPOSAL.md)
- **EVM Validation & Phase II Audit Report:** [`docs/JINN_WEB3_PHASE2_AUDIT_REPORT.md`](docs/JINN_WEB3_PHASE2_AUDIT_REPORT.md)
- **Multi-Sig Governance & Treasury:** [`docs/JINN_OPEN_ECONOMY_GOVERNANCE.md`](docs/JINN_OPEN_ECONOMY_GOVERNANCE.md)
- **PoUC Specification (Bounties):** [`docs/PROOF_OF_USEFUL_CONTRIBUTION_SPEC.md`](docs/PROOF_OF_USEFUL_CONTRIBUTION_SPEC.md)
- **Wallet Architecture (Ring 3):** [`docs/JINN_WALLET_ARCHITECTURE.md`](docs/JINN_WALLET_ARCHITECTURE.md)
- **Aladdin Wallet (Ring 3 UI Code):** [`apps/ola-mundo/src/wallet.rs`](apps/ola-mundo/src/wallet.rs)
- **User Space Daemon `jinn-web3d`:** [`tools/jinn-web3/src/daemon.rs`](tools/jinn-web3/src/daemon.rs)
- **DePIN Computational Economy (JDA):** [`docs/JDA_DEPIN_COMPUTATIONAL_ECONOMY.md`](docs/JDA_DEPIN_COMPUTATIONAL_ECONOMY.md)
- **Smart Contracts Internal Audit:** [`docs/SMART_CONTRACTS_AUDIT_REPORT.md`](docs/SMART_CONTRACTS_AUDIT_REPORT.md)
- **Testnet Deployment Guide:** [`docs/TESTNET_DEPLOYMENT_GUIDE.md`](docs/TESTNET_DEPLOYMENT_GUIDE.md)

---

## Licensing and Governance

**Jinn OS** is distributed under a **Dual Licensing Model**:

1. **Open / Community License (`GPL-2.0-only`):** Designed for the open-source community, academic researchers, and hobbyists. Derivative works and redistributions are freely governed under the GNU General Public License version 2 ([`LICENSES/GPL-2.0-only.txt`](LICENSES/GPL-2.0-only.txt)).
2. **Proprietary Commercial License (`Jinn-Commercial-1.0`):** Available for enterprise deployments, embedded hardware appliances, and closed-source industrial products requiring exemption from GPL copyleft provisions ([`LICENSES/Jinn-Commercial-1.0.txt`](LICENSES/Jinn-Commercial-1.0.txt)).

For full legal terms and licensing details, refer to [`LICENSE`](LICENSE). Contributing code requires signing the [Contributor License Agreement (`CLA.md`)](CLA.md) and adhering to the [Contribution Guidelines (`CONTRIBUTING.md`)](CONTRIBUTING.md).

---

## Contact and Contributions

This project welcomes contributions from the global systems programming community, Rustaceans, and kernel developers worldwide. Whether through kernel enhancements, hardware drivers, security auditing, or documentation, your support is invaluable. Please review [CONTRIBUTING.md](CONTRIBUTING.md) to get started.
