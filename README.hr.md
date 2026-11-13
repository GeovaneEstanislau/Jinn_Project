# Jinn OS — Minimalistički Preemptivni x86_64 Mikrokernal u Rustu

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

> **Status:** Aktivni razvoj (Alpha v0.0.3)  
> **Arhitektura:** `x86_64` | **Programski jezik:** Rust (`#![no_std]`, bare-metal, ciljna platforma `x86_64-unknown-none`)  
> **Bootloader:** Limine Boot Protocol v2 (Higher-Half)  
> **Zajednica:** Razvijeno u Brazilu s fokusom na računalni suverenitet, višeslojnu sigurnost i radikalnu jednostavnost.

---

## O Projektu

**Jinn OS** je operacijski sustav s mikrokernel arhitekturom napisan u potpunosti od nule u modernom Rustu. Projekt je nastao iz potrebe za izgradnjom pouzdane, minimalističke i formalno provjerljive računalne osnove, u kojoj samo najnužniji temeljni mehanizmi djeluju unutar najprivilegiranijeg načina rada procesora (**Ring 0**).

Za razliku od tradicionalnih monolitnih kernela, u Jinn OS-u svi složeni podsustavi — uključujući mrežne upravljačke programe (drivere), datotečne sustave, grafički poslužitelj prozora **Aladdin Desktop** i aplikacije poput preglednika **Jinn Browser** — izvršavaju se kao izolirani procesi u korisničkom prostoru (**Ring 3**). Cjelovitost sustava i granice procesa osigurani su hardverskim straničenjem memorije, strogim provjerama ovlasti i brzom komunikacijom među procesima bez zaključavanja (IPC).

---

## Arhitektonski Stupovi i Prekretnice (Alpha v0.0.3)

1. **Pravo Preemptivno i Višezadaćno Raspoređivanje:**
   - Preemptivni Round-Robin raspoređivač (scheduler) vođen hardverskim PIT timerom (100 Hz / kvant od 10ms) uz integraciju s lokalnim APIC timerom.
   - Potpuno odvojena obrada hardverskih prekida (IRQ) i eksplicitno slanje signala završetka (LAPIC EOI) kako bi se spriječilo zagušenje sabirnice prekida.

2. **Stroga Izolacija Memorije (x86_64 PML4 i Sigurnost W^X):**
   - 4-razinsko straničenje memorije (PML4) uz strogo upravljanje kanonskim virtualnim adresnim prostorom.
   - Korisnički procesi (Ring 3) smješteni su u sigurnom adresnom prostoru (`>= 0x400000`), bez izravnog pristupa fizičkoj memoriji ili kontrolnim strukturama Ringa 0.
   - Dosljedna primjena W^X pravila (*Write XOR Execute*) na svim dinamički dodijeljenim stranicama memorije.

3. **Načelo Najmanjih Privilegija i Kanonsko Sučelje Sistemskih Poziva:**
   - Ring 0 ograničen je na prebacivanje konteksta, dodjelu fizičkih okvira memorije, usmjeravanje hardverskih prekida i kanonsko sučelje sistemskih poziva (Syscalls `0..85` putem `syscall`/`sysret` uz podršku za kompatibilni `int 0x80`).
   - Mikrokernal nikada ne sprema, ne obrađuje niti prosljeđuje privatne kriptografske ključeve ili mnemoničke fraze.

4. **Komunikacija među Procesima bez Zaključavanja (Lock-Free IPC):**
   - Statički definirani pretinci poruka po procesu bez dinamičkih alokacija memorije na kritičnoj stazi prijenosa podataka.

5. **Virtualni Datotečni Sustav (VFS) s Transakcijskom Zaštitom:**
   - Izvorna i istodobna podrška za **FAT32** (`/disk`) i **Ext2** datotečne sustave.
   - Namjenski **BlockCache** podsustav s Least Recently Used (LRU) algoritmom za znatno ubrzanje I/O operacija na disku.
   - Standardne POSIX zastavice pristupa datotekama (`O_RDONLY`, `O_WRONLY`, `O_RDWR`, `O_CREAT`, `O_APPEND`, `O_TRUNC`).
   - Mehanizam transakcijske rotacije (`.TMP` -> `.BAK`) koji u slučaju iznenadnog nestanka struje sprječava oštećenje predmemorije preglednika, oznaka, povijesti i kolačića.

6. **Aladdin Desktop Grafičko Okruženje u Ring 3:**
   - Samostalni poslužitelj i kompozitor prozora koji u potpunosti radi u korisničkom prostoru (`apps/aladdin-server`).
   - Iscrtavanje visoke rezolucije izravno na Linear Framebuffer koji osigurava Limine bootloader.
   - PS/2 upravljački program za miša s odvojenom obradom paketa, glatkim praćenjem pokazivača u stvarnom vremenu i usmjeravanjem događaja na prozorima (`Focus`, `Drag`, `Resize`, `Close`).

7. **Suvereni Mrežni Stog, JinnWall SPI Vatrozid i PKI Kriptografija:**
   - Mrežni stog bez vanjskih C ovisnosti za IPv4, TCP, UDP, ARP, DHCP i DNS s podrškom za VirtIO-Net i Intel E1000 mrežne kartice.
   - Vatrozid s provjerom stanja paketa (**JinnWall SPI**).
   - Izvorna provjera valjanosti **PKI X.509** digitalnih certifikata i sigurno **TLS 1.3** rukovanje (RFC 8446) s HKDF-SHA256/384 generiranjem ključeva.

8. **Izvorne Aplikacije Korisničkog Prostora (Ring 3):**
   - **Jinn Browser:** Grafički web preglednik bez ovisnosti o bibliotekama jezika C, opremljen mehanizmom za prikaz HTML/CSS izgleda, HTTP klijentom, lokalnom predmemorijom, oznakama i trajnom poviješću u `/disk/browser/`.
   - **Aladdin Shell:** Interaktivni terminal s ugrađenim dijagnostičkim alatima za pregled stanja kernela (`sysinfo`, `vfs`, `net`, `ps`, `mem`).
   - **Aladdin Wallet:** Web3 novčanik bez skrbništva s determinističkim izvođenjem ključeva prema standardima BIP-39/BIP-44, potpuno izoliran u Ring 3 prostoru.

---

## Struktura Repozitorija

```text
jinn/
├── Cargo.toml            # Glavna konfiguracijska datoteka mikrokernal radnog prostora
├── limine.conf           # Konfiguracija Limine v2 bootloadera
├── build.rs              # Skripta za izgradnju i povezivanje kernela
├── apps/                 # Izvorne aplikacije i servisi u korisničkom prostoru (Ring 3)
│   ├── aladdin-server/   # Aladdin Desktop poslužitelj prozora (Ring 3 kompozitor)
│   └── ola-mundo/        # Jinn Browser (browser.rs) i Aladdin Wallet (wallet.rs)
├── sdk/                  # Aladdin SDK i klijentske biblioteke za Ring 3
│   └── aladdin-sdk/      # Omotači sistemskih poziva, grafike, događaja i IPC-a
├── contracts/            # BEP-20 pametni ugovori i Hardhat skup testova
│   ├── contracts/        # JinnToken.sol, JinnVesting.sol, JinnRewardDistributor.sol
│   ├── test/             # Jedinični testovi i formalne provjere sigurnosti na EVM-u
│   └── scripts/          # Skripte za postavljanje na BSC Testnet/Mainnet
├── tools/                # Razvojni alati i simulatori
│   ├── jinn-web3/        # jinn-web3 (CLI) i jinn-web3d (Ring 3 demon)
│   └── jda-sim/          # Simulator DAG cjevovoda i SMP testovi performansi
├── web/                  # Službeni web portal sustava Jinn OS (jinn-os.dev.br / GitHub Pages)
├── docs/                 # Tehničke specifikacije, ADR zapisi i revizijska izvješća
└── src/                  # Izvorni kod jezgre mikrokernala (Ring 0)
    ├── main.rs           # Ulazna točka kernela (_start) i pokretanje podsustava
    ├── gdt.rs            # Global Descriptor Table i TSS (Ring 0 / Ring 3)
    ├── idt.rs            # Interrupt Descriptor Table i rukovatelji ISR/IRQ
    ├── syscall.rs        # Kanonski dispečer sistemskih poziva (Syscalls 0..85)
    ├── scheduler.rs      # Preemptivni višezadaćni raspoređivač
    ├── ipc.rs            # Statički mehanizam razmjene poruka bez zaključavanja
    ├── memory/           # Alokator fizičkih okvira (Buddy), Kernel Heap i PML4
    ├── drivers/          # Upravljački programi (PIT, LAPIC, tipkovnica, PS/2 miš, IDE, PCI, VirtIO, E1000)
    ├── net/              # TCP/IP stog, JinnWall vatrozid, TLS 1.3 i PKI X.509
    └── vfs/              # Virtualni datotečni sustav (FAT32, Ext2, LRU BlockCache)
```

---

## Izgradnja i Pokretanje

### Preduvjeti
- **Rust Alati:** `nightly` prevodilac s instaliranom ciljnom platformom `x86_64-unknown-none`.
- **Alati za Emulaciju:** `qemu-system-x86_64`, PowerShell 7+ (Windows/Linux) ili Python 3.

```bash
# Dodavanje bare-metal x86_64 ciljne platforme
rustup target add x86_64-unknown-none
```

### 1. Izgradnja Mikrokernala i Aplikacija
Pokrenite integriranu skriptu za automatsku izgradnju kernela, aplikacija u Ring 3 prostoru i pripremu pokretačke FAT32 slike diska:

```powershell
# Iz korijenskog direktorija repozitorija
.\scripts\build.ps1
```

### 2. Pokretanje u QEMU Emulatoru
Pokrenite virtualni stroj s emulacijom hardvera i periferije:

```powershell
# Standardno pokretanje s Aladdin Desktop grafičkim sučeljem i mrežom
.\scripts\run.ps1

# Testiranje stabilnosti pod strogim ograničenjem radne memorije (npr. 64MB RAM test):
.\scripts\run.ps1 -Memory 64M
```

---

## Web3 Ekosustav i Jinn Open Economy ($JINN)

Kako bi se osigurala dugoročna financijska održivost istraživanja otvorenih operacijskih sustava i nagradili suradnici putem modela *Proof of Useful Contribution* (PoUC), projekt integrira pametne ugovore izvornog tokena **$JINN** na mreži BNB Smart Chain (BSC).

> [!IMPORTANT]
> **Apsolutna Izolacija Privatnih Ključeva:** Jinn mikrokernal (Ring 0) nikada ne pristupa, ne pohranjuje niti obrađuje korisničke privatne ključeve, mnemoničke fraze ili financijske transakcije. Sve kriptografske operacije ostaju strogo izolirane u korisničkom prostoru (Ring 3) unutar aplikacije **Aladdin Wallet**.

- **Ekonomski Prijedlog i Whitepaper:** [`JINN_TOKEN_PROPOSAL.md`](JINN_TOKEN_PROPOSAL.md)
- **EVM Validacija i Izvješće o Reviziji Faze II:** [`docs/JINN_WEB3_PHASE2_AUDIT_REPORT.md`](docs/JINN_WEB3_PHASE2_AUDIT_REPORT.md)
- **Multi-Sig Upravljanje i Riznica:** [`docs/JINN_OPEN_ECONOMY_GOVERNANCE.md`](docs/JINN_OPEN_ECONOMY_GOVERNANCE.md)
- **PoUC Specifikacija (Sustav Nagrađivanja):** [`docs/PROOF_OF_USEFUL_CONTRIBUTION_SPEC.md`](docs/PROOF_OF_USEFUL_CONTRIBUTION_SPEC.md)
- **Arhitektura Novčanika (Ring 3):** [`docs/JINN_WALLET_ARCHITECTURE.md`](docs/JINN_WALLET_ARCHITECTURE.md)
- **Izvorni Kod Aladdin Wallet Sučelja:** [`apps/ola-mundo/src/wallet.rs`](apps/ola-mundo/src/wallet.rs)
- **Demon Korisničkog Prostora `jinn-web3d`:** [`tools/jinn-web3/src/daemon.rs`](tools/jinn-web3/src/daemon.rs)
- **DePIN Računalna Ekonomija (JDA):** [`docs/JDA_DEPIN_COMPUTATIONAL_ECONOMY.md`](docs/JDA_DEPIN_COMPUTATIONAL_ECONOMY.md)
- **Interna Revizija Pametnih Ugovora:** [`docs/SMART_CONTRACTS_AUDIT_REPORT.md`](docs/SMART_CONTRACTS_AUDIT_REPORT.md)
- **Vodič za Postavljanje na Testnet:** [`docs/TESTNET_DEPLOYMENT_GUIDE.md`](docs/TESTNET_DEPLOYMENT_GUIDE.md)

---

## Licenciranje i Upravljanje

**Jinn OS** se distribuira pod **Dvojnim Modelom Licenciranja (Dual Licensing)**:

1. **Otvorena Licenca Zajednice (`GPL-2.0-only`):** Namijenjena zajednici otvorenog koda, istraživačima i entuzijastima. Korištenje i izvedena djela slobodni su prema uvjetima licence GNU General Public License verzije 2 ([`LICENSES/GPL-2.0-only.txt`](LICENSES/GPL-2.0-only.txt)).
2. **Komercijalna Vlasnička Licenca (`Jinn-Commercial-1.0`):** Za industrijska ugrađena računala, uređaje zatvorenog koda i komercijalne proizvode koji zahtijevaju izuzeće od copyleft obveza GPL licence ([`LICENSES/Jinn-Commercial-1.0.txt`](LICENSES/Jinn-Commercial-1.0.txt)).

Pojedinosti o pravnim uvjetima potražite u datoteci [`LICENSE`](LICENSE). Doprinosi izvornom kodu zahtijevaju prethodno potpisivanje [Ugovora o Licenci Suradnika (`CLA.md`)](CLA.md) i poštivanje [Smjernica za Doprinos (`CONTRIBUTING.md`)](CONTRIBUTING.md).

---

## Kontakt i Sudjelovanje

Jinn OS s veseljem poziva hrvatsku i međunarodnu zajednicu Rust inženjera, programera sustava niske razine i entuzijasta operacijskih sustava na suradnju. Bilo da se radi o optimizaciji kernela, razvoju drivera, sigurnosnim revizijama ili dokumentaciji, vaša je pomoć iznimno vrijedna! Pogledajte [CONTRIBUTING.md](CONTRIBUTING.md) kako biste saznali kako poslati zakrpe i Pull Requestove.
