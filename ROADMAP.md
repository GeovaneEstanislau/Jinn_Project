# 🗺️ Mapa de Desenvolvimento (Roadmap) — Jinn OS

> **Jinn OS**: Microkernel preemptivo x86_64 minimalista escrito em Rust.
> Focado em alto desempenho, baixo overhead, controle explícito de recursos e isolamento rigoroso entre Kernel e Usuário.

Este documento detalha o planejamento arquitetural, os marcos já alcançados e as metas futuras para o desenvolvimento do sistema operacional, com chamados diretos para a comunidade colaborar em cada etapa.

---

## 📌 Visão Geral das Fases

* **Fase 1 (Concluída ✅)**: Fundação do Microkernel, Paginação PML4, Preempção com APIC Timer, Syscalls via `int 0x80`, IPC lock-free, Carregador ELF64 e Isolamento Real em Ring 3.
* **Fase 2 (Em Andamento 🚧)**: Primitivas 2D, Driver de Mouse PS/2, Renderização de Cursor, Double Buffering e Compositor de Janelas ([Issue #32](https://github.com/GeovaneEstanislau/Jinn_Project/issues/32)).
* **Fase 3 (Planejado 🔜)**: Driver de Disco (IDE/AHCI SATA), Sistema de Arquivos (Ext2/FAT32) e montagem VFS ([Issue #30](https://github.com/GeovaneEstanislau/Jinn_Project/issues/30)).
* **Fase 4 (Em Andamento 🚧)**: Melhorias de DX, scripts de build multiplataforma e CI no GitHub Actions ([Issue #31](https://github.com/GeovaneEstanislau/Jinn_Project/issues/31) e [Issue #33](https://github.com/GeovaneEstanislau/Jinn_Project/issues/33)).
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
- [x] **IPC (Inter-Process Communication)**: Mensageria assíncrona baseada em filas lock-free para comunicação entre processos (`src/ipc.rs`).
- [x] **Camada de Syscalls**: Despacho via `int 0x80` (`sys_exit`, `sys_write`, `sys_read`, `sys_yield`, `sys_getpid`, `sys_ipc_send`, `sys_ipc_recv`).
- [x] **Carregador ELF64 & Isolamento de Ring 3**:
  - Leitura de binários ELF64 com parsing de cabeçalhos e segmentos `PT_LOAD`.
  - Alocação de PML4 dedicado por processo (espaço de endereçamento isolado).
  - Mapeamento das páginas de código/dados do executável com permissões de usuário (`USER_RW`).
  - Execução segura do processo de usuário no espaço inferior de memória (`0x400000`).
  - Aplicação de demonstração `apps/ola-mundo` compilada de forma independente e carregada via módulo Limine.

---

## 🎨 Fase 2: Interface Gráfica (GUI) & Subsistema de Entrada (Em Andamento 🚧)

> 📌 **Discussão e Colaboração:** Participe na [Issue #32: Criar Interface Gráfica (GUI) e Compositor](https://github.com/GeovaneEstanislau/Jinn_Project/issues/32).

- [x] **Primitivas Gráficas 2D (`src/gui.rs`)**:
  - `draw_pixel(x, y, color)` e `read_pixel(x, y)`.
  - `fill_rect(x, y, width, height, color)`.
  - `draw_char_at` e `draw_string_at` com fonte bitmap 8x8 customizada.
  - Renderização de janelas com molduras, barra de título e botão de fechar (`draw_window`).
- [x] **Driver de Mouse PS/2 (`src/interrupts/ps2_mouse.rs`)**:
  - Habilitada a porta auxiliar do controlador i8042.
  - Tratamento de interrupção IRQ 12 (vetor 44 na IDT).
  - Decodificação de pacotes de 3 bytes (Flags/Botões, Delta X, Delta Y).
  - Rastreamento absoluto de coordenadas na tela (1024x768).
- [x] **Renderização de Cursor com Restauração de Fundo**:
  - Sprite de seta de cursor desenhado em tempo real.
  - Salvamento e restauração automática do fundo sob o cursor para evitar degradação gráfica.
- [ ] **Double Buffering / Backbuffer na RAM**:
  - Alocação de backbuffer linear na memória RAM para eliminar flickering e tearing.
  - Rotina de cópia rápida em lote (`swap_buffers` / `flush`) para o Linear Framebuffer do Limine.
- [ ] **Compositor & Gerenciador de Janelas**:
  - Estrutura `Window` com ID, retângulo `(x, y, w, h)`, título e estado de foco.
  - Z-Order (janela ativa desenhada no topo).
  - Detecção de clique e arrasto na barra de título para mover janelas.
  - Suporte a eventos de mouse (click, drag, hover).
- [ ] **Terminal Gráfico**:
  - Janela de terminal renderizando o Shell do Jinn OS dentro da interface gráfica.

---

## 💽 Fase 3: Armazenamento & Sistema de Arquivos Real (Planejado 🔜)

> 📌 **Discussão e Colaboração:** Participe na [Issue #30: Implementar Filesystem Real e Drivers de Disco](https://github.com/GeovaneEstanislau/Jinn_Project/issues/30).

- [ ] **Drivers de Armazenamento**:
  - Driver de disco IDE (PIO mode) para máxima compatibilidade em emuladores e máquinas virtuais.
  - Driver AHCI SATA para discos modernos.
  - Detecção e enumeração de dispositivos no barramento PCI.
- [ ] **Sistema de Arquivos**:
  - Implementação de leitura em sistema de arquivos Ext2 ou FAT32.
  - Reconhecimento de tabelas de partição MBR e GPT.
- [ ] **Integração com o VFS**:
  - Montagem de sistema de arquivos raiz (`/`).
  - Carregamento sob demanda de binários ELF a partir do disco (`/bin/`).

---

## 🛠️ Fase 4: Experiência de Desenvolvimento (DX) & Automação (Em Andamento 🚧)

> 📌 **Discussão e Colaboração:** Participe nas issues:
> - [Issue #31: Padronizar o sistema de build (Scripts vs Make/Cargo-make)](https://github.com/GeovaneEstanislau/Jinn_Project/issues/31)
> - [Issue #33: Configurar GitHub Actions para CI e geração da ISO](https://github.com/GeovaneEstanislau/Jinn_Project/issues/33)

- [x] Scripts PowerShell automatizados (`scripts/build.ps1`, `scripts/run.ps1`) com suporte a `xorriso`.
- [ ] **Makefile Multiplataforma / `cargo-make`**:
  - Suporte suave para desenvolvedores no Linux, macOS e Windows.
- [ ] **Pipeline de CI/CD no GitHub Actions (`.github/workflows/ci.yml`)**:
  - Validação contínua da compilação do kernel (`cargo build --release`).
  - Validação de build das aplicações de usuário (`apps/ola-mundo`).
  - Geração automática e disponibilização de artefatos da ISO (`jinn.iso`) em releases e PRs.

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

## 🤝 Como Participar e Ajudar

Nós valorizamos cada pessoa interessada em sistemas operacionais, Rust e computação de baixo nível!
Para começar:
1. Veja a lista completa de tarefas no [GitHub Issues](https://github.com/GeovaneEstanislau/Jinn_Project/issues).
2. Deixe um comentário na issue que deseja trabalhar para que possamos alinhar e ajudá-lo(a).
3. Leia nosso guia de contribuição em [CONTRIBUTING.md](CONTRIBUTING.md).
