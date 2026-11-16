# 🧪 Jinn OS — Plano de Validação e Testes de Conformidade FPU/SIMD
## Protocolos de Homologação Numérica, Concorrência SMP e Não-Regressão (Etapa B.1)

> **Documento Oficial de Engenharia — Etapa B.1 (Diagnóstico e Planejamento)**  
> **Data de Emissão:** 16 de Novembro de 2026 | **Branch:** `main` | **Commit Base:** `537c534`  
> **Status:** Concluído | **Objetivo:** Estabelecer a bateria de testes rigorosa para homologação da preservação de FPU antes de qualquer alteração de código.  
> **Regra Fundamental:** Os testes devem verificar **valores numéricos e corretude aritmética estrita**, e não apenas a ausência de pânico no kernel.

---

## 1. Metodologia de Teste e Instrumentação

A validação da preservação de estado da FPU será executada por uma bateria de **9 testes formais**, combinando asserções determinísticas em tempo de execução com medição de ciclos de CPU via instrução `RDTSC`.

### Princípio da Verificação por Padrão de Bits (Bit-Pattern Verification)
Cada tarefa em teste carrega nos registradores `XMM0..XMM15` e na pilha x87 `ST0..ST7` uma assinatura numérica única e exclusiva:
* **Tarefa $\alpha$ (Alpha):** Carrega valores derivados de potências de 2 e constantes transcendentais ($\pi, e$).
* **Tarefa $\beta$ (Beta):** Carrega valores derivados da sequência de Fibonacci e constantes de Euler ($\gamma, \phi$).
* A cada iteração após preempção forçada ou voluntária, a tarefa compara o conteúdo exato de cada um dos 16 registradores `XMM` com seu padrão esperado. Se um único bit divergir, o teste falha imediatamente acusando corrupção cruzada.

---

## 2. Especificação Detalhada dos 9 Casos de Teste

---

### Teste 1: Isolamento entre Duas Tarefas Ring 3 com Estados XMM Distintos
* **Objetivo:** Comprovar que duas tarefas concorrentes em espaço de usuário não sobrescrevem os registradores vetoriais uma da outra.
* **Procedimento:**
  1. Instanciar Tarefa A com padrão `XMM0 = [1.0, 2.0, 3.0, 4.0]` até `XMM15`.
  2. Instanciar Tarefa B com padrão `XMM0 = [10.0, 20.0, 30.0, 40.0]` até `XMM15`.
  3. Ambas as tarefas entram em loop cooperativo executando verificações contínuas.
* **Critério de Sucesso:** 1.000 iterações em cada tarefa sem divergência em nenhuma das componentes float de 32/64 bits.
* **Classificação Atual:** `NÃO TESTADO` (planejado para execução na Etapa B.2).

---

### Teste 2: Alternância Rápida de Contexto via Cessão Voluntária (`sys_yield`)
* **Objetivo:** Validar o salvamento e restauração síncrona nos caminhos de *voluntary yield* (`yield_now` / `sys_yield`).
* **Procedimento:**
  1. Tarefa A executa `ADDPS XMM0, XMM1`, grava o acumulador e invoca `sys_yield()`.
  2. Tarefa B executa `SUBPS XMM0, XMM1`, grava o acumulador e invoca `sys_yield()`.
  3. O escalonador comuta repetidamente entre A e B em velocidade máxima.
* **Critério de Sucesso:** 10.000 trocas voluntárias consecutivas com acumulação aritmética idêntica à computação em thread única.
* **Classificação Atual:** `NÃO TESTADO`.

---

### Teste 3: Teste de Estresse de Longa Duração sob Preempção por Timer (APIC 100 Hz)
* **Objetivo:** Garantir estabilidade da FPU sob preempções assíncronas e imprevisíveis disparadas por interrupções de hardware.
* **Procedimento:**
  1. Duas tarefas executam multiplicação contínua de matrizes $4 \times 4$ usando instruções `MULPS` e `ADDPS` durante 60 segundos ininterruptos.
  2. O timer do APIC preempciona as tarefas a cada 10 ms (100 Hz).
  3. Total de preempções forçadas monitorado via `scheduler::timer_preemptions()`.
* **Critério de Sucesso:** Mínimo de 6.000 preempções de hardware sem nenhum erro aritmético e com checksum final de matriz correto.
* **Classificação Atual:** `NÃO TESTADO`.

---

### Teste 4: Concorrência e Escalonamento em Topologia SMP (QEMU 1 vCPU vs. 4 vCPUs)
* **Objetivo:** Comprovar a robustez do salvamento/restauração sob múltiplos núcleos independentes executando em paralelo.
* **Procedimento:**
  1. Executar a bateria em ambiente mononúcleo (`qemu-system-x86_64 -smp 1`).
  2. Repetir rigorosamente o mesmo teste em ambiente multi-core com 4 núcleos (`-smp 4`).
  3. Verificar se múltiplos APs executando `preempt_core` simultaneamente não causam contenção no `FpuState` de nenhuma tarefa.
* **Critério de Sucesso:** Zero anomalias em ambas as configurações; ausência de *data races* confirmada pela proteção de lock atômico.
* **Classificação Atual:** `NÃO TESTADO`.

---

### Teste 5: Migração de Tarefas entre Núcleos SMP
* **Objetivo:** Certificar que uma tarefa que inicia cálculos no Core 0 e é retomada no Core 1, 2 ou 3 mantém integridade numérica absoluta.
* **Procedimento:**
  1. Tarefa registra o ID da CPU onde iniciou via `CPUID` ou MSR APIC.
  2. Tarefa realiza fatias de cálculo vetorial intercaladas por preempção.
  3. Detectar eventos onde `cpu_id_fim != cpu_id_inicio` (migração efetiva).
  4. Validar o acumulador `XMM` imediatamente após a migração.
* **Critério de Sucesso:** Pelo menos 100 eventos de migração entre núcleos monitorados sem nenhuma falha de integridade.
* **Classificação Atual:** `NÃO TESTADO`.

---

### Teste 6: Inicialização Segura de Novos Processos (`sys_spawn` e `sys_execve`)
* **Objetivo:** Provar a ausência de vazamento de registradores residuais em processos recém-nascidos.
* **Procedimento:**
  1. Tarefa "espiã" é disparada imediatamente após uma tarefa que preencheu a FPU com dados sensíveis (`0xDEADBEEFCAFEBABE`).
  2. A nova tarefa inspeciona todos os registradores `XMM0..XMM15` antes de executar qualquer operação.
  3. Verificar se todos os registradores contêm rigorosamente `0.0` e se `MXCSR == 0x1F80`.
  4. Repetir o teste após invocação de `sys_execve_framed`.
* **Critério de Sucesso:** Leitura limpa com 100% dos registradores zerados em todos os testes.
* **Classificação Atual:** `NÃO TESTADO`.

---

### Teste 7: Encerramento de Tarefas e Reciclagem de Slots
* **Objetivo:** Garantir que quando uma tarefa finaliza (`sys_exit` ou término forçado) e seu slot de PID é reutilizado pelo escalonador, o novo ocupante não herde resíduos da tarefa morta.
* **Procedimento:**
  1. Processo A preenche FPU e termina (`sys_exit(0)`).
  2. Escalonador recicla o slot com nova geração (`slot_generation`).
  3. Processo C aloca o mesmo slot de ID e valida seu estado inicial.
* **Critério de Sucesso:** Slot redefinido limpo sem vazamento de estado numérico anterior.
* **Classificação Atual:** `NÃO TESTADO`.

---

### Teste 8: Não-Regressão do Escalonador e Aladdin Desktop
* **Objetivo:** Comprovar que a inclusão de `FXSAVE64`/`FXRSTOR64` não degrada as tarefas que não utilizam ponto flutuante intensivo.
* **Procedimento:**
  1. Boot completo do microkernel Jinn OS com o compositor Aladdin Desktop.
  2. Executar movimentação de janelas, digitação no console serial e clique de menus.
  3. Monitorar contadores de preempção e taxa de quadros (60 FPS).
* **Critério de Sucesso:** Zero travamentos, congelamentos ou degradação perceptível na fluidez da interface gráfica.
* **Classificação Atual:** `NÃO TESTADO`.

---

### Teste 9: Interrupções Durante Cálculos de Ponto Flutuante Intensivo
* **Objetivo:** Validar que interrupções de periféricos (mouse, teclado, pacotes de rede VirtIO-Net) não corrompem registradores `XMM` no meio de cálculos vetoriais.
* **Procedimento:**
  1. Disparar flood de pacotes ICMP (ping) e movimentação frenética de mouse enquanto um processo de usuário executa cálculos trigonométricos contínuos.
  2. O driver de rede e o driver PS/2 disparam dezenas de IRQs por segundo.
* **Critério de Sucesso:** Cálculos numéricos concluem com exatidão matemática idêntica ao resultado esperado em silêncio de interrupções.
* **Classificação Atual:** `NÃO TESTADO`.

---

## 3. Métricas de Desempenho e Medição de Sobrecarga (Overhead)

A sobrecarga introduzida pelo salvamento da FPU será medida em tempo real utilizando os contadores de telemetria já existentes no escalonador:

$$\Delta_{\text{ciclos}} = \text{LAST\_SCHED\_CYCLES}_{\text{com\_FPU}} - \text{LAST\_SCHED\_CYCLES}_{\text{sem\_FPU}}$$

| Métrica de Desempenho | Instrumento de Medição | Valor Base (Sem FPU) | Meta com FXSAVE64/FXRSTOR64 | Limite de Rejeição |
|---|---|:---:|:---:|:---:|
| **Ciclos por Context Switch** | `read_tsc()` em `preempt_core` | ~450 a 800 ciclos | **600 a 1.050 ciclos** | $> 1.800$ ciclos |
| **Tempo de Execução `FXSAVE64`** | Inline assembly com RDTSC | N/A | **~70 a 110 ciclos** | $> 250$ ciclos |
| **Tempo de Execução `FXRSTOR64`**| Inline assembly com RDTSC | N/A | **~80 a 120 ciclos** | $> 250$ ciclos |
| **Impacto na Taxa de Preempção** | Timer APIC a 100 Hz | 100 ticks/seg | **100 ticks/seg** | $< 98$ ticks/seg |

---

## 4. Tabela de Rastreabilidade e Critérios de Homologação

| Caso de Teste | Pré-requisito Técnico | Dependência de SMP | Critério Formal de Homologação |
|:---:|---|:---:|---|
| **Teste 1** | `Task.fpu_state` implementado | Não | Zero divergência de bits em XMM0..XMM15 |
| **Teste 2** | `fxsave`/`fxrstor` em `yield_now` | Não | Acumulador idêntico em 10.000 yields |
| **Teste 3** | `fxsave`/`fxrstor` em `preempt_core` | Não | 60 segundos sob 100 Hz sem erro em matriz 4x4 |
| **Teste 4** | Lock atômico `SCHED_LOCK` | Sim (4 vCPUs) | Comportamento idêntico em UP1 e SMP4 |
| **Teste 5** | Migração com Acquire/Release | Sim (4 vCPUs) | Coerência mantida em $> 100$ migrações |
| **Teste 6** | Sanitização em `spawn` e `execve` | Não | Todos os registradores XMM lidos como zero |
| **Teste 7** | Reset de FPU na reciclagem de PID | Não | Novo processo com slot reciclado inicia zerado |
| **Teste 8** | Compositor Aladdin compilado | Não | Desktop opera normalmente a 60 FPS |
| **Teste 9** | Drivers PS/2 e VirtIO com soft-float | Não | 100% de precisão sob tempestade de IRQs |
