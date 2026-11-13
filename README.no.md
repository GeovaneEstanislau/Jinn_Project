# Jinn OS — Minimalistisk Preemptiv x86_64-Mikrokjerne i Rust

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

> **Status:** Aktiv utvikling (Alpha v0.0.3)  
> **Arkitektur:** `x86_64` | **Programmeringsspråk:** Rust (`#![no_std]`, bare-metal, målaritektur `x86_64-unknown-none`)  
> **Oppstartslaster:** Limine Boot Protocol v2 (Higher-Half)  
> **Fellesskap:** Utviklet i Brasil med et urokkelig fokus på digital suverenitet, dybdebasert sikkerhet og radikal enkelhet.

---

## Om Prosjektet

**Jinn OS** er et rent mikrokjerneoperativsystem (Microkernel) bygget fra bunnen av i moderne Rust. Prosjektet oppsto fra et fundamentalt behov for å skape et pålitelig, minimalistisk og formelt verifiserbart beregningsgrunnlag, der utelukkende de strengt nødvendige kjernemekanismene kjører i prosessorens mest privilegerte modus (**Ring 0**).

I motsetning til tradisjonelle monolittiske operativsystemer, kjører alle komplekse delsystemer i Jinn OS — inkludert nettverksdrivere, filsystemer, vinduskompositoren **Aladdin Desktop** og applikasjoner som **Jinn Browser** — som isolerte prosesser i brukerrommet (**Ring 3**). Systemets integritet og prosessgrenser håndheves gjennom maskinvarebasert sidetabellinndeling, strenge rettighetskontroller og lynrask, låsfri interprosesskommunikasjon (IPC).

---

## Sentrale Arkitektoniske Pilarer og Milepæler (Alpha v0.0.3)

1. **Reell Preemptiv og Fleroppgavebasert Skedulering:**
   - Preemptiv Round-Robin-skedulerer styrt av maskinvarens PIT-timer (100 Hz / 10ms tidskvantum) samt integrasjon med den lokale APIC-timeren.
   - Frikoblet avbruddshåndtering (IRQ) og eksplisitt LAPIC EOI-signalering (End-Of-Interrupt) for å forhindre blokkering av avbruddsbussen.

2. **Strenge Minneisolering (x86_64 PML4 og W^X-Sikkerhet):**
   - 4-nivåers sidetabeller (PML4) med rigorøs styring av det kanoniske virtuelle adresserommet.
   - Brukerprosessene (Ring 3) plasseres i et sikkert kanonisk adresserom (`>= 0x400000`), uten direkte tilgang til fysisk minne eller kontrollstrukturene i Ring 0.
   - Kompromissløs håndheving av W^X-prinsippet (*Write XOR Execute*) over alle dynamisk allokerte minnesider.

3. **Prinsippet om Minste Privilegium og Kanonisk Syscall-Grensesnitt:**
   - Ring 0 er strengt begrenset til kontekstbytte, tildeling av fysiske siderammer, håndtering av maskinvareavbrudd og systemkall-ruting (Syscall `0..85` via `syscall`/`sysret` med kompatibel `int 0x80`-støtte).
   - Mikrokjernen håndterer, lagrer eller prosesserer aldri private kryptografiske nøkler eller mnemoniske fraser.

4. **Låsfri Interprosesskommunikasjon (Lock-Free IPC):**
   - Statiske, avgrensede meldingskøer per prosess uten noen dynamisk minneallokering (heap) i den kritiske rutingbanen.

5. **Virtuelt Filsystem (VFS) med Transaksjonell Sikkerhet:**
   - Samtidig innfødt støtte for både **FAT32** (`/disk`) og **Ext2**-filsystemer.
   - Eget **BlockCache**-delsystem basert på Least Recently Used (LRU)-algoritmen for å maksimere hastigheten på disk-I/O.
   - Standardiserte POSIX-tilgangsflagg (`O_RDONLY`, `O_WRONLY`, `O_RDWR`, `O_CREAT`, `O_APPEND`, `O_TRUNC`).
   - Roterende transaksjonell filbevaring (`.TMP` -> `.BAK`) som forhindrer datakorrupsjon i nettleserens hurtigbuffer, bokmerker, historikk og informasjonskapsler ved brå strømbrudd.

6. **Grafisk Aladdin Desktop-Brukermiljø i Ring 3:**
   - Uavhengig visningsserver og vinduskompositor som kjører fullt ut i brukerrommet (`apps/aladdin-server`).
   - Rask direktegjengivelse til det høyoppløselige Linear Framebuffer levert av Limine.
   - PS/2-musdriver med asynkron pakkebehandling, jevn markørsporing i sanntid og komplett vindushendelsesruting (`Focus`, `Drag`, `Resize`, `Close`).

7. **Suveren Nettverksstakk, JinnWall SPI-Brannmur og PKI-Kryptografi:**
   - C-uavhengig nettverksstakk i ren Rust for IPv4, TCP, UDP, ARP, DHCP og DNS med støtte for VirtIO-Net og Intel E1000-nettverkskort.
   - Tilstandsbasert pakkefiltreringsbrannmur (**JinnWall SPI**).
   - Innfødt **PKI X.509**-sertifikatskjede-validering og sikker **TLS 1.3**-håndtrykk (RFC 8446) med HKDF-SHA256/384 nøkkelutledning.

8. **Innfødte Ring 3-Brukerapplikasjoner:**
   - **Jinn Browser:** C-fri grafisk nettlesermotor med HTML/CSS-layoutmotor, HTTP-klient, lokal hurtigbuffer, bokmerker og transaksjonelt sikret historikk under `/disk/browser/`.
   - **Aladdin Shell:** Interaktivt terminalmiljø med innebygde kjerne-diagnostikkverktøy (`sysinfo`, `vfs`, `net`, `ps`, `mem`).
   - **Aladdin Wallet:** Web3-lommebok uten depot med deterministisk nøkkelutledning etter BIP-39/BIP-44-standardene, fullstendig isolert i Ring 3.

---

## Repositoriumstruktur

```text
jinn/
├── Cargo.toml            # Manifest for mikrokjernens rot-arbeidsområde
├── limine.conf           # Konfigurasjon for Limine v2 bootloader
├── build.rs              # Bygge- og lenkeskript for kjernen
├── apps/                 # Innfødte brukerromsapplikasjoner og tjenester (Ring 3)
│   ├── aladdin-server/   # Aladdin Desktop vinduskompositor (Ring 3)
│   └── ola-mundo/        # Jinn Browser (browser.rs) og Aladdin Wallet (wallet.rs)
├── sdk/                  # Aladdin SDK og Ring 3 klientbiblioteker
│   └── aladdin-sdk/      # Wrappere for systemkall, grafikkflater, hendelser og IPC
├── contracts/            # BEP-20 smarte kontrakter og Hardhat testsuite
│   ├── contracts/        # JinnToken.sol, JinnVesting.sol, JinnRewardDistributor.sol
│   ├── test/             # Enhetstester og formelle sikkerhetsinvarianter på EVM
│   └── scripts/          # Distribusjonsskript for BSC Testnet/Mainnet
├── tools/                # Utviklingsverktøy og simulatorer
│   ├── jinn-web3/        # jinn-web3 (CLI) og jinn-web3d (Ring 3 nisse/daemon)
│   └── jda-sim/          # DAG-flytsimulator og SMP-ytelsesmålinger
├── web/                  # Jinn OS offisiell nettportal (jinn-os.dev.br / GitHub Pages)
├── docs/                 # Tekniske spesifikasjoner, ADR-rapporter og revisjoner
└── src/                  # Mikrokjernens kildekode (Ring 0)
    ├── main.rs           # Kjernens inngangspunkt (_start) og oppstart av delsystemer
    ├── gdt.rs            # Global Descriptor Table og TSS (Ring 0 / Ring 3)
    ├── idt.rs            # Interrupt Descriptor Table og ISR/IRQ-avbruddsrutiner
    ├── syscall.rs        # Kanonisk systemkall-ruting (Syscalls 0..85)
    ├── scheduler.rs      # Preemptiv fleroppgave-skedulerer
    ├── ipc.rs            # Statisk låsfritt meldingsutvekslingssystem
    ├── memory/           # Fysisk sideallokator (Buddy), kjerne-heap og PML4
    ├── drivers/          # Drivere (PIT, LAPIC, tastatur, PS/2-mus, IDE, PCI, VirtIO, E1000)
    ├── net/              # TCP/IP-stakk, JinnWall-brannmur, TLS 1.3 og X.509 PKI
    └── vfs/              # Virtuelt filsystem (FAT32, Ext2, LRU BlockCache)
```

---

## Bygging og Kjøring

### Forutsetninger
- **Rust Verktøykjede:** `nightly`-kompilator med målaritekturen `x86_64-unknown-none` installert.
- **Emuleringsverktøy:** `qemu-system-x86_64`, PowerShell 7+ (Windows/Linux) eller Python 3.

```bash
# Legg til x86_64 bare-metal målaritektur
rustup target add x86_64-unknown-none
```

### 1. Kompilere Mikrokjernen og Brukerapplikasjonene
Kjør det integrerte byggeskriptet for å bygge kjernen, kompilere Ring 3-applikasjonene og opprette det oppstartbare FAT32-diskbilde:

```powershell
# Fra roten av repositoriet
.\scripts\build.ps1
```

### 2. Kjøring i QEMU-Emulatoren
Start den virtuelle maskinen med full maskinvare- og periferi-emulering:

```powershell
# Standardkjøring med Aladdin Desktop og virtuelt nettverk
.\scripts\run.ps1

# Stresstesting under strenge minnebegrensninger (f.eks. 64MB RAM grensetest):
.\scripts\run.ps1 -Memory 64M
```

---

## Web3-Økosystem og Jinn Open Economy ($JINN)

For å sikre langsiktig økonomisk bærekraft for forskning på åpne systemer, og belønne bidragsytere gjennom *Proof of Useful Contribution* (PoUC), integrerer prosjektet smarte kontrakter for det innfødte tokenet **$JINN** på BNB Smart Chain (BSC).

> [!IMPORTANT]
> **Absolutt Nøkkelisolasjon:** Jinn-mikrokjernen (Ring 0) aksesserer, lagrer eller håndterer aldri private nøkler, sikkerhetsfraser eller transaksjoner. Alle kryptografiske operasjoner forblir strengt isolert i brukerrommet (Ring 3) i **Aladdin Wallet**-applikasjonen.

- **Økonomisk Forslag og Whitepaper:** [`JINN_TOKEN_PROPOSAL.md`](JINN_TOKEN_PROPOSAL.md)
- **EVM-Validering og Fase II Revisjonsrapport:** [`docs/JINN_WEB3_PHASE2_AUDIT_REPORT.md`](docs/JINN_WEB3_PHASE2_AUDIT_REPORT.md)
- **Multi-Sig Styring og Skattkammer:** [`docs/JINN_OPEN_ECONOMY_GOVERNANCE.md`](docs/JINN_OPEN_ECONOMY_GOVERNANCE.md)
- **PoUC-Spesifikasjon (Dusørordning):** [`docs/PROOF_OF_USEFUL_CONTRIBUTION_SPEC.md`](docs/PROOF_OF_USEFUL_CONTRIBUTION_SPEC.md)
- **Lommebokarkitektur (Ring 3):** [`docs/JINN_WALLET_ARCHITECTURE.md`](docs/JINN_WALLET_ARCHITECTURE.md)
- **Kildekode for Aladdin Wallet Brukergrensesnitt:** [`apps/ola-mundo/src/wallet.rs`](apps/ola-mundo/src/wallet.rs)
- **Brukerrom-Daemon `jinn-web3d`:** [`tools/jinn-web3/src/daemon.rs`](tools/jinn-web3/src/daemon.rs)
- **DePIN Beregningsøkonomi (JDA):** [`docs/JDA_DEPIN_COMPUTATIONAL_ECONOMY.md`](docs/JDA_DEPIN_COMPUTATIONAL_ECONOMY.md)
- **Intern Revisjonsrapport for Smarte Kontrakter:** [`docs/SMART_CONTRACTS_AUDIT_REPORT.md`](docs/SMART_CONTRACTS_AUDIT_REPORT.md)
- **Veiledning for Distribusjon til Testnet:** [`docs/TESTNET_DEPLOYMENT_GUIDE.md`](docs/TESTNET_DEPLOYMENT_GUIDE.md)

---

## Lisensiering og Styring

**Jinn OS** distribueres under en **Dobbelt Lisensmodell (Dual Licensing)**:

1. **Åpen Fellesskapslisens (`GPL-2.0-only`):** Rettet mot åpen kildekode-miljøet, akademia og uavhengige utviklere. All videreformidling og avledede verk reguleres under betingelsene i GNU General Public License versjon 2 ([`LICENSES/GPL-2.0-only.txt`](LICENSES/GPL-2.0-only.txt)).
2. **Proprietær Kommersiell Lisens (`Jinn-Commercial-1.0`):** For industriell bruk, integrerte systemer og kommersielle produkter som krever fritak fra copyleft-kravene i GPL ([`LICENSES/Jinn-Commercial-1.0.txt`](LICENSES/Jinn-Commercial-1.0.txt)).

For fullstendige juridiske vilkår, se [`LICENSE`](LICENSE). Kildekodebidrag forutsetter forhåndssignering av [Bidragsyteravtalen (`CLA.md`)](CLA.md) og etterlevelse av [Retningslinjer for Bidrag (`CONTRIBUTING.md`)](CONTRIBUTING.md).

---

## Kontakt og Bidrag

Jinn OS ønsker utviklere fra det nordiske og globale Rust-miljøet hjertelig velkommen til å delta! Enten det gjelder kjerneoptimalisering, utvikling av maskinvaredrivere, sikkerhetsrevisjon eller dokumentasjon, er ethvert bidrag av stor betydning. Se [CONTRIBUTING.md](CONTRIBUTING.md) for instruksjoner om innsending av oppdateringer og pull requests.

