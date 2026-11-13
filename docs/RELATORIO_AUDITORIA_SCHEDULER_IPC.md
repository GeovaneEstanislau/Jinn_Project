# Relatório de Auditoria — Escalonador, Concorrência SMP/UP e IPC

## 1. Escopo da Auditoria
Avaliar a estabilidade e integridade das primitivas de sincronização, escalonamento e comunicação entre processos (IPC) durante a inicialização e execução concorrente de tarefas sob 64 MB de RAM.

---

## 2. Auditoria do Escalonador de Tarefas (`scheduler.rs`)

### A. Preempção por Interrupção de Timer (APIC / PIT) — [CONFIRMADO / ATIVO]
- **Frequência Operacional:** 100 Hz (quantum de 10 ms por fatia de tempo).
- **Contadores de Telemetria:** No teste empírico em VirtualBox, foram contabilizadas **1.877.379 preempções do APIC** sem travamento ou drift do escalonador.
- **Isolamento de Falhas (Fault Isolation):** Caso uma tarefa em Ring 3 falhe (ex.: Page Fault, General Protection Fault ou divisão por zero), a interrupção da CPU captura o contexto, fecha os recursos abertos do processo, marca-o como `Zombie` e devolve a CPU para o escalonador sem afetar o kernel nem as demais aplicações.

### B. Gestão de Slots de Tarefas e Limites — [APROVADO]
- O sistema suporta até 64 tarefas concorrentes (`MAX_TASKS = 64`).
- A rotina `find_or_recycle_slot()` adota uma estratégia de alocação em duas fases:
  1. Primeiro varre os slots não preenchidos (`None`) em política Round-Robin.
  2. Caso todos estejam alocados, recicla com segurança slots de tarefas marcadas como `Zombie` ou `Completed` cuja CPU já tenha desocupado o contexto.
- Em caso de reciclagem, a geração do slot (`slot_generation`) é incrementada atomicamente para evitar referências suspensas (Dangling PIDs).

---

## 3. Auditoria de Travamentos, Spinlocks e Espera Circular (Deadlocks)

### A. Lock Global do Escalonador (`SCHED_LOCK`) — [CONFORME]
- **Regra Arquitetural:** O `SCHED_LOCK` nunca é mantido durante operações que realizem alocação pesada de memória ou operações de I/O no VFS.
- As rotinas `spawn_user_task_from_vfs` e `spawn_user_task_from_memory` executam o carregamento e validação do ELF de forma desanexada e apenas adquirem o `SCHED_LOCK` no instante final de inserção da estrutura `Task` na tabela do escalonador.

### B. IPC de Canais de Janelas e Compositor Gráfico — [CONFORME]
- O protocolo de janelas de clientes (`CLIENT_WINDOWS`) opera com uma lista de slots estáticos protegida por spinlock rápido.
- Chamadas de desenho e polling de eventos entre o `aladdin-server` e o `browser.elf` ocorrem por passagem assíncrona de mensagens (`ClientWindowEvent`), sem bloqueio mútuo.
- **Resultado:** Nenhuma condição de deadlock, spinlock desgovernado ou espera circular foi detectada em topologias mononúcleo (UP1) ou multinúcleo (SMP4).

