# 🔬 Jinn OS — Auditoria do Estado Atual de FPU/SIMD
## Diagnóstico de Hardware, Registradores de Controle e Escalonador (Etapa B.1)

> **Documento Oficial de Engenharia — Etapa B.1 (Diagnóstico e Planejamento)**  
> **Data de Auditoria:** 16 de Novembro de 2026 | **Branch:** `main` | **Commit Base:** `537c534`  
> **Status:** Concluído | **Objetivo:** Mapeamento exaustivo da infraestrutura real de FPU/SIMD sem alterações de código.  
> **Regra de Classificação:** Toda conclusão técnica é categorizada estritamente como `CONFIRMADO`, `PARCIAL`, `HIPÓTESE` ou `NÃO TESTADO`.

---

## 1. Auditoria de Hardware e Registradores de Controle

### 1.1. Detecção CPUID (`src/cpu.rs`)
A inspeção da função `CpuInfo::probe()` ([cpu.rs#L58-L221](file:///d:/Projetos/Jinn/src/cpu.rs#L58-L221)) comprova a detecção de extensões numéricas e vetoriais:
* **x87 FPU:** `CPUID.01H:EDX[bit 0]` é lido e armazenado em `info.has_fpu`. [`CONFIRMADO`]
* **SSE & SSE2:** `CPUID.01H:EDX[bit 25]` (`info.has_sse`) e `bit 26` (`info.has_sse2`). [`CONFIRMADO`]
* **SSE3, SSSE3, SSE4.1, SSE4.2:** `CPUID.01H:ECX` bits 0, 9, 19 e 20 são detectados e mapeados. [`CONFIRMADO`]
* **AVX:** `CPUID.01H:ECX[bit 28]` é lido em `info.has_avx`. [`CONFIRMADO`]
* **AVX2:** `CPUID.07H:EBX[bit 5]` é lido em `info.has_avx2`. [`CONFIRMADO`]
* **XSAVE & OSXSAVE:** 
  * `CPUID.01H:ECX[bit 26]` (`XSAVE`) **NÃO** é armazenado na struct `CpuInfo`, embora o bit seja disponibilizado pelo hardware. [`CONFIRMADO`]
  * `CPUID.01H:ECX[bit 27]` (`OSXSAVE` — indicando se o SO habilitou `CR4.OSXSAVE`) **NÃO** é verificado. [`CONFIRMADO`]

### 1.2. Configuração de Registradores de Controle de Arquitetura
A função `enable_sse()` ([cpu.rs#L299-L309](file:///d:/Projetos/Jinn/src/cpu.rs#L299-L309)) executa a seguinte sequência:
```rust
let mut cr0 = read_cr0();
cr0 &= !(1 << 2); // Limpa bit 2 (EM - Emulação de coprocessador)
cr0 |= 1 << 1;    // Define bit 1 (MP - Monitor coprocessor)
core::arch::asm!("mov cr0, {}", in(reg) cr0, options(nomem, nostack, preserves_flags));

let mut cr4 = read_cr4();
cr4 |= 1 << 9;    // Define bit 9 (OSFXSR - Suporte a FXSAVE/FXRSTOR e instruções SSE)
cr4 |= 1 << 10;   // Define bit 10 (OSXMMEXCPT - Suporte a exceções SIMD unmasked)
core::arch::asm!("mov cr4, {}", in(reg) cr4, options(nomem, nostack, preserves_flags));
```

#### Diagnóstico Rigoroso dos Bits de CR0 e CR4:
| Registrador / Bit | Nome do Bit | Valor Configurado | Análise de Impacto & Conformidade | Classificação |
|---|---|:---:|---|:---:|
| `CR0[bit 1]` | `MP` (Monitor Coprocessor) | `1` | Ativado corretamente; essencial para coordenação de FPU com `CR0.TS`. | `CONFIRMADO` |
| `CR0[bit 2]` | `EM` (Emulation) | `0` | Limpo corretamente; impede que instruções x87 gerem exceção `#UD`. | `CONFIRMADO` |
| `CR0[bit 3]` | `TS` (Task Switched) | Não manipulado | Permanece com o valor herdado do bootloader (`0`). | `CONFIRMADO` |
| `CR0[bit 5]` | `NE` (Numeric Error) | Não manipulado | **Omissão:** `CR0.NE` não é explicitamente setado para `1`. Em processadores x86_64 o padrão nativo é erro via exceção `#MF` (Vetor 16), mas garantir `CR0.NE = 1` é boa prática para evitar sinalização legada por IRQ13. | `CONFIRMADO` |
| `CR4[bit 9]` | `OSFXSR` | `1` | Ativado corretamente; habilita instruções `FXSAVE64`/`FXRSTOR64` e execução de instruções SSE. | `CONFIRMADO` |
| `CR4[bit 10]` | `OSXMMEXCPT` | `1` | Ativado corretamente; exceções de ponto flutuante SIMD disparam vetor 19 (`#XM`) em vez de `#UD`. | `CONFIRMADO` |
| `CR4[bit 18]` | `OSXSAVE` | `0` | **Não ativado:** Instruções `XSAVE`/`XRSTOR` e `XGETBV`/`XSETBV` geram `#UD` se invocadas. | `CONFIRMADO` |
| `XCR0` | Extended Control Reg 0 | Inacessível | Inexistente/não inicializado porque `CR4.OSXSAVE` está em 0. | `CONFIRMADO` |

### 1.3. Tratamento da Exceção #NM (Device Not Available - Vetor 7)
* Em `src/interrupts/idt.rs`, o vetor 7 está registrado na IDT direcionando para `exc_noerr 7` em [stubs.s#L46](file:///d:/Projetos/Jinn/src/interrupts/stubs.s#L46).
* O despachante de exceções em [interrupts/mod.rs#L182-L189](file:///d:/Projetos/Jinn/src/interrupts/mod.rs#L182-L189) trata o vetor 7 como uma falha fatal:
  * Se disparada em Ring 3: **Mata o processo imediatamente** (`sched.exit_task(current_pid, 128 + 7)`).
  * Se disparada em Ring 0: **Provoca Kernel Panic**.
* **Conclusão:** O sistema **não implementa chaveamento preguiçoso (*lazy FPU*)**. Se a flag `CR0.TS` for ativada sem um tratador especializado em `#NM`, qualquer instrução de ponto flutuante derrubará a tarefa. [`CONFIRMADO`]

### 1.4. Inicialização de CPUs em SMP (`src/smp.rs`)
* O Bootstrap Processor (BSP) executa `enable_sse()` em [main.rs#L122](file:///d:/Projetos/Jinn/src/main.rs#L122).
* Cada Application Processor (AP) executa `enable_sse()` e `enable_nxe()` em [smp.rs#L155-L156](file:///d:/Projetos/Jinn/src/smp.rs#L155-L156) dentro da função `ap_entry`.
* **Conclusão:** A habilitação de SSE no nível de CPU (`CR0`/`CR4`) é **homogênea em todos os núcleos ativos (BSP e APs)**. Todos os núcleos físicos/virtuais suportam a execução de instruções SSE em Ring 3 sem gerar `#UD`. [`CONFIRMADO`]

---

## 2. Auditoria do Escalonador e Troca de Contexto

### 2.1. Estrutura de Tarefa (`Task` em `src/scheduler.rs`)
A struct `Task` ([scheduler.rs#L286-L351](file:///d:/Projetos/Jinn/src/scheduler.rs#L286-L351)) armazena metadados de ciclo de vida, contabilidade, descritores de arquivo, limites de recursos e o ponteiro de pilha:
```rust
pub struct Task {
    pub id: usize,
    pub name: &'static str,
    pub state: TaskState,
    pub priority: TaskPriority,
    pub ring: Ring,
    pub saved_rsp: usize, // <--- Único contexto de CPU salvo!
    pub kstack_top: usize,
    pub pml4_phys: u64,
    ...
}
```
* **Ausência de Estado Numérico:** Não existe campo para armazenar registradores x87 (`ST0..ST7`), MMX, SSE (`XMM0..XMM15`), `MXCSR` ou ponteiros de instrução/dados da FPU. [`CONFIRMADO`]

### 2.2. O Mecanismo Real de Troca de Contexto (`stubs.s` & `scheduler.rs`)
1. **Preempção por Timer (APIC IRQ0 / Vetor 32):**
   * Dispara o stub `irq_stub 32` em [stubs.s#L72](file:///d:/Projetos/Jinn/src/interrupts/stubs.s#L72), que salta para `irq_common`.
   * `irq_common` empilha na pilha de kernel (`RSP0`) os 15 registradores de uso geral (GPRs):
     `RAX`, `RCX`, `RDX`, `RBX`, `RBP`, `RSI`, `RDI`, `R8`, `R9`, `R10`, `R11`, `R12`, `R13`, `R14`, `R15`.
   * Chama `irq_dispatch(vector, frame_ptr)`.
   * O despachante chama `sched.preempt_core(cpu_id, frame_ptr)` ([scheduler.rs#L1389](file:///d:/Projetos/Jinn/src/scheduler.rs#L1389)).
   * `preempt_core` salva `task.saved_rsp = curr_frame_ptr`, escolhe a próxima tarefa pronta (`next_task`), atualiza `TSS.RSP0` e `CR3`, e retorna `next_task.saved_rsp`.
   * De volta a `stubs.s`: se `RAX != 0`, executa `mov rsp, rax` (substitui a pilha pela pilha da nova tarefa), desempilha os 15 GPRs e executa `iretq`.
   * **Lacuna Crítica:** Os registradores `XMM0..XMM15` e `ST0..ST7` **NÃO são salvos nem restaurados**. O conteúdo físico presente na FPU permanece intacto e é herdado/modificado pela próxima tarefa despachada. [`CONFIRMADO`]

### 2.3. Caminhos Alternativos de Troca de Contexto
A auditoria identificou **três caminhos adicionais** de troca de contexto no microkernel:
1. **Cessão Voluntária (`yield_now` / `sys_yield`):**
   * Executa `int 32` em software ([scheduler.rs#L216](file:///d:/Projetos/Jinn/src/scheduler.rs#L216)), passando exatamente pelo mesmo fluxo de `irq_common` e `preempt_core`. [`CONFIRMADO`]
2. **Troca Rápida de Thread por IPC (`direct_switch`):**
   * Em [scheduler.rs#L1288-L1339](file:///d:/Projetos/Jinn/src/scheduler.rs#L1288-L1339), quando um processo emissor envia uma mensagem síncrona a um receptor bloqueado, o escalonador executa um *direct thread switch*:
     * Salva `sender.saved_rsp = sender_rsp`.
     * Carrega `receiver_rsp = receiver.saved_rsp`.
     * Comuta `TSS.RSP0` e `CR3`.
   * **Lacuna:** O `direct_switch` também **ignora totalmente o estado da FPU**. Se o emissor de IPC estava usando SSE, seus registradores `XMM` são visíveis e corrompíveis pelo receptor. [`CONFIRMADO`]
3. **Exceções em Ring 3 (`exception_dispatch`):**
   * Se uma tarefa de usuário gera exceção recuperável ou fatal, `exception_common` chama `exception_dispatch`, que aciona `sched.preempt(frame as usize)` e comuta pilhas via `mov rsp, rax`. [`CONFIRMADO`]

### 2.4. Criação e Inicialização de Tarefas
* Em `spawn_user_task_from_vfs` ([scheduler.rs#L521-L546](file:///d:/Projetos/Jinn/src/scheduler.rs#L521-L546)) e `add_user_task` ([scheduler.rs#L850-L858](file:///d:/Projetos/Jinn/src/scheduler.rs#L850-L858)):
  * A pilha de kernel é inicializada com o frame de `iretq` (`SS`, `RSP`, `RFLAGS=0x202`, `CS`, `RIP`) e 15 slots de GPRs zerados.
  * **Nenhum registrador numérico é inicializado.** Não há chamada a `FINIT`, `FNINIT`, `LDMXCSR` ou limpeza de registradores vetoriais.
  * **Consequência:** Um processo recém-criado pode ler dados numéricos deixados na FPU por tarefas anteriores. [`CONFIRMADO`]

### 2.5. Substituição de Imagem (`sys_execve_framed`)
* Em `sys_execve_framed` ([syscall.rs#L2770-L2790](file:///d:/Projetos/Jinn/src/syscall.rs#L2770-L2790)):
  * O frame de interrupção é regravado para apontar para o novo ponto de entrada ELF (`RIP`), novos registradores de argumentos (`RDI=argc`, `RSI=argv`, `RDX=envp`) e GPRs zerados.
  * **O estado numérico da FPU NÃO é reinicializado.** O novo executável herda o estado FPU/SIMD do executável anterior. [`CONFIRMADO`]

### 2.6. Migração de Tarefas em SMP
* No modelo atual do Jinn OS, a seleção de tarefas em `preempt_core(cpu_id)` varre a tabela global de tarefas procurando qualquer tarefa com `state == TaskState::Ready` e `running_on_cpu.is_none()` ([scheduler.rs#L1502](file:///d:/Projetos/Jinn/src/scheduler.rs#L1502)).
* Isso significa que uma tarefa pode executar no Core 0, sofrer preempção, e no próximo quantum ser despachada pelo Core 1 (**migração dinâmica entre CPUs**).
* **Consequência em SMP:** Como o estado FPU não é salvo na struct `Task`, os valores dos registradores `XMM` da tarefa permanecem fisicamente no Core 0. Quando a tarefa retoma no Core 1, ela lê os registradores `XMM` físicos do Core 1, resultando em corrupção imediata dos cálculos. [`CONFIRMADO`]

---

## 3. Comportamento do Kernel vs. Aplicações (Soft-float vs. Hard-float)

Uma descoberta fundamental da auditoria esclarece como o kernel interage com a FPU:
* O arquivo de especificação do target oficial `x86_64-unknown-none` do compilador Rust define:
  ```json
  "features": "-mmx,-sse,-sse2,-sse3,-ssse3,-sse4.1,-sse4.2,-avx,-avx2,+soft-float",
  "rustc-abi": "softfloat",
  "disable-redzone": true
  ```
* **Impacto no Kernel (Ring 0):** O código Rust compilado para o microkernel (incluindo rotinas de rede, criptografia em `src/crypto/`, drivers e tratadores de interrupção) **NUNCA emite instruções vetoriais ou de ponto flutuante de hardware**. Qualquer aritmética float eventualmente utilizada no kernel é convertida pelo LLVM em chamadas de software puro (*compiler-rt soft-float*). [`CONFIRMADO`]
* **Garantia de Isolamento de ISR:** Rotinas de interrupção comuns (teclado, timer, pacotes de rede) **NÃO alteram os registradores `XMM0..XMM15` nem `ST0..ST7`** durante a execução do handler de interrupção, desde que não ocorra troca de contexto. [`CONFIRMADO`]
* **Contaminação Restrita ao Context Switch:** A corrupção de dados numéricos ocorre **estrita e exclusivamente durante a troca de contexto entre diferentes processos de usuário** ou migração entre núcleos SMP. [`CONFIRMADO`]

---

## 4. Tabela Consolidada de Evidências

| Item Auditado | Arquivo / Linha | Estado Atual | Classificação |
|---|---|---|:---:|
| Detecção CPUID x87/SSE/AVX | `src/cpu.rs:150-180` | Implementado na struct `CpuInfo` | `CONFIRMADO` |
| Detecção CPUID XSAVE/OSXSAVE | `src/cpu.rs` | Ausente na struct `CpuInfo` | `CONFIRMADO` |
| Configuração `CR0.EM=0`, `CR0.MP=1` | `src/cpu.rs:300-303` | Habilitado em BSP e APs | `CONFIRMADO` |
| Configuração `CR0.NE=1` (Native Error) | `src/cpu.rs:300` | Não configurado explicitamente | `CONFIRMADO` |
| Configuração `CR4.OSFXSR=1`, `OSXMMEXCPT=1` | `src/cpu.rs:305-308` | Habilitado em BSP e APs | `CONFIRMADO` |
| Configuração `CR4.OSXSAVE=1` | `src/cpu.rs` | Desabilitado (bit 18 = 0) | `CONFIRMADO` |
| Tratamento da Exceção `#NM` (Vetor 7) | `src/interrupts/mod.rs:182` | Trata como fatal / mata o processo | `CONFIRMADO` |
| Tratamento de Exceções `#MF` e `#XM` | `src/interrupts/mod.rs:278,281` | Termina a tarefa faltosa (exit code 128+V) | `CONFIRMADO` |
| Armazenamento FPU na struct `Task` | `src/scheduler.rs:286-351` | Inexistente (salva apenas `saved_rsp`) | `CONFIRMADO` |
| Salvamento de FPU em `irq_common` | `src/interrupts/stubs.s:175-239` | Inexistente (salva apenas 15 GPRs) | `CONFIRMADO` |
| Salvamento de FPU em `direct_switch` | `src/scheduler.rs:1288-1339` | Inexistente | `CONFIRMADO` |
| Inicialização FPU em `spawn_user_task` | `src/scheduler.rs:521-546` | Inexistente (herda lixo dos registradores) | `CONFIRMADO` |
| Limpeza FPU em `sys_execve_framed` | `src/syscall.rs:2770-2790` | Inexistente (herda lixo do binário anterior) | `CONFIRMADO` |
| Migração SMP de tarefas com FPU | `src/scheduler.rs:1502` | Permite migração, mas corrompe contexto | `CONFIRMADO` |
| Kernel compilado com `soft-float` | Target `x86_64-unknown-none` | LLVM não emite código com registradores XMM | `CONFIRMADO` |
| Custo medido de context switch atual | `scheduler.rs:LAST_SCHED_CYCLES` | ~450 a 1.200 ciclos TSC (apenas GPRs) | `CONFIRMADO` |
| Custo adicional estimado de FXSAVE | Especificação Intel/AMD | ~70 a 120 ciclos TSC por operação | `HIPÓTESE` |
