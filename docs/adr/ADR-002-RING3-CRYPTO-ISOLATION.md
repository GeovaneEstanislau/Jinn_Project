# ADR-002: Isolamento Estrito de Chaves Criptográficas em Ring 3

**Status:** Aprovado (Diretriz Inviolável)  
**Data:** Novembro de 2026  
**Decisores:** Conselho de Arquitetura e Segurança do Jinn Project  

---

## 1. Contexto

A integração de capacidades de blockchain e carteiras digitais em sistemas operacionais frequentemente introduz vetores graves de ataque quando credenciais ou chaves privadas são manipuladas com privilégios de sistema excessivos.

## 2. Decisão

Estabelecemos como **regra inviolável de projeto** que **o microkernel Jinn (Ring 0) jamais manipulará, armazenará ou receberá chaves privadas, sementes mnemônicas (seed phrases) ou lógicas de consenso blockchain**.

### Diretrizes de Isolamento:
1. O kernel Ring 0 oferece estritamente serviços de rede agnósticos de transporte (sockets TCP, interface Ethernet e firewall JinnWall).
2. Todo o gerenciamento de credenciais Web3, derivação de chaves via Argon2id, decifração de keystores AES-256-GCM e assinaturas elípticas secp256k1 é executado **exclusivamente no espaço do usuário (Ring 3)** pelo daemon `jinn-web3d` e pela aplicação `Aladdin Wallet`.
3. Memórias voláteis que contenham material criptográfico sensível devem ser explicitamente limpas com escrita volátil (*zeroize*) imediatamente após o uso.
4. Nenhuma syscall existente ou futura do Jinn OS aceitará chaves privadas como argumento.

## 3. Consequências

- **Positivas:** Mesmo em caso de eventual falha ou exploit em um driver do kernel, as sementes do usuário permanecem protegidas pela cifra em disco e isolamento de processo. O microkernel preserva sua pureza minimalista.
- **Negativas:** Requer um daemon adicional de userspace para interagir com o stack de sockets.
