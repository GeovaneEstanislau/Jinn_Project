# Scheduler Core — Jinn OS

## 1. Visão Geral

O `Scheduler Core` é o mecanismo central de escalonamento do Jinn OS. Responsável pela gestão eficiente de tarefas e threads, o scheduler separa explicitamente o **mecanismo** (como escalonar) das **políticas** (quem escalonar), permitindo múltiplos perfis operacionais, adaptação dinâmica e integração profunda com a Predictive Engine.

### Princípios de Design

- **Separação Mecanismo/Política:** Core fornece apenas o engine; políticas são plugáveis
- **Adaptabilidade:** Suporte a perfis operacionais distintos (latência, throughput, determinismo)
- **Escalabilidade:** Otimizado para SMP e preparado para NUMA
- **Previsibilidade:** Integração com telemetria e predição para decisões pró-ativas
- **Segurança:** Isolamento de contexto entre tarefas, validação de políticas

## 2. Objetivos

- ✅ Separar engine de escalonamento de políticas de seleção
- ✅ Suportar múltiplos perfis operacionais (desktop, server, industrial, real-time)
- ✅ Integração com Predictive Engine para decisões otimizadas
- ✅ Oferecer baixa latência, alta vazão e determinismo configurável
- ✅ Escalonamento eficiente em sistemas multi-core (SMP) com preparação para NUMA
- ✅ Balanceamento de carga entre CPUs com migração inteligente
- ✅ QoS garantido para serviços críticos

## 3. Responsabilidades

- **Gerenciamento de Filas:** Manter filas de execução por CPU e globais
- **Seleção de Tarefas:** Chamar Policy Layer para decidir próxima tarefa
- **Context Switching:** Realizar troca segura de contexto com save/restore de estado
- **Migração de Threads:** Coordenar migração entre CPUs com validação de affinity
- **Hooks para Políticas:** Expor pontos de extensão (on_enqueue, on_dequeue, on_switch)
- **Integração Predictiva:** Consultar Predictive Engine para hints de carga e otimizações
- **Garantias de QoS:** Enforcar reservas de CPU, latência máxima e fairness

## 4. Arquitetura Interna

### Modelo em Camadas

O Scheduler Core é organizado em três camadas bem definidas:

```
┌─────────────────────────────────────────────────────────────────┐
│                    Predictive Engine                            │
│           (Load hints, cache insights, policies)                │
└────────────────────────┬────────────────────────────────────────┘
                         │ (hints, suggestions)
┌────────────────────────▼────────────────────────────────────────┐
│                  Policy Layer                                   │
│   ┌──────────────┐ ┌──────────────┐ ┌──────────────┐           │
│   │  Desktop     │ │   Server     │ │ Real-Time    │ (plugins) │
│   │  Policy      │ │   Policy     │ │  Policy      │           │
│   └──────────────┘ └──────────────┘ └──────────────┘           │
│   • Prioridades    • Fairness     • Latência      │           │
│   • Aging          • Reservas CPU • Deadline      │           │
└────────────────────────┬────────────────────────────────────────┘
                         │ (seleção de prioridade)
┌────────────────────────▼────────────────────────────────────────┐
│                Scheduler Engine                                 │
│  [Core Kernel] Mecanismo confiável de escalonamento            │
│                                                                  │
│   Per-CPU Runqueues    │    Global Balancer                    │
│   ┌─────────────────┐  │   ┌────────────────┐                 │
│   │ CPU0: RunQueue  │  │   │ Load Analysis  │                 │
│   │ CPU1: RunQueue  │  │   │ Migration Plan │                 │
│   │ CPU2: RunQueue  │  │   │ Affinity Mgmt  │                 │
│   └─────────────────┘  │   └────────────────┘                 │
│                                                                  │
│   • Context switch    • Fila lock-free                         │
│   • Task dispatch     • Atomic operations                       │
│   • State management  • Memory ordering                         │
└────────────────────────┬────────────────────────────────────────┘
                         │ (execução)
┌────────────────────────▼────────────────────────────────────────┐
│                      Hardware                                   │
│         (CPUs, Interrupts, Memory, Caches)                     │
└─────────────────────────────────────────────────────────────────┘
```

### Scheduler Engine

O mecanismo confiável do kernel que realiza:

- Seleção rápida de próxima tarefa (O(1) no melhor caso)
- Context switching seguro com save/restore completo
- Balanceamento de carga periódico
- Validação de afinidade e migrações

Implementado em Rust com `unsafe` mínimo, abstraído em tipos seguros.

### Policy Layer

Interface de extensão para políticas de escalonamento. Cada política implementa:

```rust
pub trait SchedulerPolicy {
    fn choose_priority(&self, rq: &RunQueue, cpu: usize) -> u8;
    fn on_enqueue(&mut self, task: &Task, rq: &RunQueue);
    fn on_dequeue(&mut self, task: &Task, rq: &RunQueue);
    fn should_preempt(&self, current: &Task, ready: &Task) -> bool;
    fn estimate_runtime(&self, task: &Task) -> u64; // in nanoseconds
}
```

Políticas não devem:
- Bloquear por muito tempo
- Fazer allocações dinâmicas (alocar no init)
- Violar capacidades de tarefas

### Per-CPU Runqueues

Cada CPU tem sua fila de execução local para minimizar contenção:

```rust
struct RunQueue {
    priority_bitmap: AtomicU64,  // Fast lookup da prioridade máxima
    buckets: [LinkedList<Task>; 256],  // Uma lista por nível de prioridade
    load: AtomicU32,  // Carga aproximada (# tarefas prontas)
    last_balanced: AtomicU64,  // Timestamp do último balanceamento
}
```

Vantagens:
- Lock-free para operações comuns
- Cache locality para contexto de tarefa
- Escalável para muitos cores

### Global Balancer

Executa periodicamente (a cada N ticks ou evento de desbalanceamento):

```rust
struct GlobalBalancer {
    cpus: Vec<CpuState>,
    migration_queue: LockFreeQueue<MigrationHint>,
    hotspot_cache: HashMap<TaskId, CpuAffinity>,
}

struct CpuState {
    id: usize,
    runqueue: &'static RunQueue,
    load: u32,
    last_task: Option<TaskId>,
}
```

Operações:
- **Load Sampling:** Amostrar carga de cada CPU
- **Migration Planning:** Selecionar candidatos para migração
- **Affinity Enforcement:** Validar constraints de afinidade
- **Predictive Hints:** Consultar Predictive Engine para otimizações

## 5. Estruturas de Dados Principais

### Task Control Block (TCB)

Cada tarefa tem um bloco de controle com seu estado:

```rust
pub struct Task {
    id: TaskId,
    state: AtomicU8,  // Ready, Running, Blocked, Terminated
    priority: AtomicU8,  // 0-255 (0 = mais alto)
    cpu_affinity: AtomicU64,  // Máscara de CPUs permitidas
    
    // Context para switching
    context: TaskContext,
    
    // Accounting
    runtime_ns: AtomicU64,
    switches_count: AtomicU64,
    last_scheduled: AtomicU64,
    
    // IPC & sincronização
    pending_messages: LockFreeQueue<Message>,
    wait_channel: Option<ChannelId>,
    
    // Capacidades de segurança
    capabilities: CapabilitySet,
    
    // Metadata para Predictive Engine
    predicted_runtime: Option<u64>,
    access_pattern: AccessPattern,
}

pub enum TaskState {
    Ready = 0x00,
    Running = 0x01,
    Blocked = 0x02,
    Terminated = 0x03,
}
```

### TaskContext

Contexto que deve ser salvado/restaurado em switches:

```rust
pub struct TaskContext {
    // Registradores (arquitetura x86_64)
    rax: u64, rbx: u64, rcx: u64, rdx: u64,
    rdi: u64, rsi: u64, rbp: u64, rsp: u64,
    r8: u64, r9: u64, r10: u64, r11: u64,
    r12: u64, r13: u64, r14: u64, r15: u64,
    
    rip: u64,  // Instruction pointer
    rflags: u64,  // Flags register
    
    // Stack pointer e limite
    stack_bottom: *mut u8,
    stack_top: *mut u8,
    
    // FPU state (quando aplicável)
    fpu_state: Option<FpuState>,
}
```

### Message Queue

Para integração com IPC Core:

```rust
pub struct MessageQueue {
    messages: LockFreeQueue<Message>,
    max_size: usize,
    waiting_task: Option<TaskId>,
}

pub struct Message {
    sender_id: TaskId,
    receiver_id: TaskId,
    payload: &'static [u8],
    priority: u8,  // Mensagens críticas primeiro
    timestamp: u64,
}
```

## 6. Fluxos de Execução

### Boot Inicial do Scheduler

```
1. Kernel init chama sched_initialize()
   ├─ Aloca RunQueues per-CPU
   ├─ Cria threads de idle para cada CPU
   ├─ Registra política padrão (desktop/server)
   └─ Habilita interrupts de timer

2. Primeiros threads de sistema são enfileirados
   ├─ Init process (PID 1)
   ├─ Supervisor de drivers
   └─ Serviços críticos

3. Scheduler começa a despachar (first dispatch)
   └─ CPU entra em loop: run task -> timer interrupt -> select next
```

### Fluxo de Context Switch

```
1. Timer interrupt (tick) ou yield syscall
   │
2. ISR salva contexto completo (CPU registers)
   │
3. sched_tick() é chamado
   │
4. Se task atual não é idle:
   │   ├─ Atualiza accounting (runtime, switches)
   │   └─ Enfileira novamente na RunQueue (se ready)
   │
5. Policy Layer escolhe próxima prioridade
   │   └─ callback: policy.choose_priority(rq)
   │
6. Scheduler Engine pop próxima task da RunQueue
   │
7. Se task == idle:
   │   └─ Entra em halt/pause (espera por interrupt)
   │   
8. Else:
   │   ├─ Valida afinidade da task
   │   ├─ Restaura contexto (CPU registers)
   │   ├─ Restaura espaço de endereço (page tables, se necessário)
   │   └─ Salta para RIP da task (execution resume)
   │
9. Task executa até próximo interrupt ou yield
```

Diagrama ASCII:

```
┌──────────────────────────────────────────────────────────────┐
│           CONTEXT SWITCH FLOW                                │
├──────────────────────────────────────────────────────────────┤
│                                                               │
│  Running Task         Timer Interrupt                        │
│       │                    │                                 │
│       └────────┬───────────┘                                 │
│               │                                              │
│        [ISR: Save Context]                                   │
│               │                                              │
│        [Update Accounting]                                   │
│               │                                              │
│     [Policy: choose_priority()] ← Predictive Engine hints   │
│               │                                              │
│    [Engine: pop_highest_ready()] ← RunQueue                 │
│               │                                              │
│        [Validate Affinity]                                   │
│               │                                              │
│  ┌────────────┴─────────────┐                               │
│  │                          │                               │
│ Idle?              Restore Context                          │
│  │                   & Resume                               │
│ Halt        ┌─────────────────┐                             │
│  │          │  Running Task   │                             │
│  │          │   (Next)        │                             │
│  └──────────┴─────────────────┘                             │
│                                                               │
└──────────────────────────────────────────────────────────────┘
```

### Fluxo de Balanceamento Global

Executa a cada ~100ms ou quando desbalanceamento é detectado:

```
1. GlobalBalancer.balance()
   │
2. Amostrar carga de cada RunQueue
   │   ├─ CPU0 load: 5 tarefas
   │   ├─ CPU1 load: 1 tarefa  ← desbalanceado
   │   └─ CPU2 load: 3 tarefas
   │
3. Selecionar CPUs candidatas para migração
   │   └─ Se (max_load - min_load) > threshold:
   │       └─ Migrar 1-2 tarefas de CPU mais carregada
   │
4. Para cada candidato de migração:
   │   ├─ Verificar afinidade (tarefa permite novo CPU?)
   │   ├─ Estimar custo de cache (Predictive Engine)
   │   └─ Se benefício > custo:
   │       └─ Enfileirar MigrationHint
   │
5. Aplicar migrações (lazy)
   │   └─ Quando tarefa for despachada, mudar RunQueue
   │
6. Atualizar hotspot cache (próxima iteração)
```

### Fluxo de Integração com IPC

Quando uma tarefa bloqueia em `receive_message()`:

```
1. Task A chama receive_message(channel_id)
   │
2. Se channel vazio:
   │   ├─ Enfileira Task A em wait_list do canal
   │   ├─ Marca Task A como Blocked
   │   └─ Retira de RunQueue
   │
3. Próximo context switch acontece
   │   └─ Task A não é selecionada (está Blocked)
   │
4. Task B envia mensagem via send_message()
   │   ├─ Enfileira Message em Message Queue
   │   └─ Checa se há waiting task
   │
5. Se Task A estava esperando:
   │   ├─ Move Task A para Ready state
   │   ├─ Re-enfileira em RunQueue
   │   └─ Sinaliza preemption se prioridade > current
   │
6. Próximo tick, Task A é selecionada
   │   └─ receive_message() retorna com mensagem
```

## 7. Interfaces Públicas

### Syscalls de Escalonamento

```rust
/// Cede CPU voluntariamente
pub extern "C" fn sched_yield() -> i32;

/// Define afinidade de uma tarefa a CPUs específicas
pub extern "C" fn sched_setaffinity(
    tid: TaskId,
    cpu_mask: u64,  // Máscara de CPUs (bits 0-63)
) -> i32;

/// Obtém afinidade atual de uma tarefa
pub extern "C" fn sched_getaffinity(tid: TaskId, cpu_mask: &mut u64) -> i32;

/// Obtém ID da tarefa atual
pub extern "C" fn sched_current_task() -> TaskId;

/// Configura prioridade de uma tarefa (apenas admin/kernel)
pub extern "C" fn sched_setpriority(tid: TaskId, priority: u8) -> i32;

/// Obtém prioridade de uma tarefa
pub extern "C" fn sched_getpriority(tid: TaskId) -> u8;

/// Aguarda término de uma tarefa
pub extern "C" fn sched_waitpid(tid: TaskId, status: &mut i32) -> i32;
```

### Funções Internas do Kernel

```rust
/// Inicializa o scheduler (chamado durante boot)
pub fn sched_initialize() -> Result<(), SchedError>;

/// Cria nova tarefa no scheduler
pub fn sched_create_task(
    entry: extern "C" fn() -> !,
    priority: u8,
    affinity: u64,
) -> Result<TaskId, SchedError>;

/// Marca tarefa para remoção (ao terminar)
pub fn sched_terminate_task(tid: TaskId) -> Result<(), SchedError>;

/// Bloqueia tarefa em recurso (usado por sync primitives)
pub fn sched_block_task(tid: TaskId, reason: BlockReason) -> Result<(), SchedError>;

/// Desbloqueia tarefa
pub fn sched_unblock_task(tid: TaskId) -> Result<(), SchedError>;

/// Registra uma nova política de escalonamento
pub fn sched_register_policy(
    policy: Box<dyn SchedulerPolicy>,
) -> Result<PolicyHandle, SchedError>;

/// Ativa uma política registrada
pub fn sched_activate_policy(handle: PolicyHandle) -> Result<(), SchedError>;

/// Hook chamado a cada tick do timer (por CPU)
pub fn sched_tick(cpu: usize);

/// Hook para context switch (antes de restaurar contexto)
pub fn sched_on_switch(prev_task: &Task, next_task: &Task);
```

### Policy Interface

```rust
pub trait SchedulerPolicy: Send + Sync {
    /// Escolhe nível de prioridade para próxima tarefa
    fn choose_priority(&self, rq: &RunQueue, cpu: usize) -> u8;
    
    /// Chamado quando tarefa é adicionada à RunQueue
    fn on_enqueue(&mut self, task: &Task, rq: &RunQueue);
    
    /// Chamado quando tarefa é removida da RunQueue
    fn on_dequeue(&mut self, task: &Task, rq: &RunQueue);
    
    /// Determina se próxima tarefa deve preemptar a atual
    fn should_preempt(&self, current: &Task, ready: &Task) -> bool;
    
    /// Estima tempo de execução (em nanoseconds)
    fn estimate_runtime(&self, task: &Task) -> u64;
    
    /// Tratamento de dica do Predictive Engine
    fn on_predictive_hint(&mut self, hint: &PredictiveHint);
    
    /// Retorna nome/versão da política (debug)
    fn name(&self) -> &'static str;
}

pub struct PredictiveHint {
    pub hint_type: HintType,
    pub priority_boost: Option<i8>,
    pub cpu_preference: Option<usize>,
    pub expected_runtime: Option<u64>,
}

pub enum HintType {
    LatencyCritical,
    HighThroughput,
    CacheWarm,
    CacheCold,
    DeadlineApproaching,
}
```

### Policy Examples

**Low-Latency Policy (Real-Time):**
```rust
pub struct LowLatencyPolicy {
    max_latency_ns: u64,
    preempt_threshold: u8,
}

impl SchedulerPolicy for LowLatencyPolicy {
    fn choose_priority(&self, rq: &RunQueue, _cpu: usize) -> u8 {
        // Sempre escolhe prioridade mais alta disponível
        rq.priority_bitmap.leading_zeros() as u8
    }
    
    fn should_preempt(&self, current: &Task, ready: &Task) -> bool {
        // Preempt se pronta tem prioridade significativamente maior
        ready.priority < current.priority - self.preempt_threshold
    }
}
```

**Throughput Policy (Server):**
```rust
pub struct ThroughputPolicy {
    fairness_window: u64,  // em nanoseconds
    quantum: u64,
}

impl SchedulerPolicy for ThroughputPolicy {
    fn choose_priority(&self, rq: &RunQueue, _cpu: usize) -> u8 {
        // Escolhe prioridade com tarefas mais antigas primeiro (fairness)
        // Ignora algumas interrupções para melhor throughput
        rq.get_oldest_nonempty_priority()
    }
    
    fn should_preempt(&self, _current: &Task, _ready: &Task) -> bool {
        // Não preempta em servidor (cooperativo/RR)
        false
    }
}
```

## 8. Integração com IPC Core

O scheduler interage intimamente com o IPC Core para operações de bloqueio e desbloqueio:

### Ciclo de vida de mensagem

```
Task A (sender)                  IPC Channel               Task B (receiver)
    │                                  │                        │
    ├─ send_message()                  │                        │
    │                                  │                        │
    ├─ Enfileira msg                   │                        │
    │                                  │                        │
    │                           Check waiting                   │
    │                                  │                        │
    │                          Is Task B blocked?               │
    │                                  ├──────────→ sched_unblock_task()
    │                                  │                 │
    │                                  │           Re-enfileira em RunQueue
    │                                  │                 │
    │                                  │           Marca como Ready
    │                                  │                 │
    │                                  │           ← return (woken up)
    │                                  │
    └─ return to Task A (continue)
```

### Bloqueio em Recepção

Quando `receive_message()` é chamado e canal está vazio:

```rust
pub fn receive_message(channel_id: ChannelId) -> Result<Message, Error> {
    let channel = get_channel(channel_id)?;
    
    // Tenta receber sem bloquear
    if let Some(msg) = channel.try_recv() {
        return Ok(msg);
    }
    
    // Canal vazio, bloqueia
    let current_task = sched_current_task();
    channel.add_waiter(current_task);
    
    // Sinaliza scheduler para bloquear this task
    sched_block_task(current_task, BlockReason::ReceiveMessage)?;
    
    // Syscall retorna; scheduler não mais selecionará esta tarefa
    // Quando mensagem chegar, task será desbloqueada
}
```

### Desbloqueio via Envio

Quando `send_message()` encontra tarefa esperando:

```rust
pub fn send_message(
    channel_id: ChannelId,
    payload: &[u8],
) -> Result<(), Error> {
    let channel = get_channel(channel_id)?;
    
    // Enfileira mensagem
    channel.enqueue(Message {
        sender: sched_current_task(),
        payload: payload.to_owned(),
    })?;
    
    // Verifica se há tarefa aguardando
    if let Some(waiting_task) = channel.pop_waiter() {
        // Desbloquia a tarefa
        sched_unblock_task(waiting_task)?;
        
        // Se prioridade de waiting_task > current, pode preemptar
        let current = sched_current_task();
        if needs_preemption(current, waiting_task) {
            sched_yield();  // Cooperativo
        }
    }
    
    Ok(())
}
```

## 9. Segurança e Isolamento

### Validação de Políticas

Políticas carregadas dinamicamente devem ser validadas:

```rust
pub fn validate_policy(policy: &dyn SchedulerPolicy) -> Result<(), PolicyError> {
    // Checklist de validação
    
    // 1. Timeout checks (não deve bloquear por tempo indeterminado)
    let start = Timestamp::now();
    let priority = policy.choose_priority(&dummy_rq, 0);
    let elapsed = Timestamp::now() - start;
    if elapsed > 100_us {  // Timeout de 100 microssegundos
        return Err(PolicyError::TooSlow);
    }
    
    // 2. Signature verification (se assinada)
    // (futuro: verificação criptográfica)
    
    // 3. Capability checks
    // Políticas só podem acessar dados públicos (não privados de tarefas)
    
    Ok(())
}
```

### Isolamento de Contexto

Cada tarefa executa em contexto isolado:

```
Task A Memory Space       Task B Memory Space       Task C Memory Space
┌──────────────────┐    ┌──────────────────┐      ┌──────────────────┐
│                  │    │                  │      │                  │
│  Stack           │    │  Stack           │      │  Stack           │
│  Heap            │    │  Heap            │      │  Heap            │
│  Code            │    │  Code            │      │  Code            │
│  Private Data    │    │  Private Data    │      │  Private Data    │
│                  │    │                  │      │                  │
└──────────────────┘    └──────────────────┘      └──────────────────┘
        │                       │                        │
        └───────────────┬───────┴────────────┬───────────┘
                        │                    │
              ┌─────────▼────────┐  ┌────────▼──────────┐
              │  Shared Memory   │  │  IPC Channels    │
              │  (Capabilities)  │  │  (Message Based) │
              └──────────────────┘  └──────────────────┘
```

### Proteção contra DoS

- **Timeout de políticas:** Políticas não podem executar indefinidamente
- **Limite de tarefas:** Máximo de tarefas por UID/domínio
- **CPU reservation:** Serviços críticos têm reserva mínima de CPU
- **Message rate limiting:** Taxa máxima de mensagens por canal

```rust
pub struct TaskQuota {
    pub uid: u32,
    pub max_tasks: usize,
    pub current_tasks: AtomicUsize,
    pub cpu_reservation_us: u64,  // Microsegundos por segundo
}

pub fn create_task_check(uid: u32) -> Result<(), QuotaError> {
    let quota = get_user_quota(uid)?;
    if quota.current_tasks.load(Ordering::Seq) >= quota.max_tasks {
        return Err(QuotaError::LimitExceeded);
    }
    Ok(())
}
```

## 10. Escalabilidade e Limites

### Escalabilidade Horizontal (Muitos Cores)

| Métrica | Limite | Notas |
|---------|--------|-------|
| CPUs | 4096 | Array per-CPU, separado em NUMA zones |
| Tasks por CPU | 10,000 | Lock-free queue, O(1) enqueue/dequeue |
| Policies | 16 | Reusáveis entre perfis |
| Ticks por segundo | 1000 Hz | Ajustável, default para servidor |
| Message latency | < 10μs | Entre tasks na mesma CPU |
| Context switch overhead | < 1μs | Sem copy, só register swap |

### Overhead de Memória

```
Por Task:
  TaskControlBlock: ~256 bytes
  Stack (mínimo): 4 KB
  Task ID: 4 bytes
  ────────────────────
  Total mínimo: ~4.3 KB

Exemplo: 10,000 tarefas
  = 10K × 4.3K = ~43 MB
```

### Preparação para NUMA

```rust
pub struct NumaNode {
    cpus: Vec<usize>,
    runqueues: Vec<&'static RunQueue>,
    local_memory: MemoryRegion,
}

pub struct GlobalBalancerNuma {
    nodes: Vec<NumaNode>,
    inter_node_migration_cost: u64,  // Penalidade de latência
}

// Preferência: tarefa fica no NUMA node onde foi criada
// Migração inter-node só se grande desbalanceamento
```

## 11. Futuras Evoluções

### Curto Prazo (Próximas versões 0.0.x)

- ✅ Integração completa com IPC Core
- ✅ Suporte a afinidade CPU dinâmica
- ⏳ Preemption (atualmente cooperativo)
- ⏳ Mais políticas built-in (Gaming, Battery-saving)
- ⏳ Tracing e profiling de scheduling

### Médio Prazo (v0.1+)

- ⏳ Integração com Predictive Engine (cache warming, thread displacement)
- ⏳ Suporte NUMA completo com migração automática
- ⏳ Políticas ML-based (machine learning)
- ⏳ Real-time guarantees com deadline scheduler
- ⏳ Energy-aware scheduling

### Longo Prazo (v1.0+)

- ⏳ Scheduler kernels assimétricos (big.LITTLE)
- ⏳ Adaptive clock scaling (DVFS) integrado
- ⏳ Verificação formal de propriedades de latência
- ⏳ Suporte a cluster scheduling (distribuído)

## 12. Comparação com sistemas modernos

- Linux CFS: similar em estimativas de load, mas Jinn separa mecanismo/política explicitamente e integra predição.
- Zircon: modelo de capacidades e IPC semelhante; Jinn foca adaptabilidade de políticas.

## 13. Pseudocódigo

Scheduler loop (simplificado):

```pseudo
on_tick(cpu):
  rq = cpu.runqueue
  if rq.has_ready():
    prio = policy.choose_priority(rq, cpu)
    thread = rq.pop(prio)
    context_switch_to(thread)
  else:
    idle()

periodic_balance():
  for cpu in cpus:
    if cpu.load > threshold:
       candidate = select_migration_candidate(cpu)
       migrate(candidate, target_cpu)

```

Adaptive Policy switching:

```pseudo
if predictive.hint == "latency_critical":
  activate_policy(low_latency_policy)
elif predictive.hint == "high_throughput":
  activate_policy(high_throughput_policy)

```

## 14. Diagramas ASCII

Topologia básica:

  CPU0 RunQ --\
  CPU1 RunQ ---- GlobalBalancer <-> PredictiveEngine
  CPU2 RunQ --/

Fluxo de escolha:

  [Interrupt] -> [Per-CPU Scheduler Engine] -> [Policy Layer] -> [Thread]

## Adaptive Policy Scheduler — Detalhes

- Engine: responsável por enfileirar, desempilhar e realizar context switch e migrações.
- Policy Layer: provê heurísticas (priority boosting, aging, bandwidth reservations, deadline enforcement).
- Perfis operacionais: coleções de parâmetros (latency target, throughput target, fairness) e módulos de decisão.

### CPU Affinity

- Afinidade por default é por processo; políticas podem ajustar afinidade para reduzir custo de cache.
- `sched_set_affinity` define máscara; migrator respeita preferências e limitações de política.

### Thread Migration

- Migração é realizada quando ganho estimado > custo (copy-queues, warm caches via Predictive Engine).
- Usar handshake com processos para re-localização de memória (NUMA hints).

### Priority Management

- Prioridades organizadas em classes + subprioridades.
- Bitmap por runqueue para seleção rápida da prioridade mais alta.

### QoS

- Reservoirs de CPU: políticas podem reservar fatia mínima de CPU para serviços críticos.
- Enforcement por contagem de tempo e throttling sob políticas.

### Latency Control

- Políticas de preemption agressiva para perfis de baixa latência.
- Deadline-aware scheduling para modos industriais (modo determinístico).

### Integration with Predictive Engine

- Predictive Engine fornece:
  - Hints de carga futura por serviço.
  - Riscos de hotspots de cache/memória.
  - Recomendações de política e afinidade.

Flow integration:

1. Predictive Engine analisa telemetria.
2. Emite hint (e.g., "short burst incoming on service X").
3. Scheduler ativa política de baixa latência temporária para threads de X.

---

## 12. Comparação com Sistemas Modernos — Detalhado

### Linux CFS vs Jinn Scheduler

| Aspecto | Linux CFS | Jinn Scheduler |
|---------|-----------|----------------|
| **Estrutura** | Red-Black Tree | Bitmap + per-CPU queues |
| **Complexidade** | O(log n) select | O(1) select |
| **Fairness** | Vruntime (embarcada) | Policy-based (desacoplada) |
| **Latência p50** | ~100μs | <10μs (alvo) |
| **Determinismo** | Baixo | Alto (com real-time policy) |
| **Plugabilidade** | Limitada | Completa (policy trait) |
| **NUMA** | Sim, complexo | Sim, simples (futuro) |
| **Preemption** | Sim | Sim (futuro) |
| **Cores suportados** | 4096+ | 4096+ (preparado) |

**Diferenças-chave:**
- Jinn explicitamente separa mecanismo/política; CFS embute fairness
- CFS otimizado para desktop/servidor genérico
- Jinn otimizado para adaptabilidade e previsibilidade

### seL4 Microkernel vs Jinn

| Aspecto | seL4 | Jinn |
|---------|------|------|
| **Verificação Formal** | Sim (Hoare logic) | Em progresso |
| **Tamanho de kernel** | ~10k LOC | ~5k LOC (alvo) |
| **Linguagem** | C | Rust |
| **IPC** | Síncrono | Assíncrono |
| **Scheduling** | Fixed priority + RR | Adaptável |
| **Performance** | Latência ultra-low | Latência + throughput |

**Vantagens Jinn:**
- Memory safety via Rust
- Assincronismo nativo (melhor throughput)
- Políticas adaptáveis

**Vantagens seL4:**
- Verificação formal completa
- Track record industrial
- IPC síncrono para certos casos de uso

### Zircon (Fuchsia) vs Jinn

| Aspecto | Zircon | Jinn |
|---------|--------|------|
| **Linguagem** | C++ | Rust |
| **Kernel LOC** | ~40k | ~5k (alvo) |
| **Capabilities** | Sim (complexo) | Sim (simples, evolvable) |
| **Scheduler** | Priority levels | Policy-based |
| **Predictive** | Não | Sim (futuro) |
| **Open Source** | Sim | Sim |
| **Maturidade** | ~8 anos | 0 anos |

### QNX Neutrino vs Jinn

| Aspecto | QNX | Jinn |
|---------|-----|------|
| **Maturidade** | 30+ anos | Experimental |
| **Certified Real-Time** | Sim | Targeting |
| **License** | Proprietário | Open Source |
| **POSIX Compliant** | Sim | Parcial (futuro) |
| **Plugins** | Limitados | Rich |
| **Performance** | Excelente | Promising |

---

## 13. Pseudocódigo Avançado

### Algorithm: Context Switch com Segurança

```pseudo
function context_switch_to(next_task):
    // Save current task context (x86_64)
    current_task = CPU.current_task
    if current_task != null:
        assert(current_task.state == Running)
        save_registers(current_task.context)
        save_stack_pointers(current_task.context)
        save_page_tables(current_task.context)
    
    // Validações de segurança
    assert(next_task.state != Running)
    assert(validate_affinity(next_task, CPU.id))
    assert(check_capabilities(next_task, current_user))
    
    // Update accounting
    if current_task != null:
        current_task.total_runtime += (now() - current_task.last_scheduled)
        current_task.switch_count++
    
    next_task.state = Running
    next_task.last_scheduled = now()
    CPU.current_task = next_task
    
    // Restore next task context
    restore_registers(next_task.context)
    restore_stack_pointers(next_task.context)
    restore_page_tables(next_task.context)
    
    // Jump to next instruction pointer
    jump_to(next_task.context.rip)
```

### Algorithm: Scheduler Tick Completo

```pseudo
function sched_tick(cpu_id):
    rq = CPU[cpu_id].runqueue
    current_task = CPU[cpu_id].current_task
    now = get_timestamp()
    
    // Update current task stats
    if current_task.state == Running:
        time_used = now - current_task.last_tick_time
        current_task.runtime_this_quantum += time_used
        current_task.total_runtime += time_used
        
        // Check if time quantum expired
        if current_task.runtime_this_quantum >= policy.time_quantum():
            current_task.state = Ready
            rq.enqueue(current_task)
            
            // Select next task
            priority = policy.choose_priority(rq, cpu_id)
            next_task = rq.dequeue_at_priority(priority)
            
            if next_task == null:
                next_task = idle_task
            
            context_switch_to(next_task)
        
        // Check if higher priority task arrived
        elif rq.highest_priority() < current_task.priority:
            should_preempt = policy.should_preempt(current_task, rq.peek())
            if should_preempt:
                current_task.state = Ready
                rq.enqueue(current_task)
                
                priority = policy.choose_priority(rq, cpu_id)
                next_task = rq.dequeue_at_priority(priority)
                context_switch_to(next_task)
    
    // Periodic global balancing
    if tick_count % BALANCE_INTERVAL == 0:
        for each migration_hint in global_balancer.pending_migrations():
            if can_migrate(migration_hint.task, migration_hint.target_cpu):
                apply_migration(migration_hint)
        
        // Re-sample loads and plan new migrations
        loads = []
        for each cpu in CPUs:
            loads.append((cpu.id, cpu.runqueue.get_load()))
        
        if is_imbalanced(loads):
            new_hints = global_balancer.plan_migrations(loads)
            for each hint in new_hints:
                // Consult Predictive Engine for cache cost
                cost = predictive_engine.estimate_migration_cost(hint)
                if cost < hint.benefit_estimate():
                    global_balancer.queue_migration(hint)
```

### Algorithm: Global Load Balancing

```pseudo
function global_balancer.balance():
    loads = []
    
    // Phase 1: Sample load from each CPU
    for each cpu in CPUs:
        load = cpu.runqueue.approximate_load()  // O(1) sample
        loads.append((cpu.id, load))
    
    // Phase 2: Identify imbalance
    max_load = max(loads[*].load)
    min_load = min(loads[*].load)
    imbalance = max_load - min_load
    
    if imbalance <= IMBALANCE_THRESHOLD:
        return  // System is balanced
    
    // Phase 3: Plan migrations
    migrations = []
    overloaded_cpus = sort_desc_by_load(loads)
    underloaded_cpus = sort_asc_by_load(loads)
    
    for each (cpu_id, load) in overloaded_cpus:
        while load > (max_load + min_load) / 2:
            // Find best candidate for migration
            candidate = select_migratable_task(cpu_id, policy)
            if candidate == null:
                break
            
            // Find best destination
            for each (dest_cpu, dest_load) in underloaded_cpus:
                if candidate.cpu_affinity & (1 << dest_cpu):
                    if dest_load < load:
                        // Check cache cost
                        cost = estimate_cache_migration_cost(candidate, dest_cpu)
                        benefit = load - dest_load
                        
                        if benefit > cost * COST_FACTOR:
                            migrations.append((candidate.id, dest_cpu, cost))
                            load--
                            break
    
    // Phase 4: Enqueue migrations (applied lazily)
    for each (task_id, target_cpu, cost) in migrations:
        migration_queue.enqueue(MigrationHint {
            task_id: task_id,
            target_cpu: target_cpu,
            cost_estimate: cost,
            timestamp: now(),
        })
```

### Algorithm: IPC Wake-up with Priority

```pseudo
function send_message(channel_id, payload):
    channel = get_channel(channel_id)
    current_task = sched_current_task()
    
    // Enqueue message
    msg = Message {
        sender: current_task.id,
        receiver: channel.receiver_hint,
        payload: payload,
        priority: payload.priority,  // optional
        timestamp: now(),
    }
    
    if not channel.try_enqueue(msg):
        return Error::ChannelFull
    
    // Check for waiting task
    waiting_task_id = channel.pop_waiter()
    if waiting_task_id == null:
        return Ok()
    
    // Wake up the task
    waiting_task = get_task(waiting_task_id)
    sched_unblock_task(waiting_task_id)
    
    // Update task state
    waiting_task.state = Ready
    waiting_task.pending_messages.push(msg)
    
    // Re-enqueue in target CPU runqueue
    target_rq = waiting_task.cpu_affinity_rq()
    target_rq.enqueue(waiting_task)
    
    // Check if preemption needed
    if waiting_task.priority < current_task.priority:
        if policy.should_preempt(current_task, waiting_task):
            // Signal IPI (Inter-Processor Interrupt) if on different CPU
            if waiting_task.cpu != CPU.id:
                send_ipi(waiting_task.cpu, RESCHEDULE_VECTOR)
    
    return Ok()
```

---

## 14. Diagramas ASCII Avançados

### State Machine de Tarefa Completo

```
                ┌───────────────┐
                │    Created    │
                │  (initializing)│
                └────────┬──────┘
                         │
                 sched_create_task()
                         │
                    ┌────▼──────┐
                    │   Ready   │  (enqueued, waiting for CPU)
                    └────┬──────┘
                         │     ◄─────────┐
                         │              │
                   schedule()    sched_unblock_task()
                         │        (from Blocked)
                         │              │
                    ┌────▼──────────────┘
                    │
                    ▼
            ┌──────────────┐
            │   Running    │  (executing on CPU)
            └──────┬───────┘
                   │
        ┌──────────┼──────────┐
        │          │          │
    Yield/Timer  Block     Exit
        │          │          │
    Back to   ┌───▼────┐   ┌─▼──────────┐
    Ready │   Blocked  │   │ Terminated │
        │   (waiting   │   │ (cleanup)  │
        └──► for IPC)  │   └────────────┘
            │          │
            │   Woken by IPC
            │   send_message()
            │          │
            └──────┬───┘
                   │
            Re-enqueued Ready
```

### Multi-CPU Scheduler Architecture

```
    ┌────────────────────────────────────────────────────────────┐
    │            Predictive Engine                               │
    │  (telemetry, load forecasts, cache insights)              │
    └────────────────────┬─────────────────────────────────────┘
                         │ hints
                         ▼
    ┌────────────────────────────────────────────────────────────┐
    │         Global Balancer                                   │
    │  • Load sampling every 100ms                              │
    │  • Migration planning                                      │
    │  • Affinity enforcement                                    │
    │  • NUMA awareness (future)                               │
    └────┬──────────────┬──────────────┬──────────────┬──────────┘
         │              │              │              │
         │              │              │              │
    ┌────▼────┐   ┌────▼────┐   ┌────▼────┐   ┌────▼────┐
    │ CPU 0   │   │ CPU 1   │   │ CPU 2   │   │ CPU 3   │
    │ RunQ    │   │ RunQ    │   │ RunQ    │   │ RunQ    │
    │ ┌─────┐ │   │ ┌─────┐ │   │ ┌─────┐ │   │ ┌─────┐ │
    │ │Task1│ │   │ │Task5│ │   │ │Task2│ │   │ │Task7│ │
    │ │Task3│ │   │ │Task6│ │   │ │Task4│ │   │ │Task8│ │
    │ │Idle │ │   │ │Idle │ │   │ │Idle │ │   │ │Idle │ │
    │ └─────┘ │   │ └─────┘ │   │ └─────┘ │   │ └─────┘ │
    │ Policy  │   │ Policy  │   │ Policy  │   │ Policy  │
    │ Engine  │   │ Engine  │   │ Engine  │   │ Engine  │
    └────┬────┘   └────┬────┘   └────┬────┘   └────┬────┘
         │              │              │              │
         └──────────────┴──────────────┴──────────────┘
                        │
              Context switching, load balancing
```

### IPC Message Flow with Scheduler

```
Task A (CPU0)            IPC Channel             Task B (CPU1)
  │                          │                       │
  ├─ send_message()          │                       │
  │    ├─ enqueue msg ───────┤                       │
  │    │                     │                       │
  │    └─ check_waiter()     │                       │
  │                          │                       │
  │           ┌──────────────►│ pop_waiter()        │
  │           │              │                       │
  │           │              ├─ was_blocked (Ready)→ B (Blocked)
  │           │              │                       │
  │           │         sched_unblock_task()         │
  │           │              │                       │
  │           │         ┌─────────────────┐         │
  │           │         │ Re-enqueue in   │         │
  │           │         │ RunQueue(CPU1)  │         │
  │           │         └────────┬────────┘         │
  │           │                  │                  │
  │           │                  └──────────────────┤──► Now Ready
  │           │                                     │
  │           └─ Check preemption (cross-core IPI) │
  │                (if CPU1 should reschedule)     │
  │                                                │
  └─────────────────────────────────────────────────┤
          return to Task A                          │
                                          (Next tick)
                                                    ▼
                                         Task B selected
                                         receive_message() ret
```

---

## 15. Considerações para Implementação em Rust

### Memory Safety Patterns

```rust
// ❌ INSEGURO: Raw pointers sem bounds
struct TaskContext {
    stack_ptr: *mut u8,  // Onde termina?
}

// ✅ SEGURO: Owned heap allocation
struct TaskStack {
    buffer: Box<[u8; STACK_SIZE]>,
}

struct Task {
    stack: TaskStack,  // Lifetime explícito
    id: TaskId,
    state: AtomicU8,
}

impl Task {
    fn stack_ptr(&self) -> *mut u8 {
        self.stack.buffer.as_mut_ptr()
    }
}
```

### Lock-Free Runqueue

```rust
use parking_lot::SpinLock;

pub struct RunQueue {
    // Bitmap for O(1) priority lookup
    priority_bitmap: AtomicU64,
    
    // Lock-free linked lists per priority
    buckets: [LockFreeLinkedList<Task>; 256],
    
    size: AtomicUsize,
}

impl RunQueue {
    pub fn enqueue(&self, task: &Task) {
        let prio = task.priority.load(Ordering::Acquire) as usize;
        
        // Add to linked list (lock-free)
        self.buckets[prio].push_front(task);
        
        // Atomically update bitmap
        self.priority_bitmap.fetch_or(
            1u64 << (prio % 64),
            Ordering::Release,
        );
        
        self.size.fetch_add(1, Ordering::Release);
    }
    
    pub fn dequeue_highest(&self) -> Option<&Task> {
        let bitmap = self.priority_bitmap.load(Ordering::Acquire);
        
        if bitmap == 0 {
            return None;
        }
        
        // Find highest set bit
        let prio = 63 - bitmap.leading_zeros() as usize;
        
        // Try to dequeue from that priority
        if let Some(task) = self.buckets[prio].pop_front() {
            
            // Update size
            self.size.fetch_sub(1, Ordering::Release);
            
            // Check if priority bucket is now empty
            if self.buckets[prio].is_empty() {
                self.priority_bitmap.fetch_and(
                    !(1u64 << (prio % 64)),
                    Ordering::Release,
                );
            }
            
            return Some(task);
        }
        
        None
    }
}
```

### Unsafe Block Justification

```rust
/// Context switch assembly (NECESSARY unsafe)
///
/// SAFETY: This function switches CPU context between two tasks.
/// Preconditions:
/// - Both TaskContext pointers must be valid and properly aligned
/// - TaskContext must contain properly initialized CPU registers
/// - Must be called with interrupts disabled
/// - Caller must ensure stack switching is valid
#[naked]
pub unsafe extern "C" fn context_switch_asm(
    prev_ctx: *mut TaskContext,
    next_ctx: *const TaskContext,
) {
    core::arch::asm!(
        // Save current registers to prev_ctx
        "mov [rdi + {off_rax}], rax",
        "mov [rdi + {off_rbx}], rbx",
        // ... save all 16 general-purpose registers
        
        // Restore from next_ctx
        "mov rax, [rsi + {off_rax}]",
        "mov rbx, [rsi + {off_rbx}]",
        // ... restore all 16 general-purpose registers
        
        // Jump to next instruction pointer
        "jmp [rsi + {off_rip}]",
        
        off_rax = const(offset_of!(TaskContext, rax)),
        off_rbx = const(offset_of!(TaskContext, rbx)),
        // ... other offsets
        off_rip = const(offset_of!(TaskContext, rip)),
        
        in("rdi") prev_ctx,
        in("rsi") next_ctx,
    );
}

// JUSTIFICATION: Impossible to implement in safe Rust because:
// 1. Requires direct CPU register manipulation
// 2. Requires jumping to arbitrary instruction addresses
// 3. Requires stack switching without intermediate storage
// 4. Must preserve CPU state exactly (no Rust abstractions)
//
// MITIGATION: 
// - Well-defined invariants documented above
// - Unit tests verify correct register save/restore
// - Fuzzing tests with random TaskContext values
// - Formal verification planned (phase 3)
```

### Testing Strategy

```rust
#[cfg(test)]
mod scheduler_tests {
    use super::*;
    
    #[test]
    fn test_runqueue_fifo() {
        let rq = create_test_runqueue();
        let t1 = create_task(Priority::Normal);
        let t2 = create_task(Priority::Normal);
        let t3 = create_task(Priority::Normal);
        
        rq.enqueue(&t1);
        rq.enqueue(&t2);
        rq.enqueue(&t3);
        
        assert_eq!(rq.dequeue_highest().id, t1.id);
        assert_eq!(rq.dequeue_highest().id, t2.id);
        assert_eq!(rq.dequeue_highest().id, t3.id);
    }
    
    #[test]
    fn test_runqueue_priority() {
        let rq = create_test_runqueue();
        let low = create_task(Priority::Low);   // 255
        let high = create_task(Priority::High); // 0
        let med = create_task(Priority::Normal); // 128
        
        rq.enqueue(&low);
        rq.enqueue(&med);
        rq.enqueue(&high);
        
        assert_eq!(rq.dequeue_highest().priority, 0);   // High first
        assert_eq!(rq.dequeue_highest().priority, 128); // Then normal
        assert_eq!(rq.dequeue_highest().priority, 255); // Then low
    }
    
    #[test]
    fn test_context_switch_latency() {
        let task1 = create_task(Priority::Normal);
        let task2 = create_task(Priority::Normal);
        
        let start = measure_rdtsc();
        context_switch_to(&task1, &task2);
        let latency_tsc = measure_rdtsc() - start;
        
        let latency_ns = tsc_to_ns(latency_tsc);
        assert!(latency_ns < 1_000,  // < 1 microsecond
                "Context switch took {:?}ns", latency_ns);
    }
    
    #[test]
    fn test_ipc_wake_up_priority() {
        let channel = create_channel();
        let sender = create_task(Priority::High);
        let receiver = create_task(Priority::Low);
        
        // Receiver blocks
        sched_block_task(receiver.id).unwrap();
        
        // Sender sends message
        let start = measure_rdtsc();
        send_message(&channel, b"hello").unwrap();
        let wake_latency = measure_rdtsc() - start;
        
        // Receiver should be ready
        assert_eq!(receiver.state, TaskState::Ready);
        assert!(wake_latency < 10_000, // <10 microseconds
                "Wake-up took {:?} TSC cycles", wake_latency);
    }
}
```

---

## 16. Roadmap de Implementação

### Fase 0 (v0.0.1 — Atual)
**Status: ✅ Concluída**
- [x] Scheduler cooperativo básico
- [x] Round-robin entre tarefas
- [x] Yield syscall
- [x] Documentação completa (este arquivo)
- [ ] Testes unitários (próximo)

### Fase 1 (v0.0.2 — Próximas 2 semanas)
**Objetivo: Integração com IPC**
- [ ] Blocking em receive_message()
- [ ] Unblocking via send_message()
- [ ] Per-CPU runqueues (multi-core)
- [ ] Basic load balancer
- [ ] Integration tests com IPC

### Fase 2 (v0.1 — 1-2 meses)
**Objetivo: Multi-policy scheduler**
- [ ] Preemptive scheduling
- [ ] Three built-in policies:
  - Low-latency (real-time)
  - Throughput (server)
  - Desktop (balanced)
- [ ] CPU affinity enforcement
- [ ] Task migration
- [ ] Performance benchmarks

### Fase 3 (v0.2 — 2-3 meses)
**Objetivo: Predictive integration**
- [ ] Predictive Engine hookups
- [ ] Cache-aware migration
- [ ] ML-based policy hints
- [ ] NUMA support (basic)

### Fase 4 (v1.0 — 6+ months)
**Objetivo: Production-ready**
- [ ] Formal verification (latency bounds)
- [ ] Extended real-time support
- [ ] Energy-aware scheduling
- [ ] Full NUMA support
- [ ] Security hardening

---

## 17. Checklist de Revisão

Before merging scheduler changes:

- [ ] All tests pass (unit + integration)
- [ ] Latency benchmarks < 10μs (context switch)
- [ ] Load balancing working (verified with sys-load test)
- [ ] IPC integration stable (message passing works)
- [ ] Documentation updated
- [ ] Code review approved
- [ ] No unsafe code outside justification list
- [ ] Memory sanitizers clean

---

**Document Version:** 1.0  
**Last Updated:** 2026-08-17  
**Status:** 📋 Complete & Ready for Implementation  
**Next Review:** 2026-09-01

## Considerações para implementação em Rust

- Usar `unsafe` apenas no mínimo: abstrair runqueues com tipos seguros.
- `Atomic*` e `spin`/`parking_lot` para sincronização de baixo nível.
- Interfaces FFI para políticas (drivers de política) devem usar boundary-safe APIs e validação de entrada.
- Favor `no_std` para código do kernel e dividir em crates: `jinn-sched-engine`, `jinn-sched-policy`, `jinn-sched-common`.

---

Arquivo: [Scheduler Core](./scheduler-core.md)

Arquitetura

O Núcleo do Agendador consiste em três camadas principais:

Mecanismo de Agendamento: Esta camada lida com o agendamento de processos, alocando e desalocando recursos conforme necessário.

Política de Agendamento: Esta camada define a política de agendamento específica a ser usada (por exemplo, Primeiro a Chegar, Primeiro a Ser Servido, Agendamento por Prioridade, etc.). O Núcleo do Agendador recebe as decisões de agendamento desta camada e as executa.

Interface do Motor Preditivo: Esta camada integra-se ao Motor Preditivo, recebendo resultados de modelagem preditiva e ajustando as decisões de agendamento de acordo.

Fluxo de Operação

O Motor Preditivo fornece um conjunto de modelos preditivos, que são usados ​​para determinar a decisão de agendamento ideal para cada processo.

A camada de Política de Agendamento define a política de agendamento específica a ser usada, com base em fatores como prioridade do processo, uso da CPU, consumo de memória, etc.

A camada de Mecanismo de Agendamento recebe as decisões de agendamento da camada de Política de Agendamento e as executa, alocando ou desalocando recursos do sistema conforme necessário.

O Núcleo do Agendador monitora o desempenho do processo e ajusta seu estado interno de acordo.

Principais Estruturas de Dados

Fila de Processos: Uma estrutura de dados que mantém o controle dos processos em execução, com cada entrada contendo informações relevantes, como ID do processo, prioridade e uso de recursos.

Decisão de Agendamento: Uma estrutura que representa a decisão de agendamento tomada pela camada de Política de Agendamento, incluindo o processo específico a ser agendado em seguida.

Interfaces Públicas

O Núcleo do Agendador fornece as seguintes interfaces públicas:

schedule_process(): Agenda um processo com base na política de agendamento atual.

get_current_scheduled_process(): Retorna o processo atualmente agendado.

update_predictive_model(): Atualiza o modelo preditivo usado pelo Mecanismo Preditivo.

Integração com outros componentes do Jinn

O Núcleo do Agendador integra-se com os seguintes componentes:

Mecanismo Preditivo: Recebe resultados de modelagem preditiva e ajusta as decisões de agendamento de acordo.

Supervisor de Processos: Fornece informações sobre os processos em execução, permitindo decisões de agendamento mais informadas.

Núcleo de Memória: Gerencia a alocação e desalocação de memória para todo o sistema.

Considerações de Segurança

O Núcleo do Agendador deve garantir que as informações confidenciais sejam protegidas e que os processos sejam devidamente isolados uns dos outros. Ele consegue isso por meio de:

Aplicação de níveis de privilégio e permissões de processo.

Monitoramento do uso de recursos do processo para evitar o consumo excessivo de recursos do sistema.

Colaboração com o Mecanismo Preditivo para otimizar as decisões de agendamento com base em considerações de segurança.

Implementações Futuras

Possíveis implementações futuras para o Núcleo do Agendador incluem:

Suporte para processos hierárquicos (ou seja, relações pai-filho).

Integração com um coletor de lixo para gerenciar o uso de memória de forma eficiente.

Modelagem preditiva aprimorada para decisões de agendamento mais precisas.

Exemplo de Fluxo de Execução

Suponha que temos três processos de nível de usuário, A, B e C, em execução simultânea. O fluxo de operação do Núcleo de Agendamento pode ser semelhante a este:

O Mecanismo Preditivo fornece um conjunto de modelos preditivos, indicando que A tem a maior prioridade.

A camada de Política de Agendamento define uma política de agendamento com base na prioridade do processo, alocando recursos de CPU para A.

A camada de Mecanismo de Agendamento agenda A como o próximo processo a ser executado, alocando memória e outros recursos conforme necessário.

À medida que os processos B e C ficam disponíveis, o Núcleo de Agendamento ajusta seu estado interno e os reagenda com base em sua prioridade.

Ao gerenciar cuidadosamente os recursos do sistema e integrar-se ao Mecanismo Preditivo, o Núcleo de Agendamento garante que o Jinn OS seja executado de forma eficiente, previsível e segura em uma ampla gama de cenários.