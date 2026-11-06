# JINN HARDWARE PROFILES SPECIFICATION 1.0
## Especificação dos Perfis Oficiais de Hardware A..E

**Projeto:** Jinn OS  
**Subcomponente:** Hardware Profiler & Policy Engine  
**Versão:** 1.0  
**Data:** 06/11/2026  

---

## 1. Definição Canônica dos Perfis

Os perfis de hardware são determinados durante o boot pelo `HardwareProfiler` e aplicados transparentemente pelo microkernel.

### Perfil A — Jinn Legacy (256 MB)
- **RAM Alvo:** 256 MB (faixa < 384 MB).
- **Processador de Referência:** AMD Athlon 64 / Intel Pentium 4 EM64T (1 vCPU).
- **Armazenamento:** HD IDE mecânico de 20 a 80 GB.
- **Gráficos:** VESA Framebuffer 800x600 ou 1024x768 (16/32 bpp).
- **Diretriz de Execução:** Desktop essencial funcional, zero transparências alfa, abas em background hibernam imediatamente, cache de disco de 8 KiB (16 slots), limite de tarefas no escalonador: 16.

### Perfil B — Jinn Compact (512 MB)
- **RAM Alvo:** 512 MB (faixa 384 MB a 768 MB).
- **Processador de Referência:** Athlon 64 / Core 2 Duo inicial (1 vCPU).
- **Armazenamento:** HD IDE ou SATA inicial (40 a 120 GB).
- **Gráficos:** Resolução até 1024x768x32 bpp.
- **Diretriz de Execução:** Desktop completo com terminal, navegador com até 3 abas ativas, compositor com dirty rectangles, cache de disco de 16 KiB (32 slots), limite de tarefas: 32.

### Perfil C — Jinn Standard (1 GB a 4 GB)
- **RAM Alvo:** 1 GB a 4 GB (faixa 768 MB a 3.5 GB).
- **Processador de Referência:** Core 2 Duo / Quad / AMD Athlon X2 (2 vCPUs).
- **Armazenamento:** SATA AHCI / SSD SATA inicial.
- **Gráficos:** Resolução 1280x720 / 1920x1080.
- **Diretriz de Execução:** Multitarefa fluida para uso cotidiano, até 8 abas ativas no navegador, efeitos visuais ativados, cache de disco de 32 KiB (64 slots), limite de tarefas: 64.

### Perfil D — Jinn Performance (4 GB a 16 GB)
- **RAM Alvo:** 4 GB a 16 GB (faixa 3.5 GB a 14 GB).
- **Processador de Referência:** Intel Core i5/i7 / AMD FX / Ryzen inicial (4 vCPUs).
- **Armazenamento:** SSD SATA / NVMe PCIe.
- **Diretriz de Execução:** Alto paralelismo, multitarefa ampliada, ring buffers JDA ampliados, cache de disco de 64 KiB (128 slots), limite de tarefas: 128.

### Perfil E — Jinn High-End (16 GB+)
- **RAM Alvo:** 16 GB ou mais.
- **Processador de Referência:** Processadores modernos multicore (8+ vCPUs).
- **Armazenamento:** NVMe PCIe Gen3/4/5.
- **Diretriz de Execução:** Alta densidade de processos, bancos de dados em memória, virtualização e servidores Simbad sem desperdício de memória, limite de tarefas: 256.

---

## 2. Matriz de Maturidade e Estado de Implementação (JAHA Seção 16)

| Perfil de Hardware | Faixa de RAM | Estado | Teste Unitário | Integração | Validação QEMU | Validação Hardware Físico |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **Perfil A — Jinn Legacy** | 256 MB (< 320 MB) | **Implementado** | [V] Aprovado | [V] Aprovado | [V] 256M Aprovado | [V] Validado |
| **Perfil B — Jinn Compact**| 512 MB (320–768 MB)| **Implementado** | [V] Aprovado | [V] Aprovado | [V] 512M Aprovado | [V] Validado |
| **Perfil C — Jinn Standard**| 1–4 GB (768M–3.5G) | **Implementado** | [V] Aprovado | [V] Aprovado | [V] 1024M Aprovado| [V] Validado |
| **Perfil D — Jinn Performance**| 4–16 GB (3.5–14G)| **Implementado** | [V] Aprovado | [V] Aprovado | [V] 4096M Aprovado| [V] Validado |
| **Perfil E — Jinn High-End**| 16 GB+ (>= 14 GB) | **Implementado** | [V] Aprovado | [V] Aprovado | [V] Validado | [V] Validado |
| **Perfil Experimental 128M**| 128 MB | **Experimental** | [V] Aprovado | [V] Aprovado | [V] 128M Aprovado | Pendente validação |
| **Perfil Experimental 64M** | 64 MB | **Experimental** | [V] Aprovado | [V] Aprovado | [V] 64M Aprovado | Pendente validação |

