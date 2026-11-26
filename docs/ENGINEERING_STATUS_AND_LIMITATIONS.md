# Fronteira Honesta: Estado Verificado de Engenharia & Limitações Conhecidas (`ENGINEERING_STATUS_AND_LIMITATIONS`)

> **Escopo:** Registro público auditável da fronteira exata entre **capacidades comprovadas com testes reproduzíveis** e **capacidades experimentais / não homologadas** no ecossistema Jinn (`v0.0.3-alpha`).  
> **Política Inegociável:** É terminantemente vedado reivindicar funcionalidades da **Tabela B** como prontas para produção ou equivalentes a hardware físico homologado.

---

## Tabela A — Comprovado com Evidência Reproduzível (Ambiente x86_64 QEMU TCG / Disk / PCAP)

| Subsistema / Capacidade | Comportamento Verificado | Arquivos & Mecanismos Reais | Script / Evidência Reproduzível |
| :--- | :--- | :--- | :--- |
| **Isolamento Ring 3 + Paginação PML4 `W^X`** | Kernel mínimo em Ring 0; aplicações gráficas (`aladdin-server`), browser/shell (`ola-mundo`) e `jhcs-daemon` executam em espaço de usuário `>= 0x400000` com enforçamento `W^X` e rejeição `EFAULT` para ponteiros inválidos/kernel nas syscalls. | `src/memory/paging.rs`, `src/gdt.rs`, `src/syscall.rs`, `src/elf.rs` | `scripts/run.ps1 -Smp 4`; boot serial log `[MAIN] aladdin[U3]` & `[+] Validacao de Memoria e Syscalls EFAULT... OK`. |
| **Contabilidade SMP Per-CPU (`Syscall 88`)** | Estatísticas reais por CPU (`total_ticks`, `idle_ticks`, uso por processo e contagem de CPUs ativas) exportadas do kernel para userspace Aladdin Desktop via `sys_get_system_stats`. | `src/syscall.rs` (Syscall `88`), `src/smp.rs`, `src/scheduler.rs` | Monitor JAHA & Rede/DNS no Aladdin Desktop (`Aladdin/src/apps/`), testes SMP `-smp 4`. |
| **Comportamento *Zero-Idle* (`HLT`) sob QEMU TCG** | Núcleos ociosos (APs 1–3) entram em `HLT` orientado a eventos sem *busy-loops* ou timers periódicos espúrios de limpeza em background; validado em repouso em QEMU `-smp 4`. | `src/scheduler.rs`, `src/smp.rs`, `src/interrupts/pic.rs` (EOI incondicional PIC 8259A e `IrqSafeSpinLock`) | `scripts/test_dns_cache_e2e.py`, `scripts/test_hot_routes_e2e.py` (verificação de `0.00%` CPU ociosa sob QEMU). |
| **Integridade Física de Memória JAHA 1.2** | Contabilidade determinística física *no-overcommit* operando sob orçamento estrito (`64 MiB`–`512 MiB`) com invariante de **`0 bytes discrepancy`** no encerramento/ciclos de alocação. | `src/memory/`, `docs/JAHA_12_SPECIFICATION.md` | `scripts/test_jaha_12_integrity.py`, `scripts/reproduce_64m.py`. |
| **Pilha de Rede (`e1000`/`VirtIO-Net`), DNS 3 Camadas & `HotRouteTable`** | Driver `e1000`/VirtIO-Net funcional via SLIRP/QEMU; resolução DNS em 3 estágios (Hosts estáticos $\to$ Cache TTL *lazy-on-lookup* RAM/FAT32 $\to$ `1.1.1.1`/`8.8.8.8`/`10.0.2.3`); fast-path unificado L2/L3 `HotRouteTable` alinhado a 64B em `.bss`. | `src/net/dns.rs`, `src/net/hot_routes.rs`, `src/syscall.rs` (`40`, `86..87`, `93..99`), `src/drivers/net/` | `scripts/test_dns_cache_e2e.py`, `scripts/test_dns_cross_pcap.py`, `scripts/test_hot_routes_e2e.py` (+ captura `.pcap`). |
| **TLS 1.3 (RFC 8446) Multi-Record em `#![no_std]`** | Handshake TLS 1.3 (`ChaCha20-Poly1305-SHA256` & `AES-128-GCM-SHA256`), X25519 ECDHE, reassembly de múltiplos registros TCP e carregamento real de sites HTTPS (`jinn-os.dev.br`, `example.com`). | `apps/ola-mundo/src/tls.rs`, `src/net/tls13.rs` | `scripts/test_tls13_homologation.py`, `scripts/test_browser_navigation_e2e.py`. |
| **Persistência FAT32 LFN & Auto-Recuperação `.TMP` $\to$ `.BAK`** | Montagem FAT32 (`/disk`) + Ext2 R/O; nomes longos (LFN); persistência transacional `.TMP` $\to$ `.BAK` para cache DNS, rotas quentes e cache/histórico do browser. | `src/vfs/`, `apps/ola-mundo/src/browser.rs` (`storage`) | `scripts/test_persistence.ps1`, `scripts/test_filemanager_real.py`, suíte `BrowserStorageTests`. |
| **Compositor Ring 3 `Aladdin` & `Jinn Browser`** | Anéis `AladdinSharedRings` (Syscalls `89..90`), *damage tracking* (`DirtyRect`), frosted glass SIMD CPU, redirecionamentos HTTP `301/302/303/307/308` e imagens PNG/BMP. | `Aladdin/src/`, `sdk/aladdin-sdk/`, `apps/ola-mundo/src/` | Validado por capturas `screendump` e inspeção visual em QEMU `-device e1000`. |

---

## Tabela B — Experimental / Ainda Não Homologado (Proibido Afirmar Como Pronto)

| Capacidade / Afirmação | Estado Atual Real no Código | Por Que Não Está Homologado (Limitação Explícita) | O Que Falta para Homologar |
| :--- | :--- | :--- | :--- |
| **Timer LAPIC *One-Shot* Completamente Tickless & Economia Elétrica em Hardware Físico Real (Watts/Joules)** | `src/interrupts/apic.rs` configura registradores LAPIC básicos, mas o escalonador principal ainda depende do **PIT timer periódico (100 Hz)** e as medições *Zero-Idle* foram conduzidas sob **QEMU TCG**. | **Emulação QEMU TCG não mede C-states profundos (`C1E`/`C6`/`C10`), DVFS nem consumo elétrico físico.** Contar ciclos `HLT` em VM prova ausência de *busy-waiting*, não redução medida em Watts/Joules na placa-mãe. | Implementar disparo dinâmico *One-Shot* puramente *tickless* no LAPIC + executar bancada em silício bare-metal com RAPL / wattímetro físico. |
| **Benchmarks Comparativos Sistemáticos vs. Linux Bare-Metal** | Existem metas documentadas em `docs/benchmarks/JINN_PERFORMANCE_BASELINE.md`, porém sem suíte executada lado a lado em hardware físico contra kernels Linux atuais nas mesmas cargas I/O/IPC/compute. | Comparar execução QEMU TCG vs. distribuições desktop convencionais sem hardware idêntico, kernel config declarado e workloads idênticos é metodologicamente inválido. | Executar suíte padronizada em hardware físico idêntico (x86_64) publicando configurações, versões e dispersão estatística (P50/P95/P99). |
| **Portabilidade Executável para ARM64 (`aarch64`) / RISC-V (`riscv64`)** | Somente o núcleo abstrato `jhcs-core` / `jhcs-backend-*` é portável em nível de linguagem; todo boot, GDT/IDT/PML4, drivers e ABI de syscall do microkernel são exclusivamente **`x86_64`**. | Especificações em `docs/strategic-issues/01_NEON_ARM64_SIMD.md` representam issues/desenhos abertos, ainda sem código bootável ou kernels NEON compilados em placa ARM64 real. | Implementar hal `arch/aarch64`, bootloader UEFI ARM64, adaptadores GICv2/v3 e validação real em SBC (ex.: Raspberry Pi / Rockchip). |
| **Isolamento KPTI, IOMMU (`VT-d`/`AMD-Vi`) e Aceleração GPU em Bare-Metal (`Jinn OS`)** | Drivers DMA/rede (`e1000`/`VirtIO`) e proteção de memória usam paginação PML4 Ring 0/3; backend `wgpu` do `jhcs-rs` funciona apenas sob **host (`std`)** Windows/Linux. | Não há remapeamento de tabelas DMA via `VT-d`/`AMD-Vi` nem driver Vulkan/DRM nativo em Ring 3 para GPUs discretas dentro do Jinn OS. | Implementar driver IOMMU no Ring 0/supervisor e ponte VirtIO-GPU/KDM real em bare-metal (`05_VIRTIO_GPU_KDM_BACKEND.md`). |
| **Execução Web Completa JavaScript ES2024+ / WebGL / DOM Reativo no Jinn Browser** | `MiniJsRuntime` (`apps/ola-mundo/src/browser.rs`) suporta apenas um subconjunto leve de scripts inline/estáticos e detecta shells SPA vazios com aviso diagnóstico. | Sem JIT/engine V8/SpiderMonkey ou WebAssembly completo, aplicações React/Next.js/Vue complexas não executam client-side no Jinn Browser. | Expandir gradualmente APIs DOM/Fetch ou manter foco explícito em HTML/CSS estático, feeds RSS/Atom e JSON REST. |

---

## Regra de Comunicação Externa

Qualquer postagem pública, documento de grant ou apresentação (*Brief*) deve citar explicitamente:
1. O escopo exato da validação (**QEMU TCG/KVM, `-device e1000`, `-smp 4`, `-m 64M`/`512M`**);
2. Que *"0.00% CPU ociosa"* em emulação atesta a arquitetura orientada a eventos (*no busy-loop / HLT gating*), enquanto eficiência elétrica em Watts depende das medições futuras da **Tabela B**.
