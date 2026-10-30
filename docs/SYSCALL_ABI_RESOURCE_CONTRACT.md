# ESPECIFICAÇÃO NORMATIVA — CONTRATO DE RECURSOS E ABI DE SYSCALLS
## Interface Binária de Aplicações (ABI) Ring 3 do Jinn OS Microkernel

**Versão da Norma:** 3.0.0<br/>
**Status:** Oficial / Auditado e Estável<br/>
**Projetos Afetados:** `jinn` (Kernel x86-64) e `aladdin` (Desktop & Compositor)<br/>
**Associação:** Issue Jinn [#44](https://github.com/GeovaneEstanislau/Jinn_Project/issues/44), [#47](https://github.com/GeovaneEstanislau/Jinn_Project/issues/47) / Aladdin [#3](https://github.com/GeovaneEstanislau/Aladdin/issues/3)

---

## 1. Princípios Arquiteturais e Isolamento de Privilégios

O **Jinn OS** implementa uma arquitetura de microkernel modular em Rust `no_std`, baseada em isolamento estrito de privilégios entre o kernel supervisor (Ring 0) e processos de usuário (Ring 3):

1. **Separação de Espaço de Endereçamento:** Cada processo possui sua própria tabela de páginas de 4 níveis (CR3 / PML4 isolado). A metade superior do espaço de endereços (`0xFFFF_8000_0000_0000..0xFFFF_FFFF_FFFF_FFFF`) mapeia o kernel com bit de privilégio supervisor (Ring 0). A metade inferior (`0x0000_0000_0000_0000..0x0000_7FFF_FFFF_FFFF`) abriga os segmentos de código, dados, heap e pilha do processo em Ring 3.
2. **Imunidade contra Page Faults do Kernel:** O kernel nunca desreferencia ponteiros de usuário diretamente sem validação de limites canônicos e mapeamento físico prévio (`validate_user_buffer`). Tentativas de passar ponteiros nulos (`0x0`) retornam `-EINVAL` (-22) e ponteiros para o espaço de kernel ou não mapeados retornam `-EFAULT` (-14).
3. **Imposição de W^X (Write XOR Execute):** Páginas de código ELF de usuário são mapeadas sem o bit `WRITABLE`. Páginas de heap e pilha de usuário são mapeadas com o bit `NO_EXECUTE` (NX), prevenindo injeção e execução de shellcode.
4. **Contabilidade e Cotas Estritas:** Todo processo está sujeito a cotas fixas de memória, descritores de arquivos, sockets, janelas gráficas e buffers JDA. Violações de limites são barradas antes de comprometer estruturas do sistema, registrando a infração em telemetria sem provocar pânico do kernel.
5. **Encerramento Idempotente:** A finalização de uma tarefa (normal ou forçada) executa a rotina de limpeza de recursos uma única vez, garantida por flag atômica (`task.cleanup_done`), liberando todos os recursos associados sem afetar outros processos.

---

## 2. Convenção de Chamada de Sistema (Syscalls ABI)

As chamadas de sistema no Jinn OS utilizam a interrupção de software vetor `0x80` (`int 0x80`).

### 2.1. Convenção de Registradores
- **Identificador da Syscall:** Registrador `RAX`.
- **Argumento 0:** Registrador `RDI`.
- **Argumento 1:** Registrador `RSI`.
- **Argumento 2:** Registrador `RDX`.
- **Argumento 3:** Registrador `R10` (quando aplicável).
- **Argumento 4:** Registrador `R8` (quando aplicável).
- **Argumento 5:** Registrador `R9` (quando aplicável).
- **Retorno:** Registrador `RAX`.
  - Valores `>= 0`: Operação concluída com sucesso (ou descritor/bytes transferidos/PID).
  - Valores `< 0`: Código de erro negativo padronizado conforme convenção POSIX.
- **Registradores Preservados:** `RBX`, `RSP`, `RBP`, `R12`, `R13`, `R14`, `R15`.

```rust
#[inline(always)]
pub unsafe fn syscall3(nr: u64, arg0: u64, arg1: u64, arg2: u64) -> i64 {
    let ret: i64;
    core::arch::asm!(
        "int 0x80",
        inout("rax") nr => ret,
        in("rdi") arg0,
        in("rsi") arg1,
        in("rdx") arg2,
        options(nostack)
    );
    ret
}
```

---

## 3. Matriz Normativa Completa de Syscalls (0 a 36)

A tabela a seguir reflete com fidelidade técnica o estado real do código em `src/syscall.rs` e drivers correlatos. Nenhum stub é apresentado como funcionalidade concluída.

| Número | Nome | Estado | Argumentos (Registradores e Tipos) | Retorno (`RAX`) | Códigos de Erro | Recursos Afetados | Evidência Técnica |
| :---: | :--- | :---: | :--- | :--- | :--- | :--- | :--- |
| **0** | `sys_exit` | **Implementada** | `rdi: exit_code (i32)` | Não retorna (preempção imediata) | N/A | Processo, memória física, FDs, janelas, JDA, sockets | `src/syscall.rs:188`, `test_qemu.ps1` |
| **1** | `sys_write` | **Implementada** | `rdi: fd (u64)`, `rsi: *const u8`, `rdx: len (usize)` | `usize` (bytes escritos) | `-EBADF` (-9), `-EFAULT` (-14), `-EINVAL` (-22) | Console VGA, UART serial, VFS, VirtIO-Net | `src/syscall.rs:197`, `apps/ola-mundo` |
| **2** | `sys_read` | **Implementada** | `rdi: fd (u64)`, `rsi: *mut u8`, `rdx: len (usize)` | `usize` (bytes lidos) | `-EBADF` (-9), `-EFAULT` (-14), `-EINVAL` (-22) | Buffer teclado, VFS, fila RX VirtIO-Net | `src/syscall.rs:242`, `apps/ola-mundo` |
| **3** | `sys_yield` | **Implementada** | Nenhum | `0` (sucesso) | N/A | Escalonador (cede fatia de tempo) | `src/syscall.rs:281`, `test_qemu.ps1` (Worker) |
| **4** | `sys_getpid` | **Implementada** | Nenhum | `u64` (PID atual) | N/A | Leitura de metadados da tarefa | `src/syscall.rs:288`, `apps/ola-mundo` |
| **5** | `sys_ipc_send` | **Implementada** | `rdi: to_pid (usize)`, `rsi: *const u8 (msg)`, `rdx: len (usize)` | `0` (sucesso) | `-ESRCH` (-3), `-EFAULT` (-14), `-EINVAL` (-22), `-EBUSY` (-16) | Fila IPC de mensagens do processo | `src/syscall.rs:296`, `src/ipc.rs` |
| **6** | `sys_ipc_recv` | **Implementada** | `rdi: *mut usize (from_pid)`, `rsi: *mut u8 (buf)`, `rdx: len (usize)` | `usize` (tamanho msg) | `-EFAULT` (-14), `-EINVAL` (-22), `-EAGAIN` (-11) | Fila IPC de mensagens do processo | `src/syscall.rs:322`, `src/ipc.rs` |
| **7** | `sys_alloc_page` | **Implementada** | Nenhum | `u64` (endereço virtual alocado) | `-ENOMEM` (-12) | Heap de usuário (`0x10000000..`), tabela PML4, cota `max_frames` | `src/syscall.rs:356`, `test_memory_pressure` |
| **8** | `sys_free_page` | **Implementada** | `rdi: virt_addr (u64)` | `0` (sucesso) | `-EFAULT` (-14) | Heap de usuário, alocador físico de frames | `src/syscall.rs:372`, `test_adv_memory_protection` |
| **9** | `sys_spawn` | **Parcial** | `rdi: fn_ptr (u64)`, `rsi: *const u8 (name)`, `rdx: priority (usize)` | `i64` (PID > 0 ou -1) | `-EINVAL` (-22), `-1` (Ring 3 não suportado) | Criação de tarefas Ring 0 (rejeita Ring 3) | `src/syscall.rs:397`, código de inspeção |
| **10** | `sys_waitpid` | **Implementada** | `rdi: pid (usize)`, `rsi: *mut i32 (status)` | `i64` (PID aguardado) | `-ESRCH` (-3), `-EFAULT` (-14), `-ECHILD` (-10) | Escalonador (bloqueia em `WaitingOn(pid)`) | `src/syscall.rs:410`, `src/task.rs` |
| **11** | `sys_sleep` | **Implementada** | `rdi: ms (u64)` | `0` (sucesso) | N/A | Escalonador, temporizador PIT (100 Hz) | `src/syscall.rs:434`, `test_qemu.ps1` (Periodic) |
| **12** | `sys_jda_shm_alloc` | **Implementada** | `rdi: is_read_only (u64)` | `u64` (handle codificado: `(gen<<16)\|slot`) | `-ENOBUFS` (-105), `-ENOMEM` (-12) | Tabela global JDA SHM, cota `max_jda_buffers` | `src/syscall.rs:444`, `test_jda_stress` |
| **13** | `sys_jda_shm_release` | **Implementada** | `rdi: handle (u64)` | `0` (sucesso) | `-EPERM` (-1), `-EBADF` (-9) | Buffer compartilhado JDA, incremento de geração | `src/syscall.rs:465`, `test_adv_jda_stale_handles` |
| **14** | `sys_jda_submit` | **Implementada** | `rdi: job_type (u64)`, `rsi: handle (u64)`, `rdx: len (usize)` | `i64` (resultado computado) | `-EPERM` (-1), `-EINVAL` (-22) | Motor de processamento JDA (jobs 1, 2, 3) | `src/syscall.rs:479`, `test_jda_stress` |
| **15** | `sys_open` | **Implementada** | `rdi: *const u8 (path)`, `rsi: len (usize)`, `rdx: flags (u32)` | `i64` (FD unificado no intervalo 3..7) | `-ENOENT` (-2), `-EFAULT` (-14), `-EINVAL` (-22), `-EMFILE` (-24) | Tabela unificada de descritores, VFS (FAT32/Ext2/ramfs) | `src/syscall.rs:499`, `test_fd_stress` |
| **16** | `sys_close` | **Implementada** | `rdi: fd (u64)` | `0` (sucesso) | `-EBADF` (-9) | Tabela unificada de descritores, arquivos VFS, sockets de rede | `src/syscall.rs:529`, `test_adv_unified_descriptors` |
| **17** | `sys_stat` | **Implementada** | `rdi: *const u8 (path)`, `rsi: len (usize)`, `rdx: *mut FileStat` | `0` (sucesso) | `-ENOENT` (-2), `-EFAULT` (-14), `-EINVAL` (-22), `-EPERM` (-1) | VFS (FAT32/Ext2/ramfs), validação de buffer | `src/syscall.rs`, `apps/ola-mundo` |
| **18** | `sys_fstat` | **Implementada** | `rdi: fd (u64)`, `rsi: *mut FileStat` | `0` (sucesso) | `-EBADF` (-9), `-EFAULT` (-14), `-EINVAL` (-22), `-EPERM` (-1) | Descritor unificado (Arquivo VFS ou Socket), validação de buffer | `src/syscall.rs`, `apps/ola-mundo` |
| **19** | `sys_lseek` | **Implementada** | `rdi: fd (u64)`, `rsi: offset (i64)`, `rdx: whence (u64)` | `i64` (novo deslocamento `>= 0`) | `-EBADF` (-9), `-EINVAL` (-22), `-ESPIPE` (-29), `-ENOENT` (-2) | Offset interno do arquivo no VFS (`fd_entry.offset`) | `src/syscall.rs`, `apps/ola-mundo` |
| **20** | `sys_window_create` | **Implementada** | `rdi: *const u8 (title)`, `rsi: len (usize)`, `rdx: packed_dims (u64)` | `i64` (win_id no compositor Aladdin) | `-EINVAL` (-22), `-EFAULT` (-14), `-EMFILE` (-24), `-ENOSYS` (-38) | Janelas do compositor Aladdin, cota `max_windows` | `src/syscall.rs:548`, `test_window_stress` |
| **21** | `sys_window_update` | **Implementada** | `rdi: win_id (usize)`, `rsi: *const u32 (buf)`, `rdx: len_bytes (usize)` | `0` (sucesso) | `-EBADF` (-9), `-EPERM` (-1), `-EINVAL` (-22), `-EFAULT` (-14) | Buffer cliente do compositor, invalidação visual | `src/syscall.rs:581`, `test_client_window_protocol` |
| **22** | `sys_window_close` | **Implementada** | `rdi: win_id (usize)` | `0` (sucesso) | `-EBADF` (-9), `-EPERM` (-1) | Janelas do compositor, desalocação de buffers | `src/syscall.rs:616`, `test_window_stress` |
| **23** | `sys_sync` | **Implementada** | Nenhum | `0` (sucesso) | `-EIO` (-5) | VFS e Cache de Blocos ATA/IDE | `src/syscall.rs:1176`, `scripts/test_persistence.ps1` |
| **24** | `sys_window_poll_event` | **Implementada** | `rdi: win_id (usize)`, `rsi: *mut ClientWindowEvent`, `rdx: flags (usize)` | `i64` (`1`=evento recebido, `0`=fila vazia) | `-EINVAL` (-22), `-EFAULT` (-14), `-EBADF` (-9), `-EPERM` (-1), `-ENOSYS` (-38) | Fila FIFO de eventos da janela cliente (16 slots) | `src/syscall.rs:1197`, `apps/ola-mundo` |
| **25** | `sys_res_stat` | **Implementada** | `rdi: target_pid (usize)`, `rsi: *mut ProcessResourceStat`, `rdx: size (usize)` | `0` (sucesso) | `-ESRCH` (-3), `-EFAULT` (-14), `-EINVAL` (-22) | Telemetria de recursos da tarefa | `src/syscall.rs`, `test_observer_app` |
| **26** | `sys_jda_shm_share` | **Implementada** | `rdi: handle (u64)`, `rsi: target_pid (usize)`, `rdx: make_owner_ro (u64)` | `i64` (endereço virtual no alvo) | `-EPERM` (-1), `-EINVAL` (-22) | Compartilhamento Zero-Copy, remapeamento PML4, ref_count | `src/syscall.rs`, `test_jda_phase3_ioring_and_zerocopy` |
| **27** | `sys_jda_ring_submit` | **Implementada** | `rdi: *const JdaSqe`, `rsi: count (usize)` | `usize` (operações submetidas) | `-EINVAL` (-22), `-EFAULT` (-14) | Fila de submissão JDA io_uring (SQ) | `src/syscall.rs`, `src/jda/ring.rs` |
| **28** | `sys_jda_ring_poll` | **Implementada** | `rdi: *mut JdaCqe`, `rsi: max_count (usize)` | `usize` (conclusões colhidas) | `-EINVAL` (-22), `-EFAULT` (-14) | Fila de conclusão JDA io_uring (CQ) | `src/syscall.rs`, `src/jda/ring.rs` |
| **29** | `sys_jda_shm_get_vaddr` | **Implementada** | `rdi: handle (u64)` | `i64` (endereço virtual `0x30000000+`) | `-EPERM` (-1) | Mapeamento virtual direto no PML4 da tarefa | `src/syscall.rs`, `test_jda_phase3_ioring_and_zerocopy` |
| **30** | `sys_socket` | **Implementada** | `rdi: domain (i32)`, `rsi: type (i32)`, `rdx: protocol (i32)` | `i64` (FD unificado no intervalo 3..7) | `-EINVAL` (-22), `-EMFILE` (-24) | Tabela unificada de descritores, `SOCKET_TABLE`, cota `max_sockets` | `src/syscall.rs`, `test_adv_unified_descriptors` |
| **31** | `sys_connect` | **Implementada** | `rdi: sockfd (u64)`, `rsi: *const SockAddrIn`, `rdx: size (usize)` | `0` (sucesso) | `-EBADF` (-9), `-EFAULT` (-14), `-EINVAL` (-22) | Estado do socket, resolução ARP | `src/syscall.rs`, `test_adv_unified_descriptors` |
| **32** | `sys_bind` | **Implementada** | `rdi: sockfd (u64)`, `rsi: *const SockAddrIn`, `rdx: size (usize)` | `0` (sucesso) | `-EBADF` (-9), `-EFAULT` (-14), `-EINVAL` (-22), `-EADDRINUSE` (-98) | Socket de rede, porta local vinculada | `src/syscall.rs`, `src/net/socket.rs` |
| **33** | `sys_listen` | **Implementada** | `rdi: sockfd (u64)`, `rsi: backlog (usize)` | `0` (sucesso) | `-EBADF` (-9), `-EINVAL` (-22), `-EPERM` (-1) | Estado TCP (LISTEN), fila de conexões pendentes | `src/syscall.rs`, `src/net/socket.rs` |
| **34** | `sys_accept` | **Implementada** | `rdi: sockfd (u64)`, `rsi: *mut SockAddrIn`, `rdx: addrlen (usize)` | `i64` (novo FD unificado 3..7) | `-EBADF` (-9), `-EFAULT` (-14), `-EINVAL` (-22), `-EAGAIN` (-11), `-EMFILE` (-24) | Tabela unificada de descritores, cota `max_sockets`, conexão TCP filha | `src/syscall.rs`, `src/net/socket.rs` |
| **35** | `sys_send` | **Implementada** | `rdi: sockfd (u64)`, `rsi: *const u8 (buf)`, `rdx: len (usize)` | `usize` (bytes transmitidos) | `-EBADF` (-9), `-EFAULT` (-14), `-EINVAL` (-22) | Placa VirtIO-Net, firewall JinnWall (conntrack de saída) | `src/syscall.rs:214`, `test_adv_real_network_dns` |
| **36** | `sys_recv` | **Implementada** | `rdi: sockfd (u64)`, `rsi: *mut u8 (buf)`, `rdx: len (usize)` | `usize` (bytes recebidos ou 0 se fila vazia) | `-EBADF` (-9), `-EFAULT` (-14), `-EINVAL` (-22) | Fila circular RX do socket | `src/syscall.rs:260`, `test_adv_real_network_dns` |
| **37** | `sys_jinnwall_ctrl` | **Implementada** | `rdi: cmd (usize)`, `rsi: arg0 (usize)`, `rdx: arg1 (usize)` | `i64` (perfil ou 0 sucesso) | `-EINVAL` (-22), `-EFAULT` (-14) | Controle e telemetria do firewall stateful JinnWall SPI | `src/syscall.rs`, `test_tcp_network_interoperability` |
| **38** | `sys_execve` | **Implementada** | `rdi: *const u8 (path)`, `rsi: *const *const u8 (argv)`, `rdx: *const *const u8 (envp)` | `0` (sucesso não retorna; substitui imagem) | `-ENOENT` (-2), `-EACCES` (-13), `-EFAULT` (-14), `-EINVAL` (-22), `-ENOEXEC` (-8), `-E2BIG` (-7), `-ENOMEM` (-12) | Espaço de endereçamento (CR3 comutado para novo PML4), registradores Ring 3 resetados, janelas/buffers JDA antigos limpos; mantém o mesmo PID e FDs abertos | `src/syscall.rs:185`, `src/scheduler.rs:456`, `apps/ola-mundo` |
| **39** | `sys_spawn` | **Implementada** | `rdi: *const u8 (path)`, `rsi: *const *const u8 (argv)`, `rdx: *const *const u8 (envp)` | `i64` (novo PID > 0) | `-ENOENT` (-2), `-EACCES` (-13), `-EFAULT` (-14), `-EINVAL` (-22), `-ENOEXEC` (-8), `-E2BIG` (-7), `-ENOMEM` (-12), `-EMFILE` (-24) | Cria novo processo filho independente em Ring 3, novo PML4, nova pilha com `argc`, `argv`, `envp` | `src/syscall.rs:194`, `src/scheduler.rs:341`, `apps/ola-mundo` |
| **40** | `sys_dns_resolve` | **Implementada** | `rdi: *const u8 (domain)`, `rsi: name_len (usize)`, `rdx: *mut u8 (out_ip_4bytes)` | `0` (sucesso) | `-ENOENT` (-2), `-EFAULT` (-14), `-EINVAL` (-22), `-ETIMEDOUT` (-110) | Placa VirtIO-Net, DNS UDP/53, Cache DNS do kernel, JinnWall SPI | `src/syscall.rs`, `src/net/dns.rs`, `apps/ola-mundo` |

---

## 3.1. Semântica Normativa de Execução: `sys_execve` vs `sys_spawn`

O Jinn OS distingue com rigor técnico as duas primitivas de execução de programas ELF:

1. **`sys_spawn` (Criação de Processo Filho):**
   - Cria uma nova entrada na tabela de tarefas (`scheduler.tasks[new_pid]`), atribuindo um PID único.
   - Constrói um novo espaço de paginação PML4 isolado a partir do clone das tabelas do kernel.
   - Carrega integralmente os segmentos `PT_LOAD` do binário ELF a partir do caminho no VFS (ex: `/disk/APP.ELF`).
   - Constrói a pilha de usuário mapeando 64 frames (256 KiB) sob a base `0x0000_7FFF_FFFF_0000 - (PID * 256 KiB)` e serializa strings e ponteiros de `argc`, `argv` e `envp` no topo da pilha.
   - Inicializa os registradores na pilha de kernel com a convenção System V AMD64 (`RDI=argc`, `RSI=argv`, `RDX=envp`) e alinhamento de 16 bytes.
   - O processo chamador continua executando concorrentemente e pode aguardar o encerramento do filho chamando `sys_waitpid(child_pid)`.

2. **`sys_execve` (Substituição de Imagem no Mesmo PID):**
   - Substitui o programa em execução no processo corrente, **preservando o mesmo PID** e a herança de descritores de arquivos abertos válidos.
   - Constrói o novo PML4, carrega o ELF e aloca a nova pilha com argumentos em segundo plano (*off-line*).
   - **Ponto de Commit Transacional:** Se a abertura no VFS, o parser ELF ou as alocações de memória falharem, o kernel aborta a operação e retorna o código de erro em `RAX`, mantendo o processo chamador 100% intacto, sem alterar o CR3 nem invalidar a memória anterior.
   - Uma vez validados todos os segmentos e frames, o ponto de commit é atingido: as janelas GUI antigas e buffers Zero-Copy antigos são desalocados, o registrador CR3 da CPU é comutado para o novo PML4, a memória do espaço de usuário antigo é liberada via `paging::free_user_pml4(pml4_old)`, e o frame de interrupção na pilha de kernel é reconfigurado para retornar diretamente no ponto de entrada (`e_entry`) em Ring 3 via `iretq`.


---

## 4. Contrato de Integração Formal entre Jinn OS e Aladdin Desktop

A integração entre o kernel e o ambiente gráfico Aladdin é regida por contratos estritos de compilação, abstração de plataforma (PAL) e gerenciamento de recursos:

### 4.1. Vínculo e Dependência via Cargo
- O kernel consome a crate `aladdin` declarada em `Cargo.toml`:
  ```toml
  aladdin = { path = "../Aladdin", default-features = false }
  ```
- O Aladdin é compilado em modo `#![no_std]` puro, dependendo exclusivamente de `core` e `alloc`.
- A suite de testes unitários do Aladdin (`tests/aladdin_tests.rs`) utiliza a feature opcional padrão `std`, permitindo execução nativa rápida no host de compilação sem necessidade de boot no QEMU.

### 4.2. Abstração de Plataforma (PAL) em `src/gui.rs`
O kernel implementa as traits declaradas em `aladdin::contracts` sem expor tipos internos do kernel para o compositor:
- `FramebufferDevice`:
  - `dimensions(&self) -> (usize, usize, usize)`: Fornece resolução física (1024 × 768) e pitch da VRAM Limine.
  - `swap_rect(&mut self, rect: &DirtyRect, buffer: &[u32])`: Caminho otimizado que copia somente a região retangular danificada para a VRAM.
  - `swap_full(&mut self, buffer: &[u32])`: Cópia total da tela quando necessário redesenho integral.
- `SystemHost`:
  - `ticks(&self) -> u64`: Fornece timestamp do temporizador PIT (100 Hz).
  - `system_metrics(&self) -> SystemMetrics`: Fornece taxa de CPU, uso de heap do kernel e conexões ativas do firewall JinnWall.
- `VfsHost` e `ProcessHost`:
  - Permitem ao Aladdin consultar diretórios VFS (`/apps`, `/sys`) e obter lista de tarefas ativas para exibição de métricas na barra de tarefas.

### 4.3. Protocolo de Buffers de Janela (Ownership e Sanitização)
- **Alocação e Propriedade:** O kernel aloca e mantém a posse dos buffers de pixels do cliente (`client_buffers: [Option<Vec<u32>>; MAX_WINDOWS]`).
- **Sanitização de Fronteiras:**
  - A Syscall 20 valida que `client_w > 0 && client_h > 0` e `client_w * client_h <= 1024 * 768`.
  - A Syscall 21 valida o buffer de usuário com `validate_user_buffer` garantindo que o ponteiro pertença estritamente ao espaço de usuário mapeado antes de copiar para o buffer do compositor.
  - Processos não proprietários (`owner_pid != calling_pid`) recebem `-EPERM` (-1).
- **Aritmética Segura:** Dimensões totais das janelas utilizam `saturating_add`, impedindo transbordamentos inteiros que pudessem corromper a pilha gráfica.

### 4.4. Limpeza Automática no Encerramento de Processos (Zombie Cleanup)
- Quando uma tarefa é encerrada (normalmente via `sys_exit` ou abortada por violação), o escalonador invoca `gui::close_windows_for_pid(pid)`.
- Todas as janelas associadas a `owner_pid` são fechadas, seus buffers desalocados e a área da tela marcada como danificada para restauração do papel de parede.
- O campo `task.cleanup_done` assegura execução única e estritamente idempotente.

---

## 5. Cotas e Limites de Recursos por Tarefa (`ResourceLimits`)

O subsistema de contabilidade de recursos do kernel (`src/resources.rs`) impõe limites fixos por processo:

```rust
pub struct ResourceLimits {
    pub max_frames: usize,       // Padrão: 128 frames (512 KiB)
    pub max_fds: usize,          // Padrão: 8 descritores unificados
    pub max_windows: usize,      // Padrão: 4 janelas do compositor
    pub max_jda_buffers: usize,  // Padrão: 4 buffers Zero-Copy JDA
    pub max_sockets: usize,      // Padrão: 4 sockets de rede
    pub max_cpu_ticks: u64,      // Cota máxima de ticks de CPU (0 = ilimitado)
}
```

### Telemetria e Monitoramento de Recursos (`ProcessResourceStat`)
A Syscall 25 (`sys_res_stat`) preenche a seguinte estrutura:

```rust
#[repr(C)]
pub struct ProcessResourceStat {
    pub pid: u64,
    pub state: u32,
    pub cpu_ticks: u64,
    pub context_switches: u64,
    pub memory_frames: usize,
    pub peak_memory_frames: usize,
    pub max_memory_frames: usize,
    pub open_fds: usize,
    pub max_fds: usize,
    pub open_windows: usize,
    pub max_windows: usize,
    pub jda_buffers: usize,
    pub max_jda_buffers: usize,
    pub open_sockets: usize,
    pub max_sockets: usize,
    pub limit_violations: u64,
    pub last_violation_code: i64,
    pub max_cpu_ticks: u64,
    pub irq_preemptions: u64,
    pub voluntary_yields: u64,
}
```

### Metadados de Arquivo e Diretório (`FileStat`)
As Syscalls 17 (`sys_stat`) e 18 (`sys_fstat`) preenchem a estrutura normativa `FileStat` (56 bytes, alinhamento natural a 8 bytes, sem padding implícito):

```rust
#[repr(C)]
pub struct FileStat {
    pub size: u64,          // [Offset 0,  8B] Tamanho do arquivo em bytes
    pub inode: u64,         // [Offset 8,  8B] Identificador único (ou inode do VFS / FD de stdio / ID de socket)
    pub node_type: u32,     // [Offset 16, 4B] 0 = File, 1 = Directory, 2 = CharDevice, 3 = BlockDevice, 4 = Symlink, 5 = Socket
    pub mode: u32,          // [Offset 20, 4B] Modo POSIX (ex: 0o100644 arquivo, 0o040755 diretório, 0o020666 chardev, 0o140666 socket)
    pub blksize: u32,       // [Offset 24, 4B] Tamanho de bloco ideal para I/O (512 bytes)
    pub blocks: u32,        // [Offset 28, 4B] Blocos alocados de 512B: (size + 511) / 512
    pub atime: u64,         // [Offset 32, 8B] Timestamp de acesso (ticks monotônicos do PIT @ 100 Hz desde o boot)
    pub mtime: u64,         // [Offset 40, 8B] Timestamp de modificação (ticks monotônicos do PIT @ 100 Hz desde o boot)
    pub ctime: u64,         // [Offset 48, 8B] Timestamp de criação (ticks monotônicos do PIT @ 100 Hz desde o boot)
}
```

> [!NOTE]
> **Semântica dos Timestamps:** Os campos `atime`, `mtime` e `ctime` registram **ticks monotônicos do PIT (100 Hz = 10 ms por tick)** contados desde a inicialização do microkernel. Não representam segundos de calendário POSIX (Unix Epoch) devido à ausência deliberada de driver de RTC na arquitetura microkernel atual.

> [!NOTE]
> **Descritores Especiais em `fstat`:**
> - `stdin` (FD 0), `stdout` (FD 1) e `stderr` (FD 2) são identificados como dispositivos de caractere (`node_type = 2`, `mode = 0o020666`, `inode = fd`).
> - Sockets de rede (FD >= 3 associados a TCP/UDP) são identificados como sockets (`node_type = 5`, `mode = 0o140666`, `inode = sock_id`).

#### Flags e Regras de Reposicionamento (`sys_lseek` - Syscall 19)
- `SEEK_SET` (`0`): O deslocamento é ajustado diretamente para `offset`. Requer `offset >= 0`.
- `SEEK_CUR` (`1`): O deslocamento é somado ao offset atual do descritor (`current_offset + offset`). Requer que a soma não exceda os limites de representação de `i64` nem resulte em valor negativo.
- `SEEK_END` (`2`): O deslocamento é somado ao tamanho total do arquivo (`file_size + offset`). Requer que a soma não cause overflow aritmético nem resulte em valor negativo.

**Regras de Segurança e Invariância em `sys_lseek`:**
1. **Unseekable Streams:** Tentativas de reposicionamento em fluxos de console (`fd < 3`) ou em sockets de rede retornam estritamente `-ESPIPE` (-29).
2. **Prevenção contra Overflow:** Cálculos aritméticos utilizam `checked_add`. Saturação além de `i64::MAX` ou underflow abaixo de zero retornam `-EINVAL` (-22).
3. **Invariância de Posição:** Caso qualquer validação falhe (offset negativo, overflow, whence inválido), o cursor de leitura/escrita do descritor permanece **estritamente inalterado**.

#### Contrato de Janelas Gráficas e Fila de Eventos do Compositor (`sys_window_create`, `sys_window_update`, `sys_window_close`, `sys_window_poll_event`)

As syscalls gráficas integram processos em Ring 3 ao **Aladdin Desktop Compositor**:

- **`sys_window_create(title_ptr, title_len, packed_dims)` (Syscall 20):** Cria uma janela do cliente Aladdin com largura e altura empacotadas em `packed_dims = ((width as u64) << 16) | (height as u64)`. Retorna o identificador de janela `win_id >= 0` ou `-EMFILE` (-24) caso atinja a cota `max_windows`.
- **`sys_window_update(win_id, buffer_ptr, buffer_len_bytes)` (Syscall 21):** Copia o buffer de pixels ARGB32 de Ring 3 para o framebuffer da janela do compositor. Valida o intervalo completo de páginas do buffer via `validate_user_buffer` e marca a região danificada para recomposição imediata.
- **`sys_window_close(win_id)` (Syscall 22):** Encerra a janela, remove-a do compositor, esvazia sua fila de eventos e decrementa a cota `task.resources.open_windows`. Retorna `0` em caso de sucesso ou `-EBADF` (-9) se a janela não existir.
- **`sys_window_poll_event(win_id, event_ptr, flags)` (Syscall 24):** Consulta o próximo evento pendente na fila FIFO da janela.

##### Estrutura Binária `ClientWindowEvent` (32 bytes, alinhamento natural a 8 bytes):

```rust
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ClientWindowEvent {
    pub event_type: u32,     // [Offset 0,  4B] Tipo de evento (EVENT_* de 0 a 7)
    pub win_id: u32,         // [Offset 4,  4B] ID da janela de destino
    pub x: i32,              // [Offset 8,  4B] Coordenada X relativa à área cliente
    pub y: i32,              // [Offset 12, 4B] Coordenada Y relativa à área cliente
    pub button_or_key: u32,  // [Offset 16, 4B] Código do botão do mouse ou caractere ASCII da tecla
    pub modifiers: u32,      // [Offset 20, 4B] Modificadores (Shift, Ctrl, Alt)
    pub timestamp: u64,      // [Offset 24, 8B] Timestamp monotônico em ticks do PIT (100 Hz)
}
```

##### Tipos de Eventos e Realidade de Hardware:

| Constante | Valor | Descrição Semântica | Origem e Despacho no Aladdin | Estado do Hardware |
| :--- | :---: | :--- | :--- | :--- |
| `EVENT_NONE` | 0 | Ausência de eventos | Retornado quando a fila está vazia | Implementado e testado |
| `EVENT_MOUSE_DOWN` | 1 | Botão do mouse pressionado | Despachado ao clicar na área cliente com coordenadas `(x, y)` | Implementado e testado (PS/2) |
| `EVENT_MOUSE_UP` | 2 | Botão do mouse liberado | Despachado ao soltar o botão esquerdo sobre a janela | Implementado e testado (PS/2) |
| `EVENT_MOUSE_MOVE` | 3 | Movimento do cursor | Movimento do mouse sobre a área cliente | Suportado na ABI |
| `EVENT_KEY_DOWN` | 4 | Tecla pressionada | Encaminhado à janela cliente com foco ativo (`button_or_key = ch as u32`) | Implementado e testado (PS/2) |
| `EVENT_KEY_UP` | 5 | Tecla liberada | Notificação de liberação de tecla | Suportado na ABI; o driver PS/2 atual não gera break-codes |
| `EVENT_SCROLL` | 6 | Roda de rolagem | Rolagem de conteúdo | Suportado na ABI; mouse PS/2 padrão de 3 bytes não possui wheel packet |
| `EVENT_WINDOW_CLOSE` | 7 | Pedido de fechamento | Notificação de clique no botão `[X]` da barra de título | Implementado e testado com proteção contra descarte |

##### Ciclo de Vida e Política de Fila:
1. **Fila FIFO Delimitada (16 slots por janela):** Cada janela cliente possui uma fila circular independente de eventos (`ClientEventQueue`).
2. **Proteção de `EVENT_WINDOW_CLOSE` contra Overflow:** Caso a fila esteja cheia devido ao fluxo intenso de eventos de mouse ou teclado, a política de inserção descarta o evento mais antigo que **não seja** `EVENT_WINDOW_CLOSE` (`drop_oldest_non_close`). O evento de fechamento nunca é descartado silenciosamente.
3. **Ciclo de Fechamento em Dois Estágios:**
   - No primeiro clique em `[X]`, a janela é marcada com `close_requested = true`, oculta visualmente (`is_open = false`) e um `EVENT_WINDOW_CLOSE` é inserido na fila para o processo cliente fazer a limpeza graciosa chamando `sys_window_close`.
   - Se o usuário clicar novamente em `[X]` ou se o processo proprietário for encerrado, o compositor destrói a janela de forma forçada e recupera os recursos.
4. **Isolamento de Processos e Recuperação:**
   - Apenas o PID proprietário pode consultar eventos de sua janela (`poll_client_event(owner_pid, win_id)` retorna `-EPERM` para outro PID).
   - Ao terminar o processo (`sys_exit` ou morte por sinal/cota), o kernel executa `close_windows_for_pid(pid)`, destruindo todas as janelas do processo e liberando totalmente a memória e as cotas.
5. **Semântica de Retorno da Syscall 24:**
   - Retorno `1`: Evento copiado com sucesso para o ponteiro `event_ptr`.
   - Retorno `0`: Fila vazia, sem eventos pendentes no momento da consulta.
   - Retorno `-EINVAL` (-22): `event_ptr` nulo.
   - Retorno `-EFAULT` (-14): `event_ptr` fora do espaço de usuário, não mapeado ou cruzando limites de página não mapeada/somente leitura.
   - Retorno `-EBADF` (-9): `win_id` inexistente ou janela já destruída.
   - Retorno `-EPERM` (-1): Processo chamador não é o proprietário da janela.
   - Retorno `-ENOSYS` (-38): Sessão gráfica do compositor não ativa.

#### Telemetria e Controle do Firewall (`JinnWallStats` - Syscall 37)
A Syscall 37 (`sys_jinnwall_ctrl`) permite gerenciar e auditar o firewall stateful SPI a partir de Ring 3:
- `cmd = 0`: Consulta perfil ativo (`0` = `PublicSecure`, `1` = `Development`).
- `cmd = 1`: Ajusta perfil ativo (`arg0`: `0` ou `1`). Retorna `0` ou `-EINVAL`.
- `cmd = 2`: Preenche a estrutura normativa `JinnWallStats` (32 bytes, alinhamento natural a 8 bytes, sem padding implícito):

```rust
#[repr(C)]
pub struct JinnWallStats {
    pub inspected: u64,      // [Offset 0,  8B] Total de pacotes IP avaliados pelo firewall
    pub accepted: u64,       // [Offset 8,  8B] Total de pacotes autorizados e despachados para L4
    pub dropped: u64,        // [Offset 16, 8B] Total de pacotes descartados por regras/política
    pub active_conns: u32,   // [Offset 24, 4B] Fluxos bidirecionais ativos na tabela Conntrack
    pub profile: u32,        // [Offset 28, 4B] Perfil ativo (0 = PublicSecure, 1 = Development)
}
```

#### Contrato de Backlog TCP e Fila de Conexões (`sys_listen` e `sys_accept`)
A implementação de sockets servidores TCP opera com filas delimitadas e seguras:
- **Separação de Filas:**
  - `syn_queue`: Armazena conexões embrionárias (SYN recebido, handshake em andamento no estado `SynReceived`). Limite máximo: `MAX_SYN_BACKLOG = 8`.
  - `accept_queue`: Armazena conexões totalmente estabelecidas (`Established`) aguardando retirada via `sys_accept`. Limite configurável pelo argumento `backlog` em `sys_listen(sockfd, backlog)`, delimitado no intervalo `1..=MAX_BACKLOG (8)`.
- **Comportamento em Saturação:**
  - Caso `syn_queue` ou a soma de conexões pendentes atinja a capacidade, novos pacotes `SYN` são silenciosamente descartados (conforme RFC 1122), induzindo o cliente a retransmitir sob backoff exponencial.
  - Caso a `accept_queue` esteja saturada no momento da conclusão do handshake (ACK final), a conexão é abortada com emissão de segmento `RST` para prevenir conexões órfãs.
- **Entrega Única e Ordem FIFO:**
  - `sys_accept` extrai conexões da cabeça da `accept_queue` (FIFO estrito).
  - A conexão retirada tem seu ponteiro `parent_listener` desvinculado (`None`), garantindo entrega exatamente uma vez para o descritor recém-alocado.
- **Encerramento de Listener e Processos:**
  - Ao fechar o socket ou no término do processo (`close_all_for_pid`), todas as conexões pendentes em `syn_queue` e `accept_queue` são abortadas com emissão de pacote `RST|ACK` ao par remoto, e suas entradas na tabela Conntrack do JinnWall e tabela global de sockets são liberadas.

#### Contrato de Escrita Persistente FAT32, Alocação de Clusters e Sincronização (`sys_write` e `sys_sync`)
O driver de sistema de arquivos FAT32 do Jinn OS opera com semântica de escrita persistente real sobre discos ATA/IDE (modo PIO LBA28):
- **Alocação Dinâmica de Clusters:**
  - A busca por clusters livres varre as entradas da FAT32 (entradas com valor `0x0000_0000`).
  - Ao alocar, a entrada é imediatamente marcada com marca de fim de arquivo (`0x0FFF_FFFF`) preservando os 4 bits superiores reservados.
  - Para conformidade estrita com o padrão FAT32, a gravação é **espelhada em todas as tabelas FAT do volume** (`num_fats`).
  - Os setores de dados do cluster alocado são zerados na mídia para mitigar vazamento de dados residuais.
- **Transposição de Limites de Setor e Cluster:**
  - A escrita suporta cargas de dados contíguas que cruzam múltiplos setores (512 bytes) e múltiplos clusters (4096 bytes).
  - Escritas parciais preservam bytes adjacentes fora da janela modificada via protocolo *read-modify-write*.
  - Escritas que atingem o final da cadeia alocam dinamicamente novos clusters, encadeando `cluster_anterior -> novo_cluster` em todas as tabelas FAT.
- **Atualização Consistente de Metadados e Entradas de Diretório:**
  - O descritor aberto armazena `dir_sector_lba` e `dir_entry_offset`.
  - Ao expandir o arquivo ou alocar o primeiro cluster, a entrada de diretório de 32 bytes correspondente tem seu tamanho (`size`) e cluster inicial (`first_cluster`) atualizados e persistidos diretamente no disco.
- **Garantias de Persistência e Sincronização (`sys_sync` - Syscall 23):**
  - O fechamento do descritor (`sys_close`) e a chamada explícita de `sys_sync()` invocam `flush_primary_cache()`, descarregando todos os blocos sujos da memória RAM para os setores físicos do disco rígido.
  - A persistência física dos bytes é mantida integralmente através de ciclos de desligamento e reinicialização a frio (*Cold Reboot*), com validação forense no arquivo `disk.img` e leitura íntegra via VFS.
- **Limitações e Integridade pós-Falha:**
  - O FAT32 não possui *journaling* transacional nativo. Quedas abruptas de energia durante a transição de setores podem gerar clusters órfãos. A ordem operacional do Jinn OS (dados -> FAT -> diretório) minimiza a exposição de dados corrompidos.


---

### 5.1. Preempção por Hardware vs Yields Voluntários
- O timer PIT (IRQ0, vetor 32) opera a 100 Hz gerando interrupções a cada 10 ms.
- A cada interrupção com o quantum esgotado (`quantum_remaining == 0`), o escalonador força a troca de contexto, incrementando `irq_preemptions`.
- Quando um processo cede a CPU voluntariamente via `sys_yield()`, a flag de rendição voluntária é acionada e o escalonador incrementa `voluntary_yields`.
- Processos em computação pura (*CPU-hog*) sem chamadas de sistema acumulam `irq_preemptions > 0` e estritamente `voluntary_yields == 0`.

### 5.2. Aplicação Efetiva de Cotas de CPU
- Quando `limits.max_cpu_ticks > 0` e o tempo de execução acumulado (`cpu_ticks`) atinge o limite da cota, o escalonador registra a violação com código `-EDQUOT` (-122) e encerra o processo via `exit_task(pid, -122)`.
- Todos os recursos mantidos pelo processo (páginas de memória, janelas gráficas, descritores de arquivo/socket, buffers JDA) são automaticamente liberados pelo procedimento de encerramento do kernel.

### 5.3. Comportamento em Caso de Violação de Cota
- As funções `check_and_*` barram a operação antes de comprometer qualquer estrutura interna.
- O contador `task.resources.limit_violations` é incrementado monotonicamente.
- O código do último erro é armazenado em `task.resources.last_violation_code` para auditoria via `sys_res_stat`.
- Nenhum pânico ocorre no kernel; o erro POSIX correspondente é devolvido imediatamente à aplicação.

### 5.4. Metodologia de Validação Adversarial e Gráfica
- **Concorrência e Ausência de Starvation:** Enquanto uma tarefa monopolizadora em Ring 3 executa em loop fechado de computação pura, tarefas concorrentes (como a Tarefa Periódica emitindo heartbeats e o teste de janelas) continuam progredindo através do chaveamento periódico forçado pelo PIT (100 Hz).
- **Recuperação Gráfica pós-Saturação:** A criação e renderização visual do cliente Aladdin Desktop (`run_gui_demo`) após o encerramento do processo monopolizador atesta a integridade do compositor, liberação dos recursos associados e pronta resposta da interface gráfica após eventos de saturação de CPU.

---

## 6. Tabela Normativa de Códigos de Erro POSIX

| Código Simbólico | Valor Inteiro | Descrição Semântica no Jinn OS |
| :--- | :--- | :--- |
| `EPERM` | `-1` | Operação não permitida (violação de acesso cross-PID ou handle JDA de outra tarefa). |
| `ENOENT` | `-2` | Arquivo ou entrada de diretório não encontrada no VFS. |
| `ESRCH` | `-3` | Processo não encontrado (PID inexistente ou já finalizado). |
| `EBADF` | `-9` | Descritor inválido (FD fechado, fora do intervalo `3..8`, ou type mismatch como `connect` em arquivo). |
| `ECHILD` | `-10` | Processo não possui tarefas filhas a aguardar. |
| `EAGAIN` | `-11` | Recurso temporariamente indisponível (fila IPC de recepção vazia). |
| `ENOMEM` | `-12` | Memória física ou cota de frames do processo esgotada (`max_frames`). |
| `EFAULT` | `-14` | Falha de segmentação: endereço de ponteiro inválido, fora do espaço de usuário ou não mapeado. |
| `EBUSY` | `-16` | Fila IPC de destino cheia. |
| `EINVAL` | `-22` | Argumento inválido (parâmetro nulo, alinhamento incorreto ou flags incompatíveis). |
| `EMFILE` | `-24` | Esgotamento de descritores de arquivo, sockets ou janelas do processo (`max_fds`, `max_windows`). |
| `ESPIPE` | `-29` | Deslocamento inválido em pipe ou socket (`sys_lseek` em descritor não suportado). |
| `ENOSYS` | `-38` | Funcionalidade ou chamada de sistema não suportada nesta configuração. |
| `EADDRINUSE` | `-98` | Endereço ou porta local já em uso por outro socket. |
| `ECONNRESET` | `-104` | Conexão reinicializada abruptamente pelo par remoto (RST). |
| `ENOBUFS` | `-105` | Esgotamento de buffers compartilhados JDA (`max_jda_buffers`). |
| `ETIMEDOUT` | `-110` | Tempo limite de conexão ou transmissão de rede esgotado. |
| `ECONNREFUSED` | `-111` | Conexão recusada ativamente pelo par de rede. |
| `EDQUOT` | `-122` | Cota de recursos esgotada (ex: cota máxima de tempo de CPU `max_cpu_ticks` atingida). |
