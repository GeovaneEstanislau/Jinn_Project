# ADR-003: Proof of Useful Contribution (PoUC) e Distribuição por Merkle Tree

**Status:** Aprovado  
**Data:** Novembro de 2026  
**Decisores:** Conselho de Arquitetura e Governança do Jinn Project  

---

## 1. Contexto

Projetos de código aberto frequentemente enfrentam dificuldades para incentivar desenvolvedores sem cair no erro de premiar commits superficiais, "line-counting" ou gerar spam de PRs. Além disso, pagar cada contribuidor diretamente na blockchain via transferências individuais incorre em custos operacionais e de gás desnecessários.

## 2. Decisão

Adotamos o mecanismo **Proof of Useful Contribution (PoUC)** com agregação mensal em **Árvores de Merkle (Merkle Trees)** gerenciadas pelo contrato `JinnRewardDistributor.sol`.

### Diretrizes:
1. Avaliação baseada em 7 critérios ponderados (Utilidade, Complexidade, Qualidade, Impacto, Manutenibilidade, Segurança e Revisão).
2. Human-in-the-Loop: Apenas PRs mesclados e com CLA assinado são pontuados por mantenedores credenciados.
3. Eficiência de Gás: Em vez de dezenas de transações on-chain por mês, os mantenedores publicam uma única raiz de Merkle (`merkleRoot`) por época. Cada desenvolvedor resgata seus tokens de forma descentralizada submetendo sua prova de inclusão criptográfica (`claimReward`).
4. Proteção contra Replay: Mapeamento de índices garante que nenhuma recompensa possa ser retirada mais de uma vez.
5. Prazos e Expiração: Campanhas possuem vigência definida e fundos não reivindicados podem ser restituídos transparentemente à tesouraria após o encerramento.

## 3. Consequências

- **Positivas:** Máxima transparência; incentivo focado no mérito técnico real; economia brutal de taxas de gás on-chain.
- **Negativas:** Exige processo de revisão manual disciplinado por parte dos mantenedores antes de cada fechamento de época.
