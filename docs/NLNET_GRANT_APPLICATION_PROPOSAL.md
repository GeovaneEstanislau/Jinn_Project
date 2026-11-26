# NLnet Foundation / NGI Zero Grant Application Proposal

**Project Title:** Jinn OS & Modular Architecture: An Experimental Microkernel, Ring 3 Compositor (`Aladdin`), and Heterogeneous Compute Engine (`jhcs-rs`) for Efficient Computing  
**Target Program:** NGI Zero Core / NGI Zero Entrust (Next Generation Internet)  
**Applicant:** Geovane da Conceição Estanislau (Lead Architect & Core Developer)  
**Canonical Repositories:**  
- Microkernel & OS Testbed: https://github.com/GeovaneEstanislau/Jinn_Project  
- Out-of-Kernel Compositor: https://github.com/GeovaneEstanislau/Aladdin  
- Portable Heterogeneous Compute Broker: https://github.com/GeovaneEstanislau/jhcs-rs  
- Technical Observatory: https://jinn-os.dev.br  
**Primary License:** `GPL-2.0-only` (Jinn OS community license, with optional commercial track under `CLA.md`), `MIT OR Apache-2.0` (`Aladdin`), `Apache-2.0` (`jhcs-rs`)  
**Requested Funding:** €45,000 EUR  
**Duration:** 6–8 Months (Milestone-based delivery)  

---

## 1. General Project Information

### 1.1 Project Abstract (Elevator Pitch — max 120 words)
Modern systems infrastructure relies heavily on monolithic C/C++ kernels whose driver attack surface and baseline idle overhead push millions of devices toward premature obsolescence. **Jinn** is an experimental, memory-safe `x86_64` Rust (`#![no_std]`) microkernel architecture engineered as a verifiable testbed for efficient, isolated, and heterogeneous computing. Alongside the OS, its modular building blocks—**Zero-Idle event-driven SMP scheduling**, **JAHA 1.2 deterministic memory accounting**, **Aladdin** out-of-kernel Ring 3 compositor (`AladdinSharedRings`), **`HotRouteTable` + 3-layer sovereign DNS/TLS 1.3 networking**, and the portable **`jhcs-rs`** compute broker—are engineered for independent verification, low-resource **64 MiB–256 MiB** execution, and digital sovereignty.

---

## 2. General Description of the Project

### 2.1 Problem Statement & Status Quo
The foundational software layer of the internet—the operating system and browser—faces three critical crises:

1. **Systemic Memory Safety Vulnerabilities:** Over 70% of high-severity CVEs in traditional kernels (Linux, Windows) originate from memory safety bugs (buffer overflows, use-after-free, double frees) inherent to C/C++. Monolithic kernels run hundreds of complex third-party hardware drivers directly in Ring 0, where a single driver flaw compromises the entire machine.
2. **Hyper-Bloated Resource Footprints and E-Waste:** Contemporary operating systems (Windows 11, mainstream Linux desktop distributions) and browser engines (Chromium, WebKit) require gigabytes of RAM and multi-core CPUs merely to render an idle desktop. This planned and structural obsolescence discards millions of functional computing devices annually, exacerbating electronic waste and disenfranchising lower-income communities and schools globally.
3. **Monopolistic Web Monoculture:** The web browsing engine market has coalesced around a Blink/WebKit monoculture. Independent, privacy-respecting, lightweight engines that can operate securely without multi-gigabyte runtimes or complex multi-process telemetry are virtually non-existent.

### 2.2 Proposed Solution
Jinn addresses these foundational bottlenecks cleanly from bare metal while preserving modular reuse:

* **Minimalist Microkernel Architecture:** Jinn's Ring 0 kernel limits privileges to preemptive SMP per-CPU thread scheduling (`Zero-Idle` event-driven `HLT` gating), 4-level x86_64 PML4 `W^X` paging, `IrqSafeSpinLock` synchronization, unconditional PIC 8259A EOI signaling, and static `.bss` IPC mailboxes.
* **Complete Ring 3 Driver & Service Isolation:** Hardware drivers (PS/2, ATA/IDE, PCI, Intel E1000, VirtIO-Net), VFS layers (FAT32/Ext2 with `.TMP -> .BAK` transactional safety), the 3-layer DNS resolver + `HotRouteTable`, and the **TLS 1.3** (RFC 8446) / browser stack execute as unprivileged Ring 3 processes.
* **Aladdin & `AladdinSharedRings`:** An out-of-kernel compositor operating via lock-free zero-copy SPSC shared memory rings (`sdk/aladdin-sdk/`), dirty rectangle compositing, and CPU SIMD frosted glass, validated even within **64 MiB physical RAM** constraints on QEMU (`-m 64M`).
* **JHCS-RS (`jhcs-rs`):** A standalone Rust heterogeneous compute broker (portable across Linux, Windows, and Jinn Ring 3) that evaluates DAG workloads and selects Scalar CPU, SIMD (`AVX2`/`FMA`), or `wgpu` GPU backends using empirical breakeven cost models.

---

## 3. Comparison with the State of the Art

| Feature / Metric | Linux Desktop (Ubuntu/Debian) | Redox OS | seL4 | **Jinn OS & Aladdin** |
| :--- | :--- | :--- | :--- | :--- |
| **Language & Safety** | Monolithic C (Memory Unsafe) | Microkernel Rust | Microkernel C (Formally Verified) | **Microkernel Rust (`#![no_std]`)** |
| **Driver Isolation** | Ring 0 (Fatal on crash) | Ring 3 (Scheme-based) | Ring 3 (Capabilities) | **Ring 3 (Strict Syscall/IPC Sandbox)** |
| **Minimum RAM for GUI** | 2,048 MiB – 4,096 MiB | 512 MiB – 1,024 MiB | N/A (Embedded focus) | **64 MiB – 256 MiB (Validated)** |
| **Asynchronous I/O** | `io_uring` (Kernel C) | Standard syscalls | Custom IPC | **JDA Lock-Free DAG / Ring Queues** |
| **Native TLS 1.3 Stack** | OpenSSL/GnuTLS (Large C) | Rustls / Ported POSIX | Third-party / C | **Native Userspace RFC 8446 Stack** |
| **E-Waste & Edge Focus** | Low (Deprecates legacy) | Moderate | Industrial / Aerospace | **Primary Architectural Invariant** |

* **Contrast with Redox OS:** While Redox has demonstrated the viability of Rust microkernels, it targets modern hardware profiles with standard POSIX abstractions. Jinn is specifically engineered for deterministic memory ceilings, running its compositor, VFS, and networking stack within micro-allocator bounds (< 64 MiB).
* **Contrast with seL4:** seL4 provides mathematical formal verification for its C microkernel but imposes an exceptionally steep capability-programming barrier, lacks a native lightweight desktop/browser stack, and is rarely deployed as a sovereign personal workstation OS.

---

## 4. Technical Feasibility & Current Implementation Status

Jinn OS is not a theoretical proposal; it has an existing, functioning codebase with audited engineering milestones:

1. **Bootloader & Memory Management:** Boots via the modern Limine Boot Protocol v2 (higher-half x86_64). Features a validated Buddy Allocator, kernel heap, and PML4 page frame management.
2. **Userspace Cryptographic Stack:** Independently audited implementation of **TLS 1.3 Key Schedule (RFC 8446 / RFC 5869)**, HKDF-Extract/Expand, ChaCha20-Poly1305, SHA-256, and X.509 ASN.1 DER parser running in Ring 3.
3. **64 MiB Hardened Desktop Milestone:** Aladdin Desktop compositor (`aladdin.elf`) and window manager verified running stably on QEMU under strict `-m 64M` constraints with zero page leaks, deterministic memory accounting, and zero memory quota violations (`EDQUOT`).
4. **Hardware Driver Subsystems:** Userspace PS/2 mouse and keyboard drivers, ATA/IDE block device driver, and FAT32/Ext2 VFS mounting engines.

---

## 5. Work Plan: Milestones, Deliverables & Budget Breakdown

Total Budget Requested: **€45,000 EUR**  
Execution Timeline: **8 Months**

```mermaid
flowchart TD
    M1["Milestone 1 (€12k): Network & TLS 1.3"] --> M2["Milestone 2 (€11k): Bare-Metal & Modern Drivers"]
    M2 --> M3["Milestone 3 (€12k): Aladdin & JBE Web Engine"]
    M3 --> M4["Milestone 4 (€10k): CI/CD, Fuzzing & Public Docs"]
```

### Milestone 1: Userspace Network Stack Hardening & VirtIO-Net Driver (€12,000)
* **Objective:** Expand the existing Ring 3 network stack into a resilient, production-ready subsystem.
* **Deliverables:**
  1. Implementation of a native VirtIO-Net PCI driver running in Ring 3 for accelerated virtualization and cloud edge environments.
  2. Integration of the audited RFC 8446 TLS 1.3 handshake with full bi-directional record encryption over native TCP/IP sockets.
  3. Memory-bounded socket buffer manager guaranteeing zero heap exhaustion under high-throughput adversarial traffic.
  4. Test suite with automated PCAP regression capture validating TCP flow control and packet retransmission.
* **Acceptance Criteria:** Successful automated fetching of static HTTPS web assets via TLS 1.3 in QEMU with full network capture logs published.

### Milestone 2: Bare-Metal UEFI Bootloader & Modern Block Storage (€11,000)
* **Objective:** Transition Jinn from virtualized environments (QEMU/Bochs) to booting seamlessly on physical x86_64 PC hardware.
* **Deliverables:**
  1. Implementation of native 64-bit UEFI boot sequence using Limine UEFI-CD and bare-metal handoff without legacy BIOS dependencies.
  2. Native AHCI (SATA) and introductory NVMe block driver implementation in userspace Ring 3.
  3. APIC / IO-APIC interrupt controller initialization, replacing legacy 8259 PIC for multi-core scalability.
  4. Hardware compatibility test matrix covering standard second-life laptops (ThinkPad T4xx/X2xx series, Intel Atom/Celeron edge boards).
* **Acceptance Criteria:** Bootable hybrid ISO tested and verified on at least three distinct physical bare-metal hardware platforms with logged kernel diagnostics.

### Milestone 3: Aladdin Desktop & Jinn Browser Engine (JBE) Stabilization (€12,000)
* **Objective:** Deliver a practical, everyday user experience for low-resource environments.
* **Deliverables:**
  1. Stabilization of the Aladdin Window Manager (multi-window focus, keyboard navigation, clipboard IPC service).
  2. Hardening of JBE (Jinn Browser Engine): compliant HTTP/1.1 client, incremental HTML/CSS tokenizer, and linear frame text/image renderer running in userspace.
  3. Implementation of a persistent user configuration and file manager applet operating over FAT32/Ext2 partitions.
  4. End-to-end memory stress validation maintaining < 128 MiB RAM utilization with Aladdin Desktop, Terminal, and Browser running simultaneously.
* **Acceptance Criteria:** Demonstration of Jinn OS booting on a 128 MiB system, opening the Aladdin file manager, and browsing an informational web page via HTTPS.

### Milestone 4: Continuous Integration, Fuzzing & Public Community Documentation (€10,000)
* **Objective:** Guarantee long-term software supply chain integrity, third-party auditability, and ease of adoption.
* **Deliverables:**
  1. Automated GitHub Actions CI/CD pipeline compiling the `#![no_std]` Rust microkernel, building ISO artifacts, and executing headless QEMU integration tests on every commit.
  2. Syzkaller/AFL-based syscall fuzzing harness targeting the Jinn Syscall ABI (Syscalls 0..52) and IPC endpoints.
  3. Comprehensive English technical documentation: Architecture Reference, Driver SDK Guide, Syscall Specification, and Step-by-Step Self-Hosting Installation Manual.
  4. Publicly accessible live binary ISO images for community testing and academic evaluation.
* **Acceptance Criteria:** 100% automated CI test pass rate, fuzzing report proving zero kernel panics across 100,000 synthetic syscall mutations, and complete documentation published online.

---

## 6. Relevance to Digital Sovereignty, Sustainability & Society

* **Combating Planned Obsolescence:** Millions of computers discarded due to arbitrary OS requirements (e.g., Windows 11 TPM and CPU generation gates) can run Jinn OS smoothly, extending computer lifespans by 10+ years and significantly curbing global electronic waste.
* **Digital Inclusion in Developing Nations:** In regions where modern hardware is economically inaccessible, a secure, memory-safe OS capable of browsing documentation and executing software on legacy machines bridges the digital divide.
* **Sovereign Infrastructure:** Public entities, academic laboratories, and privacy-conscious users gain a verifiable, microkernel alternative free from telemetry, proprietary blob dependencies in Ring 0, and Big Tech lock-in.

