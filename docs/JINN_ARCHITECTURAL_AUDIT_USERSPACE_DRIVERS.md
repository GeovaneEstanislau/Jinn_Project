# AUDITORIA ARQUITETURAL INICIAL: DRIVERS EM ESPAÇO DE USUÁRIO E MICROKERNEL JINN
## Mapeamento de Dependências de Hardware, Fronteiras de Privilégio e Plano Sequencial de Migração

**Projeto:** Jinn OS & Aladdin Desktop
**Microkernel:** Jinn Microkernel (x86_64, `#![no_std]`)
**Data:** 08/11/2026
**Status do Laudo:** Auditoria Concluída — Linha de Base Estabelecida

---

## 1. Sumário Executivo

O objetivo desta auditoria é preparar a evolução do microkernel Jinn para uma arquitetura adaptativa com drivers executando em **espaço de usuário (Ring 3)** sob supervisão estrita, reduzindo o código em Ring 0 ao mínimo indispensável, conforme preconizado pelos princípios de sistemas operacionais tolerantes a falhas.

A auditoria confirma que, atualmente, **todos os 16 drivers e controladores de hardware do sistema executam em Ring 0 (CPL=0)** dentro do binário do kernel. Embora o `DriverSupervisor` (`src/drivers/supervisor.rs`) implemente uma máquina de estados com contenção de falhas e o `DeviceManager` (`src/drivers/device_manager.rs`) centralize o inventário de dispositivos, **não há isolamento de memória por processo entre os drivers e o núcleo do kernel**.

Para permitir uma migração segura, esta auditoria mapeia detalhadamente cada dependência de baixo nível (portas I/O, MMIO, DMA, interrupções e barramentos) e define uma sequência de migração progressiva, estabelecendo a **Etapa 1** (Infraestrutura de Driver Manager e driver de entrada PS/2 em Ring 3) como o primeiro passo tecnicamente seguro.

---

## 2. Inventário Completo dos Controladores e Drivers Existentes

O microkernel Jinn possui atualmente 16 controladores de hardware e subsistemas periféricos implementados:

| Nº | Controlador / Driver | Arquivo Fonte | Categoria | Barramento | Ring Atual |
| :---: | :--- | :--- | :--- | :--- | :---: |
| **01** | **IDE / ATA Primário & Secundário** | `src/drivers/ide.rs` | Storage | I/O Ports / BMIDE | **Ring 0** |
| **02** | **SATA AHCI** | `src/drivers/ahci.rs` | Storage | PCI MMIO (BAR 5) | **Ring 0** |
| **03** | **NVMe PCIe** | `src/drivers/nvme.rs` | Storage | PCI MMIO (BAR 0) | **Ring 0** |
| **04** | **VirtIO-Block** | `src/drivers/virtio_blk.rs` | Storage | PCI I/O & DMA | **Ring 0** |
| **05** | **VirtIO-Net** | `src/drivers/net/virtio.rs` | Network | PCI I/O & DMA | **Ring 0** |
| **06** | **Intel PRO/1000 (e1000)** | `src/drivers/net/e1000.rs` | Network | PCI MMIO (BAR 0) | **Ring 0** |
| **07** | **AMD PCnet-FAST III** | `src/drivers/net/pcnet.rs` | Network | PCI I/O & DMA | **Ring 0** |
| **08** | **Realtek RTL8139 / 8168** | `src/drivers/net/realtek.rs` | Network | PCI I/O (Sondado) | **Ring 0** |
| **09** | **Teclado PS/2 (Intel 8042)** | `src/interrupts/ps2_keyboard.rs`| Input | I/O Ports `0x60`/`0x64` | **Ring 0** |
| **10** | **Mouse PS/2 (Intel 8042)** | `src/interrupts/ps2_mouse.rs` | Input | I/O Ports `0x60`/`0x64` | **Ring 0** |
| **11** | **Limine Framebuffer (GOP)** | `src/gui.rs`, `src/limine.rs` | Display | Linear VRAM (MMIO) | **Ring 0** |
| **12** | **VGA Text Mode Console** | `src/vga.rs` | Display | MMIO `0xB8000` (HHDM) | **Ring 0** |
| **13** | **UART Serial 16550 (COM1)** | `src/main.rs` | Character | I/O Port `0x3F8` | **Ring 0** |
| **14** | **Temporizadores (PIT 8254 & APIC)**| `src/timer.rs`, `src/interrupts/apic.rs` | System | I/O `0x40` / MMIO | **Ring 0** |
| **15** | **PIC 8259A Legado & APIC** | `src/interrupts/pic.rs`, `apic.rs` | System | I/O `0x20`/`0xA0` / MMIO | **Ring 0** |
| **16** | **Barramento PCI (Config & Scan)**| `src/drivers/pci.rs` | Bus | I/O `0xCF8`/`0xCFC` | **Ring 0** |

---

## 3. Classificação Atual: Ring 0 vs Ring 3

### 3.1. Componentes Executando em Ring 0 (CPL=0)
1. **Núcleo do Kernel:** Escalonador preemptivo SMP, GDT/TSS, IDT com 48 vetores de interrupção, chaveamento de páginas CR3.
2. **Gerenciador de Memória Física:** Buddy Allocator (32 GiB), alocador dinâmico de Heap (16 MiB).
3. **Todos os 16 Drivers de Dispositivos:** Acesso irrestrito a instruções privilegiadas (`inb`, `outb`, `inw`, `outw`, `cli`, `sti`, `invlpg`, `rdmsr`, `wrmsr`).
4. **Pilha de Rede Completa:** Protocolos Ethernet, ARP, IPv4, ICMP, UDP, TCP, DHCP, DNS e Firewall JinnWall SPI.
5. **Sistemas de Arquivos e Armazenamento:** VFS montado, ramfs, devfs, driver FAT32, driver Ext2, cache primário LRU de blocos.
6. **Desktop Compositor Aladdin:** A sessão gráfica (`AladdinSession`) roda em Ring 0 na função `gui::run_desktop()`, invocada pelo shell do kernel.
7. **Motor JDA (Dataflow Engine):** Anéis IoRing de mensagens e buffers compartilhados.

### 3.2. Componentes Executando em Ring 3 (CPL=3)
1. **Processos ELF de Usuário:** Carregados pelo ELF Loader em memória virtual de usuário (`0x0000_1000_0000` a `0x0000_7FFF_FFFF_FFFF`):
   - `apps/ola-mundo` (Suite principal de processos em Ring 3: tarefa principal, worker com cota de CPU de 25 ticks e tarefa periódica).
   - `apps/ola-grafico` (Aplicação gráfica com buffer de janela).
   - `apps/contador` (Aplicação de computação aritmética).
   - `apps/browser` (`browser.elf` - Processo de navegação web com arena DOM e CSS).
2. **Fronteira de Acesso:** Os processos Ring 3 só interagem com o sistema por meio de interrupção de software (`int 0x80` ou `syscall`), através de 53 syscalls padronizadas em `src/syscall.rs`.

---

## 4. Mapeamento de Dependências de Baixo Nível

Para migrar qualquer driver para o Ring 3, é fundamental auditar como ele se comunica com o hardware:

```
                            MAPA DE DEPENDÊNCIAS DE HARDWARE
┌─────────────────────────────────┬─────────────────────────────────┬─────────────────────────────────┐
│     PORTAS I/O x86 (in/out)     │         MMIO (Memória)          │       DMA (Memória Física)      │
├─────────────────────────────────┼─────────────────────────────────┼─────────────────────────────────┤
│ • IDE: 0x1F0-0x1F7, 0x3F6       │ • AHCI: BAR 5 (ABAR, 4-8 KiB)   │ • IDE: PRDT BMIDE (4K físico)   │
│ • PS/2: 0x60, 0x64              │ • NVMe: BAR 0 (4-16 KiB MMIO)   │ • AHCI: CmdList, FIS, PRD (4K)  │
│ • PCI: 0xCF8, 0xCFC             │ • e1000: BAR 0 (128 KiB MMIO)   │ • NVMe: SQ/CQ Rings & PRPs (4K) │
│ • COM1: 0x3F8-0x3FF             │ • Limine FB: VRAM (ex: 0xFD000) │ • VirtIO-Blk: VirtQueue Rings   │
│ • VirtIO-Blk: BAR 0 I/O         │ • Local APIC: 0xFEE00000        │ • VirtIO-Net: RX/TX VirtQueues  │
│ • VirtIO-Net: BAR 0 I/O         │ • VGA Text: 0xB8000             │ • e1000: RX/TX Descriptor Rings │
│ • PCnet: BAR 0 I/O (RAP/RDP)    │                                 │ • PCnet: InitBlock & Descriptors│
│ • PIT/PIC: 0x20, 0x40, 0xA0     │                                 │                                 │
└─────────────────────────────────┴─────────────────────────────────┴─────────────────────────────────┘
```

### 4.1. Interrupções de Hardware (IRQs)
- **IRQ 0:** PIT / APIC Timer (Preempção do escalonador).
- **IRQ 1:** Teclado PS/2 (Controlador 8042).
- **IRQ 4:** UART COM1 (Porta Serial).
- **IRQ 12:** Mouse PS/2 (Pacotes de 3 bytes).
- **IRQ 14/15:** IDE Canal Primário / Secundário.
- **Linhas PCI IRQ / MSI:** Compartilhadas dinamicamente entre VirtIO-Net, e1000, AHCI e NVMe.

---

## 5. Análise de Viabilidade de Migração para Ring 3

Classificamos a viabilidade de migração para o espaço de usuário em quatro níveis:

| Categoria | Controladores | Complexidade | Riscos Identificados | Pré-Requisitos no Kernel |
| :--- | :--- | :---: | :--- | :--- |
| **Fase 1: Entrada & Terminal** | PS/2 Keyboard, PS/2 Mouse, COM1 Serial | **Baixa** | Perda de teclas ou dessincronização de pacotes do mouse se a tarefa do driver for preemptada. | Fila circular de eventos IPC no kernel e tratamento mínimo na ISR de Ring 0. |
| **Fase 2: Rede** | VirtIO-Net, e1000, PCnet, JinnWall | **Média** | Latência de entrega de pacotes; overhead de IPC; necessidade de concessão de buffers DMA contíguos. | Syscall de alocação de buffer DMA seguro (`sys_dma_alloc`) e mapeamento BAR. |
| **Fase 3: Armazenamento** | IDE, VirtIO-Blk, AHCI, NVMe | **Alta** | Corrupção de disco FAT32/Ext2 se requisições falharem no meio da gravação; race condition em DMA. | IOMMU ou buffers de contingência verificados pelo kernel; canal assíncrono IoRing. |
| **Fase 4: Vídeo & GUI** | Aladdin Compositor, Limine Framebuffer | **Alta** | Queda de framerate de desenho se houver cópia excessiva entre processos de clientes e compositor. | Mapeamento de memória compartilhada Zero-Copy para o backbuffer do Framebuffer. |

---

## 6. Funcionalidades Ainda Não Implementadas

A auditoria identificou lacunas arquiteturais que precisam ser construídas:
1. **Infraestrutura de Driver em Espaço de Usuário (Ring 3 Driver Framework):**
   - Não há um protocolo padronizado de IPC para drivers (`DriverIpcProtocol`).
   - Não há syscalls para permitir que um processo em Ring 3 receba notificações de interrupção (`sys_driver_irq_wait`).
   - Não há primitivas controladas para concessão de portas I/O ou mapeamento de BAR MMIO no espaço virtual de um processo específico.
2. **TSS I/O Permission Bitmap (IOPB):**
   - O TSS em `src/gdt.rs` não possui o bitmap de 8 KiB para liberar seletivamente portas I/O para processos sem que disparem `#GP`.
3. **Subsistema de Áudio e USB:**
   - Drivers de áudio (Intel HDA/AC97) e controladores USB (UHCI, EHCI, XHCI) ainda não possuem implementação física operacional.
4. **Jinn Memory Intelligence (Paginação e Compressão):**
   - O alocador Buddy e o Heap gerenciam frames físicos contíguos, mas ainda não possuem swap em disco nem compressão de páginas em RAM (ZRAM).

---

## 7. Linha de Base dos Testes Existentes

Antes de qualquer modificação no código-fonte, a linha de base de estabilidade foi registrada:

- **Suíte de Testes do Aladdin Desktop:** **43/43 Testes Aprovados** (`cargo test --manifest-path E:\Projetos\Aladdin\Cargo.toml`).
- **Selftests do Kernel Jinn:**
  - JDA Engine & IoRing: **4/4 Aprovados**.
  - Driver Supervisor Fault Injection: **6/6 Aprovados**.
  - ELF Loader Security Tests: **5/5 Aprovados**.
  - Validação de Syscalls EFAULT/EINVAL: **Aprovada**.
  - DNS RFC 1035 Cache: **5/5 Aprovados**.
  - JinnWall SPI Policy: **5/5 Aprovados**.
  - TCP RFC 793 Protocol: **6/6 Aprovados**.
- **Validação Turnkey de Memória (JAHA 1.2):** `test_jaha_12_integrity.py` executado com **0 bytes de discrepância** ($\Delta = 0$) em ambiente QEMU com perfil de 256 MB.

---

## 8. Matriz de Riscos Técnicos e Medidas de Mitigação

| Risco Técnico | Probabilidade | Impacto | Medida de Mitigação Obrigatória |
| :--- | :---: | :---: | :--- |
| **Overhead de Troca de Contexto por Interrupção** | Alta | Médio | Não encaminhar cada interrupção individualmente via syscall; a ISR de Ring 0 armazena dados em fila compartilhada e sinaliza a tarefa via IPC orientado a eventos. |
| **Driver em Ring 3 Trava ou Entra em Loop** | Média | Alto | O `DriverSupervisor` atua como watchdog com timeout configurável e quotas de ticks de CPU (`max_cpu_ticks`). |
| **Acesso Ilegal a Dispositivos por Processos Não-Autorizados** | Média | Crítico | O kernel valida privilégio e ID de capacidade do processo antes de conceder acesso a descritores de hardware. |
| **Corrupção de Memória do Kernel via DMA** | Baixa | Catastrófico | Buffers DMA só podem ser alocados pelo kernel em frames explicitamente marcados como DMA-safe; drivers em Ring 3 recebem ponteiros virtuais mapeados em seu PML4. |
| **Regressão nos 43 Testes do Aladdin Desktop** | Média | Alto | Manter a interface atual do `gui.rs` como fallback compatível enquanto o novo modelo de drivers é testado. |

---

## 9. Plano Sequencial de Migração

Para atender aos princípios obrigatórios de não comprometer a estabilidade e testar incrementalmente:

```
                      SEQUÊNCIA DE MIGRAÇÃO PROGRESSIVA DE DRIVERS
┌────────────────────────────────────────────────────────────────────────────────────────┐
│ ETAPA 1: Infraestrutura de Driver Manager e Drivers de Entrada PS/2 (Teclado e Mouse)  │
│          • Criar primitivas IPC de hardware e filas de eventos de entrada no kernel    │
│          • Mover decodificação de scancodes e estado do mouse para serviço em Ring 3   │
│          • Validar no Aladdin Desktop sem alterações nos drivers de disco ou rede      │
└──────────────────────────────────────────┬─────────────────────────────────────────────┘
                                           │
                                           ▼
┌────────────────────────────────────────────────────────────────────────────────────────┐
│ ETAPA 2: Pilha de Rede e Drivers Ethernet (VirtIO-Net / e1000)                         │
│          • Implementar buffers de descritores DMA mapeados em espaço de usuário        │
│          • Migrar processamento de pacotes Ethernet/IP/TCP para serviço de rede Ring 3 │
│          • Kernel mantém apenas a ISR mínima e controle do anel físico PCI             │
└──────────────────────────────────────────┬─────────────────────────────────────────────┘
                                           │
                                           ▼
┌────────────────────────────────────────────────────────────────────────────────────────┐
│ ETAPA 3: Controladores de Armazenamento e Sistema de Arquivos (VirtIO-Blk / IDE / AHCI)│
│          • Implementar canal de comandos assíncronos IoRing com validação de LBA       │
│          • Serviços VFS em espaço de usuário mantendo Ring 0 estritamente transacional │
└──────────────────────────────────────────┬─────────────────────────────────────────────┘
                                           │
                                           ▼
┌────────────────────────────────────────────────────────────────────────────────────────┐
│ ETAPA 4: Compositor Aladdin e Framebuffer Gráfico em Ring 3                            │
│          • Mapeamento Zero-Copy da VRAM linear para processo exclusivo do Aladdin      │
│          • Janelas de clientes comunicam-se via protocolo de superfície compartilhada  │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 10. Especificação da Etapa 1: A Primeira Etapa Segura

A **Etapa 1** é a transição tecnicamente mais segura e com menor raio de destruição do sistema:

### Componentes Afetados na Etapa 1:
- `src/drivers/device_manager.rs`: Evoluir para registrar drivers em espaço de usuário, atribuir quotas de recursos e gerenciar permissões.
- `src/syscall.rs`: Adicionar syscalls de controle de drivers:
  - `sys_driver_register(device_id, flags)`
  - `sys_driver_event_poll(device_id, out_event_ptr, max_events)`
  - `sys_driver_send_input(event_type, code, value)`
- `src/interrupts/ps2_keyboard.rs` e `ps2_mouse.rs`: As ISRs de interrupção física (IRQ 1 e IRQ 12) em Ring 0 apenas lêem as portas `0x60`/`0x64` e depositam os bytes brutos em uma fila circular de hardware sem bloquear o sistema.
- `src/services/input_server/` ou daemon de entrada Ring 3: Consome os bytes brutos, converte scancodes em caracteres UTF-8, decodifica o protocolo de 3 bytes do mouse PS/2 e despacha eventos limpos para o Aladdin Desktop.

### Critérios de Aceite para a Etapa 1:
1. Todos os 43 testes do Aladdin Desktop continuam aprovados (100% de aprovação).
2. O teclado e o mouse continuam funcionando interativamente no shell e no desktop gráfico em ambiente QEMU com 256 MB.
3. Se o processo do driver de entrada em Ring 3 falhar ou for finalizado, o microkernel permanece estável e o `DriverSupervisor` reinicia o serviço automaticamente sem Kernel Panic.
4. Telemetria serial comprova a operação da fila de eventos com zero polling desnecessário.

---
*Relatório de auditoria técnica aprovado e homologado para orientar a implementação incremental da Etapa 1.*
