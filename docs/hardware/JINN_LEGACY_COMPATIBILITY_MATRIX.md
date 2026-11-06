# JINN OS — MATRIZ DE COMPATIBILIDADE DE HARDWARE LEGADO 1.0
## Classificação Arquitetural, Conjunto de Instruções e Limites de Plataforma

**Data:** 06/11/2026  
**Status:** Etapa 1 Aprovada (Auditoria Arquitetural)  
**Kernel:** Jinn Microkernel (x86_64)  
**Escopo:** Compatibilidade com processadores, memórias e chipsets históricos (2003–2026)  

---

## 1. Classificação Fundamental: Legacy x86_64 vs. Legacy x86 32-bit

O microkernel Jinn é nativamente compilado para a arquitetura de 64 bits (`x86_64-unknown-none`). Para evitar ambiguidades técnicas, o ecossistema Jinn divide o hardware histórico em duas categorias rigorosas:

```
                           ESPECTRO DE COMPATIBILIDADE HISTÓRICA
+----------------------------------------------------+-----------------------------------------------+
|             CATEGORIA 1: LEGACY x86_64            |           CATEGORIA 2: LEGACY 32-BIT          |
|                 (TOTALMENTE SUPORTADO)             |              (INVESTIGAÇÃO FUTURA)            |
+----------------------------------------------------+-----------------------------------------------+
| • Suporte nativo ao Long Mode (EFER.LME/LMA)       | • Ausência de Long Mode (CPUID 0x80000001.LM=0)|
| • SSE e SSE2 disponíveis na arquitetura             | • Barramentos estritamente de 32 bits         |
| • Paginação de 4 níveis (PML4)                     | • Paginação de 2 níveis ou PAE (3 níveis)     |
| • Syscalls x86_64 (SYSCALL / SYSRET com LSTAR)     | • Chamadas via INT 0x80 ou SYSENTER/SYSEXIT   |
| • Ex: AMD Athlon 64 (2003), Opteron (2003),        | • Ex: Pentium III, Pentium 4 Northwood (2002),|
|   Pentium 4 Prescott 6xx (2004), Core 2 Duo (2006) |   AMD Athlon XP (Socket A / K7), Duron        |
+----------------------------------------------------+-----------------------------------------------+
```

---

## 2. Requisitos de CPUID e Instruções Mandatórias (x86_64)

Para que uma CPU execute o Jinn OS com estabilidade, os seguintes recursos de arquitetura são auditados em tempo de boot pelo subsistema `cpu.rs` e `hardware_profiler`:

| Recurso / Flag CPUID | Posição CPUID | Finalidade no Jinn OS | Estado no Jinn |
| :--- | :--- | :--- | :--- |
| **Long Mode (LM)** | `0x80000001:EDX[29]` | Registradores de 64 bits (RAX..R15, RIP) e espaço de 48 bits | **Obrigatório** (Falha fatal se ausente) |
| **FPU (x87)** | `0x00000001:EDX[0]` | Unidade de ponto flutuante básica | **Obrigatório** |
| **TSC** | `0x00000001:EDX[4]` | Time-Stamp Counter para medição de ciclos RDTSC | **Obrigatório** |
| **MSR** | `0x00000001:EDX[5]` | Registradores de configuração de modelo (EFER, LSTAR, STAR) | **Obrigatório** |
| **PAE** | `0x00000001:EDX[6]` | Suporte a endereçamento físico estendido de páginas | **Obrigatório** |
| **APIC** | `0x00000001:EDX[9]` | Controlador de interrupções local avançado (LAPIC) | **Obrigatório** (Fallback PIC 8259) |
| **SSE** | `0x00000001:EDX[25]` | Instruções SIMD de precisão simples | **Obrigatório** (Ativado em CR4.OSFXSR) |
| **SSE2** | `0x00000001:EDX[26]` | Instruções SIMD de precisão dupla (padrão ABI x86_64) | **Obrigatório** (Ativado em CR4.OSFXSR) |
| **NX / XD (NXE)** | `0x80000001:EDX[20]` | No-Execute Page Protection (W^X security) | **Obrigatório** (Ativado em EFER.NXE) |
| **CMPXCHG16B** | `0x00000001:ECX[13]` | Operação atômica de 128 bits | Suportado se presente |
| **Invariant TSC** | `0x80000007:EDX[8]` | TSC invariante com relação a frequência e ACPI P-States | Detectado e utilizado se disponível |

---

## 3. Avaliação de Viabilidade e Custos de Portabilidade para 32-bit (i686)

Conforme a diretriz estratégica, **não haverá migração ou bifurcação do kernel para 32 bits neste ciclo**. Abaixo encontra-se o estudo de impacto técnico para uma futura abstração de arquitetura:

1. **Camada de Paginação:**
   - O Jinn atual utiliza uma tabela de 4 níveis (`PML4 -> PDPT -> PD -> PT`) manipulada em `src/memory/paging.rs`.
   - Um kernel 32 bits exigiria implementar ou PAE de 3 níveis (`PDPTR -> PD -> PT` com ponteiros físicos de 36 bits e lógicos de 32 bits) ou paginação não-PAE de 2 níveis (`PD -> PT` limitando a 4 GB físicos).
2. **Modelo de Syscalls e Vetores:**
   - A instrução `syscall` em x86 32-bit não carrega automaticamente o seletor `CS` em modo de 32 bits na maioria dos processadores Intel antigos. Seria necessário implementar stubs via `int 0x80` ou `sysenter`/`sysexit`.
3. **Estruturas de Contexto de Tarefa (`TaskContext`):**
   - Redução dos registradores de 64 bits (`rdi, rsi, rdx, rcx, r8..r15`) para 32 bits (`eax, ebx, ecx, edx, esi, edi, ebp, esp`).
4. **Alocação de BSS e Bitmaps:**
   - Redução do bitmap do Buddy Allocator de 128 KiB para 32 KiB (1 MiB / 32 KiB para 1 GB).
5. **Decisão:** Manter a elegância e segurança do x86_64, aproveitando computadores legados compatíveis com 64 bits (processadores a partir de 2003/2004).

---

## 4. Matriz de Compatibilidade de Controladores e Periféricos Históricos

| Barramento / Dispositivo | Tecnologia / Padrão | Driver Jinn OS | Estado de Implementação |
| :--- | :--- | :--- | :--- |
| **Armazenamento Primário** | IDE / PATA (PIIX3, ICH) LBA28/LBA48 | `src/drivers/ide.rs` | **Implementado e Validado** (Portas 0x1F0/0x170) |
| **Armazenamento SATA** | AHCI SATA 1.0 / 2.0 / 3.0 | `src/drivers/ahci.rs` | **Implementado e Validado** (PCI Class 0x01, Subclass 0x06) |
| **Armazenamento Moderno** | NVMe PCIe / VirtIO Blk | `src/drivers/nvme.rs`, `virtio_blk.rs` | **Implementado e Validado** |
| **Rede Histórica** | Realtek RTL8139 / Intel PRO/1000 (e1000) | `src/drivers/net/e1000.rs` | **Implementado e Validado** (PCI Device 0x100E) |
| **Rede Virtualizada** | VirtIO-Net (PCI) | `src/drivers/net/virtio.rs` | **Implementado e Validado** (PCI Vendor 0x1AF4) |
| **Vídeo / Framebuffer** | Limine VESA / GOP / VGA | `src/gui.rs`, `limine.rs` | **Implementado e Validado** (BPP 16/24/32) |
| **Entrada (Input)** | Teclado PS/2 e Mouse PS/2 (IRQ 12) | `interrupts/ps2_keyboard.rs`, `ps2_mouse.rs` | **Implementado e Validado** (Portas 0x60/0x64) |
| **Barramento PCI** | PCI Configuration Mechanism #1 | `src/drivers/pci.rs` | **Implementado e Validado** (Portas 0xCF8/0xCFC) |
| **Temporizador** | PIT 8254 (100 Hz) e APIC Timer | `pit.rs`, `timer.rs` | **Implementado e Validado** |

