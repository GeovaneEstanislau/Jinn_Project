# Title: 🇩🇪 Aufruf zur Mitarbeit und Übersetzungskontrolle: Deutschsprachige Community (README.de.md)
# Labels: documentation, i18n, help wanted, good first issue

Herzlich willkommen in der **Jinn OS** Community!

Wir haben die offizielle deutschsprachige Version unserer zentralen technischen Dokumentation veröffentlicht: [**`README.de.md`**](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/README.de.md).

### 🎯 Ziel dieses Issues
**Jinn OS** ist ein hochgradig minimalistisches, präemptives **x86_64-Mikrokernel**-Betriebssystem, das vollständig von Grund auf in modernem Rust entwickelt wurde (`#![no_std]`, Bare-Metal). Das System erzwingt strikte Privilegienbegrenzung: Lediglich essenzielle Basismechanismen verbleiben in **Ring 0**, während Netzwerktreiber, Dateisysteme und der Fensterkompositor **Aladdin Desktop** isoliert im Benutzerraum (**Ring 3**) arbeiten.

Wir laden Entwickler, Low-Level-Programmierer, Rustaceans und Forscher aus dem gesamten deutschsprachigen Raum herzlich ein, das Projekt kennenzulernen und aktiv mitzugestalten!

### 🔍 Wie Sie beitragen können
1. **Prüfung von Fachterminologie und Textqualität:**
   - Sicherstellen, dass systemspezifische Konzepte (*vierstufige Paginierung PML4*, *W^X-Speichersicherheit*, *PIT 100Hz Präemption*, *lock-freie IPC*, *transaktionale VFS-Persistenz*, *Ring-3 Aladdin Desktop Display-Server*) präzise und fachgerecht formuliert sind.
   - Melden von Tippfehlern und Vorschläge für noch präzisere Beschreibungen.
2. **Entwicklung von Kernel- und User-Space-Modulen:**
   - Treiberentwicklung und Hardware-Anpassung (VirtIO, Intel E1000, NVMe, AHCI, USB).
   - Weiterentwicklung nativer Ring-3-Applikationen (Grafischer Webbrowser **Jinn Browser**, interaktives Terminal **Aladdin Shell**, Web3-Wallet **Aladdin Wallet**).
   - Stresstests auf physischer Hardware und unter Virtualisierungsplattformen (QEMU, VirtualBox).
3. **Austausch und Architekturdiskussion:**
   - Nutzen Sie diesen Issue gern für Fragen, Anregungen oder Diskussionen zu Designentscheidungen!

### 📚 Erste Schritte
- Dokumentation lesen: [`README.de.md`](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/README.de.md)
- Beitragsrichtlinien: [`CONTRIBUTING.md`](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/CONTRIBUTING.md)
- Mitwirkendenvereinbarung: [`CLA.md`](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/CLA.md)

Vielen Dank für Ihre Unterstützung beim Aufbau eines transparenten, souveränen und sicheren Softwarefundaments in Rust!

