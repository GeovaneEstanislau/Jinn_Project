# JINN PROJECT — SIMBAD RESOURCE ISOLATION & QUOTAS 0.2.0
## Isolamento de Recursos, Cotas Operacionais e Prevenção contra Exaustão

**Documento:** Relatório Técnico de Recursos e Quotas  
**Marco:** Simbad Server Foundation 0.2.0  
**Data:** Novembro de 2026  
**Status:** Auditado e Validado  

---

# 1. Princípios de Isolamento no Kernel Jinn

Em sistemas de servidor multi-serviço, um daemon instável ou mal-intencionado não pode comprometer a estabilidade dos demais daemons ou provocar pânico de kernel (*Kernel Panic*).

No microkernel Jinn, os limites de recursos são **estritamente aplicados e verificados pelo próprio kernel** no ponto de entrada das chamadas de sistema, e não delegados à boa vontade dos processos de usuário.

---

# 2. Matriz de Cotas e Limites de Recursos

A estrutura [`ResourceLimits`](file:///e:/Projetos/Jinn/src/resources.rs#L10) define as cotas máximas para cada tarefa:

| Recurso do Sistema | Limite Usuário Ring 3 (`user_default`) | Limite Kernel Ring 0 (`kernel_default`) | Efeito da Violação de Limite |
| :--- | :---: | :---: | :--- |
| **Memória Física (Frames 4 KiB)** | 128 frames (~512 KiB) | 65.536 frames (256 MiB) | Retorna `-ENOMEM` (-12) sem alocar |
| **Descritores de Arquivos (`MAX_TASK_FDS`)** | 8 descritores | 8 descritores | Retorna `-EMFILE` (-24) |
| **Sockets de Rede** | 4 sockets | 16 sockets | Retorna `-EMFILE` (-24) |
| **Buffers Zero-Copy (JDA)** | 4 buffers | 16 buffers | Retorna `-ENOBUFS` (-105) |
| **Janelas Gráficas (Aladdin)** | 4 janelas | 8 janelas | Retorna `-EMFILE` (-24) |
| **Cota de CPU (`max_cpu_ticks`)** | Configurável (0 = ilimitado) | Ilimitado | **Terminação Forçada:** encerra processo com `-EDQUOT` (-122) |

---

# 3. Mecanismos de Aplicação no Kernel

### A. Limite de Memória e Recuperação de `ENOMEM`
- Toda chamada a `sys_alloc_page()` ou expansão de heap executa:
  [`res.check_and_alloc_frames(count, &limits)`](file:///e:/Projetos/Jinn/src/resources.rs#L87).
- Se a soma dos frames ultrapassar `limits.max_frames`, a operação é cancelada, o contador `limit_violations` é incrementado, e o código de erro `-ENOMEM` é retornado à aplicação chamadora. O kernel permanece 100% estável e não sofre exaustão de páginas.

### B. Limite de Descritores e Sockets (`EMFILE`)
- Ao abrir novos arquivos (`sys_open`) ou criar sockets de rede (`sys_socket`), o kernel valida [`check_and_open_fd`](file:///e:/Projetos/Jinn/src/resources.rs#L107) e [`check_and_open_socket`](file:///e:/Projetos/Jinn/src/resources.rs#L186).
- Sockets órfãos ou conexões abortadas antes do `sys_accept` são limpos automaticamente pelo listener sem consumir descritores ativos da tarefa.

### C. Enforçamento de Quotas de CPU (`max_cpu_ticks`)
- No escalonador preemptivo ([`src/scheduler.rs`](file:///e:/Projetos/Jinn/src/scheduler.rs#L1240-L1270)), a cada interrupção de timer de hardware, o kernel avalia:
  ```rust
  if task.limits.max_cpu_ticks > 0 && task.cpu_ticks >= task.limits.max_cpu_ticks {
      self.exit_task_locked(cur, -122); // -EDQUOT (cota de CPU esgotada)
  }
  ```
- Processos que monopolizam a CPU ou entram em loops não-cooperativos são interrompidos preventivamente e transicionam para `TaskState::Zombie` com código de erro `-122`.

### D. Tarefas Bloqueadas e Dormindo (Custo Zero de CPU)
- Tarefas no estado `TaskState::Blocked` ou `TaskState::Sleeping(until_tick)` são ignoradas pelo escalonador Round-Robin, consumindo **zero ciclos de CPU** enquanto aguardam pacotes de rede ou temporizadores.

---

# 4. Evidências de Validação Automatizada

- **Arquivo de Testes:** [`tools/jda-sim/tests/simbad_runtime_tests.rs`](file:///e:/Projetos/Jinn/tools/jda-sim/tests/simbad_runtime_tests.rs#L622-L658)
- **Caso de Teste:** `test_kernel_resource_isolation_and_quotas`
- **Resultados Observados:**
  1. Alocação de 3 frames (dentro do limite 4) -> `Ok(())`.
  2. Alocação de +2 frames (total 5 > 4) -> Rejeição imediata com `-ENOMEM` (-12) e registro de violação.
  3. Abertura de 2 arquivos -> `Ok(())`; 3º arquivo rejeitado com `-EMFILE` (-24).
  4. Abertura de 2 sockets -> `Ok(())`; 3º socket rejeitado com `-EMFILE` (-24).
- **Resultado:** **Validado e Aprovado com 100% de precisão.**
