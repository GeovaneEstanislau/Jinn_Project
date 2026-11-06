# JINN MEMORY BUDGET SPECIFICATION 1.0
## Distribuição Rigorosa de Memória Física para 256 MB, 512 MB, 1 GB e 4 GB

**Projeto:** Jinn OS  
**Subcomponente:** Memory Management Subsystem (`src/memory/`)  
**Versão:** 1.0  
**Data:** 06/11/2026  

---

## 1. Princípio de Orçamento Físico (No-Overcommit)

No Jinn OS, a memória física não é prometida além do que o hardware é capaz de entregar. A tabela abaixo documenta o orçamento estrito de consumo para os principais níveis de memória, provando a viabilidade operacional em hardware com 256 MB de RAM:

```
                           ORÇAMENTO DE MEMÓRIA (256 MB TOTAL)
+-----------------------------------------------------------------------------------+
|  [Reservado Firmware/VRAM]  16.0 MB  (Stolen Memory / Framebuffer 800x600x32)     |
|  [Kernel Base + BSS + Pag]   4.0 MB  (PML4/PDPT/PD/PT, IDT, GDT, Scheduler)      |
|  [Kernel Dynamic Heap]      16.0 MB  (Alocador Dinâmico Central)                  |
|  [Drivers + Supervisor]      2.0 MB  (Descritores de Bloco, NIC, Dispositivos)   |
|  [Compositor Aladdin]        4.0 MB  (Superfície de composição / Dirty Rects)     |
|  [Serviços Sistema]          4.0 MB  (VFS, IPC, JDA, JinnWall)                    |
|  [Jinn Browser (1 aba)]      8.0 MB  (DOM Arena, Mini-CSS, Mini-JS, Surface)      |
|  [Aplicações Usuário]       16.0 MB  (Terminal, Monitor, Editor)                  |
|  [Memória Livre Disponível] 186.0 MB  (Margem de segurança > 75% da RAM total!)   |
+-----------------------------------------------------------------------------------+
```

---

## 2. Matriz Comparativa de Orçamentos por Camada de Sistema

| Subsistema / Camada | Orçamento 256 MB | Orçamento 512 MB | Orçamento 1 GB | Orçamento 4 GB |
| :--- | :---: | :---: | :---: | :---: |
| **Reservado / VRAM Firmware** | 16 MB | 32 MB | 64 MB | 128 MB |
| **Kernel Base & Paginação** | 4 MB | 6 MB | 8 MB | 16 MB |
| **Kernel Heap Central** | 16 MB | 32 MB | 64 MB | 128 MB |
| **Drivers & Supervisor** | 2 MB | 4 MB | 8 MB | 16 MB |
| **Cache LRU de Armazenamento** | 8 KiB (16 slots) | 16 KiB (32 slots) | 32 KiB (64 slots) | 64 KiB (128 slots) |
| **Compositor Gráfico (Aladdin)** | 4 MB | 8 MB | 16 MB | 32 MB |
| **Serviços (VFS, IPC, JDA)** | 4 MB | 8 MB | 16 MB | 32 MB |
| **Jinn Browser (Engine & Tabs)** | 8 MB (1 aba ativa) | 16 MB (3 abas) | 48 MB (8 abas) | 128 MB (16 abas) |
| **Aplicações de Usuário Ring 3** | 16 MB | 32 MB | 96 MB | 256 MB |
| **Memória Livre Garantida** | **~186 MB (72%)** | **~374 MB (73%)** | **~704 MB (68%)** | **~3.2 GB (80%)** |

---

## 3. Avaliação Rigorosa do Perfil 256 MB (Legacy Milestone)

A meta de 256 MB foi estabelecida como um critério de viabilidade estrito:
1. O microkernel consome em repouso menos de 24 MB (incluindo o Heap de 16 MB).
2. O compositor Aladdin aloca apenas a superfície necessária para a resolução ativa (em 800x600x32 bpp, uma superfície consome meros 1.87 MB).
3. A Arena DOM Flat Vector do Jinn Browser pré-aloca um array fixo de 64 nós sem fragmentação da heap de usuário.
4. Sobram mais de 180 MB de memória livre para a execução simultânea de múltiplos processos Ring 3!

