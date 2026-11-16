# 🔍 Auditoria Técnica de Fundações para o JHCS
## Mapeamento de Infraestrutura Existente (PCI, KDM, JDA, IPC, Shm e Ring 3)

> **Documento Oficial de Engenharia do Jinn OS**  
> **Status:** Concluído | **Objetivo:** Identificar os módulos e subsistemas já presentes no código-fonte que servem de base para o **Jinn Heterogeneous Compute Service (JHCS)**, evitando duplicação de infraestrutura e preservando o microkernel.

---

## 1. Resumo Executivo da Auditoria

A auditoria exaustiva do repositório `D:\Projetos\Jinn` revelou uma descoberta arquitetural de alto impacto: **o Jinn OS já possui implementadas a maior parte das fundações de baixo nível necessárias para suportar o JHCS**. 

Em vez de projetar novos drivers de DMA, anéis assíncronos ou mecanismos de memória compartilhada do zero, o JHCS pode se acoplar diretamente a três pilares já consolidados no Ring 0:
1. **Kernel DMA Manager (KDM):** 9 syscalls ativas (60 a 68) provendo isolamento de DMA, tokens de segurança e bounce buffers.
2. **Jinn Dataflow Architecture (JDA):** Modelo formal de grafos acíclicos dirigidos (`DataflowNode`), anéis assíncronos de submissão/conclusão (`JdaSqe`/`JdaCqe`) no padrão `io_uring` e pool de memória física compartilhada Zero-Copy.
3. **Hardware Profiler & Cotas (JAHA 1.0):** Classificação determinística de hardware (Perfis A a E) e controle estrito de limites de buffers e frames por processo.

---

## 2. Inventário Detalhado dos Módulos de Fundação

### 2.1. Barramento PCI & Descoberta de Hardware (`src/drivers/pci.rs`)
* **Estado Atual:** `IMPLEMENTADO E VALIDADO`
* **Recursos Disponíveis:**
  * Varredura canônica de 256 barramentos, 32 slots e 8 funções via portas de I/O `0xCF8` / `0xCFC`.
  * Filtro de classe de dispositivos: `class_code == 0x03` identifica controladoras gráficas (VGA e Display Controllers).
  * Mapeamento de fabricantes: detecção de `0x1AF4` (Red Hat / VirtIO), `0x1234` (QEMU / Bochs BGA), `0x8086` (Intel), `0x10DE` (NVIDIA) e `0x1002`/`0x1022` (AMD).
  * Leitura e armazenamento dos 6 registradores de endereço base (BAR0 a BAR5).
  * Habilitação de Bus Mastering: função `enable_bus_mastering()` ativa bits de I/O, MMIO e DMA no registrador PCI Command (`offset 0x04`).
* **Conexão com o JHCS:** O JHCS pode consultar via syscall quais aceleradores PCI classe `0x03` estão presentes no barramento, inicializando o backend VirtIO-GPU (`1AF4:1050`) ou Bochs BGA sem tocar diretamente em portas de I/O a partir de Ring 3.

---

### 2.2. Kernel DMA Manager — KDM (`src/dma/` & Syscalls 60 a 68)
* **Estado Atual:** `IMPLEMENTADO E VALIDADO`
* **Arquivos do Subsistema:**
  * `src/dma/manager.rs`: Orquestrador central e tabelas globais (`DMA_BUFFER_REGISTRY`, `DMA_POOL_MANAGER`).
  * `src/dma/token.rs`: Tokens opacos com contadores de geração de 64 bits para validação temporal anti-use-after-free.
  * `src/dma/buffer.rs` & `pool.rs`: Alocação de pools de frames físicos contíguos alinhados a 4 KiB e 2 MiB.
  * `src/dma/bounce.rs`: Mediador de buffers intermediários seguros (*bounce buffers*) para isolar drivers de espaço de usuário da memória física de outros processos.
  * `src/dma/state.rs`: Registro de quarentena permanente (`QUARANTINE_REGISTRY`) para anular hardware que falhe em responder a comandos de reset.
* **Interface de Chamadas de Sistema (Syscalls):**
  * `sys_dma_pool_create` (Syscall 60), `sys_dma_pool_destroy` (61)
  * `sys_dma_buffer_alloc` (Syscall 62), `sys_dma_buffer_release` (63)
  * `sys_dma_buffer_map` (Syscall 64), `sys_dma_buffer_unmap` (65)
  * `sys_dma_transfer_prepare` (Syscall 66), `sys_dma_transfer_complete` (67)
  * `sys_dma_device_reset_status` (Syscall 68)
* **Conexão com o JHCS:** O JHCS utiliza o KDM para gerenciar buffers de comandos de GPU e transferências DMA de imagens sem que o serviço de Ring 3 tenha privilégios para forjar ponteiros físicos de memória.

---

### 2.3. Jinn Dataflow Architecture — JDA (`src/jda/` & Syscalls 12-14, 26-29)
* **Estado Atual:** `IMPLEMENTADO E VALIDADO`
* **Arquivos do Subsistema:**
  * `src/jda/graph.rs`: Define `DataflowNode`, nós de computação (`OpKind::Compute`), transferência de memória (`MemoryTransfer`), I/O e barreiras de sincronização (`Barrier`). Suporte nativo a resolução de dependências por bitmask de 64 bits (`dependencies_mask` e `resolved_deps_mask`).
  * `src/jda/ring.rs`: Filas circulares lock-free de 32 entradas de submissão (`JdaSqe`) e conclusão (`JdaCqe`) com atômicos de memória (`head`/`tail` em `Acquire`/`Release`).
  * `src/jda/shmem.rs`: Pool de buffers físicos compartilhados (`SHARED_BUFFER_POOL`) com mapeamento Zero-Copy entre processos.
  * `src/jda/topology.rs`: Descoberta de nós NUMA, núcleos lógicos e capacidade de unidades vetoriais.
  * `src/jda/telemetry.rs`: Métricas de ciclos de CPU (TSC) por operação para cálculo de latência e throughput.
* **Interface de Chamadas de Sistema (Syscalls):**
  * `sys_jda_shm_alloc` (Syscall 12), `sys_jda_shm_release` (13), `sys_jda_shm_share` (26), `sys_jda_shm_get_vaddr` (29)
  * `sys_jda_submit` (Syscall 14), `sys_jda_ring_submit` (27), `sys_jda_ring_poll` (28)
* **Conexão com o JHCS:** **O JDA é o alicerce exato do Compute Broker do JHCS.** Em vez de criar um novo mecanismo de enfileiramento, o JHCS pode expor em Ring 3 uma API de alto nível construída sobre os anéis `JdaSqe`/`JdaCqe` e a resolução de grafos do JDA.

---

### 2.4. Hardware Profiler & Gestão de Cotas (`src/services/hardware_profiler/` & `src/resources.rs`)
* **Estado Atual:** `IMPLEMENTADO E VALIDADO`
* **Recursos Disponíveis:**
  * Classificação automática nos Perfis de Hardware JAHA 1.0:
    * `Legacy` (256 MB / 64 MB de teste, 1 vCPU, 16 tarefas máximas, 14 MiB por processo).
    * `Compact` (512 MB, 1 vCPU, 32 tarefas máximas, 16 MiB por processo).
    * `Standard` (1 a 4 GB, 2 vCPUs, 64 tarefas).
    * `Performance` / `HighEnd` (4 a 16 GB+, 4-8+ vCPUs).
  * Estrutura `ResourceLimits`:
    * Limite estrito de frames físicos por processo (`max_frames`).
    * Limite de buffers Zero-Copy JDA (`max_jda_buffers`: 2 no perfil Legacy, 4 no Compact).
    * Limite de janelas gráficas (`max_windows`: 4 no perfil Legacy).
* **Conexão com o JHCS:** O JHCS adapta suas decisões ao perfil ativo: sob o perfil `Legacy`, desativa pipelines que demandem buffers intermediários volumosos e respeita o teto de 4 MiB para pre-caching preditivo.

---

### 2.5. Compositor Aladdin Desktop & Motor Gráfico 2D (`aladdin/`)
* **Estado Atual:** `IMPLEMENTADO E VALIDADO`
* **Recursos Disponíveis:**
  * Loop de eventos a 60 FPS com double-buffering.
  * `aladdin/src/primitives.rs`: Renderização completa em software de retângulos, gradientes, bordas arredondadas, curvas Bézier e blend alpha com 32 bpp.
  * `aladdin/src/wm.rs`: Gerenciamento de janelas clientes e roteamento de superfícies.
* **Conexão com o JHCS:** O motor em software do Aladdin serve de **baseline de validação visual e de desempenho**. Quando o JHCS processa operações na GPU ou FPU/SIMD, a saída é comparada pixel a pixel com a renderização por software para atestar fidelidade matemática de 100%.

---

## 3. Matriz de Reutilização: O que Existe vs. O que o JHCS Adiciona

| Subsistema / Camada | Já Existe no Jinn (Kernel / Ring 0) | O que o JHCS Constrói em Ring 3 |
|---|---|---|
| **Memória Compartilhada** | `src/jda/shmem.rs` (`SHARED_BUFFER_POOL`) | Gerenciamento de superfícies lógicas de janelas e texturas. |
| **Isolamento de DMA** | `src/dma/` (KDM com tokens e quarentena) | Montagem de buffers de comandos gráficos em Ring 3. |
| **Anéis Assíncronos** | `src/jda/ring.rs` (`SubmissionQueue`, `CompletionQueue`) | Fila de requisições de clientes com prioridades de UI. |
| **Grafo de Operações** | `src/jda/graph.rs` (`DataflowNode`, `dependencies_mask`) | API declarativa `ComputeTaskDecl` e pipeline de composição. |
| **Descoberta de GPU** | `src/drivers/pci.rs` (Classe `0x03`, VirtIO `1AF4:1050`) | Backend de emissão de comandos 2D (fill, blit, copy). |
| **Telemetria de Custo** | `src/jda/telemetry.rs` (Ciclos TSC por nó) | Heurística $T_{\text{GPU}} \text{ vs } T_{\text{CPU}}$ para despacho adaptativo. |
| **Fallback em Software** | `aladdin/src/primitives.rs` (Motor 2D puro) | Rota canônica sempre ativa quando a GPU estiver ausente. |

---

## 4. Conclusão da Auditoria

Não há necessidade de modificar o microkernel Jinn para suportar o JHCS. A fundação de Ring 0 (KDM, JDA, PCI, Paging e Syscalls) está **arquiteturalmente madura e pronta para sustentar o serviço**.

O único pré-requisito técnico estrito de Ring 0 que resta ser concluído antes da ativação do backend numérico do JHCS é a **Etapa B (Preservação de Estado FPU/SIMD no Escalonador)**, de modo que o processamento vetorial concorrente não sofra corrupção cruzada de registradores `XMM`.
