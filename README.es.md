# Jinn OS — Microkernel x86_64 Minimalista y Preemptivo en Rust

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

> **Estado:** En desarrollo activo (Alpha v0.0.3)  
> **Arquitectura:** `x86_64` | **Lenguaje:** Rust (`#![no_std]`, bare-metal, target `x86_64-unknown-none`)  
> **Cargador de Arranque:** Limine Boot Protocol v2 (Higher-Half)  
> **Comunidad:** Desarrollado en Brasil con enfoque en soberanía computacional, seguridad en profundidad y simplicidad radical.

---

## Sobre el Proyecto

**Jinn OS** es un sistema operativo microkernel desarrollado desde cero en Rust moderno. El proyecto nace de la necesidad de construir una base de cómputo confiable, minimalista y formalmente verificable, donde solo lo estrictamente indispensable reside en el nivel más privilegiado de la CPU (**Ring 0**).

A diferencia de los kernels monolíticos convencionales, en Jinn OS todos los subsistemas complejos —como los controladores de dispositivos de red, los sistemas de archivos, el servidor gráfico **Aladdin Desktop** y aplicaciones como el navegador **Jinn Browser**— se ejecutan como procesos aislados en espacio de usuario (**Ring 3**). La integridad del sistema está garantizada por protección de hardware, paginación estricta y comunicación interprocesos (IPC) de alta velocidad y libre de bloqueos (*lock-free*).

---

## Pilares Arquitectónicos e Hitos (Alpha v0.0.3)

1. **Preempción Real y Planificación Multitarea:**
   - Planificador Round-Robin preemptivo gobernado por el temporizador PIT (100 Hz / cuanto de 10 ms) e integración con el temporizador local APIC.
   - Procesamiento desacoplado de interrupciones (IRQs) y señalización explícita de fin de interrupción (EOI) en el LAPIC para prevenir la saturación del bus de eventos.

2. **Aislamiento Estricto de Memoria (PML4 x86_64 y Seguridad W^X):**
   - Tablas de páginas de 4 niveles con gestión canónica estricta del espacio de direcciones virtuales.
   - Procesos de Ring 3 mapeados en direcciones seguras de usuario (`>= 0x400000`), sin acceso directo a memoria física ni a estructuras de control de Ring 0.
   - Aplicación rigurosa de la política W^X (*Write XOR Execute*) en todas las páginas de memoria dinámica.

3. **Mínimo Privilegio e Interfaz Canónica de Syscalls:**
   - Ring 0 restringido exclusivamente al cambio de contexto, asignación de marcos de páginas físicas, despacho de interrupciones de hardware e interfaz canónica de llamadas al sistema (Syscalls `0..85` mediante `syscall`/`sysret` con respaldo compatible `int 0x80`).
   - El microkernel nunca almacena, procesa ni manipula claves privadas ni credenciales criptográficas.

4. **Paso de Mensajes sin Bloqueos (IPC Lock-Free):**
   - Buzón de mensajes estático y acotado por proceso, sin asignaciones dinámicas de memoria en la ruta crítica de despacho.

5. **Sistema de Archivos Virtual (VFS) con Persistencia Transaccional:**
   - Soporte nativo y simultáneo para **FAT32** (`/disk`) y **Ext2**.
   - Subsistema de **BlockCache** con política de desalojo Least Recently Used (LRU) para optimizar el I/O de disco.
   - Modos y banderas de apertura estándar POSIX (`O_RDONLY`, `O_WRONLY`, `O_RDWR`, `O_CREAT`, `O_APPEND`, `O_TRUNC`).
   - Capa de integridad transaccional rotacional (`.TMP` -> `.BAK`) para proteger la caché, historial, marcadores y cookies de aplicaciones Ring 3 frente a cortes repentinos de energía.

6. **Entorno Gráfico Aladdin Desktop en Ring 3:**
   - Servidor gráfico y compositor de ventanas independiente que se ejecuta íntegramente en espacio de usuario (`apps/aladdin-server`).
   - Renderizado sobre Linear Framebuffer de alta resolución provisto por Limine.
   - Controlador de ratón PS/2 con ingestión desacoplada de paquetes, rastreo de cursor en tiempo real y despacho de eventos de ventana (`Focus`, `Drag`, `Resize`, `Close`).

7. **Pila de Red Soberana, JinnWall SPI y Criptografía PKI:**
   - Pila de red bare-metal IPv4, TCP, UDP, ARP, DHCP y DNS con soporte para adaptadores VirtIO-Net e Intel E1000.
   - Cortafuegos de inspección de paquetes con estado (**JinnWall SPI**).
   - Validación nativa de certificados **PKI X.509** y apretón de manos seguro **TLS 1.3** (RFC 8446) con derivación de claves HKDF-SHA256/384.

8. **Aplicaciones Nativas en Ring 3:**
   - **Jinn Browser:** Motor de navegación web gráfico sin dependencias de bibliotecas C, con renderizador HTML/CSS, cliente HTTP, caché local, marcadores e historial persistente almacenado en `/disk/browser/`.
   - **Aladdin Shell:** Terminal interactivo con utilidades de diagnóstico del sistema (`sysinfo`, `vfs`, `net`, `ps`, `mem`).
   - **Aladdin Wallet:** Billetera Web3 no custodial con derivación determinista de claves mnemónicas BIP-39/BIP-44, totalmente aislada en Ring 3.

---

## Estructura del Repositorio

```text
jinn/
├── Cargo.toml            # Manifiesto de la crate principal del microkernel
├── limine.conf           # Configuración del cargador de arranque Limine v2
├── build.rs              # Script de compilación y enlazado del kernel
├── apps/                 # Aplicaciones y servicios nativos de espacio de usuario (Ring 3)
│   ├── aladdin-server/   # Servidor gráfico Aladdin Desktop (Compositor Ring 3)
│   └── ola-mundo/        # Jinn Browser (browser.rs) y Aladdin Wallet (wallet.rs)
├── sdk/                  # Aladdin SDK y bibliotecas cliente para Ring 3
│   └── aladdin-sdk/      # Envoltorios de Syscalls, superficies gráficas, eventos e IPC
├── contracts/            # Smart contracts BEP-20 / EVM y suite de pruebas Hardhat
│   ├── contracts/        # JinnToken.sol, JinnVesting.sol, JinnRewardDistributor.sol
│   ├── test/             # Pruebas unitarias e invariantes de seguridad en EVM
│   └── scripts/          # Automatización de despliegue para BSC Testnet/Mainnet
├── tools/                # Utilidades de desarrollo y arneses de simulación
│   ├── jinn-web3/        # jinn-web3 (CLI) y jinn-web3d (Daemon en Ring 3)
│   └── jda-sim/          # Simulador de tuberías DAG y benchmarks SMP
├── web/                  # Portal web oficial de Jinn OS (jinn-os.dev.br / GitHub Pages)
├── docs/                 # Especificaciones técnicas, ADRs e informes de auditoría
└── src/                  # Código fuente principal del microkernel (Ring 0)
    ├── main.rs           # Punto de entrada (_start) e inicialización del kernel
    ├── gdt.rs            # Global Descriptor Table y TSS (Ring 0 / Ring 3)
    ├── idt.rs            # Interrupt Descriptor Table y manejadores ISR/IRQ
    ├── syscall.rs        # Despachador canónico de llamadas al sistema (Syscalls 0..85)
    ├── scheduler.rs      # Planificador multitarea preemptivo
    ├── ipc.rs            # Mensajería estática libre de bloqueos entre procesos
    ├── memory/           # Asignador de marcos físicos (Buddy), Heap y PML4
    ├── drivers/          # Controladores (PIT, LAPIC, Teclado, Ratón PS/2, IDE, PCI, VirtIO, E1000)
    ├── net/              # Pila TCP/IP, JinnWall SPI, TLS 1.3 y PKI X.509
    └── vfs/              # Virtual File System (FAT32, Ext2, BlockCache LRU)
```

---

## Cómo Compilar y Ejecutar

### Requisitos Previos
- **Toolchain de Rust:** Compilador `nightly` con el target `x86_64-unknown-none` instalado.
- **Emulación y Herramientas:** `qemu-system-x86_64`, PowerShell 7+ (Windows/Linux) o Python 3.

```bash
# Agregar el target bare-metal de Rust
rustup target add x86_64-unknown-none
```

### 1. Compilación del Microkernel y Aplicaciones
Ejecute el script de compilación automatizado para compilar el kernel, generar los binarios de Ring 3 y estructurar la imagen de disco FAT32:

```powershell
# Desde la raíz del repositorio
.\scripts\build.ps1
```

### 2. Ejecución en QEMU
Inicie la máquina virtual simulando la plataforma x86_64 con emulación de periféricos:

```powershell
# Ejecución estándar de Jinn OS con Aladdin Desktop y red
.\scripts\run.ps1

# Prueba de homologación bajo memoria estricta (ejemplo: prueba de estrés con 64MB RAM):
.\scripts\run.ps1 -Memory 64M
```

---

## Ecosistema Web3 y Jinn Open Economy ($JINN)

Para garantizar la sostenibilidad económica soberana a largo plazo, financiar la investigación en sistemas operativos y recompensar a los colaboradores de código abierto mediante *Proof of Useful Contribution* (PoUC), el proyecto integra la infraestructura del token nativo **$JINN** en BNB Smart Chain (BSC).

> [!IMPORTANT]
> **Aislamiento Inviolable de Credenciales:** El microkernel Jinn (Ring 0) nunca accede, procesa ni almacena claves privadas, frases semilla mnemónicas ni credenciales financieras. Todas las operaciones criptográficas residen estrictamente en el espacio de usuario (Ring 3), dentro de la aplicación **Aladdin Wallet**.

- **Propuesta Económica y Whitepaper:** [`JINN_TOKEN_PROPOSAL.md`](JINN_TOKEN_PROPOSAL.md)
- **Informe de Validación EVM y Auditoría Fase II:** [`docs/JINN_WEB3_PHASE2_AUDIT_REPORT.md`](docs/JINN_WEB3_PHASE2_AUDIT_REPORT.md)
- **Gobernanza y Tesorería Multi-Sig:** [`docs/JINN_OPEN_ECONOMY_GOVERNANCE.md`](docs/JINN_OPEN_ECONOMY_GOVERNANCE.md)
- **Especificación de PoUC (Recompensas):** [`docs/PROOF_OF_USEFUL_CONTRIBUTION_SPEC.md`](docs/PROOF_OF_USEFUL_CONTRIBUTION_SPEC.md)
- **Arquitectura de la Billetera (Ring 3):** [`docs/JINN_WALLET_ARCHITECTURE.md`](docs/JINN_WALLET_ARCHITECTURE.md)
- **Aladdin Wallet (Código Ring 3 UI):** [`apps/ola-mundo/src/wallet.rs`](apps/ola-mundo/src/wallet.rs)
- **Daemon de Espacio de Usuario `jinn-web3d`:** [`tools/jinn-web3/src/daemon.rs`](tools/jinn-web3/src/daemon.rs)
- **Economía Computacional DePIN (JDA):** [`docs/JDA_DEPIN_COMPUTATIONAL_ECONOMY.md`](docs/JDA_DEPIN_COMPUTATIONAL_ECONOMY.md)
- **Auditoría Interna de Smart Contracts:** [`docs/SMART_CONTRACTS_AUDIT_REPORT.md`](docs/SMART_CONTRACTS_AUDIT_REPORT.md)
- **Guía de Despliegue en Testnet:** [`docs/TESTNET_DEPLOYMENT_GUIDE.md`](docs/TESTNET_DEPLOYMENT_GUIDE.md)

---

## Licencia y Gobernanza

**Jinn OS** se distribuye bajo un modelo de **Licenciamiento Dual (Dual Licensing)**:

1. **Licencia Abierta / Comunitaria (`GPL-2.0-only`):** Dirigida a la comunidad de código abierto, investigadores y entusiastas. El uso y las obras derivadas son libres bajo los términos de la GNU General Public License versión 2 ([`LICENSES/GPL-2.0-only.txt`](LICENSES/GPL-2.0-only.txt)).
2. **Licencia Comercial Propietaria (`Jinn-Commercial-1.0`):** Destinada al uso corporativo, embebido e industrial en dispositivos cerrados o productos comerciales que requieren exención de las obligaciones copyleft de la GPL ([`LICENSES/Jinn-Commercial-1.0.txt`](LICENSES/Jinn-Commercial-1.0.txt)).

Para consultar los términos legales completos, revise [`LICENSE`](LICENSE). Las contribuciones de código requieren la firma previa del [Acuerdo de Licencia de Colaborador (`CLA.md`)](CLA.md) y el cumplimiento de las [Pautas de Contribución (`CONTRIBUTING.md`)](CONTRIBUTING.md).

---

## Contacto y Contribuciones

Agradecemos e impulsamos la colaboración de la comunidad hispana e internacional de desarrolladores Rust, programadores de bajo nivel y entusiastas de la ingeniería de sistemas. ¡Sus contribuciones en código, controladores de hardware, auditorías de seguridad y documentación son siempre bienvenidas! Consulte [CONTRIBUTING.md](CONTRIBUTING.md) para enviar sus parches y pull requests.
