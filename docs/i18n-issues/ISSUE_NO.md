# Title: 🇳🇴 Invitasjon til bidragsytere og faglig korrektur: Norsk fellesskap (README.no.md)
# Labels: documentation, i18n, help wanted, good first issue

Velkommen til åpen kildekode-fellesskapet for **Jinn OS**!

Vi har publisert den offisielle norskspråklige utgaven av prosjektets sentrale tekniske dokumentasjon: [**`README.no.md`**](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/README.no.md).

### 🎯 Formål med dette Issue
**Jinn OS** er et minimalistisk og preemptivt operativsystem basert på en **x86_64-mikrokjerne**, skrevet fra bunnen av i moderne Rust (`#![no_std]`, bare-metal). Systemet håndhever prinsippet om minste privilegium: Kun strengt nødvendige operasjoner kjøres i **Ring 0**, mens nettverksdrivere, filsystemer og vinduskompositoren **Aladdin Desktop** kjører fullstendig isolert i brukerrommet (**Ring 3**).

Vi ønsker å invitere utviklere, systemprogrammerere, Rustaceans og teknologientusiaster fra det nordiske fagmiljøet til å bidra!

### 🔍 Hvordan du kan bidra
1. **Faglig Gjennomgang og Språkvask:**
   - Kvalitetssikre at datatekniske begreper (*4-nivåers sidetabell PML4*, *W^X minnesikkerhet*, *PIT 100Hz preemptering*, *låsfri IPC*, *transaksjonell VFS-bevaring*, *Aladdin Desktop vinduskompositor i Ring 3*) er oversatt presist og idiomatisk til norsk.
   - Melde fra om skrivefeil eller foreslå språklige forbedringer.
2. **Videreutvikling av Kjerne og Brukerprogrammer:**
   - Skrive og teste maskinvaredrivere (VirtIO, Intel E1000, NVMe, AHCI, USB).
   - Bidra til Ring 3-applikasjoner (den grafiske nettleseren **Jinn Browser**, terminalen **Aladdin Shell**, Web3-lommeboken **Aladdin Wallet**).
   - Teste operativsystemet på fysisk maskinvare eller i virtualiseringsmiljøer (QEMU, VirtualBox).
3. **Erfaringsutveksling og Idéer:**
   - Legg gjerne igjen en kommentar nedenfor med spørsmål, arkitekturforslag eller tilbakemeldinger!

### 📚 Kom i Gang
- Les dokumentasjonen: [`README.no.md`](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/README.no.md)
- Retningslinjer for bidrag: [`CONTRIBUTING.md`](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/CONTRIBUTING.md)
- Bidragsyteravtale: [`CLA.md`](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/CLA.md)

Hjertelig takk for at du hjelper oss med å bygge et suverent, transparent og sikkert softwarefundament i Rust!
