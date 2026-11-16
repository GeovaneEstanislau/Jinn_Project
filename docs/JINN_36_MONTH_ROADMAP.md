# 🗺️ Jinn Project — Roadmap Estratégico e Técnico de 36 Meses (2026–2029)
## Planejamento em Fases, Marcos Canônicos de Maturidade, Alocação de Recursos e Critérios de Aceitação

> **Documento Oficial de Engenharia do Jinn Project — Documento 9 de 11**  
> **Data de Referência:** 16 de Novembro de 2026  
> **Horizonte de Planejamento:** Novembro de 2026 a Novembro de 2029 (36 Meses)  
> **Status:** Concluído e Auditado | **Classificação:** Planejamento Estratégico e Técnico Canônico  

---

## 1. Visão Geral das Fases de Evolução

O roadmap de 36 meses estrutura o desenvolvimento do Jinn Project em quatro horizontes temporais encadeados, assegurando que o fortalecimento da segurança e estabilidade preceda a expansão de produtos comerciais:

```mermaid
flowchart LR
    Phase1["Curto Prazo (M1-M3)\nNov/26 – Fev/27\nConsolidação & Estabilização\n(Etapas A e B)"]
    Phase2["Médio Prazo (M4-M12)\nMar/27 – Nov/27\nServiços Ring 3 & Computação\n(Etapas C e D)"]
    Phase3["Longo Prazo (M13-M24)\nDez/27 – Nov/28\nHardware, DMA & Produtos\n(Etapas E e F)"]
    Phase4["Consolidação (M25-M36)\nDez/28 – Nov/29\nValidação Externa & Mercado\n(Etapa G)"]

    Phase1 --> Phase2
    Phase2 --> Phase3
    Phase3 --> Phase4
```

---

## 2. Detalhamento das Etapas e Entregas

### 2.1. Curto Prazo (Meses 1 a 3: Novembro de 2026 – Fevereiro de 2027)
**Foco:** Resolução de Falhas Críticas de Espaço de Usuário, Isolamento de Memória e Baseline de Estabilidade

#### Atividade 1.1: Correção do Congelamento no File Manager e Renderização do Browser em Ring 3
* **Período:** Mês 1 (Novembro – Dezembro de 2026)
* **Objetivo:** Eliminar o travamento do desktop ao clicar em "Arquivos" e garantir que a janela cliente do Jinn Browser seja exibida no Aladdin com carregamento de páginas HTML5.
* **Entregáveis:**
  * Patch em `Aladdin/src/apps/files.rs` desacoplando `statvfs` do loop síncrono de renderização.
  * Ajuste de captura de argumentos em `apps/ola-mundo/src/main.rs:_start` e sincronização do mapa de clientes em `apps/aladdin-server`.
* **Critérios de Conclusão:** Abertura e manipulação do Gerenciador de Arquivos e do Jinn Browser sem travamento ou perda de quadros a 60 FPS sob 64 MB de RAM em QEMU.
* **Riscos:** Regressões no alinhamento de pilhas de usuário no carregador ELF.

#### Atividade 1.2: Auditoria de Segurança e Proteção Anti-Meltdown (KPTI)
* **Período:** Meses 2 a 3 (Janeiro – Fevereiro de 2027)
* **Objetivo:** Separar as tabelas de páginas do kernel da metade de usuário via Kernel Page Table Isolation.
* **Entregáveis:**
  * Implementação de PML4 de usuário e trampolines de transição em `src/memory/paging.rs`.
  * Validação de ausência de vazamento especulativo de cache L1 em testes automatizados.
* **Critérios de Conclusão:** 100% dos vetores de leitura de memória do kernel a partir de Ring 3 bloqueados por hardware.

---

### 2.2. Médio Prazo (Meses 4 a 12: Março de 2027 – Novembro de 2027)
**Foco:** Arquitetura de Serviços Desacoplada, Computação Heterogênea (JHCS) e Otimização SMP

#### Atividade 2.1: Desacoplamento Total do Aladdin Desktop e SDK de Aplicações
* **Período:** Meses 4 a 6 (Março – Maio de 2027)
* **Objetivo:** Remover a dependência de fallback do compositor em Ring 0 (`src/gui.rs`), consolidando o Aladdin como servidor gráfico puro de espaço de usuário com ABI estável.
* **Entregáveis:**
  * Publicação do `aladdin-sdk` na versão 0.2.0 com documentação completa de chamadas.
  * Supervisão de processos clientes com terminação graciosa e limpeza automática de superfícies.
* **Critérios de Conclusão:** Microkernel compilado sem dependência de módulos visuais ou estruturas de interface gráfica.

#### Atividade 2.2: Escalonador com Per-CPU Runqueues e Work Stealing
* **Período:** Meses 6 a 8 (Maio – Julho de 2027)
* **Objetivo:** Quebrar o spinlock global `SCHED_LOCK`, viabilizando escalabilidade linear para topologias de até 16 processadores lógicos.
* **Entregáveis:**
  * Filas de tarefas independentes por CPU em `src/scheduler.rs`.
  * Algoritmo de Work Stealing atômico sem contenção em alta concorrência.
* **Critérios de Conclusão:** Latência média de troca de contexto inferior a 500 ciclos de clock sob 8 CPUs.

#### Atividade 2.3: Consolidação do JHCS Core e Suporte a AVX/AVX2 no Microkernel
* **Período:** Meses 9 a 12 (Agosto – Novembro de 2027)
* **Objetivo:** Habilitar suporte a `XSAVE`/`XRSTOR` e vetores de 256 bits no scheduler do Jinn OS, e integrar o `jhcs-platform-jinn` em Ring 3 para aceleração vetorial cooperativa.
* **Entregáveis:**
  * Kernel x86_64 com suporte condicional a AVX/AVX2 via CPUID.
  * Integração da DAG do JHCS com os anéis `io_uring` da JDA no Jinn OS.
* **Critérios de Conclusão:** Multiplicação matricial (MatMul) e soma vetorial executadas no Jinn OS com speedup comprovado sobre execução puramente escalar.

---

### 2.3. Longo Prazo (Meses 13 a 24: Dezembro de 2027 – Novembro de 2028)
**Foco:** Expansão de Hardware, IOMMU, Backend GPU e Produtos Especializados

#### Atividade 3.1: Suporte a IOMMU de Hardware (Intel VT-d e AMD-Vi)
* **Período:** Meses 13 a 16 (Dezembro de 2027 – Março de 2028)
* **Objetivo:** Prover isolamento de DMA mediado diretamente pelo hardware de chipset da placa-mãe.
* **Entregáveis:**
  * Driver de controle da tabela DMAR (DMA Remapping) em `src/dma/`.
  * Proteção nativa contra dispositivos PCI arbitrários em máquinas físicas reais.
* **Critérios de Conclusão:** Ataques simulados de injeção direta de DMA bloqueados e registrados pelo controlador IOMMU.

#### Atividade 3.2: Backend GPU para o JHCS (WebGPU / VirtIO-GPU)
* **Período:** Meses 17 a 20 (Abril – Julho de 2028)
* **Objetivo:** Adicionar capacidade de processamento gráfico e computacional na GPU ao middleware JHCS.
* **Entregáveis:**
  * Crate `jhcs-backend-gpu` baseada em `wgpu` para execução no host.
  * Driver de aceleração VirtIO-GPU para espaço de usuário no Jinn OS.
* **Critérios de Conclusão:** Despacho adaptativo alternando entre SIMD na CPU e shaders na GPU com base no custo $\Delta T$.

#### Atividade 3.3: Lançamento do Perfil Headless / Servidor (Simbad)
* **Período:** Meses 21 a 24 (Agosto – Novembro de 2028)
* **Objetivo:** Empacotar e disponibilizar uma distribuição headless ultra-leve do Jinn OS para microsserviços e edge computing.
* **Entregáveis:**
  * Imagem de disco e ISO sem interface gráfica, com consumo inferior a 8 MB de RAM.
  * Pilha de rede TCP/IP com servidor HTTP/1.1 embutido de alto rendimento.
* **Critérios de Conclusão:** 10.000 requisições HTTP simultâneas atendidas com sucesso sob 64 MB de RAM física.

---

### 2.4. Consolidação e Validação Externa (Meses 25 a 36: Dezembro de 2028 – Novembro de 2029)
**Foco:** Testes em Hardware Físico, Certificações de Segurança, Parcerias e Licenciamento Comercial

#### Atividade 4.1: Validação Massiva em Hardware Físico de Referência
* **Período:** Meses 25 a 28 (Dezembro de 2028 – Março de 2029)
* **Objetivo:** Homologar o Jinn OS e o Aladdin em 10 plataformas de hardware físico de baixa potência (computadores com processadores Intel Core 2 Duo, Atom, Celeron e AMD Athlon 64 com 256 MB a 2 GB de RAM).
* **Entregáveis:**
  * Matriz formal de compatibilidade de placas-mãe, chipsets e periféricos de armazenamento físico.
  * Relatório de estresse ininterrupto de 720 horas de operação sem vazamento de memória ou travamento.
* **Critérios de Conclusão:** Instalação limpa via USB e operação contínua comprovada em hardware real.

#### Atividade 4.2: Auditoria Independente de Segurança Externa e Certificação
* **Período:** Meses 29 a 32 (Abril – Julho de 2029)
* **Objetivo:** Contratar auditoria de segurança defensiva (*Penetration Testing*) com firma especializada independente para atestar a solidez do microkernel e da pilha criptográfica TLS 1.3.
* **Entregáveis:**
  * Relatório formal de auditoria externa com CVEs mitigadas e selo de conformidade de código seguro.
* **Critérios de Conclusão:** Zero vulnerabilidades críticas ou altas em aberto.

#### Atividade 4.3: Lançamento Comercial e Programa de Licenciamento Duplo
* **Período:** Meses 33 a 36 (Agosto – Novembro de 2029)
* **Objetivo:** Iniciar a oferta formal de suporte comercial, contratos de manutenção e relicenciamento proprietário para fabricantes de hardware embarcado e sistemas industriais.
* **Entregáveis:**
  * Portal de licenciamento comercial com termos jurídicos auditados.
  * Kit de Desenvolvimento de Software (SDK) para parceiros industriais.
* **Critérios de Conclusão:** Primeiros contratos piloto de licenciamento comercial formalizados.

---

## 3. Marcos Canônicos de Maturidade do Ecossistema

Para evitar a declaração prematura de maturidade, o Jinn Project adota critérios objetivos e mensuráveis para cada estágio de lançamento:

| Marco Canônico | Definição e Critérios Obrigatórios | Meta Temporal |
| :--- | :--- | :--- |
| **Alpha Técnica** | Microkernel compila com zero erros em `x86_64-unknown-none`. Boot em QEMU, inicialização de GDT, IDT, APIC, paginação de memória e escalonador SMP com 4 núcleos funcionais. Preservação de FPU com `FXSAVE64`/`FXRSTOR64` comprovada. | **Concluído (Novembro/2026)** |
| **Alpha Funcional** | Ambiente gráfico Aladdin executa em espaço de usuário (Ring 3). Suporte a mouse PS/2 estável, leitura/escrita em FAT32, terminal gráfico funcional e suíte de testes de estresse aprovada. | **Mês 3 (Fevereiro/2027)** |
| **Beta** | Resolução total dos incidentes de congelamento do File Manager e da janela do Jinn Browser. Pilha de rede TCP/IP com TLS 1.3 interoperável contra servidores externos. JHCS executando kernels SIMD no host e no Jinn OS. | **Mês 12 (Novembro/2027)** |
| **Release Candidate (RC)** | Kernel Page Table Isolation (KPTI) implementado. Suporte a AVX/AVX2 via `XSAVE`. Driver AHCI SATA e NVMe consolidados para armazenamento. Ausência de panics em 100 horas de estresse contínuo em QEMU. | **Mês 24 (Novembro/2028)** |
| **Release Estável (1.0)** | Validação e boot comprovados em hardware físico com 256 MB de RAM. Auditoria independente de segurança externa concluída sem vulnerabilidades críticas. Instalador para pendrive USB funcional. | **Mês 36 (Novembro/2029)** |
| **Produto Comercial** | Documentação jurídica de licenciamento duplo finalizada, CLA ativo, registro de marca concluído no INPI e suporte comercial garantido para parceiros industriais. | **Mês 36 (Novembro/2029)** |
