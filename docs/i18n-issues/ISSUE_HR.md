# Title: 🇭🇷 Poziv za suradnike i tehničku lekturu: Hrvatska zajednica (README.hr.md)
# Labels: documentation, i18n, help wanted, good first issue

Dobrodošli u zajednicu otvorenog koda sustava **Jinn OS**!

Objavili smo službenu verziju središnje tehničke dokumentacije na hrvatskom jeziku: [**`README.hr.md`**](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/README.hr.md).

### 🎯 Cilj ovog Issuea
**Jinn OS** je minimalistički, preemptivni operacijski sustav s arhitekturom **x86_64 mikrokernela**, napisan od nule u modernom programskom jeziku Rust (`#![no_std]`, bare-metal). Sustav primjenjuje strogo načelo najmanjih mogućih privilegija: samo se kritične operacije izvršavaju u **Ring 0** načinu rada, dok upravljački programi uređaja, datotečni sustavi i grafički kompozitor **Aladdin Desktop** djeluju izolirano u korisničkom prostoru (**Ring 3**).

Želimo pozvati hrvatske i regionalne softverske inženjere, programere sustava niske razine i ljubitelje Rusta da se pridruže projektu.

### 🔍 Kako možete pomoći?
1. **Pregled Tehničkog Nazivlja i Lektura:**
   - Provjera točnosti i prirodnosti prijevoda koncepata operacijskih sustava (*4-razinsko straničenje PML4*, *sigurnost W^X*, *preempcija PIT 100Hz*, *IPC bez zaključavanja*, *transakcijska VFS zaštita*, *Aladdin Desktop grafički poslužitelj u Ring 3*).
   - Prijava tipfelera i prijedlozi za poboljšanje stila i jasnoće.
2. **Razvoj Kernela i Korisničkog Prostora:**
   - Pisanje i testiranje upravljačkih programa za hardver (VirtIO, Intel E1000, NVMe, AHCI, USB).
   - Proširenje korisničkih aplikacija (izvorni grafički preglednik **Jinn Browser**, interaktivni terminal **Aladdin Shell**, nekustodijalni novčanik **Aladdin Wallet**).
   - Testiranje performansi na stvarnom hardveru i u virtualnim okruženjima (QEMU, VirtualBox).
3. **Pitanja, Prijedlozi i Rasprava:**
   - Ostavite komentar ispod s vašim idejama, arhitektonskim prijedlozima ili pitanjima!

### 📚 Prvi Koraci
- Pročitajte dokumentaciju: [`README.hr.md`](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/README.hr.md)
- Smjernice za doprinos: [`CONTRIBUTING.md`](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/CONTRIBUTING.md)
- Ugovor o licenci suradnika: [`CLA.md`](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/CLA.md)

Hvala vam na podršci u izgradnji suverenog, transparentnog i sigurnog računalnog temelja u Rustu!

