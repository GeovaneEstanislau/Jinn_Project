# Contribuindo para o Jinn OS

Obrigado pelo seu interesse! O Jinn OS é um microkernel bare-metal x86_64 escrito em Rust, feito no Brasil para o mundo. O projeto valoriza contribuições que sigam a filosofia de alta performance e código limpo.

---

## Primeiros Passos

### 1. Configurar o ambiente

```powershell
# Instalar a versão nightly do Rust
rustup toolchain install nightly
rustup default nightly

# Adicionar a arquitetura bare-metal
rustup target add x86_64-unknown-none
rustup component add rust-src llvm-tools-preview
```

### 2. Clonar e Compilar

```powershell
git clone https://github.com/GeovaneEstanislau/Jinn_Project
cd jinn-os
.\scripts\build.ps1       # Compila o binário ELF do kernel
.\scripts\make_iso.ps1    # Empacota em uma imagem ISO9660
```

### 3. Executar

**VirtualBox**: Anexe o arquivo `jinn.iso` como um CD-ROM (Boot em BIOS, modo 64-bits).

**QEMU**:
```bash
qemu-system-x86_64 -cdrom jinn.iso -m 128M -no-reboot -d int,cpu_reset 2>qemu.log
```

---

## Estrutura de Código

```
src/
├── main.rs              # Ponto de entrada (_start) e sequência de boot
├── limine.rs            # Bindings do protocolo de boot Limine
├── vga.rs               # Renderizador de texto via Framebuffer
├── font.rs              # Dados da fonte em bitmap
├── cpu.rs               # Detecção de recursos da CPU (CPUID, MSR)
├── timer.rs             # Contador global de ticks
├── pit.rs               # Driver legado PIT (mantido como referência)
├── keyboard_buffer.rs   # Fila lock-free circular do teclado
├── shell.rs             # Shell interativo do kernel
├── scheduler.rs         # Escalonador Preemptivo (Round-Robin)
├── ipc.rs               # Passagem de mensagens entre processos (IPC)
├── syscall.rs           # Tabela de dispatch das chamadas de sistema
├── interrupts/
│   ├── mod.rs           # Dispatcher de IRQs e Exceções
│   ├── idt.rs           # Configuração da Tabela de Descritores de Interrupção
│   ├── pic.rs           # Controlador 8259 PIC (desativado via software)
│   ├── apic.rs          # Driver do Local APIC + APIC Timer
│   ├── ps2_keyboard.rs  # Tratamento de interrupção do Teclado PS/2
│   └── stubs.s          # Stubs em Assembly para os 48 vetores de interrupção
├── memory/
│   ├── mod.rs           # Índice do módulo de memória
│   ├── alloc.rs         # Alocador de Frames Físicos (Bitmap)
│   ├── paging.rs        # Paginação de 4 níveis x86_64 (PML4)
│   └── heap.rs          # Alocador de Heap Customizado (Bump + Free List)
└── vfs/
    ├── mod.rs           # Traits do VFS, tabela de montagem e resolução de paths
    └── ramfs.rs         # Sistema de arquivos em memória RAM (/dev/*)
scripts/
├── build.ps1            # Script wrapper para compilar o kernel
└── make_iso.ps1         # Construtor nativo de ISO9660 em PowerShell
```

---

## Áreas Abertas para Colaboração

| Área | Dificuldade | Descrição |
|---|---|---|
| **Userspace (Ring 3)** | Difícil | TSS, mudança de privilégios (GDT), pilhas de usuário |
| **IOAPIC / MSI** | Difícil | Substituir roteamento de IRQ legado por tabelas APIC modernas |
| **SMP (Multi-core)** | Muito Difícil | Inicialização de processadores AP, filas por CPU no escalonador |
| **Driver Ext2 / FAT32** | Média | Implementar a trait `FileSystem` para acesso real a disco |
| **Pilha de Rede** | Difícil | Driver RTL8139 ou VirtIO-Net + Camada TCP/IP |
| **Carregador de ELF** | Média | Carregar e executar binários ELF a partir do ramfs |
| **Teclado ABNT2** | Fácil | Adicionar o mapa de scancodes ABNT2 em `ps2_keyboard.rs` |
| **Novos Comandos** | Fácil | Implementar `ls`, `cat`, `echo`, `ps` no `shell.rs` |
| **Log via Serial QEMU**| Fácil | Redirecionar panic e logs para a porta serial COM1 |

---

## Estilo de Código

- **Sem `std`** — O kernel utiliza `#![no_std]`.
- **Zero Panics em Interrupções** — Interrupções de hardware não podem entrar em pânico nem alocar memória.
- **Blocos `unsafe`** — Devem sempre ser acompanhados de um comentário `// SAFETY:` explicando os invariantes.
- **Sem preenchimento implícito** — Se precisar de um buffer zerado, chame `.fill(0)` explicitamente.
- Funções públicas devem conter doc comments explicativos.

---

## Lembretes da Filosofia

> "Se você estiver adicionando uma abstração, pergunte-se: isso torna o kernel mais rápido ou menor? Se a resposta for não, isso não pertence ao kernel."

- Prefira alocação em **pilha (stack)** ao invés de **heap** em caminhos críticos do kernel.
- Prefira o uso de variáveis **atômicas** ao invés de locks sempre que possível.
- O escalonador é a lei. Nunca utilize `loop {}` sem incluir uma instrução `hlt` ou `yield`.
