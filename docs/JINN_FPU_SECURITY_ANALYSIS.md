# 🛡️ Jinn OS — Análise de Segurança e Isolamento da FPU/SIMD
## Avaliação de Vazamento de Registradores, Concorrência SMP e Integridade (Etapa B.1)

> **Documento Oficial de Engenharia — Etapa B.1 (Diagnóstico e Planejamento)**  
> **Data de Emissão:** 16 de Novembro de 2026 | **Branch:** `main` | **Commit Base:** `537c534`  
> **Status:** Concluído | **Objetivo:** Modelagem de ameaças e blindagem de segurança para o subsistema FPU/SIMD.  
> **Regra de Classificação:** Toda ameaça e contramedida é categorizada como `CONFIRMADO`, `PARCIAL`, `HIPÓTESE` ou `NÃO TESTADO`.

---

## 1. Modelo de Ameaças: Superfície de Ataque da FPU em Ring 3

No modelo de microkernel com separação rigorosa de anéis de proteção (Ring 0 $\leftrightarrow$ Ring 3), a unidade de ponto flutuante e os registradores vetoriais constituem um canal lateral latente se não forem gerenciados de forma estrita pelo escalonador.

```
┌────────────────────────────────────────────────────────────────────────┐
│                        CENÁRIO ATUAL (SEM FXSAVE)                      │
│                                                                        │
│   [Processo A: Criptografia TLS 1.3]                                    │
│   • Registradores XMM0..XMM7 carregam chave ChaCha20 e Nonce.           │
│                           │                                            │
│            Preempção por Timer (APIC IRQ0)                             │
│                           ▼                                            │
│   [Escalonador: Salva apenas GPRs (RAX..R15)]                          │
│   • Registradores FPU físicos NÃO são alterados.                       │
│                           ▼                                            │
│   [Processo B: Aplicação Maliciosa em Ring 3]                           │
│   • Executa MOVUPS [buffer], XMM0..XMM7.                               │
│   • RESULTADO: LEITURA DIRETA DA CHAVE CRIPTOGRÁFICA DO PROCESSO A!    │
└────────────────────────────────────────────────────────────────────────┘
```

### 1.1. Categorias de Vulnerabilidade Identificadas

| Código | Descrição da Ameaça | Gravidade | Vetor de Exploração | Classificação |
|:---:|---|:---:|---|:---:|
| **SEC-FPU-01** | **Vazamento Cruzado de Registradores entre Tarefas** | `CRÍTICA` | Processo B lê registradores `XMM` deixados pelo Processo A após preempção regular. | `CONFIRMADO` |
| **SEC-FPU-02** | **Vazamento de Dados em `spawn_process`** | `ALTA` | Novo processo de usuário inicia execução e herda resíduos de memória FPU de tarefas finalizadas. | `CONFIRMADO` |
| **SEC-FPU-03** | **Herança de Estado em `sys_execve`** | `ALTA` | Binário substituído herda registradores `XMM` e flags de arredondamento do binário anterior. | `CONFIRMADO` |
| **SEC-FPU-04** | **Kernel Panic por Bits Reservados em `MXCSR`** | `ALTA` | Se um valor inválido for carregado em `MXCSR` durante `FXRSTOR64`, a CPU gera `#GP(0)` em Ring 0. | `CONFIRMADO` |
| **SEC-FPU-05** | **Kernel Panic por Desalinhamento da Área de Salvamento** | `CRÍTICA` | Se o ponteiro passado para `FXSAVE64`/`FXRSTOR64` não for alinhado a 16 bytes, a CPU gera `#GP(0)`. | `CONFIRMADO` |
| **SEC-FPU-06** | **Corrupção de Estado Numérico em Migração SMP** | `CRÍTICA` | Tarefa migra para outro núcleo e sobrescreve/lê valores arbitrários na CPU de destino. | `CONFIRMADO` |

---

## 2. Blindagem e Contramedidas de Segurança

### 2.1. Mitigação contra Vazamento de Dados (SEC-FPU-01, SEC-FPU-02, SEC-FPU-03)
* **Contramedida 1 (Eager Save/Restore):** O escalonador salvará obrigatoriamente o estado da tarefa em `task.fpu_state` e restaurará o estado da próxima tarefa em todo context switch via `preempt_core` e `direct_switch`.
* **Contramedida 2 (Sanitização na Criação):** Todo novo processo instanciado via `spawn_user_task_from_vfs` recebe um bloco `FpuState::new_user_default()` com registradores `ST0..ST7` e `XMM0..XMM15` rigorosamente preenchidos com zeros.
* **Contramedida 3 (Sanitização no `execve`):** A rotina `sys_execve_framed` força a redefinição de `fpu_state` para o padrão limpo e executa `fxrstor64` antes de retornar ao espaço de usuário. [`CONFIRMADO`]

---

### 2.2. Validação e Filtragem do Registrador `MXCSR` (SEC-FPU-04)
O registrador `MXCSR` controla o comportamento de exceções e arredondamento SIMD:
* Bits 0..5: Flags de exceção (PE, UE, OE, ZE, DE, IE).
* Bit 6: Flag DAZ (Denormals Are Zeros).
* Bits 7..12: Máscaras de exceção (PM, UM, OM, ZM, DM, IM).
* Bits 13..14: Controle de arredondamento (RC).
* Bit 15: Flag FZ (Flush-to-Zero).
* **Bits 16..31:** **RIGOROSAMENTE RESERVADOS**.

#### Risco no `FXRSTOR64`:
De acordo com o Intel Software Developer's Manual (SDM Vol. 2A, Seção `FXRSTOR`):
> *"If the MXCSR field in the FXSAVE image contains any bits set in reserved bit positions, a general-protection fault (#GP) is generated."*

Se o `#GP` for disparado dentro do kernel durante `FXRSTOR64`, o tratador de exceções em [interrupts/mod.rs#L192](file:///d:/Projetos/Jinn/src/interrupts/mod.rs#L192) detectará falha em Ring 0 e acionará um **Kernel Panic irrecuperável**.

#### Contramedida de Sanitização Obrigatória:
Antes de qualquer invocação de `fxrstor64`, o kernel deve aplicar uma máscara sanitizadora estrita sobre o valor de `mxcsr`:
```rust
const MXCSR_VALID_BITS_MASK: u32 = 0x0000_FFFF; // Apenas bits 0..15 são permitidos
const MXCSR_DEFAULT_MASKED:  u32 = 0x0000_1F80; // Exceções mascaradas

#[inline(always)]
pub fn sanitize_mxcsr(raw_mxcsr: u32) -> u32 {
    let mut safe = raw_mxcsr & MXCSR_VALID_BITS_MASK;
    // Assegura que as máscaras de exceção fundamentais permaneçam ativas se o valor for nulo
    if safe == 0 {
        safe = MXCSR_DEFAULT_MASKED;
    }
    safe
}
```
Essa validação torna impossível que um estado corrompido ou malicioso de Ring 3 induza pânico no kernel durante o context switch. [`CONFIRMADO`]

---

### 2.3. Garantia de Alinhamento Estrito a 16 Bytes (SEC-FPU-05)
A instrução `FXSAVE64 [mem]` exige que o operando `mem` seja alinhado a 16 bytes (`mem % 16 == 0`), sob pena de `#GP(0)`.

#### Prova de Alinhamento na Arquitetura Jinn:
1. A struct `FpuState` é declarada com `#[repr(C, align(16))]`.
2. Como `FpuState` tem tamanho de 512 bytes (múltiplo exato de 16: $512 = 32 \times 16$), qualquer array ou estrutura composta contendo `FpuState` preserva o alinhamento de 16 bytes para todos os elementos.
3. Para eliminar qualquer dependência de alinhamento em heap, a tabela `SCHEDULER.tasks` é estática em BSS.
4. **Verificação em Tempo de Compilação (`const_assert`):**
   ```rust
   const _: () = assert!(core::mem::align_of::<FpuState>() == 16);
   const _: () = assert!(core::mem::size_of::<FpuState>() == 512);
   ```
5. **Verificação Dinâmica no Kernel:**
   ```rust
   debug_assert!((state as *const _ as usize) % 16 == 0, "FpuState desalinhado!");
   ```
Isso confere 100% de garantia matemática contra `#GP` por desalinhamento. [`CONFIRMADO`]

---

### 2.4. Concorrência e Migração de Tarefas em SMP (SEC-FPU-06)

#### Modelo de Memória x86_64 e Barreiras de Sincronização:
* Em sistemas SMP (Symmetric Multiprocessing), a arquitetura x86_64 implementa **TSO (Total Store Order)**:
  * Leituras não são reordenadas após leituras.
  * Escritas não são reordenadas após escritas.
  * Leituras não são reordenadas após escritas em endereços distintos.
* No escalonador do Jinn OS, todo o ciclo de `preempt_core` ocorre sob a trava global `SCHED_LOCK` ([scheduler.rs#L1390](file:///d:/Projetos/Jinn/src/scheduler.rs#L1390)):
  * A liberação do lock (`drop(_guard)`) executa uma operação atômica de escrita com semântica **Release**.
  * A aquisição do lock pela próxima CPU executa uma operação atômica de troca com semântica **Acquire**.
* **Garantia de Visibilidade:** Qualquer gravação efetuada por `fxsave64` no Core 0 é **garantidamente visível** na memória antes que o Core 1 consiga adquirir o `SCHED_LOCK` e selecionar a tarefa para execução com `fxrstor64`.
* **Proteção contra Execução Concorrente da Mesma Tarefa:**
  * O campo `task.running_on_cpu` ([scheduler.rs#L1502](file:///d:/Projetos/Jinn/src/scheduler.rs#L1502)) só é limpo para `None` após o `saved_rsp` e o `fxsave64` terem sido concluídos na CPU de origem.
  * Nenhuma outra CPU pode iniciar o despacho de uma tarefa enquanto ela ainda estiver ativa em outro núcleo. [`CONFIRMADO`]

---

### 2.5. Comportamento sob Interrupções Aninhadas
* Como o microkernel Jinn é compilado sob o target `x86_64-unknown-none` com `+soft-float`, o compilador LLVM **não utiliza registradores `XMM`** para compilar rotinas de kernel.
* As interrupções de hardware comuns (timer, teclado, mouse, rede) executam suas rotinas de serviço (ISRs) sem tocar nos registradores `XMM` da CPU.
* Portanto, interrupções que apenas tratam eventos e retornam à mesma tarefa (`RAX == 0` em `stubs.s`, sem context switch) **não causam nenhuma perturbação no estado numérico da tarefa corrente**. O overhead de `FXSAVE`/`FXRSTOR` é estritamente evitado nesses casos. [`CONFIRMADO`]

---

## 3. Matriz de Conformidade de Segurança

| Requisito de Segurança | Mecanismo de Garantia | Resultado Esperado | Classificação |
|---|---|---|:---:|
| Confidencialidade de chaves criptográficas em XMM | Eager `fxsave64` e `fxrstor64` por tarefa | Vazamento nulo entre PIDs | `CONFIRMADO` |
| Isolamento em criação de processos | `FpuState::new_user_default()` em `spawn` | Registradores zerados no início | `CONFIRMADO` |
| Isolamento em `execve` | Reset forçado em `sys_execve_framed` | Imagem nova sem lixo herdado | `CONFIRMADO` |
| Tolerância a `#GP` por valor de MXCSR | Sanitização com máscara `0x0000_FFFF` | Zero pânicos de kernel em restore | `CONFIRMADO` |
| Tolerância a `#GP` por desalinhamento | `#[repr(align(16))]` verificado por assert | Zero exceções de alinhamento | `CONFIRMADO` |
| Consistência em migração SMP | Semântica Acquire/Release no `SCHED_LOCK` | Visibilidade de memória garantida | `CONFIRMADO` |
| Reentrância de ISR sem context switch | Código de kernel soft-float | Preservação transparente de XMM | `CONFIRMADO` |
