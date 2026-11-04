# ADR-005: Formato de Keystore Seguro com Derivação Argon2id e Cifra AES-256-GCM

**Status:** Aprovado  
**Data:** 2026-11-04  
**Contexto:** Jinn Web3 Phase II — Aladdin Desktop Keystore Security Engine  
**Decisores:** Jinn Core Maintainers, Web3 Infrastructure Lead  

---

## 1. Contexto

A manipulação de sementes e chaves privadas criptográficas em sistemas operacionais convencionais é frequentemente exposta a vulnerabilidades de memória (cold boot attacks, page swapping, core dumps e extração em processos compartilhados).

No Jinn OS, a custódia de chaves do Aladdin Desktop deve satisfazer:
1. **Isolamento de Privilégios:** Chaves privadas nunca devem ser passadas para o microkernel (Ring 0).
2. **Resistência a Força Bruta:** Derivação resistente a ataques acelerados por GPU/ASIC.
3. **Criptografia Autenticada:** Detecção imediata de violação de integridade ou adulteração de ciphertext/IV.
4. **Higiene de Memória:** Limpeza forçada e volátil (*zeroization*) de buffers após o uso.

## 2. Decisão Arquitetural

1. **KDF (Função de Derivação de Chaves):**
   Adotado o padrão **Argon2id** (RFC 9106, vencedor da Password Hashing Competition):
   - Memória ($m$): 65.536 KiB (64 MiB)
   - Iterações ($t$): 3 passadas
   - Paralelismo ($p$): 4 threads
   - Salt: 16 bytes obtidos via gerador seguro do sistema operacional (`OsRng`).

2. **Cifra Autenticada (AEAD):**
   Adotado o **AES-256-GCM** (NIST SP 800-38D):
   - Chave: 256 bits derivada do Argon2id.
   - IV (Nonce): 96 bits (12 bytes) aleatórios por envelope.
   - Tag de Autenticação (MAC): 128 bits (16 bytes).

3. **Curva Elíptica e Derivação de Endereço:**
   - Curva **secp256k1** com validação de ponto uncompressed (`0x04 || X || Y`).
   - Hash Keccak-256 aplicado sobre os 64 bytes das coordenadas $(X, Y)$.
   - Endereço oficial correspondente aos 20 bytes finais (`hash[12..32]`).

4. **Higiene de Memória:**
   - Encapsulamento de buffers sensíveis na estrutura `SecureBuffer` com implementação do trait `Zeroize` e `ZeroizeOnDrop`.
   - Chaves intermediárias derivadas são zeradas imediatamente após a encriptação/decriptação.

## 3. Consequências

- **Positivas:**
  - Compatibilidade com o ecossistema EVM e ferramentas padrão Web3.
  - Segurança comprovada por padrões criptográficos estabelecidos.
  - Prevenção garantida contra adulteração e decifração não autorizada.
- **Mitigações:**
  - Para ambientes de teste automatizados em CI, foi disponibilizado parâmetro `custom_kdf` reduzido para preservar velocidade de execução sem comprometer os parâmetros de produção.
