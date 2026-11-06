# JINN ADAPTIVE HARDWARE ARCHITECTURE (JAHA) 1.0
## Arquitetura Adaptativa de Hardware, Governança Dinâmica e Eficiência Extrema de Recursos

**Projeto:** Jinn OS  
**Versão da Diretriz:** 1.0  
**Data:** 06/11/2026  
**Status:** Em Implementação Ativa  
**Produtos Alvo:** Aladdin Desktop, Simbad Server e Ifrit Industrial  

---

## 1. Princípio Fundamental de Engenharia

> **"O Jinn deverá utilizar apenas os recursos necessários para executar suas funções, adaptando dinamicamente suas políticas ao hardware disponível."**

Diferente de sistemas convencionais que aumentam o consumo de memória proporcionalmente à RAM total instalada (ou que quebram em hardware restrito), o Jinn opera através de **Perfis Oficiais de Hardware** e um **Adaptive Resource Manager**. O mesmo kernel se calibra dinamicamente sem forks ou perda de recursos essenciais.

```
+───────────────────────────────────────────────────────────────────────────────────+
|                           JINN MICROKERNEL (x86_64)                               |
+───────────────────────────────────────────────────────────────────────────────────+
                                          │
                                          ▼
+───────────────────────────────────────────────────────────────────────────────────+
|               HARDWARE PROFILER (kernel/services/hardware_profiler)               |
|  • Validação rigorosa de Memmap Limine (Usable, Reserved, Reclaimable, ACPI)       |
|  • CPUID & Cores (Long Mode, SSE2, APIC, TSC, Frequency)                           |
|  • Barramento PCI (IDE, SATA, NVMe, NIC, Display, Áudio, USB)                      |
|  • Classificação em Perfil A..E                                                   |
+───────────────────────────────────────────────────────────────────────────────────+
                                          │
                                          ▼
+───────────────────────────────────────────────────────────────────────────────────+
|                     ADAPTIVE RESOURCE MANAGER (ARM)                               |
|  • Políticas de Quotas por Processo (Frames, FDs, Janelas, Buffers)                |
|  • Níveis de Pressão de Memória: Normal -> Moderate -> Critical                    |
|  • Backpressure: Redução de Cache LRU, Hibernação de Abas, Descarte de Buffers    |
+───────────────────────────────────────────────────────────────────────────────────+
                                          │
              ┌───────────────────────────┼───────────────────────────┐
              ▼                           ▼                           ▼
+───────────────────────────+ +───────────────────────────+ +───────────────────────────+
|      ALADDIN DESKTOP      | |       SIMBAD SERVER       | |      IFRIT INDUSTRIAL     |
| (Compositor Adaptativo,   | | (High-Throughput IO,      | | (Tempo Real Estrito,      |
|  Dirty Rects, Low-RAM UI) | |  Headless, JinnWall)      | |  Zero Alocação Dinâmica)  |
+───────────────────────────+ +───────────────────────────+ +───────────────────────────+
```

---

## 2. Os Cinco Perfis Oficiais de Hardware

| Perfil | Faixa de RAM | Alvo de CPU | Objetivo Operacional | Política de Cache e Memória |
| :--- | :---: | :---: | :--- | :--- |
| **Perfil A (Legacy)** | 256 MB | 1 vCPU | Inicialização essencial, desktop funcional, navegação estática, apps básicas | Cache de 8 a 16 blocos, zero efeitos alfa, suspensão agressiva de abas, sem cópias duplicadas |
| **Perfil B (Compact)** | 512 MB | 1 vCPU | Desktop completo, terminal, navegador com abas leves, gerenciador de arquivos | Cache de 32 blocos, composição de dirty rectangles, multitarefa moderada |
| **Perfil C (Standard)** | 1 GB a 4 GB | 2 vCPUs | Uso cotidiano, multitarefa fluida, navegação web rica, serviços de rede | Cache de 64 blocos, janelas dinâmicas, renderização de fontes suavizadas |
| **Perfil D (Performance)** | 4 GB a 16 GB | 4 vCPUs | Alto paralelismo, caches ampliados, background workers, JDA ring buffers | Cache ampliado, dirty pages assíncronas, paralelismo multicore agressivo |
| **Perfil E (High-End)** | 16 GB ou mais | 8+ vCPUs | Servidores corporativos, bancos de dados em memória, virtualização, processamento maciço | Cache adaptativo sob demanda, alocação sob demanda sem consumo supérfluo |

---

## 3. Arquitetura de Degradação sob Pressão de Recursos (Backpressure)

Quando o subsistema de memória física (`BuddyAllocator`) detectar esgotamento ou cruzar limites de segurança, o gerenciador dispara 7 níveis de contenção:
1. **Redução de Caches Recuperáveis:** Esvaziamento de páginas limpas do cache LRU de blocos de disco (`src/drivers/block.rs`).
2. **Suspensão de Serviços Não-Essenciais:** Notificação via IPC para daemons em background reduzirem suas threads de trabalho.
3. **Hibernação de Abas Elegíveis:** O navegador Jinn Browser descarta superfícies de renderização inativas e retém apenas o snapshot (`TabSnapshot`).
4. **Liberação de Buffers Temporários:** Descarte de buffers de rede pendentes não confirmados e pools de sockets ociosos.
5. **Descarte de Recursos Regeneráveis:** Liberação de buffers de pré-renderização de ícones e fontes já cacheados.
6. **Preservação de Serviços Críticos:** Proteção inviolável do PID 0 (Idle), Shell do kernel, Compositor Aladdin e Supervisor.
7. **Notificação de Backpressure:** Retorno de erro `-ENOMEM` ou flag de pressão para impedir que aplicações continuem chamando `sys_alloc_page` em loop.

---

## 4. Matriz de Maturidade e Estado de Implementação (JAHA Seção 16)

| Componente Arquitetural | Estado | Teste Unitário | Integração | Validação QEMU | Validação Hardware Físico |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Hardware Profiler** | **Implementado** | [V] Aprovado | [V] Aprovado | [V] 64M a 4GB | [V] VirtualBox / Limine BIOS |
| **Adaptive Resource Manager** | **Implementado** | [V] Aprovado | [V] Aprovado | [V] Validado | [V] Validado |
| **Backpressure em 7 Níveis** | **Implementado** | [V] Aprovado | [V] Aprovado | [V] Validado | [V] Validado |
| **Driver Supervisor & Faults** | **Implementado** | [V] Aprovado | [V] Aprovado | [V] 5/5 Falhas | [V] Validado |
| **Paginamento Dinâmico Ring 3**| **Implementado** | [V] Aprovado | [V] Aprovado | [V] Validado | [V] Validado |
| **IOMMU Hardware Isolation** | **Planejado** | — | — | — | Pendente hardware VT-d |
| **Perfis Experimentais (64M/128M)** | **Experimental**| [V] Aprovado | [V] Aprovado | [V] Validado | Pendente validação física |

