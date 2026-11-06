# JINN DRIVER FAULT CONTAINMENT & RESILIENCE 1.0
## Contenção de Falhas, Isolamento de DMA e Resiliência contra Ataques Adversariais

**Projeto:** Jinn OS  
**Subcomponente:** Driver Supervisor & Microkernel Security Boundaries  
**Versão:** 1.0  
**Data:** 06/11/2026  

---

## 1. Princípios de Segurança de Baixo Nível

A integridade do microkernel Jinn é resguardada por barreiras de hardware e software:

1. **Privilégio de Execução Ring 0 vs. Ring 3:**
   - O microkernel executa com privilégio Ring 0 (CPL=0).
   - Drivers e serviços de usuário operam em Ring 3 (CPL=3).
   - Tentativas de executar instruções privilegiadas (`cli`, `sti`, `lidt`, `mov cr3`, `wrmsr`) em Ring 3 geram imediatamente `#GP` (General Protection Fault, Vetor 13) e encerram a tarefa faltosa sem afetar o sistema.
2. **Validação Rigorosa de Buffers e Syscalls (`EFAULT`):**
   - Todo ponteiro fornecido pelo usuário a uma syscall é validado por `validate_user_buffer`:
     - Ponteiro nulo (`NULL`) -> `-EINVAL`.
     - Endereço no espaço de endereçamento do kernel (`>= 0xFFFF_8000_0000_0000`) -> `-EFAULT`.
     - Endereço virtual não mapeado no PML4 do processo -> `-EFAULT`.
3. **Contenção de DMA e Hardware sem IOMMU:**
   - Em computadores históricos sem unidade IOMMU (Intel VT-d ou AMD-Vi), operações DMA de controladores de barramento (como Bus Mastering IDE ou PCI NICs) operam sobre endereços físicos diretos.
   - O microkernel aloca buffers de DMA estritamente a partir do alocador físico verificado (`alloc_frame`), associando anéis de descritores de tamanho fixo e limites de transação verificados pelo supervisor.

---

## 2. Matriz de Injeção de Falhas e Validação Experimental

O módulo `src/drivers/supervisor.rs` executa uma suíte automatizada de testes de injeção de falhas (`run_fault_injection_tests`), cobrindo 5 cenários adversariais:

| Cenário de Falha Injetada | Comportamento Esperado | Resultado no Jinn OS |
| :--- | :--- | :---: |
| **1. Falha Inesperada em Driver de Rede** | O supervisor detecta erro no descritor, isola o driver e realiza reinicialização controlada sem derrubar o kernel. | **APROVADO [V]** (Estado: Active) |
| **2. Falha de Driver de Áudio** | Degradação imediata para `AudioMute`, liberando buffers PCM e preservando a renderização gráfica do Aladdin Desktop. | **APROVADO [V]** (Estado: Active) |
| **3. Ciclo de Falhas Repetidas** | O circuit-breaker dispara após 3 falhas consecutivas, transicionando o driver para `Disabled` e prevenindo loop infinito. | **APROVADO [V]** (Estado: Disabled) |
| **4. Falha em Controlador de Armazenamento** | Se houver escritas pendentes, o supervisor transiciona para `Degraded` e protege o disco contra reinicialização cega com perda de dados. | **APROVADO [V]** (Estado: Degraded) |
| **5. Driver Unresponsive (Timeout)** | Se o driver parar de emitir heartbeat e exceder a tolerância de ticks, o supervisor o classifica como `Unresponsive`. | **APROVADO [V]** (Estado: Unresponsive) |

---

## 3. Diretriz de Conclusão

> **Critério Central Atendido:** A falha ou encerramento inesperado de um driver periférico NÃO provoca falha do microkernel, nem corrupção do scheduler ou encerramento de aplicações independentes.

