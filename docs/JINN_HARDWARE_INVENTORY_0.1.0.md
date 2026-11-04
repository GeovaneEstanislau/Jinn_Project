# JINN PROJECT — HARDWARE INVENTORY 0.1.0
## Inventário de Hardware, Plataformas Virtuais e Topologia de Dispositivos

- **Versão do Documento:** 0.1.0
- **Data:** 04/11/2026
- **Status:** Oficial / Auditado
- **Subsistema:** Jinn Microkernel Core & Hardware Abstraction
- **Invariante Arquitetural:** Jinn permanece como microkernel central e compartilhado. Aladdin (Desktop), Simbad (Servidor) e Ifrit (Industrial) são perfis e manifestações especializadas, estritamente **sem forks de kernel**.

---

## 1. Contexto e Metodologia de Inventário

O inventário de hardware do ecossistema Jinn foi conduzido através de inspeção estática do código-fonte dos drivers (`src/drivers/`, `src/interrupts/`, `src/vga.rs`, `src/memory/`), análise da inicialização do kernel (`src/main.rs`, `src/drivers/device_manager.rs`), parametrização do emulador QEMU (`scripts/run.ps1`) e testes contratuais reproduzíveis no harness de validação (`tools/jda-sim/tests/hardware_benchmark_tests.rs`).

### Princípios de Auditoria
1. **Diferenciação Estrita entre Enumeração e Operação:** A simples aparição de um dispositivo durante a varredura do barramento PCI (via portas 0xCF8/0xCFC) não constitui suporte de hardware. O suporte operacional exige que um driver específico reconheça o dispositivo, configure registradores de controle/DMA, processe interrupções e execute leitura/escrita com tratamento de erros.
2. **Separação entre Virtualização e Hardware Físico:** As medições sob emulação QEMU TCG refletem ciclos de instrução virtuais e contenção do hospedeiro, não devendo ser confundidas com timings de silício físico bare-metal.
3. **Maturidade Transparente:** Dispositivos sem drivers anexados são formalmente catalogados como *Não Suportados* ou *Descobertos Não Operacionais*.

---

## 2. Inventário da Plataforma de Execução Virtual (QEMU / Limine)

### 2.1 Processador e Arquitetura
- **Arquitetura Base:** x86_64 (AMD64 / Intel 64) no modo Long Mode de 64 bits.
- **Modelo Apresentado (QEMU):** `qemu64` (ou `host` quando ativado KVM/WHPX).
- **Instruções Exigidas pelo Kernel:**
  - `CPUID`: Verificação de suporte SSE e topologia de núcleos.
  - `SSE` / `SSE2`: Habilitado no kernel via registradores de controle `CR0` (limpeza de EM, ajuste de MP) e `CR4` (ativação de OSFXSR e OSXMMEXCPT em `src/cpu.rs:enable_sse`).
  - `MSR` (`IA32_EFER`, `IA32_APIC_BASE`, `IA32_FS_BASE`): Manipulação de registradores de modelo específico.
  - `Long Mode Paging`: Paginação de 4 níveis (PML4 -> PDPT -> PD -> PT) com páginas canônicas de 4 KiB.
- **Capacidade SMP:** Suporte a até 8 vCPUs (`MAX_CPUS = 8` em `src/gdt.rs`), com inicialização multi-core gerenciada em `src/smp.rs`.

### 2.2 Bootloader e Firmware
- **Firmware Suportado:**
  - **BIOS Tradicional:** MBR inicializado via código binário Limine (`limine-bios-cd.bin` e `limine-bios.sys`).
  - **UEFI Moderno:** Suporte via `BOOTX64.EFI` e `limine-uefi-cd.bin`.
- **Protocolo de Inicialização:** Limine Boot Protocol v2 (memmap request, framebuffer request, SMP request).

### 2.3 Chipsets Virtuais Auditados
1. **Intel 440FX + PIIX3 (Intel 82371SB):**
   - Máquina padrão do QEMU (`qemu-system-x86_64 -M pc`).
   - Fornece ponte PCI Host (0x8086:0x1237), controladora IDE clássica (0x8086:0x7010) e controladora ISA/DMA legada.
2. **Intel ICH9 + Q35:**
   - Máquina moderna do QEMU (`qemu-system-x86_64 -M q35`).
   - Fornece barramento PCI Express root complex, controladora SATA AHCI (0x8086:0x2922) e arquitetura IO-APIC moderna.

---

## 3. Matriz de Dispositivos e Classificação de Drivers

A tabela a seguir consolida os 13 dispositivos e controladores avaliados no Jinn 0.1.0:

| ID | Dispositivo / Controlador | Identificação PCI | Categoria | Driver no Kernel | Classificação de Maturidade | Observações Operacionais |
|:---|:---|:---|:---|:---|:---|:---|
| 0 | Intel 440FX PCI Host | `8086:1237` | Bus | `src/drivers/pci.rs` | **Implementado Operacional** | Configuração PCI Tipo 1 (portas 0xCF8/0xCFC). Varredura completa de barramento. |
| 1 | PIIX3 IDE / ATA Controller | `8086:7010` | Storage | `src/drivers/ide.rs` | **Implementado Operacional** | Portas I/O 0x1F0-0x1F7 (Primary), 0x170-0x177 (Secondary). Modo PIO 28/48-bit. |
| 2 | Intel ICH9 AHCI SATA | `8086:2922` | Storage | `src/drivers/ahci.rs` | **Parcial / Experimental** | ABAR BAR5, detecção de portas SATA. Fallback PIO/DMA experimental. |
| 3 | NVMe Controller | `1B36:0010` | Storage | `src/drivers/nvme.rs` | **Parcial / Experimental** | Filas Admin SQ/CQ e Doorbell inicializados. Leitura I/O sob testes de estabilidade. |
| 4 | VirtIO-Block Device | `1AF4:1001` | Storage | `src/drivers/virtio_blk.rs` | **Implementado Operacional** | Descritores VRing, operações assíncronas em anel e transferência por DMA virtual. |
| 5 | AMD PCnet Am79C970A | `1022:2000` | Network | *Nenhum* | **Descoberto Não Suportado** | Enumerado no barramento PCI; sem driver operacional. Pacotes de rede não trafegam. |
| 6 | Intel PRO/1000 (82540EM) | `8086:100E` | Network | `src/drivers/net/e1000.rs` | **Implementado Operacional** | MMIO BAR0, anéis circulares RX/TX de descritores DMA. Totalmente integrado à stack de rede. |
| 7 | VirtIO-Net PCI | `1AF4:1000` | Network | `src/drivers/net/virtio.rs` | **Implementado Operacional** | VirtQueue RX/TX buffers, cabeçalhos de pacote, comutação de buffers no anel. |
| 8 | USB Controller (UHCI/EHCI/xHCI) | `8086:7020` | Bus/Input | *Nenhum* | **Ausente Não Suportado** | Pilha USB não implementada no Jinn. Depende de emulação legada PS/2. |
| 9 | Teclado PS/2 (Intel 8042) | N/A (Portas I/O) | Input | `src/interrupts/ps2_keyboard.rs` | **Implementado Operacional** | Portas 0x60/0x64. Tratamento de scancodes set 1 e 2, buffer circular FIFO. |
| 10 | Mouse PS/2 | N/A (IRQ 12) | Input | `src/interrupts/ps2_mouse.rs` | **Implementado Operacional** | Interrupção IRQ 12 (vetor 44), decodificação de pacotes de 3 bytes (delta X, delta Y, botões). |
| 11 | Limine GOP Framebuffer / BGA | `1234:1111` | Display | `src/vga.rs`, `src/gui.rs` | **Implementado Operacional** | Resolução linear 1024x768x32bpp BGRA. Portas BGA 0x1CE/0x1CF compatíveis com Bochs/QEMU. |
| 12 | APIC Timer / PIT 8254 | N/A | System | `src/interrupts/apic.rs`, `src/timer.rs` | **Implementado Operacional** | Temporizador Local APIC vetor 32 calibrado a 100 Hz; fallback para PIT 8254 porta 0x40/0x43. |

---

## 4. Análise de Discrepâncias e Limitações Identificadas

1. **PCnet (Am79C970A):**
   - *Comportamento Observado:* Ao executar QEMU com `-device pcnet` ou `-net nic,model=pcnet`, o PCI Manager (`src/drivers/pci.rs`) identifica com sucesso o Vendor `0x1022` e Device `0x2000`.
   - *Limitação Crítica:* A pilha de rede do Jinn (`src/net/mod.rs`) suporta exclusivamente `NicType::Virtio` e `NicType::E1000`. Não há driver para PCnet. Caso a máquina virtual possua apenas PCnet, a rede opera exclusivamente em modo **Loopback Virtual (127.0.0.1)**.
2. **Pilha USB (UHCI/OHCI/EHCI/xHCI):**
   - *Comportamento Observado:* A controladora UHCI do PIIX3 (`0x8086:0x7020`) é detectada no barramento PCI.
   - *Limitação Crítica:* O kernel não implementa enumeração de barramento USB nem drivers de classe HID. O subsistema de entrada (`src/interrupts/ps2_keyboard.rs` e `ps2_mouse.rs`) funciona exclusivamente através da controladora legada 8042 (emulada pela BIOS/firmware do QEMU).
3. **Controladoras SATA AHCI e NVMe:**
   - *Comportamento Observado:* Ambas as estruturas de controle foram criadas (`src/drivers/ahci.rs`, `src/drivers/nvme.rs`) e são cadastradas como nós no Device Manager (`/dev/ahci0`, `/dev/nvme0`).
   - *Limitação Crítica:* Sob alta concorrência de leitura/escrita, a sincronização de filas de submissão do NVMe e os buffers PRDT do AHCI ainda operam de forma experimental. Para cenários de produção e persistência crítica, o Jinn utiliza prioritariamente IDE (ATA PIO) ou VirtIO-Block.

---

## 5. Conclusão do Inventário

O ecossistema Jinn possui uma base sólida e comprovada para dispositivos de armazenamento legados (IDE) e virtualizados de alto desempenho (VirtIO-Blk), adaptadores de rede padrão de datacenter (Intel PRO/1000 e VirtIO-Net), entrada clássica de baixa latência (PS/2) e saída de vídeo com aceleração de framebuffer linear (Limine GOP). Dispositivos periféricos de rede legada (PCnet) e barramento USB permanecem formalmente sem suporte de driver, exigindo emulação compatível na camada de hypervisor.

