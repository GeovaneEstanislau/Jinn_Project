# JINN PROJECT — ESPECIFICAÇÃO ARQUITETURAL
## Kernel DMA Manager (KDM) & Relatório da Fase 0: Auditoria do Código Existente

**Projeto:** Jinn Microkernel (x86_64)  
**Etapa:** Etapa 2 — Infraestrutura de Acesso Seguro a Dispositivos DMA  
**Status:** Especificação Arquitetural e Auditoria da Fase 0 Concluídas (Drivers de Rede Congelados)  
**Dependência:** Etapa 1 — Driver Manager e PS/2 em Ring 3 (Aprovada Condicionalmente em QEMU)  
**Regra de Implementação:** Nenhum driver de rede será modificado ou migrado antes da validação integral desta especificação e da homologação do KDM.

---

# PARTE I — RELATÓRIO DA FASE 0: AUDITORIA DE COMPATIBILIDADE DO CÓDIGO EXISTENTE

Antes de iniciar a estruturação do Kernel DMA Manager, realizou-se uma auditoria minuciosa em todos os subsistemas de baixo nível do Jinn Microkernel para identificar o que o kernel já provê, onde residem os pontos de integração naturais e quais lacunas estruturais devem ser preenchidas.

### 1. Buddy Allocator e Alocação Contígua de Memória Física
- **Localização:** [`src/memory/buddy.rs`](file:///e:/Projetos/Jinn/src/memory/buddy.rs) e [`src/memory/alloc.rs`](file:///e:/Projetos/Jinn/src/memory/alloc.rs).
- **Capacidade Identificada:** O alocador Buddy implementa nativamente `alloc(order: usize) -> Option<u64>` e `free(paddr: u64, order: usize)`. O kernel já exporta publicamente as funções:
  - `crate::memory::alloc_frame() -> Option<u64>` (ordem 0 = 4 KiB);
  - `crate::memory::alloc_frames(order: usize) -> Option<u64>` (ordem $N = 4096 \times 2^N$ bytes, com alinhamento físico natural a $2^N$ páginas);
  - `crate::memory::free_frame(paddr: u64)`;
  - `crate::memory::free_frames_order(paddr: u64, order: usize)`.
- **Diagnóstico para o KDM:** O Buddy Allocator está 100% pronto para atender requisições de buffers DMA de ordens arbitrárias (ex.: ordem 2 = 16 KiB para anéis VirtIO; ordem 0 = 4 KiB para pacotes Ethernet padrão).
- **Necessidade Identificada:** O KDM deve atuar como camada de rastreamento para garantir que essas páginas alocadas permaneçam *pinned* (bloqueadas, não desalocáveis prematuramente).

### 2. Gerenciador de Memória Virtual e Paginação
- **Localização:** [`src/memory/paging.rs`](file:///e:/Projetos/Jinn/src/memory/paging.rs).
- **Capacidade Identificada:** Funções seguras de manipulação de tabelas de 4 níveis (PML4):
  - `map_page(pml4: *mut PageTable, virt: u64, phys: u64, flags: u64)`;
  - `unmap_page(pml4: *mut PageTable, virt: u64)`;
  - Flags de controle disponíveis: `USER_RO`, `USER_RW`, `USER_NX` (No-Execute), `KERNEL_RW`.
- **Diagnóstico para o KDM:** Capacidade total de mapear páginas físicas de DMA no espaço de endereçamento do processo de driver em Ring 3 com atributos estritos de proteção (leitura exclusiva para TX; leitura/escrita para RX; e bit NX ativo para impedir execução de código a partir de dados de rede).

### 3. Tabela de Syscalls e Vetores Livres
- **Localização:** [`src/syscall.rs`](file:///e:/Projetos/Jinn/src/syscall.rs#L370-L450).
- **Capacidade Identificada:** Atualmente existem 60 syscalls implementadas (números `0..=59`). As chamadas `54..=59` foram alocadas para o Driver Manager e telemetria da Etapa 1.
- **Diagnóstico para o KDM:** O intervalo `60..=68` está **completamente vago** no despachador `syscall_dispatch`. As 9 syscalls propostas pelo KDM encaixam-se perfeitamente sem necessidade de remapeamento de código legado.

### 4. Barramento PCI e Controle de Dispositivos
- **Localização:** [`src/drivers/pci.rs`](file:///e:/Projetos/Jinn/src/drivers/pci.rs).
- **Capacidade Identificada:** Leitura e escrita completas via portas `0xCF8` e `0xCFC` (`pci_read_u32`, `pci_write_u32`, `pci_read_u16`, `pci_write_u16`, `pci_read_u8`). Função `enable_bus_mastering(bus, slot, func)` ativa bit 2 no Command Register (offset 0x04).
- **Lacuna Identificada na Fase 0:** Falta a função complementar `disable_bus_mastering(bus, slot, func)`. Na recuperação de falhas (Fase 6 do KDM), o kernel precisará forçar o desligamento imediato de Bus Master no Command Register para cessar transferências DMA ativas do hardware.

### 5. Estado Atual do Suporte a IOMMU
- **Diagnóstico:** O Jinn Microkernel atualmente **não possui** driver para Intel VT-d (DMAR) ou AMD-Vi (IVRS).
- **Consequência Arquitetural Imediata:** O **Backend B (Kernel-Mediated Bounce Buffers)** é mandatório para todas as plataformas atuais do Jinn (QEMU padrão, VirtualBox e hardware x86 clássico). O Backend A (IOMMU) será modelado arquiteturalmente na especificação como uma interface conectável, pronta para ser ativada assim que a tabela ACPI DMAR for incorporada.

### 6. Padrão Existente de Buffers Compartilhados com Geração (JDA)
- **Localização:** [`src/jda/shmem.rs`](file:///e:/Projetos/Jinn/src/jda/shmem.rs).
- **Capacidade Identificada:** O subsistema JDA já utiliza handles de 32 bits codificando `slot` e `generation` (`handle = (gen << 16) | slot`), rastreamento de `mapped_mask` por PID e função de limpeza em lote `release_all_for_pid(pid)`.
- **Diagnóstico para o KDM:** O KDM adotará a mesma semântica robusta de handles com geração do JDA para `DmaToken` e `DmaBufferHandle`, garantindo consistência idiomática em todo o microkernel.

### 7. Auditoria dos Drivers de Rede Atuais em Ring 0
- **Localização:** [`src/drivers/net/virtio.rs`](file:///e:/Projetos/Jinn/src/drivers/net/virtio.rs), [`e1000.rs`](file:///e:/Projetos/Jinn/src/drivers/net/e1000.rs), [`pcnet.rs`](file:///e:/Projetos/Jinn/src/drivers/net/pcnet.rs).
- **Análise Crítica:**
  - O VirtIO-Net aloca anéis VirtQueue (`VRingDesc`) de 256 entradas diretamente com endereços físicos brutos (`desc.addr = buf_phys`) e grava na porta I/O `VIRTIO_PCI_QUEUE_NOTIFY`.
  - O E1000 grava registradores MMIO `RDBAL`/`TDBAL` com ponteiros físicos dos anéis circulares.
  - O PCnet programa registradores de 16/32 bits com endereços de descritores contíguos na memória baixa.
- **Conclusão:** Nenhum desses controladores pode ser executado em Ring 3 sem mediação. Se um processo de usuário tiver acesso irrestrito a esses descritores ou portas, ele poderá ordenar ao hardware que sobrescreva páginas críticas do kernel. O KDM será o único proprietário dos descritores e registradores PCI.

---

# PARTE II — ESPECIFICAÇÃO ARQUITETURAL DO KERNEL DMA MANAGER

## 1. Princípios de Segurança e Isolamento

1. **Propriedade Exclusiva:** O kernel é o único dono dos recursos físicos e dos anéis de descritores dos controladores de rede.
2. **Opacidade de Endereços:** Processos em Ring 3 **nunca recebem nem transmitem endereços físicos**. Operam exclusivamente via handles opacos e offsets virtuais relativos ao buffer alocado.
3. **Verificação de Limites Sem Overflow:** Toda operação calcula limites com aritmética protegida contra overflow (`offset.checked_add(len)`).
4. **Semântica de Quarentena em Falhas:** O encerramento do driver ou expiração de timeout operacional **não comprova cessação de DMA**. Memória com transferências em voo entra em quarentena até que o hardware seja devidamente reiniciado pelo kernel.
5. **Diferenciação Clara de Plataformas:**
   - *Com IOMMU:* Isolamento físico por domínios de tradução de barramento.
   - *Sem IOMMU:* Isolamento lógico por mediação estrita de **Bounce Buffers** do kernel, onde o hardware DMA apenas contra buffers seguros do Ring 0.

---

## 2. Posição no Microkernel e Organização Modular

```
src/
 └── dma/
      ├── mod.rs          // Fachada pública e interface de dispatch
      ├── pool.rs         // Reserva de páginas pinned e quotas por driver
      ├── buffer.rs       // Estrutura DmaBuffer, metadados e ciclo de vida
      ├── descriptor.rs   // Validação de offsets, comprimentos e direções
      ├── token.rs        // Identificadores de autorização (id, generation, pid)
      ├── transfer.rs     // Máquina de estados de submissão e conclusão
      ├── mapping.rs      // Mapeamento virtual seguro na PML4 do processo
      ├── iommu.rs        // Domínios de hardware (abstração para DMAR)
      ├── bounce.rs       // Buffers intermediários do kernel (Backend B)
      ├── recovery.rs     // Quarentena de memória e reset seguro de Bus Master
      ├── policy.rs       // Seleção dinâmica do backend de segurança
      └── tests.rs        // Suíte de testes unitários e adversariais
```

---

## 3. Tipos e Estruturas Fundamentais

### 3.1 Tipos Básicos
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmaDirection {
    DeviceReadsMemory,   // TX: Memória do host -> Placa de Rede
    DeviceWritesMemory,  // RX: Placa de Rede -> Memória do host
    Bidirectional,       // Reservado para protocolos de controle estrito
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmaBackend {
    Iommu,               // Tradução por domínios de hardware
    KernelBounce,        // Buffers intermediários de Ring 0 (padrão legado)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmaBufferState {
    Allocated,           // Páginas físicas pinned alocadas
    Mapped,              // Mapeado no espaço virtual do processo
    Prepared,            // Descritor validado e bloqueado para modificação
    InFlight,            // Transferência física de barramento ativa
    Completed,           // Notificação de conclusão confirmada
    Revoking,            // Processo de encerramento em andamento
    Released,            // Páginas sanitizadas e devolvidas ao Buddy
    Quarantined,         // Dispositivo não confirmou parada; memória isolada
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmaError {
    InvalidSize,
    InvalidAlignment,
    InvalidHandle,
    InvalidToken,
    PermissionDenied,
    OutOfMemory,
    OutOfBounds,
    InvalidDirection,
    InvalidState,
    DeviceBusy,
    DeviceUnavailable,
    MappingFailed,
    Timeout,
    DeviceResetFailed,
    UnsupportedPlatform,
}
```

### 3.2 DmaToken e DmaBuffer
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmaToken {
    pub id: u64,
    pub generation: u64,
    pub owner_pid: u64,
    pub device_id: u32,
}

pub struct DmaBuffer {
    pub id: u64,
    pub generation: u64,
    pub owner_pid: u64,
    pub device_id: u32,
    pub capacity: usize,
    pub alignment: usize,
    pub direction: DmaDirection,
    pub backend: DmaBackend,
    pub state: DmaBufferState,
    pub user_vaddr: Option<u64>,
    pub phys_addr: u64,          // Endereço físico retido exclusivamente no kernel
    pub active_transfers: u32,
}
```

---

## 4. Syscalls do Kernel DMA Manager (Syscalls 60..=68)

| Syscall | Identificador | Argumentos Principais | Ação de Segurança |
|:---:|---|---|---|
| **60** | `sys_dma_buffer_create` | `dev_id, size, align, dir, flags` | Valida quotas, reserva páginas *pinned*, gera token |
| **61** | `sys_dma_buffer_map` | `token, out_vaddr` | Mapeia na PML4 do processo com flags `USER_RW`/`USER_RO` + `NX` |
| **62** | `sys_dma_buffer_unmap` | `token` | Desfaz mapeamento virtual sem liberar página física |
| **63** | `sys_dma_transfer_prepare` | `token, offset, length, dir` | Valida `offset + len <= capacity`, alinhamento e direção |
| **64** | `sys_dma_transfer_submit` | `token, transfer_id` | Alimenta anel de DMA do kernel / Bounce Buffer |
| **65** | `sys_dma_transfer_poll` | `dev_id, out_cqe` | Coleta conclusões de transferências na fila CQ |
| **66** | `sys_dma_buffer_release` | `token` | Libera buffer (rejeitado se houver transferências *InFlight*) |
| **67** | `sys_dma_pool_create` | `dev_id, total_quota, backend` | Estabelece limites estritos de memória DMA por processo |
| **68** | `sys_dma_pool_destroy` | `pool_id` | Invalida pool e recolhe buffers não utilizados |

---

## 5. Fluxos de Transmissão e Recepção no Backend sem IOMMU (Bounce Buffers)

### 5.1 Fluxo de Transmissão (TX)
1. **Ring 3:** O driver monta o pacote Ethernet no buffer de usuário mapeado (`user_vaddr`).
2. **Ring 3 -> Ring 0:** Invoca `sys_dma_transfer_prepare` passando token, offset e tamanho do pacote.
3. **Validação no Kernel:** O KDM checa token, ownership, estado do buffer e MTU ($\le 1518$ bytes).
4. **Cópia para Bounce Buffer:** O kernel copia os dados validados do buffer de usuário para o **Kernel Bounce Buffer** (região contígua pertencente ao Ring 0).
5. **Disparo no Hardware:** O kernel programa o anel de TX da placa (VirtIO desc, E1000 desc) com o endereço físico do Bounce Buffer e aciona o dispositivo.
6. **Conclusão:** O ISR de rede detecta envio concluído, atualiza a fila de conclusão do driver (`CQ`) e libera o descritor.

### 5.2 Fluxo de Recepção (RX)
1. **Pre-alocação no Kernel:** O kernel mantém buffers RX de Ring 0 enfileirados na placa de rede.
2. **DMA da Placa:** O hardware recebe o pacote do cabo e executa DMA contra o Bounce Buffer do kernel.
3. **Validação no ISR/Kernel:** O kernel valida o tamanho do pacote recebido contra anomalias.
4. **Notificação ao Driver:** O KDM insere um evento na fila de eventos do driver de rede.
5. **Transferência para Ring 3:** Quando o driver processa o pacote, o kernel copia os bytes do Bounce Buffer para o buffer de recepção mapeado do processo (ou efetua remap atômico de página em plataformas que suportam).

---

## 6. Coerência de Cache e Sincronização

A interface formaliza duas barreiras de memória explícitas:
- `sync_for_device(buffer: &DmaBuffer, direction: DmaDirection)`: Executa `sfence` / `clflush` quando aplicável, garantindo que escritas da CPU alcancem a memória física antes do acionamento do dispositivo.
- `sync_for_cpu(buffer: &DmaBuffer, direction: DmaDirection)`: Executa `lfence`, garantindo que leituras da CPU vejam dados atualizados pelo DMA do dispositivo.

---

## 7. Recuperação de Falhas e Quarentena de Memória

Para eliminar a suposição perigosa de que "timeout encerrou o DMA", o protocolo de recuperação define:

```
[Driver Crash / Timeout / Revogação]
                 │
                 ▼
 1. Bloqueio imediato de novas submissões (Token Generation Invalidation)
                 │
                 ▼
 2. Desativação de Bus Master via PCI Command Register (disable_bus_mastering)
                 │
                 ▼
 3. Emissão de Reset de Hardware no Dispositivo (VirtIO status = 0 / E1000 CTRL.RST)
                 │
                 ▼
 4. Verificação de Confirmação de Hardware:
         ├── Dispositivo confirmou Reset? ──► SUCESSO: Sanitiza páginas e libera no Buddy
         │
         └── Timeout de confirmação? ───────► QUARENTENA:
                                              - Páginas NUNCA voltam ao Buddy
                                              - Isolamento permanente contra corrupção
                                              - Log crítico de alerta de hardware
```

---

## 8. Matriz dos 24 Testes Adversariais do KDM

A homologação da Etapa 2 exigirá aprovação nos 24 cenários estritos:
- **DMA-01:** Criação e destruição de buffer com parâmetros válidos.
- **DMA-02:** Rejeição de tamanho zero.
- **DMA-03:** Rejeição de alocação que exceda a quota do driver.
- **DMA-04:** Rejeição de alinhamento inválido (não múltiplo de página/hardware).
- **DMA-05:** Rejeição de handle inexistente ou forjado.
- **DMA-06:** Rejeição de token pertencente a outro PID.
- **DMA-07:** Rejeição de token com geração obsoleta (`ESRCH`).
- **DMA-08:** Rejeição de offset apontando fora dos limites do buffer.
- **DMA-09:** Rejeição de overflow na soma `offset + length`.
- **DMA-10:** Rejeição de direção incompatível (tentar enviar buffer marcado para RX).
- **DMA-11:** Rejeição de submissão duplicada de transferência ativa.
- **DMA-12:** Bloqueio de liberação de buffer enquanto houver DMA em voo.
- **DMA-13:** Recuperação segura após queda/crash do processo do driver.
- **DMA-14:** Acionamento de quarentena quando o dispositivo não responder ao reset.
- **DMA-15:** Bloqueio total de acesso após revogação de mapeamento.
- **DMA-16:** Tratamento e saturação segura da fila de conclusão (CQ overflow).
- **DMA-17:** Seleção automática do backend Bounce Buffer na ausência de IOMMU.
- **DMA-18:** Tratamento controlado de erro em mapeamento IOMMU.
- **DMA-19:** Bloqueio de qualquer tentativa de programar descritor apontando para o kernel.
- **DMA-20:** Consistência de estado em corrida entre conclusão de evento e revogação.
- **DMA-21:** Rejeição de reuso de buffer com geração antiga.
- **DMA-22:** Rollback integral em falha durante alocação parcial de pool.
- **DMA-23:** Garantia de ausência de vazamento de frames físicos em falha de mapeamento.
- **DMA-24:** Garantia de integridade de dados e ordenação de cache pós-sincronização.

---

## 9. Plano de Ação para a Fase 1

Com a Fase 0 concluída e auditada:
- A Etapa 1 permanece congelada.
- A migração de qualquer driver de rede continua bloqueada.
- A **Fase 1** consistirá unicamente na implementação dos tipos puros (`src/dma/token.rs`, `src/dma/buffer.rs`, `src/dma/descriptor.rs`) e testes unitários de validação lógica de estados, sem impacto no código operacional do kernel.

---

## 10. Conclusão e Homologação da Fase 1

A Fase 1 foi implementada com sucesso integral em conformidade com as diretrizes de baixo nível do Jinn Microkernel:

### 10.1 Artefatos de Código Desenvolvidos
- `src/dma/types.rs`: Constantes de teto físico 32-bit (`DMA_32BIT_MAX_ADDR`), alinhamento de página (4 KiB), cotas de buffer/pool, enums `DmaDirection`, `DmaBackend` e códigos de erro formais `DmaError` mapeados para convenções POSIX.
- `src/dma/token.rs`: Struct `DmaToken`, validação multi-fator (`owner_pid`, `device_id`, `generation`) e codificação/decodificação de handles opacos de 64 bits para o Ring 3.
- `src/dma/state.rs`: Máquina de estados formal (`Allocated`, `Mapped`, `Prepared`, `InTransfer`, `Completed`, `Revoking`, `Released`, `Quarantined`) com validação de transições válidas e sumidouro terminal estrito para o estado `Quarantined`.
- `src/dma/buffer.rs`: Struct `DmaDescriptor` com validação de limites à prova de estouro aritmético (`checked_add`), controle `DmaBuffer` e `QuarantineRegistry` com capacidade de isolamento de frames físicos contra reciclagem pelo Buddy Allocator.
- `src/dma/tests.rs`: Bateria com 8 testes unitários puros cobrindo segurança de tokens, overflow aritmético, ciclo de vida da máquina de estados, rejeição de transições ilegais, liberação graciosa pós-reset e quarentena permanente.
- `src/dma/mod.rs`: Ponto de entrada limpo e modular para o subsistema KDM.

### 10.2 Resultados da Homologação em Execução Real (QEMU)
- **KDM Fase 1 Selftests:** `8/8 aprovados` (100% de sucesso).
- **Isolamento de Drivers (Etapa 1):** `12/12 aprovados` (PS/2 User-Space, fallback de Ring 0 validado).
- **Driver Supervisor:** `5/5 aprovados`.
- **ELF Loader Security:** `16/16 aprovados`.
- **Reconciliação de Memória JAHA 1.2:** `0 bytes de discrepância`.
- **Testes de Rede / JinnWall / TCP:** `38/38 aprovados`.
- **Drivers de Rede:** Permanecem 100% intocados e em Ring 0 (`src/drivers/net/`).
- **Aladdin Desktop Suite:** `43/43 aprovados`.

---

## 11. Conclusão e Homologação da Fase 2

A **Fase 2 (Alocação Física e Mediador de Bounce Buffers - Backend B)** foi implementada e homologada com sucesso integral:

### 11.1 Artefatos de Código Desenvolvidos
- `src/dma/pool.rs`:
  - `DmaPool`: Gerenciamento de blocos contíguos físicos com alinhamento Buddy Allocator via `pages_to_order`.
  - Restrição de 32 bits estrita: Qualquer alocação acima de 4 GiB (`DMA_32BIT_MAX_ADDR = 0xFFFF_FFFF`) é rejeitada com `UnsupportedPlatform` e liberada imediatamente.
  - Sub-alocador por mapa de bits (`page_bitmap: u32`) para até 32 páginas por pool.
  - `DmaPoolManager`: Cota por processo de 32 páginas (128 KiB), isolamento por PID/dispositivo e quarentena permanente mandatória de todos os frames em caso de destruição de pool com buffers não liberados.
- `src/dma/bounce.rs`:
  - `BounceBufferMediator`: Mediador de barramento para Backend B.
  - Prevenção de Race Conditions e TOCTOU: O hardware NUNCA acessa ponteiros de Ring 3. Todo tráfego é intermediado pelo Bounce Buffer de Ring 0 com barreiras `SeqCst`.
  - Operações `prepare_tx` / `prepare_tx_slice` (cópia Ring 3 -> Bounce Buffer antes da ativação do HW).
  - Operações `prepare_rx` (sanitização do buffer com zeros) e `complete_rx` / `complete_rx_slice` (cópia Bounce Buffer -> Ring 3 somente após conclusão atestada do DMA).
- `tests/kdm_host_tests.rs`:
  - Suíte de integração no host com 5 baterias de testes (incluindo teto de 32 bits, quarentena por vazamento e integridade de ida e volta).
- `src/dma/tests.rs`:
  - Expandido de 8 para 16 testes unitários em execução direta no boot do kernel.

### 11.2 Resultados da Homologação em Execução Real (QEMU)
- **KDM Fase 1 e 2 Selftests:** `16/16 aprovados` (100% no boot do kernel).
- **Host Unit Tests (`tests/kdm_host_tests.rs`):** `5/5 aprovados` (0 falhas).
- **Driver Manager (Etapa 1):** `12/12 aprovados`.
- **Driver Supervisor:** `5/5 aprovados`.
- **ELF Loader Security:** `16/16 aprovados`.
- **Reconciliação de Memória JAHA 1.2:** `0 bytes de discrepância`.
- **Testes de Rede / JinnWall / TCP:** `38/38 aprovados`.
- **Aladdin Desktop Suite:** `43/43 aprovados`.
- **Drivers de Rede (`src/drivers/net/`):** Permanecem 100% intocados e em Ring 0.

