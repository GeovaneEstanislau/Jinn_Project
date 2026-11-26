# JDA Compute & DePIN — Economia Computacional Distribuída

**Documento:** Arquitetura de Infraestrutura Física Descentralizada (DePIN) e Mercados de Processamento  
**Projeto:** Jinn Project  
**Subsistema:** JDA Engine (Jinn Dataflow Architecture) & Simbad OS  
**Versão:** 1.0.0  
**Data:** Novembro de 2026  
**Status:** Especificação Arquitetural  

---

## 1. Visão Geral e Rejeição à Mineração Especulativa

A infraestrutura **DePIN** do Jinn Project conecta estações de trabalho domésticas ociosas (**Aladdin Desktop**) e servidores de alto desempenho (**Simbad OS**) em uma malha de processamento e armazenamento distribuídos através do motor **JDA (Jinn Dataflow Architecture)**.

### Princípio Fundamental:
> **O ecossistema Jinn rejeita terminantemente a mineração especulativa por força bruta (Proof-of-Work). Não há queima inútil de eletricidade.**  
> O sistema recompensa **exclusivamente trabalho computacional útil, verificável e consumido por tarefas reais** (renderização gráfica, compilação de código, inferência de modelos de IA, simulações científicas e custódia segura de blocos de dados).

---

## 2. Diferenciação de Métricas de Recursos

Para evitar fraudes de "nós fantasmas" ou nós que apenas fingem disponibilidade, o protocolo divide a contabilidade em três métricas estritamente segregadas:

```
┌────────────────────────────────────────────────────────────────────────┐
│                   MÉTRICAS DE RECURSOS NO PROTOCOLO JDA                │
├──────────────────────────┬───────────────────────┬─────────────────────┤
│ 1. Capacidade Oferecida  │ 2. Capacidade Consumida│ 3. Execução Validada│
│ (Declared / Heartbeat)   │ (Active Cycle Meter)  │ (Proof of Execution)│
├──────────────────────────┼───────────────────────┼─────────────────────┤
│ Quantidade de núcleos    │ Ciclos de CPU e RAM   │ Resultado conferido │
│ SMP, VRAM e NVMe livres  │ efetivamente alocados │ criptograficamente  │
│ declarados pelo nó.      │ durante a execução.   │ por desafio/quórum. │
│ ❌ NÃO GERA RECOMPENSA!  │ [!] MÉTRICA DE MEDIÇÃO │ ✅ GERA RECOMPENSA! │
└──────────────────────────┴───────────────────────┴─────────────────────┘
```

Nenhum nó recebe tokens $JINN apenas por manter a máquina ligada sem processar dados válidos.

---

## 3. Identificação e Autenticação dos Nós

1. **Identidade Criptográfica do Nó (NodeID):**
   - Cada nó gera um par de chaves assimétricas em userspace (Ring 3).
   - O `NodeID` é o endereço hexadecimal correspondente à chave pública.
2. **Attestation de Hardware:**
   - Durante o handshake de ingresso na rede, o nó executa um microbenchmark determinístico de CPUID e criptografia, assinando os parâmetros com sua chave privada.
3. **Isolamento de Tarefas no Host:**
   - O microkernel Jinn utiliza anéis de proteção (Ring 3), tabelas de página independentes (PML4) e cotas de prioridade no escalonador preemptivo (`src/scheduler.rs`) para garantir que uma tarefa distribuída não consiga acessar os dados pessoais do hospedeiro, nem monopolizar os recursos locais da máquina.

---

## 4. Verificação de Execução (Proof of Execution - PoE)

Para impedir que nós maliciosos entreguem resultados corrompidos ou falsificados para cobrar recompensas:

1. **Execução Determinística:**  
   Tarefas distribuídas compiladas para o JDA Engine operam com sementes pseudoaleatórias fixas e aritmética IEEE-754 previsível.
2. **Amostragem Redundante (Spot-Checking):**  
   Uma fração configurável de tarefas (ex: 5% a 10%) é enviada aleatoriamente para dois nós distintos de forma cega. Se os hashes dos resultados finais divergirem, ambos os nós entram em disputa.
3. **Penalidade de Reputação e Slash de Staking:**  
   Nós provedores que submetem resultados falsificados perdem reputação no protocolo e têm seus slots de prioridade revogados. Nós operadores que realizam staking de garantia têm uma parcela queimada.

---

## 5. Liquidação Econômica em $JINN

- **Contratantes de Computação:** Desenvolvedores e empresas pagam em $JINN por tarefas computacionais no JDA Dataflow (ex: compilação de grandes sistemas em paralelo com múltiplos nós Simbad).
- **Provedores de Computação:** Recebem 90% do valor da tarefa liquidado diretamente em sua carteira no Aladdin Desktop ou Simbad Server.
- **Taxa de Deflação do Protocolo:** 10% do valor de cada tarefa é recolhido pelo contrato; uma parcela é destinada ao cofre de desenvolvimento e outra é queimada permanentemente (`_burn`), conferindo pressão deflacionária ao token $JINN à medida que o uso do sistema operacional cresce.
