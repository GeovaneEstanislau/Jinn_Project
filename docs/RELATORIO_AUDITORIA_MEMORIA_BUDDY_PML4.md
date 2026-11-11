# JINN OS — RELATÓRIO DE AUDITORIA E ENDURECIMENTO DE MEMÓRIA (BUDDY ALLOCATOR, HEAP E PML4)

**Data:** 11 de Novembro de 2026  
**Subsistemas Auditados:** Alocador Físico (`BuddyAllocator`), Kernel Dynamic Heap (`KernelHeap`), Paginação x86-64 (`PML4/PDPT/PD/PT`), Gerenciador de Recursos (`ProcessResources`), Escalonador e Shell  
**Arquivos de Referência:**
- `src/memory/heap.rs`
- `src/memory/buddy.rs`
- `src/memory/paging.rs`
- `src/resources.rs`
- `src/scheduler.rs`
- `src/shell.rs`
- `tests/heap_memory_stress_tests.rs`

---

## 1. Diagnóstico Inicial e Investigação de Consumo Excessivo

Durante a execução em máquinas virtuais com alocação de 1 GiB de memória física (perfil `Standard` do JAHA 1.0), observou-se esgotamento prematuro de memória ao abrir múltiplas janelas no compositor Aladdin e abas/superfícies no Jinn Browser. A auditoria minuciosa dos subsistemas de Ring 0 e Ring 3 revelou cinco vetores críticos de estrangulamento e vazamento:

### 1.1 Teto Fixo de Heap do Kernel (16 MiB) em Máquinas de 1 GiB
- **Causa Raiz:** O alocador de heap do kernel (`src/memory/heap.rs`) possuía constante `HEAP_SIZE = 16 * 1024 * 1024` (16 MiB) e mapeava estaticamente apenas 4.096 páginas no boot.
- **Impacto:** Mesmo com 1 GiB de memória física disponível no `BuddyAllocator` (> 980 MiB livres), o heap do kernel ficava restrito a 16 MiB. Considerando que o backbuffer Limine (1024x768x4 bpp) consome ~3,14 MiB e cada janela de cliente do compositor (ex: navegador ou terminal) aloca um buffer de cliente equivalente em heap de kernel, a criação de apenas 2 a 3 janelas saturava a faixa de 16 MiB, disparando `ENOMEM` no kernel enquanto centenas de megabytes de RAM física permaneciam ociosos.
- **Correção:** Implementação de teto dinâmico de até 128 MiB (`MAX_HEAP_SIZE`), com mapeamento de páginas físicas sob demanda na rotina `alloc_bump` quando a alocação ultrapassar a faixa inicial mapeada.

### 1.2 Fragmentação Severa por Coalescência Unidirecional no Heap do Kernel
- **Causa Raiz:** No método `KernelHeap::dealloc`, ao devolver um bloco desalocado para a lista encadeada livre (`free_list`), o algoritmo verificava unicamente se o bloco liberado era adjacente ao *sucessor* (`cur`). Se blocos fossem liberados em ordem direta ou intercalada, a fusão com o nó *predecessor* (`prev_node`) nunca ocorria.
- **Impacto:** O heap do kernel sofria de fragmentação acelerada. Blocos de memória contíguos de 4 KiB ou 8 KiB não eram reunificados, forçando o alocador `try_alloc_free_list` a falhar e recorrendo desnecessariamente ao `alloc_bump` até a exaustão da memória.
- **Correção:** Introdução de coalescência bidirecional estrita no `src/memory/heap.rs`, fundindo o nó inserido tanto com seu predecessor quanto com seu sucessor.

### 1.3 Desvio Contábil de Páginas Dinâmicas no Encerramento de Processos
- **Causa Raiz:** Na chamada de sistema `sys_alloc_page`, o contador atômico `ALLOCATED_DYNAMIC_PAGES` era incrementado. Contudo, quando um processo Ring 3 era terminado (`exit_task_locked` ou `kill_task_locked`), o kernel liberava a árvore de paginação PML4 (`free_user_pml4`), mas não incrementava `FREED_DYNAMIC_PAGES` para as páginas dinâmicas residuais da tarefa.
- **Impacto:** A telemetria exibida no **Monitor de Sistema** do Aladdin Desktop (`dynamic_pages_allocated - dynamic_pages_freed`) apresentava um falso vazamento perpétuo, acusando páginas ativas mesmo após todas as tarefas de usuário terem sido encerradas.
- **Correção:** Contabilização atômica rigorosa de todas as páginas ativas no vetor `task.dynamic_pages` antes da desmontagem do PML4 na finalização de processos.

### 1.4 Estrangulamento do Vetor de Páginas Dinâmicas (`MAX_DYNAMIC_PAGES`)
- **Causa Raiz:** O limite `MAX_DYNAMIC_PAGES` em `src/scheduler.rs` estava fixado em 1.024 páginas (4 MiB). No perfil de hardware `Standard` (1 GiB de RAM), o limite de quota de memória por processo (`max_user_frames`) é de 4.096 páginas (16 MiB).
- **Impacto:** Um processo Ring 3 no perfil Standard que tentasse alocar mais de 4 MiB recebia `ENOMEM` devido ao esgotamento de slots no array estático da tarefa, antes mesmo de atingir a sua cota configurada de 16 MiB.
- **Correção:** Expansão de `MAX_DYNAMIC_PAGES` para 4.096 entradas, alinhando a capacidade do escalonador ao perfil de 1 GiB de RAM.

### 1.5 Distinção POSIX entre Esgotamento Global (`ENOMEM`) e Cota por Processo (`EDQUOT`)
- **Causa Raiz:** Em `src/resources.rs`, ao ultrapassar `limits.max_frames` ou `limits.max_windows`, o método retornava genericamente `ENOMEM` (-12) ou `EMFILE` (-24).
- **Correção:** O kernel agora reporta estritamente `EDQUOT` (-122) e grava `last_violation_code = -122` quando um processo excede seus limites de cota atribuídos, reservando `ENOMEM` exclusivamente para quando o alocador físico global de RAM estiver verdadeiramente esgotado.

---

## 2. Telemetria e Monitoramento de Memória

### 2.1 Comando `meminfo` Atualizado no Shell
O comando `meminfo` foi reescrito para distinguir com precisão matemática o consumo dos anéis de execução e dos subsistemas de hardware:

```text
=== Informacoes de Memoria e Telemetria ===
  RAM Fisica Total : 1024 MiB
  RAM Fisica Usada : 36 MiB (9216 frames de 4 KiB)
  RAM Fisica Livre : 988 MiB (252928 frames livres)
--------------------------------------------------
  Kernel Heap (R0) : 7 MiB / 128 MiB (Teto Dinamico)
  Tarefas Ring 0   : 64 frames
  Tarefas Ring 3   : 24 MiB (6144 frames mapeados)
  Buffer Grafico   : 3.14 MiB (1024x768x4 bpp Backbuffer)
  Paginas Dinamicas: 0 ativas (800 alocadas, 800 liberadas)
  [Ring 0: diagnostico com precisao de quadros fisicos]
```

### 2.2 Comando `ps` com Diagnóstico de Memória por Processo
A tabela de processos agora inclui colunas de telemetria residente:

```text
PID  NOME                RING  PRIO      MEM (KiB)  FRAMES  ESTADO
---  ------------------  ----  --------  ---------  ------  ----------
  0  kernel              K(0)  Normal          256      64  Executando <-- voce
  1  compositor          U(3)  RealTime       8192    2048  Pronto   
  2  browser             U(3)  Normal         4096    1024  Dormindo 
```

---

## 3. Matriz de Testes e Homologação Local

A suíte de testes unitários e de estresse em `tests/heap_memory_stress_tests.rs` foi ampliada para cobrir os cenários descritos:

| ID | Teste | Cenário Validado | Resultado |
|---|---|---|:---:|
| **T1** | `test_buddy_allocator_coalescence_and_zero_leak` | Alocação de ordens 0, 1 e 3, verificação de alinhamento e coalescência física completa. | **PASSOU** |
| **T2** | `test_dynamic_heap_lifecycle_and_quota_enforcement` | Ciclo de vida `sys_alloc_page`/`sys_free_page`, rejeição de ponteiros desalinhados (`EINVAL`), proteção anti-kernel (`EFAULT`) e rejeição por estouro de cota (`EDQUOT`). | **PASSOU** |
| **T3** | `test_rapid_tab_lifecycle_100_cycles_leak_check` | 100 ciclos consecutivos de abertura e fechamento de abas/superfícies no Jinn Browser; restauração de 100% dos frames físicos ao baseline. | **PASSOU** |
| **T4** | `test_bidirectional_free_list_coalescence` | Coalescência bidirecional de blocos adjacentes em ordens direta e inversa; atendimento de alocações contíguas de 8 KiB. | **PASSOU** |
| **T5** | `test_window_surface_quotas_and_zero_leak` | Criação e destruição de superfícies gráficas; imposição da cota `EDQUOT` na 5ª janela; liberação total de memória sem vazamento de bytes. | **PASSOU** |
| **T6** | `test_system_monitor_telemetry_invariants` | Invariantes contábeis de memória física, heap e VFS sob perfil Standard de 1 GiB. | **PASSOU** |

---

## 4. Geração de Imagem de Boot e Compatibilidade

- **Mídia:** `jinn.iso` gerada e assinada com bootloader híbrido Limine (BIOS + UEFI).
- **Disco Virtual:** `disk.img` particionado com MBR, Partição 1 FAT32 (31 MiB) e Partição 2 Ext2 (31 MiB).
- **Target Oficial:** `x86_64-unknown-none` compilado em modo Release com checagem de tipos estrita.
