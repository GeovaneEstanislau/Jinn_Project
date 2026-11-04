# JINN OS — REFERÊNCIA CANÔNICA DA ABI E TABELA DE SYSCALLS 0.2.0
## Resolução Definitiva de Divergências, Especificação de Interfaces Ring 3 e Verificação de Colisões

**Documento:** Especificação Técnica da ABI e Chamadas de Sistema  
**Projeto:** Jinn Microkernel  
**Versão:** 0.2.0  
**Data:** Novembro de 2026  
**Status:** Auditado, Unificado e Validado  

---

## 1. Resolução da Divergência Documental (Syscalls 50, 51 e 52)

Durante auditorias históricas do ecossistema, identificou-se uma inconsistência em relatórios anteriores no tocante à numeração das chamadas de criação de diretório, exclusão e renomeação de arquivos. Documentos legados referiam-se alternadamente a `sys_rename` como Syscall 51 ou 50, gerando desalinhamento entre o espaço de usuário e a tabela de despacho do microkernel.

### Determinação Canônica e Imutável no Código Efetivo

A auditoria direta na tabela de despacho do kernel (`src/syscall.rs`, linhas 389..391) estabelece a correspondência canônica vinculante:

```rust
// Tabela de Despacho do Microkernel (src/syscall.rs):
50 => sys_unlink(arg0 as *const u8, arg1),
51 => sys_mkdir(arg0 as *const u8, arg1),
52 => sys_rename(arg0 as *const RenameArgs),
```

| Nº Syscall | Função Canônica | Assinatura no Kernel | Wrapper Ring 3 (`syscall_user.rs`) |
|---|---|---|---|
| **50** | `sys_unlink` | `sys_unlink(path_ptr: *const u8, path_len: usize) -> isize` | `pub unsafe fn sys_unlink(path: &str) -> i64` |
| **51** | `sys_mkdir` | `sys_mkdir(path_ptr: *const u8, path_len: usize) -> isize` | `pub unsafe fn sys_mkdir(path: &str) -> i64` |
| **52** | `sys_rename` | `sys_rename(args_ptr: *const RenameArgs) -> isize` | `pub unsafe fn sys_rename(old_path: &str, new_path: &str) -> i64` |

---

## 2. Convenção de Chamada da ABI (x86_64)

As chamadas de sistema no Jinn OS utilizam a interrupção por software `int 0x80`, com porta de entrada mediada pela IDT configurada com nível de privilégio DPL=3, permitindo acesso seguro a partir do Ring 3.

- **RAX:** Número identificador da Syscall (0..=52).
- **RDI:** Primeiro argumento (`arg0`).
- **RSI:** Segundo argumento (`arg1`).
- **RDX:** Terceiro argumento (`arg2`).
- **Retorno:** Registrador `RAX` (Valores `>= 0` indicam sucesso ou contagem de bytes; valores `< 0` representam códigos de erro POSIX `-errno`).

---

## 3. Tabela Canônica Completa de Syscalls (0..52)

| Nº | Nome | RDI (`arg0`) | RSI (`arg1`) | RDX (`arg2`) | Retorno (RAX) | Privilégio | Descrição |
|---|---|---|---|---|---|---|---|
| **0** | `sys_exit` | `code: i32` | — | — | Não retorna | Ring 0/3 | Encerra a tarefa atual; o kernel a coloca em estado Zombie e dispara preempção. |
| **1** | `sys_write` | `fd: u64` | `buf: *const u8` | `len: u64` | Bytes escritos / `-errno` | Ring 0/3 | Escreve em descritor de arquivo (1=stdout, 2=stderr, >=3 arquivos VFS ou sockets). |
| **2** | `sys_read` | `fd: u64` | `buf: *mut u8` | `len: u64` | Bytes lidos / `-errno` | Ring 0/3 | Lê bytes de arquivo VFS ou socket de rede. |
| **3** | `sys_yield` | — | — | — | `0` | Ring 0/3 | Cede voluntariamente o restante da fatia de tempo da CPU. |
| **4** | `sys_getpid` | — | — | — | `PID: u64` | Ring 0/3 | Retorna o identificador de processo da tarefa chamadora. |
| **5** | `sys_ipc_send` | `to_pid: u32` | `tag: u32` | `payload: *const u8` | `0` / `-errno` | Ring 0/3 | Envia mensagem IPC síncrona/assíncrona para outro processo. |
| **6** | `sys_ipc_recv` | `buf: *mut u8` | `len: u64` | — | Bytes lidos / `-errno` | Ring 0/3 | Coleta mensagem IPC da fila do processo. |
| **7** | `sys_alloc_page` | — | — | — | Endereço físico / `-errno` | Ring 0/3 | Aloca um frame de memória física de 4 KiB. |
| **8** | `sys_free_page` | `phys_addr: u64` | — | — | `0` / `-errno` | Ring 0/3 | Libera frame de memória física alocado anteriormente. |
| **9** | `sys_spawn` | `fn_ptr: usize` | `name_ptr: *const u8` | `ring: usize` | `PID` / `-errno` | Ring 0 | Cria uma nova tarefa kernel a partir de um ponteiro de função. |
| **10** | `sys_waitpid` | `pid: u64` | — | — | Código saída / `-errno` | Ring 0/3 | Bloqueia a tarefa chamadora até a terminação do processo filho `pid`. |
| **11** | `sys_sleep` | `ms: u64` | — | — | `0` / `-errno` | Ring 0/3 | Coloca a tarefa em dormência pelo intervalo em milissegundos. |
| **12** | `sys_jda_shm_alloc` | `is_ro: u64` | — | — | `handle: u64` / `-errno` | Ring 0/3 | Aloca buffer compartilhado na infraestrutura JDA. |
| **13** | `sys_jda_shm_release` | `handle: u64` | — | — | `0` / `-errno` | Ring 0/3 | Libera buffer compartilhado do JDA. |
| **14** | `sys_jda_submit` | `job_type: u64` | `handle: u64` | `len: u64` | Resultado / `-errno` | Ring 0/3 | Submete operação de computação em fluxo de dados JDA. |
| **15** | `sys_open` | `path: *const u8` | `path_len: usize` | `flags: u32` | `fd: i64` / `-errno` | Ring 0/3 | Abre arquivo VFS e aloca descritor unificado na tabela do processo. |
| **16** | `sys_close` | `fd: u64` | — | — | `0` / `-errno` | Ring 0/3 | Fecha descritor aberto (arquivo, socket ou dispositivo). |
| **17** | `sys_stat` | `path: *const u8` | `path_len: usize` | `stat: *mut FileStat` | `0` / `-errno` | Ring 0/3 | Consulta metadados de arquivo ou diretório por caminho. |
| **18** | `sys_fstat` | `fd: u64` | `stat: *mut FileStat` | — | `0` / `-errno` | Ring 0/3 | Consulta metadados a partir de um descritor aberto. |
| **19** | `sys_lseek` | `fd: u64` | `offset: i64` | `whence: usize` | Novo offset / `-errno` | Ring 0/3 | Reposiciona cursor de leitura/escrita em arquivo VFS. |
| **20** | `sys_window_create` | `title: *const u8` | `title_len: usize` | `(w << 16) \| h` | `win_id: i64` / `-errno` | Ring 0/3 | Cria janela gráfica no Aladdin Desktop. |
| **21** | `sys_window_update` | `win_id: usize` | `buf: *const u32` | `len_bytes: usize` | `0` / `-errno` | Ring 0/3 | Submete buffer de pixels para composição no desktop. |
| **22** | `sys_window_close` | `win_id: usize` | — | — | `0` / `-errno` | Ring 0/3 | Fecha janela no Aladdin e libera buffer de vídeo associado. |
| **23** | `sys_sync` | — | — | — | `0` / `-errno` | Ring 0/3 | Força o flush de todos os caches sujos do VFS para a mídia de disco física. |
| **24** | `sys_window_poll_event` | `win_id: usize` | `event: *mut Event` | `flags: usize` | `1` se evento, `0` s/ evento | Ring 0/3 | Coleta eventos de mouse e teclado direcionados à janela. |
| **25** | `sys_res_stat` | `pid: usize` | `stat: *mut ResStat` | `len: usize` | `0` / `-errno` | Ring 0/3 | Coleta telemetria e limites de recursos da tarefa `pid`. |
| **26** | `sys_jda_shm_share` | `handle: u64` | `target_pid: usize` | `make_ro: u64` | `0` / `-errno` | Ring 0/3 | Compartilha buffer de memória de forma imutável zero-copy. |
| **27** | `sys_jda_ring_submit` | `sqe: *const JdaSqe` | `count: usize` | — | Submetidos / `-errno` | Ring 0/3 | Enfileira requisições assíncronas no anel de E/S. |
| **28** | `sys_jda_ring_poll` | `cqe: *mut JdaCqe` | `max_count: usize` | — | Coletados / `-errno` | Ring 0/3 | Coleta conclusões assíncronas do anel de E/S. |
| **29** | `sys_jda_shm_get_vaddr`| `handle: u64` | — | — | Endereço virtual / `-errno`| Ring 0/3 | Mapeia e retorna o endereço virtual do buffer compartilhado. |
| **30** | `sys_socket` | `domain: u32` | `socket_type: u32` | `proto: u32` | `sockfd: i64` / `-errno` | Ring 0/3 | Cria um descritor de endpoint de rede (UDP/TCP). |
| **31** | `sys_connect` | `fd: u64` | `addr: *const u8` | `addr_len: usize` | `0` / `-errno` | Ring 0/3 | Conecta socket TCP ao endereço remoto. |
| **32** | `sys_bind` | `fd: u64` | `addr: *const u8` | `addr_len: usize` | `0` / `-errno` | Ring 0/3 | Associa socket a uma porta de rede local. |
| **33** | `sys_listen` | `fd: u64` | `backlog: usize` | — | `0` / `-errno` | Ring 0/3 | Coloca socket TCP em modo passivo de escuta. |
| **34** | `sys_accept` | `fd: u64` | `addr: *mut u8` | `len: usize` | Novo `fd` / `-errno` | Ring 0/3 | Aceita conexão TCP de entrada. |
| **35** | `sys_send` | `fd: u64` | `buf: *const u8` | `len: usize` | Bytes enviados / `-errno` | Ring 0/3 | Transmite payload através de socket de rede. |
| **36** | `sys_recv` | `fd: u64` | `buf: *mut u8` | `len: usize` | Bytes recebidos / `-errno` | Ring 0/3 | Recebe payload através de socket de rede. |
| **37** | `sys_jinnwall_ctrl` | `cmd: usize` | `arg0: usize` | `arg1: usize` | Status / `-errno` | Ring 0/3 | Consulta telemetria ou altera perfil do firewall JinnWall. |
| **38** | `sys_execve` | `path: *const u8` | `argv: *const *const u8` | `envp: *const *const u8` | Não retorna / `-errno` | Ring 3 | Substitui a imagem do processo atual pelo executável ELF no mesmo PID. |
| **39** | `sys_spawn_process` | `path: *const u8` | `argv: *const *const u8` | `envp: *const *const u8` | `PID` / `-errno` | Ring 0/3 | Cria um novo processo filho independente a partir de executável ELF. |
| **40** | `sys_dns_resolve` | `name: *const u8` | `name_len: usize` | `out_ip: *mut u8` | `0` / `-errno` | Ring 0/3 | Resolve FQDN via consulta RFC 1035 UDP/53 para IPv4 de 4 bytes. |
| **41** | `sys_statvfs` | `path: *const u8` | `path_len: usize` | `out: *mut FsStatUser` | `0` / `-errno` | Ring 0/3 | Obtém capacidade total, espaço livre e parâmetros do filesystem montado. |
| **42** | `sys_disk_info` | `disk_idx: usize` | `out: *mut DiskInfoUser` | — | `0` / `-errno` | Ring 0/3 | Consulta geometria e modelo do disco de bloco físico. |
| **43** | `sys_mount` | `prefix: *const u8` | `prefix_len: usize` | `(disk << 16) \| part` | `0` / `-errno` | Ring 0 | Monta dinamicamente partição de disco no prefixo VFS informado. |
| **44** | `sys_unmount` | `prefix: *const u8` | `prefix_len: usize` | — | `0` / `-errno` | Ring 0 | Desmonta o sistema de arquivos registrado no prefixo VFS. |
| **45** | `sys_device_info` | `idx: usize` | `desc: *mut DeviceDesc` | — | `0` / `-errno` | Ring 0/3 | Consulta descritor de dispositivo no Unified Device Manager. |
| **46** | `sys_device_count` | — | — | — | Quantidade ativa | Ring 0/3 | Retorna total de dispositivos ativos no Unified Device Manager. |
| **47** | `sys_partition_info`| `disk_idx: usize` | `part_idx: usize` | `out: *mut PartitionInfo` | `0` / `-errno` | Ring 0/3 | Consulta limites LBA e tipo de uma partição MBR/GPT. |
| **48** | `sys_volume_info` | `vol_idx: usize` | `out: *mut VolumeInfo` | — | `0` / `-errno` | Ring 0/3 | Consulta metadados de volume catalogado no Volume Manager. |
| **49** | `sys_volume_count` | — | — | — | Total de volumes | Ring 0/3 | Retorna total de volumes catalogados no Volume Manager. |
| **50** | `sys_unlink` | `path_ptr: *const u8` | `path_len: usize` | — | `0` / `-errno` | Ring 0/3 | Remove arquivo ou link simbólico do VFS. |
| **51** | `sys_mkdir` | `path_ptr: *const u8` | `path_len: usize` | — | `0` / `-errno` | Ring 0/3 | Cria um novo diretório no sistema de arquivos subjacente. |
| **52** | `sys_rename` | `args: *const RenameArgs` | — | — | `0` / `-errno` | Ring 0/3 | Renomeia ou move arquivo/diretório de forma atômica no VFS. |

---

## 4. Layouts Estruturais e Contratos Binários da ABI

```rust
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RenameArgs {
    pub old_ptr: *const u8, // Offset 0  (8 bytes)
    pub old_len: usize,     // Offset 8  (8 bytes)
    pub new_ptr: *const u8, // Offset 16 (8 bytes)
    pub new_len: usize,     // Offset 24 (8 bytes)
} // Tamanho total = 32 bytes, Alinhamento = 8 bytes
```

### Validações de Segurança Executadas pelo Kernel em Syscalls de String:
1. **Comprimento de Caminho:** Validação obrigatória de `len > 0 && len <= 128`. Rejeição com `-EINVAL` (`-22`) se excedido.
2. **Ponteiro de Usuário:** Inspeção contra limites de página em espaço de usuário via `validate_user_buffer`. Rejeição com `-EFAULT` (`-14`) para ponteiros kernel ou não mapeados.
3. **Imunidade de Pontos de Sistema:** `sys_unlink` proíbe expressamente remoção de pontos de montagem raiz (`/`, `/disk`, `/ext2`, `/dev`), retornando deterministamente `-EPERM` (`-1`).

---

## 5. Evidência de Verificação nos Testes de ABI

A suíte de testes `tools/jda-sim/tests/abi_syscall_tests.rs` comprovou 100% dos requisitos da ABI:
- Ausência de colisões numéricas no intervalo `0..=52`.
- Correspondência estrita e unívoca de `50 -> sys_unlink`, `51 -> sys_mkdir`, `52 -> sys_rename`.
- Layouts de memória de `RenameArgs` (32 bytes), `FileStat` (56 bytes) e `FsStatUser` (64 bytes).
- Rejeição previsível de ponteiros nulos e descritores inválidos.
