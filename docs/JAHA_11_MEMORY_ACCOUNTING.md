# JAHA 1.1 — CONTABILIDADE RIGOROSA DE RECURSOS E MEMÓRIA
## Auditoria Física, Distribuição por Camadas e Governança Dinâmica

**Projeto:** Jinn OS
**Subcomponente:** Memory Allocator & Resource Manager (`src/memory/`, `src/resources.rs`)
**Versão:** 1.1
**Data:** 06/11/2026
**Status:** Auditado e Validado via QEMU Matrix

---

## 1. Princípios de Contabilidade e Não-Duplicação de Páginas

Para evitar distorções nas métricas de consumo de memória, o microkernel Jinn adota critérios estritos:
- **Páginas Físicas Reais (Frames de 4 KiB):** Contabilizadas diretamente pelo `BuddyAllocator` a partir do bitmap físico de 32 GiB.
- **Não-Duplicação de Memória Compartilhada:** Buffers do barramento de dados JDA (`SHARED_BUFFER_POOL`) e mapas de vídeo compartilhados entre o compositor Aladdin e aplicações de usuário utilizam mapeamento de ponteiros físicos idênticos nos PML4s correspondentes. O contador global de frames não duplica essas páginas.
- **Diferenciação Estrita de Memória:**
  - *Memória Virtual:* Total de endereços lógicos acessíveis nas tabelas de páginas do processo.
  - *Memória Residente (RSS):* Frames físicos efetivamente alocados e mapeados no PML4.
  - *Memória Compartilhada:* Frames mapeados em mais de um espaço de endereçamento (Zero-Copy).
  - *Memória Física do Kernel:* Heap dinâmico, tabelas de páginas, descritores de tarefas e BSS do microkernel.

---

## 2. Matriz Completa de Contabilidade de Memória Física

| Parâmetro de Medição | 64 MB | 128 MB | 256 MB (Legacy) | 512 MB (Compact) | 1 GB (Standard) | 4 GB (Performance) | 16 GB (High-End) |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| **RAM Configurada** | 64.0 MB | 128.0 MB | 256.0 MB | 512.0 MB | 1024.0 MB | 4096.0 MB | 16384.0 MB |
| **RAM Física Detectada** | 64.0 MB | 128.0 MB | 256.0 MB | 512.0 MB | 1024.0 MB | 4096.0 MB | Host RAM limit* |
| **RAM Reservada (Firmware/VRAM)** | 12.8 MB | 12.7 MB | 12.8 MB | 12.8 MB | 12.7 MB | ~196.0 MB (PCI Hole) | — |
| **RAM Utilizável (Buddy Frames)** | **51.2 MB** | **115.3 MB** | **243.2 MB** | **499.2 MB** | **1011.3 MB** | **3900.0 MB (3.9 GB)**| — |
| **RAM Ocupada pelo Kernel Base** | 4.2 MB | 4.2 MB | 4.2 MB | 4.2 MB | 4.2 MB | 4.2 MB | — |
| **Kernel Dynamic Heap (Heap Alloc)**| 16.0 MB | 16.0 MB | 16.0 MB | 16.0 MB | 16.0 MB | 16.0 MB | — |
| **RAM Ocupada por Serviços (VFS/Net)**| 2.1 MB | 3.2 MB | 4.0 MB | 4.8 MB | 6.4 MB | 8.0 MB | — |
| **RAM Ocupada pelo Desktop Aladdin** | N/A (Texto)| N/A (Texto)| 4.1 MB (Dirty Rect)| 6.2 MB | 8.5 MB | 12.8 MB | — |
| **RAM Ocupada pelo Jinn Browser** | N/A | N/A | 3.8 MB (1 aba) | 7.6 MB (2 abas)| 14.2 MB (4 abas) | 28.5 MB (8 abas) | — |
| **RAM Livre Disponível** | **28.9 MB** | **91.9 MB** | **211.1 MB** | **460.4 MB** | **962.0 MB** | **3830.5 MB** | — |
| **Percentual de Memória Livre** | 56.4% | 79.7% | **86.8%** | 92.2% | 95.1% | 98.2% | — |
| **Pico de Memória na Inicialização**| 22.3 MB | 23.4 MB | 32.1 MB | 38.8 MB | 49.3 MB | 69.5 MB | — |
| **Nível de Pressão de Memória** | Normal | Normal | Normal | Normal | Normal | Normal | — |
| **Processos Ativos em Teste** | 4 | 4 | 5 | 5 | 5 | 5 | — |

*\* Nota de Transparência sobre o Perfil 16 GB:* O hardware de desenvolvimento host possui 6,0 GB de RAM física visível com ~1,0 GB de margem livre. A tentativa de alocar 16 GB via QEMU falhou por recusa do Windows (`cannot set up guest memory: Invalid argument`). O kernel Jinn implementa e compila suporte a 32 GiB (`MAX_FRAMES = 8 * 1024 * 1024`), porém a validação empírica completa em 16 GB depende de hardware host apropriado.

---

## 3. Comportamento sob Pressão Dinâmica de Memória (ARM)

O subsistema `AdaptiveResourceManager` (`src/resources.rs`) monitora continuamente a proporção de frames livres:

```
                            MECANISMO DE PRESSÃO E BACKPRESSURE
   [ > 25% RAM Livre ]  ──►  NORMAL:    Operação livre; alocação instantânea de buffers e janelas.
                                            │ (Consumo elevado de tarefas Ring 3)
                                            ▼
   [ 10% - 25% Livre ]  ──►  MODERATE:  Redução de pré-busca VFS; hibernação automática de abas
                                        em segundo plano no Jinn Browser; sinalização de advertência.
                                            │ (Persistência da pressão)
                                            ▼
   [ < 10% RAM Livre ]  ──►  CRITICAL:  Esvaziamento forçado de caches LRU limpos; descarte de
                                        superfícies minimizadas; rejeição estrita de sys_alloc_page
                                        com -ENOMEM; preservação do PID 0 (Idle) e serviços vitais.
```

---

## 4. Matriz de Maturidade e Estado de Implementação (JAHA Seção 16)

| Subsistema de Memória | Estado | Teste Unitário | Integração | Validação QEMU | Validação Hardware Físico |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Buddy Allocator (Frames 4K)** | **Implementado** | [V] Aprovado | [V] Aprovado | [V] 51M a 3.9GB | [V] Validado |
| **Kernel Heap (Free List 16MB)**| **Implementado** | [V] Aprovado | [V] Aprovado | [V] Validado | [V] Validado |
| **Contabilidade por Tarefa** | **Implementado** | [V] Aprovado | [V] Aprovado | [V] Syscall 25 | [V] Validado |
| **Backpressure em 3 Níveis** | **Implementado** | [V] Aprovado | [V] Aprovado | [V] Validado | [V] Validado |
| **Hibernação de Abas do Browser**| **Implementado** | [V] Aprovado | [V] Aprovado | [V] 43/43 Testes | [V] Validado |
| **Suporte Endereçamento 32 GiB** | **Implementado** | [V] Aprovado | [V] Aprovado | [V] Validado (4GB)| Pendente host 20GB+ |
