# 🔬 Jinn Project — Auditoria Técnica Detalhada por Subsistema
## Diagnóstico de Código, Análise de Incidentes, Evidências e Limitações

> **Documento Oficial de Engenharia do Jinn Project — Documento 3 de 11**  
> **Data de Referência:** 16 de Novembro de 2026  
> **Status:** Concluído e Auditado | **Classificação:** Auditoria Técnica em Profundidade  
> **Escopo:** Microkernel, Drivers, Rede, Criptografia, Aladdin Desktop e Incidentes Históricos  

---

## 1. Auditoria do Microkernel e Escalonamento (SMP & FPU)

### 1.1. Context Switching e Preservação de FPU/SIMD

* **Arquivo Auditado:** `src/fpu.rs`, `src/scheduler.rs`, `src/cpu.rs`
* **Estrutura Central:** `#[repr(C, align(16))] pub struct FpuState` (512 bytes).
* **Asserção Estática:** `const _: () = assert!(core::mem::size_of::<FpuState>() == 512 && core::mem::align_of::<FpuState>() == 16);`
* **Diagnóstico da Implementação:**
  * O microkernel opera com `-C target-feature=+soft-float,-mmx,-sse`, impedindo que o compilador LLVM gere instruções vetoriais ou toque nos registradores `XMM0..XMM15` dentro do Ring 0.
  * A preservação de contexto adota a abordagem **Eager FPU Switching**, salvando o estado da FPU/SSE no momento exato da comutação de tarefas (`preempt_core` e `direct_switch`) para qualquer processo com privilégio `Ring::User`.
  * **Mitigação do LazyFP (CVE-2018-3665):** Ao eliminar o uso de comutação preguiçosa (*Lazy Switching*) baseada na exceção `#NM` e bit `CR0.TS`, anula-se completamente o vazamento de chaves criptográficas residentes em registradores `XMM` entre tarefas distintas.
  * **Sanitização de MXCSR:** Na rotina `src/fpu.rs:fxrstor64`, os bits 16..31 do registrador `MXCSR` são mascarados com `0x0000_FFFF` antes da execução da instrução. Se o valor resultante for nulo, aplica-se o padrão seguro `0x1F80`. Essa blindagem previne que um processo malicioso em Ring 3 force uma exceção de proteção geral (`#GP(0)`) no kernel ao carregar bits reservados.
  * **Exceções de Coprocessador (`CR0.NE = 1`):** A função `enable_sse()` em `src/cpu.rs` ativa o bit 5 de `CR0` (`NE - Numeric Error`), garantindo que erros de ponto flutuante gerem interrupções nativas de hardware via vetor 16 (`#MF`), sem travar o barramento da CPU física.
* **Limitação Identificada:** O suporte atual cobre extensões x87, MMX e SSE (registradores `XMM0..XMM15`). Extensões AVX/AVX2 com registradores de 256 bits (`YMM`) ou AVX-512 (`ZMM`) exigem as instruções `XSAVE`/`XRSTOR` e manipulação do registrador `XCR0`, que estão em fase de planejamento no JHCS e ainda não foram integradas ao scheduler do kernel.

---

### 1.2. Escalonamento SMP e Barreira de Sincronização

* **Arquivo Auditado:** `src/smp.rs`, `src/scheduler.rs`
* **Diagnóstico da Implementação:**
  * A inicialização de múltiplos processadores utiliza a sequência canônica INIT-SIPI-SIPI com trampoline de 16 bits para entrar diretamente em modo longo de 64 bits.
  * A sincronização entre APs é controlada por um spinlock central (`SCHED_LOCK`) e pela variável atômica `AP_RUNNING_COUNT`.
  * Cada núcleo possui sua própria fila de execução e identifica seu ID via `current_cpu_id()`.
* **Fragilidades e Riscos de Concorrência:**
  * O uso de um spinlock global único (`SCHED_LOCK`) para proteger toda a estrutura `Scheduler` introduz contenção sob contagens elevadas de CPUs (ex: $\ge 8$ vCPUs).
  * A migração de tarefas entre núcleos durante preempção ocorre sem afinidade estrita de cache L1/L2, podendo gerar penalidades de *cache invalidation* em cargas intensivas.

---

## 2. Auditoria de Drivers e Gerenciamento de DMA (KDM)

### 2.1. Kernel DMA Manager e Mitigação de Ataques de DMA

* **Arquivo Auditado:** `src/dma/manager.rs`, `src/dma/token.rs`, `src/dma/bounce.rs`, `src/dma/state.rs`
* **Diagnóstico da Implementação:**
  * O KDM gerencia pools de memória física alinhada a 4 KiB e 2 MiB para dispositivos PCI.
  * O ciclo de vida do buffer é protegido por um `DmaToken` composto por um índice de 32 bits e um contador de geração de 32 bits (`generation`). Quando um buffer é liberado via `sys_dma_buffer_release`, seu contador de geração é incrementado atomicamente, invalidando qualquer tentativa posterior de acesso com o token antigo (*use-after-free mitigation*).
  * O mecanismo de **Bounce Buffers** (`src/dma/bounce.rs`) intercepta transferências destinadas a processos de usuário, copiando os dados para buffers do kernel antes de entregá-los ao hardware.
* **Limitações e Falso Sentido de Segurança:**
  * **Ausência de IOMMU de Hardware:** O KDM opera integralmente por software. Se um periférico PCI malicioso ou com defeito de firmware iniciar um ciclo de Bus Mastering para um endereço arbitrário de memória física sem consultar o driver do kernel, o hardware da CPU executará o acesso à revelia do KDM. A proteção total contra *Direct Memory Access Attacks* em barramentos físicos reais exige programação das tabelas de páginas da IOMMU (Intel VT-d DMAR ou AMD-Vi).
  * **Timeout de Hardware:** O encerramento por timeout em `src/dma/manager.rs` não garante por si só que o periférico cessou o acesso ao barramento; apenas impede que o sistema operacional confie nos dados retornados.

---

### 2.2. Barramento PCI e Detecção de Dispositivos

* **Arquivo Auditado:** `src/drivers/pci.rs`
* **Diagnóstico da Implementação:**
  * Enumeração completa de 256 barramentos via portas `0xCF8` e `0xCFC`.
  * Filtro de dispositivos gráficos: `class_code == 0x03` detecta controladoras de vídeo. O sistema reconhece corretamente VirtIO-GPU (`1AF4:1050`), Bochs BGA (`1234:1111`), controladoras Intel (`8086`), AMD (`1002`/`1022`) e NVIDIA (`10DE`).
  * As funções de leitura e escrita de registradores de configuração e ativação de Bus Mastering operam corretamente em ambiente virtual.

---

## 3. Investigação e Diagnóstico de Incidentes Críticos

### 3.1. Incidente do Mouse PS/2 e Conflito de Porta 60h / IRQ12

* **Sintoma Histórico:** Congelamento intermitente do cursor do mouse PS/2 ou perda de resposta do teclado durante eventos simultâneos de digitação e arrasto.
* **Causa Raiz Identificada:**
  1. O controlador 8042 compartilha a mesma porta de dados de I/O (`0x60`) para teclado (IRQ 1) e mouse (IRQ 12).
  2. Quando interrupções simultâneas ocorriam em núcleos distintos sob SMP, ocorria uma condição de corrida (*race condition*) na leitura da porta `0x60`: um byte de movimento do mouse era lido pelo manipulador do teclado ou vice-versa, corrompendo o pacote de 3 bytes do mouse e dessincronizando o alinhamento de estado.
* **Ação Corretiva Auditada:**
  * Criação do lock atômico de barramento `PORT60_LOCK` e rotina de drenagem controlada com timeout de 1000 iterações.
  * Validação estrita do bit de alinhamento obrigatório no primeiro byte do mouse (`byte[0] & 0x08 != 0`). Caso o bit esteja zerado, o decodificador descarta o byte e reinicia a máquina de estados, recuperando a sincronização em menos de 10 ms.
  * Teste de regressão validado em `tests/ps2_mouse_stress_tests.rs` com 10.000 pacotes consecutivos processados sem perda de sincronismo.

---

### 3.2. Incidente de Congelamento ao Abrir o Jinn Browser sob 64 MB de RAM

* **Sintoma Histórico:** Ao clicar no botão para abrir o Jinn Browser no ambiente gráfico Aladdin sob 64 MB de RAM, o sistema congelava completamente.
* **Diagnóstico Técnico por Camadas:**
  1. **Falha de Mídia / VFS (VirtualBox SATA vs. IDE):** Em máquinas virtuais recém-criadas no VirtualBox com controladora SATA AHCI padrão, o disco `disk.img` não era reconhecido pelo driver de bloco IDE PIO (`src/drivers/ide.rs`), tornando a partição FAT32 inacessível. O carregamento de `/disk/browser.elf` falhava com `-ENOENT`.
  2. **Estouro de Heap Estático no Servidor Aladdin Ring 3:** No código original de `apps/aladdin-server/src/main.rs`, o heap estático em `.bss` estava configurado com apenas 7 MiB. Como o backbuffer de 1024×768 pixels a 32 bpp consome 3,14 MiB, a alocação concorrente de buffers de superfície de clientes (`client_cache_buf`) excedia a capacidade do bump allocator, disparando `handle_alloc_error` no runtime do Rust e travando o display server.
* **Ações Corretivas Auditadas:**
  * Expansão do heap estático do servidor gráfico em espaço de usuário para 14 MiB (`const HEAP_SIZE = 14 * 1024 * 1024`), acomodando confortavelmente múltiplos clientes sob 64 MB de RAM física.
  * Inclusão de fallback transparente para carregar `browser.elf` diretamente dos módulos Limine da memória RAM caso a mídia de disco não esteja conectada.

---

### 3.3. Diagnóstico: Janela Cliente do Browser Não Renderizada

* **Sintoma Atual:** O log acusa a chamada de execução, o processo do navegador é criado, mas a janela cliente não aparece no desktop Aladdin.
* **Causa Raiz Identificada no Código:**
  1. No arquivo `apps/aladdin-server/src/main.rs:520`, o display server invoca a Syscall 39 (`SYS_SPAWN_PROCESS`) passando o caminho `/disk/browser.elf` e os argumentos `["/disk/browser.elf", "browser", NULL]`.
  2. No kernel (`src/syscall.rs:2836`), a criação do processo chama `crate::gui::register_browser_pid(pid)`. Porém, esta função registra o PID na variável estática `SESSION` do **Ring 0**.
  3. No modo primário Ring 3, a sessão ativa (`GLOBAL_SESSION`) reside no processo de usuário do `aladdin-server`, e o kernel mantém sua variável `SESSION` como `None`. Como resultado, o display server em Ring 3 nunca recebe a notificação de associação de PID via `register_browser_pid`.
  4. Além disso, o laço de polling do `aladdin-server` (`sys_list_client_windows`) processa janelas clientes criadas via Syscall 20 (`sys_window_create`). Quando o executável `browser.elf` inicia, se houver qualquer divergência no parsing dos argumentos `argc`/`argv` em `apps/ola-mundo/src/main.rs:_start`, o processo não executa `run_browser_interactive()`, caindo na suíte de testes de validação adversarial ou encerrando prematuramente.

---

### 3.4. Diagnóstico: Travamento Total (*Freeze*) ao Clicar no Gerenciador de Arquivos

* **Sintoma Atual:** Ao clicar em "Abrir o Gerenciador de Arquivos" no dock ou menu iniciar, o sistema sofre travamento completo.
* **Causa Raiz Identificada no Código:**
  1. No arquivo `Aladdin/src/desktop/session.rs:744`, o clique no slot do Gerenciador de Arquivos executa:
     ```rust
     self.files.refresh_cache(vfs);
     self.wm.bring_to_front(3);
     ```
  2. Em seguida, durante o ciclo de renderização do frame (`session.rs:407`), o compositor chama:
     ```rust
     let storage_adapter = crate::contracts::storage::SystemHostStorageAdapter(sys);
     self.files.render(buf, screen_w, screen_h, win.x, win.y, win.width, win.height, &storage_adapter);
     ```
  3. Dentro de `Aladdin/src/apps/files.rs:1071`, a barra inferior tenta obter as informações do volume ativo:
     ```rust
     let vol_opt = storage.get_volume(self.active_volume);
     ```
  4. No modo Ring 0 (`src/gui.rs:703`), a implementação de `storage_get_volume` executa:
     ```rust
     if let Ok(st) = crate::vfs::statvfs(pfx) {
         let total_mb = st.total_bytes / (1024 * 1024);
         let free_mb = st.free_bytes / (1024 * 1024);
         ...
     ```
  5. A chamada `crate::vfs::statvfs` adquire o lock `MOUNTS.lock()` e invoca o método `statvfs()` do sistema de arquivos FAT32 (`src/vfs/fat32.rs:1345`).
  6. **Ponto Crítico de Deadlock / Contenção:** O driver FAT32 percorre a tabela de alocação de arquivos (FAT) executando leituras síncronas de setores através do controlador de disco IDE PIO. Como o laço do compositor gráfico executa na mesma thread principal com interrupções parcialmente desabilitadas durante o desenho do frame, qualquer espera bloqueante por interrupção ou contenção de spinlock do disco causa o congelamento irreversível do loop gráfico.

---

## 4. Auditoria de Rede, Criptografia e TLS 1.3

### 4.1. Pilha TCP/IP e Sockets

* **Arquivo Auditado:** `src/net/tcp.rs`, `src/net/socket.rs`, `src/net/arp.rs`
* **Conformidade com RFC 793:** Máquina de estados completa: `Closed`, `Listen`, `SynSent`, `SynRcvd`, `Established`, `FinWait1`, `FinWait2`, `CloseWait`, `Closing`, `LastAck`, `TimeWait`.
* **Retransmissão e Controle de Fluxo:** Suporte a temporizadores de retransmissão com recuo exponencial (*exponential backoff*) e janelas deslizantes de recepção/transmissão de 16 KiB.
* **Testes Executados:** 13/13 testes de interoperabilidade TCP aprovados em `tests/tcp_socket_stress_tests.rs`.

---

### 4.2. Criptografia Nativa e CSPRNG

* **Arquivo Auditado:** `src/crypto/`
* **Entropia e Aleatoriedade:** O CSPRNG (`src/crypto/csprng.rs`) implementa ChaCha20 DRBG. A semente inicial é coletada de fontes não correlacionadas de ruído temporal do TSC e data/hora do RTC. A cada geração de bytes aleatórios, o estado interno sofre avanço irreversível (*ratchet*), garantindo sigilo futuro (*forward secrecy*).
* **Tempo Constante:** As primitivas de comparação de tags Poly1305 e aritmética bignum utilizam funções com tempo de execução constante para mitigar ataques de canal lateral baseados em tempo (*timing attacks*).

---

### 4.3. TLS 1.3 (RFC 8446) e Infraestrutura de Chaves Públicas (PKI X.509)

* **Arquivo Auditado:** `src/net/tls13.rs`, `src/net/x509.rs`, `src/net/pki.rs`
* **Conformidade RFC 8446:**
  * **Key Schedule Completo:** Geração e transição determinística entre `Early Secret`, `Handshake Secret` e `Master Secret` utilizando HKDF-SHA256.
  * **Transcript Hash:** Hash contínuo das mensagens trocadas no handshake, com injeção sintética do `message_hash` de 32 bytes em caso de `HelloRetryRequest`.
  * **AEAD:** Cifração autenticada de dados de aplicação utilizando ChaCha20-Poly1305 (RFC 8439) com nonces exclusivos combinados por XOR com sequence numbers de 64 bits.
* **Validação X.509 (RFC 5280):**
  * Parser DER nativo em Rust puro sem dependências externas.
  * Validação estrita de restrições de nome (Subject Alternative Name - SAN) com verificação de curingas (*wildcards*), restrições básicas (`is_ca = true` para emissores intermediários) e verificação temporal de validade (`not_before` e `not_after`).
  * Repositório de confiança (`TrustStore`) embutido contendo certificados raiz de autoridade certificadora (CA).

---

## 5. Auditoria de Persistência e Integridade de Arquivos

### 5.1. FAT32 e Transacionalidade de Aplicação

* **Arquivo Auditado:** `src/vfs/fat32.rs`, `Aladdin/src/apps/settings.rs`, `apps/ola-mundo/src/browser.rs`
* **Garantias do Sistema de Arquivos:**
  * O driver FAT32 do Jinn OS suporta alocação de novos clusters na FAT, leitura e escrita de dados, criação e exclusão de diretórios e arquivos.
  * O sistema de arquivos FAT32 **não é transacional nem possui journaling** no nível de disco. Uma interrupção abrupta de energia durante a escrita de setores na tabela FAT pode deixar cadeias de clusters órfãs.
* **Atomicidade no Nível de Aplicação:**
  * Para mitigar essa fraqueza estrutural do FAT32, as aplicações críticas (painel de configurações do Aladdin e banco de dados do Jinn Browser) utilizam um protocolo transacional de duas fases:
    1. Gravação dos dados em um arquivo temporário com sufixo `.TMP`.
    2. Leitura de confirmação e cálculo de soma de verificação em memória.
    3. Renomeação atômica do arquivo original para `.BAK` e do arquivo `.TMP` para o nome definitivo via Syscall 52 (`sys_rename`).
    4. Se ocorrer falha ou corrupção no arquivo principal, o carregador restaura automaticamente a versão `.BAK` íntegra.

---

## 6. Tabela Consolidada de Evidências por Subsistema

| Subsistema | Módulo Principal | Estado Auditado | Suíte de Testes Associada | Resultado / Evidência |
| :--- | :--- | :--- | :--- | :--- |
| **Boot & CPU** | `src/main.rs`, `src/cpu.rs` | **VALIDADO EM QEMU** | Boot QEMU Limine 8.x | Boot sem falhas, Limine framebuffer detectado |
| **GDT / TSS** | `src/gdt.rs` | **VALIDADO EM QEMU** | Boot SMP com 4 APs | Pilhas `RSP0` isoladas por CPU |
| **FPU / SIMD** | `src/fpu.rs` | **VALIDADO EM QEMU** | Eager FXSAVE/FXRSTOR | Sanitização de MXCSR (`0x0000_FFFF`), CR0.NE ativo |
| **Scheduler** | `src/scheduler.rs` | **VALIDADO EM QEMU** | Preempção APIC a 100 Hz | 100% de preempção em tarefas de Ring 3 |
| **Memory Paging** | `src/memory/paging.rs` | **IMPLEMENTADO E TESTADO**| `tests/heap_memory_stress_tests.rs` | Páginas de 4 KiB/2 MiB com proteção $W \oplus X$ |
| **KDM** | `src/dma/manager.rs` | **IMPLEMENTADO E TESTADO**| `tests/kdm_host_tests.rs` | 23/23 testes de tokens e bounce buffers aprovados |
| **PCI** | `src/drivers/pci.rs` | **VALIDADO EM QEMU** | Descoberta de barramento no boot | 9 dispositivos PCI catalogados |
| **PS/2 Mouse** | `src/interrupts/ps2_mouse.rs`| **VALIDADO EM QEMU** | `tests/ps2_mouse_stress_tests.rs` | 10.000 pacotes sem perda de sincronismo |
| **TCP / Sockets** | `src/net/tcp.rs` | **IMPLEMENTADO E TESTADO**| `tests/tcp_socket_stress_tests.rs` | 13/13 testes de estados e janelas aprovados |
| **TLS 1.3** | `src/net/tls13.rs` | **IMPLEMENTADO E TESTADO**| `tests/tls13_handshake_tests.rs` | Key Schedule, Transcript e AEAD RFC 8446 aprovados |
| **PKI X.509** | `src/net/pki.rs` | **IMPLEMENTADO E TESTADO**| `tests/pki_x509_host_tests.rs` | Parser DER, validação SAN e cadeia de certificados OK |
| **VFS / FAT32** | `src/vfs/fat32.rs` | **VALIDADO EM QEMU** | `tests/vfs_fat32_ext2_stress_tests.rs` | Leitura/escrita FAT32 persistida em `disk.img` |
| **Aladdin Server**| `apps/aladdin-server` | **VALIDADO EM QEMU** | Boot e loop a 60 FPS | Servidor Ring 3 registrado, heap de 14 MiB OK |
| **JHCS Middleware**| `D:\Projetos\jhcs-rs` | **IMPLEMENTADO E TESTADO**| MatMul 4x4 DAG Example | Paridade numérica CPU vs SIMD aprovada |
