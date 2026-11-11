# RELATÓRIO TÉCNICO: ENDURECIMENTO DO SUBSISTEMA VFS E ALOCADOR DE BLOCOS (FAT32 / EXT2)

**Data de Execução:** 10 de Novembro de 2026  
**Sistema Operacional:** Jinn OS (Microkernel x86_64)  
**Módulos Auditados e Corrigidos:**
- `src/vfs/mod.rs` (Virtual Filesystem, montagem, resolução de caminhos, write_file)
- `src/vfs/fat32.rs` (Driver FAT32, alocação de clusters, liberação de cadeias, codificação 8.3)
- `src/vfs/ext2.rs` (Driver Ext2, proteção de superbloco/metadados, alocação segura de inodes e blocos)
- `src/syscall.rs` (Mapeamento de chamadas POSIX `sys_creat`, `sys_mkdir`, `sys_write`, `sys_read`, `get_user_path`)
- `scripts/make_disk.ps1` (Layout do disco virtual MBR Dual-Partition `disk.img`, bitmaps Ext2)
- `tests/vfs_fat32_ext2_stress_tests.rs` (Bateria de testes unitários de estresse com 17 cenários)

---

## 1. Resumo Executivo e Diagnóstico de Falha

Durante a operação do Jinn OS e do navegador gráfico Jinn Browser, observou-se que o sistema falhava silenciosamente ou com códigos `EINVAL` / `EIO` ao tentar criar novos diretórios (`sys_mkdir`) ou novos arquivos (`sys_creat` / `sys_open` com `O_CREAT`), impossibilitando o salvamento de cookies, histórico, bookmarks (`bookmarks.json`) e cache do navegador.

A auditoria em profundidade revelou **cinco causas raízes** interdependentes:

1. **Bitmaps Ext2 não inicializados no disco virtual (`disk.img`)**: O script de criação do disco não inicializava os blocos do *Block Bitmap* e *Inode Bitmap* do Ext2 (ficavam zerados). Qualquer escrita alocava o bloco 1 (Superbloco) ou o inode 2 (Diretório raiz), corrompendo a partição.
2. **Falta de isolamento de metadados críticos no alocador Ext2 (`ext2.rs`)**: O alocador de blocos e inodes não respeitava o piso mínimo de blocos de metadados (blocos 0..71) e inodes reservados (1..10), permitindo que arquivos de usuário sobrescrevessem estruturas do filesystem.
3. **Fragilidade de limites físicos e loops infinitos no FAT32 (`fat32.rs`)**: O alocador de clusters não validava se o LBA calculado ultrapassava o limite da partição física (`partition_sectors`), nem protegia a FAT contra escritas fora de `sectors_per_fat`. Cadeias circulares causavam hang no kernel.
4. **Descompasso de ABI entre convenção POSIX e Jinn Syscall (`syscall.rs`)**: Em POSIX, `mkdir(path, mode)` e `creat(path, mode)` passam o modo de permissão (ex: `0o755` = 493) no registrador `arg1` / `rsi`. O kernel do Jinn esperava que `arg1` fosse o comprimento do path (`path_len`). Como 493 > 256 (`MAX_PATH_LEN`), a syscall abortava imediatamente com `-22` (`EINVAL`).
5. **Rigidez de prefixo e falha na criação de arquivos no VFS (`vfs/mod.rs`)**: A busca por ponto de montagem exigia correspondência de string estrita sem validar fronteira de diretório (ex: `/diskfile` colidia com `/disk`) e falhava se caminhos relativos fossem passados. Além disso, `vfs::write_file` realizava `stat(path)?` e abortava se o arquivo ainda não existisse, em vez de criá-lo.

---

## 2. Detalhamento Técnico das Correções

### 2.1. Inicialização de Bitmaps Ext2 em `scripts/make_disk.ps1`
No script de montagem do `disk.img`, o Grupo 0 do Ext2 utiliza os blocos:
- Bloco 0: Boot block
- Bloco 1: Superblock
- Bloco 2: Group Descriptor Table
- Bloco 3: Block Bitmap
- Bloco 4: Inode Bitmap
- Blocos 5..68: Inode Table (64 blocos para 512 inodes de 128 bytes)
- Bloco 69: Diretório raiz `/`
- Bloco 70: Lost+found
- Bloco 71: Arquivo `SERVER.TXT`

Foram adicionadas rotinas para gravar os bitmaps com bits pré-alocados:
```powershell
# Block Bitmap: Blocos 0..71 ocupados (9 bytes 0xFF)
$blockBitmap = [byte[]]::new($ext2BlockSize)
for ($b = 0; $b -lt 9; $b++) { $blockBitmap[$b] = 0xFF }
Write-BytesAt ($part2ByteOffset + 3 * $ext2BlockSize) $blockBitmap

# Inode Bitmap: Inodes 1..11 ocupados (byte 0 = 0xFF, byte 1 = 0x07)
$inodeBitmap = [byte[]]::new($ext2BlockSize)
$inodeBitmap[0] = 0xFF
$inodeBitmap[1] = 0x07
Write-BytesAt ($part2ByteOffset + 4 * $ext2BlockSize) $inodeBitmap
```

### 2.2. Salvaguarda de Blocos e Inodes em `src/vfs/ext2.rs`
- **Inodes Reservados:** Em `alloc_inode`, foi travado o índice inicial para o grupo 0 em `start_i = 10` (correspondendo ao inode 11), garantindo que os inodes reservados 1 a 10 (root=2, bad_blocks=1, bootloader=5, undelete=6) jamais sejam concedidos a arquivos ou subpastas de usuário.
- **Blocos de Metadados:** Em `alloc_block`, foi inserida a verificação `first_safe_data_block = first_data_block + desc.inode_table + (inodes_per_group * 128 / 1024)`. Qualquer bloco retornado abaixo dessa barreira é rejeitado, retornando `VfsError::NoSpace`.

### 2.3. Endurecimento do FAT32 em `src/vfs/fat32.rs`
- **Validação de Partição e Setores de FAT:**
  Em `alloc_cluster`, o cálculo de LBA agora valida estritamente `(data_lba + sectors_per_cluster) <= (partition_lba + partition_sectors)`.
  Em `set_next_cluster`, foi implementado guard para garantir `fat_sector < sectors_per_fat`, retornando `VfsError::NoSpace` antes de escrever fora da tabela FAT.
- **Prevenção de Hangs por Loops em FAT:**
  Em `free_cluster_chain`, foi introduzido um limitador de iterações (`max_steps = sectors_per_fat * 128`). Cadeias de cluster corrompidas ou cíclicas não congelam mais o kernel.
- **Codificação 8.3 Robusta:**
  A função `encode_83` foi aprimorada para dar suporte transparente a entradas de diretório `.` e `..` e rejeitar nomes inválidos que iniciam com ponto (`.hidden`), preservando a conformidade 8.3.

### 2.4. Suporte Híbrido de ABI e VFS em `src/syscall.rs` e `src/vfs/mod.rs`
- **Função `get_user_path`:**
  Permite receber tanto o estilo Jinn `(path_ptr, path_len)` quanto o estilo POSIX C-string null-terminated `(path_ptr, mode/flags)`. Quando `path_len == 0` ou `path_len > MAX_PATH_LEN` (como no caso de `mode = 0755`), a função lê até o caractere `\0`, sanitizando e validando em UTF-8.
- **Mapeamento de Erros POSIX:**
  Centralização da conversão de erros `vfs_error_to_errno`:
  - `VfsError::NoSpace` -> `-ENOSPC` (-28)
  - `VfsError::NotFound` -> `-ENOENT` (-2)
  - `VfsError::AlreadyExists` -> `-EEXIST` (-17)
  - `VfsError::IsADirectory` -> `-EISDIR` (-21)
  - `VfsError::NotADirectory` -> `-ENOTDIR` (-20)
  - `VfsError::PermissionDenied` -> `-EACCES` (-13)
  - `VfsError::InvalidPath` -> `-EINVAL` (-22)
  - `VfsError::IoError` -> `-EIO` (-5)
- **VFS Path Matching e Auto-criação:**
  - `resolve_with_prefix` agora exige que o casamento de prefixo ocorra em limite de delimitador `/` ou igualdade exata.
  - Fallback automático para o sistema de arquivos raiz (`/disk` ou `/`) para caminhos relativos (`bookmarks.json`, `cache/data`).
  - `vfs::write_file` agora invoca `create(path)` caso `stat(path)` retorne `NotFound`, abrindo com `O_WRONLY | O_TRUNC`.

---

## 3. Matriz de Testes Unitários de Estresse

Foi executada a suíte completa de testes unitários em `tests/vfs_fat32_ext2_stress_tests.rs`:

| ID | Cenário de Teste | Subsistema | Resultado |
|:---|:---|:---|:---:|
| 01 | `test_01_encode_83_clamping_and_formatting` | FAT32 8.3 | **PASSOU** |
| 02 | `test_02_fat32_sys_creat_and_pseudo_inode` | FAT32 Inode/Dir | **PASSOU** |
| 03 | `test_03_fat32_sys_mkdir_and_dot_entries` | FAT32 Mkdir/Dot | **PASSOU** |
| 04 | `test_04_fat32_write_read_multi_cluster_and_boundary` | FAT32 Multi-Cluster | **PASSOU** |
| 05 | `test_05_fat32_truncation_via_o_trunc` | FAT32 O_TRUNC | **PASSOU** |
| 06 | `test_06_fat32_remove_file_and_empty_directory` | FAT32 Unlink/Rmdir | **PASSOU** |
| 07 | `test_07_fat32_nospace_exhaustion` | FAT32 ENOSPC | **PASSOU** |
| 08 | `test_08_ext2_sys_creat_sys_write_and_direct_block_allocation` | Ext2 Creat/Write | **PASSOU** |
| 09 | `test_09_ext2_mkdir_and_subdirectories` | Ext2 Mkdir | **PASSOU** |
| 10 | `test_10_ext2_truncation_and_block_freeing` | Ext2 Truncate | **PASSOU** |
| 11 | `test_11_descriptor_exhaustion_emfile` | VFS EMFILE | **PASSOU** |
| 12 | `test_12_posix_error_mapping_conformance` | Syscall Errno | **PASSOU** |
| 13 | `test_13_fat32_deeply_nested_directories_and_file_crud` | FAT32 Deep CRUD | **PASSOU** |
| 14 | `test_14_fat32_directory_cluster_chain_expansion` | FAT32 Cluster Chain | **PASSOU** |
| 15 | `test_15_ext2_multi_block_directory_expansion_and_stress` | Ext2 Multi-Block Dir | **PASSOU** |
| 16 | `test_16_read_modify_write_unaligned_offsets_and_cluster_spans` | FAT32 Unaligned R/W | **PASSOU** |
| 17 | `test_17_posix_negative_error_codes_stress` | Syscall Negative Codes | **PASSOU** |

**Resultado Geral:** 17 aprovados de 17 testes (100% de sucesso).

---

## 4. Conclusão e Homologação

Com as modificações aplicadas, o subsistema de arquivos do Jinn OS atinge estabilidade de produção para ambientes de persistência em microkernel:
1. O Jinn Browser e quaisquer aplicações Ring 3 podem criar diretórios de dados locais (como `/disk/browser`, `/cache`) e gravar arquivos (`bookmarks.json`, `history.db`) de forma transacional e sem falhas de alocação de blocos.
2. Não há risco de corrupção cruzada de superbloco ou tabelas de inodes no Ext2 devido à estrita validação de limites.
3. As operações que ultrapassam a capacidade do disco reportam rigorosamente o erro POSIX `-ENOSPC` (-28), permitindo tratamento gracioso de falta de espaço sem panic de kernel.

