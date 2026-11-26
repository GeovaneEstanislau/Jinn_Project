# Jinn Open Engineering Outreach — Templates Operacionais (EN / PT) e Pacotes "1 Experimento → 5 Ativos"

> **Documento Operacional Complementar a:** [`JINN_GLOBAL_INITIATIVE_ESTRATEGIA.md`](JINN_GLOBAL_INITIATIVE_ESTRATEGIA.md)  
> **Checklist Pré-Disparo Obrigatório:**
> 1. Existe evidência reproduzível anexada (commit, script `scripts/test_*.py`, PCAP ou log serial)?
> 2. O texto distingue explicitamente emulação (QEMU) de medição em hardware físico (especialmente energia)?
> 3. Se houver marcação social (Nível A), ela está restrita a **no máximo 2–3 organizações** com relação técnica direta e uma pergunta específica?
> 4. O contato individual **não** pede investimento de saída, oferecendo diálogo técnico ou caso de teste concreto?

---

## Parte 1 — Templates Canônicos de Abordagem Individual

### 1.1. Template em Inglês (Círculo 1 Internacional & Círculo 3 Acadêmico Global)

```text
Subject: Technical inquiry & reproducible report: [Specific Component / Problem] in Rust no_std (Jinn Project)

Hello [Name / Team Name],

We are developing Jinn, an experimental Rust-based (#![no_std]) microkernel and modular systems architecture focused on Ring 3 driver isolation, deterministic resource accounting and heterogeneous computing.

Recently, we published an engineering investigation into [specific technology or problem — e.g., TLS 1.3 Poly1305 arithmetic edge cases without heap / event-driven SMP scheduling / workload-aware SIMD-GPU selection], including our implementation, testing methodology, measured results and known limitations.

Given your team's work in [specific field/system], we thought this investigation might be relevant to your engineering ecosystem. We are interested in exchanging technical perspectives, identifying real-world workloads and exploring opportunities for collaboration, testing or further research.

The technical report and reproducible materials are available here:
- Engineering report: [URL on jinn-os.dev.br]
- Source code & test harness: [GitHub commit / script URL]

We would appreciate any technical feedback or perspective your team might be willing to share—especially regarding [one concrete engineering question].

Best regards,
[Your Name]
Jinn Project — Open Engineering Outreach
https://jinn-os.dev.br | https://github.com/GeovaneEstanislau/Jinn_Project
```

---

### 1.2. Template em Português — Círculo 2 (Empresas Operacionais, Automação e Integradores Regionais)

```text
Assunto: Troca técnica e validação em hardware real — arquitetura isolada em Rust de baixo footprint (Projeto Jinn)

Olá [Nome / Equipe Técnica],

Estou conduzindo o desenvolvimento do Projeto Jinn (https://jinn-os.dev.br), uma arquitetura experimental em Rust bare-metal com foco em três pontos práticos para equipamentos restritos e operação contínua:
1. Drivers e aplicações em espaço de usuário (Ring 3) com contenção de falhas sem derrubar o sistema inteiro;
2. Consumo previsível de memória sem overcommit (operação completa validada em perfis de 64 MB a 256 MB de RAM com auditoria contínua de páginas);
3. Composição gráfica em CPU via SIMD sem obrigatoriedade de GPU dedicada, além de perfil headless compacto para rede/edge.

Acompanho a atuação da [Nome da Empresa] em [automação / IHM / equipamentos / infraestrutura] e gostaria de ouvir a perspectiva técnica da sua equipe sobre um desafio específico: quais gargalos práticos de estabilidade, consumo de memória ou suporte a hardware legado/modesto vocês enfrentam hoje onde sistemas operacionais convencionais acabam sendo pesados demais?

Não estamos buscando venda fechada neste contato — nosso interesse agora é trocar perspectiva técnica com quem opera hardware em campo e, se fizer sentido mútuo no futuro, avaliar um experimento controlado ou piloto em hardware real de bancada.

Nosso código aberto e relatórios técnicos auditáveis estão disponíveis em:
- Portal de Engenharia: https://jinn-os.dev.br
- Repositório GitHub: https://github.com/GeovaneEstanislau/Jinn_Project

Se alguém da engenharia tiver disponibilidade para uma troca breve por e-mail ou 15 minutos de conversa técnica, fico inteiramente à disposição.

Atenciosamente,
[Seu Nome]
Jinn Project — Research. Engineering. Outreach. Collaboration.
```

---

### 1.3. Template em Português — Círculo 3 (Universidades, Laboratórios e Pesquisadores)

```text
Assunto: Proposta de diálogo técnico e validação experimental em hardware físico (Microkernel Rust / Zero-Idle / JAHA 1.2)

Prezado(a) Prof.(a) [Nome] / Grupo [Nome do Laboratório],

Tenho acompanhado os trabalhos do grupo na linha de [sistemas operacionais / arquitetura de computadores / embarcados / segurança] e gostaria de compartilhar uma arquitetura experimental aberta que estamos desenvolvendo e documentando de forma reproduzível: o Projeto Jinn.

Trata-se de um microkernel x86_64 escrito em Rust (#![no_std]) acompanhado de especificação de contabilidade física de memória (JAHA 1.2, com invariante auditada de 0 bytes de discrepância), drivers isolados em Ring 3, escalonamento orientado a eventos (Zero-Idle) e um middleware separado de computação heterogênea (jhcs-rs).

Uma limitação explícita que documentamos em nossos relatórios atuais é que nossas métricas de inatividade (`HLT` / contadores per-CPU `RDTSC`) foram medidas em QEMU, o que mede eficiência operacional do escalonador, mas não substitui medições elétricas de energia em silício real.

Caso haja aderência com linhas de pesquisa ou projetos de iniciação científica/mestrado do laboratório, gostaríamos muito de receber críticas sobre nossa metodologia e avaliar a possibilidade de reproduzir nossos experimentos em bancada física instrumentada ou propor temas abertos para estudantes interessados em sistemas/Rust.

Materiais abertos para inspeção rápida (execução reproduzível via QEMU/scripts Python):
- Portal e Relatórios: https://jinn-os.dev.br/artigos/
- Código e suíte de integridade: https://github.com/GeovaneEstanislau/Jinn_Project

Desde já agradeço pelo tempo e por qualquer crítica técnica que puderem compartilhar.

Atenciosamente,
[Seu Nome]
Jinn Project
```

---

## Parte 2 — Pacotes Prontos **"1 Experimento → 5 Ativos"**

### Pacote A (Pronto para Publicação Imediata) — *Zero-Idle, SMP Accounting & Interrupt Gating*

- **Base verificada no repositório:** `src/scheduler.rs`, `src/smp.rs`, `src/interrupts/pic.rs`; relatórios [`auditoria-zero-idle-smp-percpu-irq-apic-gating.html`](../../web/artigos/auditoria-zero-idle-smp-percpu-irq-apic-gating.html) e [`post-mortem-pic-eoi-zero-gpu.html`](../../web/artigos/post-mortem-pic-eoi-zero-gpu.html).
- **Nível de Marcação:** Nível B contextual no artigo (`Intel`/`AMD`/`x86_64 APIC`); Nível A criterioso no LinkedIn (**máx. 2–3**: ex. ecossistema **Linaro**, **Red Hat** ou comunidades **Rust/Embedded** com pergunta genuína de engenharia).

1. **Ativo 1 (Observatory `jinn-os.dev.br`):** *Jinn Zero-Idle: From Blocked Tasks to Event-Driven Execution — An Architectural Investigation* (versão EN consolidando os dados per-CPU e deixando explícito que `idle=100.00%` nos APs em QEMU mede *execution quiescence*, não consumo elétrico físico).
2. **Ativo 2 (GitHub):** Tag/commit reprodutível apontando diretamente para `scripts/test_dns_cache_e2e.py` e `scripts/test_hot_routes_e2e.py` (que imprimem `[PERCPU] cpus=4 net_irqs=... task=0 idle=100.00%` nos núcleos auxiliares).
3. **Ativo 3 (LinkedIn — ~850 caracteres, EN):**
```text
How should operating systems balance event-driven scheduling, APIC timer gating, idle-state transitions and I/O responsiveness under tight resource budgets?

In Jinn OS (an experimental x86_64 microkernel in #![no_std] Rust), we audited CPU activity across 4 vCPUs during network/UI quiescent windows:
• Replaced periodic polling with IRQ-driven RX wakeups and lazy TTL evaluation;
• Gated AP local timers cleanly under quiescent intervals;
• Measured per-CPU cycle accounting in QEMU (APs reaching ~100% HLT / zero background task ticks while idle).

Important distinction: QEMU HLT metrics prove scheduling quiescence, not physical silicon power savings — physical hardware power telemetry is our next step.

Full architectural report & reproducible QEMU scripts:
https://jinn-os.dev.br/artigos/auditoria-zero-idle-smp-percpu-irq-apic-gating.html

For engineers working on power-aware scheduling & embedded kernels (@Linaro @RedHat): where do tickless approaches usually regress first under bursty NIC interrupts?

#SystemsProgramming #RustLang #OSDev #Microkernel #EmbeddedSystems
```
4. **Ativo 4 (Roteiro de Vídeo YouTube — 4 a 6 min):**
   - `0:00–0:45`: O problema clássico de OSes que gastam CPU ociosa com polling/timers;
   - `0:45–2:00`: Arquitetura no Jinn (`IrqSafeSpinLock`, EOI incondicional PIC 8259A, AP timer gating);
   - `2:00–3:30`: Execução ao vivo do QEMU mostrando `[PERCPU]` antes/depois de interação de rede;
   - `3:30–4:30`: Limitações honestas (emulação TCG vs hardware real) + chamada para feedback técnico.
5. **Ativo 5 (Mastodon / `r/osdev` / `TWiR`):** Disparo automático via `scripts/syndicate_broadcast.py` com `--link https://jinn-os.dev.br/artigos/auditoria-zero-idle-smp-percpu-irq-apic-gating.html`.

---

### Pacote B (Pronto para Publicação Imediata) — *TLS 1.3 `#![no_std]`, Poly1305 Carry Bug & Browser Zero-Heap*

- **Base verificada no repositório:** [`post-mortem-tls13-poly1305-fallback-http-telemetria-jinn-browser.html`](../../web/artigos/post-mortem-tls13-poly1305-fallback-http-telemetria-jinn-browser.html), `apps/ola-mundo/src/tls.rs`, `src/net/tls13.rs` e commit `1b93f0c` (validado contra `https://jinn-os.dev.br` com `54,762` bytes + PNG/BMP remotos).
- **Nível de Marcação:** **Nível B / Nível C** (comunidade Rust `#![no_std]`, criptografia aplicada — **não** forçar marcação em fabricantes de hardware sem relação direta).

1. **Ativo 1 (Observatory):** Post-mortem em inglês / bilíngue detalhando a captura PCAP no QEMU e os dois bugs reais encontrados: aritmética de limbs `44/44/42` do Poly1305 (`|` vs `wrapping_add` no carry para o bit 88 em fluxos `>35 KiB`) + `close_notify` cifrado (`CONTENT_ALERT`) + reassembly multi-record.
2. **Ativo 2 (GitHub):** Commit `bcac193` + `1b93f0c` + inspeção de código em `apps/ola-mundo/src/tls.rs`.
3. **Ativo 3 (LinkedIn — ~780 caracteres, EN):**
```text
Debugging a subtle ChaCha20-Poly1305 failure at byte 37,049 inside a custom #![no_std] TLS 1.3 stack.

While fetching a 54 KB HTTPS payload in Jinn Browser (zero C dependencies, zero heap), our TLS 1.3 client succeeded for the handshake and first 28 application records—then failed AEAD verification cleanly at record #34.

Wire-level QEMU PCAP inspection ruled out packet loss. The root cause was a single carry propagation edge case in 44/44/42-bit Poly1305 limb finalization (`h1 >> 44` colliding via bitwise OR into `h2`). Replacing bit-OR packing with carry propagation + `wrapping_add` fixed real-world HTTPS fetches end-to-end.

Post-mortem, PCAP analysis & code:
https://jinn-os.dev.br/artigos/post-mortem-tls13-poly1305-fallback-http-telemetria-jinn-browser.html

#RustLang #Cryptography #TLS13 #NoStd #SystemsEngineering
```
4. **Ativo 4 (Vídeo YouTube — 3 a 5 min):** Mostrando `Jinn Browser` carregando `https://jinn-os.dev.br` (PNG sobre TLS 1.3), redirecionamento `68k.news`, e a diferença entre depurar no código vs auditar pacotes reais no PCAP.
5. **Ativo 5 (Reddit `r/rust` + `This Week in Rust`):** Foco puramente técnico em implementação `#![no_std]` de RFC 8446 / RFC 8439 e captura de tela real do navegador em Ring 3.

---

### Pacote C (Modelo para o Próximo Ciclo Técnico) — *JHCS Workload-Aware Execution Selection Experiment*

- **Base no ecossistema:** [`jhcs-rs`](../../../jhcs-rs/README.md) (`crates/jhcs-core`, `crates/jhcs-broker`, `crates/jhcs-backend-simd`, `crates/jhcs-backend-wgpu`), [`01_NEON_ARM64_SIMD.md`](../strategic-issues/01_NEON_ARM64_SIMD.md), [`02_ZERO_ALLOC_COMPUTE_GRAPH.md`](../strategic-issues/02_ZERO_ALLOC_COMPUTE_GRAPH.md).
- **Regra de Tom (Anti-Exagero):** *Nunca afirmar "JHCS é mais inteligente que frameworks atuais"*. Usar exatamente a tese experimental rigorosa abaixo antes de rodar e publicar os dados reais:

> *"We implemented a workload-aware execution selection experiment considering computation size, transfer overhead, execution latency and available resources. This report evaluates the trade-offs between scalar CPU execution, SIMD and GPU execution under a reproducible test environment."*

- **Nível de Marcação (somente após o benchmark real ser executado e publicado):** Nível A seletivo (máx. 2–3, ex.: **AMD**, **Arm**, **Intel**) acompanhado da pergunta pública:
  *"At what payload sizes and memory transfer latencies does dynamic CPU-SIMD vs GPU switching stop paying off in #![no_std] / edge compute DAGs, and how do teams model those cutoffs without runtime allocation overhead?"*
