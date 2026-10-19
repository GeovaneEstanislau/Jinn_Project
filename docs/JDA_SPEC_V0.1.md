# 🏛️ Jinn Dataflow Architecture (JDA) — Especificação Técnica v0.1

> **Status**: Especificação Formal v0.1 (Fase 1)  
> **Sistema**: Jinn Microkernel x86_64 (`no_std`, Bare-Metal)  
> **Licença**: MIT / Apache-2.0  

---

## 1. Visão Geral e Filosofia Arquitetural

O **Jinn Dataflow Architecture (JDA)** é um modelo experimental e subsistema de engenharia projetado para investigar a eliminação de movimentação redundante de dados, a contenção de memória e a ociosidade da CPU em arquiteturas microkernel.

### 1.1. Princípios Fundamentais
1. **Orientação a Dados (Data-Driven Execution)**: Operações computacionais não são despachadas apenas por fatia de tempo (*time-slice*), mas prioritariamente quando seus dados e dependências antecedentes estão garantidamente prontos e alinhados na memória.
2. **Minimização de Cópias (Zero-Copy Transfer)**: Substituição de transferências por cópia em buffer intermediário (`memcpy`) por mapeamento de páginas no PML4 ou passagem de descritores de buffer com validação explícita de ciclo de vida.
3. **Verdade de Hardware (Hardware Truthfulness)**: Nenhuma métrica, largura de banda ou latência é inventada. O sistema distingue rigidamente entre:
   - **ProbedHardware (100%)**: Detectado diretamente via registradores, CPUID, ACPI ou portas de I/O.
   - **Estimated (50%)**: Estimativas analíticas conservadoras declaradas.
   - **Unknown (0%)**: Valores não mensurados onde o sistema adota políticas conservadoras de referência.
4. **Microkernel Não Invasivo & Reversibilidade**: O JDA é desacoplado do núcleo central. Caso desativado (`JDA_ACTIVE = false`), o microkernel opera em seu escalonador Round-Robin e IPC padrão sem qualquer degradação.

---

## 2. Modelo de Topologia de Recursos (Resource Topology Model)

O JDA constrói um inventário interno de hardware através da estrutura `HardwareTopologyModel` (`src/jda/topology.rs`).

### 2.1. Núcleos de CPU (`CpuCoreTopology`)
- **Descoberta**: Sondagem via CPUID Folha 1 (informações do processador) e Folha 7 (extensões de conjunto de instruções).
- **Campos**:
  - `apic_id`: Identificador Local APIC do núcleo.
  - `logical_id`: Índice lógico sequencial no sistema.
  - `is_bsp`: Indica se o núcleo é o Bootstrap Processor.
  - `has_sse`, `has_avx`, `has_avx2`: Extensões vetoriais disponíveis para computação paralela SIMD.
  - `confidence`: Nível de confiança da informação.

### 2.2. Hierarquia de Caches (`CacheDescriptor`)
- **Descoberta**: Sondagem direta via CPUID Folha Determinística de Cache (EAX=4, iterando subfolhas ECX=0..N) e folha estendida 0x80000006.
- **Campos**:
  - `level`: Nível na hierarquia (L1, L2, L3).
  - `cache_type`: `Data`, `Instruction`, `Unified`.
  - `size_bytes`: Capacidade física real em bytes calculada pela fórmula do hardware:
    $$\text{Tamanho} = (\text{Ways} + 1) \times (\text{Partitions} + 1) \times (\text{LineSize} + 1) \times (\text{Sets} + 1)$$
  - `line_size`: Tamanho da linha de coerência em bytes (geralmente 64 bytes).
  - `ways`: Vias de associatividade.

### 2.3. Regiões de Memória DRAM (`MemoryRegionTopology`)
- **Descoberta**: Leitura dos descritores `LimineMemmapEntry` fornecidos pelo bootloader no boot.
- **Campos**:
  - `base_paddr`: Endereço físico base alinhado a 4 KiB.
  - `length`: Tamanho do segmento físico em bytes.
  - `is_usable_dram`: Diferenciação estrita entre DRAM convencional utilizável e regiões reservadas/ACPI/MMIO.

---

## 3. Grafo de Dependências e Fluxo de Operações (Dataflow DAG)

O processamento no JDA é estruturado como um Grafo Acíclico Dirigido (`DataflowGraph` em `src/jda/graph.rs`):

```
       [ Nó 0: Preparação de Dados ]
              /             \
             v               v
   [ Nó 1: Computação A ]  [ Nó 2: Computação B ]
             \               /
              v             v
       [ Nó 3: Barreira / Agregação ]
```

### 3.1. Tipos de Operação (`OpKind`)
- `Compute`: Execução de algoritmo na CPU ou unidade vetorial.
- `MemoryTransfer`: Reassociação ou mapeamento de páginas de memória.
- `IoRead`: Submissão de leitura assíncrona de armazenamento ou rede.
- `IoWrite`: Submissão de escrita assíncrona.
- `Barrier`: Sincronização entre ramos concorrentes.

### 3.2. Ciclo de Vida do Nó (`OpState`)
1. `WaitingDependencies`: O nó possui dependências antecedentes não resolvidas (`dependencies_mask & !resolved_deps_mask != 0`).
2. `Ready`: Todas as dependências foram satisfeitas (`are_dependencies_met() == true`).
3. `Running`: O nó está atualmente despachado e executando.
4. `Completed`: Conclusão bem-sucedida; propaga automaticamente a resolução para todos os nós filhos via máscara de bits de 64 bits em tempo $O(1)$.
5. `Failed` / `Cancelled`: Tratamento e recuperação de falha sem derrubar o kernel.

---

## 4. Modelo de Memória e Zero-Copy (Memory Pipes)

Para eliminar o custo de cópias intermediárias na memória RAM:
- `DataBufferRef`: Aponta para o endereço físico (`phys_addr`), endereço virtual de trabalho (`virt_addr`) e tamanho exato (`size_bytes`).
- Em vez de copiar buffers entre remetente e destinatário:
  1. O remetente aloca o frame físico via PMM (`alloc_frame()`).
  2. O JDA transfere a posse do frame ajustando o mapeamento no PML4 de destino via [`paging::map_page()`](file:///e:/Projetos/Jinn/src/memory/paging.rs#L100).
  3. O TLB do processador é invalidado localmente via `invlpg`.
  4. Nenhuma instrução de movimentação byte a byte é emitida, economizando milhares de ciclos de clock para blocos de dados típicos (4 KiB a 64 KiB).

---

## 5. Telemetria e Medições de Hardware (Observability)

O subsistema registra estatísticas reais através de registradores de hardware (`src/jda/telemetry.rs`):
- `read_tsc()`: Leitura do Timestamp Counter x86 (`rdtsc`) com precisão de ciclos de clock.
- Métricas globais monitoradas:
  - `total_operations_submitted`: Contagem de requisições.
  - `total_operations_completed`: Operações concluídas.
  - `zero_copy_transfers`: Transferências realizadas por manipulação de ponteiros.
  - `redundant_copies`: Cópias tradicionais que não puderam ser evitadas.
  - `bytes_transferred_zero_copy`: Volume total poupado de barramento de memória.
  - `cycles_compute`: Ciclos consumidos em cálculo efetivo.
  - `cycles_waiting`: Ciclos em espera ou contenção.

---

## 6. Políticas de Escalonamento e Seleção de Unidades

O JDA adota o trait `JdaPolicy` (`src/jda/policy.rs`), permitindo a substituição de estratégias em tempo de execução:
1. **FIFO Reference Policy (Baseline)**:
   - Política determinística de referência.
   - Seleciona o primeiro nó pronto na ordem ordinal do vetor.
   - Não realiza pré-busca especulativa.
2. **Dataflow Locality & Priority Policy**:
   - Avalia a prioridade do nó combinada com a localidade de dados.
   - Bonifica tarefas que consomem buffers resultantes de nós que acabaram de executar, aproveitando linhas quentes no cache da CPU.
   - Aplica controle de pré-busca apenas se a confiança for superior a 80% e a pressão de memória for baixa (< 40%).

---

## 7. Modelo de Permissões e Segurança

- **Privilégios de Ring**:
  - Criação de nós e manipulação direta de frames físicos restrita ao Ring 0 (Kernel).
  - Ring 3 interage através de descritores de buffer concedidos e canais IPC auditados.
- **Isolamento de Memória**:
  - Buffers de espaço de usuário permanecem nos endereços inferiores do PML4 (0..255).
  - É proibido compartilhar buffers que cruzem a fronteira de privilégio sem validação de ponteiro nulo e limites de tamanho.

---

## 8. O que é Medido vs O que é Desconhecido

| Recurso | Status de Detecção | Fonte da Informação | Nível de Confiança |
| :--- | :--- | :--- | :--- |
| **Topologia de Núcleo (APIC ID)** | Detectado | CPUID Leaf 1 (EBX) | `ProbedHardware (100)` |
| **Capacidades SIMD (AVX/SSE)** | Detectado | CPUID Leaf 1 e Leaf 7 | `ProbedHardware (100)` |
| **Hierarquia de Cache L1/L2/L3** | Detectado | CPUID Leaf 4 e 0x80000006 | `ProbedHardware (100)` |
| **Mapa de DRAM Física** | Detectado | Limine Bootloader Memmap | `ProbedHardware (100)` |
| **Latência Absoluta de Acesso à RAM** | Desconhecido | Não mensurado pelo hardware | `Unknown (0)` |
| **Largura de Banda PCIe em Tempo Real** | Desconhecido | Depende de contadores MSR uncore | `Unknown (0)` |
| **Topologia NUMA Multissubstrato** | Parcial | Assumido nó único (Single-Node) | `Estimated (50)` |

---
*Fim da Especificação JDA v0.1.*
