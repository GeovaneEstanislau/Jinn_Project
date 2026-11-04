# ADR-006: Daemon Userspace `jinn-web3d` e Primitivas de Protocolo IPC

**Status:** Aprovado  
**Data:** 2026-11-04  
**Contexto:** Jinn Web3 Phase II — jinn-web3d Daemon & Aladdin Wallet Integration  
**Decisores:** Jinn Core Maintainers, Userspace & Architecture Leads  

---

## 1. Contexto

Para garantir que o Jinn OS funcione de forma 100% autônoma e independente de redes blockchain, nenhuma chamada de rede externa ou consulta RPC deve ser executada pelo microkernel ou pelo processo de inicialização do sistema operacional.

A interface gráfica do usuário (Aladdin Wallet) necessita de um serviço mediador em Ring 3 para:
- Conectar-se a nós RPC da BNB Smart Chain (BSC Testnet 97 / Mainnet 56).
- Preparar calldata de contratos BEP-20 (`balanceOf`, `transfer`, `approve`, `burn`).
- Gerenciar o histórico e status de transações.
- Fornecer respostas rápidas e desacopladas à interface via mensagens IPC.

## 2. Decisão Arquitetural

1. **Processo Autônomo em Ring 3:**
   - Criação do binário `jinn-web3d` em espaço de usuário.
   - O daemon é um serviço desacoplado; caso o daemon seja interrompido ou a rede externa esteja inacessível, o desktop Aladdin continua funcionando sem degradação.

2. **Opcodes do Protocolo IPC (`Web3IpcOpcode`):**
   - `1 = GetStatus`: Consulta status do daemon, rede ativa e contrato $JINN.
   - `2 = GetBalance`: Consulta saldo BEP-20 e BNB nativo.
   - `3 = EstimateGas`: Estimativa de taxas para transações.
   - `4 = PrepareTransfer`: Montagem segura de calldata BEP-20 e criação de registro de rascunho de transação.
   - `5 = SignAndSend`: Recebimento de transação assinada pela carteira Ring 3 e transmissão via `eth_sendRawTransaction`.
   - `6 = ClaimReward`: Processamento de requisições de resgate de bounty PoUC.

3. **Segurança de Logs e Isolamento de Chaves:**
   - O daemon nunca solicita e nunca recebe a chave privada do usuário. A assinatura ocorre internamente no processo da carteira.
   - Registros de eventos (`DaemonEventLog`) são estritamente sanitizados: apenas PIDs, opcodes, timestamps, endereços e hashes públicos são registrados.

## 3. Consequências

- **Independência Tecnológica:** O microkernel mantém sua pureza minimalista.
- **Resiliência:** Falhas em endpoints RPC resultam em códigos de erro IPC (`STATUS_ERR_RPC_UNAVAILABLE`) tratados elegantemente pela interface do Aladdin Wallet.
