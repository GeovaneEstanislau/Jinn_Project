/// Jinn OS — User-Space Syscall API (Ring 3)
///
/// Este módulo contém os wrappers de baixo nível para todas as syscalls
/// do Jinn OS. Ele deve ser usado **exclusivamente** por código que executa
/// em Ring 3 (processos de usuário).
///
/// As funções aqui NÃO fazem nenhuma chamada direta ao kernel — toda
/// comunicação acontece via `int 0x80`, com a CPU mediando a transição
/// Ring 3 → Ring 0 → Ring 3.
///
/// # Convenção de chamada
///
/// Segue o mesmo modelo do Linux i386 (familiar e simples):
///   - `RAX` = número da syscall
///   - `RDI` = arg0,  `RSI` = arg1,  `RDX` = arg2
///   - Retorno em `RAX` (negativo = código de erro)
///
/// # Tabela de Syscalls
///
/// | Nº | Nome         | Assinatura (RDI, RSI, RDX)              |
/// |----|--------------|------------------------------------------|
/// |  0 | `exit`       | código_saída                             |
/// |  1 | `write`      | fd, *buf, len                            |
/// |  2 | `read`       | fd, *buf, len                            |
/// |  3 | `yield`      | —                                        |
/// |  4 | `getpid`     | — → PID em RAX                           |
/// |  5 | `ipc_send`   | pid_dst, tag, *payload                   |
/// |  6 | `ipc_recv`   | *buf, len → bytes lidos                  |
/// |  7 | `alloc_page` | — → endereço físico do frame             |
/// |  8 | `free_page`  | endereço_físico                          |
/// |  9 | `spawn`      | fn_ptr, name_ptr, ring (0=K, 3=U) → PID |
/// | 10 | `waitpid`    | pid → exit_code (bloqueia até Zombie)    |

// ── Primitiva de chamada genérica ─────────────────────────────────────────────

/// Emite `int 0x80` com três argumentos e retorna o valor em RAX.
///
/// Esta é a única instrução privilegiada que código Ring 3 pode emitir
/// sem causar uma General Protection Fault — a IDT registra o vetor 0x80
/// com DPL=3, permitindo sua execução em Ring 3.
#[inline(always)]
unsafe fn syscall3(nr: u64, arg0: u64, arg1: u64, arg2: u64) -> i64 {
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

/// Emite `int 0x80` sem argumentos.
#[inline(always)]
unsafe fn syscall0(nr: u64) -> i64 {
    let ret: i64;
    core::arch::asm!(
        "int 0x80",
        inout("rax") nr => ret,
        options(nostack)
    );
    ret
}

// ── Syscall 0 — exit ──────────────────────────────────────────────────────────

/// Finaliza o processo atual com um código de saída.
///
/// O kernel marca a tarefa como `Zombie` e dispara uma preempção imediata.
/// Esta função **não retorna**.
///
/// # Exemplo
/// ```no_run
/// sys_exit(0);   // sucesso
/// sys_exit(1);   // erro genérico
/// ```
#[inline(always)]
pub unsafe fn sys_exit(code: i32) -> ! {
    syscall3(0, code as u64, 0, 0);
    // Caso improvável de retorno (não deveria acontecer):
    loop { core::arch::asm!("hlt"); }
}

// ── Syscall 1 — write ─────────────────────────────────────────────────────────

/// Escreve `len` bytes de `buf` no descritor de arquivo `fd`.
///
/// Descritores suportados:
/// - `fd = 1` → stdout (VGA, cor padrão)
/// - `fd = 2` → stderr (VGA, vermelho)
///
/// Retorna o número de bytes escritos, ou um código de erro negativo.
///
/// # Safety
/// `buf` deve apontar para `len` bytes válidos e acessíveis em Ring 3.
#[inline(always)]
pub unsafe fn sys_write(fd: u64, buf: *const u8, len: u64) -> i64 {
    syscall3(1, fd, buf as u64, len)
}

/// Conveniência: escreve um `&[u8]` em stdout (fd=1).
///
/// # Safety
/// O slice deve ser válido durante a chamada.
#[inline(always)]
pub unsafe fn sys_write_str(s: &[u8]) -> i64 {
    sys_write(1, s.as_ptr(), s.len() as u64)
}

// ── Syscall 2 — read ──────────────────────────────────────────────────────────

/// Lê até `len` bytes do descritor de arquivo `fd` para `buf`.
///
/// Descritores suportados:
/// - `fd = 0` → stdin (buffer de teclado, não-bloqueante)
///
/// Retorna o número de bytes lidos (0 se o buffer estiver vazio),
/// ou um código de erro negativo.
///
/// # Safety
/// `buf` deve apontar para `len` bytes graváveis em Ring 3.
#[inline(always)]
pub unsafe fn sys_read(fd: u64, buf: *mut u8, len: u64) -> i64 {
    syscall3(2, fd, buf as u64, len)
}

// ── Syscall 3 — yield ─────────────────────────────────────────────────────────

/// Cede voluntariamente o quantum de CPU restante para a próxima tarefa pronta.
///
/// O escalonador seleciona a próxima tarefa de maior prioridade.
/// Esta função **retorna** quando a tarefa recebe CPU novamente.
///
/// # Exemplo
/// ```no_run
/// loop {
///     // faz trabalho...
///     sys_yield();
/// }
/// ```
#[inline(always)]
pub unsafe fn sys_yield() {
    syscall0(3);
}

// ── Syscall 4 — getpid ────────────────────────────────────────────────────────

/// Retorna o PID (Process ID) da tarefa atual.
///
/// O PID é o índice da tarefa no array interno do escalonador (0–15).
///
/// # Exemplo
/// ```no_run
/// let pid = sys_getpid();
/// ```
#[inline(always)]
pub unsafe fn sys_getpid() -> u64 {
    syscall0(4) as u64
}

// ── Syscall 5 — ipc_send ─────────────────────────────────────────────────────

/// Envia uma mensagem IPC para a tarefa `to_pid`.
///
/// - `to_pid`: PID destino (deve estar registrado no IPC)
/// - `tag`: identificador semântico da mensagem (definido pelo protocolo)
/// - `payload`: ponteiro para até 64 bytes de dados (ou null)
///
/// Retorna `0` em sucesso, ou código de erro negativo (`ENOBUFS`).
///
/// # Safety
/// Se `payload` não for null, deve apontar para pelo menos 64 bytes válidos.
#[inline(always)]
pub unsafe fn sys_ipc_send(to_pid: u32, tag: u32, payload: *const u8) -> i64 {
    syscall3(5, to_pid as u64, tag as u64, payload as u64)
}

// ── Syscall 6 — ipc_recv ─────────────────────────────────────────────────────

/// Recebe a próxima mensagem IPC pendente para esta tarefa.
///
/// Copia a mensagem para `buf` (tamanho máximo `len` bytes).
/// Retorna o número de bytes copiados, ou `0` se não houver mensagens.
///
/// # Safety
/// `buf` deve apontar para `len` bytes graváveis em Ring 3.
#[inline(always)]
pub unsafe fn sys_ipc_recv(buf: *mut u8, len: u64) -> i64 {
    syscall3(6, buf as u64, len, 0)
}

// ── Syscall 7 — alloc_page ───────────────────────────────────────────────────

/// Aloca um frame físico de 4 KiB.
///
/// Retorna o **endereço físico** do frame alocado, ou `ENOMEM` (< 0).
///
/// # Nota
/// Esta syscall aloca memória física — o processo é responsável por
/// mapear o frame no seu espaço virtual antes de usá-lo.
#[inline(always)]
pub unsafe fn sys_alloc_page() -> i64 {
    syscall0(7)
}

// ── Syscall 8 — free_page ────────────────────────────────────────────────────

/// Libera um frame físico previamente alocado via `sys_alloc_page`.
///
/// # Safety
/// `phys` deve ser um endereço físico retornado por `sys_alloc_page`.
/// Liberar um frame inválido ou duas vezes causa comportamento indefinido.
#[inline(always)]
pub unsafe fn sys_free_page(phys: u64) {
    syscall3(8, phys, 0, 0);
}

// ── Syscall 9 — spawn ────────────────────────────────────────────────────────

/// Cria uma nova tarefa de **kernel (Ring 0)** apontando para `fn_ptr`.
///
/// Retorna o PID da nova tarefa, ou código de erro negativo.
///
/// # Safety
/// `fn_ptr` deve ser um ponteiro para uma função `fn()` válida no espaço
/// de kernel. Passar ponteiros de Ring 3 causará comportamento indefinido.
#[inline(always)]
pub unsafe fn sys_spawn_kernel(fn_ptr: usize) -> i64 {
    syscall3(9, fn_ptr as u64, 0, 0)
}

/// Cria uma nova tarefa de **usuário (Ring 3)** apontando para `fn_ptr`.
///
/// Retorna o PID da nova tarefa, ou código de erro negativo.
///
/// # Safety
/// `fn_ptr` deve ser um ponteiro para uma função `fn()` válida (mapeada
/// e acessível em Ring 3 no novo espaço de endereços da tarefa).
#[inline(always)]
pub unsafe fn sys_spawn_user(fn_ptr: usize) -> i64 {
    syscall3(9, fn_ptr as u64, 0, 3)
}

// ── Syscall 10 — waitpid ─────────────────────────────────────────────────────

/// Aguarda que a tarefa `pid` termine (estado `Zombie`) e coleta seu
/// código de saída.
///
/// Enquanto a tarefa não terminar, cede a CPU repetidamente via yield
/// (polling cooperativo). Retorna o exit code da tarefa coletada.
///
/// Retorna `EINVAL` (-22) se o PID não existir ou já foi coletado.
///
/// # Exemplo
/// ```no_run
/// let child_pid = sys_spawn_user(child_fn as usize) as u64;
/// let exit_code = sys_waitpid(child_pid);
/// ```
#[inline(always)]
pub unsafe fn sys_waitpid(pid: u64) -> i64 {
    syscall3(10, pid, 0, 0)
}

// ── Syscall 11 — sleep ───────────────────────────────────────────────────────

/// Coloca a tarefa atual em estado de dormência por pelo menos `ms` milissegundos.
///
/// Durante o sono, o escalonador remove a tarefa da fila de prontos,
/// garantindo que ela consuma rigorosamente **0 ciclos de CPU**.
#[inline(always)]
pub unsafe fn sys_sleep(ms: u64) -> i64 {
    syscall3(11, ms, 0, 0)
}

// ── Syscall 12 — jda_shm_alloc ───────────────────────────────────────────────

/// Aloca um buffer de página física compartilhado no subsistema JDA para transferência Zero-Copy.
///
/// Retorna o descritor (handle >= 0), ou código de erro negativo.
#[inline(always)]
pub unsafe fn sys_jda_shm_alloc(is_read_only: bool) -> i64 {
    syscall3(12, if is_read_only { 1 } else { 0 }, 0, 0)
}

// ── Syscall 13 — jda_shm_release ─────────────────────────────────────────────

/// Libera uma referência de buffer de página física compartilhado no JDA.
#[inline(always)]
pub unsafe fn sys_jda_shm_release(handle: u64) -> i64 {
    syscall3(13, handle, 0, 0)
}

// ── Syscall 14 — jda_submit ──────────────────────────────────────────────────

/// Submete um trabalho de fluxo de dados Zero-Copy diretamente sobre um buffer físico do JDA.
///
/// `job_type`: 1 = Fletcher32 Checksum, 2 = Acumulador, 3 = Transformação In-place XOR.
/// Retorna o resultado numérico do kernel, ou código de erro negativo.
#[inline(always)]
pub unsafe fn sys_jda_submit(job_type: u64, handle: u64, len: u64) -> i64 {
    syscall3(14, job_type, handle, len)
}

// ── Syscall 15 — open ────────────────────────────────────────────────────────

/// Abre um arquivo no VFS para leitura ou escrita e retorna um file descriptor (FD >= 3).
#[inline(always)]
pub unsafe fn sys_open(path: &str, flags: u32) -> i64 {
    syscall3(15, path.as_ptr() as u64, path.len() as u64, flags as u64)
}

// ── Syscall 16 — close ───────────────────────────────────────────────────────

/// Fecha um descritor de arquivo aberto previamente via `sys_open`.
#[inline(always)]
pub unsafe fn sys_close(fd: u64) -> i64 {
    syscall3(16, fd, 0, 0)
}

// ── Syscall 20 — window_create ────────────────────────────────────────────────

/// Cria uma nova janela de cliente no Aladdin Desktop.
/// `title`: título exibido na barra superior da janela.
/// `client_w`, `client_h`: largura e altura da área útil de desenho em pixels.
/// Retorna o ID da janela (>= 0), ou código de erro negativo.
#[inline(always)]
pub unsafe fn sys_window_create(title: &str, client_w: usize, client_h: usize) -> i64 {
    let packed = (((client_w & 0xFFFF) as u64) << 16) | ((client_h & 0xFFFF) as u64);
    syscall3(20, title.as_ptr() as u64, title.len() as u64, packed)
}

// ── Syscall 21 — window_update ────────────────────────────────────────────────

/// Apresenta um buffer de pixels (0x00RRGGBB) na janela de cliente.
/// `win_id`: identificador da janela retornado por `sys_window_create`.
/// `buffer`: fatia com exatamente `client_w * client_h` pixels.
#[inline(always)]
pub unsafe fn sys_window_update(win_id: usize, buffer: &[u32]) -> i64 {
    let len_bytes = buffer.len() * core::mem::size_of::<u32>();
    syscall3(21, win_id as u64, buffer.as_ptr() as u64, len_bytes as u64)
}

// ── Syscall 22 — window_close ─────────────────────────────────────────────────

/// Fecha a janela de cliente no Aladdin Desktop e libera seus recursos.
#[inline(always)]
pub unsafe fn sys_window_close(win_id: usize) -> i64 {
    syscall3(22, win_id as u64, 0, 0)
}

// ── Syscall 26 — jda_shm_share ───────────────────────────────────────────────

/// Compartilha um buffer físico com outro processo `target_pid` em modo USER_RO.
/// Se `make_owner_ro` for true, rebaixa também a página do processo chamador para USER_RO.
#[inline(always)]
pub unsafe fn sys_jda_shm_share(handle: u64, target_pid: usize, make_owner_ro: bool) -> i64 {
    syscall3(26, handle, target_pid as u64, if make_owner_ro { 1 } else { 0 })
}

// ── Syscall 27 — jda_ring_submit ─────────────────────────────────────────────

/// Submete um lote de operações (SQEs) ao anel assíncrono io_uring do JDA.
#[inline(always)]
pub unsafe fn sys_jda_ring_submit(sqe_ptr: *const crate::jda::ring::JdaSqe, count: usize) -> i64 {
    syscall3(27, sqe_ptr as u64, count as u64, 0)
}

// ── Syscall 28 — jda_ring_poll ───────────────────────────────────────────────

/// Coleta conclusões (CQEs) concluídas do anel assíncrono do JDA.
#[inline(always)]
pub unsafe fn sys_jda_ring_poll(cqe_ptr: *mut crate::jda::ring::JdaCqe, max_count: usize) -> i64 {
    syscall3(28, cqe_ptr as u64, max_count as u64, 0)
}

// ── Syscall 29 — jda_shm_get_vaddr ───────────────────────────────────────────

/// Obtém o endereço virtual do buffer mapeado no PML4 do processo de usuário.
#[inline(always)]
pub unsafe fn sys_jda_shm_get_vaddr(handle: u64) -> i64 {
    syscall3(29, handle, 0, 0)
}

// ── Syscall 38 — execve ───────────────────────────────────────────────────────

/// Substitui a imagem do processo atual pelo executável ELF especificado em `path`.
/// Mantém o mesmo PID e herda descritores de arquivo válidos.
#[inline(always)]
pub unsafe fn sys_execve(path: *const u8, argv: *const *const u8, envp: *const *const u8) -> i64 {
    syscall3(38, path as u64, argv as u64, envp as u64)
}

// ── Syscall 39 — spawn ────────────────────────────────────────────────────────

/// Cria um novo processo filho independente a partir do executável ELF em `path`.
/// Retorna o PID do novo processo em caso de sucesso ou código negativo em caso de erro.
#[inline(always)]
pub unsafe fn sys_spawn_process(path: *const u8, argv: *const *const u8, envp: *const *const u8) -> i64 {
    syscall3(39, path as u64, argv as u64, envp as u64)
}
