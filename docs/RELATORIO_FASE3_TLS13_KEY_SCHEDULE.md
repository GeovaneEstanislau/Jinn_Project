# RELATÓRIO TÉCNICO: FASE 3 — TLS 1.3 KEY SCHEDULE, TRANSCRIPT HASH E RESOLUÇÃO DO TECLADO PS/2

**Data:** 11 de Novembro de 2026  
**Status:** HOMOLOGADO E VERIFICADO  
**Alvo:** Jinn Microkernel v0.0.3-userspace & Subsistema Criptográfico TLS 1.3  
**Arquitetura:** x86_64 (`#![no_std]`)  
**Repositórios:** `E:\Projetos\Jinn` e `E:\Projetos\Aladdin`

---

## 1. Sumário Executivo

Este trabalho concluiu com êxito duas frentes técnicas essenciais para a estabilização do **Jinn OS Alpha 0.0.3**:

1. **Correção e Endurecimento da Entrada PS/2 (Teclado e Mouse):** Diagnóstico e resolução da concorrência e congelamento de teclado após a carga do Aladdin Desktop em Ring 3, assegurando tratamento de timeout/OBF no controlador 8042, fila não-bloqueante no servidor gráfico e fallback de foco para o Terminal.
2. **Implementação da Fase 3 — TLS 1.3 Key Schedule e Handshake (RFC 8446):** Construção da infraestrutura criptográfica completa do TLS 1.3 em `src/net/tls13.rs`, implementando derivações HKDF (RFC 5869), cálculo de Transcript Hash SHA-256 contínuo, máquina de estados do Handshake (`ClientHello`, `ServerHello`, `EncryptedExtensions`, `Certificate`, `CertificateVerify`, `Finished`), derivação de chaves AEAD e validação de autenticidade com o motor X.509 da Fase 2.

A conformidade foi atestada por **10 novos testes unitários e de estresse** em `tests/tls13_handshake_tests.rs`, somando um total de **96 testes automatizados aprovados (100% de sucesso)** no host e boot verificado no **QEMU x86_64 (`-m 64M`)**.

---

## 2. Diagnóstico e Resolução do Teclado Congelado (Parte 1)

### 2.1 Causa Raiz Identificada

1. **Descarte de Eventos por Concorrência de Polling no `aladdin-server`:**
   - Anteriormente, o compositor possuía closures separadas para `poll_key` e `poll_mouse`, ambas chamando a Syscall 75 (`sys_graphics_poll_input`).
   - Quando `poll_key` recebia um evento de mouse (`kind >= 3`), o evento era sumariamente descartado. Quando `poll_mouse` recebia uma tecla, apenas um único valor era salvo em um atomic, sobrescrevendo qualquer tecla prévia.
2. **Perda de Foco da Janela Interativa:**
   - O `AladdinSession` somente despachava scancodes se a janela no topo do Z-order (`top_window_id()`) fosse uma janela de cliente ou pertencesse estritamente ao terminal (`top_idx == 1`). Se o usuário clicasse no Monitor de Sistema (janela 0) ou no papel de parede, o desktop perdia o foco de digitação e todas as teclas eram ignoradas.
3. **Ausência de Drenagem e Recuperação de OBF no Driver PS/2:**
   - Em caso de timeout de leitura no controlador 8042, a porta de status (`0x64`) e a porta de dados (`0x60`) não eram drenadas, deixando o buffer de saída do hardware travado em estado pendente (`Output Buffer Full`).

### 2.2 Correções Implementadas

- **Tratamento de Exceção PS/2 (`src/interrupts/ps2_keyboard.rs`):**
  - Em `ps2_wait_write` e `ps2_wait_read`, adicionada drenagem automática de `inb(0x60)` após esgotamento de timeout, recuperando o controlador de estados travados.
  - Em `kbd_irq_handler`, verificado o bit 0 de status (`OBF`) antes da leitura de dados e garantido o envio imediato de EOI ao PIC.
- **Fila Unificada e Resiliente de Eventos (`apps/aladdin-server/src/main.rs`):**
  - Implementada fila FIFO circular `KEY_QUEUE` de 64 entradas no servidor gráfico.
  - Implementada função de drenagem em lote (`drain_hardware_events`) que consome até 16 eventos por ciclo: teclas são enfileiradas sem perda e movimentos/cliques de mouse atualizam as coordenadas `CURRENT_MOUSE` de forma consistente.
- **Fallback de Foco no Compositor (`Aladdin/src/desktop/session.rs`):**
  - Implementado fallback automático de teclado: quando a janela em foco não for uma aplicação de texto ativa, os scancodes são automaticamente direcionados ao Terminal Interativo caso ele esteja aberto, eliminando a percepção de congelamento.

---

## 3. Arquitetura da Fase 3 — TLS 1.3 RFC 8446 (Parte 2)

### 3.1 Funções de Derivação HKDF (RFC 5869 & RFC 8446 Seção 7.1)

O módulo [`src/net/tls13.rs`](file:///E:/Projetos/Jinn/src/net/tls13.rs) implementa:

1. **`hkdf_extract(salt, ikm) -> [u8; 32]`:** Extração de entropia baseada em HMAC-SHA256.
2. **`hkdf_expand(prk, info, okm_len) -> Vec<u8>`:** Expansão em múltiplos blocos $T(1), T(2), \dots$ conforme a RFC 5869.
3. **`hkdf_expand_label(secret, label, context, length) -> Vec<u8>`:** Constrói a estrutura tipada `HkdfLabel` prefixando o rótulo com `"tls13 "` e codificando tamanhos em *big-endian*.
4. **`derive_secret(secret, label, transcript_hash) -> [u8; 32]`:** Derivação segura parametrizada pelo hash do histórico do handshake.

### 3.2 Árvore Criptográfica (Key Schedule)

A máquina de estados implementa a progressão rigorosa de segredos:

```
               0
               |
               v
  (0 / PSK) -> HKDF-Extract = Early Secret
               |
               v
         Derive-Secret(., "derived", "")
               |
               v
  (ECDHE)  ->  HKDF-Extract = Handshake Secret
               |
               +---> Derive-Secret(., "c hs traffic", CH..SH) -> client_handshake_key/iv
               |
               +---> Derive-Secret(., "s hs traffic", CH..SH) -> server_handshake_key/iv
               |
               v
         Derive-Secret(., "derived", "")
               |
               v
  (0)      ->  HKDF-Extract = Master Secret
               |
               +---> Derive-Secret(., "c ap traffic", CH..SF) -> client_app_key/iv
               |
               +---> Derive-Secret(., "s ap traffic", CH..SF) -> server_app_key/iv
```

### 3.3 Transcript Hash

A estrutura `TranscriptHash` encapsula uma instância de `Sha256` que acumula todas as mensagens de handshake sem os cabeçalhos de 5 bytes da camada de registro (*Record Layer*). Permite inspeções instantâneas (`current_hash()`) sem invalidar o estado contínuo.

### 3.4 Derivação de Nonces AEAD

Para cada pacote protegido por ChaCha20-Poly1305 ou AES-GCM:
- O IV base derivado (96 bits / 12 bytes) sofre uma operação **XOR** contra o sequence number do registro (64 bits alinhados à direita com 4 bytes zero à esquerda).

### 3.5 Verificação de `CertificateVerify` e `Finished`

- **`CertificateVerify`:** O payload assinado pelo servidor é construído com 64 octetos `0x20`, a string de contexto `b"TLS 1.3, server CertificateVerify\0"` e o `transcript_hash`. A assinatura é verificada utilizando a chave pública extraída do certificado X.509 da Fase 2.
- **`Finished`:** O cliente e o servidor derivam `finished_key` via `HKDF-Expand-Label(secret, "finished", "", 32)` e computam o HMAC sobre o transcript. A validação é realizada em tempo constante para mitigar ataques de canal lateral (*timing attacks*).

---

## 4. Matriz de Cobertura de Testes Automatizados no Host

| Suíte de Testes | Arquivo | Testes Executados | Status |
| :--- | :--- | :---: | :---: |
| **TLS 1.3 Handshake & Key Schedule** | `tests/tls13_handshake_tests.rs` | 10 | **APROVADO (10/10)** |
| **PKI X.509 RFC 5280 / RFC 6125** | `tests/x509_pki_tests.rs` | 15 | **APROVADO (15/15)** |
| **VFS FAT32 / Ext2 & Inodes** | `tests/vfs_fat32_ext2_stress_tests.rs` | 17 | **APROVADO (17/17)** |
| **TCP Sockets & Fluxo HTTP** | `tests/tcp_socket_stress_tests.rs` | 16 | **APROVADO (16/16)** |
| **Criptografia & Assinaturas X.509** | `tests/pki_x509_host_tests.rs` | 12 | **APROVADO (12/12)** |
| **Segurança de Syscalls & Ring 3** | `tests/syscall_security_stress_tests.rs` | 6 | **APROVADO (6/6)** |
| **Heap, Buddy Allocator & Cotas** | `tests/heap_memory_stress_tests.rs` | 6 | **APROVADO (6/6)** |
| **Application Runtime & Isolamento**| `tests/application_runtime_tests.rs` | 7 | **APROVADO (7/7)** |
| **Kernel DMA Manager (KDM)** | `tests/kdm_host_tests.rs` | 7 | **APROVADO (7/7)** |
| **Total Consolidado** | — | **96** | **100% APROVADO** |

---

## 5. Conclusão

Com a conclusão desta fase:
1. O subsistema de entrada PS/2 e a fila de eventos do compositor Aladdin Desktop estão imunes a perdas de pacotes e bloqueios de foco.
2. O microkernel Jinn OS dispõe agora de uma pilha TLS 1.3 nativa, robusta e matematicamente alinhada com as RFCs 8446 e 5869.
3. A imagem ISO (`jinn.iso`) e o disco virtual (`disk.img`) foram atualizados e verificados em execução com 64 MB de RAM no QEMU.
