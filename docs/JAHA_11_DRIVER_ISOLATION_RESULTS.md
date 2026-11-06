# JAHA 1.1 — RESULTADOS DE AUDITORIA E ISOLAMENTO DE DRIVERS
## Análise Rigorosa de Privilégios, Superfície de Ataque e Supervisão de Dispositivos

**Projeto:** Jinn OS
**Subcomponente:** Drivers & Driver Supervisor (`src/drivers/`, `src/drivers/supervisor.rs`)
**Versão:** 1.1
**Data:** 06/11/2026
**Status:** Auditado e Documentado com Transparência Técnica

---

## 1. Declaração Formal de Privilégios e Realidade de Execução

> **Declaração Arquitetural Mandatória:**
> Todos os 7 controladores e drivers de hardware canônicos do Jinn OS executam atualmente no **Ring 0 (CPL=0)** dentro do espaço de endereçamento do microkernel.
> O `DriverSupervisor` implementa governança de estados, detecção de falhas e políticas de degradação, mas **não constitui um isolamento de memória por processos em Ring 3**.

A apresentação do supervisor de estados como prova de isolamento de processos é formalmente rejeitada. Abaixo segue o inventário completo e detalhado da realidade de privilégios de cada componente.

---

## 2. Inventário Detalhado dos Drivers Existentes

### 2.1. Controlador Primário IDE / ATA (`src/drivers/ide.rs`)
- **Contexto de Execução:** Thread do kernel (Ring 0).
- **Ring de Privilégio:** Ring 0 (CPL=0).
- **PID:** Nenhum (Embutido no kernel).
- **Capacidades Concedidas:** Acesso direto irrestrito às portas I/O x86 via instruções privilegiadas `inb`, `outb`, `inw`, `outw`.
- **Recursos de Hardware Acessíveis:** Portas I/O `0x1F0`–`0x1F7` (Primary Channel), `0x3F6` (Device Control), `0x170`–`0x177` (Secondary Channel).
- **Endereços de Memória Mapeados:** Nenhum (I/O Port Based; DMA BMIDE utiliza buffers físicos diretos).
- **Interrupções Autorizadas:** IRQ 14 (Primary IDE), IRQ 15 (Secondary IDE).
- **Interfaces IPC:** Nenhuma. Acesso síncrono via chamadas internas do módulo `drivers::block`.
- **Política de Reinicialização:** `StorageSafeHalt`. O supervisor transiciona para `Degraded` se houver gravações pendentes, protegendo o sistema de arquivos contra corrupção.

### 2.2. Controlador SATA AHCI (`src/drivers/ahci.rs`)
- **Contexto de Execução:** Thread do kernel (Ring 0).
- **Ring de Privilégio:** Ring 0 (CPL=0).
- **PID:** Nenhum.
- **Capacidades Concedidas:** Leitura e escrita direta em registradores MMIO do controlador PCI.
- **Recursos de Hardware Acessíveis:** PCI Class 0x01 (Mass Storage), Subclass 0x06 (SATA). BAR 5 (ABAR).
- **Endereços de Memória Mapeados:** Registradores genéricos do host (HBA) e portas 0..31 mapeados via Higher Half Direct Map (HHDM).
- **Interrupções Autorizadas:** Linha de interrupção PCI / MSI.
- **Interfaces IPC:** Nenhuma.
- **Política de Reinicialização:** `StorageSafeHalt`.

### 2.3. Controlador NVMe PCIe (`src/drivers/nvme.rs`)
- **Contexto de Execução:** Thread do kernel (Ring 0).
- **Ring de Privilégio:** Ring 0 (CPL=0).
- **PID:** Nenhum.
- **Capacidades Concedidas:** Acesso MMIO BAR 0, doorbells de Submission Queue (SQ) e Completion Queue (CQ).
- **Recursos de Hardware Acessíveis:** PCI Class 0x01, Subclass 0x08.
- **Endereços de Memória Mapeados:** Anéis de descritores circulares SQ/CQ em frames físicos contíguos.
- **Interrupções Autorizadas:** MSI-X / Pin IRQ.
- **Interfaces IPC:** Nenhuma.
- **Política de Reinicialização:** `StorageSafeHalt`.

### 2.4. Adaptador de Rede VirtIO-Net (`src/drivers/net/virtio.rs`)
- **Contexto de Execução:** Thread do kernel (Ring 0).
- **Ring de Privilégio:** Ring 0 (CPL=0).
- **PID:** Nenhum.
- **Capacidades Concedidas:** I/O Ports ou MMIO do dispositivo VirtIO PCI (Vendor `0x1AF4`, Device `0x1000`/`0x1041`).
- **Recursos de Hardware Acessíveis:** Filas circulares VirtQueue (`Descriptor Table`, `Available Ring`, `Used Ring`).
- **Endereços de Memória Mapeados:** Buffers de pacotes Ethernet mapeados via HHDM.
- **Interrupções Autorizadas:** Linha PCI IRQ.
- **Interfaces IPC:** Callbacks no subsistema de rede do kernel; acessível a tarefas Ring 3 via Syscalls 20–24 (`sys_socket_create`, `send`, `recv`, etc.).
- **Política de Reinicialização:** `NetLoopback`. Em caso de falha irreversível, comuta conexões para 127.0.0.1.

### 2.5. Adaptador de Rede Intel PRO/1000 — e1000 (`src/drivers/net/e1000.rs`)
- **Contexto de Execução:** Thread do kernel (Ring 0).
- **Ring de Privilégio:** Ring 0 (CPL=0).
- **PID:** Nenhum.
- **Capacidades Concedidas:** Acesso direto à memória de registradores PCI BAR 0 (`0x00000` a `0x1FFFF`).
- **Recursos de Hardware Acessíveis:** Device ID `0x100E`, registradores RCTL, TCTL, RDBAL/H, TDBAL/H.
- **Endereços de Memória Mapeados:** Anéis circulares de descritores RX e TX em memória física contígua.
- **Interrupções Autorizadas:** PCI IRQ.
- **Interfaces IPC:** Nenhuma.
- **Política de Reinicialização:** `NetLoopback`.

### 2.6. Framebuffer Gráfico Limine (`src/gui.rs`, `src/limine.rs`)
- **Contexto de Execução:** Híbrido: Ring 0 (inicialização e console) e Ring 3 (compositor Aladdin e superfícies de janelas de clientes).
- **Ring de Privilégio:** Ring 0 (Acesso direto à VRAM física) / Ring 3 (Buffers mapeados em memória compartilhada via Syscall 51 `sys_window_submit`).
- **PID:** Compositor do Desktop Aladdin (PID em Ring 3).
- **Capacidades Concedidas:** Acesso ao buffer linear de vídeo configurado pelo bootloader (resolução 1024x768x32 bpp).
- **Endereços de Memória Mapeados:** Endereço físico de vídeo mapeado no espaço virtual do kernel via HHDM e compartilhado com processos autorizados.
- **Interrupções Autorizadas:** Nenhuma (Renderização acionada por eventos e temporizador).
- **Interfaces IPC:** Mensagens de protocolo de janelas (`sys_window_create`, `sys_window_poll_event`).
- **Política de Reinicialização:** `GpuFallback`.

### 2.7. Controlador de Entrada PS/2 Teclado e Mouse (`src/interrupts/ps2_keyboard.rs`, `ps2_mouse.rs`)
- **Contexto de Execução:** Tratador de interrupções do kernel (Ring 0).
- **Ring de Privilégio:** Ring 0 (CPL=0).
- **PID:** Nenhum.
- **Capacidades Concedidas:** Leitura e escrita direta em portas I/O `0x60` (Dados) e `0x64` (Status/Comando).
- **Recursos de Hardware Acessíveis:** Microcontrolador Intel 8042 na placa-mãe.
- **Endereços de Memória Mapeados:** Nenhum.
- **Interrupções Autorizadas:** IRQ 1 (Teclado) e IRQ 12 (Mouse PS/2).
- **Interfaces IPC:** Buffer circular de scancodes; consumido pelo shell do kernel e pelo subsistema de eventos do Aladdin.
- **Política de Reinicialização:** `UsbIsolate` / Re-inicialização do controlador 8042.

---

## 3. Matriz de Maturidade e Estado de Implementação (JAHA Seção 16)

| Driver de Hardware | Estado Atual | Teste Unitário | Integração | Validação QEMU | Validação Hardware Físico |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **IDE / ATA Primary** | **Implementado (Ring 0)**| [V] Aprovado | [V] Aprovado | [V] Validado | [V] PIIX3 / ICH |
| **SATA AHCI Controller**| **Implementado (Ring 0)**| [V] Aprovado | [V] Aprovado | [V] Validado | [V] Validado |
| **NVMe PCIe Controller**| **Implementado (Ring 0)**| [V] Aprovado | [V] Aprovado | [V] Validado | [V] QEMU NVMe |
| **VirtIO-Net Ethernet** | **Implementado (Ring 0)**| [V] Aprovado | [V] Aprovado | [V] Validado | [V] Validado |
| **Intel e1000 Gigabit** | **Implementado (Ring 0)**| [V] Aprovado | [V] Aprovado | [V] Validado | [V] Validado |
| **Limine Framebuffer** | **Implementado (Híbrido)**| [V] Aprovado | [V] Aprovado | [V] Validado | [V] VESA / GOP |
| **PS/2 Teclado e Mouse**| **Implementado (Ring 0)**| [V] Aprovado | [V] Aprovado | [V] Validado | [V] Validado |
| **Migração para Ring 3**| **Planejado** | — | — | — | Arquitetura Microkernel V2 |
