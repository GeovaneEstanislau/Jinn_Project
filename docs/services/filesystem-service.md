# Filesystem Service — Jinn OS

## 1. Visão Geral

O `Filesystem Service` provê abstrações de armazenamento e acesso a arquivos para serviços e aplicações no Jinn. Implementado como um serviço em espaço de usuário, ele oferece VFS, pontos de montagem, namespace, contratos de QoS, journaling e integrações com `Cache Manager`, `Driver Manager` e `Predictive Engine`.

## 2. Objetivos

- Fornecer uma camada unificada de arquivos (VFS) que orquestra backends locais e remotos.
- Suportar desempenho (latência/throughput), consistência configurável e recuperação após falhas.
- Expor contratos de SLA para caches, prefetch e replicação.
- Isolar dados por tenant/serviço com capabilities.

## 3. Responsabilidades

- Resolver caminhos, gerenciar mounts e namespaces.
- Mapear requests de I/O para drivers via `Driver Manager` e para caches via `Cache Manager`.
- Implementar políticas de consistência (strong/ eventual) e journaling/crash-recovery.
- Expor APIs IPC para operações de arquivo e control-plane (mount/unmount/config).

## 4. Arquitetura Interna

- `VFS Layer`: dispatcher de chamadas e abstração de inode/vnode.
- `Backend Adapters`: drivers de filesystem (ext-like, object store, network FS).
- `Transaction Log / Journal`: persistência de metadados e operações críticas.
- `Lease & Lock Manager`: gerencia locks distribuídos e leases locais.
- `Consistency Engine`: aplica políticas (sync/async, replication).

Topo:

  [Clients/Services] -> [IPC Gateway] -> [VFS Layer]
                                  |-> [Cache Manager]
                                  |-> [Backend Adapters] -> [Driver Manager / Storage Service]
                                  |-> [Transaction Log]
                                  |-> [Predictive Engine]

## 5. Estruturas de Dados

- `Superblock { id, fs_type, flags, block_size, root_inode }`
- `Inode { ino, mode, owner, size, blocks, atime, mtime, ctime, caps }`
- `Dentry { name, parent, inode }`
- `FileHandle { fh_id, inode, pos, flags, lease }`
- `JournalEntry { tx_id, ops[], checksum, ts }`

## 6. Fluxo de Funcionamento

1. `open(path, flags)`
   - Resolver `dentry` → `inode` via VFS cache.
   - Checar capabilities e quotas via `Security Core` / `Process Supervisor`.
   - Se necessário, solicitar dados ao `Cache Manager`; em miss, fetch via backend adapter.

2. `read/write`
   - Leitura: serve de `Cache Manager` quando possível; fallback para backend e atualiza cache.
   - Escrita: acumula em journal (modo transacional) e aplica sincronização conforme política (sync/async).

3. Mount/Unmount
   - Validar manifesto de mount (source, options), aplicar namespaces para requester, notificar `Driver Manager` se necessário.

## 7. Interfaces Públicas (IPC)

- `fs_open(path, flags) -> FileHandle`.
- `fs_read(fh, offset, size) -> data`.
- `fs_write(fh, offset, data) -> bytes_written`.
- `fs_sync(fh)` — força flush de journal para durabilidade.
- `fs_mount(source, target, opts)` — mountar backend.
- `fs_stat(path)` — metadados.
- `fs_subscribe_changes(path)` — notificações de mudança.

Access control: todas as chamadas exigem `CapabilityToken` apropriado; operações sensíveis (mount, format) exigem privilégios administrativos.

## 8. Integração com outros componentes

- `Cache Manager`: leitura/escrita preferencial para pools, warming prévia via Predictive Engine.
- `Driver Manager` / `Storage Service`: backend físico lógico e mapeamento de dispositivos.
- `Predictive Engine`: prefetch hints, warming de blocos ou inodes quentes, e previsão de hotspots.
- `Memory Core`: alocação de páginas para buffers e DMA pools para transferências.
- `Security Core`: validação de capabilities e políticas de isolamento.

## 9. Segurança

- Isolamento por namespace/tenant: cada serviço opera em seu conjunto de mounts e permissões.
- Assinatura de images/manifestos de filesystems montáveis.
- Proteções contra TOCTOU: leases e locks para operações críticas.
- Proteção de dados em trânsito e em repouso via integração com serviços de criptografia (user-space crypto service).

## 10. Escalabilidade

- Arquitetura shardable: particionar namespace por tenant ou prefixo.
- Cache local por nó com fallback para cache global.
- Replicação assíncrona com políticas configuráveis para diferentes diretórios/trees.

## 11. Futuras Evoluções

- Filesystem distribuído nativo com multi-master e conflict-resolution política (CRDT/operational transforms) para workloads colaborativos.
- Tiering automático entre RAM, NVRAM e backing storage com políticas ditadas pelo Predictive Engine.
- Plugins Wasm para operations extensíveis em sandbox.

## 12. Comparação com sistemas modernos

- Linux VFS: Jinn mantém VFS, mas extracção do backend para serviço user-space permite isolamento e atualizações dinâmicas.
- Network filesystems (NFS/Ceph): Jinn pode compor backends object-store e network FS, com integração nativa ao Cache Manager e Predictive Engine para melhor latência.

## 13. Pseudocódigo

Open/Read/Write (simplificado):

```pseudo
fs_open(path, flags, requester):
  if not security.check_cap(requester, path, OPEN):
    return PERMISSION_DENIED
  d = vfs.resolve(path)
  inode = inode_cache.get_or_load(d)
  fh = new_filehandle(inode, flags)
  return fh

fs_read(fh, offset, size):
  pool = cache.select_pool(fh.owner)
  if pool.contains(fh.inode, offset, size):
    return pool.read(fh.inode, offset, size)
  else:
    data = backend.read(fh.inode, offset, size)
    cache.put(fh.inode, offset, data)
    return data

fs_write(fh, offset, data):
  tx = journal.start_tx()
  tx.append(write_op(fh.inode, offset, data))
  if fsync_mode == SYNC:
    tx.commit()
  else:
    journal.commit_deferred(tx)
  cache.invalidate_range(fh.inode, offset, len(data))
  return len(data)

```

## 14. Diagramas ASCII

Request path:

  [Client] -> [IPC Gateway] -> [VFS] -> [Cache Manager]
                                     |-> [Backend Adapter] -> [Driver/Storage Service]
                                     |-> [Journal]

Mount sequence:

  [Admin] -> [fs_mount] -> [Security Core validate] -> [Driver Manager probe] -> [VFS attach]

## 15. Considerações para implementação em Rust

- Organização em crates:
  - `jinn-fs-core` (VFS, resolution, mounts)
  - `jinn-fs-backends` (adapters para backends locais e remotos)
  - `jinn-fs-journal` (transaction log, replay, recovery)
  - `jinn-fs-locks` (leases, distributed locks)

- Boas práticas:
  - Usar `serde`/schemas para manifests e IPC messages; preferir Cap'n Proto/Flatbuffers para hot-path IPC.
  - Minimizar `unsafe` e isolar operações que tocam buffers físicos (DMA) em módulos auditáveis.
  - Testes de integridade de journal e recovery automatizados.
  - Perfis de build: modos `debug` mais conservadores, `release` com otimizações e features ML desligadas por padrão.

---

Arquivo: [Filesystem Service](./filesystem-service.md)
