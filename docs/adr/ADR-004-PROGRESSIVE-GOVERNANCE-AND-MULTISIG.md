# ADR-004: Governança Progressiva e Custódia Multi-Sig da Tesouraria

**Status:** Aprovado  
**Data:** Novembro de 2026  
**Decisores:** Conselho de Arquitetura e Governança do Jinn Project  

---

## 1. Contexto

A gestão de 25% do suprimento de $JINN alocado à tesouraria do projeto e dos parâmetros administrativos dos smart contracts exige salvaguardas rigorosas contra apropriação indevida, conluio ou perda de chaves privadas.

## 2. Decisão

Implementamos uma governança estruturada em **Descentralização Progressiva**, iniciando pela custódia via **Gnosis Safe Multi-Sig (mínimo 3 de 5 assinaturas)** com **Timelock obrigatório de 48 horas**.

### Diretrizes:
1. Rejeição a proclamações falsas de descentralização plena enquanto o protocolo estiver em estágio inicial.
2. Composição diversificada de signatários (arquitetos do kernel, líderes do desktop, infraestrutura, comunidade e segurança).
3. Transações de tesouraria requerem período de anúncio público (48 horas) antes de poderem ser liquidadas, garantindo tempo de reação da comunidade.
4. Transição documentada para votações no Snapshot (Fase 2) e contratos de DAO soberana (Fase 3) à medida que o sistema operacional ganhar maturidade.

## 3. Consequências

- **Positivas:** Elimina risco de ponto único de falha (*Single Point of Failure*); protege o tesouro do projeto contra hacks ou extorsão; fornece clareza para a comunidade e parceiros institucionais.
- **Negativas:** Exige coordenação prévia de pelo menos 3 signatários para aprovar liberações de fundos.
