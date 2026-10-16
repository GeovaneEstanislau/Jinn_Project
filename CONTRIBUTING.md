# Contribuindo para o Jinn OS

Obrigado pelo interesse no Jinn OS, um kernel bare-metal x86_64 escrito em Rust.

---

## Primeiros Passos

### 1. Configurar o ambiente

```powershell
# Instalar a versão nightly do Rust
rustup toolchain install nightly --component rust-src --component llvm-tools-preview

# Instalar os componentes necessários ao build
rustup target add x86_64-unknown-none --toolchain nightly
```

### 2. Clonar e Compilar

```bash
git clone https://github.com/GeovaneEstanislau/Jinn_Project.git
cd Jinn_Project
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\run.ps1 -NoRun
```

### 3. Executar

O launcher baixa os binários do Limine se eles ainda não estiverem no clone. A criação da ISO requer `mkisofs`, `genisoimage` ou `xorriso`. No Windows, `oscdimg` também é aceito como alternativa.

**VirtualBox**: Anexe `jinn.iso` como CD-ROM e use uma VM x86_64.

**QEMU**:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\run.ps1
```

---

## Estrutura de Código

```
src/                 Código do kernel
apps/ola-mundo/      App bare-metal de exemplo
scripts/build.ps1    Build do kernel e do app
scripts/run.ps1      Criação da ISO e execução opcional no QEMU
iso_root/limine.conf Configuração de boot
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
