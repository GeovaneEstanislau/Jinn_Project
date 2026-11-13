# Jinn OS — Microkernel x86_64 Minimalista e Preemptive in Rust

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

> **Stato:** In sviluppo attivo (Alpha v0.0.3)  
> **Architettura:** `x86_64` | **Linguaggio:** Rust (`#![no_std]`, bare-metal, target `x86_64-unknown-none`)  
> **Bootloader:** Limine Boot Protocol v2 (Higher-Half)  
> **Comunità:** Sviluppato in Brasile con un impegno per la sovranità computazionale, la sicurezza approfondita e la semplicità radicale.

---

## Informazioni sul Progetto

**Jinn OS** è un sistema operativo microkernel sviluppato interamente da zero in Rust moderno. Il progetto nasce dalla necessità di stabilire una base di calcolo affidabile, minimalista e formalmente verificabile, in cui solo i meccanismi rigorosamente indispensabili risiedono nel livello più privilegiato del processore (**Ring 0**).

A differenza dei sistemi operativi monolitici convenzionali, in Jinn OS tutti i sottosistemi complessi — inclusi i driver di rete, i file system, il server grafico **Aladdin Desktop** e le applicazioni applicative come il **Jinn Browser** — vengono eseguiti come processi isolati nello spazio utente (**Ring 3**). L'integrità del sistema e i confini tra processi sono garantiti dalla paginazione della memoria a livello hardware, da rigorosi controlli di capacità e da una comunicazione interprocesso (IPC) lock-free ad altissima velocità.

---

## Pilastri Architetturali e Traguardi (Alpha v0.0.3)

1. **Preemption Reale e Scheduling Multitasking:**
   - Scheduler preemptive Round-Robin sincronizzato tramite il timer hardware PIT (100 Hz / quanto temporale di 10ms) con integrazione del timer locale APIC.
   - Elaborazione disaccoppiata degli interrupt (IRQ) e segnalazione esplicita di fine interrupt (LAPIC EOI) per prevenire la saturazione del bus degli eventi.

2. **Isolamento Rigoroso della Memoria (PML4 x86_64 e Sicurezza W^X):**
   - Tabelle delle pagine a 4 livelli (PML4) con gestione canonica rigorosa dello spazio di indirizzamento virtuale.
   - I processi di Ring 3 risiedono in indirizzi utente sicuri (`>= 0x400000`), senza alcun accesso diretto alla memoria fisica o alle strutture di controllo di Ring 0.
   - Applicazione rigorosa della politica di sicurezza W^X (*Write XOR Execute*) su tutte le pagine allocate dinamicamente.

3. **Principio del Privilegio Minimo e Interfaccia Canonica di Syscall:**
   - Ring 0 è limitato al context switching, all'allocazione dei frame fisici, alla gestione degli interrupt hardware e all'interfaccia canonica delle chiamate di sistema (Syscall `0..85` via `syscall`/`sysret` con fallback compatibile `int 0x80`).
   - Il microkernel non gestisce, elabora né memorizza chiavi crittografiche private o seed mnemonici.

4. **Scambio di Messaggi Lock-Free (IPC):**
   - Caselle di posta IPC statiche e limitate per processo, senza alcuna allocazione dinamica nell'heap lungo il percorso critico di recapito.

5. **Virtual File System (VFS) con Persistenza Transazionale:**
   - Supporto nativo simultaneo per **FAT32** (`/disk`) ed **Ext2**.
   - Sottosistema dedicato **BlockCache** con politica di rimpiazzo Least Recently Used (LRU) per l'accelerazione dell'I/O su disco.
   - Flag di accesso ai file conformi agli standard POSIX (`O_RDONLY`, `O_WRONLY`, `O_RDWR`, `O_CREAT`, `O_APPEND`, `O_TRUNC`).
   - Meccanismo di salvaguardia rotazionale transazionale (`.TMP` -> `.BAK`), che impedisce il danneggiamento di cache del browser, cookie, segnalibri e file di configurazione in caso di arresti anomali improvvisi.

6. **Ambiente Grafico Aladdin Desktop in Ring 3:**
   - Display server e window compositor indipendente che opera interamente nello spazio utente (`apps/aladdin-server`).
   - Rendering ad alte prestazioni su Linear Framebuffer fornito dal protocollo Limine.
   - Driver per mouse PS/2 con ingestione disaccoppiata dei pacchetti, tracciamento fluido del cursore in tempo reale e gestione completa degli eventi di finestra (`Focus`, `Drag`, `Resize`, `Close`).

7. **Stack di Rete Sovrano, Firewall JinnWall SPI e Crittografia PKI:**
   - Stack di rete IPv4, TCP, UDP, ARP, DHCP e DNS scritto in Rust puro bare-metal con supporto per schede VirtIO-Net e Intel E1000.
   - Firewall con ispezione di stato dei pacchetti (**JinnWall SPI**).
   - Validazione nativa dei certificati **PKI X.509** e handshake sicuro **TLS 1.3** (RFC 8446) con derivazione di chiavi HKDF-SHA256/384.

8. **Applicazioni Native in Spazio Utente (Ring 3):**
   - **Jinn Browser:** Motore di navigazione web grafico privo di dipendenze da librerie C esterne, dotato di parser di layout HTML/CSS, client HTTP, cache locale, segnalibri e cronologia persistente archiviata in `/disk/browser/`.
   - **Aladdin Shell:** Terminale interattivo completo di utilità diagnostiche del kernel (`sysinfo`, `vfs`, `net`, `ps`, `mem`).
   - **Aladdin Wallet:** Portafoglio Web3 non-custodial con derivazione deterministica delle chiavi secondo gli standard BIP-39/BIP-44, isolato in Ring 3.

---

## Struttura del Repository

```text
jinn/
├── Cargo.toml            # Manifesto del workspace principale per il microkernel
├── limine.conf           # File di configurazione del bootloader Limine v2
├── build.rs              # Script di compilazione e linking del kernel
├── apps/                 # Applicazioni e servizi nativi dello spazio utente (Ring 3)
│   ├── aladdin-server/   # Server grafico Aladdin Desktop (Compositor Ring 3)
│   └── ola-mundo/        # Jinn Browser (browser.rs) e Aladdin Wallet (wallet.rs)
├── sdk/                  # Aladdin SDK e librerie client per Ring 3
│   └── aladdin-sdk/      # Wrapper per Syscall, superfici grafiche, eventi e IPC
├── contracts/            # Smart contract BEP-20 / EVM e suite di test Hardhat
│   ├── contracts/        # JinnToken.sol, JinnVesting.sol, JinnRewardDistributor.sol
│   ├── test/             # Test unitari e invarianti di sicurezza formali EVM
│   └── scripts/          # Pipeline di deployment su BSC Testnet/Mainnet
├── tools/                # Utilità di sviluppo e simulazione
│   ├── jinn-web3/        # jinn-web3 (CLI) e jinn-web3d (Demone Ring 3)
│   └── jda-sim/          # Simulatore di pipeline DAG e benchmark SMP
├── web/                  # Portale web ufficiale di Jinn OS (jinn-os.dev.br / GitHub Pages)
├── docs/                 # Specifiche tecniche, ADR e relazioni di audit
└── src/                  # Codice sorgente principale del microkernel (Ring 0)
    ├── main.rs           # Punto di ingresso del kernel (_start) e avvio sottosistemi
    ├── gdt.rs            # Global Descriptor Table e Task State Segment (Ring 0 / Ring 3)
    ├── idt.rs            # Interrupt Descriptor Table e gestori ISR/IRQ
    ├── syscall.rs        # Dispatcher canonico delle chiamate di sistema (Syscalls 0..85)
    ├── scheduler.rs      # Scheduler multitasking preemptive
    ├── ipc.rs            # Motore di scambio messaggi statico e lock-free
    ├── memory/           # Allocatore di frame fisici (Buddy), Heap del kernel e PML4
    ├── drivers/          # Driver (PIT, LAPIC, Tastiera, Mouse PS/2, IDE, PCI, VirtIO, E1000)
    ├── net/              # Stack TCP/IP, firewall JinnWall SPI, TLS 1.3 e PKI X.509
    └── vfs/              # Virtual File System (FAT32, Ext2, BlockCache LRU)
```

---

## Compilazione ed Esecuzione

### Prerequisiti
- **Toolchain Rust:** Compilatore `nightly` con target `x86_64-unknown-none` installato.
- **Strumenti di Emulazione:** `qemu-system-x86_64`, PowerShell 7+ (Windows/Linux) o Python 3.

```bash
# Aggiungere il target x86_64 bare-metal
rustup target add x86_64-unknown-none
```

### 1. Compilare il Microkernel e le Applicazioni
Eseguire lo script di build automatizzato per compilare il kernel, assemblare i binari di Ring 3 e generare l'immagine disco FAT32 avviabile:

```powershell
# Dalla radice del repository
.\scripts\build.ps1
```

### 2. Avviare la Macchina Virtuale in QEMU
Avviare l'emulatore con supporto hardware e periferiche attive:

```powershell
# Esecuzione standard con interfaccia Aladdin Desktop e rete
.\scripts\run.ps1

# Test di omologazione sotto vincoli rigorosi di memoria (es. test limite 64MB RAM):
.\scripts\run.ps1 -Memory 64M
```

---

## Ecosistema Web3 e Jinn Open Economy ($JINN)

Per garantire la sostenibilità economica sovrana a lungo termine, finanziare la ricerca sui sistemi operativi aperti e ricompensare i contributori attraverso il meccanismo di *Proof of Useful Contribution* (PoUC), il progetto integra un ecosistema di smart contract alimentato dal token nativo **$JINN** su BNB Smart Chain (BSC).

> [!IMPORTANT]
> **Isolamento Inviolabile delle Chiavi Private:** Il microkernel Jinn (Ring 0) non accede, non elabora né memorizza mai chiavi private, frasi mnemoniche o credenziali finanziarie. Tutte le operazioni crittografiche rimangono rigorosamente isolate nello spazio utente (Ring 3) all'interno dell'applicazione **Aladdin Wallet**.

- **Proposta Economica e Whitepaper:** [`JINN_TOKEN_PROPOSAL.md`](JINN_TOKEN_PROPOSAL.md)
- **Validazione EVM e Rapporto di Audit Fase II:** [`docs/JINN_WEB3_PHASE2_AUDIT_REPORT.md`](docs/JINN_WEB3_PHASE2_AUDIT_REPORT.md)
- **Governance Multi-Sig e Tesoreria:** [`docs/JINN_OPEN_ECONOMY_GOVERNANCE.md`](docs/JINN_OPEN_ECONOMY_GOVERNANCE.md)
- **Specifica PoUC (Sistema di Taglie):** [`docs/PROOF_OF_USEFUL_CONTRIBUTION_SPEC.md`](docs/PROOF_OF_USEFUL_CONTRIBUTION_SPEC.md)
- **Architettura del Wallet (Ring 3):** [`docs/JINN_WALLET_ARCHITECTURE.md`](docs/JINN_WALLET_ARCHITECTURE.md)
- **Codice Sorgente Aladdin Wallet:** [`apps/ola-mundo/src/wallet.rs`](apps/ola-mundo/src/wallet.rs)
- **Demone Userspace `jinn-web3d`:** [`tools/jinn-web3/src/daemon.rs`](tools/jinn-web3/src/daemon.rs)
- **Economia Computazionale DePIN (JDA):** [`docs/JDA_DEPIN_COMPUTATIONAL_ECONOMY.md`](docs/JDA_DEPIN_COMPUTATIONAL_ECONOMY.md)
- **Audit Interno degli Smart Contract:** [`docs/SMART_CONTRACTS_AUDIT_REPORT.md`](docs/SMART_CONTRACTS_AUDIT_REPORT.md)
- **Guida al Deployment in Testnet:** [`docs/TESTNET_DEPLOYMENT_GUIDE.md`](docs/TESTNET_DEPLOYMENT_GUIDE.md)

---

## Licenze e Governance

**Jinn OS** è distribuito secondo un **Modello a Doppia Licenza (Dual Licensing)**:

1. **Licenza Aperta / Comunitaria (`GPL-2.0-only`):** Destinata alla comunità open source, ai ricercatori accademici e agli appassionati. L'uso e le opere derivate sono regolati dalla GNU General Public License versione 2 ([`LICENSES/GPL-2.0-only.txt`](LICENSES/GPL-2.0-only.txt)).
2. **Licenza Commerciale Proprietaria (`Jinn-Commercial-1.0`):** Per installazioni aziendali, dispositivi embedded industriali e prodotti commerciali che richiedono l'esenzione dagli obblighi copyleft della GPL ([`LICENSES/Jinn-Commercial-1.0.txt`](LICENSES/Jinn-Commercial-1.0.txt)).

Per tutti i dettagli legali, consultare il file [`LICENSE`](LICENSE). Per contribuire al codice sorgente è richiesta la firma preventiva del [Contributor License Agreement (`CLA.md`)](CLA.md) e il rispetto delle [Linee Guida per i Contributi (`CONTRIBUTING.md`)](CONTRIBUTING.md).

---

## Contatti e Contributi

Il progetto accoglie con entusiasmo il contributo della comunità italiana e internazionale di sviluppatori Rust, esperti di programmazione low-level e appassionati di sistemi operativi. Dai miglioramenti del kernel ai driver di periferica, fino agli audit di sicurezza e alla documentazione, ogni supporto è prezioso! Consultate [CONTRIBUTING.md](CONTRIBUTING.md) per scoprire come inviare patch e pull request.
