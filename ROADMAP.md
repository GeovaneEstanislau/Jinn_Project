# 🗺️ Mapa de Desenvolvimento (Roadmap) — Jinn OS

> **Jinn OS**: Microkernel preemptivo x86_64 em Rust.
> Focado em alto desempenho, baixo overhead, controle explícito de recursos e isolamento rigoroso entre Kernel e Usuário.

Este documento detalha o planejamento arquitetural, os marcos já alcançados e as metas futuras para o desenvolvimento do sistema operacional.

---

## 📌 Visão Geral do Progresso

| Fase | Descrição | Status |
|:---:|---|:---:|
| **Fase 1** | Fundação do Microkernel & Isolamento de Memória (Ring 3 + ELF) | ✅ **Concluída** |
| **Fase 2** | Interface Gráfica (GUI) & Subsistema de Entrada (Mouse / Compositor) | 🚧 **Em Andamento** |
| **Fase 3** | Armazenamento & Sistema de Arquivos Real (Drivers de Disco + FS) | 🔜 **Planejado** ([Issue #30](https://github.com/GeovaneEstanislau/Jinn_Project/issues/30)) |
| **Fase 4** | Melhorias de Build, DX & Ferramentas Multiplataforma | 🚧 **Em Andamento** ([Issue #31](https://github.com/GeovaneEstanislau/Jinn_Project/issues/31)) |
| **Fase 5** | Pilha de Rede, SMP e Ecossistema de Usuário | 📋 **Futuro** |

---

## 🚀 Fase 1: Fundação do Microkernel & Isolamento de Memória (Concluída)

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

## 🎨 Fase 2: Interface Gráfica (GUI) & Subsistema de Entrada (Foco Atual)

Objetivo: Prover um ambiente gráfico interativo funcional utilizando o Linear Framebuffer fornecido pelo Limine (1024x768x32).

- [x] **Primitivas Gráficas 2D (`src/gui.rs`)**:
  - [x] `draw_pixel(x, y, color)`
  - [x] `fill_rect(x, y, width, height, color)`
  - [x] `draw_char_at(ch, x, y, fg, bg)` e `draw_string_at(text, x, y, fg, bg)` com fonte bitmap customizada
  - [x] Renderização de janelas com molduras, barra de título e botão de fechar (`draw_window`)
- [ ] **Driver de Mouse PS/2**:
  - [ ] Habilitar porta auxiliar do controlador 8042 (porta de comando `0x64`, porta de dados `0x60`).
  - [ ] Tratamento de interrupção IRQ 12 (vetor 44 na IDT).
  - [ ] Leitura e decodificação do pacote de 3 bytes (Flags/Botões, Delta X, Delta Y).
  - [ ] Rastreamento de coordenadas absolutas do cursor `(mouse_x, mouse_y)` dentro dos limites da tela (1024x768).
- [ ] **Renderização de Cursor & Double Buffering**:
  - [ ] Alocação de Backbuffer na RAM para desenho sem cintilação (*flickering* / *tearing*).
  - [ ] Renderização de sprite de cursor com canal alfa / transparência.
  - [ ] Rotina de `swap_buffers` / `flush` rápida via cópia de memória em lote.
- [ ] **Compositor & Gerenciador de Janelas Simples**:
  - [ ] Estrutura `Window` com ID, retângulo `(x, y, w, h)`, título e estado de foco.
  - [ ] Z-Order (janela ativa desenhada no topo).
  - [ ] Detecção de clique e arrasto na barra de título para mover janelas.
  - [ ] Redesenho apenas das regiões danificadas (*dirty rects*) para máxima performance.
- [ ] **Terminal Gráfico**:
  - [ ] Janela de terminal renderizando o Shell do Jinn OS dentro da interface gráfica.

---

## 💽 Fase 3: Armazenamento & Sistema de Arquivos Real ([Issue #30](https://github.com/GeovaneEstanislau/Jinn_Project/issues/30))

Objetivo: Substituir o carregamento de binários injetados na memória por persistência em disco real.

- [ ] **Drivers de Armazenamento**:
  - [ ] Driver de disco IDE (PIO mode) para máxima compatibilidade em emuladores e máquinas antigas.
  - [ ] Driver AHCI SATA para hardware moderno.
  - [ ] Detecção de dispositivos via barramento PCI.
- [ ] **Sistema de Arquivos**:
  - [ ] Implementação de leitura em sistema de arquivos Ext2 ou FAT32.
  - [ ] Particionamento MBR / GPT.
- [ ] **Integração com o VFS**:
  - [ ] Montagem de sistema de arquivos raiz (`/`).
  - [ ] Carregamento sob demanda de binários ELF a partir do disco (`/bin/`).

---

## 🛠️ Fase 4: Experiência de Desenvolvimento (DX) & Automação ([Issue #31](https://github.com/GeovaneEstanislau/Jinn_Project/issues/31))

Objetivo: Facilitar a contribuição da comunidade em qualquer sistema operacional (Linux, macOS, Windows).

- [x] Scripts PowerShell automatizados (`scripts/build.ps1`, `scripts/run.ps1`) com suporte a `xorriso`.
- [ ] Criação de `Makefile` ou suporte a `cargo-make` para ambientes Linux/macOS.
- [ ] Pipeline de Integração Contínua (CI) no GitHub Actions:
  - Validação de compilação do kernel e das apps de usuário.
  - Linting (`cargo clippy`, `cargo fmt`).
  - Geração automática de artefatos da ISO em releases/tags.

---

## 🌐 Fase 5: Expansão de Hardware & Rede (Longo Prazo)

- [ ] **Suporte a SMP (Multiprocessamento Simétrico)**:
  - Inicialização de APs (Application Processors) via IPIs do APIC.
  - Escalonamento distribuído entre múltiplos núcleos de CPU.
- [ ] **Driver de Rede**:
  - Driver para Intel 8254x (e1000) e VirtIO-Net.
- [ ] **Pilha de Rede em Userspace**:
  - Processo de rede em Ring 3 implementando ARP, IPv4, ICMP (ping) e UDP/TCP via IPC.
- [ ] **Biblioteca Padrão C / Rust (libjinn)**:
  - Criação de uma runtime leve em Ring 3 para facilitar a escrita de aplicativos nativos semânticos.

---

## 🤝 Como Contribuir

Quer ajudar a construir uma dessas fases?
- Escolha uma das tarefas abertas no [GitHub Issues](https://github.com/GeovaneEstanislau/Jinn_Project/issues).
- Consulte as diretrizes em [CONTRIBUTING.md](CONTRIBUTING.md).
- Toda colaboração em arquitetura, documentação, testes ou código de baixo nível é muito bem-vinda!
