# JINN PROJECT — HARDWARE COMPATIBILITY MATRIX 0.1.0
## Matriz Canônica de Compatibilidade de Hardware do Ecossistema Jinn

- **Versão do Documento:** 0.1.0
- **Data:** 04/11/2026
- **Status:** Oficial / Auditado
- **Subsistema:** Jinn Ecosystem Hardware Abstraction & Profile Matrix
- **Invariante Arquitetural:** Jinn permanece como o único microkernel compartilhado. Aladdin, Simbad e Ifrit operam sob a mesma ABI e tabelas de despacho, variando apenas em perfis de carga e serviços em Ring 3.

---

## 1. Legenda e Convenções da Matriz

| Símbolo | Classificação | Definição Operacional |
|:---:|:---|:---|
| **`[S]`** | **Suportado / Operacional** | Driver implementado, testado e validado em produção com tratamento de erros. |
| **`[P]`** | **Parcial / Experimental** | Driver funcional para operações básicas de leitura/sonda; requer cuidados em escrita pesada. |
| **`[R]`** | **Restrito / Degradado** | Funcional, porém opera sob restrição severa de memória ou desempenho. |
| **`[N]`** | **Não Suportado** | Driver ausente no kernel; dispositivo não operacional mesmo se enumerado pelo barramento PCI. |

---

## 2. Matriz Canônica de Compatibilidade

| Categoria | Componente de Hardware / Configuração | Jinn Kernel | Aladdin (Desktop) | Simbad (Servidor) | Ifrit (Industrial) | Observações Arquiteturais |
|:---|:---|:---:|:---:|:---:|:---:|:---|
| **CPU** | x86_64 Long Mode (Instruções Base) | **`[S]`** | **`[S]`** | **`[S]`** | **`[S]`** | Exigência mandatória para todas as manifestações |
| | Extensões SSE / SSE2 | **`[S]`** | **`[S]`** | **`[S]`** | **`[S]`** | Habilitadas via registradores CR0/CR4 |
| | Uniprocessador (1 vCPU) | **`[S]`** | **`[S]`** | **`[S]`** | **`[S]`** | Escalonamento preemptivo via timer APIC/PIT |
| | Multiprocessamento Simétrico (2–8 vCPUs) | **`[S]`** | **`[S]`** | **`[S]`** | **`[S]`** | Ativação via Limine SMP Request |
| **Memória** | 64 MB RAM (16.384 frames) | **`[S]`** | **`[R]`** | **`[S]`** | **`[S]`** | Aladdin sofre com margem de apenas ~7 MB livres |
| | 128 MB RAM (32.768 frames) | **`[S]`** | **`[S]`** | **`[S]`** | **`[S]`** | Patamar mínimo padrão recomendado para Desktop |
| | 256 MB RAM (65.536 frames) | **`[S]`** | **`[S]`** | **`[S]`** | **`[S]`** | Desempenho ideal com cache LRU e multitarefa |
| | 512 MB+ RAM (131.072+ frames) | **`[S]`** | **`[S]`** | **`[S]`** | **`[S]`** | Excelente folga para servidores de alta densidade |
| **Firmware**| BIOS Tradicional (Limine MBR) | **`[S]`** | **`[S]`** | **`[S]`** | **`[S]`** | Carregado via `limine-bios-cd.bin` |
| | UEFI x86_64 (Limine EFI) | **`[S]`** | **`[S]`** | **`[S]`** | **`[S]`** | Carregado via `BOOTX64.EFI` |
| **Chipset** | Intel 440FX + PIIX3 (`-M pc`) | **`[S]`** | **`[S]`** | **`[S]`** | **`[S]`** | Plataforma recomendada para Legacy e Embedded |
| | Intel ICH9 + Q35 (`-M q35`) | **`[S]`** | **`[S]`** | **`[S]`** | **`[S]`** | Acesso PCIe via mecanismo compatível I/O 0xCF8 |
| **Storage** | IDE / ATA (Primary/Secondary PIO) | **`[S]`** | **`[S]`** | **`[S]`** | **`[S]`** | Primário para inicialização MBR e FAT32/Ext2 |
| | VirtIO-Block PCI | **`[S]`** | **`[S]`** | **`[S]`** | **`[S]`** | Transferência por DMA virtual de alta velocidade |
| | AHCI SATA (Intel ICH9) | **`[P]`** | **`[P]`** | **`[P]`** | **`[P]`** | Inicialização e leitura OK; escrita em testes |
| | NVMe (QEMU / PCIe) | **`[P]`** | **`[P]`** | **`[P]`** | **`[P]`** | Filas Admin SQ/CQ ativas; I/O experimental |
| **Rede** | VirtIO-Net PCI (`1AF4:1000`) | **`[S]`** | **`[S]`** | **`[S]`** | **`[S]`** | Driver primário para virtualização moderna |
| | Intel PRO/1000 82540EM (`8086:100E`)| **`[S]`** | **`[S]`** | **`[S]`** | **`[S]`** | Totalmente funcional com DHCP, DNS e TCP |
| | AMD PCnet Am79C970A (`1022:2000`) | **`[N]`** | **`[N]`** | **`[N]`** | **`[N]`** | Sem driver; rede comuta para Loopback local |
| | Loopback Virtual (`127.0.0.1`) | **`[S]`** | **`[S]`** | **`[S]`** | **`[S]`** | Sempre operacional para IPC de rede interno |
| **Entrada** | Teclado PS/2 (Intel 8042 Port 0x60) | **`[S]`** | **`[S]`** | **`[S]`** | **`[S]`** | Modo FIFO e polling de inicialização |
| | Mouse PS/2 (IRQ 12) | **`[S]`** | **`[S]`** | N/A | **`[S]`** | Usado no cursor gráfico do Aladdin |
| | USB UHCI / EHCI / xHCI | **`[N]`** | **`[N]`** | **`[N]`** | **`[N]`** | Sem pilha USB nativa; requer emulação PS/2 |
| **Vídeo** | Limine GOP Framebuffer (1024x768x32)| **`[S]`** | **`[S]`** | N/A | **`[S]`** | Buffer BGRA linear de alto desempenho |
| | Bochs BGA (Portas 0x1CE/0x1CF) | **`[S]`** | **`[S]`** | N/A | **`[S]`** | Detecção e compatibilidade de vídeo virtual |
| | Serial Console (`COM1` 0x3F8) | **`[S]`** | **`[S]`** | **`[S]`** | **`[S]`** | Console primário para perfil de servidor Simbad |
| **Timers** | Local APIC Timer (Vetor 32, 100 Hz)| **`[S]`** | **`[S]`** | **`[S]`** | **`[S]`** | Preempção do escalonador e quanta de tempo |
| | PIT 8254 (Porta 0x40/0x43) | **`[S]`** | **`[S]`** | **`[S]`** | **`[S]`** | Temporizador de fallback calibrado |

---

## 3. Diretrizes de Implantação e Boas Práticas

1. **Servidores em Produção (Simbad):**
   - *Configuração Recomendada:* 512 MB RAM, 4 vCPUs, Chipset ICH9/Q35, Armazenamento VirtIO-Block, Rede VirtIO-Net, Console Serial stdio.
2. **Estações de Trabalho Desktop (Aladdin):**
   - *Configuração Recomendada:* 256 MB a 512 MB RAM, 2 vCPUs, Chipset i440FX ou Q35, Armazenamento IDE ou VirtIO, Adaptador de Rede Intel e1000, Vídeo Limine GOP 1024x768x32bpp, Entrada PS/2.
3. **Controladores e Nodos Embarcados (Ifrit):**
   - *Configuração Recomendada:* 64 MB a 128 MB RAM, 1 vCPU, Chipset i440FX, Armazenamento IDE/Flash, Anéis de dados JDA dedicados.
4. **Configurações a Evitar:**
   - Evitar configurar adaptadores de rede `pcnet` no QEMU, uma vez que a ausência de driver força a placa a operar exclusivamente em Loopback interno.
   - Evitar operar o Aladdin Desktop com menos de 128 MB de RAM para prevenir saturação do buffer de composição gráfica.

