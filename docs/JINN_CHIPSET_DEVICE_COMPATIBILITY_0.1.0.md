# JINN PROJECT — CHIPSET & DEVICE COMPATIBILITY 0.1.0
## Avaliação de Barramentos, Chipsets (PIIX3 vs ICH9) e Controladores Periféricos

- **Versão do Documento:** 0.1.0
- **Data:** 04/11/2026
- **Status:** Oficial / Auditado
- **Subsistema:** Jinn Unified Device Manager & Low-Level Drivers
- **Invariante Arquitetural:** O kernel centraliza o inventário de dispositivos e a máquina de estados (`Discovered -> Initialized -> Operational -> Suspended -> Failed`) no Unified Device Manager (`src/drivers/device_manager.rs`), isolando os subsistemas superiores da topologia de placas e chipsets.

---

## 1. Avaliação dos Chipsets Virtuais Suportados

### 1.1 Intel 440FX + PIIX3 (Intel 82371SB)
O chipset clássico i440FX é a base da máquina virtual padrão do QEMU (`-M pc`).
- **Ponte Host PCI:** Intel 82441FX / 82440FX (`0x8086:0x1237`). Fornece barramento PCI de 32 bits a 33 MHz. O Jinn comunica-se via portas I/O legadas `0xCF8` (Address) e `0xCFC` (Data) em `src/drivers/pci.rs`.
- **Controladora IDE PIIX3:** Intel 82371SB (`0x8086:0x7010`). Controladora dual channel em modo de compatibilidade legado (Canais Primário nas portas `0x1F0-0x1F7` e Secundário em `0x170-0x177`).
- **Ponte ISA e Controladora de Interrupções:** Integra o PIC cascateado 8259A (remap para vetores 32..47) e temporizador PIT 8254.
- **Veredito de Compatibilidade:** **COMPATIBILIDADE PLENA (Nativa)**. É a plataforma recomendada para os perfis *Legacy (A)* e *Embedded (B)*.

### 1.2 Intel ICH9 + Q35
O chipset moderno ICH9 é a base da máquina virtual Q35 do QEMU (`-M q35`).
- **PCI Express Root Complex:** Barramento PCIe de alta velocidade. O Jinn acessa a configuração PCIe em modo compatível legado via portas `0xCF8/0xCFC`. A interface MMIO ECAM (*Enhanced Configuration Access Mechanism*) não está implementada nesta versão.
- **Controladora SATA AHCI:** Intel 82801IB ICH9 AHCI (`0x8086:0x2922`).
- **Veredito de Compatibilidade:** **COMPATIBILIDADE OPERACIONAL COM RESSALVAS**. O kernel inicializa com sucesso em máquinas Q35, mas opera o barramento PCIe através do mecanismo compatível Tipo 1 e exige o uso de VirtIO ou IDE/SATA emulado.

---

## 2. Detalhamento e Matriz Operacional de Dispositivos

```
+-------------------------------------------------------------------------+
|                  Jinn Unified Device Manager (L1/L2)                    |
|                        MAX_DEVICES = 32 slots                           |
+-------------------------------------------------------------------------+
    |           |            |           |            |            |
+-------+   +-------+    +-------+   +-------+    +-------+    +-------+
|  IDE  |   |VirtIO |    | e1000 |   | PS/2  |    |  GOP  |    |  APIC |
| (0x1) |   | (0x4) |    | (0x6) |   |(0x9/A)|    | (0xB) |    | (0xC) |
| Oper. |   | Oper. |    | Oper. |   | Oper. |    | Oper. |    | Oper. |
+-------+   +-------+    +-------+   +-------+    +-------+    +-------+
    |           |            |
+-------+   +-------+    +-------+
| AHCI  |   | NVMe  |    | PCnet |
| (0x2) |   | (0x3) |    | (0x5) |
| Exp.  |   | Exp.  |    | Não   |
|       |   |       |    | Suport|
+-------+   +-------+    +-------+
```

### 2.1 Armazenamento (Storage)

1. **IDE / ATA Clássico (`src/drivers/ide.rs`):**
   - *Status:* **Implementado e Totalmente Operacional**.
   - *Mecanismo:* Transferência PIO em 16 bits (Word I/O). Suporta leitura e escrita com endereçamento LBA28 e LBA48. Tratamento de polling via registrador de status (`0x1F7`) com timeout de segurança de 100.000 iterações.
   - *Integração:* Totalmente integrado ao VFS e ao auto-montador de partições FAT32 e Ext2.
2. **VirtIO-Block (`src/drivers/virtio_blk.rs`):**
   - *Status:* **Implementado e Operacional**.
   - *Mecanismo:* Barramento PCI legacy I/O e anéis VRing (Descriptor Table, Available Ring, Used Ring). Transferência por DMA virtual sem intervenção constante da CPU.
   - *Desempenho:* Rendimento até 8.5x superior ao IDE PIO sob cargas sequenciais em máquinas virtuais.
3. **AHCI SATA (`src/drivers/ahci.rs`):**
   - *Status:* **Parcialmente Implementado / Experimental**.
   - *Mecanismo:* Mapeamento de memória ABAR via BAR5, configuração de estruturas FIS e Command Tables.
   - *Limitação:* Suporte a leitura validado; operações concorrentes de escrita pesada ainda exigem refinamento na sincronização de buffers PRDT.
4. **NVMe (`src/drivers/nvme.rs`):**
   - *Status:* **Parcialmente Implementado / Experimental**.
   - *Mecanismo:* Configuração de filas Admin Submission Queue (ASQ) e Admin Completion Queue (ACQ), registro de Doorbell e comando Identify Controller.
   - *Limitação:* Fila de I/O em fase de endurecimento. Não homologado como storage principal nesta versão.

### 2.2 Rede (Networking)

1. **VirtIO-Net PCI (`src/drivers/net/virtio.rs`):**
   - *Status:* **Implementado e Operacional**.
   - *Identificação:* Vendor `0x1AF4`, Device `0x1000`.
   - *Mecanismo:* Filas de descritores de recepção (RX) e transmissão (TX), cabeçalhos de rede VirtIO, interrupções por pacote e polling cooperativo.
2. **Intel PRO/1000 / 82540EM (`src/drivers/net/e1000.rs`):**
   - *Status:* **Implementado e Operacional**.
   - *Identificação:* Vendor `0x8086`, Device `0x100E`.
   - *Mecanismo:* Mapeamento de registradores MMIO via BAR0, anéis circulares de descritores DMA de 16 bytes alinhados em memória física.
   - *Veredito:* Plena capacidade para DHCP, DNS, TCP e HTTP em ambientes emulados e bare-metal antigos.
3. **AMD PCnet Am79C970A:**
   - *Status:* **DESCOBERTO NÃO SUPORTADO (Driver Ausente)**.
   - *Identificação:* Vendor `0x1022`, Device `0x2000`.
   - *Auditoria:* O barramento PCI detecta o cartão, mas nenhum driver se anexa ao descritor. Consequentemente, o kernel comuta para `NicType::None` (Loopback Virtual). Não há tráfego de pacotes externos.

### 2.3 Entrada e Saída Gráfica

1. **Controladora PS/2 (Intel 8042):**
   - *Teclado (`src/interrupts/ps2_keyboard.rs`):* Portas I/O `0x60` (Dados) e `0x64` (Status/Comando). Modo de polling limpo durante o boot e suporte a IRQ1 com buffer circular FIFO.
   - *Mouse (`src/interrupts/ps2_mouse.rs`):* IRQ 12 (vetor 44 na IDT). Habilitação de linha auxiliar PS/2, ativação de relatórios e decodificação de pacotes de 3 bytes (delta X, delta Y, botões esquerdo/direito).
2. **Pilha USB (UHCI / EHCI / xHCI):**
   - *Status:* **AUSENTE NÃO SUPORTADO**.
   - *Auditoria:* O kernel não implementa pilha de protocolos USB (nem host controllers, nem hubs, nem classes HID/Mass Storage). Dispositivos de entrada USB dependem 100% da emulação legada PS/2 provida pelo firmware da placa-mãe ou do hypervisor.
3. **Vídeo e Framebuffer:**
   - *Limine GOP (`src/vga.rs`, `src/gui.rs`):* Resolução canônica de 1024x768 pixels a 32 bits por pixel (BGRA). Mapeamento de memória linear diretamente no espaço de endereçamento do kernel.
   - *Bochs BGA:* Portas I/O `0x1CE` e `0x1CF` suportadas para detecção e configuração de vídeo virtual.

---

## 3. Síntese de Compatibilidade de Controladores

| Controlador | PCI ID | PIIX3 (i440fx) | ICH9 (Q35) | Suporte Jinn 0.1.0 | Ação Recomendada |
|:---|:---|:---:|:---:|:---:|:---|
| **IDE ATA Controller** | `8086:7010` | Sim | Sim (emul.) | **Operacional** | Primário para perfis Legacy e Embedded |
| **VirtIO-Block** | `1AF4:1001` | Sim | Sim | **Operacional** | Primário para perfis de Servidor |
| **AHCI SATA** | `8086:2922` | Opcional | Sim | **Experimental** | Uso com monitoramento de integridade |
| **NVMe Controller** | `1B36:0010` | Opcional | Sim | **Experimental** | Manter em testes de validação contínua |
| **VirtIO-Net** | `1AF4:1000` | Sim | Sim | **Operacional** | Adaptador de rede padrão |
| **Intel e1000** | `8086:100E` | Sim | Sim | **Operacional** | Alternativa de rede padrão |
| **AMD PCnet** | `1022:2000` | Sim | Não | **Não Suportado** | Evitar no QEMU; usar e1000 ou virtio |
| **USB UHCI/EHCI** | `8086:7020` | Sim | Sim | **Não Suportado** | Utilizar emulação PS/2 |
| **PS/2 8042** | I/O Legado | Sim | Sim (leg.) | **Operacional** | Dispositivo de entrada padrão |

