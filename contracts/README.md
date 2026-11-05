# Jinn Token Smart Contracts ($JINN)

Repositório de contratos inteligentes do ecossistema **Jinn OS**, projetados para a rede **BNB Smart Chain (BSC)** compatível com EVM.

---

## 📜 Contratos Inclusos

1. **`contracts/JinnToken.sol`**:
   - Padrão **BEP-20** integral com compatibilidade ERC-20 e método `getOwner()`.
   - **Teto Fixo (Hard Cap):** 1.000.000.000 $JINN (1 bilhão de tokens, sem possibilidade de emissão extra após deploy).
   - **Queima (Deflationary Burn):** Funções `burn` e `burnFrom` integradas para deflação e consumo de serviços.
   - **Permit (EIP-2612):** Aprovações sem gás baseadas em assinaturas ECDSA off-chain.
   - **Circuito de Segurança (Pausable):** Pausa emergencial controlada por governança.
   - **Governança Segura (Ownable2Step):** Transferência de posse em duas fases para evitar perda de controle.

2. **`contracts/JinnVesting.sol`**:
   - Cofre de liberação linear com carência (*cliff*) para a Tesouraria e Mantenedores Centrais.
   - Cálculo determinístico e suporte a cronogramas revogáveis e irrevogáveis.

3. **`contracts/JinnRewardDistributor.sol`**:
   - Módulo de recompensas por contribuição (*Proof of Useful Contribution - PoUC*).
   - Suporta resgates descentralizados via árvores de Merkle ou distribuição em lote para contribuidores do kernel e do desktop.

---

## 🚀 Como Compilar e Testar

### 1. Pré-requisitos
- Node.js >= 18.x
- NPM ou Yarn

### 2. Instalação de Dependências
```bash
npm install
```

### 3. Compilação
```bash
npm run compile
```

### 4. Execução da Bateria de Testes
```bash
npm test
```

### 5. Configuração de Variáveis de Ambiente
Copie o arquivo de exemplo e preencha suas chaves de teste:
```bash
cp .env.example .env
```

### 6. Deploy na BSC Testnet (Chapel)
```bash
npm run deploy:testnet
```

### 7. Verificação de Código no BscScan
```bash
npx hardhat verify --network bscTestnet <ENDEREÇO_DO_CONTRATO> <ARGUMENTOS_CONSTRUTOR>
```

