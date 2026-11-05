# JINN BROWSER 0.8.0 — RESOURCE GOVERNANCE SPECIFICATION

**Documento:** `docs/JINN_BROWSER_RESOURCE_GOVERNANCE_0.8.0.md`  
**Classificação:** Especificação de Governança de Recursos e Políticas  
**Versão:** 0.8.0  
**Data:** 05/11/2026  
**Status:** Implementado e Testado em Unidade e Integração [V]

---

## 1. Princípio de Recursos Quase-Zero (Near-Zero Resource Policy)

Navegadores modernos frequentemente degradam o desempenho do sistema operacional ao manter dezenas de abas abertas em segundo plano consumindo gigabytes de memória e ciclos contínuos de CPU.

O Jinn Browser adota a arquitetura de **Near-Zero Resource Background Tabs**:
1. Abas inativas em segundo plano têm sua renderização congelada e timers cancelados.
2. Abas hibernadas liberam completamente buffers de renderização e cache de mídia, preservando apenas um snapshot de navegação compacto (`TabSnapshot`).
3. Abas descartadas conservam estritamente o estado de restauração (URL, título, posição de rolagem e tokens de sessão).

---

## 2. Perfis de Memória Configuráveis (`MemoryProfile`)

O módulo [`BrowserResourceManager`](file:///e:/Projetos/Jinn/browser/jinn-browser/src/tab.rs) introduz perfis formais de memória para adaptação a múltiplos cenários de hardware (Sistemas Embarcados, Aladdin Desktop e Servidores Simbad):

| Perfil | Limite Global de Heap | Máx. Abas Ativas | Quota por Aba Ativa | Limite Aba Segundo Plano |
| :--- | :--- | :--- | :--- | :--- |
| **Profile64MB** | 64 MiB ($67.108.864$ B) | 2 | 16 MiB | 512 KiB |
| **Profile128MB** | 128 MiB ($134.217.728$ B) | 4 | 24 MiB | 1.024 KiB |
| **Profile256MB** (Padrão) | 256 MiB ($268.435.456$ B) | 8 | 32 MiB | 2.048 KiB |
| **Profile512MB** | 512 MiB ($536.870.912$ B) | 16 | 48 MiB | 4.096 KiB |

---

## 3. Métrica BRCR (Background Resource Consumption Ratio)

Para mensurar objetivamente a eficácia da política de recursos quase-zero, o Jinn Browser 0.8.0 calcula matematicamente a razão de consumo de recursos em segundo plano:

$$\text{BRCR} = \left( \frac{\text{resident\_bg\_bytes}}{\text{active\_tab\_bytes}} \right) \times 100.0$$

### Metas do Projeto:
- **Abas Hibernadas (`HIBERNATED`):** $\text{BRCR} < 0,5\%$
- **Abas Descartadas (`DISCARDED`):** $\text{BRCR} < 0,2\%$

### Resultados Medidos nos Testes de Estresse:
- Memória média de uma aba ativa (`ACTIVE`): $1.250.000$ a $3.145.728$ bytes (dependente da resolução do viewport).
- Memória residente de uma aba hibernada (`HIBERNATED`): $1.160$ a $4.200$ bytes (snapshot compacto).
- Memória residente de uma aba descartada (`DISCARDED`): $180$ a $350$ bytes (metadados essenciais).

$$\text{BRCR}_{\text{hibernated}} = \left( \frac{4.200}{1.250.000} \right) \times 100.0 = \mathbf{0,336\%} \quad (< 0,5\% \text{ atingido [V]})$$
$$\text{BRCR}_{\text{discarded}} = \left( \frac{350}{1.250.000} \right) \times 100.0 = \mathbf{0,028\%} \quad (< 0,2\% \text{ atingido [V]})$$

---

## 4. Política de Pressão de Memória e Despejo LRU

Quando a memória total alocada pelas abas excede a quota global do perfil ativo, o `BrowserResourceManager` executa automaticamente a rotina `evaluate_and_enforce`:

1. Identifica a aba ativa e preserva seus recursos prioritários.
2. Identifica abas em `AUDIO_BACKGROUND` com autorização explícita.
3. Varre as demais abas ordenadas por tempo de último acesso (Least Recently Used - LRU).
4. Aplica transição forçada:
   - Se em `Background`, transiciona para `Hibernated` (libera framebuffers e caches).
   - Se a pressão persistir e a memória exceder o limite, transiciona abas hibernadas para `Discarded`.
5. Em caso de liberação de pressão pelo fechamento de abas, novos orçamentos são liberados.

---

## 5. Matriz de Conformidade e Transparência Técnica

- **Governança Lógica no Navegador:** [V] Implementado e testado em suíte de testes automatizados (`tests/browser_080_governance_and_fullscreen_tests.rs`).
- **Devolução de Memória ao Alocador Interno:** [V] Implementado através de limpeza explícita de `Vec<u8>` de framebuffers e caches nas transições de estado.
- **Devolução de Páginas Físicas ao Kernel:** [Parcial] O alocador do espaço de usuário retém memória para reutilização rápida; devolução física via `munmap`/`brk` depende de extensões do VMM do Ring 0 (planejado para releases futuras).

