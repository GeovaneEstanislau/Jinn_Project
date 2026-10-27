# 🗺️ Mapa de Desenvolvimento (Roadmap) — Jinn OS

> **Jinn OS**: Microkernel preemptivo x86_64 minimalista escrito em Rust.
> Focado em alto desempenho, baixo overhead, controle explícito de recursos e isolamento rigoroso entre Kernel e Usuário.

Este documento detalha o planejamento arquitetural, os marcos já alcançados e as metas futuras para o desenvolvimento do sistema operacional, com chamados diretos para a comunidade colaborar em cada etapa.

---

## 📌 Visão Geral das Fases

* **Fase 1 (Concluída ✅)**: Fundação do Microkernel, Paginação PML4, Preempção com APIC Timer, Syscalls via `int 0x80`, IPC lock-free, Carregador ELF64 e Isolamento Real em Ring 3.
* **Fase 2 (Concluída ✅)**: Primitivas 2D, Driver de Mouse PS/2, Renderização de Cursor, Double Buffering, Compositor de Janelas e Terminal Interativo ([Issue #32](https://github.com/GeovaneEstanislau/Jinn_Project/issues/32)).
* **Fase 3 (Em Andamento 🚧)**: Driver de Disco (IDE/ATA PIO Mode), MBR, Sistema de Arquivos (Ext2/FAT32) e montagem VFS ([Issue #30](https://github.com/GeovaneEstanislau/Jinn_Project/issues/30)).
* **Fase 4 (Concluída ✅)**: Melhorias de DX, scripts de build multiplataforma (Makefile/PS1) e CI no GitHub Actions ([Issue #31](https://github.com/GeovaneEstanislau/Jinn_Project/issues/31) e [Issue #33](https://github.com/GeovaneEstanislau/Jinn_Project/issues/33)).
* **Fase 5 (Futuro 📋)**: Suporte a SMP (Multiprocessamento), Driver de Rede (e1000/VirtIO) e Stack TCP/IP em Ring 3 ([Issue #34](https://github.com/GeovaneEstanislau/Jinn_Project/issues/34) e [Issue #35](https://github.com/GeovaneEstanislau/Jinn_Project/issues/35)).

---

## 🚀 Fase 1: Fundação do Microkernel & Isolamento de Memória (Concluída ✅)

- [x] **Bootloader Moderno**: Inicialização via Limine Bootloader v12 (suporte completo a UEFI e BIOS).
- [x] **Gerenciamento de Memória Física (PMM)**: Alocador de frames físicos baseado em bitmap de alta performance (`src/memory/mod.rs`).
- [x] **Gerenciamento de Memória Virtual (VMM)**: Paginação de 4 níveis x86_64 (PML4) com cópia sob demanda e isolamento de espaço de endereçamento (`src/memory/paging.rs`).
- [x] **Kernel Heap Customizado**: Alocador próprio (Bump + Free-List) sem dependências externas (`src/memory/heap.rs`).
- [x] **Tabela de Descritores Globais (GDT) & TSS**: Configuração de segmentos para Kernel (Ring 0) e Usuário (Ring 3), com TSS configurada para troca segura de pilhas (`src/gdt.rs`).
- [x] **Interrupções & Temporizador**: IDT completa (48 vetores), Local APIC Timer calibrado a 100 Hz, PIC legado desativado (`src/interrupts/`).
- [x] **Escalonador Preemptivo**: Escalonador preemptivo com suporte a prioridades (RealTime, Normal, Idle) e alternância de tarefas acionada pelo timer (`src/scheduler.rs`).
- [x] **Dormência a Custo Zero de CPU (`sys_sleep` e `TaskState::Sleeping`)**: Processos podem suspender execução temporizada consumindo 0 ciclos de CPU; a CPU repousa em `hlt` quando não há tarefas prontas.
- [x] **IPC (Inter-Process Communication)**: Mensageria assíncrona baseada em filas lock-free para comunicação entre processos (`src/ipc.rs`).
- [x] **Camada de Syscalls**: Despacho via `int 0x80` (`sys_exit`, `sys_write`, `sys_read`, `sys_yield`, `sys_getpid`, `sys_ipc_send`, `sys_ipc_recv`, `sys_alloc_page`, `sys_free_page`, `sys_spawn`, `sys_waitpid`, `sys_sleep`).
- [x] **Carregador ELF64 & Isolamento de Ring 3**:
  - Leitura de binários ELF64 com parsing de cabeçalhos e segmentos `PT_LOAD`.
  - Alocação de PML4 dedicado por processo (espaço de endereçamento isolado).
  - Mapeamento das páginas de código/dados do executável com permissões de usuário (`USER_RW`).
  - Execução segura do processo de usuário no espaço inferior de memória (`0x400000`).
  - Aplicação de demonstração `apps/ola-mundo` compilada de forma independente e carregada via módulo Limine.

---

## 🎨 Fase 2: Interface Gráfica (GUI) & Subsistema de Entrada (Concluída ✅)

> 📌 **Discussão e Colaboração:** Concluída na [Issue #32: Criar Interface Gráfica (GUI) e Compositor](https://github.com/GeovaneEstanislau/Jinn_Project/issues/32).

- [x] **Primitivas Gráficas 2D (`src/gui.rs`)**:
  - `draw_pixel(x, y, color)` e `read_pixel(x, y)`.
  - `fill_rect(x, y, width, height, color)`.
  - `draw_line_bb` (Algoritmo de Bresenham) e `draw_rect_outline_bb`.
  - `draw_progress_bar_bb` e helpers de formatação decimal sem alocador dinâmico.
  - `draw_char_at` e `draw_string_at` com fonte bitmap 8x8 customizada.
- [x] **Driver de Mouse PS/2 (`src/interrupts/ps2_mouse.rs`)**:
  - Habilitada a porta auxiliar do controlador i8042.
  - Tratamento de interrupção IRQ 12 (vetor 44 na IDT).
  - Decodificação de pacotes de 3 bytes (Flags/Botões, Delta X, Delta Y).
  - Rastreamento absoluto de coordenadas na tela (1024x768).
- [x] **Renderização de Cursor com Restauração de Fundo**:
  - Sprite de seta de cursor desenhado em tempo real.
  - Salvamento e restauração automática do fundo sob o cursor para evitar degradação gráfica.
- [x] **Double Buffering / Backbuffer na RAM**:
  - Alocação de backbuffer linear (3 MiB) na memória RAM eliminando flickering e tearing.
  - Rotina de cópia rápida em lote (`swap_buffers`) para o Linear Framebuffer do Limine.
  - Throttling inteligente a 60 FPS orientado a eventos (movimento, cliques, teclado ou heartbeat de 100ms).
- [x] **Compositor & Gerenciador de Janelas**:
  - Estrutura `Window` com ID, retângulo `(x, y, w, h)`, título, linhas de conteúdo e estado de foco.
  - Z-Order com elevação de janela ativa para o topo (`bring_to_front`).
  - Detecção de clique e arrasto na barra de título em tempo real com o mouse.
  - Fechamento de janelas pelo botão vermelho `[X]`.
  - Barra de tarefas inferior (Taskbar) com botão Iniciar `[ Jinn OS ]` e abas clicáveis.
  - Menu Iniciar funcional com 6 opções interativas (Monitor, Terminal, Spawn U3, Sobre, Reboot, Exit).
  - Monitor de Sistema dinâmico com Uptime, contagem de tarefas e barra gráfica de uso de RAM.
  - Comando interativo `gui` / `desktop` no shell do kernel.
- [x] **Terminal Gráfico Interativo com Teclado na Janela**:
  - Janela de terminal renderizando console dinâmico dentro da interface gráfica.
  - Entrada de teclado PS/2 em tempo real, buffer de digitação, cursor piscante e histórico com cores.
  - Comandos integrados no terminal da GUI: `help`, `spawn`, `ps`, `uptime`, `clear`, `reboot`, `exit`.

---

## 💽 Fase 3: Armazenamento & Sistema de Arquivos Real (Em Andamento 🚧)

> 📌 **Discussão e Colaboração:** Participe na [Issue #30: Implementar Filesystem Real e Drivers de Disco](https://github.com/GeovaneEstanislau/Jinn_Project/issues/30).

- [x] **Driver de Armazenamento IDE/ATA (Primary Bus)**:
  - Driver em modo PIO (Portas 0x1F0-0x1F7 / 0x3F6) para discos IDE/ATA e dispositivos ATAPI (`src/drivers/ide.rs`).
  - Identificação de drives via comando `0xEC` (IDENTIFY) com extração de modelo, serial e capacidade em MB.
  - Leitura de setores físicos LBA28 (512 bytes) e parsing de partições MBR (LBA 0).
  - Comando interativo `disk` no shell do kernel inspecionando o barramento primário.
- [x] **Detecção e Enumeração do Barramento PCI (`src/drivers/pci.rs`)**:
  - Varredura de barramentos PCI via Mecanismo de Configuração #1 (Portas `0xCF8` / `0xCFC`).
  - Mapeamento de Vendor ID, Device ID, Classes (Storage, Network, Display, Bridge) e Base Address Registers (BARs).
  - Comando interativo `lspci` (alias `pci`) no shell listando todos os dispositivos do sistema.
- [x] **Sistema de Arquivos FAT32 (`src/vfs/fat32.rs`)**:
  - Leitura do Boot Sector / BIOS Parameter Block (BPB) da partição.
  - Navegação e encadeamento de clusters na tabela FAT32.
  - Leitura e listagem de entradas de diretório no formato 8.3 e extração de atributos.
  - Implementação completa do trait `vfs::FileSystem` permitindo leitura de arquivos e diretórios.
- [x] **Integração com o VFS e Comandos de Shell**:
  - Comando `mount_fat [lba]`: Montagem de partições FAT32 em `/disk`.
  - Comando `ls [path]`: Listagem de arquivos e diretórios em qualquer caminho do VFS (`/`, `/dev`, `/disk`).
  - Comando `cat <file>`: Leitura de conteúdo de arquivos texto em qualquer sistema de arquivos montado.
  - Comando `mounts`: Exibição de todos os pontos de montagem ativos.
  - **Auto-montagem no boot**: Detecção automática de partições FAT32 (tipos `0x0B` e `0x0C`) e montagem em `/disk`.
  - **Carregamento dinâmico de binários ELF do disco/VFS**: Comando `spawn` e GUI agora leem arquivos executáveis diretamente de `/disk` (`spawn /disk/ola.elf`), com alocação e isolamento em Ring 3.
  - **Gerador de Disco Virtual FAT32**: Scripts multiplataforma (`scripts/make_disk.ps1` e `scripts/make_disk.py`) que geram discos RAW de 32 MiB com MBR e arquivos de teste para o QEMU.
- [ ] **Drivers Adicionais & Expansão de Armazenamento**:
  - Driver AHCI SATA para discos modernos em modo DMA.
  - Reconhecimento de tabelas de partição GPT.

---

## 🛠️ Fase 4: Experiência de Desenvolvimento (DX) & Automação (Concluída ✅)

> 📌 **Discussão e Colaboração:** Concluída nas issues:
> - [Issue #31: Padronizar o sistema de build (Scripts vs Make/Cargo-make)](https://github.com/GeovaneEstanislau/Jinn_Project/issues/31)
> - [Issue #33: Configurar GitHub Actions para CI e geração da ISO](https://github.com/GeovaneEstanislau/Jinn_Project/issues/33)

- [x] **Scripts PowerShell automatizados** (`scripts/build.ps1`, `scripts/run.ps1`) com suporte a `xorriso`, `limine.exe bios-install` e discos virtuais.
- [x] **Makefile Multiplataforma (`Makefile`)**:
  - Suporte completo para desenvolvedores no Linux, macOS e WSL (`make kernel`, `make app`, `make disk`, `make iso`, `make run`, `make clean`).
- [x] **Pipeline de CI/CD no GitHub Actions (`.github/workflows/ci.yml`)**:
  - Validação contínua da compilação do kernel (`cargo build --release`).
  - Validação de build das aplicações de usuário (`apps/ola-mundo`).
  - Geração automática e disponibilização dos artefatos `jinn.iso` e `disk.img` em cada commit e Pull Request.

---

## 🌐 Fase 5: Expansão de Hardware, Rede & SMP (Futuro 📋)

> 📌 **Discussão e Colaboração:** Participe nas issues:
> - [Issue #34: Suporte a SMP: Inicialização de núcleos secundários (APs)](https://github.com/GeovaneEstanislau/Jinn_Project/issues/34)
> - [Issue #35: Implementar Pilha de Rede TCP/IP no Espaço de Usuário](https://github.com/GeovaneEstanislau/Jinn_Project/issues/35)

- [ ] **Suporte a SMP (Multiprocessamento Simétrico)**:
  - Detecção de núcleos via tabelas ACPI (MADT).
  - Envio de IPIs (INIT/SIPI) pelo Local APIC para inicialização dos APs.
  - Escalonamento preemptivo distribuído entre múltiplos núcleos com spinlocks finos.
- [ ] **Driver de Placa de Rede**:
  - Driver para Intel 8254x (e1000) ou VirtIO-Net.
- [ ] **Pilha de Rede em Userspace (Ring 3)**:
  - Implementação de Ethernet, ARP, IPv4, ICMP (ping) e sockets UDP/TCP.
- [ ] **Biblioteca Padrão C / Rust (`libjinn`)**:
  - Runtime leve em Ring 3 para facilitar a escrita de novos utilitários e softwares nativos.

---

## 🧬 Linha de Pesquisa: Jinn Dataflow Architecture (JDA — Em Andamento 🚧)

Investigação experimental para eliminar movimentação desnecessária de dados, contenção de recursos e ociosidade da CPU por meio de grafos dirigidos por dados, transferências zero-copy e observabilidade de ciclos de clock.

- [x] **Fase 0 — Auditoria Técnica e Diagnóstico**:
  - Auditoria completa dos componentes de memória, escalonador, IPC, VFS e drivers.
  - Mapeamento de gargalos de cópia e polling síncrono.
- [x] **Fase 1 — Especificação v0.1 & Modelo de Dados**:
  - [Especificação Técnica Formal v0.1](docs/JDA_SPEC_V0.1.md) cobrindo DAGs, invariâncias, confiança de hardware e políticas.
  - Modelo de Topologia de Hardware (`src/jda/topology.rs`): detecção de cores e hierarquia de caches L1/L2/L3 via CPUID.
  - Grafo Acíclico Dirigido (`src/jda/graph.rs`): resolução de dependências por bitmask e descritores de buffer zero-copy.
  - Motor de Telemetria (`src/jda/telemetry.rs`): contadores de ciclos TSC (`rdtsc`), bytes poupados e latência.
  - Políticas Substituíveis (`src/jda/policy.rs`): política FIFO de referência e política adaptativa orientada à localidade.
  - Comandos interativos no shell do kernel: `jda` e `jdabench`.
- [x] **Fase 2 — Protótipo de Referência e Cargas Sintéticas**:
  - Simulador em espaço de usuário (`tools/jda-sim`) com suite de 10 cargas sintéticas representativas.
  - Telemetria de percentis p50/p95/p99, desvio padrão e contabilidade de bytes poupados.
- [x] **Fase 3 — Integração no Microkernel & Anéis Assíncronos (io_uring)**:
  - Anéis de submissão (SQ) e conclusão (CQ) lock-free (`src/jda/ring.rs`) para I/O de disco e computação in-kernel.
  - Compartilhamento real de páginas físicas (`PhysicalPageBuffer`) entre Ring 3 e kernel com remapeamento no PML4 (`USER_JDA_BASE` a `0x30000000`).
  - Imposição estrita de imutabilidade (`USER_RO`) em compartilhamento multi-processo e rejeição de escritas não autorizadas.
  - Novas syscalls: `sys_jda_shm_share`, `sys_jda_ring_submit`, `sys_jda_ring_poll`, `sys_jda_shm_get_vaddr`.
  - Autotestes no boot do kernel (`run_jda_selftests`), suíte de validação em Ring 3 (`apps/ola-mundo`) e comandos interativos `jda` e `jdabench`.

---

## 🤝 Como Participar e Ajudar

Nós valorizamos cada pessoa interessada em sistemas operacionais, Rust e computação de baixo nível!
Para começar:
1. Veja a lista completa de tarefas no [GitHub Issues](https://github.com/GeovaneEstanislau/Jinn_Project/issues).
2. Deixe um comentário na issue que deseja trabalhar para que possamos alinhar e ajudá-lo(a).
3. Leia nosso guia de contribuição em [CONTRIBUTING.md](CONTRIBUTING.md).
