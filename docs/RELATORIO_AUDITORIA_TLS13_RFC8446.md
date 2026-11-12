# Relatório Técnico de Auditoria de Conformidade Normativa: TLS 1.3 (RFC 8446 / RFC 5869) no Jinn OS

---

## 1. Objetivo e Escopo da Auditoria

Este relatório consolida a auditoria técnica e criptográfica independente da implementação da **Fase 3: TLS 1.3 Key Schedule, Transcript Hash e Handshake State Machine** (`src/net/tls13.rs`), desenvolvida para a pilha de rede segura do **Jinn Microkernel** e do **Jinn Browser**.

A auditoria verificou o código linha a linha quanto à aderência estrita às especificações **RFC 8446 (The Transport Layer Security (TLS) Protocol Version 1.3)** e **RFC 5869 (HMAC-based Extract-and-Expand Key Derivation Function - HKDF)**.

---

## 2. Auditoria Estrutural dos Subsistemas Criptográficos

### 2.1. Funções de Derivação HKDF (RFC 5869) e HKDF-Expand-Label (RFC 8446 § 7.1)
- **HKDF-Extract:** Utiliza o motor `hmac_sha256` nativo do microkernel. Calcula $\text{PRK} = \text{HMAC-Hash}(\text{Salt}, \text{IKM})$. Quando o salt é zero ou omitido, aplica buffer preenchido com 32 zeros, conforme a RFC 5869 § 2.2.
- **HKDF-Expand:** Implementa a iteração de blocos $T(i) = \text{HMAC-Hash}(\text{PRK}, T(i-1) \parallel \text{info} \parallel i)$ para $i = 1 \dots N$.
- **HKDF-Expand-Label:** Valida a serialização formal da estrutura `HkdfLabel`:
  ```text
  struct {
      uint16 length = Length;
      opaque label<7..255> = "tls13 " + Label;
      opaque context<0..255> = Context;
  } HkdfLabel;
  ```
  - **Verificação:** Comprovou-se a inserção mandatória do prefixo literal `b"tls13 "` (6 bytes) concatenado ao rótulo específico, comprimento `uint16` em Big-Endian e vetor de contexto com cabeçalho de 1 byte.
  - **Conformidade:** 100% conforme RFC 8446 § 7.1.

---

### 2.2. Árvore de Chaves do TLS 1.3 Key Schedule (RFC 8446 § 7.1)
O fluxo de transição de segredos foi auditado em todos os nós da árvore criptográfica:

```
             0
             |
             v
   PSK -> HKDF-Extract = Early Secret
             |
             +-----> Derive-Secret(..., "c e traffic", ClientHello)
             |
             v
     Derive-Secret(..., "derived", "")
             |
             v
(ECDHE) -> HKDF-Extract = Handshake Secret
             |
             +-----> Derive-Secret(..., "c hs traffic", ClientHello...ServerHello)
             |
             +-----> Derive-Secret(..., "s hs traffic", ClientHello...ServerHello)
             |
             v
     Derive-Secret(..., "derived", "")
             |
             v
     0 -> HKDF-Extract = Master Secret
             |
             +-----> Derive-Secret(..., "c ap traffic", ClientHello...server Finished)
             |
             +-----> Derive-Secret(..., "s ap traffic", ClientHello...server Finished)
             |
             +-----> Derive-Secret(..., "res master", ClientHello...client Finished)
```

- **Verificação:** Todas as chamadas `Derive-Secret(Secret, Label, Transcript)` utilizam o hash corrente do histórico da sessão (`TranscriptHash`).
- **Chaves AEAD e IVs:** As chaves simétricas de 128/256 bits e os IVs estáticos de 12 bytes (96 bits) são derivados corretamente com os rótulos `b"key"` e `b"iv"` a partir dos segredos de tráfego correspondentes.
- **Resumption Master Secret:** Implementou-se a derivação de `res master` a partir do `Master Secret` após o Finished do cliente.
- **Conformidade:** 100% conforme RFC 8446 § 7.1.

---

### 2.3. Integridade do Transcript Hash (RFC 8446 § 4.4.1)
- **Delimitação de Camadas (Record Layer vs Handshake Message):**
  - A RFC 8446 § 4.4.1 exige estritamente que o hash seja calculado sobre o cabeçalho de 4 bytes da mensagem de handshake (`HandshakeType` + comprimento de 24 bits) e seu corpo, **excluindo expressamente qualquer cabeçalho do Record Layer** (5 bytes: tipo de record, versão legado e tamanho).
  - **Verificação:** O método `TranscriptHash::update` é invocado unicamente com a fatia da mensagem de handshake (ex: `HandshakeType::ClientHello` + comprimento + payload), sem o envelope do Record Layer.
  - **Imutabilidade e Snapshots:** O método `current_hash(&self)` clona o hasher interno SHA-256 e finaliza o clone sem mutar o acumulador da sessão.
  - **Conformidade:** 100% conforme RFC 8446 § 4.4.1.

---

### 2.4. Mecanismo de HelloRetryRequest e `message_hash` (RFC 8446 § 4.4.1 & § 4.1.4)
- **Identificação de HRR:** Reconhece o random fixo padronizado:
  $$\text{HELLO\_RETRY\_REQUEST\_RANDOM} = \text{SHA-256}(\text{"HelloRetryRequest"})$$
- **Substituição Sintética:** Ao receber um HelloRetryRequest, o cliente não concatena simplesmente o ClientHello1 anterior. Em vez disso, a RFC 8446 § 4.4.1 exige a substituição de ClientHello1 pela mensagem sintética `message_hash`:
  ```text
  struct {
      HandshakeType msg_type = message_hash;    /* 254 */
      uint24 length = Hash.length;              /* 32 */
      opaque Hash.digest[Hash.length];          /* SHA-256(ClientHello1) */
  };
  ```
  - **Verificação:** A rotina `handle_hello_retry_request()` reseta o hasher do transcript e acumula `message_hash` seguido do `HelloRetryRequest`. Em seguida, `build_client_hello2()` acumula o novo ClientHello2.
  - **Conformidade:** 100% conforme RFC 8446 § 4.4.1.

---

### 2.5. Verificação do CertificateVerify (RFC 8446 § 4.4.3)
- **Construção do Payload de Assinatura:**
  - 64 repetições do caractere espaço (`0x20`).
  - Context string literal: `b"TLS 1.3, server CertificateVerify"`.
  - Byte separador nulo (`0x00`).
  - `Transcript-Hash(ClientHello...Certificate)` (32 bytes SHA-256).
- **Validação Criptográfica:** O payload estruturado é entregue à chave pública extraída do certificado X.509 verificado na Fase 2, suportando tanto assinaturas **RSA-PSS** (com salting e hash SHA-256) quanto **ECDSA P-256**.
- **Conformidade:** 100% conforme RFC 8446 § 4.4.3.

---

### 2.6. Verificação do Bloco Finished e Comparação em Tempo Constante (RFC 8446 § 4.4.4)
- **Cálculo de `verify_data`:**
  $$\text{finished\_key} = \text{HKDF-Expand-Label}(\text{BaseKey}, \text{"finished"}, \text{""}, 32)$$
  $$\text{verify\_data} = \text{HMAC-SHA256}(\text{finished\_key}, \text{Transcript-Hash})$$
- **Proteção contra Timing Attacks:** A rotina `verify_server_finished()` compara os 32 bytes recebidos com a expectativa calculada através de operação XOR acumulada em tempo constante:
  ```rust
  let mut diff = 0u8;
  for i in 0..32 {
      diff |= received_verify_data[i] ^ expected[i];
  }
  if diff == 0 { Ok(()) } else { Err(Tls13Alert::DecryptError) }
  ```
- **Conformidade:** 100% conforme RFC 8446 § 4.4.4.

---

### 2.7. Construção de Nonces AEAD (RFC 8446 § 5.3)
- O nonce de 12 bytes por registro é obtido aplicando a operação XOR entre o IV estático de 12 bytes e o Sequence Number de 64 bits alinhado à direita com preenchimento de 4 bytes zero à esquerda.
- O método `Tls13KeySchedule::compute_nonce(&iv, seq_num)` executa essa transformação sem alocações dinâmicas de heap.
- **Conformidade:** 100% conforme RFC 8446 § 5.3.

---

## 3. Conclusão da Auditoria

O subsistema TLS 1.3 do microkernel Jinn OS cumpre com integridade e rigor todas as cláusulas normativas das RFCs 8446 e 5869 auditadas. Não foram identificadas vulnerabilidades de oráculo de padding, correspondência cega de strings ou vazamentos de canal lateral de temporização.

