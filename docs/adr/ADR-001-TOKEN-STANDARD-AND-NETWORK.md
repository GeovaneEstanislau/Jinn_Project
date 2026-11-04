# ADR-001: Seleção de Rede Blockchain e Padrão de Token

**Status:** Aprovado  
**Data:** Novembro de 2026  
**Decisores:** Conselho de Arquitetura do Jinn Project  

---

## 1. Contexto

O Jinn Project necessita de um ativo digital ($JINN) para sustentar a pesquisa do microkernel, financiar recompensas de código (PoUC) e alimentar microsserviços no Aladdin Desktop. A escolha da rede hospedeira deve garantir viabilidade econômica para transações pequenas e integração confiável com código em Rust e clientes userspace.

## 2. Decisão

Adotamos a **BNB Smart Chain (BSC)** e o padrão **BEP-20** (com extensões OpenZeppelin v5, EIP-2612 Permit, Pausable e Ownable2Step).

### Justificativas Técnicas:
1. **Custo de Transação:** Taxas médias de rede inferiores a \$0.03 por transação, tornando viável a liquidação de micropagamentos e resgates de bounties sem que o gás supere o valor do incentivo.
2. **Tempo de Confirmação:** Tempo médio de bloco de ~3 segundos, proporcionando experiência ágil e responsiva na interface gráfica do Aladdin Desktop.
3. **Compatibilidade EVM Integral:** Permite reaproveitar tooling maduro de smart contracts, carteiras de hardware e clientes leves em Rust.
4. **Ambiente Testnet Acessível:** A testnet pública da BSC (Chapel) oferece um ambiente estável para simulações completas sem custo de capital.

## 3. Consequências

- **Positivas:** Baixa fricção para desenvolvedores e usuários; facilidade de auditoria dos contratos; infraestrutura de testes abundante.
- **Riscos e Mitigações:** Centralização do conjunto de validadores da BSC. *Mitigação:* Contratos desenhados com compatibilidade total EVM, permitindo migração ou pontes (bridges) futuras para Ethereum L2s ou Rollups caso a governança técnica decida.
