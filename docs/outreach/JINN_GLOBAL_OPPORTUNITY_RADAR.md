# Jinn Global Opportunity Radar — Registro Vivo de Interlocutores e Oportunidades

> **Documento Operacional Complementar a:** [`JINN_GLOBAL_INITIATIVE_ESTRATEGIA.md`](JINN_GLOBAL_INITIATIVE_ESTRATEGIA.md)  
> **Arquivo Estruturado Espelho (CSV):** [`opportunity_radar.csv`](opportunity_radar.csv)  
> **Regra de Governança:** Qualquer organização real descoberta durante uma investigação técnica, incidente, *benchmark* ou interação espontânea deve ser incorporada imediatamente ao Radar — mesmo fora de campanhas planejadas. Nomes de pessoas, e-mails ou evidências de resposta **nunca são presumidos**: permanecem como `A pesquisar` até investigação real documentada.

---

## 1. Escala Canônica de Status de Relacionamento (`0` a `6`)

| Código | Estágio | Definição Objetiva | Critério para Avanço ao Próximo Estágio |
| :---: | :--- | :--- | :--- |
| **`0`** | **Identificado** | Organização adicionada ao radar pelo encaixe tecnológico ou regional, sem pesquisa aprofundada de pessoas/programas | Mapear equipe técnica, área de P&D, programa open source/fomento e problema compatível |
| **`1`** | **Investigado** | Área técnica, programas (parcerias/pesquisa/grants), requisitos e aderência com algum componente do Jinn identificados | Compartilhar/referenciar publicação técnica aderente ou preparar pauta de contato |
| **`2`** | **Exposto** | Publicação técnica relevante referenciou contextualmente (Nível B) ou marcou criteriosamente (Nível A) ou foi distribuída em canal onde atua | Preparar e enviar mensagem individual com razão técnica específica |
| **`3`** | **Contatado** | Contato individual enviado (e-mail, LinkedIn, issue/RFC ou canal institucional acadêmico/empresarial) com material reproduzível | Receber resposta, comentário substantivo ou encaminhamento interno |
| **`4`** | **Engajamento** | Resposta recebida, discussão técnica aberta, crítica de engenharia, convite para conversa ou revisão externa | Definir escopo concreto de experimento, *workload*, hardware ou contribuição |
| **`5`** | **Colaboração** | Teste conjunto, *workload* cedido, projeto-piloto, iniciação científica, benchmark em bancada física ou contribuição de código | Formalizar termo contratual, patrocínio, *grant*, convênio ou licença |
| **`6`** | **Oportunidade formal** | Contrato assinado, *grant* aprovado, patrocínio, investimento, convênio formal ou licenciamento sob `Jinn-Commercial-1.0` / `MIT`/`Apache-2.0` (`jhcs-rs`) | Execução de entrega contínua e relatório de resultados (com confidencialidade respeitada) |

---

## 2. Dicionário de Campos do Radar (`opportunity_radar.csv`)

1. `Organizacao`: Empresa, universidade, laboratório, órgão público ou comunidade.
2. `Pais`: Localização (país / UF quando Brasil).
3. `Circulo`: `1-Pioneira` | `2-Operacional` | `3-Academia` | `4-Instituicao`.
4. `Segmento`: Hardware, Semicondutores, GPU, Edge, Automação Industrial, Segurança, Pesquisa em SO, Sistemas Embarcados etc.
5. `Tecnologia_Jinn_Relacionada`: `JHCS`, `Zero-Idle / SMP / IRQ`, `Isolamento / Ring 3 Drivers`, `JAHA 1.2 / Densidade Memoria`, `Simbad / JDA`, `Aladdin SIMD / TLS 1.3 no_std`.
6. `Problema_ou_Oportunidade`: Relação técnica legítima ou gargalo potencial.
7. `Nivel_Marcacao`: `Nivel A (Marcacao direta)` | `Nivel B (Referencia contextual)` | `Nivel C (Radar silencioso)`.
8. `Publicacoes_Relacionadas`: Artigos no Observatory (`jinn-os.dev.br/artigos/...`), commits ou issues em `docs/strategic-issues/`.
9. `Ultimo_Contato`: Data ISO (`YYYY-MM-DD`) ou `N/A (nunca contatado)`.
10. `Contato`: Pessoa, departamento, laboratório ou programa (ex.: `A pesquisar — equipe de kernel/pesquisa`).
11. `Tipo_de_Contato`: `Tecnico` | `Academico` | `Institucional` | `Comercial/Financeiro`.
12. `Status_Num` / `Status_Nome`: Escala `0` a `6`.
13. `Proximo_Passo`: Ação concreta verificável.
14. `Prioridade`: `Alta` | `Media` | `Baixa`.
15. `Evidencia_de_Interesse`: Resposta, comentário, convite, reunião ou contribuição real (ou `Nenhuma ate o momento`).

---

## 3. Radar Vivo Inicial — Segmentado por Tecnologia e Círculos

### 3.1. Frente A — JHCS (`jhcs-rs`) & Computação Heterogênea

| Organização | País | Círculo | Segmento | Problema / Oportunidade Técnica | Nível | Publicações / Ativos Relacionados | Contato | Status | Próximo Passo | Prioridade |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **AMD** | EUA | `1-Pioneira` | CPU, GPU, Heterogeneidade | Seleção adaptativa de executor (CPU escalar vs SIMD AVX2 vs GPU) com modelo explícito de custo de transferência | `Nivel C` (subir para `A` quando houver benchmark reprodutível JHCS) | `jhcs-rs/crates/jhcs-broker`; `docs/strategic-issues/02_ZERO_ALLOC_COMPUTE_GRAPH.md` | A pesquisar (equipe de arquitetura / compute open source) | `0 - Identificado` | Executar experimento reproduzível CPU vs AVX2 vs `wgpu` em `jhcs-rs` antes de referência contextual | Média |
| **Intel** | EUA | `1-Pioneira` | CPU, SIMD, Idle States, Hardware | Kernels AVX2/FMA sem heap, alinhamento de cache line (`64 B`), telemetria `RDTSC`/`RDTSCP` e futuro isolamento VT-d | `Nivel B` | `web/artigos/auditoria-zero-idle-cache-prefetch-syscalls-jaha-jhcs.html`; `src/cpu.rs` | A pesquisar (Open Source Technology / pesquisa em sistemas) | `0 - Identificado` | Consolidar relatório em inglês sobre `RDTSCP` + *cache prefetching* / SIMD antes de contato | Média |
| **NVIDIA** | EUA | `1-Pioneira` | GPU, Workloads Paralelos | DAG heterogêneo (`jhcs-core`) avaliando quando **não** despachar lotes pequenos para GPU devido à latência de cópia | `Nivel C` | `jhcs-rs/crates/jhcs-backend-wgpu` | A pesquisar (grupos de computação paralela / pesquisa) | `0 - Identificado` | Manter em radar silencioso até existir benchmark heterogêneo multi-backend validado em hardware físico | Baixa |
| **Arm** | Reino Unido | `1-Pioneira` | Arquitetura, Edge, Eficiência | Portabilidade `aarch64` NEON 128-bit sem `std` para kernels `matmul_f32` / `conv2d_f32` em borda | `Nivel B` | `docs/strategic-issues/01_NEON_ARM64_SIMD.md` | A pesquisar (ecossistema Arm/embedded Rust) | `0 - Identificado` | Publicar RFC/artigo em inglês sobre implementação NEON em `jhcs-backend-simd` | Média |
| **Qualcomm** | EUA | `1-Pioneira` | SoC, Edge, Aceleradores | Execução heterogênea cooperativa e degradação graciosa sem acelerador secundário em dispositivos restritos | `Nivel C` | `jhcs-rs/README.md`; `docs/architecture/JINN_ADAPTIVE_HARDWARE_ARCHITECTURE.md` | A pesquisar | `0 - Identificado` | Manter em radar silencioso até maturidade do backend `aarch64` NEON | Baixa |

---

### 3.2. Frente B — Zero-Idle, Eficiência Operacional, SMP & Sistemas Embarcados

| Organização | País | Círculo | Segmento | Problema / Oportunidade Técnica | Nível | Publicações / Ativos Relacionados | Contato | Status | Próximo Passo | Prioridade |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Linaro** | Reino Unido / Global | `1-Pioneira` | Sistemas Embarcados, Eficiência, Arm | Padrões de arquitetura orientada a eventos, contagem per-CPU, interrupções e transição de *idle-states* em dispositivos restritos | `Nivel B` | `web/artigos/auditoria-zero-idle-smp-percpu-irq-apic-gating.html` | A pesquisar (working groups de kernel / power) | `0 - Identificado` | Publicar versão em inglês da auditoria Zero-Idle com separação clara entre `HLT` em QEMU e consumo em silício real | Alta |
| **Red Hat** | EUA / Global | `1-Pioneira` | Sistemas Operacionais, Kernel, Infraestrutura | Contabilidade per-CPU sem spinlock global, EOI incondicional 8259A/LAPIC e arquiteturas tickless | `Nivel B` | `web/artigos/auditoria-zero-idle-smp-percpu-irq-apic-gating.html`; `post-mortem-pic-eoi-zero-gpu.html` | A pesquisar (engenheiros de kernel/ecosistema) | `0 - Identificado` | Incluir pergunta aberta de engenharia sobre *tickless vs APIC timer gating* no LinkedIn/Observatory | Média |
| **Siemens** | Alemanha / Brasil | `1-Pioneira` | Edge Industrial, Automação | Perfis determinísticos de baixa sobrecarga (`Ifrit Industrial`), recuperação de driver em Ring 3 sem parada total de sistema | `Nivel C` | `docs/architecture/JINN_DRIVER_ISOLATION_ARCHITECTURE.md`; `docs/benchmarks/JINN_MEMORY_BUDGET.md` | A pesquisar (equipe técnica de automação / P&D local ou global) | `0 - Identificado` | Construir demonstração técnica objetiva de recuperação de driver sem panic + bancada física antes de abordar | Média |
| **Bosch** | Alemanha / Brasil | `1-Pioneira` | Embarcados, Automotivo/Industrial | Isolamento entre camadas de I/O, orçamento previsível *no-overcommit* e ausência de alocações dinâmicas em hot-paths | `Nivel C` | `docs/JAHA_12_SPECIFICATION.md`; `docs/benchmarks/JINN_MEMORY_BUDGET.md` | A pesquisar | `0 - Identificado` | Validar Jinn Profile Legacy / 64M–256M em placa x86 física para gerar evidência reproduzível | Média |

---

### 3.3. Frente C — Pilha `#![no_std]` Soberana (TLS 1.3, DNS/HotRoute, Aladdin Compositor) & Densidade de Memória

| Organização | País | Círculo | Segmento | Problema / Oportunidade Técnica | Nível | Publicações / Ativos Relacionados | Contato | Status | Próximo Passo | Prioridade |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Comunidade Rust Embedded / OSDev (`This Week in Rust`, `r/rust`, `r/osdev`)** | Global | `3-Academia` / Comunidade | Linguagem Rust `#![no_std]` & Microkernels | Reassembly multi-record TLS 1.3 em buffers `.bss` estáticos, aritmética Poly1305 sem alocador e compositor GUI `SSE2`/`AVX2` em Ring 3 | `Nivel A` (submissão comunitária estruturada) | `web/artigos/post-mortem-tls13-poly1305-fallback-http-telemetria-jinn-browser.html`; `web/artigos/aladdin-shared-rings-spsc-lock-free-zero-copy-io-uring.html` | Editores / PRs de comunidade (`rust-lang/this-week-in-rust`) | `1 - Investigado` | Submeter post-mortem TLS 1.3 / Poly1305 + Zero-Idle no próximo ciclo editorial | **Alta** |

---

### 3.4. Frente D — Círculos 2, 3 e 4 (Empresas Operacionais, Universidades e Instituições no Brasil)

> [!NOTE]
> Os registros desta tabela representam **nichos prioritários e alvos em prospecção ativa**. Conforme o pesquisador responsável identificar organizações nominais (ex.: empresas de automação industrial em MG/SP/SC, grupos de pesquisa de UFMG/USP/UNICAMP/UFRGS/UTFPR, institutos federais), cada linha deve ser desdobrada em entradas nominais individuais mantendo exatamente este esquema no [`opportunity_radar.csv`](opportunity_radar.csv).

| Organização / Alvo | País / Região | Círculo | Segmento | Problema / Oportunidade Técnica | Nível | Publicações / Ativos Relacionados | Contato | Status | Próximo Passo | Prioridade |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Empresas Regionais de Automação Industrial e IHM/SCADA (Polos MG/SP/SC)** | Brasil (MG / SP / SC) | `2-Operacional` | Automação Industrial, IHM, Hardware Legado | Máquinas industriais ou totens com hardware x86 modesto (256 MB–2 GB RAM) que sofrem com bloat de SOs convencionais ou travamento por driver em Ring 0 | `Nivel C` $\to$ Abordagem Direta | `docs/benchmarks/JINN_MEMORY_BUDGET.md`; `docs/architecture/JINN_DRIVER_ISOLATION_ARCHITECTURE.md` | A pesquisar — listar 5 empresas nominais de automação/integradores | `0 - Identificado` | Selecionar 5 empresas operacionais regionais reais com hardware embarcado x86 e enviar mensagem consultiva pedindo feedback de *workload* | **Alta** |
| **Fabricantes / Integradores Nacionais de Appliances de Rede & Gateways** | Brasil | `2-Operacional` | Segurança de Rede, Roteamento, Edge | Firewall SPI (`JinnWall`), resolução DNS 3 camadas com expiração *lazy*, `HotRouteTable` L2/L3 e perfil headless `Simbad` < 24 MB | `Nivel C` $\to$ Abordagem Direta | `web/artigos/tabela-rotas-quentes-dns-l2-fast-path-jinn-os.html`; `web/artigos/tabela-local-dns-hosts-cache-persistente-ttl-aladdin.html` | A pesquisar — listar 3 fabricantes/integradores de firewalls/appliances locais | `0 - Identificado` | Gravar vídeo demo (3–5 min) de *boot* + `HotRouteTable` 0 DNS/ARP após cache e usar na abordagem | **Alta** |
| **Grupos de Pesquisa em Sistemas Operacionais, Arquitetura e Segurança (Universidades Brasileiras — ex. UFMG, USP, UNICAMP, UFRGS, UFSC)** | Brasil | `3-Academia` | Pesquisa em SO, Sistemas Embarcados, Verificação | Bancada física de teste (x86_64 real com medição de potência elétrica real vs QEMU), auditoria de isolamento Ring 3 / KPTI e projetos de IC/mestrado | `Nivel B` $\to$ Abordagem Direta | `docs/JAHA_12_SPECIFICATION.md`; `docs/benchmarks/JINN_PERFORMANCE_BASELINE.md`; `web/artigos/auditoria-zero-idle-smp-percpu-irq-apic-gating.html` | A pesquisar — professores líderes de laboratórios de sistemas/arquitetura | `0 - Identificado` | Mapear 5 docentes/laboratórios com publicação recente em SBRC, WSCAD ou SBESC e enviar síntese técnica com convite para crítica e bancada | **Alta** |
| **Institutos Federais & Laboratórios Públicos de Hardware / Inclusão Tecnológica** | Brasil | `4-Instituicao` | Educação Técnica, Infraestrutura Pública | Uso experimental transparente em laboratórios didáticos de SO ou reuso de estações x86 modestas para ensino de programação em Rust/sistemas | `Nivel C` | `docs/benchmarks/JINN_MEMORY_BUDGET.md`; `README.md` | A pesquisar — coordenações de laboratórios de computação / IFs regionais | `0 - Identificado` | Documentar protocolo ético/didático antes de qualquer contato formal institucional | Baixa |
