# Guia Prático de Implantação e Validação em Testnet (BSC Chapel)

**Projeto:** Jinn OS & Aladdin Desktop Multiverse  
**Ativo:** $JINN (Jinn Network Token)  
**Ambiente:** BNB Smart Chain Testnet (Chain ID: 97 — Chapel)  
**Versão:** 1.0.0  
**Data:** Novembro de 2026  

---

## 1. Pré-Requisitos e Ferramental

Para realizar a implantação experimental na rede de testes da BSC, você precisará de:

1. **Ambiente com Node.js:** Node.js v18+ ou v20+ e gerenciador de pacotes NPM.
2. **Carteira de Teste com tBNB:**
   - Obtenha tBNB (BNB de teste sem valor financeiro) na torneira oficial (*faucet*):
     - [BNB Chain Testnet Faucet](https://testnet.binance.org/faucet-smart) ou Discord oficial da BNB Chain.
   - Guarde a chave privada da carteira de testes em local seguro. **Nunca utilize sua chave privada principal de produção.**
3. **Chave de API do BSCScan:**
   - Crie uma conta gratuita em [BscScan.com](https://bscscan.com) para obter uma chave de API usada na verificação do código-fonte.

---

## 2. Configuração do Arquivo de Ambiente

Na pasta `contracts/`, copie o template `.env.example` para `.env`:

```bash
cd contracts
cp .env.example .env
```

Edite o arquivo `.env`:

```env
# Chave privada da sua carteira de testes (com saldo de tBNB)
DEPLOYER_PRIVATE_KEY=0x_sua_chave_privada_de_testnet_aqui

# Endereço RPC oficial da BSC Testnet
BSC_TESTNET_RPC=https://data-seed-prebsc-1-s1.binance.org:8545/

# Chave de API para verificar o contrato no BSCScan Testnet
BSCSCAN_API_KEY=sua_chave_bscscan_aqui

# Endereço da Tesouraria / Multi-Sig (ou da própria carteira de deploy em testes)
TREASURY_SAFE_ADDRESS=0x_endereco_da_carteira_de_testes
```

---

## 3. Compilação e Testes Locais

Antes de qualquer interação com a rede, certifique-se de que os testes automatizados locais estão passando:

```bash
# Instalação das dependências
npm install

# Compilação dos smart contracts
npm run compile

# Execução da suíte completa de testes de invariantes
npm test
```

Também execute a ferramenta em Rust para validar a integridade dos 6 invariantes:

```bash
cargo run --manifest-path ../tools/jinn-web3/Cargo.toml -- invariants
```

---

## 4. Executando o Deploy na BSC Testnet

Execute o script automatizado de implantação:

```bash
npm run deploy:testnet
```

O console exibirá o progresso detalhado:

```text
=================================================
   Deploying Jinn Network Token ($JINN) System    
=================================================
Deployer account: 0x71C634C26B331E0cd51a04579997424946344955
Account balance: 1.45 BNB

Target Treasury / Multi-Sig Receiver: 0x71C634C26B331E0cd51a04579997424946344955

[1/3] Deploying JinnToken...
-> JinnToken deployed at: 0x25aB829c36d07d1B73b88b488F35b2e98782E780
   Total Supply: 1000000000000000000000000000 (1,000,000,000 $JINN)

[2/3] Deploying JinnVesting Vault...
-> JinnVesting deployed at: 0x8a92B45C11D348E19c3684B29E35b2e98782E781

[3/3] Deploying JinnRewardDistributor...
-> JinnRewardDistributor deployed at: 0x3F2bA78c22E459F28d4795C30E46b2e98782E782
=================================================
```

---

## 5. Verificação do Código-Fonte no BscScan Testnet

Para que os contratos fiquem auditáveis e verificados publicamente no explorador de blocos:

```bash
npx hardhat verify --network bscTestnet <ENDEREÇO_JINN_TOKEN> "<ENDEREÇO_TESOURARIA>"
npx hardhat verify --network bscTestnet <ENDEREÇO_JINN_VESTING> "<ENDEREÇO_JINN_TOKEN>"
npx hardhat verify --network bscTestnet <ENDEREÇO_JINN_DISTRIBUTOR> "<ENDEREÇO_JINN_TOKEN>"
```

---

## 6. Testando as Operações em Testnet

### 6.1. Simular uma Transferência com a CLI Rust
Use o utilitário `tools/jinn-web3` para gerar o calldata e testar o RPC:

```bash
cargo run --manifest-path ../tools/jinn-web3/Cargo.toml -- encode
```

### 6.2. Importar o Token no MetaMask / Trust Wallet (Testnet)
- **Rede:** BNB Smart Chain Testnet
- **RPC URL:** `https://data-seed-prebsc-1-s1.binance.org:8545/`
- **Chain ID:** `97`
- **Símbolo:** `tBNB`
- **Endereço do Contrato do Token:** Cole o endereço gerado pelo `deploy:testnet`.
- O saldo exibirá seus tokens $JINN de teste.
