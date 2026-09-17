# Jinn OS: Design de Recursos Avançados (Fase 3 & Drivers)

Este documento descreve a arquitetura para os próximos grandes saltos do Jinn OS, detalhando o funcionamento de três componentes cruciais: o Motor Preditivo, o Escalonador Adaptativo e o Gerenciador de Drivers.

---

## 1. Motor Preditivo (Predictive Engine)
**Status:** Fase de Implementação Inicial

O Motor Preditivo é o diferencial técnico do Jinn OS. Em vez de reagir passivamente à carga do sistema, o kernel analisa os dados de telemetria em background para prever o comportamento das tarefas (Ring 3).

### Arquitetura
- **Coleta (Input):** Lê o módulo `telemetry.rs` (contadores de CPU Ticks, IPC TX/RX, Page Faults).
- **Processamento:** Executa heurísticas a cada N *ticks* do relógio do sistema.
  - *IPC Affinity Scoring:* Se o Processo A envia milhares de mensagens para o Processo B, o motor gera um "Vínculo de Afinidade" alto entre eles.
  - *I/O Bound vs CPU Bound:* Tarefas com altos *ticks* mas pouco IPC são classificadas como `CPU Bound`. Tarefas que fazem muito IPC em poucos ticks são `I/O Bound`.
- **Matriz de Predição (Output):** Uma tabela leve em Ring 0 que contém `(PID -> PID_Afinidade, Boost_Recomendado)`. 

---

## 2. Escalonador Adaptativo (Adaptive Scheduler)
**Status:** Planejamento

Atualmente o `scheduler.rs` utiliza Round-Robin puro (todas as tarefas recebem o mesmo tempo de CPU, na mesma ordem). O Escalonador Adaptativo modificará isso para utilizar a Matriz de Predição.

### Arquitetura
- **Pre-warming de Cache L1/L2:** Se a Matriz de Predição indicar que a Tarefa A conversa intensamente com a Tarefa B (alta afinidade), o escalonador garantirá que a Tarefa B execute *imediatamente após* a Tarefa A, aproveitando que os dados trocados por IPC ainda estão quentes nos caches L1/L2 do processador.
- **Dynamic Priority Boost:** Tarefas classificadas como `I/O Bound` (ex: drivers) receberão um ganho automático de prioridade momentâneo ao receberem uma mensagem, reduzindo a latência de resposta.
- **Penalização:** Tarefas puramente matemáticas (`CPU Bound`) terão seus quantums de tempo alongados, mas sua prioridade imediata rebaixada para não travar a interatividade do sistema.

---

## 3. Gerenciador de Drivers (Driver Manager)
**Status:** Planejamento (Fase 2 deferida)

Em arquiteturas Microkernel, drivers não rodam no kernel (Ring 0), mas sim como processos de usuário comuns (Ring 3). O Gerenciador de Drivers é um processo de sistema que atua como o "supervisor" de hardware.

### Arquitetura
- **Manifestos:** Cada driver informa de quais portas I/O (ex: `0x60` para teclado) e interrupções ele precisa (via `driver-manifest.md`).
- **Capability Routing:** O Driver Manager pede ao Kernel permissão (capabilities) em nome do driver.
- **Fail-safe Recovery:** Se um driver de rede tentar acessar memória inválida e causar um *Page Fault*, o kernel o mata. O Driver Manager percebe a morte via IPC, e instantaneamente sobe uma nova cópia do driver de rede sem causar *Kernel Panic*, tornando o sistema imortal a falhas de driver.
