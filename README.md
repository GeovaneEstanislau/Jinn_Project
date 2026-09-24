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
| x86_64 IDT (48 vetores) | ✅ |
| Local APIC + Timer (100 Hz) | ✅ |
| Escalonador Preemptivo Round-Robin | ✅ |
| Alocador de Frames Físicos (Bitmap) | ✅ |
| Paginação de 4 Níveis (PML4) | ✅ |
| Kernel Heap Próprio (Bump + Free List) | ✅ |
| Driver de Teclado PS/2 | ✅ |
| Shell Interativo | ✅ |
| IPC (Filas de mensagens lock-free) | ✅ |
| Interface de Syscalls (10 chamadas) | ✅ |
| VFS + ramfs (`/dev/`) | ✅ |
| Espaço do Usuário (Ring 3) | 🚧 Em Andamento |
| Pilha de Rede | 🔜 Planejado |
| Driver Ext2 / FAT32 | 🔜 Planejado |

---

## Arquitetura

```
┌────────────────────────────────────────────────────────────┐
│                  Espaço do Usuário (Ring 3)                │
│                  ┌─────────┐  ┌─────────┐  ┌───────────┐   │
│                  │  App A  │  │  App B  │  │  Shell    │   │
└──────────────────┴────┬────┴──┴────┬────┴──┴─────┬─────┘───┘
                         │ int 0x80  │              │
┌────────────────────────▼──────────▼──────────────▼─────────┐
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
│           Subsistema de Interrupções (interrupts/)         │
│      IDT  │  Local APIC  │  Teclado PS/2   │  PIC(off)     │
├────────────────────────────────────────────────────────────┤
│         Abstração de Hardware (limine.rs, cpu.rs, vga.rs)  │
└────────────────────────────────────────────────────────────┘
```

---

## Como Compilar

### Pré-requisitos

- [Rust nightly](https://rustup.rs/) com a target `x86_64-unknown-none`
- `cargo` com suporte a `-Zbuild-std`

```powershell
rustup toolchain install nightly
rustup target add x86_64-unknown-none --toolchain nightly
rustup component add rust-src --toolchain nightly
```

### Compilar e Gerar a ISO

O Jinn OS utiliza scripts nativos em PowerShell, sem depender de ferramentas externas em C/C++:

```powershell
.\scripts\build.ps1
.\scripts\make_iso.ps1
```

A ISO será gerada como `jinn.iso` e é inicializável no VirtualBox ou QEMU.

### Executar no QEMU

```bash
qemu-system-x86_64 -cdrom jinn.iso -m 128M -serial stdio
```

---

## Comandos do Shell

Uma vez iniciado, o shell interativo (`jinn>`) suporta:

| Comando | Descrição |
|---|---|
| `help` | Lista os comandos disponíveis |
| `uname` | Mostra o nome e a versão do kernel |
| `meminfo` | Mostra as estatísticas da memória física e do heap |
| `clear` | Limpa a tela |

---

## Licença

MIT — Veja o arquivo `LICENSE`.

## Contato e Contribuição

Este projeto valoriza imensamente a comunidade de tecnologia brasileira. Estamos buscando desenvolvedores e entusiastas de sistemas operacionais. Veja o arquivo [CONTRIBUTING.md](CONTRIBUTING.md) para saber por onde começar.
