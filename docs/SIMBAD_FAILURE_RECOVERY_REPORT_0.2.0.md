# JINN PROJECT — SIMBAD FAILURE RECOVERY & RESILIENCE 0.2.0
## Mecanismos de Tolerância a Falhas, Recuperação de Daemons e Consistência Pós-Queda

**Documento:** Relatório Técnico de Tolerância a Falhas  
**Marco:** Simbad Server Foundation 0.2.0  
**Data:** Novembro de 2026  
**Status:** Auditado e Validado  

---

# 1. Visão Geral da Arquitetura de Tolerância a Falhas

Em sistemas operacionais de missão crítica e servidores, o isolamento de falhas requer que o microkernel e seus subsistemas de supervisão sobrevivam ao colapso de daemons individuais, garantindo continuidade dos serviços essenciais.

O Simbad 0.2.0 estabelece quatro camadas de contenção de falhas:
1. **Contenção no Espaço de Usuário:** Falhas em daemons Ring 3 geram códigos de saída formais (`exit_code`) ou exceções capturadas pelo kernel sem causar *General Protection Fault* (GPF) no anel 0.
2. **Supervisão com Backoff Exponencial:** O supervisor detecta o encerramento da tarefa e orquestra a reinicialização progressiva, evitando exaustão de PIDs.
3. **Disjuntor de Proteção (*Circuit Breaker*):** Interrompe o reinício automático caso o serviço atinja 5 falhas consecutivas, prevenindo saturação da CPU.
4. **Recuperação de Armazenamento e Barreira FAT32:** O driver de blocos e o VFS preservam a integridade dos metadados através de ordenação de escrita e reescrita de clusters.

---

# 2. Taxonomia de Falhas e Estratégias de Recuperação

| Cenário de Falha | Componente Afetado | Detecção | Resposta do Sistema | Tempo de Recuperação |
| :--- | :---: | :---: | :--- | :---: |
| **Crash Inesperado de Daemon** | `jinn-httpd` / `jinn-netmgr` | Scheduler / Zombie state | Supervisor agenda reinício com backoff (1s -> 2s -> 4s...) | 1.000 a 16.000 ms |
| **Crash Loop Infinito** | Qualquer serviço | Contador `consecutive_failures` | Desarme de disjuntor (*Circuit Breaker*), transição para `FAILED` | Imediato (bloqueia loop) |
| **Exaustão de Memória (`ENOMEM`)** | Aplicação / Daemon | `check_and_alloc_frames` | Rejeição atômica da alocação, retorno `-ENOMEM` (-12) sem pânico | 0 ms (não trava kernel) |
| **Exaustão de Descritores (`EMFILE`)** | Sockets de Rede | `check_and_open_socket` | Rejeição imediata, conexão nova é descartada ou posta em espera | 0 ms |
| **Queda Abrupta de Energia** | Cache de Storage | Bootloader / Mount Manager | Preservação dos setores gravados até o último ciclo de flush de 5s | Bounded: $\le 5.000\text{ ms}$ |
| **Parada Inválida de Dependência** | DAG de Serviços | `stop_service` | Rejeição imediata da chamada se houver dependente ativo | 0 ms (prevenção) |

---

# 3. Protocolo de Recuperação do Administrador (`jinnctl reset`)

Quando um serviço tem seu *Circuit Breaker* desarmado (estado `FAILED`), o operador do servidor pode inspecionar os logs estruturados e restaurar o serviço:

```bash
# 1. Inspeciona os registros de falha e causa raiz
jinnctl logs

# 2. Verifica os contadores e estado do serviço
jinnctl services

# 3. Reseta os contadores de falha e rearma o Circuit Breaker
jinnctl reset <servico>

# 4. Inicia novamente o serviço com dependências satisfeitas
jinnctl start <servico>
```

---

# 4. Evidências Experimentais de Recuperação

1. **Recuperação de Falha Repetida:**
   - No teste `test_supervisor_exponential_backoff_and_circuit_breaker`, o serviço `flaky-daemon` foi forçado a falhar sucessivamente.
   - O supervisor registrou tempos de espera progressivos de 1s, 2s, 4s, 8s e 16s.
   - Na 6ª falha, o disjuntor desarmou, travando o serviço em `FAILED` e impedindo qualquer tentativa espúria de inicialização até o reset administrativo explícito.
2. **Recuperação de Persistência:**
   - No teste `test_storage_reliability_flusher_and_crash_consistency`, a queda de energia simulada confirmou que os blocos descarregados antes do corte permaneceram bit a bit intactos após reinicialização da máquina virtual.
