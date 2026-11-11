# JINN OS — RELATÓRIO TÉCNICO FORMAL: FASE 2
## Validação da Cadeia de Confiança PKI X.509 e Parser Estrutural DER (RFC 5280 / RFC 6125)

**Data de Emissão:** 11 de Novembro de 2026  
**Status do Subsistema:** Homologado e Integrado  
**Alvo Arquitetural:** Jinn Microkernel (`x86_64-unknown-none`, `no_std`)  
**Repositório:** `E:\Projetos\Jinn`  

---

## 1. Resumo Executivo e Contexto

No escopo de modernização do microkernel **Jinn OS**, a **Fase 2** foi dedicada à substituição definitiva do modelo simplificado de correspondência de strings (*string matching*) por uma infraestrutura formal de **Chaves Públicas (PKI - Public Key Infrastructure)** e **Validação Criptográfica de Cadeia de Certificados X.509** em conformidade rigorosa com as normas **RFC 5280** e **RFC 6125**.

Essa implementação assegura que o **Jinn Core** e o **Jinn Browser** possam autenticar criptograficamente pontos de terminação TLS/HTTPS, impedindo ataques de personificação (*spoofing*), interceptação (*man-in-the-middle*) e emissões fraudulentas por autoridades não autorizadas.

---

## 2. Arquitetura Implementada

A infraestrutura foi desenhada em camadas desacopladas e modulares no espaço de rede e criptografia do microkernel:

```
+---------------------------------------------------------------------------------------+
|                                    Jinn Browser / HTTP                                |
+---------------------------------------------------------------------------------------+
                                           |
                                           v
+---------------------------------------------------------------------------------------+
| `src/net/pki.rs` — Validador de Cadeia de Confiança & Trust Store (RFC 5280 / RFC 6125)|
|  - TrustStore com Trust Anchors Embutidos (ISRG Root X1 / RSA-4096, DigiCert / RSA-2048)  |
|  - Algoritmo Bottom-Up Path Builder (Leaf -> Intermediário(s) -> Trust Anchor)         |
|  - Validação Estrita de Hostname & SAN / Wildcards (RFC 6125)                          |
+---------------------------------------------------------------------------------------+
                                           |
                                           v
+---------------------------------------------------------------------------------------+
| `src/net/x509.rs` — Analisador Estrutural ASN.1 DER (Zero-Alloc Safe Parsing)         |
|  - Decodificação de TBSCertificate, SPKI, Validity Window, DN (CN, O, OU, C, ST, L)   |
|  - Extração de Extensões: BasicConstraints (cA, pathLen), KeyUsage, EKU, SAN          |
+---------------------------------------------------------------------------------------+
                                           |
                                           v
+---------------------------------------------------------------------------------------+
| `src/crypto/` — Primitivas Criptográficas Nativas do Microkernel                      |
|  - `bignum.rs`: Aritmética de precisão arbitrária e Exponenciação Modular Knuth D     |
|  - `rsa.rs`: Verificação de Assinatura RSA PKCS#1 v1.5 e RSA-PSS com SHA-256          |
|  - `p256.rs`: Aritmética Elíptica NIST P-256 e Verificação ECDSA                     |
|  - `sha256.rs`: Algoritmo de Hashing Criptográfico SHA-256 (RFC 6234)                 |
+---------------------------------------------------------------------------------------+
```

---

## 3. Especificações dos Módulos Desenvolvidos

### 3.1. Parser Estrutural DER e ASN.1 (`src/net/x509.rs`)
- **Zero-Copy e Limites Seguros:** O leitor `DerReader` percorre os bytes em formato DER efetuando validação estrita de tags e comprimentos (definidos e múltiplos octetos), prevenindo transbordamentos de buffer (*buffer overflow*) e alocações recursivas.
- **Estruturas TBSCertificate:**
  - Extração do campo `version` (v1, v2 ou v3 com identificação explícita de `[0]`).
  - Extração do `serialNumber` em bytes brutos.
  - Extração do identificador de algoritmo (`SignatureAlgorithm`): `Sha256WithRsa`, `RsaPssSha256`, `EcdsaWithSha256`, `Sha384WithRsa`, `Sha512WithRsa`, `Ed25519`.
  - Extração e formatação canônica de Distinguished Names (`Issuer` e `Subject`), com suporte a RDNs múltiplos (`CN`, `O`, `OU`, `C`, `ST`, `L`).
  - Janela de Validade (`notBefore` e `notAfter`), convertendo `UTCTime` e `GeneralizedTime` em segundos UNIX epoch através de `x509_date_to_unix`.
  - Subject Public Key Info (`SPKI`): decodificação da chave pública de sujeito para RSA (2048/4096 bits), ECDSA NIST P-256 e Ed25519.
- **Extensões de Governança PKI:**
  - `BasicConstraints` (OID `2.5.29.19`): flag booleana `is_ca` e restrição de profundidade `path_len_constraint`.
  - `KeyUsage` (OID `2.5.29.15`): verificação dos bits operacionais (`digitalSignature`, `keyCertSign`, etc.).
  - `ExtendedKeyUsage` (OID `2.5.29.37`): verificação do propósito `serverAuth` (`1.3.6.1.5.5.7.3.1`) ou `anyExtendedKeyUsage`.
  - `SubjectAlternativeName` (OID `2.5.29.17`): extração de nomes de domínio (`dNSName`) e endereços IP (`iPAddress`).

### 3.2. Validador da Cadeia de Confiança e Trust Store (`src/net/pki.rs`)
- **Trust Store Embutido:**
  - **ISRG Root X1:** Let's Encrypt Root CA (RSA 4096 bits, $e=65537$).
  - **DigiCert Global Root CA:** DigiCert Inc (RSA 2048 bits, $e=65537$).
  - Capacidade dinâmica para inserção de novas âncoras validadas via `TrustStore::add_certificate` (exigindo autoassinatura válida e flag `cA=true`).
- **Construção de Caminho (Path Building):**
  - Algoritmo bottom-up que remonta do nó folha (`leaf`) passando pelos certificados intermediários até a âncora raiz confiável.
- **Verificação Matemática Estrita:**
  - Cada elo da cadeia é verificado contra a chave pública do elo pai calculando o hash SHA-256 sobre os bytes originais de `raw_tbs`.
  - O certificado raiz da cadeia é submetido à prova criptográfica contra a chave pública pré-armazenada no Trust Store.
  - Rejeição mandatória se qualquer elo intermediário não possuir `is_ca = true` ou omitir o bit `keyCertSign`.
  - Enforçamento das restrições de profundidade da árvore (`pathLenConstraint`).

### 3.3. Conformidade Estrita com RFC 6125 (Hostname, SAN e Wildcards)
- **Precedência de SAN:** Em estrita aderência ao padrão RFC 6125, a presença da extensão `SubjectAlternativeName` anula o uso de `Subject CommonName` (legado). A validação é realizada unicamente sobre os itens SAN.
- **Correspondência de Wildcards:**
  - Curingas são aceitos **exclusivamente** no rótulo mais à esquerda (`leftmost label`), como em `*.jinn-os.org`.
  - Curingas parciais (ex: `sub*.dominio.org`) são sumariamente rejeitados.
  - Curingas multinível são proibidos: `*.jinn-os.org` valida `portal.jinn-os.org`, mas rejeita estritamente `a.b.jinn-os.org`.
  - O domínio ápice sem prefixo (ex: `jinn-os.org`) não coincide com `*.jinn-os.org`.
  - Curingas em TLDs públicos (ex: `*.com`, `*.org`, `*`) são terminantemente bloqueados.

---

## 4. Matriz de Testes de Unidade e Provas Criptográficas

A suíte formal de testes foi implementada em `tests/x509_pki_tests.rs`, composta por 15 cenários automatizados cobrindo condições positivas e ataques adversariais:

| Identificador do Teste | Escopo da Validação | Resultado Obtido |
|:---|:---|:---:|
| `test_positive_full_chain_path_building` | Validação completa de 3 níveis: Folha -> Intermediário -> Raiz (ISRG/OpenSSL) | **APROVADO** |
| `test_premature_certificate_rejected` | Rejeição de certificado com data atual anterior ao `notBefore` | **APROVADO** |
| `test_expired_certificate_rejected` | Rejeição de certificado com data atual posterior ao `notAfter` | **APROVADO** |
| `test_forged_certificate_rejected` | Rejeição sumária de certificado forjado com chave espúria e falso emissor | **APROVADO** |
| `test_corrupted_tbs_and_corrupted_signature_rejected` | Detecção e falha criptográfica após adulteração de 1 byte no TBS ou assinatura | **APROVADO** |
| `test_non_ca_intermediate_rejected` | Rejeição de cadeia onde intermediário possui `basicConstraints: cA=false` | **APROVADO** |
| `test_intermediate_missing_key_cert_sign_rejected` | Rejeição de intermediário com `keyUsage` sem permissão `keyCertSign` | **APROVADO** |
| `test_hostname_rfc6125_san_exact_match` | Correspondência exata e insensível a maiúsculas/minúsculas no SAN | **APROVADO** |
| `test_hostname_rfc6125_san_valid_wildcard` | Correspondência válida de 1 nível para curingas `*.jinn-os.org` | **APROVADO** |
| `test_hostname_rfc6125_san_multilevel_wildcard_rejected` | Rejeição estrita de subdomínio de 2º nível `a.b.jinn-os.org` para `*.jinn-os.org` | **APROVADO** |
| `test_hostname_rfc6125_san_apex_domain_rejected` | Rejeição do domínio ápice `jinn-os.org` contra o padrão curinga `*.jinn-os.org` | **APROVADO** |
| `test_hostname_rfc6125_domain_mismatch_rejected` | Rejeição de certificados válidos apresentados por domínios não autorizados | **APROVADO** |
| `test_hostname_rfc6125_san_ip_address_matching` | Validação de correspondência de endereços IP via SAN `iPAddress` | **APROVADO** |
| `test_preloaded_trust_anchors` | Verificação dos módulos matemáticos das raízes ISRG Root X1 e DigiCert Global Root CA | **APROVADO** |
| `test_ecdsa_p256_verification_known_point` | Verificação de curvas elípticas e pontos afins na curva NIST P-256 | **APROVADO** |

**Resultado Geral dos Testes:**
```text
running 15 tests
test test_ecdsa_p256_verification_known_point ... ok
test test_forged_certificate_rejected ... ok
test test_expired_certificate_rejected ... ok
test test_hostname_rfc6125_domain_mismatch_rejected ... ok
test test_hostname_rfc6125_san_apex_domain_rejected ... ok
test test_hostname_rfc6125_san_multilevel_wildcard_rejected ... ok
test test_intermediate_missing_key_cert_sign_rejected ... ok
test test_non_ca_intermediate_rejected ... ok
test test_preloaded_trust_anchors ... ok
test test_corrupted_tbs_and_corrupted_signature_rejected ... ok
test test_premature_certificate_rejected ... ok
test test_hostname_rfc6125_san_ip_address_matching ... ok
test test_hostname_rfc6125_san_exact_match ... ok
test test_positive_full_chain_path_building ... ok
test test_hostname_rfc6125_san_valid_wildcard ... ok

test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

---

## 5. Homologação da Compilação e Geração de Mídia

1. **Checagem do Kernel (`no_std`):**
   ```bash
   cargo check --target x86_64-unknown-none '-Zbuild-std=core,alloc'
   # Status: OK (0 erros de compilação)
   ```
2. **Execução das Suítes Complementares:**
   - `tests/pki_x509_host_tests.rs`: 12 testes aprovados (0 falhas).
   - `browser/crates/jbe-tls`: 11 testes aprovados (0 falhas).
   - `tests/tcp_socket_stress_tests.rs`: 16 testes aprovados (0 falhas).
   - `tests/vfs_fat32_ext2_stress_tests.rs`: 17 testes aprovados (0 falhas).
3. **Geração do Binário e Mídia de Boot:**
   - Binário `boot/kernel.elf` atualizado.
   - Imagem de CD-ROM inicializável `jinn.iso` regenerada com bootloader Limine e suporte aos novos módulos de segurança.

---

## 6. Conclusão

Com a conclusão da **Fase 2**, o microkernel **Jinn OS** e suas aplicações de rede contam com um subsistema de autenticação criptográfica completo e auditável. As vulnerabilidades associadas à validação ingênua de certificados foram completamente eliminadas, estabelecendo uma base criptográfica sólida e em total conformidade com os padrões globais da Internet.

