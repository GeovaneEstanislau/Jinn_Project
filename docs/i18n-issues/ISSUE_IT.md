# Title: 🇮🇹 Chiamata ai contributori e revisione tecnica: Comunità Italiana (README.it.md)
# Labels: documentation, i18n, help wanted, good first issue

Benvenuti nella community open-source di **Jinn OS**!

Abbiamo pubblicato la versione ufficiale in lingua italiana della documentazione tecnica centrale: [**`README.it.md`**](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/README.it.md).

### 🎯 Obiettivo di questo Issue
**Jinn OS** è un sistema operativo con architettura **microkernel x86_64** minimale e preemptive, scritto interamente da zero in Rust moderno (`#![no_std]`, bare-metal). Il sistema applica il principio del minimo privilegio: solo i meccanismi fondamentali risiedono in **Ring 0**, mentre i driver di periferica, i file system e il window compositor **Aladdin Desktop** operano isolati nello spazio utente (**Ring 3**).

Desideriamo coinvolgere la comunità italiana di sviluppatori software, esperti di programmazione a basso livello e appassionati del linguaggio Rust.

### 🔍 Come puoi contribuire?
1. **Revisione della Terminologia Tecnica:**
   - Verificare che i concetti architetturali (*paginazione a 4 livelli PML4*, *politica W^X*, *preemption PIT a 100Hz*, *IPC lock-free*, *persistenza transazionale VFS*, *compositor grafico Aladdin Desktop in Ring 3*) siano tradotti con la massima accuratezza tecnica.
   - Segnalare refusi o suggerire miglioramenti stilistici.
2. **Sviluppo del Kernel e delle Applicazioni Ring 3:**
   - Implementazione e test di nuovi driver hardware (VirtIO, Intel E1000, NVMe, AHCI, USB).
   - Espansione delle applicazioni utente (motore di rendering **Jinn Browser**, terminale interattivo **Aladdin Shell**, portafoglio **Aladdin Wallet**).
   - Test di stabilità su hardware reale o ambienti virtualizzati (QEMU, VirtualBox).
3. **Discussione e Proposte:**
   - Lascia un commento qui sotto per porre domande, suggerire nuove funzionalità o discutere le scelte architetturali!

### 📚 Primi Passi
- Leggi la documentazione: [`README.it.md`](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/README.it.md)
- Linee guida per i contributi: [`CONTRIBUTING.md`](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/CONTRIBUTING.md)
- Accordo di licenza per i contributori: [`CLA.md`](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/CLA.md)

Grazie per il supporto nella costruzione di una base di calcolo sovrana, sicura e moderna in Rust!
