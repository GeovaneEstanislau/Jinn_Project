# Inventário de Dependências e Licenças de Terceiros — Jinn OS

Este documento formaliza o inventário completo de dependências, componentes externos, ferramentas de inicialização e fontes tipográficas integradas ao **Jinn OS**, categorizados por camada arquitetural (L0 a L3) e respectivo regime de licenciamento.

---

## 1. Diretriz de Governança e Limites Arquiteturais

Conforme estabelecido no [Guia de Contribuição](CONTRIBUTING.md) e na [Política de Licenciamento](LICENSE):
- **L0 (Kernel Core):** Código próprio com isolamento total; nenhuma dependência externa com licença copyleft (GPL, LGPL) é permitida no núcleo.
- **L1 (System Services) e L2 (Device Drivers):** Serviços de sistema executados em Ring 0 ou Ring 3 com comunicação estrita via IPC. Dependências de terceiros somente com licenças permissivas aprovadas (`MIT`, `Apache-2.0`, `BSD-2-Clause`, `BSD-3-Clause`).
- **L3 (Aplicações e Desktop):** Ambiente gráfico Aladdin Desktop e binários de usuário em Ring 3.

---

## 2. Inventário de Componentes por Camada

| Camada | Componente / Subdiretório | Descrição / Função | Licença | Origem / Titularidade |
| :--- | :--- | :--- | :--- | :--- |
| **L0** | `src/` (Microkernel Core) | Escalonador, paginação PML4, IPC, syscalls, interrupções | `GPL-2.0-only OR Jinn-Commercial-1.0` | Jinn Project |
| **L0** | `src/font.rs` | Tabela de glifos bitmap 8×16 para saída de texto VGA | Domínio Público / CC0 | IBM VGA BIOS / ASCII Padrão |
| **L1** | `src/vfs/`, `src/net/` | Virtual Filesystem (FAT32, Ext2, ramfs), pilha de rede IPv4/TCP/UDP/DNS | `GPL-2.0-only OR Jinn-Commercial-1.0` | Jinn Project |
| **L2** | `src/drivers/` | Drivers de hardware (IDE/ATA, VirtIO-Net, PCI, PS/2 Mouse/Teclado) | `GPL-2.0-only OR Jinn-Commercial-1.0` | Jinn Project |
| **L3** | `aladdin` (`../Aladdin`) | Compositor gráfico, gerenciador de janelas e apps do desktop | `MIT OR Apache-2.0` | Jinn OS Contributors |
| **L3** | `apps/ola-mundo/` | Binário multi-propósito de usuário Ring 3 (browser, shell, workers) | `GPL-2.0-only OR Jinn-Commercial-1.0` | Jinn Project |
| **Boot**| `limine/` | Carregador de inicialização x86_64 UEFI/BIOS (Limine Bootloader) | `BSD-2-Clause` | Limine Contributors |
| **Tools**| `tools/elf-tests/` | Suíte de testes de validação de segurança de cabeçalhos e imagens de disco | `MIT OR Apache-2.0` | Jinn Project |
| **Tools**| `tools/net-tests/` | Suíte de testes e validação de conformidade RFC 793 (TCP) e RFC 1035 (DNS) | `MIT OR Apache-2.0` | Jinn Project |
| **Tools**| `tools/jda-sim/` | Simulador e benchmark de fluxos assíncronos io_uring JDA | `MIT OR Apache-2.0` | Jinn Project |

---

## 3. Detalhamento das Licenças de Terceiros

### 3.1. Limine Bootloader
- **Arquivos:** `limine/limine-bios-cd.bin`, `limine/limine-bios.sys`, `limine/BOOTX64.EFI`, `limine/usr/bin/limine.exe`
- **Licença:** BSD 2-Clause License
- **Texto da Licença:**
```text
Copyright (c) 2019-2024 Limine Contributors

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

### 3.2. Aladdin Desktop (`aladdin`)
- **Repositório:** `https://github.com/GeovaneEstanislau/Aladdin`
- **Licença:** `MIT OR Apache-2.0` (Dual license permissiva)
- **Status:** Integrado como subcomponente de Ring 3 e composição visual sob a PAL (`src/gui.rs`).

---

## 4. Ausência de Dependências Externas em Tempo de Compilação
O microkernel Jinn OS foi projetado com dependência zero de crates externas do ecossistema `crates.io`. Toda a compilação baseia-se exclusivamente nos componentes fundamentais da linguagem Rust:
- `core`: Tipos primitivos, semântica de memória, operações atômicas e ponteiros (`#![no_std]`).
- `alloc`: Alocador dinâmico de memória (Vec, Box, String) atendido pelo Heap interno do kernel (`src/memory/heap.rs`).
- `compiler-builtins`: Primitivas aritméticas de 128 bits e operações de memória (`memcpy`, `memset`).

Isso garante auditabilidade estrita, ausência de vetores de ataque por cadeia de suprimentos (*supply chain attacks*) e soberania técnica total sobre o código-fonte executado no microkernel.

