# Jinn OS — Minimalistischer präemptiver x86_64-Mikrokernel in Rust

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

> **Status:** Aktive Entwicklung (Alpha v0.0.3)  
> **Architektur:** `x86_64` | **Programmiersprache:** Rust (`#![no_std]`, Bare-Metal, Target `x86_64-unknown-none`)  
> **Bootloader:** Limine Boot Protocol v2 (Higher-Half)  
> **Community:** Entwickelt in Brasilien mit Fokus auf digitale Souveränität, Defense-in-Depth-Sicherheit und radikale Schlichtheit.

---

## Über das Projekt

**Jinn OS** ist ein von Grund auf in modernem Rust geschriebenes Mikrokernel-Betriebssystem. Das Projekt entstand aus der Notwendigkeit, ein zuverlässiges, minimalistisches und formal verifizierbares Softwarefundament zu schaffen, bei dem ausschließlich die unentbehrlichen Basismechanismen auf der höchsten Privilegienebene des Prozessors (**Ring 0**) verbleiben.

Im Gegensatz zu herkömmlichen monolithischen Kerneln werden in Jinn OS alle komplexen Subsysteme — darunter Netzwerktreiber, Dateisysteme, der Fensterkompositor **Aladdin Desktop** sowie Anwendungen wie der **Jinn Browser** — als isolierte Prozesse im Benutzerraum (**Ring 3**) ausgeführt. Die Integrität des Gesamtsystems wird durch hardwarebasierte Speicherpaginierung, strikte Berechtigungsprüfungen und hochperformante, lock-freie Interprozesskommunikation (IPC) gewährleistet.

---

## Architektursäulen und Meilensteine (Alpha v0.0.3)

1. **Echte Präemption und Multitasking-Scheduling:**
   - Präemptiver Round-Robin-Scheduler, getaktet über den Hardware-PIT-Timer (100 Hz / 10ms Zeitscheibe) mit Unterstützung für den lokalen APIC-Timer.
   - Entkoppelte Interrupt-Verarbeitung (IRQ) und explizite LAPIC End-Of-Interrupt-Signalisierung (EOI) zur Vermeidung von Interrupt-Bus-Blockaden.

2. **Strikte Speicherisolation (x86_64 PML4 & W^X-Sicherheit):**
   - Vierstufige Paginierung (PML4) mit rigoroser Verwaltung des kanonischen Adressraums.
   - Benutzerraumprozesse (Ring 3) residieren in sicheren Adressbereichen (`>= 0x400000`) ohne direkten Zugriff auf physischen Speicher oder Ring-0-Kontrollstrukturen.
   - Konsequente Durchsetzung der W^X-Sicherheitsrichtlinie (*Write XOR Execute*) auf allen dynamisch allokierten Speicherseiten.

3. **Minimalitätsprinzip und kanonische Syscall-Schnittstelle:**
   - Ring 0 ist strikt auf Kontextwechsel, physische Frame-Allokation, Interrupt-Verteilung und die kanonische Systemaufrufschnittstelle beschränkt (Syscalls `0..85` über `syscall`/`sysret` mit abwärtskompatiblem `int 0x80`-Trap).
   - Der Mikrokernel speichert, verarbeitet oder berührt niemals private kryptografische Schlüssel oder mnemonische Seeds.

4. **Lock-freie Interprozesskommunikation (Lock-Free IPC):**
   - Statische, nachrichtenbasierte Mailbox pro Prozess ohne dynamische Heap-Allokationen im zeitkritischen Ausführungspfad.

5. **Virtuelles Dateisystem (VFS) mit transaktionaler Persistenz:**
   - Gleichzeitige native Unterstützung für **FAT32** (`/disk`) und **Ext2**.
   - Dediziertes **BlockCache**-Subsystem mit Least-Recently-Used-Verdrängung (LRU) zur Beschleunigung von Festplatten-I/O.
   - Vollständige POSIX-Dateizugriffsflags (`O_RDONLY`, `O_WRONLY`, `O_RDWR`, `O_CREAT`, `O_APPEND`, `O_TRUNC`).
   - Transaktionale Dateirotationssicherung (`.TMP` -> `.BAK`), die Browser-Caches, Lesezeichen, Verlauf und Cookies bei abrupten Stromausfällen vor Datenkorruption schützt.

6. **Grafische Aladdin-Desktop-Umgebung in Ring 3:**
   - Eigenständiger Display-Server und Fensterkompositor, der vollständig im Benutzerraum operiert (`apps/aladdin-server`).
   - Hochauflösendes Rendern direkt in den von Limine bereitgestellten Linear Framebuffer.
   - PS/2-Maustreiber mit entkoppelter Paketverarbeitung, flüssigem Hardware-Cursor-Tracking und Fensterereignissteuerung (`Focus`, `Drag`, `Resize`, `Close`).

7. **Souveräner Netzwerk-Stack, JinnWall SPI & PKI-Kryptografie:**
   - Bare-Metal-Netzwerk-Stack für IPv4, TCP, UDP, ARP, DHCP und DNS mit Unterstützung für VirtIO-Net- und Intel E1000-Karten.
   - Stateful-Packet-Inspection-Firewall (**JinnWall SPI**).
   - Native **PKI X.509**-Zertifikatskettenvalidierung und sicherer **TLS 1.3**-Handshake (RFC 8446) mit HKDF-SHA256/384-Schlüsselableitung.

8. **Native Ring-3-Anwendungen:**
   - **Jinn Browser:** C-bibliotheksfreier nativer Webbrowser mit HTML/CSS-Layout-Parser, HTTP-Client, lokalem Cache, Lesezeichen und transaktional persistiertem Verlauf unter `/disk/browser/`.
   - **Aladdin Shell:** Interaktives Terminal mit Kernel-Diagnosebefehlen (`sysinfo`, `vfs`, `net`, `ps`, `mem`).
   - **Aladdin Wallet:** Non-Custodial Web3-Wallet mit deterministischer Schlüsselgenerierung nach BIP-39/BIP-44, vollständig isoliert in Ring 3.

---

## Verzeichnisstruktur

```text
jinn/
├── Cargo.toml            # Manifest des Root-Workspace der Mikrokernel-Crate
├── limine.conf           # Konfigurationsdatei des Limine v2 Bootloaders
├── build.rs              # Build- und Linking-Skript des Kernels
├── apps/                 # Native User-Space-Anwendungen und -Dienste (Ring 3)
│   ├── aladdin-server/   # Aladdin Desktop Window Server (Ring 3 Compositor)
│   └── ola-mundo/        # Jinn Browser (browser.rs) und Aladdin Wallet (wallet.rs)
├── sdk/                  # Aladdin SDK und Client-Bibliotheken für Ring 3
│   └── aladdin-sdk/      # Wrapper für Syscalls, Grafikoberflächen, Events und IPC
├── contracts/            # BEP-20 Smart Contracts und Hardhat-Testsuite
│   ├── contracts/        # JinnToken.sol, JinnVesting.sol, JinnRewardDistributor.sol
│   ├── test/             # Unit-Tests und EVM-Sicherheitsinvarianten
│   └── scripts/          # Deployment-Pipelines für BSC Testnet/Mainnet
├── tools/                # Entwicklungswerkzeuge und Simulations-Tools
│   ├── jinn-web3/        # jinn-web3 (CLI) und jinn-web3d (Ring-3-Daemon)
│   └── jda-sim/          # DAG-Pipeline-Simulator und SMP-Benchmarks
├── web/                  # Offizielles Webportal für Jinn OS (jinn-os.dev.br / GitHub Pages)
├── docs/                 # Technische Spezifikationen, ADRs und Audit-Berichte
└── src/                  # Quellcode des Mikrokernels (Ring 0)
    ├── main.rs           # Kernel-Einstiegspunkt (_start) und Initialisierung
    ├── gdt.rs            # Global Descriptor Table und TSS (Ring 0 / Ring 3)
    ├── idt.rs            # Interrupt Descriptor Table und ISR/IRQ-Handler
    ├── syscall.rs        # Kanonischer Syscall-Dispatcher (Syscalls 0..85)
    ├── scheduler.rs      # Präemptiver Multitasking-Scheduler
    ├── ipc.rs            # Statisches, sperrenfreies Nachrichtenübertragungssystem
    ├── memory/           # Physischer Frame-Allokator (Buddy), Kernel-Heap und PML4
    ├── drivers/          # Gerätetreiber (PIT, LAPIC, Tastatur, PS/2-Maus, IDE, PCI, VirtIO, E1000)
    ├── net/              # TCP/IP-Stack, JinnWall-Firewall, TLS 1.3 und X.509-PKI
    └── vfs/              # Virtuelles Dateisystem (FAT32, Ext2, LRU BlockCache)
```

---

## Kompilierung und Ausführung

### Voraussetzungen
- **Rust Toolchain:** `nightly`-Compiler mit installiertem Target `x86_64-unknown-none`.
- **Emulation & Werkzeuge:** `qemu-system-x86_64`, PowerShell 7+ (Windows/Linux) oder Python 3.

```bash
# Bare-Metal Target für x86_64 hinzufügen
rustup target add x86_64-unknown-none
```

### 1. Mikrokernel und Ring-3-Anwendungen kompilieren
Führen Sie das integrierte Build-Skript aus, um den Kernel und die User-Space-Programme zu kompilieren sowie das startfähige FAT32-Image zu erzeugen:

```powershell
# Im Stammverzeichnis des Repositories
.\scripts\build.ps1
```

### 2. In QEMU starten
Starten Sie die virtuelle Maschine zur Hardware-Emulation:

```powershell
# Standardstart mit Aladdin Desktop und virtuellem Netzwerk
.\scripts\run.ps1

# Stresstest unter strengen Speicherbeschränkungen (z. B. 64MB RAM Limit):
.\scripts\run.ps1 -Memory 64M
```

---

## Web3-Ökosystem & Jinn Open Economy ($JINN)

Zur Sicherung der langfristigen wirtschaftlichen Souveränität, zur Finanzierung von Open-Source-Systemforschung und zur Vergütung von Mitwirkenden über *Proof of Useful Contribution* (PoUC) betreibt das Projekt die Infrastruktur des systemeigenen Tokens **$JINN** auf der BNB Smart Chain (BSC).

> [!IMPORTANT]
> **Strikte Trennung kryptografischer Schlüssel:** Der Jinn-Mikrokernel (Ring 0) greift niemals auf private Schlüssel, mnemonische Phrasen oder Wallet-Daten zu. Sämtliche kryptografische Berechnungen sind vollständig im Benutzerraum (Ring 3) innerhalb der Anwendung **Aladdin Wallet** gekapselt.

- **Wirtschaftsvorschlag & Whitepaper:** [`JINN_TOKEN_PROPOSAL.md`](JINN_TOKEN_PROPOSAL.md)
- **EVM-Validierungs- und Phase-II-Auditbericht:** [`docs/JINN_WEB3_PHASE2_AUDIT_REPORT.md`](docs/JINN_WEB3_PHASE2_AUDIT_REPORT.md)
- **Multi-Sig-Governance & Treasury-Spezifikation:** [`docs/JINN_OPEN_ECONOMY_GOVERNANCE.md`](docs/JINN_OPEN_ECONOMY_GOVERNANCE.md)
- **PoUC-Spezifikation (Bounties):** [`docs/PROOF_OF_USEFUL_CONTRIBUTION_SPEC.md`](docs/PROOF_OF_USEFUL_CONTRIBUTION_SPEC.md)
- **Architektur der Wallet (Ring 3):** [`docs/JINN_WALLET_ARCHITECTURE.md`](docs/JINN_WALLET_ARCHITECTURE.md)
- **Aladdin Wallet Quellcode:** [`apps/ola-mundo/src/wallet.rs`](apps/ola-mundo/src/wallet.rs)
- **User-Space-Daemon `jinn-web3d`:** [`tools/jinn-web3/src/daemon.rs`](tools/jinn-web3/src/daemon.rs)
- **DePIN Rechenökonomie (JDA):** [`docs/JDA_DEPIN_COMPUTATIONAL_ECONOMY.md`](docs/JDA_DEPIN_COMPUTATIONAL_ECONOMY.md)
- **Interner Smart-Contract-Audit:** [`docs/SMART_CONTRACTS_AUDIT_REPORT.md`](docs/SMART_CONTRACTS_AUDIT_REPORT.md)
- **Testnet-Leitfaden:** [`docs/TESTNET_DEPLOYMENT_GUIDE.md`](docs/TESTNET_DEPLOYMENT_GUIDE.md)

---

## Lizenzierung und Governance

**Jinn OS** wird unter einem **dualen Lizenzmodell (Dual Licensing)** bereitgestellt:

1. **Offene Community-Lizenz (`GPL-2.0-only`):** Vorgesehen für die Open-Source-Community, Universitäten und Entwickler. Die Nutzung und Weiterentwicklung unterliegen den Bedingungen der GNU General Public License Version 2 ([`LICENSES/GPL-2.0-only.txt`](LICENSES/GPL-2.0-only.txt)).
2. **Proprietäre kommerzielle Lizenz (`Jinn-Commercial-1.0`):** Für industrielle Einsätze, eingebettete Systeme und kommerzielle Appliances, die eine Freistellung von den Copyleft-Klauseln der GPL erfordern ([`LICENSES/Jinn-Commercial-1.0.txt`](LICENSES/Jinn-Commercial-1.0.txt)).

Vollständige rechtliche Bedingungen entnehmen Sie bitte [`LICENSE`](LICENSE). Beiträge zum Quellcode erfordern die Unterzeichnung des [Contributor License Agreement (`CLA.md`)](CLA.md) sowie die Einhaltung der [Beitragsrichtlinien (`CONTRIBUTING.md`)](CONTRIBUTING.md).

---

## Kontakt und Mitarbeit

Wir freuen uns über das Engagement der internationalen Entwicklergemeinschaft, von Rustaceans und Low-Level-Ingenieuren. Ob durch Kernel-Optimierungen, Treiberentwicklung, Sicherheitsaudits oder Dokumentation — jede Unterstützung ist willkommen! Bitte konsultieren Sie [CONTRIBUTING.md](CONTRIBUTING.md), um zu erfahren, wie Sie Patches und Pull Requests einreichen können.
