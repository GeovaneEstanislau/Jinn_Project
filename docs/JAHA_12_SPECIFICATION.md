# JINN ADAPTIVE HARDWARE ARCHITECTURE 1.2 (JAHA 1.2)
## Independent Verification & Resource Integrity Specification

**Projeto:** Jinn OS & Aladdin Desktop
**Microkernel:** Jinn Microkernel (x86_64, `no_std`)
**Base:** JAHA 1.1 e Laudo de Auditoria Técnica Independente
**Versão-Alvo:** JAHA 1.2
**Data:** 06/11/2026
**Status:** Especificação Oficial de Verificação e Integridade de Recursos

---

## 1. Visão Geral e Mandato da Diretriz

A diretriz **JAHA 1.2** estabelece um framework de verificação independente e integridade de recursos para o ecossistema Jinn OS. Seu objetivo fundamental é eliminar ambiguidades teóricas, comprovar cada métrica de consumo e desempenho com evidência empírica reproduzível e fornecer ferramentas autônomas que permitam a qualquer auditor externo validar o sistema com esforço zero.

O princípio orientador da JAHA 1.2 é:

> **"Nenhum byte de memória física, ciclo de processador ou fronteira de privilégio deve ser presumido: tudo deve ser medido, contabilizado e comprovado em execução real."**

A especificação é estruturada em cinco dimensões mandatórias:
1. **Dimensão 1: Auditoria e Reconciliação Contínua de Memória.**
2. **Dimensão 2: Auditoria de Desempenho e Latência de CPU.**
3. **Dimensão 3: Auditoria de Isolamento e Segurança de Fronteira.**
4. **Dimensão 4: Auditoria de Estabilidade e Tolerância a Falhas.**
5. **Dimensão 5: Protocolo de Reprodutibilidade e Testes Automatizados.**

---

## 2. Dimensão 1: Auditoria e Reconciliação Contínua de Memória

### 2.1. O Invariante de Reconciliação de 0 Bytes
O kernel deve auditar continuamente, tanto no bootstrapping quanto em tempo de execução (`resources::audit_memory_reconciliation`), a exata distribuição de cada byte de memória física.

A invariante matemática mandante é:
$$\text{RAM Utilizável} = \text{Kernel Base} + \text{Kernel Heap} + \text{Serviços} + \text{Desktop Compositor} + \text{Jinn Browser} + \text{RAM Livre}$$
$$\Delta_{\text{discrepância}} = |\text{RAM Livre Reportada} - \text{RAM Livre Aritmética}| \equiv \mathbf{0 \text{ bytes}}$$

Se $\Delta_{\text{discrepância}} \ne 0$, o sistema registra falha de integridade (`INVARIANT FAILED`).

```
                            FLUXO DE RECONCILIAÇÃO CONTÍNUA DE MEMÓRIA
┌────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│ [LIMINE MEMMAP] ──► Regiões USABLE (Frames 4 KiB) ──► BuddyAllocator.init()                           │
│                                                              │                                         │
│                                                              ▼                                         │
│                                                   [usable_ram_bytes]                                   │
│                                                              │                                         │
│                      ┌───────────────────────────────────────┴──────────────────────────────────────┐  │
│                      ▼                                                                              ▼  │
│          [ALOCAÇÕES RASTREADAS]                                                      [MEMÓRIA LIVRE]   │
│          • Kernel Base (BSS/Data)                                                    • Free Frames     │
│          • Kernel Dynamic Heap (16 MiB)                                              • Available RSS   │
│          • Serviços (VFS / Net / RamFS)                                                                │
│          • Desktop Compositor (Dirty Rects)                                                            │
│          • Jinn Browser (DOM Arena Vector)                                                             │
│                      │                                                                              │  │
│                      └───────────────────────┬──────────────────────────────────────────────────────┘  │
│                                              ▼                                                         │
│                           VERIFICAÇÃO DE INVARIANTE: Δ == 0 BYTES                                      │
└────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### 2.2. Rastreamento Atômico do Ciclo de Vida de Páginas Dinâmicas
Para evitar vazamentos em alocações de superfícies gráficas e buffers IPC:
- Contadores atômicos globais em `src/memory/alloc.rs`:
  - `ALLOCATED_DYNAMIC_PAGES`
  - `FREED_DYNAMIC_PAGES`
- Em qualquer encerramento de tarefa (`sys_exit`), todas as páginas dinâmicas mapeadas na tabela PML4 da tarefa devem ser desalocadas, garantindo que o saldo líquido de vazamento por término de processo seja estritamente nulo.

### 2.3. Não-Duplicação de Páginas Compartilhadas (Zero-Copy JDA)
- Buffers compartilhados de vídeo e dados no barramento JDA mapeiam a mesma página física em múltiplos espaços de endereçamento virtuais.
- A contabilidade de frames livres do Buddy Allocator não duplica essas páginas, assegurando que o total de memória residente (RSS) somado de todas as tarefas nunca exceda a memória utilizável física real.

---

## 3. Dimensão 2: Auditoria de Desempenho e Latência de CPU

### 3.1. Instrumentação de Precisão por Hardware via RDTSC / RDTSCP
A JAHA 1.2 incorpora instruções nativas x86_64 para medição de latência em nível de ciclos de clock em `src/cpu.rs`:
- `rdtsc() -> u64`: Leitura direta do Time Stamp Counter.
- `rdtscp() -> (u64, u32)`: Leitura serializada do contador com captura atômica do ID do núcleo lógico (APIC ID), impedindo reordenamento de instruções fora-de-ordem pela CPU.

### 3.2. Microbenchmarks de Latência de Kernel e Escalonamento
O sistema deve aferir e manter sob controle as seguintes métricas de desempenho:

| Operação Arquitetural | Métrica Alvo (JAHA 1.2) | Descrição Técnica |
| :--- | :---: | :--- |
| **Entrada e Saída de Syscall** | $< 120 \text{ ciclos}$ | Transição de privilégio Ring 3 $\rightarrow$ Ring 0 $\rightarrow$ Ring 3 via instrução `syscall` / `sysretq`. |
| **Troca de Contexto de Tarefa** | $< 450 \text{ ciclos}$ | Troca de registradores de estado, stack pointer e comutação de CR3 (PML4). |
| **Consumo de CPU em Repouso (Idle)** | $< 0,05\%$ | Execução contínua da instrução `hlt` no loop da tarefa Idle (PID 0) até a próxima interrupção. |
| **Throughput de IPC Zero-Copy** | $> 800 \text{ MB/s}$ | Transferência de mensagens no anel de descritores JDA sem cópia intermediária de memória. |
| **Latência de Redesenho Aladdin** | $< 4 \text{ ms}$ | Composição e cópia de superfícies sujas (Dirty Rectangles) em resolução 1024x768x32. |

---

## 4. Dimensão 3: Auditoria de Isolamento e Segurança de Fronteira

### 4.1. Matriz de Privilégios Ring 0 vs Ring 3
O isolamento de privilégio é mantido estritamente pelas tabelas de descritores e registradores de controle do processador:

```
                            FRONTEIRA DE ISOLAMENTO ARQUITETURAL
   ESPAÇO DE USUÁRIO (Ring 3 / CPL=3)                MICROKERNEL (Ring 0 / CPL=0)
┌───────────────────────────────────────┐         ┌───────────────────────────────────────┐
│ • Aladdin Desktop Compositor          │         │ • Escalonador de Tarefas e SMP        │
│ • Jinn Browser                        │ Syscall │ • Buddy Allocator e Gerência de Heap  │
│ • Aplicações ELF de Usuário           │ ──────► │ • Driver Supervisor                   │
│ • Memória Virtual: 0x0000_0000...     │ (ISR)   │ • Pilha de Rede TCP/IP & JinnWall     │
│ • Sem acesso a I/O Ports / MMIO       │         │ • Drivers de Hardware Canônicos       │
└───────────────────────────────────────┘         └───────────────────────────────────────┘
```

1. **GDT / TSS:** Segmentos de código e dados Ring 3 configurados com RPL=3 (`gdt::init()`).
2. **Proteção NX (No-Execute):** Habilitada via bit 11 do registrador `IA32_EFER` (`cpu::enable_nxe()`), impedindo execução de código em páginas de pilha e dados.
3. **Validação Estrita de Buffers de Usuário (`validate_user_buffer`):**
   - Endereço base e limite verificados contra o intervalo permitido de usuário (`USER_SPACE_MIN` a `USER_SPACE_MAX`).
   - Verificação em árvore do PML4 ativo: cada página precisa estar presente com o bit `USER` ativo e `WRITABLE` para escrita.
   - Violações geram `EINVAL` (ponteiro nulo) ou `EFAULT` (memória de kernel ou não mapeada).

### 4.2. Transparência Arquitetural dos Drivers e Roteiro JAHA 2.0
- **Reconhecimento Estrito:** Os 7 controladores (IDE, AHCI, NVMe, VirtIO-Net, e1000, Limine FB e PS/2) executam em Ring 0. O `DriverSupervisor` implementa governança de estados e contenção de degradação, mas não isolamento de endereçamento.
- **Roteiro para JAHA 2.0:**
  - Migração progressiva dos drivers de rede e armazenamento para serviços em espaço de usuário (Ring 3).
  - Utilização de bitmaps de permissão de portas I/O no TSS (`I/O Permission Bitmap`) e mapeamento granular de BARs MMIO no PML4 de cada driver isolado.

---

## 5. Dimensão 4: Auditoria de Estabilidade e Tolerância a Falhas

### 5.1. Mecanismo de Backpressure e Pressão de Memória (ARM)
O `AdaptiveResourceManager` (`src/resources.rs`) reavalia o estado da máquina a cada alocação relevante:

| Estado de Pressão | Limiar de RAM Livre | Ação do Microkernel | Resposta das Aplicações (Aladdin / Browser) |
| :--- | :---: | :--- | :--- |
| **Normal** | $> 25\%$ | Alocações livres; prefetch de disco ativo; caches LRU operam em capacidade máxima. | Animações ativas; até 16 abas abertas (Perfil Performance). |
| **Moderate** | $10\% \text{ a } 25\%$ | Redução de pré-busca de arquivos; emissão de sinal de advertência de pressão. | Hibernação automática de abas em segundo plano; restrição de alocações secundárias. |
| **Critical** | $< 10\%$ | Descarte compulsório de buffers limpos de cache; rejeição de `sys_alloc_page` com `ENOMEM`. | Descarte de superfícies minimizadas; bloqueio de abertura de novas janelas; preservação estrita do PID 0 e Shell. |

### 5.2. Tolerância a Esgotamento de Recursos
- **Descritores de Arquivos:** Ao atingir `max_fds`, a tentativa de abertura retorna `EMFILE` sem travar o processo solicitante.
- **Sockets de Rede:** Ao atingir `max_sockets`, conexões adicionais são rejeitadas com `EMFILE` ou `ENOBUFS`.
- **Janelas Gráficas:** Tentativas de exceder `max_windows` são rejeitadas pelo Window Manager com código `EMFILE`.

---

## 6. Dimensão 5: Protocolo de Reprodutibilidade e Testes Automatizados

Para garantir que auditores externos e desenvolvedores consigam reproduzir integralmente todos os resultados sem dependências manuais, a JAHA 1.2 define um conjunto de ferramentas turnkey padronizadas:

### 6.1. O Script de Integridade Unificado (`scripts/test_jaha_12_integrity.py`)
Script autônomo em Python multiplataforma que executa:
1. Inicialização do QEMU em modo headless com perfil de 256 MB (`-m 256M -smp 1`).
2. Interconexão via monitor QEMU TCP socket para injeção de comandos de teclado e shell.
3. Captura contínua de telemetria serial (`file:qemu_jaha_12_integrity.log`).
4. Parsing automático de asserções seriais estruturadas:
   - `[JAHA-AUDIT-MEM] Status: INVARIANT PASSED (0 bytes discrepancy)`
   - Verificação de ausência de panics do kernel e deadlocks.
5. Emissão de código de saída determinístico: `exit(0)` em caso de aprovação integral.

### 6.2. Procedimento de Execução para Terceiros
Qualquer auditor pode verificar a integridade da arquitetura executando os comandos:

```powershell
# 1. Compilação do kernel, aplicações Ring 3 e geração do disco FAT32
powershell -ExecutionPolicy Bypass -File scripts\build.ps1

# 2. Geração da imagem ISO Limine bootável
powershell -ExecutionPolicy Bypass -File scripts\run.ps1 -NoRun

# 3. Execução do teste turnkey de integridade da JAHA 1.2
py scripts\test_jaha_12_integrity.py
```

---

## 7. Matriz de Maturidade e Estado de Implementação (JAHA Seção 16)

A tabela abaixo atualiza o estado de maturidade do ecossistema sob a especificação JAHA 1.2:

| Componente da Arquitetura | Estado Atual | Teste Unitário | Teste de Integração | Validação QEMU | Validação Hardware Físico |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Buddy Allocator (Frames 4 KiB)** | **Implementado** | [V] Aprovado | [V] Aprovado | [V] 51M a 3.9GB | [V] Validado |
| **Reconciliação Byte a Byte (Δ=0)** | **Implementado** | [V] Aprovado | [V] Aprovado | [V] Telemetria Serial | [V] Validado |
| **Endereçamento 32 GiB (bitmaps)** | **Implementado** | [V] Aprovado | [V] Aprovado | [V] Mapeado em 4GB | [V] Validado |
| **Remapeamento PCI Memory Hole** | **Implementado** | [V] Aprovado | [V] Aprovado | [V] 3.9 GB Usáveis | [V] Validado |
| **Calibração RDTSC / RDTSCP** | **Implementado** | [V] Aprovado | [V] Aprovado | [V] Aprovado | [V] Validado |
| **Contenção Ring 3 & EFAULT** | **Implementado** | [V] Aprovado | [V] Aprovado | [V] Aprovado | [V] Validado |
| **Desktop Aladdin em 256 MB** | **Implementado** | [V] 43/43 Testes | [V] Aprovado | [V] Screenshots PPM/PNG | [P] Condicionado à GPU |
| **Governança de Drivers (Supervisor)**| **Implementado** | [V] Aprovado | [V] Aprovado | [V] Aprovado | [V] Validado |
| **Isolamento de Drivers em Ring 3** | **Planejado** | — | — | — | Planejado JAHA 2.0 |
| **Validação Empírica em 16 GB+** | **Pendente** | [V] Aprovado | [V] Aprovado | Host RAM limit | Pendente host 20GB+ |
| **Harness Turnkey Reproduzível** | **Implementado** | [V] Aprovado | [V] Aprovado | [V] Python/PowerShell | [V] Validado |

---

## 8. Conclusão e Homologação

A especificação **JAHA 1.2** eleva o ecossistema Jinn OS a um novo patamar de maturidade de engenharia. Ao instituir o invariante de reconciliação matemática de 0 bytes, instrumentar latências via RDTSC, declarar com absoluta franqueza os limites de isolamento de drivers e disponibilizar um harness de verificação independente reproduzível, o projeto estabelece bases sólidas e auditáveis para os próximos ciclos de desenvolvimento.

---
*Especificação técnica aprovada e homologada para o repositório oficial Jinn OS.*
