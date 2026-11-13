# Title: 🇭🇺 Közreműködők toborzása és műszaki lektorálás: Magyar közösség (README.hu.md)
# Labels: documentation, i18n, help wanted, good first issue

Üdvözöljük a **Jinn OS** nyílt forráskódú közösségében!

Megjelent a Jinn OS központi műszaki dokumentációjának hivatalos magyar nyelvű változata: [**`README.hu.md`**](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/README.hu.md).

### 🎯 Az Issue célja
A **Jinn OS** egy modern Rust nyelven, alapjaitól felépített minimalista és preemptív **x86_64 mikrokernelt** operációs rendszer (`#![no_std]`, bare-metal). A legkisebb jogosultság elvét követve a legmagasabb kiváltsági szinten (**Ring 0**) kizárólag a legszükségesebb alapmechanizmusok futnak, míg az eszközmeghajtók, a fájlrendszerek és az **Aladdin Desktop** ablakkompozitor elszeparált felhasználói térben (**Ring 3**) üzemelnek.

Szeretnénk megszólítani és bevonni a magyar rendszermérnököket, alacsony szintű programozókat és Rust fejlesztőket a projekt formálásába!

### 🔍 Miben segíthet?
1. **Műszaki szakkifejezések és szöveg ellenőrzése:**
   - Annak ellenőrzése, hogy az operációs rendszer alapfogalmai (*4 szintű lapozás PML4*, *W^X memóriavédelem*, *PIT 100Hz preemptív ütemezés*, *zármentes IPC*, *tranzakciós VFS mentés*, *Ring 3 Aladdin Desktop ablakkompozitor*) szabatosan és világosan jelennek-e meg magyarul.
   - Esetleges elírások jelzése, pontosítási javaslatok.
2. **Kernel- és felhasználói alkalmazásfejlesztés:**
   - Hardver-illesztőprogramok írása és tesztelése (VirtIO, Intel E1000, NVMe, AHCI, USB).
   - Ring 3 natív alkalmazások fejlesztése (grafikus böngésző **Jinn Browser**, interaktív terminál **Aladdin Shell**, nem-letétkezelői pénztárca **Aladdin Wallet**).
   - Rendszertesztek végzése valós hardveren és virtualizált környezetben (QEMU, VirtualBox).
3. **Véleménycsere és ötletek:**
   - Szóljon hozzá bátran ehhez az Issue-hoz kérdéseivel, építészeti felvetéseivel vagy ötleteivel!

### 📚 Első lépések
- Olvassa el a dokumentációt: [`README.hu.md`](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/README.hu.md)
- Közreműködési irányelvek: [`CONTRIBUTING.md`](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/CONTRIBUTING.md)
- Közreműködői licencmegállapodás: [`CLA.md`](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/CLA.md)

Köszönjük a segítségét egy biztonságos, szuverén és modern számítástechnikai alap kiépítésében Rust nyelven!
