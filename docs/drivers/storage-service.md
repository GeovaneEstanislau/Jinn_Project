# Storage Service — Jinn OS

## 1. Visão Geral

O `Storage Service` provê abstração, gerenciamento e acesso a dispositivos de armazenamento (bloco e object) no Jinn. Roda em espaço de usuário e oferece interfaces seguras a serviços, integração com `Driver Manager`, `Filesystem Service`, `Cache Manager` e `Predictive Engine`.

## 2. Objetivos

- Gerenciar dispositivos de bloco e object, I/O paths e políticas de durabilidade.
- Expor operações seguras e de alto desempenho para serviços e filesystems.
- Coordenar políticas de replicação, tiering e snapshots.

## 3. Responsabilidades

- Abstração de dispositivos (logical volumes, namespaces).
- Agendamento de I/O com QoS e prioridades.
- Snapshotting, replication, encryption-at-rest integration.
- Fornecer métricas para Predictive Engine e hooks para prefetch/warm.

## 4. Arquitetura Interna

- `Device Registry`: mantém metadados de dispositivos e capacidades.
- `IO Scheduler`: politicas de merge, reorder e QoS.
- `Snapshot/Replication Manager`: cria snapshots, replicação assíncrona/ síncrona.
- `Encryption Manager`: integra com crypto services para keys e cifragem.

Topo:

  [Filesystem Service / Apps] -> [Storage API] -> [IO Scheduler] -> [Driver Manager]
                                           |-> [Snapshot Manager]
                                           |-> [Cache Manager]
                                           |-> [Predictive Engine]

## 5. Estruturas de Dados

- `Device { id, type, capacity, block_size, flags, mappings }`
- `IORequest { id, type, device, lba, len, qos, owner }`
- `Snapshot { id, device_id, base, delta_refs, ts }`
- `ReplicationChannel { target, mode, state }`

## 6. Fluxo de Funcionamento

1. I/O path
   - Request chega via IPC; scheduler valida capability e enfileira.
   - Scheduler aplica QoS, coalesces e despacha ao Driver Manager / device.

2. Snapshot
   - Criar snapshot: checkpoint metadata, redirecionamento de writes para delta.

3. Replication
   - Streaming de writes para canal de réplica; aplicar ack semantics conforme modo.

## 7. Interfaces Públicas

- `storage_open(device_id, caps)`
- `storage_read(req)` / `storage_write(req)`
- `storage_snapshot(device_id)`
- `storage_replicate(device_id, target, mode)`
- `storage_get_stats(device_id)`

## 8. Integração com outros componentes

- Driver Manager: acesso ao device node e capacidade de mapear I/O.
- Filesystem Service: backend lógico para operações de arquivo.
- Cache Manager: caching de blocos quentes e policies de eviction.
- Predictive Engine: warming, prefetch de blocos antecipados.

## 9. Segurança

- Access control via capabilities para operações e dispositivos.
- Separação de tenants via namespaces e encryption keys por tenant.
- Proteção de metadados e journaling com assinaturas e replay protection.

## 10. Escalabilidade

- Sharding de devices e distribuindo replication channels.
- Multi-queue I/O e per-CPU submission para throughput.

## 11. Futuras Evoluções

- Tiering automático entre SSD, NVRAM e object storage.
- Integration with NVMe-oF and RDMA for low-latency remote access.

## 12. Comparação com sistemas modernos

- Linux md/LSM/LVM: Jinn implementa layers em user-space com capabilities e integração com Predictive Engine para políticas dinâmicas.

## 13. Pseudocódigo

```pseudo
on_write(req):
  if not security.authorized(req.owner, WRITE):
    return DENIED
  q = ioscheduler.select_queue(req.device, req.qos)
  q.enqueue(req)

ioscheduler.dispatch():
  req = q.pop()
  if cache.has(req):
    perform_cache_write(req)
  else:
    driver.send(req)

```

## 14. Diagramas ASCII

I/O flow:

  [Client] -> [Storage Service API] -> [IO Scheduler] -> [Cache] -> [Driver Manager]
                                                          |
                                                    [Predictive Engine]

## 15. Considerações para implementação em Rust

- Crates sugeridos: `jinn-storage-core`, `jinn-storage-scheduler`, `jinn-storage-replication`.
- Isolar unsafe para áreas que fazem DMA ou interagem com driver binaries.
- Testes de desempenho e garantia de latência para SLAs.

---

Arquivo: [Storage Service](./storage-service.md)
