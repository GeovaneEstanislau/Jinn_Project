# IPC Core — Jinn OS

## 1. Visão Geral

O `IPC Core` (*Inter-Process Communication*) é a espinha dorsal de comunicação do Jinn OS. Em uma arquitetura de microkernel, onde drivers, sistemas de arquivos, pilhas de rede e subsistemas de gerenciamento executam isolados no espaço de usuário (*user-space*), a eficiência e a previsibilidade do mecanismo de IPC determinam diretamente o desempenho global de todo o sistema operacional.

O IPC Core do Jinn foi projetado com base em três modalidades essenciais:
1. **Passagem Síncrona de Mensagens (*Synchronous Rendezvous*):** Transferência direta de controle e dados com latência ultra-baixa através de registradores de CPU (*Direct Thread Switching*).
2. **Filas Assíncronas de Alta Vazão (*Lock-Free Ring Buffers*):** Projetadas para drivers de streaming (áudio, rede, GPU) e eventos sem bloqueio de thread.
3. **Transferência de Memória de Cópia Zero (*Zero-Copy Shared Pages*):** Troca de propriedade de páginas de memória física mediada por capacidades para grandes volumes de dados.

### Princípios de Design

- **Latência Ultra-Baixa no Caminho Crítico:** Mensagens pequenas (<64 bytes) transitam diretamente através dos registradores de CPU (`rdx`, `rsi`, `r8`..`r15`), sem acessar a RAM ou alocar estruturas no kernel.
- **Troca Direta de Contexto (*Direct Switch / Hand-off*):** Ao enviar uma mensagem síncrona, a thread emissora cede seu quantum de CPU diretamente à thread receptora, eliminando o overhead de reinserção na fila de escalonamento global.
- **Mediação Estrita por Capacidades:** Nenhuma mensagem é transmitida sem a apresentação de uma capacidade de canal válida no C-Space do remetente.
- **Prevenção Determinística de Deadlocks:** Todas as operações síncronas suportam *timeouts* explícitos e tratamento automático de falha de extremidade (*broken pipe / dead peer*).
- **Semântica de Zero-Copy Segura:** Compartilhamento de buffers físicos com garantias de isolamento de memória e invalidação atômica de páginas.

---

## 2. Objetivos

- ✅ Oferecer latência de IPC síncrono `<500ns` em chamadas locais (*round-trip RPC*).
- ✅ Suportar transferência de mensagens em registradores de CPU (*Fast-Path Registers*).
- ✅ Prover canais assíncronos baseados em anéis circulares *lock-free* com taxa de transferência superior a 10 milhões de mensagens/segundo.
- ✅ Implementar transferência *Zero-Copy* para payloads grandes (>4 KB) via manipulação de páginas físicas no Memory Core.
- ✅ Integrar-se diretamente ao Security Core para validação O(1) de permissões por canal.
- ✅ Suportar *Direct Thread Switch* no Scheduler Core para eliminação de *jitter*.

---

## 3. Responsabilidades

- **Gerenciamento de Canais e Endpoints:** Criação, ciclo de vida e destruição de portas de comunicação bidirecionais e unidirecionais.
- **Roteamento e Entrega de Mensagens:** Gerenciar buffers de mensagens, filas de espera de threads e transições de estado (Ready, BlockedOnSend, BlockedOnReceive).
- **Transferência de Direitos e Capacidades:** Permitir que mensagens IPC transportem capacidades para novos recursos (*capability passing*).
- **Sincronização e Rendezvous:** Coordenar o encontro síncrono de threads emissoras e receptoras.
- **Supervisão de Falhas:** Notificar extremidades quando um serviço peer for terminado inesperadamente.

---

## 4. Arquitetura Interna

O IPC Core opera através de três camadas de transporte coordenadas:

```
┌─────────────────────────────────────────────────────────────────┐
│                      Aplicações / Serviços                      │
│             (User Space: Driver A, VFS, Net, GUI)               │
└────────────────────────┬────────────────────────────────────────┘
                         │ Syscalls IPC (send, recv, call, reply)
┌────────────────────────▼────────────────────────────────────────┐
│                   IPC Core Interface & Security                 │
│  ┌─────────────────────────┐  ┌──────────────────────────────┐  │
│  │ Capability Check Gate   │  │   Endpoint Lookup & State    │  │
│  └─────────────────────────┘  └──────────────────────────────┘  │
│  • Validação O(1) de permissão no C-Space                       │
│  • Despacho para a modalidade de transporte ideal               │
└────────────────────────┬────────────────────────────────────────┘
                         │
        ┌────────────────┼────────────────┐
        │                │                │
┌───────▼──────┐ ┌───────▼──────┐ ┌───────▼──────────────────────┐
│ Synchronous  │ │ Asynchronous │ │ Zero-Copy Page Transfer      │
│ Rendezvous   │ │ Ring Buffers │ │ (Shared Physical Pages)      │
│ (Fast-Path)  │ │ (Lock-Free)  │ │ (Memory Core Mediated)       │
│ • CPU Regs   │ │ • Producer/  │ │ • Page re-mapping            │
│ • Direct     │ │   Consumer   │ │ • IOMMU DMA buffers          │
│   Switch     │ │ • High-Hz    │ │                              │
└───────┬──────┘ └───────┬──────┘ └───────┬──────────────────────┘
        │                │                │
┌───────▼────────────────▼────────────────▼──────────────────────┐
│                   Scheduler & Memory Core                       │
│  • Hand-off de quantum de CPU   • Remapeamento de TLB / PTEs   │
└─────────────────────────────────────────────────────────────────┘
```

### Modos de Transporte

1. **Fast-Path Synchronous (RPC):** O remetente bloqueia até que o receptor processe e responda. Os dados são transferidos diretamente pelos registradores de CPU da thread.
2. **Lock-Free Ring Buffer (Streaming):** Memória compartilhada pré-alocada onde o produtor escreve e o consumidor lê utilizando ponteiros de cabeça/cauda atômicos (`AtomicUsize`).
3. **Zero-Copy Page Transfer (Bulk):** Páginas físicas inteiras são desmapeadas do emissor e mapeadas no receptor ou marcadas como *Copy-on-Write* (CoW).

---

## 5. Estruturas de Dados Principais

```rust
use core::sync::atomic::{AtomicUsize, AtomicU64, AtomicBool, Ordering};

/// Identificador único de um canal IPC
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChannelId(pub u64);

/// Identificador único de uma porta/endpoint
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EndpointId(pub u64);

/// Tipo de mensagem IPC
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    Signal = 1,          // Notificação leve sem payload (só flags)
    SmallMessage = 2,    // Payload direto via registradores (até 64 bytes)
    BufferedMessage = 3, // Mensagem em buffer gerenciado (até 4 KB)
    PageTransfer = 4,    // Transferência física de páginas (4 KB+)
    CapabilityGrant = 5, // Transferência de tokens de segurança
}

/// Cabeçalho universal de mensagem IPC (16 bytes)
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessageHeader {
    pub sender_domain: u32,
    pub target_domain: u32,
    pub message_type: MessageType,
    pub flags: u8,
    pub payload_size: u16,
    pub sequence_id: u32,
    pub attached_cap_slot: Option<u32>,
}

/// Payload embutido para Fast-Path via registradores (64 bytes)
#[repr(C)]
pub struct FastRegistersPayload {
    pub r0: u64,
    pub r1: u64,
    pub r2: u64,
    pub r3: u64,
    pub r4: u64,
    pub r5: u64,
    pub r6: u64,
    pub r7: u64,
}

/// Estado de um canal IPC bidirecional
pub enum ChannelState {
    Active,
    SenderBlocked { waiting_thread: usize },
    ReceiverBlocked { waiting_thread: usize },
    Closed,
}

/// Estrutura do Canal no Kernel
pub struct IpcChannel {
    pub id: ChannelId,
    pub endpoint_a: EndpointId,
    pub endpoint_b: EndpointId,
    pub state: ChannelState,
    pub total_messages_sent: AtomicU64,
    pub last_activity_tsc: AtomicU64,
}

/// Fila circular assíncrona Lock-Free (SPSC - Single Producer Single Consumer)
pub struct LockFreeRingBuffer<const CAPACITY: usize> {
    pub buffer: [u8; CAPACITY],
    pub head: AtomicUsize, // Atualizado pelo produtor
    pub tail: AtomicUsize, // Atualizado pelo consumidor
}

impl<const CAPACITY: usize> LockFreeRingBuffer<CAPACITY> {
    pub const fn new() -> Self {
        LockFreeRingBuffer {
            buffer: [0u8; CAPACITY],
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    pub fn is_full(&self) -> bool {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        ((head + 1) % CAPACITY) == tail
    }

    pub fn is_empty(&self) -> bool {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Relaxed);
        head == tail
    }
}
```

---

## 6. Fluxos de Execução

### 1. Chamada Síncrona com Troca Direta (*Fast-Path Direct Switch*)

```
Thread Emissora (Client)        IPC Core (Kernel Gate)        Thread Receptora (Server)
       │                                  │                                  │
       ├─ sys_ipc_call(endpoint, regs) ──►│                                  │
       │                                  ├─ 1. Valida Capability do canal   │
       │                                  ├─ 2. Copia regs para context(srv) │
       │                                  ├─ 3. Bloqueia Client (WaitReply)  │
       │                                  ├─ 4. DIRECT SWITCH para Server    │
       │                                  │                                  │
       │ [CLIENT SUSPENSO]                └─────────────────────────────────►│ Executa handler RPC
       │                                                                     │ (usa regs recebidos)
       │                                                                     │
       │◄─────────────────────────────────┬── sys_ipc_reply(client, regs) ───┤
       │ 1. Restaura Client com resposta │                                  │
       │ 2. DIRECT SWITCH de volta        │                                  │
       │                                  │                                  │
       ▼ Continua execução                ▼                                  ▼ Aguarda nova msg
```

### 2. Streaming Assíncrono via Ring Buffer Lock-Free
1. O Produtor verifica `not ring.is_full()`.
2. O Produtor copia a mensagem no slot `buffer[head]` e atualiza `head.store((head + 1) % CAP, Release)`.
3. O Consumidor observa `head != tail`, lê a mensagem de `buffer[tail]` e atualiza `tail.store((tail + 1) % CAP, Release)`.
4. **Zero intervenção do kernel** após a inicialização da página compartilhada.

### 3. Transferência Zero-Copy de Páginas Físicas
1. O Remetente passa um ponteiro de buffer de 4 KB ou superior na syscall `sys_ipc_share_page(vaddr, target_domain, flags)`.
2. O IPC Core consulta o `MemoryCore` para traduzir `vaddr` para o frame físico `paddr`.
3. O `MemoryCore` remove o mapeamento da tabela de páginas do remetente (ou marca como somente-leitura se CoW).
4. O frame é mapeado no espaço de endereçamento do receptor e o C-Space é atualizado.

---

## 7. Interfaces Públicas (Syscalls de IPC)

```rust
pub trait IpcSyscalls {
    /// Envia uma mensagem síncrona e bloqueia até a confirmação de recebimento
    fn sys_ipc_send(
        channel_cap_slot: usize,
        header: &MessageHeader,
        payload_regs: &FastRegistersPayload,
        timeout_ticks: u64,
    ) -> Result<(), IpcError>;

    /// Bloqueia a thread atual até a chegada de uma mensagem no endpoint
    fn sys_ipc_recv(
        endpoint_cap_slot: usize,
        out_header: &mut MessageHeader,
        out_payload_regs: &mut FastRegistersPayload,
        timeout_ticks: u64,
    ) -> Result<(), IpcError>;

    /// Chamada RPC completa: envia mensagem e aguarda resposta no mesmo quantum
    fn sys_ipc_call(
        channel_cap_slot: usize,
        header: &MessageHeader,
        payload_regs: &mut FastRegistersPayload,
        timeout_ticks: u64,
    ) -> Result<(), IpcError>;

    /// Responde a uma chamada síncrona anterior
    fn sys_ipc_reply(
        target_domain: DomainId,
        header: &MessageHeader,
        payload_regs: &FastRegistersPayload,
    ) -> Result<(), IpcError>;

    /// Cria um novo par de canais IPC conectados
    fn sys_ipc_channel_create() -> Result<(usize, usize), IpcError>; // Retorna slots de capability

    /// Compartilha uma página física de memória (Zero-Copy) com outro domínio
    fn sys_ipc_share_page(
        target_domain: DomainId,
        vaddr: usize,
        read_only: bool,
    ) -> Result<usize, IpcError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcError {
    InvalidChannelSlot,
    ChannelClosed,
    EndpointDead,
    PermissionDenied,
    Timeout,
    BufferFull,
    BufferEmpty,
    MessageTooLarge,
    CapabilityTransferFailed,
}
```

---

## 8. Integração com Outros Componentes

### Integração com Scheduler Core
- **Direct Thread Switch:** O IPC Core ignora a fila de prontos geral (*RunQueue*) quando há um receptor aguardando, transferindo o contexto do emissor diretamente para o receptor. Isso reduz a latência de troca para menos de 150 ciclos de clock.

### Integração com Security Core
- Toda chamada a `sys_ipc_*` valida se o slot indicado contém uma capacidade de canal válida e se as permissões `READ`/`WRITE` correspondem à operação pretendida.

### Integração com Memory Core
- Coordenação de alocação de páginas para *Ring Buffers* compartilhados e ajuste de permissões de tabelas de página (`PTE`) para transferência *Zero-Copy*.

### Integração com Predictive Engine
- O IPC Core emite telemetria de frequência e tamanho de mensagens por canal. O Predictive Engine usa essas métricas para agrupar threads comunicantes na mesma CPU ou nó NUMA (*affinity tuning*).

---

## 9. Segurança e Isolamento

| Ameaça | Mecanismo de Mitigação |
|--------|------------------------|
| **Canal Esgotado / Flooding DoS** | Canais assíncronos possuem quotas estritas de tamanho; emissores síncronos sofrem bloqueio se o receptor estiver ocupado. |
| **Injeção de Mensagem não Autorizada** | Endpoints são opacos; a entrega só é aceita se o remetente possuir capacidade válida vinculada ao canal. |
| **Deadlock por Peer Travado** | Todas as syscalls síncronas exigem parâmetro de `timeout`; threads nunca ficam bloqueadas indefinidamente. |
| **Vazamento de Memória em Zero-Copy** | O `MemoryCore` revoga e limpa (*zeroes*) páginas compartilhadas se o processo consumidor for encerrado inesperadamente. |

---

## 10. Escalabilidade e Limites

### Metas de Desempenho

| Métrica | Target | Implementação |
|---------|--------|---------------|
| Fast-Path RPC Round-Trip | `<500 ns` | Registradores de CPU + Direct Switch |
| Ring Buffer Throughput | `>10M msgs/s` | SPSC Lock-Free Ring Buffer |
| Zero-Copy Page Transfer | `<2 μs` | Remapeamento de PTE em nível de página |
| Validação de Canal no Gate | `<50 ns` | Checagem de slot em C-Space local |

### Limites Arquiteturais

- **Canais Concorrentes por Processo:** Até 256 canais simultâneos.
- **Tamanho Máximo de Mensagem em Registradores:** 64 bytes (8 palavras de 64 bits).
- **Tamanho Máximo de Mensagem em Buffer:** 4.096 bytes (1 página).
- **Transferência Zero-Copy:** Múltiplos de 4 KB (sem limite prático além da RAM).

---

## 11. Comparação com Sistemas Modernos

| Aspecto | Linux (Unix Sockets / D-Bus) | seL4 Microkernel | Fuchsia (Zircon Channels) | Jinn OS IPC Core |
|---------|------------------------------|------------------|---------------------------|------------------|
| **Fast-Path nos Registradores** | Não (Buffer em kernel space) | Sim (Fastpath ASM) | Não (Buffer em canal) | Sim (Direct Switch + Regs) |
| **Zero-Copy Nativo** | `vmsplice` / `memfd` complexo | Páginas Untyped | VMO Transfer | Mapeamento direto de páginas |
| **Mecanismo de Segurança** | UIDs / SELinux / Capabilities | C-Space Capabilities | Handle Rights | Tokens de Capacidade Tipados |
| **Suporte a Streaming Assíncrono**| `io_uring` / Sockets | Notificações leves | Port / Async Waiters | SPSC Lock-Free Ring Buffers |
| **Prevenção de Deadlocks** | Socket Timeouts | Sem timeout nativo | Timeouts em Zircon | Timeouts obrigatórios com watchdog |

---

## 12. Pseudocódigo Avançado

### Algoritmo 1: Fast-Path Synchronous Call & Direct Switch

```pseudo
function sys_ipc_call(channel_slot, header, payload_regs, timeout):
    // 1. Validação de capacidade
    token = current_cspace().get(channel_slot)
    if token is NULL or not token.is_valid or not token.permissions.write:
        return ERR_PERMISSION_DENIED
        
    channel = get_channel(token.resource_handle)
    target_thread = channel.get_receiver_thread()
    
    if target_thread is NULL:
        return ERR_ENDPOINT_DEAD
        
    // 2. Se o receptor já estiver aguardando (BlockedOnReceive)
    if target_thread.state == STATE_BLOCKED_ON_RECEIVE:
        // Transfere registradores diretamente de contexto para contexto
        target_thread.context.regs = payload_regs
        target_thread.context.header = header
        target_thread.state = STATE_READY
        
        // Marca o cliente como aguardando resposta
        current_thread().state = STATE_BLOCKED_ON_REPLY
        current_thread().reply_from = target_thread.id
        
        // DIRECT SWITCH: Cede a CPU diretamente para o receptor
        scheduler_direct_switch(current_thread(), target_thread)
        
        // Ao acordar, os registradores foram preenchidos pelo reply
        return SUCCESS
    else:
        // Receptor ocupado: enfileira ou aguarda com timeout
        return wait_for_rendezvous(channel, payload_regs, timeout)
```

### Algoritmo 2: Ring Buffer Lock-Free (SPSC Producer / Consumer)

```pseudo
function ring_produce(ring, data_slice):
    current_head = ring.head.load(Relaxed)
    current_tail = ring.tail.load(Acquire)
    
    next_head = (current_head + 1) % RING_CAPACITY
    if next_head == current_tail:
        return ERR_BUFFER_FULL // Buffer cheio
        
    // Escreve os dados na memória compartilhada
    memcpy(&ring.buffer[current_head], data_slice, data_slice.len)
    
    // Publica o novo head com barreira Release
    ring.head.store(next_head, Release)
    return SUCCESS

function ring_consume(ring, out_slice):
    current_head = ring.head.load(Acquire)
    current_tail = ring.tail.load(Relaxed)
    
    if current_head == current_tail:
        return ERR_BUFFER_EMPTY // Sem dados
        
    // Lê os dados
    memcpy(out_slice, &ring.buffer[current_tail], out_slice.len)
    
    // Atualiza tail com barreira Release
    next_tail = (current_tail + 1) % RING_CAPACITY
    ring.tail.store(next_tail, Release)
    return SUCCESS
```

### Algoritmo 3: Transferência Zero-Copy com Verificação de Memória

```pseudo
function sys_ipc_share_page(target_domain, vaddr, is_read_only):
    // 1. Valida alinhamento de 4KB
    if (vaddr % PAGE_SIZE) != 0:
        return ERR_INVALID_ALIGNMENT
        
    // 2. Resolve endereço virtual para frame físico
    paddr = memory_core_translate(current_domain(), vaddr)
    if paddr is NULL:
        return ERR_PAGE_NOT_MAPPED
        
    // 3. Valida se o domínio atual possui capability de escrita sobre a página
    if not memory_core_has_cap(current_domain(), paddr, CAP_WRITE):
        return ERR_PERMISSION_DENIED
        
    // 4. Mapeia no espaço de endereço do domínio alvo
    target_vaddr = memory_core_find_free_vaddr(target_domain)
    flags = is_read_only ? PROT_READ : (PROT_READ | PROT_WRITE)
    memory_core_map_page(target_domain, target_vaddr, paddr, flags)
    
    // 5. Emite Capability Token de memória compartilhada para o alvo
    grant_memory_capability(target_domain, paddr, flags)
    
    return target_vaddr
```

---

## 13. Diagramas ASCII Avançados

### Linha do Tempo de Direct Thread Switch (RPC em Registradores)

```
Tempo ────────────────────────────────────────────────────────────────────────►
Client Thread:  [Executa]──►[sys_ipc_call] ──┐ (Bloqueia em WaitReply)
                                             │
Kernel Gate:                                 ├──[Direct Switch: 120 ciclos]──┐
                                             │                               │
Server Thread:                               └──────────────►[Acorda]──►[Trata Req]──►[sys_ipc_reply]
                                                                                            │
Kernel Gate:                                 ┌──[Direct Switch: 120 ciclos]─────────────────┘
                                             │
Client Thread:  [Acorda com resposta]◄───────┘
```

### Layout de Memória Compartilhada do Ring Buffer

```
 0x0000 ┌────────────────────────────────────────────────────────┐
        │  Header: Head (AtomicUsize), Tail (AtomicUsize)        │
 0x0040 ├────────────────────────────────────────────────────────┤ ◄── Tail (Consumidor lê aqui)
        │  Slot 0: [Header: 16B] [Payload: 48B]                  │
 0x0080 ├────────────────────────────────────────────────────────┤
        │  Slot 1: [Header: 16B] [Payload: 48B]                  │
 0x00C0 ├────────────────────────────────────────────────────────┤ ◄── Head (Produtor escreve aqui)
        │  Slot 2: [Livre para escrita]                          │
        │  ...                                                   │
 0x1000 └────────────────────────────────────────────────────────┘ (4 KB Página Alinhada)
```

---

## 14. Considerações para Implementação em Rust

### Estruturas `repr(C)` e Alinhamento para Cache Line

```rust
// Alinhado a 64 bytes para evitar False Sharing entre núcleos
#[repr(C, align(64))]
pub struct AlignedRingBufferHeader {
    pub head: AtomicUsize,
    pub _pad1: [u8; 56], // Padding para linha de cache L1 exclusiva
    pub tail: AtomicUsize,
    pub _pad2: [u8; 56], // Padding para linha de cache L1 exclusiva
}
```

### Justificativas de `unsafe` e Mitigações

1. **Manipulação Direta de Registradores na Troca de Thread:**
   - **Risco:** Corrupção de registradores não preservados pelo compilador (*callee-saved* vs *caller-saved*).
   - **Mitigação:** Assembly inline estrito (`interrupts.s` / stubs dedicados) salvando e restaurando explicitamente todos os registradores de propósito geral.
2. **Acesso Concorrente a Ring Buffers Compartilhados:**
   - **Risco:** Leituras fora de ordem ou inconsistência de cache.
   - **Mitigação:** Utilização de primitivas atômicas `Acquire` e `Release` que compilam para instruções com barreira de memória no x86_64 (`mfence`/bloqueio implícito de barramento).

---

## 15. Roadmap de Implementação

### Fase 0 (v0.0.1 — Atual)
- [x] Especificação arquitetural completa do IPC Core
- [x] Definição de formatos de mensagem, Fast-Path e Ring Buffers

### Fase 1 (v0.0.2 — Próximo Milestone)
- [ ] Implementação de `IpcChannel` e `sys_ipc_send`/`sys_ipc_recv` no kernel
- [ ] Direct Thread Switch básico entre tarefas registradas no scheduler
- [ ] Integração com o validador de C-Space do Security Core

### Fase 2 (v0.1.0)
- [ ] SPSC Lock-Free Ring Buffers para drivers
- [ ] Transferência Zero-Copy de páginas de memória compartilhada
- [ ] Tratamento de timeout determinístico com PIT/APIC

### Fase 3 (v0.2.0)
- [ ] Roteamento assíncrono avançado com multiplexação (*Select/Poll*)
- [ ] Telemetria de canais integrada ao Predictive Engine

### Fase 4 (v1.0.0)
- [ ] Verificação formal de propriedades livres de deadlocks
- [ ] Otimização para NUMA com nós de mensagens locais

---

## 16. Checklist de Revisão

- [x] Especificação das 3 modalidades de transporte (Síncrono, Ring Buffer e Zero-Copy)
- [x] Metas de latência (<500ns no RPC síncrono) documentadas
- [x] Estruturas de dados Rust completas (`MessageHeader`, `FastRegistersPayload`, `IpcChannel`, `LockFreeRingBuffer`)
- [x] Integração documentada com Scheduler (Direct Switch), Security Core e Memory Core
- [x] Pseudocódigos completos para os 3 algoritmos de comunicação
- [x] Diagramas ASCII de arquitetura, linha do tempo de Direct Switch e layout de Ring Buffer
- [x] Padrões de concorrência segura em Rust com barreiras `Acquire`/`Release`
- [x] Roadmap e fases de entrega alinhados ao projeto Jinn OS

---

Arquivo: [IPC Core](./ipc-core.md)