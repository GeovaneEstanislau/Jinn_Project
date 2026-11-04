# JINN KERNEL SECURITY REVIEW 0.3.0
## Auditoria de Fronteiras de Privilégio, Isolamento de Processos e Robustez Defensiva

> **Projeto:** Jinn Microkernel / Aladdin Ecosystem  
> **Versão:** 0.3.0 Hardening Milestone  
> **Classificação:** Revisão de Segurança do Kernel (Ring 0 / Ring 3)  
> **Data:** Novembro de 2026  
> **Modelo de Ameaça:** Aplicações de Usuário Não Confiáveis (Ring 3 Adversarial)

---

## 1. Modelo de Confiança e Fronteiras Arquiteturais

O modelo de segurança do **Jinn Microkernel** estabelece que o código executado no espaço de usuário (**Ring 3**) é inerentemente não confiável. Aplicações gráficas, clientes de rede e shells de comando não possuem privilégios de hardware e dependem exclusivamente da porta de chamadas de sistema `int 0x80` para solicitar serviços ao núcleo (**Ring 0**).

```mermaid
flowchart TD
    subgraph "Ring 3 (Inseguro / Não Confiável)"
        U1["Processo A (Aladdin Browser)"]
        U2["Processo B (Aladdin Terminal)"]
        U3["Processo C (Malicioso / Adversarial)"]
    end

    subgraph "Hardware & MMU Barriers"
        G1["CPL=3 Enforcement (#GP em instruções de I/O e CR3)"]
        G2["Paging Isolation (CR3 Switch, USER bit = 0 no Kernel)"]
        G3["Syscall Gate (validate_user_buffer + Bounds Check)"]
    end

    subgraph "Ring 0 (Confiável / Seguro)"
        K1["Microkernel Core (Memory, Scheduler, IPC)"]
        K2["VFS & Storage (FAT32, Ext2, Volume Manager)"]
        K3["Network Stack & JinnWall SPI"]
    end

    U1 -.->|Bloqueado| U2
    U3 -->|Tentativa de acesso ilegal| G1
    U3 -->|Tentativa de ler memória alheia| G2
    U3 -->|Ponteiro inválido via syscall| G3
    G1 -->|#GP Fault (SIGSEGV)| U3
    G2 -->|#PF Fault (SIGSEGV)| U3
    G3 -->|Retorna -EFAULT (-14)| U3
    G3 ===>|Syscall Válida e Auditada| K1
```

---

## 2. Auditoria das Fronteiras de Segurança

### 2.1. Isolamento de Privilégio (Ring 0 vs. Ring 3)
- **Instruções Privilegiadas:** A execução de instruções restritas no Ring 3 (`cli`, `sti`, `invlpg`, `mov %cr3, %rax`, `wrmsr`, `in`, `out`) é imediatamente interceptada pela CPU via **Falta de Proteção Geral (#GP - Vector 13)**. O microkernel encerra a tarefa ofensora sem desestabilizar o restante do sistema.
- **Proteção de Tabelas de Páginas:** Todas as entradas de PML4, PDPT e tabelas de páginas que mapeiam a imagem do microkernel e estruturas de controle possuem o bit `USER` desativado (`USER = 0`). Qualquer tentativa de leitura ou escrita originada no Ring 3 causa uma **Falta de Página (#PF - Vector 14)** com violação de privilégio.

### 2.2. Validação Rigorosa de Ponteiros de Usuário
A rotina [validate_user_buffer](file:///e:/Projetos/Jinn/src/syscall.rs#L258) impede ataques de injeção de ponteiros:
- **Ponteiro Nulo e Comprimento Zero:** Rejeitados com `-EINVAL` (`-22`).
- **Ponteiros do Espaço Superior do Kernel:** Endereços $\ge$ `0x0000_8000_0000_0000` geram `-EFAULT` (`-14`).
- **Overflow Aritmético:** Cálculos `addr + len` com saturação de 64 bits são barrados preventivamente.
- **PML4 Walk Ativo:** O kernel inspeciona fisicamente todas as páginas do buffer no CR3 da tarefa chamadora, garantindo que as páginas estejam presentes e tenham as permissões requeridas (leitura ou escrita).

### 2.3. Isolamento e Contenção entre Processos
- **Isolamento de Memória:** Cada processo possui sua árvore hierárquica PML4 própria. Não existem pontes de memória não mapeadas ou visibilidade cruzada de páginas físicas entre tarefas independentes.
- **Tabela de Descritores de Arquivos:** Cada TCB aloca um array restrito de 8 posições (0..7). Índices arbitrários passados por processos (negativos, $\ge 8$, ou não alocados) retornam `-EBADF` (`-9`). O Processo A não tem capacidade de ler ou fechar descritores do Processo B.
- **Hierarquia de Processos e `sys_waitpid`:**
  - Apenas o processo pai detém a prerrogativa de coletar o estado de terminação de sua tarefa filha.
  - Chamadas de `sys_waitpid` realizadas por terceiros em PIDs não relacionados retornam `-ECHILD` (`-10`), impedindo ataques de hijacking de tarefas e vazamento de status de execução.

### 2.4. Integridade de Armazenamento e Políticas de Acesso
- **Volumes Somente Leitura:** Tentativas de criação (`sys_mkdir`), escrita (`sys_write`) ou renomeação (`sys_rename`) em volumes montados com a flag `is_read_only = 1` retornam compulsoriamente `-EROFS` (`-30`).
- **Cotas de Armazenamento:** Operações de escrita em volumes que ultrapassam as cotas limites configuradas disparam o erro `-EDQUOT` (`-122`), comprovado formalmente pelo teste adversarial pré-existente.

### 2.5. Segurança de Rede e JinnWall SPI
- O subsistema **JinnWall** inspeciona o estado das conexões TCP e UDP na borda da pilha de rede.
- Sob o perfil `PublicSecure`, pacotes de entrada não correlacionados a conexões de saída previamente estabelecidas (*unsolicited inbound packets*) são imediatamente descartados, contabilizados e registrados na telemetria do sistema.

---

## 3. Matriz de Vetores de Ataque e Resoluções

| Vetor de Ataque / Ameaça | Superfície Afetada | Contramedida no Microkernel | Resultado Observado |
|---|---|---|---|
| **Pointer Confused Deputy** | Syscalls com buffers (`read`, `write`, `stat`) | `validate_user_buffer` com PML4 page walk | Abortado com `-EFAULT` |
| **Buffer Overflow Aritmético**| Syscalls de buffer (`addr + len` wrap) | `checked_add` contra `USER_SPACE_MAX` | Abortado com `-EFAULT` |
| **Esgotamento de Descritores** | `sys_open` em loop infinito | Limite estrito de 8 FDs por tarefa | Retorna `-EMFILE` ordenado |
| **Zombie Hijacking** | `sys_waitpid` por PID arbitrário | Verificação estrita de parentesco `parent_pid` | Rejeitado com `-ECHILD` |
| **DNS Cache Poisoning / Spoof**| Resolução de nomes `sys_dns_lookup` | Validação estrita de ID de transação UDP | Descarte e erro de spoofing |
| **Invasão de Memória do Kernel**| Desreferenciação direta de `0xFFFF...` | Bit `USER=0` em páginas Ring 0 pela MMU | CPU emite `#PF` e encerra tarefa |

---

## 4. Conclusão

O Jinn Microkernel 0.3.0 alcançou maturidade de segurança exemplar. A combinação de barreiras de hardware x86_64, paginação isolada por processo, checagem contratual sistemática em Ring 0 e isolamento de descritores garante uma plataforma robusta e protegida contra explorações maliciosas.
