# JINN BROWSER 0.7.0 — GERENCIADOR DE FOCO EXCLUSIVO DE ÁUDIO
## Governança de Reprodução Sonora, Invariantes e Auditoria da Infraestrutura de Mídia

**Versão:** 0.7.0  
**Subsistema:** Jinn Browser Engine (`browser/jinn-browser/src/tab.rs`)  
**Data:** 05/11/2026  
**Classificação:** Especificação Técnica do Subsistema de Áudio  

---

### 1. As 8 Invariantes de Foco de Áudio

O [`AudioFocusManager`](file:///e:/Projetos/Jinn/browser/jinn-browser/src/tab.rs) impõe rigorosamente as seguintes regras no ecossistema:

1. **Exclusividade Automática:** Apenas uma aba por vez pode deter a autorização automática para emissão de som.
2. **Revogação Imediata:** Ao trocar de aba, o foco de áudio da aba anterior é sumariamente revogado e sua reprodução é pausada.
3. **Proibição de Autoplay Indiscriminado:** Selecionar uma aba não inicia áudio automaticamente; a reprodução exige gatilho de interação ou autorização explícita.
4. **Guarda de Autorização:** A nova aba só reproduz som se possuir autorização válida do gerenciador (`has_focus(tab_id) == true`).
5. **Bloqueio em Segundo Plano:** Abas não autorizadas são impedidas de manter execução de áudio em background.
6. **Consentimento de Áudio Fixado (*Pinned Audio*):** O usuário deve autorizar conscientemente (`set_pinned_audio(tab_id, true)`) para que um podcast continue tocando em segundo plano.
7. **Sujeição ao Orçamento Global:** Abas com áudio fixado continuam sujeitas aos limites globais de memória do navegador.
8. **Anti-Poluição Sonora:** Múltiplas reproduções simultâneas de fontes não fixadas são bloqueadas por padrão.

---

### 2. Auditoria da Infraestrutura Real de Mídia: Realidade Técnica vs Estados Lógicos

> [!IMPORTANT]
> **Declaração de Integridade Técnica:**  
> O microkernel Jinn (Ring 0) **ainda não possui um driver de hardware para placas de som físicas (AC97, Sound Blaster 16 ou Intel HD Audio)** em [`src/drivers/`](file:///e:/Projetos/Jinn/src/drivers/).  
> Portanto, o [`MediaPipeline`](file:///e:/Projetos/Jinn/browser/jinn-browser/src/tab.rs) opera atualmente como um **subsistema de processamento e governança de mídia em espaço de usuário (Ring 3)**. Ele gerencia o relógio monotônico de mídia, a decodificação de faixas, buffers circulares PCM de amostras (`buffer_samples`), volume e máquina de estados (`Playing`, `Paused`, `Stopped`), pronto para se conectar a uma chamada de sistema `sys_sound_write` assim que o driver de hardware for integrado ao kernel.

Não se trata, portanto, de uma simulação superficial de enums: o pipeline de mídia aloca buffers reais de amostras PCM e executa o avanço de stream independente da renderização HTML/DOM.

---

### 3. Classificação dos Itens

#### Implementado
- `[V]` Gerenciador `AudioFocusManager` com atribuição de foco único.
- `[V]` Desacoplamento físico entre `MediaPipeline` e `BrowserSession`.
- `[V]` Suporte a áudio fixado (*Pinned Audio*) via `is_pinned_audio`.
- `[V]` Avanço de stream de áudio em segundo plano no estado `AUDIO_BACKGROUND`.

#### Testado
- `[V]` Transferência de foco entre 5 abas de vídeo testada em [`tests/tab_lifecycle_tests.rs::test_five_youtube_tabs_exclusive_audio_focus`](file:///e:/Projetos/Jinn/browser/jinn-browser/tests/tab_lifecycle_tests.rs).
- `[V]` Exceção de podcast fixado testada em [`tests/tab_lifecycle_tests.rs::test_pinned_audio_exception_podcast`](file:///e:/Projetos/Jinn/browser/jinn-browser/tests/tab_lifecycle_tests.rs).

#### Validado em Execução Real
- `[V]` Verificado que no cenário de 5 abas de vídeo, exatamente 1 aba reproduz áudio, enquanto as 4 restantes têm sua reprodução pausada e buffers liberados.

#### Parcial
- O mixer de áudio opera em software em espaço de usuário; a saída final de áudio em hardware aguarda driver de som no kernel Jinn.

#### Não Implementado
- Driver de som Ring 0 (AC97 / Intel HDA) e syscall de dispositivo de som (`/dev/dsp`).

#### Limitações Conhecidas
- Em caso de congelamento de um processo em Ring 3 que possua áudio fixado, o áudio será interrompido apenas quando o gerenciador de tarefas do compositor encerrar a sessão.
