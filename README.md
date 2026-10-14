# Jinn OS

> **Um microkernel x86_64 minimalista e preemptivo escrito em Rust.**
> Construído do zero. Sem abstrações que desperdiçam recursos. Sem concessões.

---

## Filosofia

O Jinn OS é construído sobre três pilares principais:

1. **Preempção sem desperdício** — O kernel nunca permite que uma tarefa monopolize a CPU. O timer do APIC dispara a 100 Hz e o escalonador age a cada ciclo.
2. **Explícito ao invés de implícito** — Sem alocações ocultas, sem surpresas em tempo de execução. Toda aquisição de recursos é intencional e transparente.
3. **Kernel mínimo, capacidade máxima** — O kernel faz o mínimo necessário: escalonamento, IPC (Comunicação Inter-Processos), memória e chamadas de sistema (syscalls). Todo o resto roda no espaço do usuário (Ring 3).

---

## Funcionalidades

| Funcionalidade | Status |
|---|---|
| Bootloader Limine (BIOS + UEFI) | ✅ |
| Renderizador de texto Framebuffer (fonte customizada) | ✅ |
| Primitivas Gráficas 2D & Janelas (`gui.rs`) | 🚧 Em Andamento |
| x86_64 IDT (48 vetores) | ✅ |
| Local APIC + Timer (100 Hz) | ✅ |
| Escalonador Preemptivo Round-Robin com Prioridades | ✅ |
| Alocador de Frames Físicos (Bitmap) | ✅ |
| Paginação de 4 Níveis (PML4) | ✅ |
| Kernel Heap Próprio (Bump + Free List) | ✅ |
| Driver de Teclado PS/2 | ✅ |
| Driver de Mouse PS/2 | 🚧 Em Andamento |
| Shell Interativo | ✅ |
| IPC (Filas de mensagens lock-free) | ✅ |
| Interface de Syscalls (10 chamadas via `int 0x80`) | ✅ |
| VFS + ramfs (`/dev/`) | ✅ |
| Carregador ELF64 & Isolamento Ring 3 | ✅ |
| Pilha de Rede | 🔜 Planejado |
| Driver Ext2 / FAT32 / AHCI | 🔜 Planejado ([Issue #30](https://github.com/GeovaneEstanislau/Jinn_Project/issues/30)) |

> 🗺️ **Consulte o nosso plano de desenvolvimento detalhado em [ROADMAP.md](ROADMAP.md).**

---

## Arquitetura

```
┌────────────────────────────────────────────────────────────┐
│                  Espaço do Usuário (Ring 3)                │
│                  ┌─────────┐  ┌─────────┐  ┌───────────┐   │
│                  │App (ELF)│  │  App B  │  │  Shell    │   │
│                  └────┬────┘  └────┬────┘  └─────┬─────┘   │
└───────────────────────┼────────────┼─────────────┼─────────┘
                        │  int 0x80  │             │
┌───────────────────────▼────────────▼─────────────▼─────────┐
│              Camada de Syscalls (syscall.rs)               │
│  exit │ write │ read │ yield │ getpid │ ipc_send │ spawn   │
├────────────────────────────────────────────────────────────┤
│                  IPC (ipc.rs) — Filas lock-free            │
├──────────────────┬─────────────────────┬───────────────────┤
│  Escalonador     │  Gerenciador Mem.   │   VFS             │
│  (scheduler.rs)  │  (memory/)          │   (vfs/)          │
│  Round-Robin     │  Alocador Físico    │   ramfs / devfs   │
│  Preemptivo      │  Paginação (PML4)   │                   │
│  Prioridades     │  Heap Customizado   │                   │
├──────────────────┴─────────────────────┴───────────────────┤
│         Subsistema Gráfico & GUI (gui.rs, vga.rs, font.rs) │
│      Janelas  │  Retângulos  │  Fontes  │  Linear Framebuffer │
├────────────────────────────────────────────────────────────┤
│           Subsistema de Interrupções (interrupts/)         │
│      IDT  │  Local APIC  │  Teclado PS/2   │  Mouse (IRQ12)│
├────────────────────────────────────────────────────────────┤
│         Abstração de Hardware (limine.rs, cpu.rs, elf.rs)  │
└────────────────────────────────────────────────────────────┘
```

---

## Como Compilar

Requisitos: Rust nightly com `rust-src`, `xorriso` (ou `mkisofs`/`oscdimg`) para gerar a ISO e QEMU ou VirtualBox para executá-la. O launcher baixa os binários oficiais do Limine quando necessário.

```powershell
rustup toolchain install nightly --component rust-src --component llvm-tools-preview
rustup target add x86_64-unknown-none --toolchain nightly
```

Compile e gere a ISO sem iniciar uma VM:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\run.ps1 -NoRun
```

O comando gera `jinn.iso`; sem `-NoRun`, o launcher inicia o QEMU. Para escolher outra saída, use `-IsoPath nome.iso`.

Para compilar sem gerar a ISO:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build.ps1
```

---

## Comandos do Shell

Uma vez iniciado, o shell interativo (`jinn>`) suporta:

| Comando | Descrição |
|---|---|
| `help` | Lista os comandos disponíveis |
| `uname` | Mostra o nome, versão e arquitetura do kernel |
| `meminfo` | Mostra estatísticas da memória física e heap do kernel |
| `ps` / `tasks` | Lista todas as tarefas em execução no escalonador |
| `spawn <nome>` | Inicia um processo em Ring 3 a partir do ELF carregado |
| `kill <pid>` | Finaliza uma tarefa pelo seu PID |
| `sched` | Estatísticas detalhadas do escalonador preemptivo |
| `uptime` | Tempo de atividade em ticks do APIC Timer |
| `reboot` | Reinicia a máquina de forma segura |
| `clear` / `cls` | Limpa o console de texto |

---

## Licença

MIT — Veja o arquivo `LICENSE`.

## Contato e Contribuição

Este projeto valoriza imensamente a comunidade de tecnologia brasileira. Estamos buscando desenvolvedores e entusiastas de sistemas operacionais. Veja o arquivo [CONTRIBUTING.md](CONTRIBUTING.md) para saber por onde começar.
