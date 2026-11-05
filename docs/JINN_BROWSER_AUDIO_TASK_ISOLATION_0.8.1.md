# JINN BROWSER 0.8.1 — AUDIO TASK ISOLATION & FOCUS GOVERNANCE

**Documento:** `docs/JINN_BROWSER_AUDIO_TASK_ISOLATION_0.8.1.md`  
**Classificação:** Especificação Técnica de Isolamento de Tarefas de Áudio  
**Versão:** 0.8.1  
**Data:** 05/11/2026  
**Status:** IMPLEMENTED [V] | UNIT_TESTED [V] | DRIVER_PHYSICAL_ABSENT [X]

---

## 1. Declaração Mandatória de Transparência de Hardware

> [!IMPORTANT]
> **Ausência de Driver Físico de Áudio no Kernel Jinn:**  
> O kernel Jinn atualmente **NÃO POSSUI** drivers físicos de hardware para **Intel HDA**, **AC97** ou **Sound Blaster 16** em Ring 0.
> 
> A reprodução de mídia opera estritamente no espaço de usuário em Ring 3 como um **pipeline de buffer PCM lógico**. A alegação de reprodução física de áudio em hardware real ou QEMU continua fora do escopo desta versão.

---

## 2. Princípio da Separação entre Tarefas de Áudio e Tarefas Visuais

Em navegadores convencionais, colocar uma aba em segundo plano enquanto toca música frequentemente mantém ativo todo o pipeline gráfico, processamento de DOM, timers de JavaScript e layouts desnecessários.

O Jinn Browser 0.8.1 implementa a **política de isolamento estrito**:
1. Quando uma aba com áudio ativo é movida para segundo plano (`TabState::AudioBackground`):
   - **Renderização e Framebuffers:** Desativados e liberados (nenhum frame é enviado ao compositor).
   - **Timers Visuais e Animações:** Pausados (`animations_active = false`, `page_timers_active = false`).
   - **Pipeline de Áudio:** Mantido ativo no estado `TaskExecutionState::ThrottledAudioOnly`, recebendo quantums estritamente necessários para manter abastecido o anel de amostras PCM.
2. Essa segregação impede que scripts pesados de páginas concorram com a estabilidade do fluxo de áudio ou desperdicem CPU.

---

## 3. Gestão e Preempção do Foco Exclusivo (`AudioFocusManager`)

O gerenciador [`AudioFocusManager`](file:///e:/Projetos/Jinn/browser/jinn-browser/src/tab.rs) impõe exclusividade em nível de sistema:
- Apenas uma aba pode deter `active_focus_tab_id` por vez.
- Se outra aba solicitar foco de áudio, a aba detentora anterior perde o foco imediatamente (`release_focus`).
- A perda de foco desarma a flag `is_pinned_audio`, forçando a aba a pausar a mídia e acionar o esvaziamento imediato das amostras PCM (`buffer_samples = 0`).

---

## 4. Evidência em Testes Automatizados

O teste unitário `test_audio_task_isolation_and_pcm_cleanup` em `tests/browser_081_reliability_and_governance_tests.rs` comprovou:
1. Início de áudio na aba com 8192 amostras e foco atribuído (`active_focus_tab_id == Some(1)`).
2. Transição para `AUDIO_BACKGROUND` com estado de execução `ThrottledAudioOnly` e buffers PCM preservados.
3. Preempção de foco por outra aba e subsequente hibernação forçada:
   - Buffer PCM reduzido imediatamente para **0 amostras (0 bytes residentes)**.
   - Estado de tarefa no kernel transicionado para `Suspended` (`TaskState::Blocked`).
   - Zero consumo de CPU ou memória residual de áudio.

