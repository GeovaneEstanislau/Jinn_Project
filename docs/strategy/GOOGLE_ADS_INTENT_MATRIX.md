# Matriz de Intenção B2B/Deep Tech — Google Ads & Negativas (`GOOGLE_ADS_INTENT_MATRIX`)

> **Conta / Tag Base:** `AW-884426711` (Google Tag ativa em `https://jinn-os.dev.br/`)  
> **Estratégia de Orçamento Enxuto:** Usar **exclusivamente correspondência Exata (`[...]`) e de Frase (`"..."`)**, bloqueando correspondência ampla para evitar tráfego genérico de consumo. Todos os anúncios devem refletir estritamente o estado auditado em [`../ENGINEERING_STATUS_AND_LIMITATIONS.md`](../ENGINEERING_STATUS_AND_LIMITATIONS.md).

---

## 1. Convenção Padronizada de Rastreamento UTM

Todas as URLs finais apontam para landing pages rápidas e específicas em `https://jinn-os.dev.br/solucoes/`:

| Grupo | Campanha (`utm_campaign`) | URL de Destino Completa com UTM |
| :--- | :--- | :--- |
| **Grupo A** | `jinn_b2b_grp_a_jhcs_compute` | `https://jinn-os.dev.br/solucoes/computacao-heterogenea-jhcs.html?utm_source=google&utm_medium=cpc&utm_campaign=jinn_b2b_grp_a_jhcs_compute&utm_content={creative}` |
| **Grupo B** | `jinn_b2b_grp_b_edge_zero_idle` | `https://jinn-os.dev.br/solucoes/edge-industrial-zero-idle.html?utm_source=google&utm_medium=cpc&utm_campaign=jinn_b2b_grp_b_edge_zero_idle&utm_content={creative}` |
| **Grupo C** | `jinn_b2b_grp_c_zero_trust_iso` | `https://jinn-os.dev.br/solucoes/infraestrutura-auditavel-zero-trust.html?utm_source=google&utm_medium=cpc&utm_campaign=jinn_b2b_grp_c_zero_trust_iso&utm_content={creative}` |
| **Grupo D** | `jinn_b2b_grp_d_radar_expandido` | `https://jinn-os.dev.br/solucoes/edge-industrial-zero-idle.html?utm_source=google&utm_medium=cpc&utm_campaign=jinn_b2b_grp_d_radar_expandido&utm_content={creative}` |

---

## 2. Grupos de Anúncios Orientados à Dor Técnica

### Grupo A — Heterogeneous Compute & SIMD (`jhcs-rs`)
- **Perfil do buscador:** Arquitetos de performance, engenheiros Rust/HPC, times otimizando *offload* CPU SIMD vs GPU.
- **Landing Page:** [`web/solucoes/computacao-heterogenea-jhcs.html`](../../web/solucoes/computacao-heterogenea-jhcs.html)
- **Palavras-chave (Exata & Frase):**
  - `[heterogeneous compute rust]`
  - `"heterogeneous compute rust"`
  - `[simd vs gpu breakeven]`
  - `"simd vs gpu breakeven"`
  - `"zero copy ring buffer ipc"`
  - `[low latency compute scheduler]`
  - `"rust no_std avx2 fma"`
  - `"wgpu matrix multiplication overhead"`
- **Títulos (máx. 30 caracteres):**
  1. `SIMD vs GPU Breakeven Rust` (26 chars)
  2. `Heterogeneous Compute Broker` (28 chars)
  3. `Apache-2.0 DAG Scheduler` (24 chars)
- **Descrições (máx. 90 caracteres):**
  1. `Benchmarked Scalar, AVX2/FMA & wgpu: avoid slow small-batch GPU offloads on x86_64.` (83 chars)
  2. `Open-source Rust no_std + host DAG broker with 64B cache alignment & reproducible data.` (87 chars)
  3. `Cut PCIe transfer overhead using measured breakeven thresholds from 8x8 to 1024x1024.` (85 chars)

### Grupo B — Edge Runtime & Zero-Idle Efficiency (`Simbad` / Microkernel)
- **Perfil do buscador:** Engenheiros de sistemas embarcados, automação, hardware *fanless* e edge computing x86 modesto.
- **Landing Page:** [`web/solucoes/edge-industrial-zero-idle.html`](../../web/solucoes/edge-industrial-zero-idle.html)
- **Palavras-chave (Exata & Frase):**
  - `[deterministic embedded runtime rust]`
  - `"deterministic embedded runtime rust"`
  - `[ultra low power edge os]`
  - `"microkernel industrial automation"`
  - `"fanless edge computing os"`
  - `"zero idle rust microkernel"`
  - `[embedded x86 low ram os]`
- **Títulos (máx. 30 caracteres):**
  1. `Zero-Idle Rust Microkernel` (26 chars)
  2. `Deterministic Edge Runtime` (26 chars)
  3. `Fanless Industrial x86 OS` (25 chars)
- **Descrições (máx. 90 caracteres):**
  1. `Event-driven HLT gating with 0.00% idle AP CPU in QEMU & per-CPU SMP cycle accounting.` (86 chars)
  2. `Static .bss buffers, Ring 3 driver isolation and 64B HotRouteTable cache for edge nodes.` (88 chars)
  3. `Built for modest 64-256 MiB x86 hardware without background polling or desktop bloat.` (85 chars)

### Grupo C — Zero-Trust Isolation & Sovereign Systems
- **Perfil do buscador:** Engenheiros de segurança low-level, auditores de cadeia de suprimentos de software, sistemas críticos.
- **Landing Page:** [`web/solucoes/infraestrutura-auditavel-zero-trust.html`](../../web/solucoes/infraestrutura-auditavel-zero-trust.html)
- **Palavras-chave (Exata & Frase):**
  - `[capability based microkernel]`
  - `"auditable operating system rust"`
  - `"isolated user space drivers"`
  - `[sovereign infrastructure os]`
  - `"zero dependencies rust kernel"`
  - `"no_std tls 1.3 rust"`
- **Títulos (máx. 30 caracteres):**
  1. `Auditable Rust Microkernel` (26 chars)
  2. `Isolated Ring 3 Drivers OS` (26 chars)
  3. `Zero crates.io Kernel Code` (26 chars)
- **Descrições (máx. 90 caracteres):**
  1. `PML4 W^X enforcement, -EFAULT syscall boundary validation & SHA-256 .jpkg verification.` (87 chars)
  2. `JAHA 1.2 physical accounting (0B discrepancy in QEMU) with no hidden third-party crates.` (88 chars)
  3. `Separate fault domains for network, UI compositor & TLS 1.3 away from Ring 0 kernel.` (84 chars)

### Grupo D — Radar Expandido (Automação Regional, Academia & Setor Público)
- **Perfil do buscador:** Integradores de IHMs/PDVs no Brasil/LATAM/UE, docentes/pesquisadores em SO e gestores de TI pública auditável.
- **Landing Page:** [`web/solucoes/edge-industrial-zero-idle.html`](../../web/solucoes/edge-industrial-zero-idle.html) (com foco regional/acadêmico e link direto para [`infraestrutura-auditavel-zero-trust.html`](../../web/solucoes/infraestrutura-auditavel-zero-trust.html))
- **Palavras-chave (Exata & Frase):**
  - `"sistema operacional microkernel rust"`
  - `"ihm industrial x86 baixo consumo"`
  - `"software livre auditavel setor publico"`
  - `"compositor grafico rust sem gpu"`
  - `"pesquisa microkernel rust universidades"`
  - `"automacao industrial x86 fanless"`
- **Títulos (máx. 30 caracteres):**
  1. `IHM Rust Zero-GPU 64MB RAM` (26 chars)
  2. `SO Auditavel Setor Publico` (26 chars)
  3. `Parceria Pesquisa OS Rust` (25 chars)
- **Descrições (máx. 90 caracteres):**
  1. `Arquitetura #![no_std] auditavel para automacao industrial, laboratorios e TI publica.` (86 chars)
  2. `Compositor Ring 3 Aladdin (MIT/Apache-2.0) + persistencia FAT32 .TMP/.BAK em x86 leve.` (86 chars)
  3. `Bancadas abertas para medicao fisica em silicio, HPC heterogeneo e reuso sem e-waste.` (85 chars)

---

## 3. Lista Mestra de Palavras-Chave Negativas (Aplicação Global na Conta)

Excluir cliques curiosos, tráfego de usuário final doméstico sem perfil técnico, gamers, pirataria e buscas por emprego:

| Categoria de Exclusão | Negativas (Exata / Frase) |
| :--- | :--- |
| **Consumo / Pirataria / Grátis genérico** | `"download grátis"`, `"baixar gratis"`, `"windows crack"`, `"ativador"`, `"serial key"`, `"iso windows"`, `"formatar pc"`, `"celular"`, `"android apk"`, `"iphone"` |
| **Jogos & Entretenimento** | `"jogos"`, `"gaming os"`, `"fps booster"`, `"minecraft"`, `"valorant"`, `"steam os"`, `"emulador ps2"`, `"placa de video gamer"` |
| **Carreira / Cursos básicos / Suporte** | `"emprego"`, `"vaga"`, `"estagio"`, `"salario"`, `"curso gratuito"`, `"aula online"`, `"apostila pdf"`, `"como instalar driver"` |
| **Tokens / Cripto especulativa** | `"airdrop"`, `"criptomoeda gratis"`, `"comprar token"`, `"web3 faucet"`, `"minerar bitcoin"` |

---

## 4. Critérios de Otimização Semanal (Campanha Enxuta)
1. **Revisão de termos de pesquisa a cada 48h** nos primeiros 14 dias: negativar imediatamente qualquer termo fora de contexto de engenharia/B2B.
2. **Métrica principal de conversão no Tag Manager (`AW-884426711`):** clique no CTA técnico de piloto (`/#win-contact`), cópia do comando de reprodução (`cargo run --release -p benchmark-breakeven`) ou acesso aos relatórios auditados no GitHub.

---

## 5. Anexo Técnico & CTA Executivo (`STRATEGIC_TECHNOLOGY_BRIEF`)
- **Documento Base de Alinhamento B2B/Deep Tech:** [`STRATEGIC_TECHNOLOGY_BRIEF.md`](STRATEGIC_TECHNOLOGY_BRIEF.md) (12 slides em inglês técnico com arquitetura Ring 0/3, modelo modular *Zero-Idle* + LAPIC *One-Shot* tickless, contratos `Aladdin` e benchmarks `jhcs-rs`).
- **CTA Pós-Clique Recomendado nas Landing Pages e Resposta a Leads:** *"Acesse o [Strategic Technology Brief (12 Slides)](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/docs/strategy/STRATEGIC_TECHNOLOGY_BRIEF.md) para revisar fronteiras auditadas (Tabela A/B), arquitetura Zero-Copy `AladdinSharedRings` e propor um piloto técnico em [`jinn-os.dev.br/#win-contact`](https://jinn-os.dev.br/#win-contact)."*
