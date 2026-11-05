# JINN BROWSER 0.8.1 — INITIAL AUDIT & BASELINE VERIFICATION

**Documento:** `docs/JINN_BROWSER_081_INITIAL_AUDIT.md`  
**Classificação:** Auditoria Inicial de Engenharia e Preservação de Repositórios  
**Versão-Alvo:** 0.8.1 (Baseline: 0.8.0)  
**Data:** 05/11/2026  
**Status:** Auditado e Concluído [V]

---

## 1. Estado dos Repositórios Git e Preservação

A inspeção inicial dos repositórios foi executada sem comandos destrutivos, confirmando conformidade estrita com o histórico:

### 1.1. Repositório Jinn OS (`E:\Projetos\Jinn`)
- **Branch:** `main` (sincronizada com `origin/main`).
- **Commit de Referência Verificado:** `c9eef22` (`feat(browser): fullscreen stability, near-zero resource governance, and 0.8.0 documentation reports`).
- **Working Tree:** Limpa (`nothing to commit, working tree clean`).
- **Remoto Oficial:** `https://github.com/GeovaneEstanislau/Jinn_Project.git`.

### 1.2. Repositório Aladdin Desktop (`E:\Projetos\Aladdin`)
- **Branch:** `main` (sincronizada com `origin/main`).
- **Commit de Referência Verificado:** `ead5893` (`feat(wm): client window resize event protocol and dynamic buffer reallocation`).
- **Working Tree:** Limpa (`nothing to commit, working tree clean`).
- **Remoto Oficial:** `https://github.com/GeovaneEstanislau/Aladdin.git`.

---

## 2. Diagnóstico da Arquitetura Encontrada (Versão 0.8.0)

A auditoria dos arquivos efetivos de implementação confirmou o seguinte panorama:

### 2.1. Subsistema Gráfico e Modo de Tela Cheia
- **`browser/jinn-browser/src/fullscreen.rs`:** Máquina de estados básica implementada com 6 variantes (`Windowed`, `EnteringFullscreen`, `Fullscreen`, `ExitingFullscreen`, `ResizePending`, `Recovery`).
  - *Diagnóstico 0.8.1:* A máquina atual faz transições síncronas diretas sem permitir modelo de duas fases (*two-phase commit*) para sincronização assíncrona com o Window Manager, e carece de testes sistemáticos para transições inválidas forçadas.
- **`apps/ola-mundo/src/main.rs` & `browser.rs`:** O cliente Ring 3 aloca um buffer estático global de 3 MiB (`[u32; 1024 * 768] = 3.145.728 bytes`) permanentemente no segmento BSS.
  - *Diagnóstico 0.8.1:* Essa abordagem desperdiça ~2,9 MiB de memória física e virtual quando o navegador opera em modo janela padrão ($300 \times 180 = 216 \text{ KiB}$). É mandatório implementar alocação sob demanda via `sys_alloc_page` / `sys_free_page`, realocação segura atômica e devolução de superfícies não utilizadas.
- **Compositor Aladdin (`Aladdin/src/wm/manager.rs` & `session.rs`):**
  - O evento `EVENT_WINDOW_RESIZE = 8` despacha larguras e alturas no `ClientWindowEvent`.
  - *Diagnóstico 0.8.1:* `update_client_buffer` redimensiona o vetor sem validação de teto máximo (`MAX_BUFFER_PIXELS`), abrindo vetor de ataque para alocação excessiva de heap caso um cliente defeituoso envie tamanhos desmesurados.

### 2.2. Integração com o Kernel e Scheduler
- **`src/scheduler.rs`:** O kernel já possui a estrutura `Task` instrumentada com `cpu_ticks: u64`, `ctx_switches: u64` e estados `TaskState::Blocked`, `Ready`, `Running`. O despachante `run_next` ignora sumariamente tarefas em `TaskState::Blocked`.
- **`src/syscall.rs`:** `sys_res_stat` (syscall 25) já expõe `cpu_ticks` e `state` do processo.
  - *Diagnóstico 0.8.1:* Falta uma chamada de sistema explícita (`sys_task_set_state` — syscall 53) que permita ao processo pai do navegador suspender tarefas filhas de abas para `TaskState::Blocked` e reativá-las para `TaskState::Ready`, tornando a alegação de 0 CPU ticks auditável diretamente pelo kernel.

### 2.3. Contabilidade de Memória e Métricas BRCR
- As métricas de pegada lógica vs física foram introduzidas em `tab.rs`, mas requerem granularidade adicional:
  - `ResidentPageCount`, `ReleasedPageCount`, `SharedMemoryFootprint`, `GraphicsBufferFootprint`, `MediaBufferFootprint`, `SnapshotFootprint`.
  - Separação dos índices do BRCR: CPU Background Consumption Ratio, Memory Retention Ratio, Graphics Resource Retention Ratio, Media Retention Ratio, I/O Background Ratio.

---

## 3. Plano de Engenharia e Ações para 0.8.1

1. **Fase 1 (Hardening Gráfico):**
   - Refinar `FullscreenController` com suporte a duas fases (`EnteringFullscreen` com confirmação e rollback seguro).
   - Implementar `DynamicSurface` em Ring 3 com realocação sob demanda e devolução de memória.
   - Proteger o compositor Aladdin contra tamanhos excessivos em `update_client_buffer`.
2. **Fase 2 & 3 (Governança e Memória Física):**
   - Implementar syscall 53 (`sys_task_set_state`) no kernel Jinn com controle de permissões.
   - Adicionar métricas formais de páginas residentes e liberadas.
   - Testar suíte de estresse de ticks de CPU comprovando zero ticks em `TaskState::Blocked`.
3. **Fase 4 & 5 (Métricas BRCR & Confiabilidade de Ciclo):**
   - Decompor o BRCR nos índices especializados com metodologia rastreável.
   - Validar testes de restauração de abas sob 100 ciclos repetidos.
4. **Fase 6 a 8 (Segurança, Regressão & Benchmarks):**
   - Executar testes de falha injetada, concorrência e regressão multisistema completa.
   - Atualizar a matriz de testes e relatórios técnicos comparativos 0.8.0 vs 0.8.1.
5. **Fase 9 a 11 (Documentação, Build & Publicação):**
   - Produzir os 15 relatórios técnicos da versão 0.8.1 em `docs/`.
   - Gerar ISO e publicar commits em ambos os repositórios.

