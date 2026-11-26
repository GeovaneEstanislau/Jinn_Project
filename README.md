# Jinn OS — Arquitetura Microkernel Experimental para Computação Eficiente, Isolada e Heterogênea

> **EN:** [README.en.md](README.en.md) | **Observatory:** [jinn-os.dev.br](https://jinn-os.dev.br)  
> **Ecossistema Modular:** [`Jinn_Project`](https://github.com/GeovaneEstanislau/Jinn_Project) · [`Aladdin`](https://github.com/GeovaneEstanislau/Aladdin) · [`jhcs-rs`](https://github.com/GeovaneEstanislau/jhcs-rs)

<div align="center">

| Idioma | Arquivo | Idioma | Arquivo |
| :--- | :---: | :--- | :---: |
| 🇧🇷 **Português** | [README.md](README.md) | 🇮🇳 **Hindi** | [README.hi.md](README.hi.md) |
| 🇺🇸 **English** | [README.en.md](README.en.md) | 🇮🇹 **Italiano** | [README.it.md](README.it.md) |
| 🇨🇳 **中文** | [README.zh.md](README.zh.md) | 🇭🇺 **Magyar** | [README.hu.md](README.hu.md) |
| 🇯🇵 **日本語** | [README.ja.md](README.ja.md) | 🇭🇷 **Hrvatski** | [README.hr.md](README.hr.md) |
| 🇪🇸 **Español** | [README.es.md](README.es.md) | 🇳🇴 **Norsk** | [README.no.md](README.no.md) |
| 🇷🇺 **Русский** | [README.ru.md](README.ru.md) | 🇩🇪 **Deutsch** | [README.de.md](README.de.md) |

</div>

> **Posição Técnica:** Arquitetura microkernel experimental em Rust bare-metal (`x86_64`, `#![no_std]`) para pesquisa e engenharia verificável de computação **eficiente**, **isolada em Ring 3** e **heterogênea**.  
> **Autoria:** Geovane da Conceição Estanislau | **Status:** Alpha v0.0.3 (`x86_64-unknown-none`, Limine v2)

---

## Tese Arquitetural e Ativos Transferíveis

O **Jinn** não se posiciona como *"apenas mais um sistema operacional para competir frontalmente com Linux ou Windows"*. O projeto é estruturado como um **laboratório integrado verificável**: o microkernel valida ponta a ponta invariantes rígidos de memória, interrupções e isolamento, enquanto seus componentes modulares constituem ativos transferíveis independentes:

| Ativo / Subsistema | Repositório / Módulo | Papel Técnico e Evidência Reproduzível |
| :--- | :--- | :--- |
| **Microkernel Minimalista (`Ring 0`)** | [`src/`](src/) | Escalonador preemptivo SMP per-CPU, paginação PML4 4 níveis com `W^X`, `IrqSafeSpinLock` em locks de ISR, EOI incondicional no PIC 8259A, IPC estático em `.bss`. |
| **Política *Zero-Idle* (`0.00%` ocioso)** | `src/scheduler.rs`, `src/smp.rs` | Ociosidade orientada a eventos via `HLT` / `wait_on` e ausência de timers de limpeza em segundo plano (`0.00%` CPU em núcleos ociosos sob QEMU `-smp 4`). |
| **Contabilidade Física JAHA 1.2** | `src/memory/`, [`scripts/test_jaha_12_integrity.py`](scripts/test_jaha_12_integrity.py) | Contabilidade física determinística (*no-overcommit*) e envelope validado em `64 MiB`–`256 MiB` com invariante de $\Delta = 0\text{ B}$ em testes E2E. |
| **`Aladdin` & `AladdinSharedRings`** | [`Aladdin`](https://github.com/GeovaneEstanislau/Aladdin), [`sdk/aladdin-sdk/`](sdk/aladdin-sdk/) | Compositor fora do kernel em **Ring 3**, anéis SPSC *lock-free* / *zero-copy* para framebuffer e entrada, `DirtyRect` e composição acrílica SIMD CPU. |
| **Rede Soberana, DNS 3 Camadas & `HotRouteTable`** | `src/net/` (`dns.rs`, `hot_routes.rs`, `tls13.rs`), `apps/ola-mundo/` | Tabela DNS local (Hosts $\to$ Cache TTL *lazy* RAM/FAT32 $\to$ Upstream), cache unificado L2/L3 `HotRouteTable`, firewall JinnWall SPI e **TLS 1.3** RFC 8446 `#![no_std]`. |
| **Executor Heterogêneo `jhcs-rs`** | [`jhcs-rs`](https://github.com/GeovaneEstanislau/jhcs-rs), `apps/jhcs-daemon/` | Agendador DAG portável para Linux/Windows e Ring 3 selecionando CPU escalar, SIMD (`AVX2`/`FMA`) ou GPU (`wgpu`) com base no ponto de equilíbrio (*breakeven*). |

> [!NOTE]
> Para conhecer exatamente a linha entre o que está comprovado por testes automatizados e o que permanece em fase experimental (ex.: medição elétrica física em silício vs. QEMU TCG, portabilidade ARM64/RISC-V), consulte [`docs/NLNET_GRANT_APPLICATION_PROPOSAL.md`](docs/NLNET_GRANT_APPLICATION_PROPOSAL.md) e os relatórios em [`docs/`](docs/). Artefatos históricos de experimentos Web3 foram movidos para [`docs/archive/`](docs/archive/README.md).

---

## Estrutura do Repositório

```text
Jinn_Project/
├── src/                      # Microkernel Ring 0 (#![no_std], Rust x86_64)
│   ├── memory/               # Buddy allocator, heap e paginação PML4 W^X (JAHA 1.2)
│   ├── interrupts/           # IDT, PIC 8259A (EOI incondicional) e LAPIC
│   ├── scheduler.rs, smp.rs  # Escalonamento SMP per-CPU com política Zero-Idle (HLT)
│   ├── net/                  # IPv4/TCP/UDP, dns.rs (3 camadas), hot_routes.rs, tls13.rs, jinnwall
│   ├── vfs/                  # FAT32 (/disk, LFN, escrita transacional .TMP/.BAK) e Ext2
│   └── syscall.rs            # ABI canônica (syscall/sysret + int 0x80)
├── apps/
│   ├── aladdin-server/       # Integração Ring 3 com o compositor Aladdin
│   ├── ola-mundo/            # Jinn Browser (HTTP/HTTPS TLS 1.3, HTML/CSS, RSS/JSON) & shell
│   └── jhcs-daemon/          # Serviço de computação heterogênea Ring 3
├── sdk/aladdin-sdk/          # AladdinSharedRings (SPSC zero-copy) e wrappers de syscalls
├── scripts/                  # Build (build.ps1, run.ps1) e suítes E2E QEMU (e1000, -smp 4)
├── web/                      # Jinn Engineering Observatory (jinn-os.dev.br) e feeds RSS/Atom
└── docs/                     # Especificações técnicas, auditorias, estratégia e archive/
```

---

## Compilação e Execução Verificável

### Pré-requisitos
- Rust `nightly` com target `x86_64-unknown-none` (`rustup target add x86_64-unknown-none`)
- `qemu-system-x86_64`, PowerShell 7+ e Python 3 (para geração de disco FAT32/Ext2 e testes E2E)

```powershell
# 1. Compilar kernel, apps Ring 3 (incluindo Aladdin), disk.img e jinn.iso
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\run.ps1 -NoRun

# 2. Executar o Jinn OS no QEMU (SMP 4 núcleos, Intel E1000)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\run.ps1 -Smp 4

# 3. Suítes de validação reproduzível (exemplos)
python scripts\test_hot_routes_e2e.py
python scripts\test_dns_cache_e2e.py
python scripts\test_jaha_12_integrity.py
```

---

## Licenciamento e Governança

O **`Jinn_Project`** segue o modelo de **Duplo Licenciamento** ([`LICENSE`](LICENSE)):
1. **Comunitário / Pesquisa:** `GPL-2.0-only` ([`LICENSES/GPL-2.0-only.txt`](LICENSES/GPL-2.0-only.txt)).
2. **Comercial / OEM:** `Jinn-Commercial-1.0` ([`LICENSES/Jinn-Commercial-1.0.txt`](LICENSES/Jinn-Commercial-1.0.txt)), mediante assinatura de [`CLA.md`](CLA.md) para contribuições externas.

Os repositórios irmãos operam sob licenças permissivas explícitas para facilitar adoção modular:
- **`Aladdin`**: `MIT OR Apache-2.0`
- **`jhcs-rs`**: `Apache-2.0` (consulte [`jhcs-rs/docs/LICENSING_POLICY.md`](../jhcs-rs/docs/LICENSING_POLICY.md))

Contribuições de arquitetura, hardware de teste e auditorias de segurança são bem-vindas conforme [`CONTRIBUTING.md`](CONTRIBUTING.md).
