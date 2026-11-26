# Jinn OS — Experimental Microkernel Architecture for Efficient, Isolated & Heterogeneous Computing

> **PT-BR:** [README.md](README.md) | **Observatory:** [jinn-os.dev.br](https://jinn-os.dev.br)  
> **Modular Ecosystem:** [`Jinn_Project`](https://github.com/GeovaneEstanislau/Jinn_Project) · [`Aladdin`](https://github.com/GeovaneEstanislau/Aladdin) · [`jhcs-rs`](https://github.com/GeovaneEstanislau/jhcs-rs)

<div align="center">

| Language | File | Language | File |
| :--- | :---: | :--- | :---: |
| 🇧🇷 **Português** | [README.md](README.md) | 🇮🇳 **Hindi** | [README.hi.md](README.hi.md) |
| 🇺🇸 **English** | [README.en.md](README.en.md) | 🇮🇹 **Italiano** | [README.it.md](README.it.md) |
| 🇨🇳 **中文** | [README.zh.md](README.zh.md) | 🇭🇺 **Magyar** | [README.hu.md](README.hu.md) |
| 🇯🇵 **日本語** | [README.ja.md](README.ja.md) | 🇭🇷 **Hrvatski** | [README.hr.md](README.hr.md) |
| 🇪🇸 **Español** | [README.es.md](README.es.md) | 🇳🇴 **Norsk** | [README.no.md](README.no.md) |
| 🇷🇺 **Русский** | [README.ru.md](README.ru.md) | 🇩🇪 **Deutsch** | [README.de.md](README.de.md) |

</div>

> **Technical Thesis:** Experimental microkernel architecture in bare-metal Rust (`x86_64`, `#![no_std]`) serving as a verifiable testbed for **efficient**, **Ring 3 isolated**, and **heterogeneous computing**.  
> **Author:** Geovane da Conceição Estanislau | **Status:** Alpha v0.0.3 (`x86_64-unknown-none`, Limine v2)

---

## Core Thesis & Transferable Building Blocks

Rather than competing head-on as a general-purpose desktop replacement against Linux or Windows, **Jinn** positions the full operating system as a **live end-to-end verification environment**, while isolating reusable subsystems as independent engineering components:

| Component / Subsystem | Location / Repository | Role & Verifiable Evidence |
| :--- | :--- | :--- |
| **Minimalist Ring 0 Microkernel** | [`src/`](src/) | SMP per-CPU preemptive scheduling, 4-level PML4 paging with `W^X`, `IrqSafeSpinLock` for ISR-shared state, unconditional PIC 8259A EOI, static `.bss` IPC. |
| **Zero-Idle Resource Policy (`0.00%` idle CPU)** | `src/scheduler.rs`, `src/smp.rs` | Event-driven blocking via `HLT` / `wait_on` with zero background polling timers (`0.00%` idle AP utilization observed under QEMU `-smp 4`). |
| **JAHA 1.2 Memory Accounting** | `src/memory/`, [`scripts/test_jaha_12_integrity.py`](scripts/test_jaha_12_integrity.py) | Deterministic physical no-overcommit accounting validated inside `64 MiB`–`256 MiB` RAM bounds with `0 bytes discrepancy` invariants. |
| **`Aladdin` & `AladdinSharedRings`** | [`Aladdin`](https://github.com/GeovaneEstanislau/Aladdin), [`sdk/aladdin-sdk/`](sdk/aladdin-sdk/) | Out-of-kernel **Ring 3** display server/compositor communicating via lock-free, zero-copy SPSC shared rings, `DirtyRect` tracking, and CPU SIMD acrylic compositing. |
| **Sovereign Network Stack, 3-Layer DNS & `HotRouteTable`** | `src/net/` (`dns.rs`, `hot_routes.rs`, `tls13.rs`), `apps/ola-mundo/` | Three-tier DNS resolver (Hosts $\to$ lazy-TTL RAM/FAT32 cache $\to$ upstream cascade), unified L2/L3 `HotRouteTable`, JinnWall SPI, and `#![no_std]` **TLS 1.3** (RFC 8446). |
| **`jhcs-rs` Heterogeneous Compute Broker** | [`jhcs-rs`](https://github.com/GeovaneEstanislau/jhcs-rs), `apps/jhcs-daemon/` | Standalone host (Linux/Windows) and `#![no_std]` Ring 3 DAG compute scheduler selecting Scalar CPU, SIMD (`AVX2`/`FMA`), or GPU (`wgpu`) via breakeven dispatch policies. |

> [!NOTE]
> Grant alignment, architecture reports, and historical archived Web3 prototypes live under [`docs/`](docs/) (`docs/NLNET_GRANT_APPLICATION_PROPOSAL.md`, [`docs/archive/`](docs/archive/README.md)).

---

## Repository Layout

```text
Jinn_Project/
├── src/                      # Minimal Ring 0 microkernel (#![no_std], x86_64 Rust)
│   ├── memory/               # Buddy frame allocator, kernel heap & PML4 W^X (JAHA 1.2)
│   ├── interrupts/           # IDT, 8259A PIC (unconditional EOI) & LAPIC
│   ├── scheduler.rs, smp.rs  # Per-CPU preemptive scheduler enforcing Zero-Idle HLT gating
│   ├── net/                  # IPv4/TCP/UDP, 3-tier dns.rs, hot_routes.rs, tls13.rs, jinnwall
│   ├── vfs/                  # FAT32 (/disk, LFN, .TMP->.BAK transactional writes) & Ext2
│   └── syscall.rs            # Canonical syscall ABI (syscall/sysret + int 0x80)
├── apps/
│   ├── aladdin-server/       # Ring 3 graphics display service wrapping Aladdin
│   ├── ola-mundo/            # Jinn Browser (TLS 1.3, HTML/CSS, RSS/JSON, images) & terminal
│   └── jhcs-daemon/          # Ring 3 heterogeneous compute daemon
├── sdk/aladdin-sdk/          # AladdinSharedRings (SPSC zero-copy) & syscall wrappers
├── scripts/                  # Automated build (build.ps1, run.ps1) & QEMU E2E test suites
├── web/                      # Jinn Engineering Observatory (jinn-os.dev.br) & feeds
└── docs/                     # Engineering specs, audits & archived historical documents
```

---

## Build & Reproducible Verification

### Prerequisites
- Rust `nightly` toolchain with `rustup target add x86_64-unknown-none`
- `qemu-system-x86_64`, PowerShell 7+, and Python 3

```powershell
# 1. Build kernel, Ring 3 binaries (including Aladdin), FAT32 disk image, and bootable ISO
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\run.ps1 -NoRun

# 2. Boot Jinn OS under QEMU (4 cores, Intel E1000 NIC)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\run.ps1 -Smp 4

# 3. Run automated verification suites
python scripts\test_hot_routes_e2e.py
python scripts\test_dns_cache_e2e.py
python scripts\test_jaha_12_integrity.py
```

---

## Licensing & Governance

**`Jinn_Project`** adopts dual licensing ([`LICENSE`](LICENSE)):
1. **Open Source / Academic Track:** `GPL-2.0-only` ([`LICENSES/GPL-2.0-only.txt`](LICENSES/GPL-2.0-only.txt)).
2. **Commercial / OEM Track:** `Jinn-Commercial-1.0` ([`LICENSES/Jinn-Commercial-1.0.txt`](LICENSES/Jinn-Commercial-1.0.txt)), paired with [`CLA.md`](CLA.md) for external contributions.

Companion repositories are permissively licensed for independent integration:
- **`Aladdin`**: `MIT OR Apache-2.0`
- **`jhcs-rs`**: `Apache-2.0` (see [`jhcs-rs/docs/LICENSING_POLICY.md`](../jhcs-rs/docs/LICENSING_POLICY.md))

See [`CONTRIBUTING.md`](CONTRIBUTING.md) before submitting patches or hardware test reports.
