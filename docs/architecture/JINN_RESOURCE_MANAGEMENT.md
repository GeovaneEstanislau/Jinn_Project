# JINN RESOURCE MANAGEMENT (JRM) 1.0
## Governança Central de Memória, CPU, Caches e Backpressure Adaptativo

**Projeto:** Jinn OS  
**Subcomponente:** Adaptive Resource Manager (`src/resources.rs`)  
**Versão:** 1.0  
**Data:** 06/11/2026  

---

## 1. Visão Geral

O **Adaptive Resource Manager (ARM)** atua como a autoridade central do microkernel Jinn para calibrar a utilização de recursos conforme a pressão dinâmica e o perfil de hardware da máquina:

- **Alocação Preditiva e Bounded:** Nenhum processo tem permissão para alocar memória, descritores ou buffers arbitrariamente sem auditoria de cotas.
- **Backpressure Reativo:** O microkernel comunica proativamente o estresse de memória às camadas de usuário e serviços de fundo antes que ocorra esgotamento físico.

---

## 2. Níveis de Pressão de Memória e Ações de Contenção

```
                    NÍVEIS DINÂMICOS DE PRESSÃO DE MEMÓRIA
+---------------------------------------------------------------------------------+
| NORMAL: Memória livre > 25% da RAM utilizável                                   |
| • Operação sem restrições. Caches de blocos e janelas alocadas sob demanda.    |
+---------------------------------------------------------------------------------+
                                        │
                                        ▼ (RAM livre cai abaixo de 25%)
+---------------------------------------------------------------------------------+
| MODERATE: Memória livre entre 10% e 25%                                         |
| • Redução de pré-busca de I/O em disco.                                         |
| • Ativação de hibernação automática de abas em segundo plano no Jinn Browser.   |
| • Notificação de advertência para daemons e servidores em background.           |
+---------------------------------------------------------------------------------+
                                        │
                                        ▼ (RAM livre cai abaixo de 10%)
+---------------------------------------------------------------------------------+
| CRITICAL: Memória livre < 10%                                                   |
| • Esvaziamento forçado de buffers limpos do cache LRU de blocos.               |
| • Descarte de superfícies gráficas de janelas minimizadas.                      |
| • Bloqueio de novas chamadas sys_alloc_page com retorno imediato de -ENOMEM.    |
| • Preservação estrita dos processos essenciais (PID 0, Shell, Supervisor).      |
+---------------------------------------------------------------------------------+
```

---

## 3. Matriz de Cotas de Recursos por Perfil de Hardware

| Perfil | Frames Físicos Máx / Processo | Memória Máx / Processo | Descritores FDs | Janelas Gráficas | Buffers JDA | Sockets |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **Legacy (256 MB)** | 1.024 frames | 4 MiB | 8 | 2 | 2 | 4 |
| **Compact (512 MB)** | 2.048 frames | 8 MiB | 12 | 4 | 4 | 8 |
| **Standard (1–4 GB)**| 4.096 frames | 16 MiB | 16 | 6 | 8 | 16 |
| **Performance (4–16 GB)** | 16.384 frames | 64 MiB | 32 | 8 | 16 | 32 |
| **High-End (16 GB+)** | 65.536 frames | 256 MiB | 64 | 16 | 32 | 64 |

---

## 4. Auditoria e Telemetria em Tempo Real (Syscall 25)

Qualquer processo Ring 3 com permissão de auditoria pode inspecionar sua própria contabilidade através da Syscall 25 (`sys_res_stat`), obtendo:
- `allocated_frames`: Frames físicos atualmente mapeados.
- `peak_allocated_frames`: Pico histórico de consumo.
- `cpu_ticks`: Tempo de CPU efetivamente consumido.
- `limit_violations`: Tentativas de ultrapassar cotas barradas pelo microkernel com `-ENOMEM`, `-EMFILE` ou `-ENOBUFS`.

