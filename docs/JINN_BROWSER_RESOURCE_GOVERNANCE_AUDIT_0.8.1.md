# JINN BROWSER 0.8.1 — RESOURCE GOVERNANCE AUDIT & QUOTA ENFORCEMENT

**Documento:** `docs/JINN_BROWSER_RESOURCE_GOVERNANCE_AUDIT_0.8.1.md`  
**Classificação:** Auditoria de Governança de Recursos, Perfis de Memória e Algoritmos de Despejo  
**Versão:** 0.8.1  
**Data:** 05/11/2026  
**Status:** IMPLEMENTED [V] | UNIT_TESTED [V] | INTEGRATION_TESTED [V]

---

## 1. O Gerenciador `BrowserResourceManager`

O gerenciador de recursos [`BrowserResourceManager`](file:///e:/Projetos/Jinn/browser/jinn-browser/src/tab.rs) foi concebido para atuar como o elo de ligação entre as necessidades da aplicação web e os limites físicos impostos pelo sistema operacional.

Suas responsabilidades são:
1. Impor cotas globais de memória baseadas no hardware disponível.
2. Monitorar a memória física total residente consumida por todas as abas.
3. Acionar automaticamente o algoritmo de despejo LRU (*Least Recently Used*) sob pressão.
4. Preservar incondicionalmente a integridade da aba ativa em primeiro plano.

---

## 2. Perfis de Memória e Quotas de Hardware

O Jinn Browser 0.8.1 define quatro perfis operacionais:

| Perfil | Limite Global de RAM | Quota Máxima por Aba | Máx. Abas Ativas | Limiar de Despejo (Discard Threshold) |
| :--- | :---: | :---: | :---: | :---: |
| **Profile64MB** | 16 MiB ($16.777.216$ B) | 2 MiB ($2.097.152$ B) | 2 | 12 MiB ($12.582.912$ B) |
| **Profile128MB** | 32 MiB ($33.554.432$ B) | 4 MiB ($4.194.304$ B) | 4 | 24 MiB ($25.165.824$ B) |
| **Profile256MB** (Padrão) | 64 MiB ($67.108.864$ B) | 8 MiB ($8.388.608$ B) | 8 | 48 MiB ($50.331.648$ B) |
| **Profile512MB** | 128 MiB ($134.217.728$ B)| 16 MiB ($16.777.216$ B)| 16 | 96 MiB ($100.663.296$ B) |

---

## 3. Algoritmo de Despejo e Enforce sob Pressão

Quando a soma das memórias residentes de todas as abas excede o `discard_threshold_bytes` do perfil:
1. O método `evaluate_and_enforce` varre a lista de abas.
2. A aba ativa (`active_id`) é categoricamente protegida de descarte.
3. Abas em `TabState::Hibernated` são transicionadas para `TabState::Discarded`, liberando seu snapshot e metadados não essenciais.
4. O consumo total é recalculado de forma atômica e $O(1)$ por aba descartada:
   $$\text{total\_resident} \leftarrow \text{total\_resident} - (\text{before} - \text{after})$$
5. O loop cessa assim que o consumo total retorna abaixo da margem de segurança do limiar.

---

## 4. Avaliação de Conformidade

- **Sobrecarga de CPU do Governança:** Inexistente. A avaliação é orientada a eventos (quando abas são criadas, navegadas ou trocadas).
- **Proteção do Kernel:** Impede que o processo atinja `max_frames` ou cause pânico por `ENOMEM` no alocador do Ring 0.
- **Restauração Transparente:** Abas descartadas preservam URL e título, permitindo recarga sob demanda quando o usuário focar novamente nelas.

