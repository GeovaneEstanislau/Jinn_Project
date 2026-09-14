# Cache Manager — Jinn OS

## 1. Visão Geral

O `Cache Manager` é um serviço central do Jinn responsável por gerenciar caches compartilhados entre serviços e drivers, implementar políticas de warming/eviction, fornecer telemetria ao Predictive Engine e ofertar contratos de cache (SLA) aos serviços.

Ele atua como uma camada entre Memory Core, Predictive Engine e serviços consumidores, oferecendo pools especializados (hot cache, warm cache, ephemeral cache) e APIs para requisição/compartilhamento de dados em memória.

## 2. Objetivos

- Reduzir latência de acesso a dados críticos via caching cooperativo.
- Expor contratos de QoS (hit-rate, latency) por serviço.
- Integrar com Predictive Engine para warming e prefetch.
- Minimizar custo quando o cache não é utilizado.

## 3. Responsabilidades

- Provisionar e gerir pools de cache (per-service, global, per-tenant).
- Garantir coerência básica entre versões de dados compartilhados.
- Expor métricas e hints para o Predictive Engine.
- Enforce quotas e políticas de isolamento entre serviços.

## 4. Arquitetura Interna

- `Cache Pools`: estrutura física de páginas/objetos alocados via Memory Core.
- `Policy Layer`: plug-ins para eviction (LRU, LFU, cost-aware), warming e prefetch.
- `Telemetry Collector`: sumariza métricas de acesso e fornece para Predictive Engine.
- `API Gateway`: IPC endpoints para operações de cache (get/put/invalidate/subscribe).

Topologia:

  [Services] -> [API Gateway] -> [Cache Manager]
                                |-> [Cache Pools] <-> [Memory Core]
                                |-> [Telemetry] -> [Predictive Engine]

## 5. Estruturas de Dados

- `CacheEntry { key, version, size, last_access, score }`
- `CachePool { id, capacity_pages, policy, allocations }`
- `CacheHandle { pool_id, lease, owner }`

Interno (Rust-like):

```rust
struct CachePool {
  id: Uuid,
  capacity_pages: usize,
  policy: Box<dyn EvictionPolicy>,
  map: HashMap<Key, CacheEntry>,
}
```

## 6. Fluxo de Funcionamento

1. Serviço requisita cache com contrato (size, latency_target).
2. Cache Manager verifica quotas e aloca um `CacheHandle` em pool apropriado.
3. Leitura `get(key)` retorna dado se presente; em miss, opcionalmente ativa prefetch ou retorna miss para que serviço busque e injete.
4. Eviction ocorre conforme política e telemetria.

## 7. Interfaces Públicas

- `cache_acquire(contract)` — cria/assinala handle.
- `cache_get(handle, key)` — retorna objeto ou miss.
- `cache_put(handle, key, data, ttl)` — insere dados.
- `cache_invalidate(key)` — invalida entrada globalmente.
- `cache_subscribe(key)` — notificação on-change.

Todas as chamadas via IPC devem fornecer `CapabilityToken` apropriado; políticas podem exigir assinaturas para operações críticas.

## 8. Integração com outros componentes

- Memory Core: alocação física para pools (Cache Pool).
- Predictive Engine: warming hints, prefetch commands e análise de padrões.
- Scheduler: afinidade para serviços sensíveis à latência de cache.
- Filesystem/Network Service: produtores comuns de dados a serem cacheados.

## 9. Segurança

- Isolamento de pools por capability e tenant.
- Evitar leaks de dados entre tenants com ACLs e capacidades.
- Controle de acesso em nível de chave/namespace.

## 10. Escalabilidade

- Pools distribuídos: sharding por chave, replicação eventual entre nós.
- Cache local per-node para reduzir latência e fallback para cache global.

## 11. Futuras Evoluções

- Modelos de prefetch baseados em ML pelo Predictive Engine.
- Cache hierárquico entre RAM e NVRAM com políticas de tiering.

## 12. Comparação com sistemas modernos

- Semelhante a sistemas de cache distribuído (Memcached/Redis) porém com contratos de QoS, integração profunda com Memory Core e capacidade de warming controlado pelo Predictive Engine.

## 13. Pseudocódigo

```pseudo
acquire(contract):
  pool = select_pool(contract)
  if pool.has_capacity(contract.size):
    return pool.allocate_handle(contract)
  else:
    evict_from_pool(pool, contract.size)
    return pool.allocate_handle(contract)

get(handle, key):
  entry = pool.map.get(key)
  if entry:
    telemetry.record_hit(handle, key)
    return entry.data
  else:
    telemetry.record_miss(handle, key)
    if predictive.hint_prefetch(key):
      start_prefetch(key)
    return miss
```

## 14. Diagramas ASCII

Cache flow:

  [Service] -> [API Gateway] -> [Cache Pool] -> [Memory Core]
                       |
                 [Telemetry] -> [Predictive Engine]

## 15. Considerações para implementação em Rust

- Crates sugeridos: `jinn-cache-core`, `jinn-cache-policy`, `jinn-cache-telemetry`.
- Evitar blocking syscalls na hot-path; usar lock-free/read-copy-update para lookup.
- Serialização segura para IPC (Cap'n Proto/Flatbuffers).
- Testes de integridade de política e benchmarks para eviction/warming.

---

Arquivo: [Cache Manager](./cache-manager.md)
