# Relatório de Auditoria Arquitetural — Jinn Storage Architecture (JSA)
**Data:** 20 de Outubro de 2026  
**Status do Repositório:** Branch `main` | Commit base `4e7ec9e` | Árvore de trabalho limpa  
**Público-Alvo:** Engenharia de Sistemas Operacionais Jinn OS

---

## 1. Visão Geral e Contexto da Auditoria

O presente documento constitui a entrega formal da **Fase 0 (Auditoria Obrigatória)** estipulada pela Missão de Engenharia da **Jinn Storage Architecture (JSA)**. O objetivo é estabelecer a fundação técnica, auditar os módulos de baixo nível reais do repositório, mapear lacunas e dependências, analisar riscos de segurança e orientar a evolução modular da pilha de armazenamento do Jinn OS para atender aos perfis **Aladdin** (doméstico), **Simbad** (servidor), **Workstation** (engenharia/desenvolvimento) e **Terminal/POS** (embarcado/transacional).

---

## 2. Inventário do Estado Real da Arquitetura

A auditoria inspecionou diretamente os arquivos-fonte do kernel `#![no_std]` em Rust x86_64, constatando a seguinte correspondência entre especificação e código:

### 2.1. Camada de Controladores Físicos de Armazenamento
- **Arquivo:** `src/drivers/ide.rs` (377 linhas).
- **Status Real:** **Implementado e Operacional em Hardware/Emulador.**
- **Mecanismos Confirmados:**
  - Identificação de unidades ATA via comando `IDENTIFY DRIVE (0xEC)` no barramento primário (`0x1F0..0x1F7` e `0x3F6`).
  - Leitura de setores (`read_sector`) via ATA PIO LBA28 com polling de registradores `BSY` e `DRQ`.
  - Escrita de setores (`write_sector`) via comando ATA `0x30` (`WRITE SECTORS`) com ciclo de atraso de porta (400ns), transmissão de 256 palavras de 16 bits e emissão de `FLUSH CACHE (0xE7)`.
  - Detecção automática de discos IDE (`detect_disks`) e decodificação de MBR (`read_mbr`).
- **Limitações:**
  - Modo estritamente PIO (Programmed I/O): consome ciclos síncronos de CPU (polling contínuo); não há suporte a Bus Master DMA / PRDT ainda.
  - Endereçamento limitado a LBA28 (máximo de $2^{28} \times 512 = 128\text{ GiB}$ de capacidade de disco; LBA48 ainda não implementado).

### 2.2. Abstração de Dispositivos de Blocos & Cache
- **Arquivo:** `src/drivers/block.rs` (284 linhas).
- **Status Real:** **Implementado e Funcional.**
- **Mecanismos Confirmados:**
  - `trait BlockDevice`: define os contratos `name()`, `block_size()`, `total_blocks()`, `read_blocks()`, `write_blocks()` e `flush()`.
  - `IdeBlockDevice`: encapsula o canal IDE primário sob a interface `BlockDevice` com verificação de limites de LBA.
  - `BlockCache`: buffer LRU de 16 slots (8 KB de RAM para setores de 512B) com rastreamento de contadores de acesso `clock` e substituição do elemento menos recentemente acessado.
  - `CachedBlockDevice<D>`: composição que intercepta leituras e escritas através do cache, monitorando atomicamente `hits` e `misses` com estratégia *write-through*.
- **Limitações:**
  - Número de slots fixo em 16 (8 KB).
  - Apenas política *write-through* síncrona; sem suporte a *write-back* com descarregamento assíncrono em segundo plano.

### 2.3. Sistema Virtual de Arquivos (VFS)
- **Arquivo:** `src/vfs/mod.rs` (245 linhas).
- **Status Real:** **Implementado e Operacional.**
- **Mecanismos Confirmados:**
  - `trait FileSystem`: `lookup`, `open`, `close`, `read`, `write`, `create`, `remove`, `readdir`.
  - `MountTable`: tabela global protegida por `SpinLock` com suporte a até 8 pontos de montagem (`MAX_MOUNTS = 8`).
  - Resolução de caminhos hierárquicos pelo método de maior prefixo coincidente (*longest match*).
  - Funções utilitárias de alto nível: `read_file(path, buf)` e `write_file(path, buf)`.
- **Limitações:**
  - Não há tabela de descritores de arquivos por processo (*file descriptor table* no PCB do `Task`).
  - Não suporta links simbólicos (*symlinks*) nem travamento concorrente fino por arquivo (*file locking*).

### 2.4. Driver de Sistema de Arquivos FAT32
- **Arquivo:** `src/vfs/fat32.rs` (565 linhas).
- **Status Real:** **Implementado (Leitura Total e Escrita In-Place).**
- **Mecanismos Confirmados:**
  - Leitura do setor de boot (BPB): bytes por setor (512), setores por cluster, setores reservados, tabelas FAT e cluster raiz.
  - Navegação da tabela FAT32 (`get_next_cluster`) com detecção de EOF (`0x0FFF_FFF8`).
  - Leitura e busca de diretórios (`for_each_entry`, `find_file`) com decodificação de nomes 8.3 e verificação de atributos (diretório, arquivo).
  - Montagem automática em `/disk` a partir da partição MBR detectada no boot (`src/main.rs`).
  - Escrita em arquivos existentes (`Fat32Fs::write`): preserva setores parciais via *read-modify-write* através de `ide::write_sector`.
- **Limitações:**
  - Escrita restrita a arquivos existentes pré-alocados; criação de novos arquivos (`create`) e alocação de novos clusters na FAT ainda não implementados.
  - Acesso direto a funções `ide::read_sector`/`ide::write_sector` em vez de desacoplamento pelo `BlockDevice`.

### 2.5. Gerenciamento de Memória, Paginação e IPC
- **Arquivos:** `src/memory/mod.rs`, `src/memory/pmm.rs`, `src/memory/vmm.rs`, `src/ipc.rs`.
- **Status Real:** **Operacional e Isolado.**
- **Mecanismos Confirmados:**
  - PMM com alocador de frames físicos de 4 KiB baseado em bitmap.
  - VMM x86_64 com paginação PML4 de 4 níveis ativa, separando espaço de kernel no *higher-half* (`0xFFFF_8000_0000_0000`) e espaço de usuário no *lower-half*.
  - IPC assíncrono via caixas de correio de 64 bytes por processo, com capacidade de transferir descritores Zero-Copy (`TAG_JDA_SHM_BUFFER`).

### 2.6. JDA (Jinn Dataflow Architecture)
- **Arquivos:** `src/jda/mod.rs`, `src/jda/shmem.rs`, `tools/jda-sim`.
- **Status Real:** **Operacional (Kernel SHM Pool + Simulador de Host com 8 Testes).**
- **Mecanismos Confirmados:**
  - Pool de buffers de páginas físicas compartilhadas (`SharedBufferPool`, 32 slots de 4 KiB) com contagem de referências.
  - `Syscall 12` (`sys_jda_shm_alloc`) e `Syscall 13` (`sys_jda_shm_release`) acessíveis pelo Ring 3.
  - Simulador de host com testes formais de detecção de ciclo em DAGs, ordenação topológica e determinismo.

---

## 3. Matriz de Distinção de Mecanismos de Memória e Armazenamento

Para prevenir ambiguidades conceituais e violações de abstração, a tabela abaixo formaliza os contratos distintos no Jinn OS:

| Mecanismo | Escopo | Unidade | Ciclo de Vida | Persistência | Módulo Responsável |
|:---|:---|:---|:---|:---|:---|
| **PMM (Physical Memory)** | Memória Física RAM | Página de 4 KiB | Boot até alocação/liberação | Volátil | `src/memory/pmm.rs` |
| **Kernel Heap** | RAM Virtual do Kernel | Bytes contíguos (Layout) | `alloc` até `dealloc` | Volátil | `src/memory/heap.rs` |
| **FS Block Allocation** | Disco Físico / Partição | Cluster (512B – 64 KiB) | Criação até remoção do arquivo | Não Volátil | Driver FS (ex: `fat32.rs`) |
| **Block Cache** | RAM do Kernel | Setor (512B / 4096B) | Amortizado por LRU | Volátil (Write-Through/Back) | `src/drivers/block.rs` |
| **File Cache / Page Cache** | Espaço Virtual de Arquivos | Página de 4 KiB | Evicção por pressão de memória | Volátil sincronizável | Planejado no VFS |
| **Memory Mapping (mmap)** | Tabela de Páginas do Processo | Mapeamento PML4 (4 KiB) | `mmap` até `munmap`/exit | Sincronizado sob demanda | Planejado no VMM |
| **JDA Zero-Copy SHM** | Pool Global Compartilhado | Frame Físico (4 KiB) | Reference Counting (Syscall 12/13) | Volátil (Handover de mensagens) | `src/jda/shmem.rs` |

---

## 4. Análise Comparativa de Sistemas de Arquivos para os Perfis do Jinn OS

| Critério | FAT32 | exFAT | ext4 | Btrfs | NTFS | ZFS / OpenZFS |
|:---|:---|:---|:---|:---|:---|:---|
| **Complexidade Relativa** | Baixa (~1.000 LoC) | Média (~2.500 LoC) | Alta (>15.000 LoC) | Extrema (>60.000 LoC) | Muito Alta (>35.000 LoC) | Extrema (>100.000 LoC) |
| **Integridade & Checksums** | Nenhuma | Nenhuma | Checksums de Journal | Checksums de Dados + Metadados (CRC32c/xxHash) | Journal de Metadados (USN) | Checksums fim-a-fim (Fletcher4/SHA256) |
| **Resiliência a Queda de Energia** | Frágil (risco de FAT órfã) | Baixa-Média | Alta (Journaling com barreiras) | Máxima (Copy-on-Write transacional) | Alta (Journal de metadados) | Máxima (Copy-on-Write transacional) |
| **Snapshots & Subvolumes** | Não suportado | Não suportado | Apenas via LVM | Nativos (árvore de raízes B-tree) | Apenas via VSS proprietário | Nativos (ZFS Pools / Datasets) |
| **Overhead de RAM Mínimo** | < 64 KiB | < 128 KiB | ~1–2 MiB (JBD2 buffer) | > 16–32 MiB (Árvores CoW ativas) | > 4–8 MiB (MFT cache) | > 64–128 MiB (ARC cache) |
| **Licenciamento** | Domínio público / Patentes expiradas | Especificação publicada / Requer cautela | GPLv2 (Reescrever em Rust livre) | GPLv2 (Reescrever ou portar) | Proprietário Microsoft (Read-Only seguro) | CDDL (Incompatível com distribuição direta) |
| **Adequação: Aladdin (Doméstico)** | Primordial (Boot / Mídia) | Excelente (USB > 4GB) | Candidato Intermediário | **Candidato Principal (Roadmap)** | Leitura de volumes Windows | Inadequado (consumo de RAM) |
| **Adequação: Simbad (Servidor)** | Inadequado | Inadequado | Suporte legado | Forte candidato | Inadequado | **Candidato Principal (Roadmap)** |
| **Adequação: Workstation** | Mídia externa | Pendrives | Excelente | Excelente | Leitura segura | Uso avançado |
| **Adequação: Terminal / POS** | Secundário | Inadequado | Modo sem journal | Inadequado (muito pesado) | Inadequado | Inadequado |

### Decisão Estratégica:
1. **Curto Prazo:** Consolidar FAT32 com abstração `BlockDevice`, suporte a partições estritas (`PartitionDevice`) e políticas de cache configuráveis por perfil.
2. **Médio Prazo:** Implementar um leitor/escritor Ext2/Ext4 simplificado sem journal ou com journal mínimo para estações de trabalho e perfil doméstico, além de um parser de leitura estritamente somente-leitura para NTFS (*safe read-only*).
3. **Longo Prazo:** Arquitetar suporte a Btrfs para o Aladdin (focado em subvolumes para rollback de sistema) e ZFS para o Simbad (focado em pools de integridade fim-a-fim).

---

## 5. Lacunas Técnicas e Riscos de Segurança Identificados

1. **Risco de Escrita Fora de Partição:**
   - Atualmente, escritas que usem LBA absoluto podem colidir com setores de outras partições se o chamador calcular incorretamente o deslocamento.
   - *Solução:* Criar a estrutura `PartitionDevice` que encapsula o `start_lba` e valida em tempo de execução que qualquer offset é estritamente menor que `sector_count`.
2. **Falta de Suporte a Partições GPT:**
   - Apenas MBR clássico de 4 entradas é decodificado. Discos modernos com particionamento GPT não têm suas partições reconhecidas.
   - *Solução:* Implementar parser para GPT Header (LBA 1, assinatura `"EFI PART"`) e GPT Partition Entries (128 bytes).
3. **Ausência de Perfis Configuráveis no Armazenamento:**
   - As políticas de cache estão fixas no código sem diferenciação entre uma máquina doméstica, servidor ou terminal POS.
   - *Solução:* Implementar enum `StorageProfile` e estrutura de políticas dinâmicas.

---

## 6. Plano de Implementação da Fase 1 e 3

1. **Passo 1 (Arquitetura de Partições e Volumes):**
   - Criar `src/drivers/partition.rs`:
     - Estruturas de partição unificadas (`PartitionInfo`).
     - Decodificadores robustos de MBR (LBA 0) e GPT (LBA 1).
     - Abstração de contenção segura `PartitionDevice<'a, D: BlockDevice>`.
2. **Passo 2 (Perfis de Armazenamento e Políticas de Cache):**
   - Em `src/drivers/block.rs`:
     - Definir `enum StorageProfile { Aladdin, Simbad, Workstation, TerminalPos }`.
     - Definir estratégias de escrita (`WriteStrategy::WriteThrough`, `WriteStrategy::WriteBack`).
     - Adicionar `flush()` com suporte real a descarregamento de dirty slots.
3. **Passo 3 (Interface e Comandos no Shell):**
   - Criar comando `storage` no shell (`src/shell.rs`) para inspecionar partições, chavear perfis em tempo de execução e verificar estatísticas de I/O.
4. **Passo 4 (Validação e Integração com JDA):**
   - Teste de transferência de blocos para buffers físicos `PhysicalPageBuffer` via Zero-Copy.
   - Verificação completa de compilação `#![no_std]`, testes do JDA e build da ISO.
