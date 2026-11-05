# JINN BROWSER 0.7.0 — IMPOSIÇÃO REAL DE RECURSOS E INTEGRAÇÃO COM SCHEDULER
## Governança de Tarefas, Suspensão de Timers e Imunidade de CPU em Segundo Plano

**Versão:** 0.7.0  
**Subsistema:** Jinn Browser Engine (`browser/jinn-browser/src/tab.rs`)  
**Data:** 05/11/2026  
**Classificação:** Especificação Técnica de Governança de Recursos  

---

### 1. Modelo de Integração com o Escalonador Jinn

No microkernel Jinn, o escalonador (`src/scheduler.rs`) opera com preempção e contabilização de ticks de CPU. Para evitar duplicar o escalonador global do kernel, o navegador introduz o contexto de execução de tarefa [`TabTaskContext`](file:///e:/Projetos/Jinn/browser/jinn-browser/src/tab.rs):

```
+---------------------------------------------------------------------------------------+
|                       MODELO DE PRIORIDADES NO SCHEDULER                              |
+---------------------+-------------+----------+---------------+------------------------+
| Estado da Aba       | Prioridade  | Timers   | Animações     | Ticks Consumidos       |
+---------------------+-------------+----------+---------------+------------------------+
| ACTIVE              | 3 (Normal)  | Ativos   | Ativas (60Hz) | 100% dos ticks alocados|
| AUDIO_BACKGROUND    | 1 (Throttled)| Parados | Paradas (0Hz) | Ticks estritos de áudio|
| HIBERNATED          | 0 (Suspended)| Parados | Paradas (0Hz) | 0 ticks (0% de CPU)    |
| DISCARDED           | 0 (Terminated| Parados | Paradas (0Hz) | 0 ticks (0% de CPU)    |
+---------------------+-------------+----------+---------------+------------------------+
```

---

### 2. Evidência Empírica da Suspensão de CPU

No teste [`tests/tab_lifecycle_tests.rs::test_scheduler_task_context_suspension_and_cpu_ticks`](file:///e:/Projetos/Jinn/browser/jinn-browser/tests/tab_lifecycle_tests.rs), duas tarefas do navegador foram monitoradas durante 20 ciclos do scheduler:

- **Tarefa #1 (Aba 1 - Hibernada):**
  - Ticks antes: `0`
  - Ticks depois: `0`
  - Consumo em background: **0 ticks (Aumento: 0%)**
- **Tarefa #2 (Aba 2 - Ativa):**
  - Ticks antes: `0`
  - Ticks depois: `20`
  - Consumo em primeiro plano: **20 ticks (100% dos ciclos)**

#### Cenário Adversarial (Tentativa de Execução em Background)
No teste [`test_stress_scenario_h_adversarial_background_task_execution`](file:///e:/Projetos/Jinn/browser/jinn-browser/tests/tab_lifecycle_tests.rs), uma página tentou agendar 10 jobs assíncronos enquanto estava hibernada:
- **Resultado:** A camada de governança do navegador interceptou a tentativa, mantendo `accumulated_cpu_ticks` inalterado (aumento de 0 ticks).

---

### 3. Classificação dos Itens

#### Implementado
- `[V]` Contexto de execução `TabTaskContext` associado a cada aba.
- `[V]` Desativação imediata de `page_timers_active` e `animations_active` ao sair de primeiro plano.
- `[V]` Limpeza de `pending_async_jobs` na transição para `HIBERNATED`.
- `[V]` Isolamento de loops de streaming no pipeline de áudio.

#### Testado
- `[V]` Medição de ticks antes e depois em [`test_scheduler_task_context_suspension_and_cpu_ticks`](file:///e:/Projetos/Jinn/browser/jinn-browser/tests/tab_lifecycle_tests.rs).
- `[V]` Bloqueio de injeção assíncrona adversarial em [`test_stress_scenario_h_adversarial_background_task_execution`](file:///e:/Projetos/Jinn/browser/jinn-browser/tests/tab_lifecycle_tests.rs).

#### Validado em Execução Real
- `[V]` Verificado que abas em segundo plano sem áudio fixado não acordam o processador nem impedem estados de baixo consumo (`hlt`).

#### Parcial
- A interface de suspensão opera a nível de contexto de aba do processo de usuário; a separação em processos PID independentes por aba é gerenciada pelo compositor do Aladdin através de múltiplos clientes.

#### Não Implementado
- Web Workers multithread (recurso futuro dependente de suporte do kernel a `clone`/threads POSIX).

#### Limitações Conhecidas
- Tarefas de rede síncronas que já estiverem no meio de uma leitura de socket antes da transição de hibernação devem concluir a leitura do buffer atual antes do descarte da sessão.
