# 📚 Jinn OS & Aladdin Desktop — Catálogo Tecnológico Completo

> **Documento Oficial de Engenharia — Etapa A (Consolidação Tecnológica)**  
> **Data de Auditoria:** 16 de Novembro de 2026 | **Branch:** `main` | **Commit Base:** `447780e`  
> **Arquitetura Alvo:** x86_64 Bare-Metal (`#![no_std]`) | **Bootloader:** Limine Boot Protocol  

---

## 1. Identificação do Ambiente e Preservação

| Parâmetro | Valor Auditado | Observação / Evidência |
|---|---|---|
| **Repositório Kernel** | `D:\Projetos\Jinn` (`git@github.com:GeovaneEstanislau/Jinn_Project.git`) | Branch `main`, HEAD `447780e`, limpo e sincronizado |
| **Repositório Desktop** | `D:\Projetos\Aladdin` (`git@github.com:GeovaneEstanislau/Aladdin.git`) | Branch `main`, HEAD `a918199`, limpo e sincronizado |
| **Compilador Rust** | `rustc 1.102.0-nightly (3ef0c22cd 2026-11-11)` | Toolchain `nightly-x86_64-pc-windows-msvc` |
| **Gerenciador Cargo** | `cargo 1.102.0-nightly (c07ac2c4d 2026-11-06)` | Targets: `x86_64-unknown-none` |
| **Emulador de Referência** | `tools/qemu/qemu-system-x86_64.exe` (QEMU 11.1.0) | Binário homologado integrado ao repositório |
| **Script de Compilação** | `scripts/build.ps1` | Gera ELF do kernel e 5 binários de usuário Ring 3 |
| **Script de Execução** | `scripts/run.ps1` | Monta ISO híbrida (`jinn.iso`), disco (`disk.img`) e dispara QEMU |
| **Script de Particionamento** | `scripts/make_disk.ps1` | Gera MBR dual-partition (FAT32 LBA 2048 + Ext2 LBA 65536) |

---

## 2. Inventário do Microkernel (Ring 0)

### 2.1. Bootloader & Inicialização do Sistema
* **Módulos:** `src/limine.rs`, `src/main.rs`
* **Implementação:** Protocolo Limine versão 64-bit com requisições estáticas em seções dedicadas (`.limine_reqs`).
* **Recursos Negociados no Boot:**
  * Base de memória e mapa físico (`LimineMemmapRequest`).
  * Framebuffer linear de vídeo GOP (`LimineFramebufferRequest`).
  * Relógio de tempo real (`LimineBootTimeRequest`).
  * Módulos carregados em memória (`LimineModuleRequest`).
  * Endereço físico de entrada do kernel relocável (`LimineKernelAddressRequest`).
* **Ponto de Entrada:** Função canônica `pub extern "C" fn _start() -> !` em `src/main.rs`. Inicializa VGA, GDT, IDT, PIC, APIC, paginação de memória, KDM, drivers de armazenamento, VFS, pilha de rede e scheduler.

### 2.2. GDT, TSS e Segmentação x86_64
* **Módulos:** `src/gdt.rs`
* **Implementação:** Global Descriptor Table com 7 entradas de 64 bits:
  1. Null Descriptor (`0x00`).
  2. Kernel Code Segment 64-bit (`0x08`, Ring 0, `DPL=0`, Exec/Read).
  3. Kernel Data Segment 64-bit (`0x10`, Ring 0, `DPL=0`, Read/Write).
  4. User Data Segment 64-bit (`0x18 | 3 = 0x1B`, Ring 3, `DPL=3`, Read/Write).
  5. User Code Segment 64-bit (`0x20 | 3 = 0x23`, Ring 3, `DPL=3`, Exec/Read).
  6. Task State Segment (TSS, descritor estendido de 16 bytes, offset `0x28`).
* **Isolamento de Pilha:** TSS configurado com `RSP0` dinamicamente atualizado no context switch, fornecendo pilha segura no kernel durante transições Ring 3 $\to$ Ring 0 causadas por interrupções ou syscalls.

### 2.3. IDT, Tratamento de Interrupções e Exceções
* **Módulos:** `src/interrupts/idt.rs`, `src/interrupts/stubs.s`, `src/interrupts/mod.rs`
* **Implementação:** Interrupt Descriptor Table completa com 256 vetores.
  * Vetores `0..=31`: Exceções de arquitetura da CPU (`#DE`, `#DB`, `#NMI`, `#BP`, `#OF`, `#BR`, `#UD`, `#NM`, `#DF`, `#TS`, `#NP`, `#SS`, `#GP`, `#PF`, `#MF`, `#AC`, `#MC`, `#XM`, `#VE`).
  * Vetores `32..=47`: IRQs de hardware mapeadas pelo PIC/APIC (Timer, Teclado PS/2, Cascata, COM1/2, LPT, Floppy, RTC, Mouse PS/2, FPU, IDE primário/secundário).
  * Vetores `48..=254`: Vetores de software, drivers PCI e IPI (Inter-Processor Interrupts).
  * Vetor `0x80` (`128`): Syscall legada por software interrupt.
* **Salvamento de Contexto:** `src/interrupts/stubs.s` salva registradores de uso geral (`RAX`, `RBX`, `RCX`, `RDX`, `RSI`, `RDI`, `RBP`, `R8..R15`) no topo da pilha, compondo a estrutura `InterruptFrame`.

### 2.4. Controladores de Interrupção (PIC, IOAPIC e LAPIC)
* **Módulos:** `src/interrupts/pic.rs`, `src/interrupts/apic.rs`
* **Implementação:**
  * **Dual 8259 PIC:** Suporte a inicialização e mascaramento completo para hardware legado.
  * **Local APIC (LAPIC):** Descoberta via MSR `IA32_APIC_BASE` (`0x1B`), mapeamento de registradores de fim de interrupção (EOI), Spurious Interrupt Vector Register (SVR) e Timer Local com divisor calibrado.
  * **IOAPIC:** Roteamento de linhas de interrupção PCI e periféricos de barramento.

### 2.5. SMP (Symmetric Multiprocessing) e APs
* **Módulos:** `src/smp.rs`
* **Implementação:**
  * Descoberta de CPUs via ACPI / MADT e CPUID Leaf 1 (`initial_apic_id`).
  * Sequência canônica de inicialização de Application Processors (APs): INIT IPI $\to$ Delay 10ms $\to$ SIPI (Startup IPI) $\to$ Delay 200µs $\to$ SIPI.
  * Trampoline em modo real de 16 bits para transição direta ao modo longo de 64 bits.
  * Barreira de sincronização atômica (`AP_RUNNING_COUNT`).
  * Identificação de núcleo ativo via `current_cpu_id()`.

### 2.6. Gerenciamento de Memória Física (Buddy Allocator)
* **Módulos:** `src/memory/buddy.rs`, `src/memory/mod.rs`
* **Implementação:** Alocador de blocos físicos por potências de 2 (Ordens 0 a 10, representando blocos de 4 KiB a 4 MiB).
* **Mapeamento:** Baseado nas regiões utilizáveis informadas pelo Limine (`LimineMemoryMapEntryType::Usable`).
* **Garantias:** Alocação de frames com complexidade $O(1)$ a $O(\log N)$ e fusão imediata de blocos irmãos (*buddies*) na liberação para mitigar fragmentação externa.

### 2.7. Gerenciamento de Memória Virtual e Paginação
* **Módulos:** `src/memory/paging.rs`
* **Implementação:** Paginação x86_64 de 4 níveis (PML4 $\to$ PDPT $\to$ PD $\to$ PT).
* **Recursos Suportados:**
  * Mapeamento de páginas padrão de 4 KiB e páginas grandes (*HugePages*) de 2 MiB.
  * Flags de proteção: `PRESENT`, `WRITABLE`, `USER_ACCESSIBLE`, `WRITE_THROUGH`, `NO_CACHE`, `ACCESSED`, `DIRTY`, `NO_EXECUTE` (NX bit).
  * Política $W \oplus X$ (Write XOR Execute) ativa em código de kernel e espaço de usuário.
  * Invalidação de TLB direcionada via `invlpg` e recarga de CR3.

### 2.8. Heap e Alocadores Dinâmicos do Kernel
* **Módulos:** `src/memory/heap.rs`, `src/memory/alloc.rs`
* **Implementação:** Alocador global `#[global_allocator]` implementando a trait `core::alloc::GlobalAlloc`.
* **Capacidade:** Heap estático pré-reservado no espaço de endereçamento do kernel com expansão sob demanda via frames do Buddy Allocator.
* **Segurança:** Detecção de double-free, corrupção de cabeçalhos de nós e verificação de alinhamento em blocos alocados.

### 2.9. Escalonador (Scheduler), Processos e Threads
* **Módulos:** `src/scheduler.rs`
* **Implementação:** Escalonador preemptivo e cooperativo de tarefas com fila protegida por spinlock.
* **Estados de Processos:** `Ready`, `Running`, `Blocked`, `Sleeping`, `Terminated`.
* **Estrutura de Tarefa (`Task`):** Armazena PID, PPID, nome, prioridade, estado, CR3 (tabela de páginas exclusiva por processo), registradores de CPU, descritor de arquivos abertos (FDT), cotas de recursos e canais de IPC.
* **Preempção:** Disparada pelo Timer do APIC / PIT ou por chamadas explícitas de cooperação (`yield_now()`).
* **Ciclo de Vida:** Suporte a clonagem de processos (`sys_fork`), substituição de imagem executável (`sys_exec`), encerramento ordenado (`sys_exit`) e sincronização de término (`sys_waitpid`).

### 2.10. Comunicação Interprocesso (IPC)
* **Módulos:** `src/ipc.rs`
* **Implementação:** Canais de mensagens síncronas e assíncronas entre processos Ring 3 e serviços do sistema.
* **Mecanismos:**
  * Filas de mensagens por PID com semântica de entrega atômica.
  * Memória compartilhada com permissões mapeadas sob demanda.
  * Notificações de eventos orientadas a descritores de arquivos / janelas.

### 2.11. Sistema de Arquivos Virtual (VFS), FAT32 e Ext2
* **Módulos:** `src/vfs/mod.rs`, `src/vfs/cache.rs`, `src/vfs/fat32.rs`, `src/vfs/ext2.rs`, `src/vfs/volume.rs`, `src/vfs/ramfs.rs`
* **Implementação:**
  * **VFS Core:** Hierarquia unificada baseada em Inodes e Dentries. Suporte a flags POSIX: `O_RDONLY`, `O_WRONLY`, `O_RDWR`, `O_CREAT`, `O_APPEND`, `O_TRUNC`.
  * **BlockCache LRU:** Cache de blocos de disco de 512 bytes / 4096 bytes com write-back síncrono e controle de *dirty blocks*.
  * **FAT32 Driver:** Leitura e escrita completas em partições FAT32, navegação em diretórios com cluster chains, alocação de novos clusters, particionamento MBR.
  * **Ext2 Driver:** Leitura e navegação completa de diretórios e arquivos em sistemas de arquivos Ext2 (leitura validada; escrita em fase de projeto/parcial).
  * **RamFS:** Sistema de arquivos temporário volátil em memória para `/tmp` e descritores efêmeros.

### 2.12. Interface de Chamadas de Sistema (Syscalls) e Isolamento Ring 3
* **Módulos:** `src/syscall.rs`, `src/syscall_user.rs`
* **Implementação:** Interface nativa via instrução `SYSCALL`/`SYSRET` e fallback por `INT 0x80`.
* **Segurança e Validação:**
  * Checagem estrita de fronteiras de ponteiros (`validate_user_buffer`, `validate_user_string`) impedindo ponteiros nulos ou apontando para a metade superior da memória (espaço do kernel $\ge$ `0xFFFF_8000_0000_0000`).
  * Mais de 60 chamadas de sistema cobrindo:
    * E/S de arquivos (`sys_open`, `sys_read`, `sys_write`, `sys_close`, `sys_stat`, `sys_readdir`).
    * Gerenciamento de tarefas (`sys_spawn`, `sys_exit`, `sys_getpid`, `sys_yield`, `sys_sleep`).
    * Memória (`sys_mmap`, `sys_munmap`, `sys_brk`).
    * Interface gráfica (`sys_window_create`, `sys_window_draw`, `sys_window_poll_event`, `sys_window_close`).
    * Rede e Sockets (`sys_socket`, `sys_bind`, `sys_connect`, `sys_send`, `sys_recv`).
    * Telemetria e Firewall (`sys_res_stat`, `sys_firewall_get`, `sys_firewall_set`).

### 2.13. Carregador de Binários ELF64
* **Módulos:** `src/elf.rs`, `src/elf_tests.rs`
* **Implementação:** Parser e loader de binários ELF64 estáticos para espaço de usuário.
* **Validações:** Magic bytes (`0x7F 'E' 'L' 'F'`), arquitetura `EM_X86_64`, formato Little-Endian, tipo `ET_EXEC` / `ET_DYN`.
* **Mapeamento:** Itera sobre segmentos `PT_LOAD`, aloca frames físicos, mapeia no espaço virtual do processo e aplica permissões de execução e escrita estritas ($W \oplus X$).

### 2.14. Kernel DMA Manager (KDM)
* **Módulos:** `src/dma/manager.rs`, `src/dma/pool.rs`, `src/dma/buffer.rs`, `src/dma/bounce.rs`, `src/dma/token.rs`, `src/dma/state.rs`, `src/dma/types.rs`, `src/dma/tests.rs`
* **Implementação:** Gerenciador centralizado de transferências diretas de memória para dispositivos PCI/VirtIO.
* **Recursos:**
  * Alocação de buffers contíguos de memória física com alinhamento de 64 bytes para barramentos PCI de 32 e 64 bits.
  * Mecanismo de **Bounce Buffers** para acomodar dispositivos incapazes de endereçamento de 64 bits ou memória não-contígua.
  * Tokens de posse rastreando ciclo de vida (*Pinning*, *Transfer*, *Unpinning*) prevenindo acessos após liberação.

### 2.15. Controladores de Armazenamento e Barramentos de Hardware
* **Módulos:** `src/drivers/pci.rs`, `src/drivers/block.rs`, `src/drivers/ide.rs`, `src/drivers/ahci.rs`, `src/drivers/nvme.rs`, `src/drivers/virtio_blk.rs`, `src/drivers/device_manager.rs`
* **Implementação:**
  * **Barramento PCI:** Enumeração por portas I/O `0xCF8`/`0xCFC` identificando Vendor ID, Device ID, Class/Subclass, BAR0..5 e linhas de IRQ.
  * **IDE Controller:** Driver clássico PIO para discos ATA legados.
  * **AHCI / SATA:** Driver de controladora SATA AHCI 1.0+ operando via DMA e listas de comandos.
  * **NVMe:** Driver de controladora de estado sólido NVMe sobre PCI Express com filas de submissão e conclusão (Admin/IO SQ e CQ).
  * **VirtIO-Blk:** Driver de armazenamento paravirtualizado acelerado para QEMU/KVM operando sobre anéis Virtqueue.

### 2.16. Controladores de Rede (NICs) e Pilha de Protocolos
* **Módulos:** `src/drivers/net/`, `src/net/`
* **Implementação de Drivers Físicos:**
  * **Intel PRO/1000 (e1000):** Suporte nativo ao chipset 82540EM (padrão QEMU/VirtualBox) com anéis de transmissão e recepção por DMA.
  * **AMD PCnet (Am79C970A / Am79C973):** Driver completo compatível com VirtualBox.
  * **VirtIO-Net:** Driver paravirtualizado para alto rendimento sob KVM/QEMU.
* **Pilha de Protocolos em Camadas:**
  * **L2:** Ethernet II framing, resolução de endereços ARP com cache dinâmico (`src/net/arp.rs`).
  * **L3:** IPv4 com cálculo de checksums, tratamento de fragmentação e ICMP Echo Request/Reply (`src/net/ipv4.rs`, `src/net/icmp.rs`).
  * **L4:** UDP com clientes DHCP (`src/net/dhcp.rs`) e DNS (`src/net/dns.rs`); TCP com máquina de estados RFC 793, retransmissão, controle de fluxo e janelas (`src/net/tcp.rs`, `src/net/socket.rs`).
  * **Firewall (JinnWall):** Filtro de pacotes com inspeção de estado (SPI) e perfis configuráveis (`src/net/jinnwall.rs`).
  * **Criptografia & TLS 1.3:** PKI X.509 com TrustStore e validação RFC 5280 (`src/net/x509.rs`, `src/net/pki.rs`), Key Schedule HKDF RFC 5869 e protocolo TLS 1.3 completo com AEAD ChaCha20-Poly1305 RFC 8439 (`src/net/tls13.rs`).

### 2.17. Criptografia e Gerador de Números Pseudoaleatórios (CSPRNG)
* **Módulos:** `src/crypto/`
* **Implementação:**
  * **CSPRNG:** Entropia coletada de jitter de hardware e RTC alimentando ChaCha20 DRBG com avanço de estado irreversível (*ratcheting forward-secrecy*).
  * **Funções Criptográficas:** SHA-256 nativo, HMAC-SHA256, biblioteca de aritmética bignum de alta precisão, chaves e assinaturas RSA-PSS / PKCS#1 v1.5 e curva elíptica NIST P-256.

### 2.18. Subsistema Gráfico do Kernel e Entrada de Usuário
* **Módulos:** `src/vga.rs`, `src/gui.rs`, `src/interrupts/ps2_keyboard.rs`, `src/interrupts/ps2_mouse.rs`
* **Implementação:**
  * **VGA / Framebuffer:** Desenho primitivo acelerado em memória linear fornecida pelo Limine (formatos 32-bit BGRX/RGBA).
  * **Compositor Ring 0:** Gerenciador de superfícies para clientes de janela gráfica com filas circulares FIFO de 16 eventos e controle de foco.
  * **Controlador PS/2:** Suporte desacoplado para teclado (IRQ 1) com mapa ABNT2/US e mouse PS/2 com pacotes de 3 bytes (IRQ 12).

---

## 3. Inventário do Ambiente Gráfico Aladdin (Ring 3)

O **Aladdin** é o servidor de interface gráfica e ambiente de desktop moderno do Jinn OS, operando inteiramente em espaço de usuário (Ring 3).

### 3.1. Arquitetura de Contratos e Independência de Kernel
* **Módulos:** `D:\Projetos\Aladdin\src\contracts\`
* **Implementação:** O Aladdin comunica-se com o ambiente através de interfaces abstratas (*Traits*), garantindo que possa ser compilado e testado tanto no host quanto no microkernel Jinn:
  * `FramebufferHost`: Acesso à memória linear de vídeo, resolução e taxa de atualização.
  * `InputHost`: Leitura de eventos de teclado, posicionamento de ponteiro e botões do mouse.
  * `ProcessHost`: Consulta de processos ativos e lançamento de novas aplicações (`spawn_app`).
  * `StorageHost`: Consulta de partições, volumes montados e estatísticas de uso de disco.
  * `SystemHost`: Obtenção de timestamps monotônicos, data/hora RTC e métricas de sistema.

### 3.2. Servidor Gráfico, Compositor e Primitivas 2D
* **Módulos:** `D:\Projetos\Aladdin\src\compositor\`, `D:\Projetos\Aladdin\src\wm\`
* **Implementação:**
  * **Renderização por Software Puro:** Motor matemático em Rust puro para rasterização de retângulos, bordas arredondadas, sombras suaves (Drop Shadows), gradientes lineares, texto com renderização por máscara de bits e blend alfa de 32 bits (`src/compositor/primitives.rs`).
  * **Rastreamento de Danos (*Damage Tracking*):** Suporte a `DirtyRect` e `ClipRect` para evitar re-renderização de áreas estáticas do desktop, minimizando ciclos de CPU e tráfego de memória.
  * **Gerenciador de Janelas (*Window Manager*):** Controle de janelas com barras de título, botões de fechar/minimizar, cálculo de z-order, movimentação por arrasto e redimensionamento interativo.

### 3.3. Sessão de Desktop e Elementos de Interface
* **Módulos:** `D:\Projetos\Aladdin\src\desktop\`
* **Implementação:**
  * **Sessão Central (`AladdinSession`):** Orquestra o ciclo de renderização a 60 FPS, roteamento de eventos de entrada e gerencia janelas de clientes externos e internos.
  * **Dock:** Barra inferior translúcida com ícones de inicialização rápida e animação de seleção.
  * **Wingpanel (Painel Superior):** Exibe data/hora do RTC, indicadores de carga de CPU/RAM, status de rede e menu de controle rápido.
  * **Switcher (Alt+Tab):** Interface visual para alternância rápida de foco entre janelas abertas.
  * **Sistema de Notificações:** Exibição de banners flutuantes de alertas do sistema com auto-dismiss.

### 3.4. Aplicações Nativas Integradas
* **Módulos:** `D:\Projetos\Aladdin\src\apps\`
* **Implementação:**
  * **Terminal (`terminal.rs`):** Emulador de terminal gráfico com buffer circular de linhas e shell interativo integrado.
  * **Monitor de Sistema (`monitor.rs`):** Telemetria em tempo real de uso de memória física, heap, carga de CPU e lista de tarefas ativas.
  * **Gerenciador de Arquivos (`files.rs`):** Navegador gráfico de diretórios com suporte a partição `/disk` (FAT32) e `/server` (Ext2), paginação de arquivos e rodapé de capacidade de armazenamento.
  * **Configurações (`settings.rs`):** Painel de controle visual com persistência atômica transacional via arquivo `.BAK` para recuperação contra cortes repentinos de energia.
  * **Gerenciador de Discos (`disks.rs`):** Visualizador de unidades de armazenamento, partições MBR e sistemas de arquivos.

### 3.5. Jinn Browser e Motor Web Nativo
* **Módulos:** `apps/ola-mundo/src/browser.rs`, `browser/crates/`
* **Implementação:**
  * Navegador web completo sem dependências externas de libc.
  * Parser HTML5 com árvore de nós DOM e suporte a tags de conteúdo estruturado.
  * Motor de folhas de estilo CSS com cálculo de cascata, layout de caixas e renderização tipográfica.
  * Motor de armazenamento local transacional persistindo Histórico, Favoritos, Cookies e Cache de recursos em partição FAT32 com integridade protegida por soma de verificação.
  * Conectividade segura nativa HTTPS sobre o módulo TLS 1.3 do microkernel.

---

## 4. Resumo de Métricas do Código-Fonte

```
Componente                 Linguagem              Arquivos    Linhas de Código
-------------------------------------------------------------------------------
Kernel (Ring 0)            Rust no_std, Assembly     86         ~48.500
Drivers de Hardware        Rust no_std               18         ~14.200
Pilha de Rede & TLS 1.3    Rust no_std               15         ~19.800
Aladdin Desktop (Ring 3)   Rust                      33         ~21.600
Jinn Browser & Engine      Rust                      26         ~28.400
Suíte de Testes (Host)     Rust                      13         ~18.900
Scripts de Automação       PowerShell, Python        16          ~4.800
-------------------------------------------------------------------------------
Total Geral                                         207        ~156.200
```
