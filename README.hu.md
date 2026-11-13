# Jinn OS — Minimalista és Preemptív x86_64 Mikrokernelt Rust Nyelven

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

> **Állapot:** Aktív fejlesztés alatt (Alpha v0.0.3)  
> **Architektúra:** `x86_64` | **Nyelv:** Rust (`#![no_std]`, bare-metal, célplatform `x86_64-unknown-none`)  
> **Rendszerbetöltő:** Limine Boot Protocol v2 (Higher-Half)  
> **Közösség:** Brazíliában fejlesztve — a digitális szuverenitás, a mélységi védelem és a radikális egyszerűség jegyében.

---

## A Projektről

A **Jinn OS** egy modern Rust nyelven, alapjaitól újraírt tiszta mikrokernelt (Microkernel) operációs rendszer. A projekt azon elengedhetetlen igényből született, hogy egy olyan megbízható, minimalista és formálisan ellenőrizhető számítástechnikai alapot hozzunk létre, amelyben kizárólag a legszükségesebb alapmechanizmusok futnak a processzor legmagasabb kiváltsági szintjén (**Ring 0**).

A hagyományos monolitikus kernelekkel ellentétben a Jinn OS-ben minden összetett alrendszer — beleértve a hálózati illesztőprogramokat, a fájlrendszereket, az **Aladdin Desktop** ablakkompozitort és az olyan alkalmazásokat, mint a **Jinn Browser** — szigorúan izolált folyamatként fut a felhasználói térben (**Ring 3**). A rendszer stabilitását hardveres lapozás, szigorú jogosultság-ellenőrzés és rendkívül gyors, zármentes folyamatközi kommunikáció (IPC) biztosítja.

---

## Fő Építészeti Pillérek és Mérföldkövek (Alpha v0.0.3)

1. **Valódi Preemptív és Többfeladatos Ütemezés:**
   - A hardveres PIT időzítő (100 Hz / 10ms időszelet) és a helyi APIC időzítő által vezérelt preemptív Round-Robin ütemező.
   - Teljesen függetlenített megszakításkezelés (IRQ) és explicit LAPIC EOI (End-Of-Interrupt) nyugtázás, amely megakadályozza a megszakítási busz túlterhelődését.

2. **Szigorú Memóriaizoláció (x86_64 PML4 és W^X Védelem):**
   - 4 szintű laptábla-kezelés a kanonikus virtuális címtér szigorú felügyeletével.
   - A Ring 3 folyamatok biztonságos felhasználói címtérben helyezkednek el (`>= 0x400000`), közvetlen hozzáférés nélkül a fizikai memóriához vagy a Ring 0 vezérlőstruktúráihoz.
   - A W^X (*Write XOR Execute*) memóriavédelmi szabályzat kompromisszummentes érvényesítése minden dinamikusan lefoglalt oldalon.

3. **Legkisebb Jogosultság Elve és Kanonikus Rendszerhívási Interfész:**
   - A Ring 0 feladata kizárólag a környezetváltásra, a fizikai lapkeretek lefoglalására, a hardveres megszakítások elosztására és a kanonikus rendszerhívások kiszolgálására korlátozódik (Syscall `0..85` a `syscall`/`sysret` és a kompatibilis `int 0x80` szoftveres csapdán keresztül).
   - A mikrokernelt soha nem tárol, nem dolgoz fel és nem fogad privát titkosítási kulcsokat vagy mnemonikus magokat.

4. **Zármentes Folyamatközi Kommunikáció (Lock-Free IPC):**
   - Folyamatonként statikusan lefoglalt, korlátos IPC postaládák, dinamikus heap-memóriafoglalás nélkül a kritikus adatátviteli útvonalon.

5. **Tranzakciós Megbízhatóságú Virtuális Fájlrendszer (VFS):**
   - Egyidejű natív támogatás a **FAT32** (`/disk`) és az **Ext2** fájlrendszerekhez.
   - Különálló **BlockCache** alrendszer Least Recently Used (LRU) cserélési stratégiával a lemezes I/O műveletek felgyorsítására.
   - Szabványos POSIX hozzáférési jelzők (`O_RDONLY`, `O_WRONLY`, `O_RDWR`, `O_CREAT`, `O_APPEND`, `O_TRUNC`).
   - Forgó tranzakciós fájlmentési mechanizmus (`.TMP` -> `.BAK`), amely hirtelen áramkimaradás esetén is megvédi a böngésző gyorsítótárát, könyvjelzőit, előzményeit és sütijeit a sérüléstől.

6. **Ring 3 Natív Aladdin Desktop Grafikus Környezet:**
   - Teljes egészében a felhasználói térben futó, független ablakkompozitor és megjelenítő kiszolgáló (`apps/aladdin-server`).
   - Nagy felbontású megjelenítés közvetlenül a Limine által biztosított Linear Framebuffer felületre.
   - PS/2 egérkezelő függetlenített csomagfeldolgozással, sima valós idejű kurzorkövetéssel és teljes körű ablakesemény-elosztással (`Focus`, `Drag`, `Resize`, `Close`).

7. **Szuverén Hálózati Verem, JinnWall SPI Tűzfal és PKI Kriptográfia:**
   - Külső C könyvtáraktól mentes, bare-metal IPv4, TCP, UDP, ARP, DHCP és DNS protokollverem VirtIO-Net és Intel E1000 hálózati kártyák támogatásával.
   - Állapotalapú csomagvizsgáló tűzfal (**JinnWall SPI**).
   - Natív **PKI X.509** tanúsítványlánc-ellenőrzés és biztonságos **TLS 1.3** kézfogás (RFC 8446) HKDF-SHA256/384 kulcslevezetéssel.

8. **Natív Ring 3 Felhasználói Alkalmazások:**
   - **Jinn Browser:** C-függőségektől mentes natív grafikus böngészőmotor HTML/CSS elrendezés-megjelenítővel, beépített HTTP-klienssel, valamint a `/disk/browser/` alatt perzisztált gyorsítótárral, könyvjelzőkkel és előzményekkel.
   - **Aladdin Shell:** Interaktív parancssor diagnosztikai parancsokkal (`sysinfo`, `vfs`, `net`, `ps`, `mem`).
   - **Aladdin Wallet:** BIP-39/BIP-44 szabványokon alapuló, nem-letétkezelői Web3 pénztárca, szigorúan izolált Ring 3 környezetben futtatva.

---

## A Repozitórium Felépítése

```text
jinn/
├── Cargo.toml            # A mikrokernelt fő workspace konfigurációja
├── limine.conf           # A Limine v2 bootloader konfigurációja
├── build.rs              # A kernel fordítási és szerkesztési szkriptje
├── apps/                 # Ring 3 natív felhasználói alkalmazások és szolgáltatások
│   ├── aladdin-server/   # Aladdin Desktop ablakkompozitor (Ring 3)
│   └── ola-mundo/        # Jinn Browser (browser.rs) és Aladdin Wallet (wallet.rs)
├── sdk/                  # Aladdin SDK és Ring 3 kliensoldali könyvtárak
│   └── aladdin-sdk/      # Rendszerhívások, grafika, események és IPC felületek
├── contracts/            # BEP-20 okosszerződések és Hardhat tesztkörnyezet
│   ├── contracts/        # JinnToken.sol, JinnVesting.sol, JinnRewardDistributor.sol
│   ├── test/             # EVM egységtesztek és biztonsági invariáns-ellenőrzések
│   └── scripts/          # BSC Testnet és Mainnet automatikus telepítési szkriptek
├── tools/                # Fejlesztői eszközök és szimulátorok
│   ├── jinn-web3/        # jinn-web3 (CLI) és jinn-web3d (Ring 3 démon)
│   └── jda-sim/          # DAG feldolgozási szimulátor és SMP teljesítménymérések
├── web/                  # Hivatalos Jinn OS weboldal (jinn-os.dev.br / GitHub Pages)
├── docs/                 # Műszaki dokumentációk, ADR-ek és auditjelentések
└── src/                  # A mikrokernelt fő forráskódja (Ring 0)
    ├── main.rs           # Kernel belépési pont (_start) és alrendszer-indítás
    ├── gdt.rs            # Globális leírótábla (GDT) és TSS (Ring 0 / Ring 3)
    ├── idt.rs            # Megszakítási leírótábla (IDT) és ISR/IRQ kezelők
    ├── syscall.rs        # Kanonikus rendszerhívás-elosztó (Syscalls 0..85)
    ├── scheduler.rs      # Preemptív többfeladatos ütemező
    ├── ipc.rs            # Statikus zármentes üzenetküldő motor
    ├── memory/           # Fizikai lapallokátor (Buddy), kernel heap és PML4
    ├── drivers/          # Illesztőprogramok (PIT, LAPIC, billentyűzet, PS/2 egér, IDE, PCI, VirtIO, E1000)
    ├── net/              # TCP/IP verem, JinnWall SPI tűzfal, TLS 1.3 és PKI X.509
    └── vfs/              # Virtuális fájlrendszer (FAT32, Ext2, LRU BlockCache)
```

---

## Fordítás és Futtatás

### Előfeltételek
- **Rust Eszköztár:** `nightly` fordító a telepített `x86_64-unknown-none` célplatformmal.
- **Emulációs Eszközök:** `qemu-system-x86_64`, PowerShell 7+ (Windows/Linux) vagy Python 3.

```bash
# Bare-metal x86_64 célplatform hozzáadása
rustup target add x86_64-unknown-none
```

### 1. A Mikrokernelt és az Alkalmazások Fordítása
Futtassa az integrált build szkriptet a kernel lefordításához, a Ring 3 alkalmazások összeállításához és az indítható FAT32 lemezkép előállításához:

```powershell
# A repozitórium gyökérkönyvtárából
.\scripts\build.ps1
```

### 2. Futtatás QEMU Virtuális Gépben
Indítsa el az operációs rendszert a hardveres környezet szimulációjával:

```powershell
# Standard indítás Aladdin Desktop felülettel és hálózattal
.\scripts\run.ps1

# Terheléses tesztelés szigorú memóriakorlát mellett (pl. 64MB RAM vizsgálat):
.\scripts\run.ps1 -Memory 64M
```

---

## Web3 Ökoszisztéma és Jinn Open Economy ($JINN)

A nyílt forráskódú operációs rendszer kutatások hosszú távú pénzügyi fenntarthatóságának biztosítása és a közreműködők jutalmazása érdekében (*Proof of Useful Contribution*, PoUC) a projekt integrálja a natív **$JINN** tokent a BNB Smart Chain (BSC) hálózatán.

> [!IMPORTANT]
> **Privát Kulcsok Érinthetetlensége:** A Jinn mikrokernelt (Ring 0) soha nem fér hozzá, nem tárol és nem dolgoz fel privát kulcsokat, mnemonikus szavakat vagy pénzügyi tranzakciókat. Minden kriptográfiai művelet szigorúan a felhasználói térben (Ring 3), az **Aladdin Wallet** biztonsági sandboxában fut.

- **Gazdasági Javaslat és Whitepaper:** [`JINN_TOKEN_PROPOSAL.md`](JINN_TOKEN_PROPOSAL.md)
- **EVM Validáció és II. Fázisú Auditjelentés:** [`docs/JINN_WEB3_PHASE2_AUDIT_REPORT.md`](docs/JINN_WEB3_PHASE2_AUDIT_REPORT.md)
- **Multi-Sig Irányítás és Kincstár:** [`docs/JINN_OPEN_ECONOMY_GOVERNANCE.md`](docs/JINN_OPEN_ECONOMY_GOVERNANCE.md)
- **PoUC Specifikáció (Bounty Rendszer):** [`docs/PROOF_OF_USEFUL_CONTRIBUTION_SPEC.md`](docs/PROOF_OF_USEFUL_CONTRIBUTION_SPEC.md)
- **Pénztárca Architektúra (Ring 3):** [`docs/JINN_WALLET_ARCHITECTURE.md`](docs/JINN_WALLET_ARCHITECTURE.md)
- **Aladdin Wallet Felület Forráskódja:** [`apps/ola-mundo/src/wallet.rs`](apps/ola-mundo/src/wallet.rs)
- **Felhasználói Démon `jinn-web3d`:** [`tools/jinn-web3/src/daemon.rs`](tools/jinn-web3/src/daemon.rs)
- **DePIN Számítási Gazdaság (JDA):** [`docs/JDA_DEPIN_COMPUTATIONAL_ECONOMY.md`](docs/JDA_DEPIN_COMPUTATIONAL_ECONOMY.md)
- **Okosszerződések Belső Biztonsági Auditja:** [`docs/SMART_CONTRACTS_AUDIT_REPORT.md`](docs/SMART_CONTRACTS_AUDIT_REPORT.md)
- **Testnet Telepítési Útmutató:** [`docs/TESTNET_DEPLOYMENT_GUIDE.md`](docs/TESTNET_DEPLOYMENT_GUIDE.md)

---

## Licencelés és Irányítás

A **Jinn OS** **Kettős Licencelési Modellt (Dual Licensing)** alkalmaz:

1. **Nyílt Közösségi Licenc (`GPL-2.0-only`):** A nyílt forráskódú fejlesztők, egyetemi kutatók és hobbifejlesztők számára. A származtatott munkák a GNU General Public License 2. verziójának feltételei szerint szabadon terjeszthetők ([`LICENSES/GPL-2.0-only.txt`](LICENSES/GPL-2.0-only.txt)).
2. **Kereskedelmi Szabadalmi Licenc (`Jinn-Commercial-1.0`):** Ipari berendezésekhez, beágyazott eszközökhöz és olyan üzleti termékekhez, amelyek mentességet igényelnek a GPL kötelezettségei alól ([`LICENSES/Jinn-Commercial-1.0.txt`](LICENSES/Jinn-Commercial-1.0.txt)).

A jogi feltételek részleteiért tekintse meg a [`LICENSE`](LICENSE) fájlt. A kódbeli hozzájárulásokhoz szükséges a [Közreműködői Licencmegállapodás (`CLA.md`)](CLA.md) aláírása és a [Közreműködési Irányelvek (`CONTRIBUTING.md`)](CONTRIBUTING.md) betartása.

---

## Kapcsolat és Közreműködés

A Jinn OS örömmel fogadja a magyar és nemzetközi Rust programozók, alacsony szintű rendszermérnökök és fejlesztők közreműködését! Legyen szó a kernel optimalizálásáról, illesztőprogramok készítéséről, biztonsági vizsgálatokról vagy dokumentációról, minden támogatást szívesen veszünk. A javítások és Pull Requestek beküldéséhez olvassa el a [CONTRIBUTING.md](CONTRIBUTING.md) útmutatót.
