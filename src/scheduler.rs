// SAFETY: Este arquivo contém blocos `unsafe` necessários para manipulação
// de ponteiros de pilha e registradores de hardware.
#![allow(dead_code)]

/// Jinn OS — Escalonador Preemptivo de Tarefas
///
/// Implementa um escalonador preemptivo com as seguintes características:
///
/// - **Round-Robin por Prioridade:** RealTime > Normal > Idle
/// - **Quantum Dinâmico:** RealTime = 4 ticks, Normal = 2 ticks, Idle = 1 tick
/// - **Contabilização de CPU:** Cada tarefa rastreia seus ticks consumidos
///   e o número de trocas de contexto (context switches).
/// - **Direct Thread Switch:** Transferência direta de contexto entre tarefas
///   para chamadas IPC síncronas (Fast-Path), sem passar pela fila de prontos.
/// - **Liberação completa de memória:** Quando uma tarefa Ring 3 termina, seu
///   PML4 e todas as páginas de usuário são liberadas automaticamente.
/// - **Tarefa Idle dedicada:** Uma tarefa com prioridade `Idle` que executa
///   `hlt` quando nenhuma outra tarefa está pronta, economizando energia.

use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use crate::vga::Writer;
use crate::gdt;
use crate::memory::paging;

/// Contador global de ticks do escalonador (incrementado a cada preempção).
static SCHED_TICKS: AtomicU64 = AtomicU64::new(0);

/// Contador global de trocas de contexto efetivas.
static CONTEXT_SWITCHES: AtomicU64 = AtomicU64::new(0);

// ── Constantes ────────────────────────────────────────────────────────────────

/// Número máximo de tarefas simultâneas no sistema.
pub const MAX_TASKS: usize = 16;

/// Tamanho da pilha de kernel de cada tarefa (8 KiB — ring 0).
pub const KSTACK_SIZE: usize = 8192;

/// Tamanho da pilha de usuário de cada tarefa (64 KiB — ring 3).
pub const USTACK_SIZE: usize = 65536;

/// Endereço virtual base das pilhas de usuário.
/// Cada tarefa recebe: USER_STACK_BASE - (id * USTACK_SIZE).
const USER_STACK_BASE: u64 = 0x0000_7FFF_F000_0000;

/// Quantum (em ticks) atribuído a cada nível de prioridade.
/// Tarefas RealTime recebem mais tempo contínuo de CPU.
const QUANTUM_REALTIME: u64 = 4;
const QUANTUM_NORMAL:   u64 = 2;
const QUANTUM_IDLE:     u64 = 1;

// ── Estado global ─────────────────────────────────────────────────────────────

static CURRENT_TASK: AtomicI32 = AtomicI32::new(-1);
static YIELDED:      AtomicBool = AtomicBool::new(false);

/// Retorna o PID (índice) da tarefa atualmente em execução.
pub fn current_pid() -> usize {
    let idx = CURRENT_TASK.load(Ordering::SeqCst);
    if idx < 0 { 0 } else { idx as usize }
}

/// Cede voluntariamente o quantum de CPU restante para a próxima tarefa pronta.
pub fn yield_now() {
    YIELDED.store(true, Ordering::SeqCst);
    // Dispara o vetor 32 (APIC Timer) manualmente para forçar preempção.
    unsafe { core::arch::asm!("int 32"); }
}

#[allow(dead_code)]
pub fn current_task_index() -> i32 {
    CURRENT_TASK.load(Ordering::SeqCst)
}

/// Retorna o número total de preempções realizadas pelo escalonador.
pub fn sched_ticks() -> u64 {
    SCHED_TICKS.load(Ordering::Relaxed)
}

/// Retorna o número total de trocas de contexto efetivas.
pub fn context_switches() -> u64 {
    CONTEXT_SWITCHES.load(Ordering::Relaxed)
}

// ── Tipos de dados ────────────────────────────────────────────────────────────

/// Nível de privilégio de execução da tarefa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ring {
    /// Kernel: executa em Ring 0 com acesso total ao hardware.
    Kernel = 0,
    /// Usuário: executa em Ring 3, sem acesso direto ao hardware.
    User   = 3,
}

/// Estado atual de uma tarefa no escalonador.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    /// Pronta para executar, esperando sua vez.
    Ready,
    /// Atualmente executando na CPU.
    Running,
    /// Bloqueada esperando I/O ou IPC.
    Blocked,
    /// Bloqueada esperando outra tarefa terminar (waitpid).
    WaitingOn(usize),
    /// Finalizada mas não coletada (código de saída disponível).
    Zombie,
    /// Finalizada e coletada. O slot pode ser reutilizado.
    Completed,
}

/// Prioridade de escalonamento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    /// Só executa quando nenhuma outra tarefa está pronta.
    Idle     = 0,
    /// Prioridade padrão para processos de usuário.
    Normal   = 1,
    /// Prioridade elevada — nunca é preterida por tarefas Normal.
    RealTime = 2,
}

/// Representa uma tarefa (processo) no sistema.
pub struct Task {
    /// Identificador único (igual ao índice no array do escalonador).
    pub id: usize,
    /// Nome descritivo para debug.
    pub name: &'static str,
    /// Estado atual no ciclo de vida.
    pub state: TaskState,
    /// Prioridade de escalonamento.
    pub priority: TaskPriority,
    /// Nível de privilégio de execução.
    pub ring: Ring,
    /// RSP salvo durante a última preempção (aponta para o frame de contexto).
    pub saved_rsp: usize,
    /// Endereço físico da pilha de kernel desta tarefa (para o TSS.RSP0).
    pub kstack_top: usize,
    /// Address of the task's top-level page table (PML4).
    pub pml4_phys: u64,
    /// Código de saída (válido quando state == Zombie).
    pub exit_code: i32,

    // ── Contabilização de CPU ─────────────────────────────────────────────
    /// Número de ticks de timer consumidos por esta tarefa.
    pub cpu_ticks: u64,
    /// Número de vezes que esta tarefa foi despachada (context switches).
    pub ctx_switches: u64,
    /// Quantum restante antes da próxima preempção forçada.
    pub quantum_remaining: u64,
}

// ── Estrutura do escalonador ──────────────────────────────────────────────────

/// Pilhas de kernel alocadas estaticamente (Ring 0).
/// Ring 3 usa páginas alocadas dinamicamente.
static mut KSTACKS: [[u8; KSTACK_SIZE]; MAX_TASKS] = [[0u8; KSTACK_SIZE]; MAX_TASKS];

pub struct Scheduler {
    pub tasks: [Option<Task>; MAX_TASKS],
    next_id: usize,
    current: Option<usize>,
}

// ── Instância global ──────────────────────────────────────────────────────────

pub static mut SCHEDULER: Scheduler = Scheduler::new();

/// Acesso à instância global do escalonador.
///
/// # Safety
/// Deve ser chamado somente quando as interrupções estão desabilitadas
/// ou quando se sabe que não há reentrada.
pub fn get() -> &'static mut Scheduler {
    unsafe { &mut *core::ptr::addr_of_mut!(SCHEDULER) }
}

// ── Implementação do escalonador ──────────────────────────────────────────────

impl Scheduler {
    pub const fn new() -> Self {
        const NONE_TASK: Option<Task> = None;
        Scheduler {
            tasks:   [NONE_TASK; MAX_TASKS],
            next_id: 0,
            current: None,
        }
    }

    // ── Criação de tarefas ────────────────────────────────────────────────────

    /// Cria uma tarefa de **kernel (Ring 0)**.
    ///
    /// A tarefa executa com todos os privilégios do kernel.
    /// A pilha é alocada estaticamente em BSS.
    pub fn add_task(&mut self, name: &'static str, entry: fn()) -> usize {
        self.add_task_inner(name, entry, Ring::Kernel, TaskPriority::Normal)
    }

    /// Cria uma tarefa de **usuário (Ring 3)**.
    ///
    /// A tarefa executa em Ring 3 com acesso restrito ao hardware.
    /// Precisa de syscalls para comunicar com o kernel.
    ///
    /// Requisitos:
    /// - O frame de `iretq` usará `USER_CODE_SEL` e `USER_DATA_SEL`.
    /// - Uma pilha de usuário é alocada via frame físico e mapeada com flag `USER`.
    /// - O TSS.RSP0 é atualizado para a pilha de kernel desta tarefa.
    pub fn add_user_task(&mut self, name: &'static str, entry: fn()) -> usize {
        self.add_task_inner(name, entry, Ring::User, TaskPriority::Normal)
    }

    /// Cria uma tarefa com prioridade específica.
    pub fn add_task_with_priority(
        &mut self,
        name:     &'static str,
        entry:    fn(),
        ring:     Ring,
        priority: TaskPriority,
    ) -> usize {
        self.add_task_inner(name, entry, ring, priority)
    }

    /// Cria a tarefa Idle dedicada (Ring 0, prioridade Idle).
    ///
    /// Só executa quando nenhuma outra tarefa está pronta.
    /// Economiza energia executando `hlt` em loop.
    pub fn add_idle_task(&mut self) -> usize {
        self.add_task_inner("idle[K]", idle_task_entry, Ring::Kernel, TaskPriority::Idle)
    }

    fn add_task_inner(
        &mut self,
        name:     &'static str,
        entry:    fn(),
        ring:     Ring,
        priority: TaskPriority,
    ) -> usize {
        let mut id = self.next_id;
        while id < MAX_TASKS {
            if self.tasks[id].is_none() {
                // ── 1. Page Table (Isolamento) ─────────────────────────────────
                let pml4_phys = match ring {
                    Ring::Kernel => paging::current_cr3(), // Kernel compartilha o mesmo espaço
                    Ring::User   => paging::clone_kernel_pml4(), // Usuário ganha seu próprio PML4
                };

                // ── 2. Pilha de kernel (sempre Ring 0) ────────────────────────
                // Usamos a pilha estática de BSS. O topo precisa ser alinhado a 16 bytes.
                let kstack_base = unsafe { KSTACKS[id].as_ptr() as usize };
                let kstack_top  = (kstack_base + KSTACK_SIZE) & !15;

                // ── 3. Pilha de usuário (Ring 3) ou reutilizar a de kernel ─────
                let (user_rsp, cs_sel, ss_sel) = match ring {
                    Ring::Kernel => {
                        // Ring 0: CS/SS lidos dos registradores atuais
                        let (cs, ss) = read_cs_ss();
                        (kstack_top, cs as u64, ss as u64)
                    }
                    Ring::User => {
                        // Ring 3: aloca e mapeia uma pilha de usuário em SEU PRÓPRIO PML4
                        let ustack_top = self.setup_user_stack(id, pml4_phys);
                        (
                            ustack_top,
                            gdt::USER_CODE_SEL as u64,
                            gdt::USER_DATA_SEL as u64,
                        )
                    }
                };

                // Precisamos também mapear o código de usuário no PML4 da tarefa.
                // Atualmente `entry` aponta para código no HHDM (Ring 0).
                // Precisamos garantir que a página que contém `entry` seja acessível em Ring 3.
                // Como não temos um ELF loader ainda, vamos mapear o frame físico onde
                // `entry` reside com permissões USER_RW (idealmente KERNEL_RO | USER, mas USER_RW é mais fácil).
                if ring == Ring::User {
                    let entry_virt = entry as u64;
                    // Limine mapeia o código do kernel na higher half. Vamos apenas pegar o endereço
                    // físico correspondente (assumindo que virt_to_phys funcione para o HHDM ou kernel virt).
                    // Para simplificar e evitar page faults, o `clone_kernel_pml4` já copiou todo o kernel
                    // para a nova page table. O problema é que o Limine marcou isso como Ring 0.
                    // Nós precisaremos que a CPU consiga fazer fetch do código Ring 3.
                    // Vamos tentar alterar a permissão da página específica do `entry`.
                    // Nota: para um OS completo, carregaríamos um ELF binário.
                    let pml4 = paging::phys_to_virt(pml4_phys) as *mut paging::PageTable;
                    unsafe { paging::make_page_user(pml4, entry_virt); }
                }

                // ── 4. Frame de iretq na pilha de KERNEL ──────────────────────
                // Layout (do topo da pilha para baixo):
                //   SS, RSP_usuário, RFLAGS, CS, RIP
                //
                // Para Ring 0: SS e RSP apontam para a própria pilha de kernel.
                // Para Ring 3: SS = USER_DATA_SEL, RSP = topo da pilha de usuário.
                let entry_addr = entry as usize;
                let mut sp = kstack_top;

                // RFLAGS: IF=1 (interrupções habilitadas), IOPL=0
                let rflags: u64 = 0x202;

                sp -= 8; unsafe { (sp as *mut u64).write_volatile(ss_sel); }         // SS
                let abi_rsp = user_rsp - 8; sp -= 8; unsafe { (sp as *mut u64).write_volatile(abi_rsp as u64); }// RSP
                sp -= 8; unsafe { (sp as *mut u64).write_volatile(rflags); }          // RFLAGS
                sp -= 8; unsafe { (sp as *mut u64).write_volatile(cs_sel); }          // CS
                sp -= 8; unsafe { (sp as *mut u64).write_volatile(entry_addr as u64);}// RIP

                // ATENÇÃO: O irq_common (stubs.s) faz `add rsp, 16` antes do `iretq`
                // para limpar o vector e o dummy error code deixados pelo irq_stub.
                // Como nós vamos pular direto para o final do irq_common na primeira
                // preempção, precisamos simular esses 16 bytes aqui, senão o `add rsp, 16`
                // vai engolir nosso RIP e CS, causando um crash (General Protection Fault
                // silencioso ou Page Fault)!
                sp -= 8; unsafe { (sp as *mut u64).write_volatile(0); } // error_code (dummy)
                sp -= 8; unsafe { (sp as *mut u64).write_volatile(0); } // vector (dummy)

                // ── 5. Frame de registradores GPR (15 registradores) ──────────
                // O stub de contexto salva/restaura: R15, R14, R13, R12, R11, R10,
                // R9, R8, RDI, RSI, RBP, RBX, RDX, RCX, RAX
                for _ in 0..15 {
                    sp -= 8;
                    unsafe { (sp as *mut u64).write_volatile(0); }
                }

                // ── 6. Atualiza TSS.RSP0 se for Ring 3 ───────────────────────
                if ring == Ring::User {
                    gdt::set_kernel_stack(kstack_top as u64);
                }

                // Quantum inicial baseado na prioridade
                let quantum = match priority {
                    TaskPriority::RealTime => QUANTUM_REALTIME,
                    TaskPriority::Normal   => QUANTUM_NORMAL,
                    TaskPriority::Idle     => QUANTUM_IDLE,
                };

                self.tasks[id] = Some(Task {
                    id,
                    name,
                    state:     TaskState::Ready,
                    priority,
                    ring,
                    saved_rsp: sp,
                    kstack_top,
                    pml4_phys,
                    exit_code: 0,
                    cpu_ticks: 0,
                    ctx_switches: 0,
                    quantum_remaining: quantum,
                });
                self.next_id = id + 1;
                return id;
            }
            id += 1;
        }
        // Sem slots disponíveis — retorna 0 (tarefa do kernel idle)
        0
    }

    /// Aloca e mapeia uma pilha de usuário no espaço de endereços do processo.
    ///
    /// Retorna o endereço virtual do **topo** da pilha (RSP inicial).
    fn setup_user_stack(&self, task_id: usize, pml4_phys: u64) -> usize {
        let stack_top_virt  = USER_STACK_BASE - (task_id as u64 * USTACK_SIZE as u64);
        let stack_base_virt = stack_top_virt - USTACK_SIZE as u64;

        let pml4 = paging::phys_to_virt(pml4_phys) as *mut paging::PageTable;

        // Mapeia cada página da pilha com flags de usuário (PRESENT | WRITABLE | USER)
        let pages = USTACK_SIZE / 4096;
        for i in 0..pages {
            let virt = stack_base_virt + (i as u64 * 4096);
            // Aloca um frame físico para cada página da pilha
            let phys = crate::memory::alloc_frame()
                .expect("Escalonador: sem frames para pilha de usuário");

            // SAFETY: pml4 é o CR3 da tarefa (novo ou atual), phys é um frame válido.
            unsafe {
                paging::map_page(
                    pml4,
                    virt,
                    phys,
                    paging::USER_RW, // PRESENT | WRITABLE | USER
                );
            }
        }

        // Topo da pilha = endereço mais alto - 16 (alinhamento ABI)
        (stack_top_virt as usize) - 16
    }

    // ── Gerenciamento de ciclo de vida ────────────────────────────────────────

    /// Marca uma tarefa como Zombie (aguardando coleta). Chamado por `sys_exit`.
    ///
    /// Se a tarefa era Ring 3, libera seu PML4 e todas as páginas de usuário.
    /// Também desbloqueia qualquer tarefa que esteja esperando via `waitpid`.
    pub fn exit_task(&mut self, pid: usize, code: i32) {
        if let Some(Some(task)) = self.tasks.get_mut(pid) {
            task.state     = TaskState::Zombie;
            task.exit_code = code;

            // Libera a memória de páginas de usuário e o próprio PML4
            if task.ring == Ring::User {
                unsafe { paging::free_user_pml4(task.pml4_phys); }
            }
        }

        // Desbloqueia quem estiver esperando esta tarefa via WaitingOn
        for slot in self.tasks.iter_mut() {
            if let Some(task) = slot {
                if task.state == TaskState::WaitingOn(pid) {
                    task.state = TaskState::Ready;
                }
            }
        }
    }

    /// Bloqueia uma tarefa (esperando I/O ou IPC).
    pub fn block_task(&mut self, pid: usize) {
        if let Some(Some(task)) = self.tasks.get_mut(pid) {
            if task.state == TaskState::Running || task.state == TaskState::Ready {
                task.state = TaskState::Blocked;
            }
        }
    }

    /// Desbloqueia uma tarefa previamente bloqueada.
    pub fn unblock_task(&mut self, pid: usize) {
        if let Some(Some(task)) = self.tasks.get_mut(pid) {
            if task.state == TaskState::Blocked {
                task.state = TaskState::Ready;
            }
        }
    }

    /// Bloqueia a tarefa atual até que `target_pid` termine (Zombie).
    ///
    /// Se o alvo já é Zombie, retorna imediatamente sem bloquear.
    /// Caso contrário, coloca a tarefa corrente em `WaitingOn(target_pid)`.
    pub fn wait_on(&mut self, waiter_pid: usize, target_pid: usize) -> bool {
        // Verifica se o alvo já terminou
        if self.zombie_of(target_pid) {
            return true;
        }

        // Verifica se o alvo sequer existe
        if target_pid >= MAX_TASKS {
            return false;
        }
        if self.tasks[target_pid].is_none() {
            return false;
        }

        // Bloqueia a tarefa que espera
        if let Some(Some(task)) = self.tasks.get_mut(waiter_pid) {
            task.state = TaskState::WaitingOn(target_pid);
        }
        false
    }

    // ── Direct Thread Switch (para IPC Fast-Path) ─────────────────────────────

    /// Transferência direta de contexto entre duas tarefas, sem passar pela
    /// fila de prontos do escalonador.
    ///
    /// Usado pelo IPC Core quando o receptor já está `BlockedOnReceive`:
    /// - O emissor é bloqueado (WaitReply).
    /// - O receptor é acordado com Direct Switch.
    /// - O quantum do emissor é cedido ao receptor.
    ///
    /// Retorna o RSP salvo do receptor (para que o stub de IRQ restaure).
    /// Retorna `None` se o receptor não existir ou não estiver bloqueado.
    pub fn direct_switch(
        &mut self,
        sender_pid: usize,
        receiver_pid: usize,
        sender_rsp: usize,
    ) -> Option<usize> {
        // Salva o estado do emissor
        if let Some(Some(sender)) = self.tasks.get_mut(sender_pid) {
            sender.saved_rsp = sender_rsp;
            sender.state = TaskState::Blocked; // Bloqueado esperando reply
        } else {
            return None;
        }

        // Verifica se o receptor está bloqueado (esperando mensagem)
        let receiver_rsp;
        let receiver_ring;
        let receiver_kstack;
        let receiver_pml4;
        {
            let receiver = self.tasks.get_mut(receiver_pid)?;
            let task = receiver.as_mut()?;
            if task.state != TaskState::Blocked {
                return None;
            }
            task.state = TaskState::Running;
            task.ctx_switches += 1;
            receiver_rsp = task.saved_rsp;
            receiver_ring = task.ring;
            receiver_kstack = task.kstack_top;
            receiver_pml4 = task.pml4_phys;
        }

        // Atualiza estado global
        self.current = Some(receiver_pid);
        CURRENT_TASK.store(receiver_pid as i32, Ordering::SeqCst);
        CONTEXT_SWITCHES.fetch_add(1, Ordering::Relaxed);

        // Atualiza TSS.RSP0 se o receptor é Ring 3
        if receiver_ring == Ring::User {
            gdt::set_kernel_stack(receiver_kstack as u64);
        }

        // Troca CR3 se necessário
        let current_cr3 = paging::current_cr3();
        if current_cr3 != receiver_pml4 {
            unsafe { paging::load_cr3(receiver_pml4); }
        }

        Some(receiver_rsp)
    }

    // ── Escalonamento ─────────────────────────────────────────────────────────

    /// Seleciona a próxima tarefa pronta com maior prioridade (Round-Robin por nível).
    pub fn run_next(&mut self) -> Option<usize> {
        // Prioridade RealTime > Normal > Idle
        for priority in [TaskPriority::RealTime, TaskPriority::Normal, TaskPriority::Idle] {
            let start = self.current.unwrap_or(usize::MAX);
            for delta in 1..=MAX_TASKS {
                let idx = if start == usize::MAX {
                    delta - 1
                } else {
                    (start + delta) % MAX_TASKS
                };
                if let Some(task) = &mut self.tasks[idx] {
                    if task.state == TaskState::Ready && task.priority == priority {
                        self.current = Some(idx);
                        task.state   = TaskState::Running;
                        CURRENT_TASK.store(idx as i32, Ordering::SeqCst);
                        YIELDED.store(false, Ordering::SeqCst);
                        return Some(idx);
                    }
                }
            }
        }
        None
    }

    /// Chamado pelo handler do APIC Timer (vetor 32) para fazer preempção.
    ///
    /// Salva o RSP atual da tarefa em execução, seleciona a próxima tarefa e
    /// retorna o RSP salvo dela. O stub em `stubs.s` usa o valor retornado
    /// para restaurar os registradores via `iretq`.
    ///
    /// Também atualiza o TSS.RSP0 e o CR3 (PML4) para a próxima tarefa.
    pub fn preempt(&mut self, curr_frame_ptr: usize) -> usize {
        // Incrementa contador de preempções
        SCHED_TICKS.fetch_add(1, Ordering::Relaxed);

        // Contabiliza tick de CPU e verifica quantum da tarefa atual
        let mut force_switch = false;
        if let Some(cur) = self.current {
            if let Some(task) = &mut self.tasks[cur] {
                task.cpu_ticks += 1;

                if task.quantum_remaining > 0 {
                    task.quantum_remaining -= 1;
                }

                // Se a tarefa foi cedida manualmente (yield) ou esgotou seu quantum
                if YIELDED.load(Ordering::SeqCst) || task.quantum_remaining == 0 {
                    force_switch = true;
                }
            }
        } else {
            // Sem tarefa atual — forçar seleção
            force_switch = true;
        }

        if !force_switch {
            // Quantum ainda não expirou e não houve yield — continua na tarefa
            return curr_frame_ptr;
        }

        // Salva RSP da tarefa atual
        if let Some(cur) = self.current {
            if let Some(task) = &mut self.tasks[cur] {
                task.saved_rsp = curr_frame_ptr;
                if task.state == TaskState::Running {
                    task.state = TaskState::Ready;
                }
            }
        }

        YIELDED.store(false, Ordering::SeqCst);

        // Escalonamento por prioridade (Round-Robin dentro de cada nível)
        for priority in [TaskPriority::RealTime, TaskPriority::Normal, TaskPriority::Idle] {
            let start = self.current.unwrap_or(usize::MAX);
            for delta in 1..=MAX_TASKS {
                let idx = if start == usize::MAX {
                    delta - 1
                } else {
                    (start + delta) % MAX_TASKS
                };
                if let Some(task) = &mut self.tasks[idx] {
                    if task.state == TaskState::Ready && task.priority == priority {
                        self.current = Some(idx);
                        task.state   = TaskState::Running;
                        task.ctx_switches += 1;
                        CURRENT_TASK.store(idx as i32, Ordering::SeqCst);
                        CONTEXT_SWITCHES.fetch_add(1, Ordering::Relaxed);

                        // Recarrega o quantum para o novo ciclo
                        task.quantum_remaining = match task.priority {
                            TaskPriority::RealTime => QUANTUM_REALTIME,
                            TaskPriority::Normal   => QUANTUM_NORMAL,
                            TaskPriority::Idle     => QUANTUM_IDLE,
                        };

                        // 1. Atualiza TSS.RSP0 para a pilha de kernel desta tarefa
                        if task.ring == Ring::User {
                            gdt::set_kernel_stack(task.kstack_top as u64);
                        }

                        // 2. Troca o espaço de endereçamento virtual se for diferente
                        let current_cr3 = paging::current_cr3();
                        if current_cr3 != task.pml4_phys {
                            unsafe { paging::load_cr3(task.pml4_phys); }
                        }

                        return task.saved_rsp;
                    }
                }
            }
        }

        // Nenhuma outra tarefa pronta — continua a atual
        curr_frame_ptr
    }

    // ── Coleta de Zombies ─────────────────────────────────────────────────────

    /// Verifica se a tarefa `pid` está no estado `Zombie` (terminou mas não
    /// foi coletada).
    pub fn zombie_of(&self, pid: usize) -> bool {
        matches!(
            self.tasks.get(pid).and_then(|t| t.as_ref()).map(|t| t.state),
            Some(TaskState::Zombie)
        )
    }

    /// Coleta uma tarefa `Zombie`: lê seu exit code, libera o slot e retorna
    /// `Some(exit_code)`. Retorna `None` se o PID não existir ou não for Zombie.
    pub fn collect_zombie(&mut self, pid: usize) -> Option<i32> {
        let slot = self.tasks.get_mut(pid)?;
        let task  = slot.as_ref()?;
        if task.state != TaskState::Zombie {
            return None;
        }
        let code = task.exit_code;
        // Libera o slot — pode ser reutilizado
        *slot = None;
        // Ajusta next_id para reaproveitar slots
        if pid < self.next_id {
            self.next_id = pid;
        }
        Some(code)
    }

    /// Retorna o número de tarefas ativas (não-None, exceto Completed).
    pub fn task_count(&self) -> usize {
        self.tasks.iter()
            .filter(|t| t.is_some())
            .count()
    }

    // ── Consultas ─────────────────────────────────────────────────────────────

    /// Retorna o estado de uma tarefa pelo PID.
    pub fn task_state(&self, pid: usize) -> Option<TaskState> {
        self.tasks.get(pid)?.as_ref().map(|t| t.state)
    }

    /// Retorna os ticks de CPU consumidos por uma tarefa.
    pub fn task_cpu_ticks(&self, pid: usize) -> Option<u64> {
        self.tasks.get(pid)?.as_ref().map(|t| t.cpu_ticks)
    }

    // ── Debug ─────────────────────────────────────────────────────────────────

    /// Imprime o status de todas as tarefas na tela.
    pub fn print_status(&self, writer: &mut Writer) {
        writer.write_line("=== Tarefas do Escalonador ===");
        for task in self.tasks.iter().flatten() {
            writer.write_string("  [");
            writer.write_decimal(task.id);
            writer.write_string("] ");
            writer.write_string(task.name);
            writer.write_string("  Ring:");
            writer.write_string(match task.ring {
                Ring::Kernel => "0",
                Ring::User   => "3",
            });
            writer.write_string("  Prio:");
            writer.write_string(match task.priority {
                TaskPriority::Idle     => "Idle",
                TaskPriority::Normal   => "Normal",
                TaskPriority::RealTime => "RealTime",
            });
            writer.write_string("  Estado:");
            writer.write_string(match task.state {
                TaskState::Ready        => "Pronto",
                TaskState::Running      => "Executando",
                TaskState::Blocked      => "Bloqueado",
                TaskState::WaitingOn(_) => "Esperando",
                TaskState::Zombie       => "Zombie",
                TaskState::Completed    => "Concluido",
            });
            writer.write_string("  CPU:");
            writer.write_decimal(task.cpu_ticks as usize);
            writer.write_string("t");
            writer.write_line("");
        }
        writer.write_line("==============================");
    }
}

// ── Tarefa Idle ───────────────────────────────────────────────────────────────

/// Ponto de entrada da tarefa Idle.
///
/// Executa `hlt` em loop para economizar energia quando nenhuma outra
/// tarefa está pronta. A instrução `hlt` suspende a CPU até a próxima
/// interrupção (que será o APIC Timer), momento em que o escalonador
/// poderá despachar outra tarefa que tenha se tornado Ready.
fn idle_task_entry() {
    loop {
        unsafe { core::arch::asm!("hlt"); }
    }
}

// ── Funções auxiliares ────────────────────────────────────────────────────────

/// Lê os registradores CS e SS atuais do processador.
fn read_cs_ss() -> (u16, u16) {
    let mut cs: u16;
    let mut ss: u16;
    unsafe {
        core::arch::asm!("mov {:x}, cs", out(reg) cs, options(nomem, nostack));
        core::arch::asm!("mov {:x}, ss", out(reg) ss, options(nomem, nostack));
    }
    (cs, ss)
}
