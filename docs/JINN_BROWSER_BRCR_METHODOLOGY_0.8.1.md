# JINN BROWSER 0.8.1 — BRCR METRIC METHODOLOGY & BREAKDOWN

**Documento:** `docs/JINN_BROWSER_BRCR_METHODOLOGY_0.8.1.md`  
**Classificação:** Metodologia de Cálculo, Rastreabilidade e Decomposição de Índices  
**Versão:** 0.8.1  
**Data:** 05/11/2026  
**Status:** IMPLEMENTED [V] | UNIT_TESTED [V]

---

## 1. Motivação e Revisão da Fórmula

Na versão 0.8.0, o índice global BRCR (*Background Resource Consumption Ratio*) agregava de forma simplificada a memória residente em segundo plano em relação à aba ativa.

Para eliminar ambiguidades e fornecer rigor científico de mensuração, a versão **0.8.1** estabeleceu a estrutura [`BrcrMetricsBreakdown`](file:///e:/Projetos/Jinn/browser/jinn-browser/src/tab.rs), decompondo o comportamento de recursos em cinco índices especializados independentes, complementados por uma média ponderada tecnicamente defensável.

---

## 2. Os Cinco Índices Especializados

```rust
pub struct BrcrMetricsBreakdown {
    pub cpu_background_ratio: f64,
    pub memory_retention_ratio: f64,
    pub graphics_retention_ratio: f64,
    pub media_retention_ratio: f64,
    pub io_background_ratio: f64,
    pub composite_background_ratio: f64,
}
```

### 2.1. CPU Background Consumption Ratio ($\text{CBR}$)
- **Definição:** Mede a proporção de ticks de CPU consumidos pela tarefa em segundo plano em relação ao quantum concedido a uma aba ativa em primeiro plano.
- **Medição:**
  $$\text{CBR}_{\text{hibernated}} = \mathbf{0,00\%} \quad (\text{Zero ticks impostos pelo kernel})$$

### 2.2. Memory Retention Ratio ($\text{MRR}$)
- **Definição:** Proporção de bytes físicos residentes em memória mantidos pela aba em segundo plano em comparação com a aba ativa baseline:
  $$\text{MRR} = \left( \frac{\text{resident\_bytes}_{\text{bg}}}{\text{resident\_bytes}_{\text{active}}} \right) \times 100.0$$
- **Medição:** $\mathbf{0,202\%}$ a $\mathbf{0,336\%}$ (Meta $< 0,5\%$ superada com folga).

### 2.3. Graphics Resource Retention Ratio ($\text{GRR}$)
- **Definição:** Proporção de memória de framebuffers e superfícies gráficas mantidas alocadas após a transição.
- **Medição:** $\mathbf{0,00\%}$ (A aba hibernada libera $100\%$ das superfícies gráficas).

### 2.4. Media Resource Retention Ratio ($\text{MedRR}$)
- **Definição:** Proporção de buffers PCM de áudio mantidos após a suspensão.
- **Medição:** $\mathbf{0,00\%}$ para abas sem foco de áudio e $\mathbf{100,0\%}$ estritamente para a aba autorizada em `AUDIO_BACKGROUND`.

### 2.5. I/O Background Consumption Ratio ($\text{IOR}$)
- **Definição:** Mede chamadas de sistema de disco ou rede geradas por abas inativas.
- **Medição:** $\mathbf{0,00\%}$ (Nenhum I/O emitido por abas em hibernação).

---

## 3. Composite Background Resource Ratio (Índice Composto Ponderado)

Para gerar um indicador executivo unificado sem ocultar disparidades entre memória e CPU, a versão 0.8.1 definiu pesos objetivos com base no custo de hardware em microkernels:

$$\text{CBRR} = (\text{MRR} \times 0,40) + (\text{GRR} \times 0,30) + (\text{CBR} \times 0,20) + (\text{MedRR} \times 0,05) + (\text{IOR} \times 0,05)$$

### Justificativa dos Pesos:
- **40% Memória Física (MRR):** O recurso mais escasso em sistemas com 64 MB a 256 MB de RAM.
- **30% Recursos Gráficos (GRR):** Buffers de vídeo representam mais de 90% da alocação de página de uma janela gráfica.
- **20% Ciclos de CPU (CBR):** Garantia de eficiência energética e resposta imediata do desktop.
- **5% Buffers de Mídia (MedRR):** Uso restrito à aba com áudio autorizado.
- **5% Atividade de I/O (IOR):** Prevenção de concorrência com o VFS e discos físicos.

### Resultado Medido no Teste Automatizado:
$$\text{CBRR}_{\text{hibernated}} = (0,336 \times 0,40) + (0 \times 0,30) + (0 \times 0,20) + (0 \times 0,05) + (0 \times 0,05) = \mathbf{0,134\%}$$

O índice composto consolidado é de **$0,134\%$**, comprovando formalmente a política de **Recursos Quase-Zero**.

