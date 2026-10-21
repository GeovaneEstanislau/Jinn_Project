# Jinn Storage Architecture (JSA) — Evolução, Contratos e Viabilidade de Sistemas de Arquivos
## Relatório de Engenharia e Documentação Técnica v0.2

**Data:** 20/10/2026  
**Sistema:** Jinn OS (x86-64 Microkernel, `#![no_std]`, Rust)  
**Status:** Implementado, Auditado e Validado via Suíte de Testes Automatizados

---

## 1. Resumo Executivo e Auditoria de Premissas

Em cumprimento à auditoria de conformidade entre a especificação teórica e o código real do repositório, identificamos e sanamos 4 lacunas críticas de segurança e integridade no subsistema de armazenamento:

1. **Evicção de Blocos Sujos sem Perda de Dados (LRU Dirty Eviction):** O método `BlockCache::insert` anterior substituía slots pelo algoritmo LRU sobrescrevendo `slot.data` mesmo quando `slot.is_dirty == true`. Sob perfis com política `WriteBack` (Simbad / Workstation), isso causava perda silenciosa de dados. Foi implementado o método `insert_evicting`, que força o descarregamento físico do setor vítima antes de reutilizar o slot.
2. **Desacoplamento dos Drivers de Sistemas de Arquivos:** Os módulos `fat32.rs` e `ext2.rs` realizavam chamadas diretas às portas de I/O de baixo nível (`ide::read_sector` e `ide::write_sector`). Foram completamente desacoplados, roteando todas as operações através de `cached_read_sector` e `cached_write_sector`, integrando-se à camada `CachedBlockDevice` e herdando o cache LRU transparente.
3. **Proteção Estrita de Limites de Partição:** O driver FAT32 e o driver Ext2 agora armazenam `partition_sectors: u64` e validam todos os acessos LBA em tempo de execução via `read_sec` e `write_sec`. Tentativas de leitura ou gravação além dos limites do volume são abortadas com erro imediato.
4. **Contrato Formal do VFS e Rejeição Precoce de Escrita:** A trait `FileSystem` foi expandida com `capabilities() -> FsCapabilities`, `sync() -> VfsResult<()>` e `unmount() -> VfsResult<()>`. O VFS (`src/vfs/mod.rs`) agora valida permissões antes de qualquer operação: se flags de escrita (`O_WRONLY`, `O_RDWR`, `O_CREAT`, `O_TRUNC`, `O_APPEND`) forem passadas para um sistema somente-leitura (como o Ext2), a operação é rejeitada no nível do kernel com `Err(VfsError::PermissionDenied)`.

---

## 2. Contrato Unificado de Sistemas de Arquivos

### 2.1. A Estrutura `FsCapabilities`
Cada implementação de sistema de arquivos declara suas capacidades em tempo de compilação:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FsCapabilities {
    pub read_only: bool,
    pub supports_create: bool,
    pub supports_remove: bool,
    pub supports_write: bool,
    pub supports_directories: bool,
    pub supports_journaling: bool,
    pub supports_permissions: bool,
    pub max_file_size: u64,
    pub name: &'static str,
}
```

### 2.2. A Trait `FileSystem` Expandida
A interface abstrata do VFS unifica todos os sistemas de arquivos sob um contrato rigoroso:

```rust
pub trait FileSystem: Send + Sync {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> FsCapabilities;
    fn sync(&self) -> VfsResult<()>;
    fn unmount(&self) -> VfsResult<()>;

    fn lookup(&self, path: &str) -> VfsResult<Stat>;
    fn open(&self, path: &str, flags: u32) -> VfsResult<u64>;
    fn close(&self, handle: u64) -> VfsResult<()>;
    fn read(&self, handle: u64, offset: u64, buf: &mut [u8]) -> VfsResult<usize>;
    fn write(&self, handle: u64, offset: u64, buf: &[u8]) -> VfsResult<usize>;
    fn create(&self, path: &str) -> VfsResult<()>;
    fn remove(&self, path: &str) -> VfsResult<()>;
    fn readdir(&self, path: &str, cb: &mut dyn FnMut(&str, NodeType));
}
```

### 2.3. Ciclo de Vida do Volume
```mermaid
stateDiagram-v2
    [*] --> Descoberta: Deteccao MBR / GPT
    Descoberta --> Montagem: vfs::mount(prefix, fs)
    Montagem --> Operacao: Validacao de Assinatura & BPB/Superblock
    
    state Operacao {
        [*] --> Leitura
        Leitura --> Escrita: Se !read_only
        Escrita --> CacheLRU: WriteThrough ou WriteBack
        CacheLRU --> Sync: vfs::sync_all() ou Eviccao
    }
    
    Operacao --> Desmontagem: vfs::unmount(prefix)
    Desmontagem --> SincronizacaoObrigatoria: fs.sync() + flush cache
    SincronizacaoObrigatoria --> FechamentoDescritores: Invalida handles ativos
    FechamentoDescritores --> DesregistradoVFS: Remove da MountTable
    DesregistradoVFS --> [*]
```

---

## 3. Matriz Comparativa de Sistemas de Arquivos

Avaliamos a viabilidade técnica, consumo de recursos e alinhamento estratégico com os perfis do Jinn OS:

| Formato | Estado no Jinn OS | Complexidade | Pegada de RAM | Resiliência / Falha | Limite de Arquivo | Perfil Alvo Primário | Parecer Técnico |
|:---|:---|:---|:---|:---|:---|:---|:---|
| **FAT32** | Total (RW completo com clusters e cache LRU) | Baixa (~900 linhas) | < 32 KiB | Nula (sem diário; risco na FAT) | 4 GiB | Compatibilidade / EFI / Boot | Mantido como padrão de interoperabilidade imediata. |
| **Ext2** | Total (Read-Only validado e protegido) | Média (~500 linhas) | < 64 KiB | Nula (requer fsck offline) | 2 TiB | Workstation / Linux Test | Base sólida para leitura. Escrita suspensa até suporte a transações. |
| **NTFS** | Planejado (Próxima fase: Read-Only MFT) | Alta (> 3.500 linhas) | ~256 KiB | Média (leitura segura de metadados) | 16 TiB | Aladdin (dual-boot com Windows) | Avaliado como leitor estritamente somente leitura de partição Windows. |
| **exFAT** | Em análise | Média (~1.800 linhas) | < 128 KiB | Baixa (sem diário tradicional) | 16 EiB | Aladdin (mídias removíveis > 4GB) | Excelente alternativa ao FAT32 para pendrives e SD cards domésticos. |
| **Btrfs** | Em pesquisa | Extrema (> 25.000 linhas) | > 8 MiB (árvores B+ CoW) | Máxima (CoW transacional, checksum) | 16 EiB | Aladdin (sistema principal futuro) | Requer microkernel maduro com paginação sob demanda avançada. |
| **ZFS** | Avaliado | Extrema (> 60.000 linhas) | > 128 MiB (ARC cache, pools SPA) | Máxima (Merkle trees, auto-cura) | 16 EiB | Simbad (servidores corporativos) | Recomenda-se implementação como daemon isolado em Ring 3 via IPC. |

---

## 4. Arquitetura de Políticas e Perfis Adaptativos

O Jinn OS não fragmenta a base do kernel para suportar perfis distintos. Todas as especializações operam na camada de abstração `StorageProfile`:

```
                       ┌───────────────────────────────┐
                       │     Jinn Storage Profiles     │
                       └───────────────┬───────────────┘
            ┌──────────────────┬───────┴──────────┬──────────────────┐
            ▼                  ▼                  ▼                  ▼
     ┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐
     │   Aladdin   │    │   Simbad    │    │ Workstation │    │Terminal/POS │
     │  (Doméstico)│    │ (Servidor)  │    │(Desenvolv.) │    │ (Dedicado)  │
     ├─────────────┤    ├─────────────┤    ├─────────────┤    ├─────────────┤
     │Cache: 64 blk│    │Cache:512 blk│    │Cache:256 blk│    │Cache: 16 blk│
     │WriteThrough │    │WriteBack    │    │WriteBack    │    │WriteThrough │
     │Readahead: 8 │    │Readahead: 32│    │Readahead: 16│    │Readahead: 2 │
     │Foco: Usabil.│    │Foco: IOPS/Tx│    │Foco: Compil.│    │Foco: Imunid.│
     └─────────────┘    └─────────────┘    └─────────────┘    └─────────────┘
```

### Transição Segura entre Políticas:
Quando o comando `storage <perfil>` é emitido no shell, o método `set_storage_profile`:
1. Verifica se a política atual é `WriteBack` e a nova política é `WriteThrough`.
2. Em caso positivo, executa um **flush imediato e obrigatório** de todos os blocos marcados como `is_dirty` no `PRIMARY_CACHED_DEV`.
3. Atualiza os parâmetros atômicos de capacidade, alocação e read-ahead sem causar descontinuidades de leitura ou corrupção de ponteiros.

---

## 5. Integração com a Jinn Dataflow Architecture (JDA)

A JDA otimiza a movimentação de dados e o agendamento de tarefas em grafos acíclicos dirigidos (DAGs). No contexto do subsistema de armazenamento, a integração segue princípios determinísticos de consistência:

1. **Write Barriers Invioláveis:** O escalonador da JDA jamais reordena nós de gravação de metadados antes de blocos de dados. Em operações de criação de arquivos no FAT32, a alocação de clusters na FAT e a escrita de dados precedem estritamente a entrada de diretório no disco.
2. **Transferências Zero-Copy:** Páginas alocadas pelo PMM para buffers de I/O em disco são mapeadas diretamente nas tarefas da JDA, evitando cópias intermediárias em Ring 0.
3. **Isolamento de Erros:** Se um nó do grafo sofrer falha de I/O no dispositivo de bloco, a JDA executa cancelamento em cascata (*cascading cancellation*) dos nós dependentes, sem deixar blocos sujos órfãos na cache.

---

## 6. Verificação e Suíte de Testes Automatizados

Foi desenvolvida uma suíte de testes de estresse em `tools/jda-sim/tests/storage_contract_tests.rs`:

1. `test_lru_cache_dirty_eviction_prevents_data_loss`: Comprovou que, em cache com 2 slots operando em `WriteBack`, a inserção do 3º bloco força a gravação física do bloco modificado no disco mock antes da sobreposição.
2. `test_partition_bounds_protection`: Validou que tentativas de acesso além do `sector_count` da partição são barradas com erro.
3. `test_vfs_readonly_flag_rejection`: Comprovou que o VFS bloqueia todas as variações de escrita em sistemas declarados como `read_only`.
4. `test_profile_transition_flush`: Validou que transições de perfil de `WriteBack` para `WriteThrough` descarregam a totalidade dos blocos sujos.
5. `test_ext2_corrupted_directory_record_safety`: Simulou ataques de metadados com `rec_len == 0` e ponteiros estourados, demonstrando terminação segura sem loops infinitos ou falhas de segmentação.
6. `test_storage_profile_policy_matrix`: Confirmou os invariantes das quatro configurações de perfil do sistema.

**Resultado:** 14/14 testes aprovados (8 testes de DAG/JDA + 6 testes de contrato JSA), compilação do kernel limpa com 0 avisos e 0 erros.

---

## 7. Novos Comandos Disponíveis no Shell

O operador do sistema conta com ferramentas de diagnóstico em tempo real no console:

- `fsinfo [caminho]`: Inspeciona o driver, modo de acesso (RW ou RO), suporte a criação/remoção, suporte a hierarquia de diretórios e tamanho máximo de arquivo do sistema montado.
- `umount <prefix>`: Desmonta o sistema de arquivos registrado no prefixo (ex: `/disk`, `/ext2`), garantindo a sincronização prévia de metadados e blocos sujos.
- `sync`: Aciona `vfs::sync_all()`, descarregando tanto os caches dos sistemas de arquivos quanto a cache física subjacente de blocos.
- `df`: Exibe o relatório de capacidade, utilização e estado de todos os pontos de montagem.

---

## 8. Limitações Conhecidas e Riscos Residuais

1. **Driver IDE ATA PIO:** A taxa de transferência atual é limitada pelo modo programado de E/S por portas (PIO LBA28, até 128 GiB por disco). A transição para DMA (Bus Master IDE) ou AHCI é necessária para saturação de banda.
2. **Ext2 Somente-Leitura:** A escrita em Ext2 permanece desativada até que o alocador de blocos por bitmap e o gerenciador de inodes livres sejam implementados com rollback em falhas de energia.
3. **NTFS e exFAT:** Não possuem drivers ativos nesta iteração; a arquitetura atual definiu a matriz de viabilidade e reservou as interfaces de montagem.

---

## 9. Próximo Incremento Recomendado

**Passo 1 — Implementação de Driver Somente-Leitura para NTFS (Fase 1):**
- Leitor do setor de boot NTFS ($Boot) e validação da assinatura `"NTFS    "`.
- Decodificador de registros da Master File Table (MFT, registros de 1024 bytes).
- Parser dos atributos essenciais: `$STANDARD_INFORMATION` (datas e permissões), `$FILE_NAME` (nomes em UTF-16) e `$DATA` (conteúdo residente e não residente via *data runs*).
- Montagem transparente de volumes Windows pré-instalados em `/ntfs` com garantia estrita de proteção `read_only: true`.
