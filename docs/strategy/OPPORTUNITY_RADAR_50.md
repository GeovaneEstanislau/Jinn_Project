# Radar Estratégico das 50 Organizações + 6ª Trilha Aberta (`OPPORTUNITY_RADAR_50`)

> **Documento Operacional da Frente 3** | **Estratégia Mãe:** [`../outreach/JINN_GLOBAL_INITIATIVE_ESTRATEGIA.md`](../outreach/JINN_GLOBAL_INITIATIVE_ESTRATEGIA.md)  
> **Política de Honestidade:** Nenhuma afirmação experimental ([`../ENGINEERING_STATUS_AND_LIMITATIONS.md`](../ENGINEERING_STATUS_AND_LIMITATIONS.md)) deve ser apresentada como pronta; pessoas nominais e contatos diretos permanecem em investigação ética sem presunção de nomes fictícios.

---

## Metodologia de Pontuação (`0` a `5` por Critério, Máximo `30`)

Cada organização é avaliada em seis dimensões objetivas:
1. **`Compat` (`0–5`):** Compatibilidade técnica com os ativos transferíveis (`jhcs-rs`, `Zero-Idle`, `Ring 3 Isolation`, `AladdinSharedRings`, `HotRouteTable`/TLS 1.3, `JAHA 1.2`).
2. **`Problema` (`0–5`):** Relevância da dor real atacada (overhead de drivers monolíticos, despacho GPU ineficiente em micro-lotes, bloat de RAM, polling ocioso).
3. **`Prog_Inov` (`0–5`):** Existência de programas abertos de inovação, OSPO, bolsas de pesquisa ou editais técnicos.
4. **`Colab` (`0–5`):** Histórico de colaboração com pesquisadores independentes e projetos open source `#![no_std]` / Rust.
5. **`Financ` (`0–5`):** Potencial financeiro ou de fomento/piloto/licenciamento comercial.
6. **`Acessib` (`0–5`):** *Custo de aproximação inverso* (`5` = acesso comunitário/programático de baixíssimo atrito; `1` = altíssimo atrito corporativo).

---

## Grupo 1 (1–10): CPU, GPU, SoC & Semicondutores

| # | Organização | Região | Cargo Técnico Alvo | Ativo Jinn Aderente | Compat | Problema | Prog_Inov | Colab | Financ | Acessib | Total |
| :-: | :--- | :--- | :--- | :--- | :-: | :-: | :-: | :-: | :-: | :-: | :-: |
| 1 | **AMD** | EUA / Global | Principal Heterogeneous Compute Architect | `jhcs-rs`, AVX2/FMA SIMD, `wgpu` breakeven | 5 | 4 | 4 | 4 | 5 | 2 | **24** |
| 2 | **Intel** | EUA / Global | Open Source Technology & Power/Idle Lead | `Zero-Idle`, cache-line `64 B`, SIMD, futuro VT-d | 5 | 4 | 4 | 3 | 5 | 2 | **23** |
| 3 | **Arm** | Reino Unido | Staff Embedded Rust & NEON Ecosystem Lead | `jhcs-backend-simd` NEON (`01_NEON_ARM64_SIMD.md`) | 4 | 5 | 4 | 4 | 5 | 3 | **25** |
| 4 | **NVIDIA** | EUA / Global | Compute Scheduling & Edge Runtime Architect | `jhcs-broker` (evitar despacho GPU sub-breakeven) | 4 | 3 | 4 | 3 | 5 | 1 | **20** |
| 5 | **Qualcomm** | EUA / Global | Edge AI & Heterogeneous SoC Systems Lead | `jhcs-core` fallback cooperativo em SoC | 4 | 4 | 3 | 2 | 5 | 2 | **20** |
| 6 | **SiFive / RISC-V Ecossistema** | EUA / Global | OS Kernel & Bare-Metal Toolchain Architect | Microkernel `#![no_std]` e portabilidade futura | 4 | 4 | 4 | 5 | 4 | 4 | **25** |
| 7 | **Tenstorrent** | Canadá / EUA | Systems Software & Compiler Runtime Lead | DAG `jhcs-core` & dispatch sem alocação | 4 | 4 | 3 | 4 | 4 | 3 | **22** |
| 8 | **Ampere Computing** | EUA | Cloud-Native Efficiency & Core Accounting Lead | `Syscall 88` SMP per-CPU & Zero-Idle gating | 4 | 4 | 3 | 3 | 4 | 3 | **21** |
| 9 | **Imagination Technologies** | Reino Unido | Heterogeneous Compute & Embedded GPU Lead | `jhcs-broker` + compositor Ring 3 Zero-GPU | 4 | 4 | 3 | 3 | 4 | 3 | **21** |
| 10 | **Raspberry Pi Ltd** | Reino Unido | Systems Software & Low-Overhead OS Engineer | `Aladdin` Ring 3 SIMD & `64 MiB`–`256 MiB` RAM | 3 | 5 | 3 | 4 | 3 | 4 | **22** |

---

## Grupo 2 (11–20): Edge Computing, Indústria & Automação

| # | Organização | Região | Cargo Técnico Alvo | Ativo Jinn Aderente | Compat | Problema | Prog_Inov | Colab | Financ | Acessib | Total |
| :-: | :--- | :--- | :--- | :--- | :-: | :-: | :-: | :-: | :-: | :-: | :-: |
| 11 | **Siemens** | Alemanha / BR | Industrial Edge Runtime & Reliability Architect | Isolamento de drivers Ring 3 sem kernel panic | 4 | 5 | 4 | 3 | 5 | 2 | **23** |
| 12 | **Bosch** | Alemanha / BR | Embedded Platform & Deterministic OS Lead | `JAHA 1.2` (`no-overcommit`) & `.bss` sem heap | 4 | 5 | 4 | 3 | 5 | 2 | **23** |
| 13 | **Toradex** | Suíça / Brasil | Embedded Systems & Industrial OS Partner Lead | Microkernel compacto + HMI Zero-GPU `Aladdin` | 4 | 5 | 4 | 4 | 4 | 4 | **25** |
| 14 | **Advantech** | Taiwan / Global | Edge Appliance & Industrial Gateway Architect | `Simbad`, `HotRouteTable` & IHM modesta `x86` | 4 | 4 | 3 | 3 | 4 | 3 | **21** |
| 15 | **Schneider Electric** | França / Global | Industrial Control & Field Device Security Lead | Isolamento estrito `W^X` & `JinnWall` SPI | 3 | 4 | 4 | 3 | 5 | 2 | **21** |
| 16 | **Beckhoff Automation** | Alemanha | Real-Time Industrial PC & EtherCAT Lead | Contabilidade de latência, Ring 3 isolation | 3 | 4 | 3 | 3 | 4 | 3 | **20** |
| 17 | **ABB** | Suíça / Global | Robotics Compute & Edge Telemetry Architect | `jhcs-rs` CPU/SIMD determinístico & `Zero-Idle` | 3 | 4 | 4 | 3 | 5 | 2 | **21** |
| 18 | **WEG (Automação / Digital)** | Brasil (SC) | Arquitetura de Software Embarcado & IIoT Lead | IHM leve, confiabilidade FAT32 `.TMP/.BAK` | 4 | 5 | 4 | 4 | 4 | 4 | **25** |
| 19 | **Edge Impulse** | EUA / Global | Embedded ML Runtime & On-Device Compute Lead | `jhcs-backend-simd` `#![no_std]` (`04_RING3_EDGE`) | 5 | 4 | 4 | 4 | 4 | 3 | **24** |
| 20 | **Arduino Pro** | Itália / Global | Firmware & Low-Footprint Compute Architect | `jhcs-core` `#![no_std]` em borda | 4 | 4 | 3 | 4 | 3 | 4 | **22** |

---

## Grupo 3 (21–30): Cloud, Infraestrutura, Redes & Hardware Aberto

| # | Organização | Região | Cargo Técnico Alvo | Ativo Jinn Aderente | Compat | Problema | Prog_Inov | Colab | Financ | Acessib | Total |
| :-: | :--- | :--- | :--- | :--- | :-: | :-: | :-: | :-: | :-: | :-: | :-: |
| 21 | **Oxide Computer Company** | EUA | Rack Systems & Embedded Rust Kernel Engineer | Rust `#![no_std]`, drivers fora do núcleo, IPC | 5 | 5 | 3 | 4 | 4 | 3 | **24** |
| 22 | **Cloudflare** | EUA / Global | Systems & Cryptographic Protocol Engineer | **TLS 1.3 `#![no_std]`** & DNS 3 camadas / fast-path | 4 | 4 | 4 | 4 | 5 | 3 | **24** |
| 23 | **Red Hat (IBM OS Group)** | EUA / Global | Open Source Program Office (OSPO) / Kernel Team | Contabilidade per-CPU SMP, EOI & arquitetura tickless | 4 | 4 | 5 | 5 | 5 | 3 | **26** |
| 24 | **Fastly** | EUA | Edge Compute & Sandboxing Architect | Isolamento forte Ring 3 + startup sub-segundo | 4 | 4 | 3 | 3 | 4 | 3 | **21** |
| 25 | **Canonical** | Reino Unido | Ubuntu Core / Embedded Appliance Architect | Comparativo de densidade RAM `64M` vs Linux | 3 | 4 | 4 | 4 | 4 | 3 | **22** |
| 26 | **SUSE** | Alemanha | Edge Linux & Systems Engineering Lead | Isolamento e perfil *appliance* minimalista | 3 | 3 | 3 | 4 | 4 | 3 | **20** |
| 27 | **Netgate** | EUA | Firewall & Routing Appliance Architect | `JinnWall` SPI, `HotRouteTable` L2/L3 & DNS cache | 4 | 5 | 2 | 3 | 4 | 3 | **21** |
| 28 | **Framework Computer** | EUA | Firmware & Modular Platform Lead | Hardware reparável + extensão de ciclo de vida | 3 | 4 | 3 | 4 | 3 | 4 | **21** |
| 29 | **Scaleway** | França | Bare-Metal & Sustainable Cloud Efficiency Lead | Eficiência ociosa e perfis servidor leves (`Simbad`) | 3 | 4 | 3 | 3 | 4 | 3 | **20** |
| 30 | **Tailscale / WireGuard-Rust** | Canadá / EUA | Network Data-Plane & Userspace Routing Lead | Pilha `#![no_std]` sem dependências C e cache L2/L3 | 4 | 4 | 3 | 4 | 4 | 3 | **22** |

---

## Grupo 4 (31–40): Sistemas Críticos, Segurança & Engenharia Low-Level

| # | Organização | Região | Cargo Técnico Alvo | Ativo Jinn Aderente | Compat | Problema | Prog_Inov | Colab | Financ | Acessib | Total |
| :-: | :--- | :--- | :--- | :--- | :-: | :-: | :-: | :-: | :-: | :-: | :-: |
| 31 | **Linaro** | Reino Unido | Kernel Working Group / Power Efficiency Lead | `Zero-Idle`, gating de interrupções e SMP | 5 | 5 | 4 | 5 | 4 | 4 | **27** |
| 32 | **Ferrous Systems** | Alemanha | Ferrocene / Safety-Critical Rust Architect | Rust bare-metal `#![no_std]`, sem heap em hot-paths | 5 | 4 | 3 | 4 | 4 | 4 | **24** |
| 33 | **seL4 Foundation / Proofcraft** | Austrália / Global | Microkernel Verification & Capability Lead | Isolamento L0 mínimo & contabilidade formal `JAHA` | 5 | 4 | 4 | 4 | 3 | 4 | **24** |
| 34 | **Trail of Bits** | EUA | Low-Level Security & Systems Audit Practice | Auditoria syscall boundary (`EFAULT`), TLS 1.3 `#![no_std]` | 4 | 4 | 3 | 4 | 4 | 3 | **22** |
| 35 | **Quarkslab** | França | Kernel Hardening & Crypto Implementation Lead | Poly1305 / X25519 / X.509 DER sem libc | 4 | 4 | 3 | 4 | 4 | 3 | **22** |
| 36 | **Cyberus Technology** | Alemanha | Microkernel, Virtualization & x86_64 Security | `x86_64` PML4 `W^X`, APIC/PIC & driver sandboxing | 5 | 5 | 3 | 4 | 4 | 4 | **25** |
| 37 | **Immunant / Prossimo (ISRG)** | EUA | Memory-Safe Critical Infrastructure Director | Substituição de componentes C por `#![no_std]` Rust | 5 | 5 | 5 | 4 | 4 | 4 | **27** |
| 38 | **System76 (COSMIC Team)** | EUA | Desktop Rust & Compositor Engineering Lead | **`Aladdin` (`MIT OR Apache-2.0`)**, `DirtyRect`, SPSC | 4 | 4 | 3 | 4 | 3 | 4 | **22** |
| 39 | **Mullvad / System Transparency** | Suécia | Bootable Verifiable Appliance Lead | `Jinn` boot enxuto, rede auditável e sem telemetria | 4 | 4 | 4 | 4 | 4 | 4 | **24** |
| 40 | **Eclipse Embedded Rust WG** | Global | Embedded / OS WG Maintainer | `Aladdin` trait contracts & `jhcs-core` `#![no_std]` | 5 | 4 | 4 | 5 | 3 | 5 | **26** |

---

## Grupo 5 (41–50): Pesquisa Deep Tech, Fundações & Fomento (Grants)

| # | Organização | Região | Cargo / Programa Alvo | Ativo Jinn Aderente | Compat | Problema | Prog_Inov | Colab | Financ | Acessib | Total |
| :-: | :--- | :--- | :--- | :--- | :-: | :-: | :-: | :-: | :-: | :-: | :-: |
| 41 | **NLnet Foundation (`NGI Zero`)** | Países Baixos / EU | NGI Zero Core / Entrust Program Manager | Microkernel soberano + TLS 1.3 + `Aladdin` ([`NLNET_GRANT_APPLICATION_PROPOSAL.md`](../NLNET_GRANT_APPLICATION_PROPOSAL.md)) | 5 | 5 | 5 | 5 | 5 | 5 | **30** |
| 42 | **Sovereign Tech Agency (STF)** | Alemanha / EU | Open Source Infrastructure Fellowship Lead | Pilha `#![no_std]` memória-segura & compositor | 4 | 5 | 5 | 4 | 5 | 4 | **27** |
| 43 | **Rust Foundation** | Global | Infrastructure & Safety-Critical Grants Lead | Ferramentas/arquiteturas `#![no_std]` & `jhcs-rs` | 5 | 4 | 5 | 5 | 4 | 5 | **28** |
| 44 | **Prototype Fund** | Alemanha / Open | Public-Interest Open Software Grant Jury | Reuso de hardware legado & infra auditável | 4 | 5 | 5 | 5 | 4 | 4 | **27** |
| 45 | **CNPq / FAPEMIG / FINEP** | Brasil (MG / BR) | Editais Deep-Tech, Subvenção & BOLSAS PD&I | Pesquisa OS/Hardware em parceria acadêmico-industrial | 4 | 5 | 4 | 4 | 4 | 4 | **25** |
| 46 | **OpenSSF (Linux Foundation)** | Global | Memory Safety & Supply-Chain Working Group | Kernel com 0 dependências `crates.io` + `W^X` | 4 | 4 | 4 | 4 | 4 | 4 | **24** |
| 47 | **Mozilla Builders / Future** | Global | Independent Browser & Open Web Tech Grants | **Jinn Browser** minimalista sem engine Blink/WebKit | 4 | 4 | 5 | 4 | 4 | 4 | **25** |
| 48 | **CERN Openlab / HEP Compute** | Suíça / Global | Heterogeneous Dispatch & Data Pipeline Lead | `jhcs-rs` DAG broker & benchmarking reprodutível | 4 | 4 | 4 | 4 | 4 | 3 | **23** |
| 49 | **SIDIA / Eldorado / CPQD (ICTs BR)** | Brasil | Pesquisa Aplicada Sistemas Embarcados & IIoT | Bancada física x86/ARM + drivers Ring 3 + Edge | 4 | 4 | 4 | 4 | 4 | 4 | **24** |
| 50 | **Eclipse Foundation** | Europa / Global | OpenHW / SDV / ThreadX / Embedded Rust Lead | Middleware modular `jhcs-rs` & perfis edge | 4 | 4 | 4 | 4 | 4 | 4 | **24** |

---

## 6ª Trilha Aberta Prioritária: Ecossistema Regional, Acadêmico e Público Eticamente Responsável

Para não depender exclusivamente de grandes corporações globais de ciclo decisório lento, esta trilha paralela estabelece portas de entrada diretas e verificáveis em três frentes de alta agilidade:

| Frente da 6ª Trilha | Perfis Elegíveis | Proposta Técnica Concreta do Jinn | Benefício Mútuo (Piloto / Bancada) | Condição Ética & Auditabilidade |
| :--- | :--- | :--- | :--- | :--- |
| **1. Indústria Local & Automação** | Empresas de automação industrial, fabricantes nacionais de totens, IHMs SCADA, gateways e integradores de hardware (`MG`, `SP`, `SC`, `PR`, `RS`). | Testar `Aladdin` (`MIT OR Apache-2.0`), `jhcs-rs` (`Apache-2.0`) ou perfil mínimo `64 MiB–256 MiB` em controladores x86 reais onde drivers fechados ou sistemas inchados causam falhas/obsolescência. | O Jinn obtém *workloads* industriais reais e acesso a placas físicas; o parceiro ganha autonomia técnica, isolamento de falhas e zero licença por assento em testes-piloto. | Nenhuma publicação de dados operacionais, topologias de planta ou hardware do cliente sem autorização formal escrita. |
| **2. Academia & Grupos de Pesquisa** | Departamentos de Ciência/Engenharia da Computação (ex.: **UFMG, UEMG, USP, UNICAMP, UFRGS, UFSC e Institutos Federais — IFMG, IFSP, CEFETs**). | Bancadas experimentais para medição elétrica **real** em silício (RAPL/wattímetro comparando `HLT` vs Linux), verificação formal/fuzzing da ABI `SYSCALL`, extensão ARM64 NEON em `jhcs-rs` e temas de IC/TCC/Mestrado. | O projeto resolve as lacunas da **Tabela B** ([`ENGINEERING_STATUS_AND_LIMITATIONS.md`](../ENGINEERING_STATUS_AND_LIMITATIONS.md)) com rigor científico e coautoria; alunos trabalham em codebase Rust `#![no_std]` completo e legível. | Todo método, seed, código e script de reprodução permanece aberto e auditável (ciência aberta). |
| **3. Setor Público Eticamente Responsável** | Laboratórios de escolas técnicas, órgãos públicos de transparência, infraestruturas cívicas e programas de recondicionamento responsável de computadores. | Aproveitamento sustentável de máquinas x86 funcionais com memória restrita para terminais de consulta, quiosques estáticos seguros sem telemetria oculta e ambientes educacionais. | Redução mensurável de descarte eletrônico (*e-waste*) e dependência de software proprietário com atualização forçada de hardware. | **Cláusula Ética Obrigatória:** Vedado uso para vigilância invasiva, rastreamento opaco ou alegação falsa de certificação militar/governamental; código 100% inspecionável. |

---

## Top 10 Prioritário para Aproximação Imediata (Pós-Frentes 4 e 5)

1. **NLnet Foundation (`NGI Zero`)** (`30/30`) — Aderência integral com [`NLNET_GRANT_APPLICATION_PROPOSAL.md`](../NLNET_GRANT_APPLICATION_PROPOSAL.md).
2. **Rust Foundation** (`28/30`) — `jhcs-rs` + infraestrutura `#![no_std]`.
3. **Linaro** (`27/30`) — Zero-Idle event gating & futuras evoluções ARM64.
4. **Immunant / Prossimo** (`27/30`) — Pilha TLS 1.3 `#![no_std]` & drivers seguros em memória.
5. **Sovereign Tech Agency (STF)** (`27/30`) & **Prototype Fund** (`27/30`) — Soberania e reuso sustentável.
6. **Red Hat (OSPO / Kernel)** (`26/30`) & **Eclipse Embedded Rust WG** (`26/30`) — Discussões abertas de escalonamento e contratos portáveis.
7. **Toradex** (`25/30`), **WEG** (`25/30`) & **Arm** (`25/30`) + **6ª Trilha Regional / Universidades MG–SP–BR** — Pilotos em placas físicas reais para fechar a Tabela B.
