/// Jinn OS — System Call Interface
///
/// Dispatches `int 0x80` interrupts from userspace (or kernel tasks calling
/// `syscall::invoke`) into well-defined kernel services.
///
/// Calling convention (follows Linux i386 for familiarity):
///   RAX = syscall number
///   RDI = arg0,  RSI = arg1,  RDX = arg2
///   Return value in RAX (negative = error, following errno conventions).
///
/// Syscall table:
///   0  — exit(code: i32)          Terminate current task
///   1  — write(fd, buf, len)      Write bytes to a file descriptor
///   2  — read(fd, buf, len)       Read bytes from a file descriptor
///   3  — yield()                  Voluntarily yield CPU quantum
///   4  — getpid()                 Return current task PID
///   5  — ipc_send(pid, tag, ptr, len)  Send IPC message
///   6  — ipc_recv(buf, len)       Receive IPC message into buffer
///   7  — alloc_page()             Allocate a physical frame (returns phys addr)
///   8  — free_page(phys)          Free a physical frame
///   9  — spawn(fn_ptr, name_ptr)  Spawn a new kernel task
///   15 — open(path, len, flags)   Open file and return per-task FD (3..=7)
///   16 — close(fd)                Close file descriptor
///   20 — window_create(title, len, dim) Create Aladdin desktop window
///   21 — window_update(id, buf, len)   Update Aladdin window client buffer
///   22 — window_close(id)              Close Aladdin desktop window

use crate::ipc::{self, Message};
use crate::memory;
use crate::scheduler;

// ── Error Codes ───────────────────────────────────────────────────────────────
pub const EPERM:    isize = -1;   // Operation not permitted
pub const EBADF:    isize = -9;   // Bad file descriptor
pub const ENOMEM:   isize = -12;  // Out of memory
pub const EFAULT:   isize = -14;  // Bad address
pub const EINVAL:   isize = -22;  // Invalid argument
pub const EMFILE:   isize = -24;  // Too many open files
pub const ENOSPC:   isize = -28;  // No space left on device
pub const ENOSYS:   isize = -38;  // Function not implemented
pub const ENOBUFS:  isize = -105; // No buffer space available

pub const USER_SPACE_MIN: usize = 0x0000_0000_0001_0000; // 64 KiB NULL guard
pub const USER_SPACE_MAX: usize = 0x0000_8000_0000_0000; // Limite canônico de usuário

/// Verifica se a tarefa atual executa em modo de usuário (Ring 3).
fn is_current_task_user() -> bool {
    let pid = scheduler::current_pid();
    if let Some(Some(task)) = scheduler::get().tasks.get(pid) {
        task.ring == scheduler::Ring::User
    } else {
        false
    }
}

/// Valida se um buffer fornecido por Ring 3 está contido no espaço de usuário permitido
/// e se todas as páginas virtuais que o compõem estão devidamente mapeadas no PML4 ativo
/// com o bit USER ativo (e WRITABLE, se `write_access` for verdadeiro).
pub fn validate_user_buffer(ptr: *const u8, len: usize, write_access: bool) -> Result<(), isize> {
    if ptr.is_null() || len == 0 {
        return Err(EINVAL);
    }
    let addr = ptr as usize;
    let end = addr.checked_add(len).ok_or(EFAULT)?;
    if addr < USER_SPACE_MIN || end > USER_SPACE_MAX {
        return Err(EFAULT);
    }

    // Consulta as tabelas de páginas ativas da tarefa para verificar a presença de cada página
    let pml4_phys = crate::memory::paging::current_cr3();
    let pml4 = crate::memory::paging::phys_to_virt(pml4_phys) as *mut crate::memory::paging::PageTable;

    let start_page = (addr as u64) & !0xFFF;
    let end_page = ((end as u64).saturating_add(0xFFF)) & !0xFFF;
    let mut page = start_page;

    while page < end_page {
        unsafe {
            match crate::memory::paging::lookup_page(pml4, page) {
                Some((_phys, flags)) => {
                    if (flags & crate::memory::paging::USER) == 0 {
                        return Err(EFAULT);
                    }
                    if write_access && (flags & crate::memory::paging::WRITABLE) == 0 {
                        return Err(EFAULT);
                    }
                }
                None => return Err(EFAULT),
            }
        }
        page = page.saturating_add(4096);
    }

    Ok(())
}

// ── Dispatcher ────────────────────────────────────────────────────────────────

/// Chamado diretamente por `isr128_stub` com o ponteiro do frame completo de registradores.
/// Permite alternar de tarefa atomicamente em `sys_exit` (0) e `sys_yield` (3),
/// garantindo que processos encerrados jamais retornem para o espaço de usuário.
#[no_mangle]
pub extern "C" fn syscall_dispatch_framed(frame: *mut usize) -> usize {
    let nr   = unsafe { *frame.add(14) }; // saved RAX
    let arg0 = unsafe { *frame.add(8) };  // saved RDI
    let arg1 = unsafe { *frame.add(9) };  // saved RSI
    let arg2 = unsafe { *frame.add(12) }; // saved RDX

    match nr {
        0 => {
            // sys_exit: encerra a tarefa, libera todos os recursos e faz preempção imediata
            let pid = scheduler::current_pid();
            scheduler::get().exit_task(pid, arg0 as i32);
            scheduler::get().preempt(frame as usize)
        }
        3 => {
            // sys_yield: cede a CPU voluntariamente de forma imediata
            scheduler::get().preempt(frame as usize)
        }
        _ => {
            let ret = syscall_dispatch(nr, arg0, arg1, arg2);
            unsafe { *frame.add(14) = ret as usize; } // atualiza o RAX salvo com o código de retorno
            0 // 0 indica para o stub continuar na mesma tarefa
        }
    }
}

/// Called from `irq_dispatch` when vector == 0x80.
/// All arguments are passed through registers by the caller.
///
/// # Safety
/// Arguments are raw register values; the function validates them internally.
#[no_mangle]
pub extern "C" fn syscall_dispatch(
    nr:   usize,  // RAX
    arg0: usize,  // RDI
    arg1: usize,  // RSI
    arg2: usize,  // RDX
) -> isize {
    match nr {
        0  => sys_exit(arg0 as i32),
        1  => sys_write(arg0, arg1 as *const u8, arg2),
        2  => sys_read(arg0, arg1 as *mut u8, arg2),
        3  => sys_yield(),
        4  => sys_getpid(),
        5  => sys_ipc_send(arg0 as u32, arg1 as u32, arg2 as *const u8),
        6  => sys_ipc_recv(arg0 as *mut u8, arg1),
        7  => sys_alloc_page(),
        8  => sys_free_page(arg0 as u64),
        9  => sys_spawn(arg0, arg1 as *const u8, arg2),
        10 => sys_waitpid(arg0),
        11 => sys_sleep(arg0),
        12 => sys_jda_shm_alloc(arg0 != 0),
        13 => sys_jda_shm_release(arg0),
        14 => sys_jda_submit(arg0, arg1, arg2),
        15 => sys_open(arg0 as *const u8, arg1, arg2 as u32),
        16 => sys_close(arg0),
        20 => sys_window_create(arg0 as *const u8, arg1, arg2),
        21 => sys_window_update(arg0, arg1 as *const u32, arg2),
        22 => sys_window_close(arg0),
        25 => sys_res_stat(arg0, arg1 as *mut crate::resources::ProcessResourceStat, arg2),
        30 => sys_socket(arg0 as u32, arg1 as u32, arg2 as u32),
        31 => sys_connect(arg0, arg1 as *const u8, arg2),
        32 => sys_bind(arg0, arg1 as *const u8, arg2),
        33 => sys_listen(arg0, arg1),
        34 => sys_accept(arg0, arg1 as *mut u8, arg2),
        35 => sys_send(arg0, arg1 as *const u8, arg2, 0),
        36 => sys_recv(arg0, arg1 as *mut u8, arg2, 0),
        _  => ENOSYS,
    }
}

// ── Syscall Implementations ───────────────────────────────────────────────────

/// `exit(code)` — Mark the current task as Zombie and reschedule.
fn sys_exit(code: i32) -> isize {
    let pid = scheduler::current_pid();
    scheduler::get().exit_task(pid, code);
    ipc::unregister(pid);
    sys_yield();
    0
}

/// `write(fd, buf, len)` — Write `len` bytes from `buf` to file descriptor `fd`.
/// fd 1 = stdout (VGA), fd 2 = stderr (VGA red), fd 3..7 = VFS files.
fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    if buf.is_null() || len == 0 { return EINVAL; }
    if len > 65536 { return EINVAL; }

    if is_current_task_user() {
        if let Err(e) = validate_user_buffer(buf, len, false) {
            return e;
        }
    }

    match fd {
        1 | 2 => {
            let slice = unsafe { core::slice::from_raw_parts(buf, len) };
            let mut w = crate::vga::Writer::new();
            if fd == 2 {
                w.set_color(crate::vga::Color::LightRed, crate::vga::Color::Black);
            }
            for &byte in slice {
                w.write_byte(byte);
            }
            len as isize
        }
        3..=7 => {
            let pid = scheduler::current_pid();
            let sched = scheduler::get();
            if let Some(Some(task)) = sched.tasks.get_mut(pid) {
                if let Some(fd_entry) = &mut task.fds[fd] {
                    if let Ok(p_str) = core::str::from_utf8(&fd_entry.path[..fd_entry.path_len]) {
                        let slice = unsafe { core::slice::from_raw_parts(buf, len) };
                        match crate::vfs::write(p_str, fd_entry.vfs_handle, fd_entry.offset, slice) {
                            Ok(n) => {
                                fd_entry.offset += n as u64;
                                n as isize
                            }
                            Err(_) => EPERM,
                        }
                    } else {
                        EBADF
                    }
                } else {
                    EBADF
                }
            } else {
                EBADF
            }
        }
        _ => EINVAL,
    }
}

/// `read(fd, buf, len)` — Read up to `len` bytes into `buf`.
/// fd 0 = stdin (keyboard buffer), fd 3..7 = VFS files.
fn sys_read(fd: usize, buf: *mut u8, len: usize) -> isize {
    if buf.is_null() || len == 0 { return EINVAL; }

    if is_current_task_user() {
        if let Err(e) = validate_user_buffer(buf as *const u8, len, true) {
            return e;
        }
    }

    match fd {
        0 => {
            let mut count = 0usize;
            let slice = unsafe { core::slice::from_raw_parts_mut(buf, len) };
            for slot in slice.iter_mut() {
                match crate::keyboard_buffer::pop() {
                    Some(ch) => { *slot = ch; count += 1; }
                    None     => break,
                }
            }
            count as isize
        }
        3..=7 => {
            let pid = scheduler::current_pid();
            let sched = scheduler::get();
            if let Some(Some(task)) = sched.tasks.get_mut(pid) {
                if let Some(fd_entry) = &mut task.fds[fd] {
                    if let Ok(p_str) = core::str::from_utf8(&fd_entry.path[..fd_entry.path_len]) {
                        let slice = unsafe { core::slice::from_raw_parts_mut(buf, len) };
                        match crate::vfs::read(p_str, fd_entry.vfs_handle, fd_entry.offset, slice) {
                            Ok(n) => {
                                fd_entry.offset += n as u64;
                                n as isize
                            }
                            Err(_) => EBADF,
                        }
                    } else {
                        EBADF
                    }
                } else {
                    EBADF
                }
            } else {
                EBADF
            }
        }
        _ => EINVAL,
    }
}

/// `open(path_ptr, path_len, flags)` — Open a file from VFS and assign a task FD.
fn sys_open(path_ptr: *const u8, path_len: usize, flags: u32) -> isize {
    if path_ptr.is_null() || path_len == 0 || path_len > 64 {
        return EINVAL;
    }
    if is_current_task_user() {
        if let Err(e) = validate_user_buffer(path_ptr, path_len, false) {
            return e;
        }
    }

    let mut path_buf = [0u8; 64];
    unsafe {
        core::ptr::copy_nonoverlapping(path_ptr, path_buf.as_mut_ptr(), path_len);
    }
    let path_str = match core::str::from_utf8(&path_buf[..path_len]) {
        Ok(s) => s,
        Err(_) => return EINVAL,
    };

    let vfs_handle = match crate::vfs::open(path_str, flags) {
        Ok(h) => h,
        Err(_) => return -2, // ENOENT
    };

    let pid = scheduler::current_pid();
    let sched = scheduler::get();
    if let Some(Some(task)) = sched.tasks.get_mut(pid) {
        // Verifica limite de descritores de arquivos por processo
        if let Err(e) = task.resources.check_and_open_fd(&task.limits) {
            let _ = crate::vfs::close(path_str, vfs_handle);
            return e;
        }

        for fd in 3..scheduler::MAX_TASK_FDS {
            if task.fds[fd].is_none() {
                task.fds[fd] = Some(scheduler::TaskFileDescriptor {
                    vfs_handle,
                    path: path_buf,
                    path_len,
                    offset: 0,
                    flags,
                });
                return fd as isize;
            }
        }
        task.resources.close_fd();
        let _ = crate::vfs::close(path_str, vfs_handle);
        EMFILE
    } else {
        let _ = crate::vfs::close(path_str, vfs_handle);
        EPERM
    }
}

/// `close(fd)` — Close a previously opened file descriptor.
fn sys_close(fd: usize) -> isize {
    if fd < 3 || fd >= scheduler::MAX_TASK_FDS {
        return EBADF;
    }
    let pid = scheduler::current_pid();
    let sched = scheduler::get();
    if let Some(Some(task)) = sched.tasks.get_mut(pid) {
        if let Some(entry) = task.fds[fd].take() {
            task.resources.close_fd();
            if let Ok(p_str) = core::str::from_utf8(&entry.path[..entry.path_len]) {
                let _ = crate::vfs::close(p_str, entry.vfs_handle);
            }
            0
        } else {
            EBADF
        }
    } else {
        EBADF
    }
}

/// `yield()` — Voluntarily give up the CPU.
fn sys_yield() -> isize {
    unsafe { core::arch::asm!("int 32", options(nomem, nostack)); }
    0
}

/// `getpid()` — Return the PID of the calling task.
fn sys_getpid() -> isize {
    scheduler::current_pid() as isize
}

/// `ipc_send(to_pid, tag, payload_ptr)` — Send an IPC message.
fn sys_ipc_send(to_pid: u32, tag: u32, payload: *const u8) -> isize {
    let from = scheduler::current_pid() as u32;
    let data = if payload.is_null() {
        &[][..]
    } else {
        if is_current_task_user() {
            if let Err(e) = validate_user_buffer(payload, 64, false) {
                return e;
            }
        }
        unsafe { core::slice::from_raw_parts(payload, 64) }
    };
    let msg = Message::new(from, to_pid, tag, data);
    match ipc::send(msg) {
        Ok(())   => 0,
        Err(_)   => ENOBUFS,
    }
}

/// `ipc_recv(buf, len)` — Receive a pending IPC message. Returns bytes written.
fn sys_ipc_recv(buf: *mut u8, len: usize) -> isize {
    if is_current_task_user() {
        if let Err(e) = validate_user_buffer(buf as *const u8, len, true) {
            return e;
        }
    }
    let pid = scheduler::current_pid();
    match ipc::recv(pid) {
        None => 0,
        Some(msg) => {
            let copy_len = len.min(core::mem::size_of::<Message>());
            let msg_bytes = unsafe {
                core::slice::from_raw_parts(&msg as *const Message as *const u8, core::mem::size_of::<Message>())
            };
            unsafe {
                core::slice::from_raw_parts_mut(buf, copy_len)
                    .copy_from_slice(&msg_bytes[..copy_len]);
            }
            copy_len as isize
        }
    }
}

/// `alloc_page()` — Allocate one physical 4 KiB frame.
fn sys_alloc_page() -> isize {
    match memory::alloc_frame() {
        Some(phys) => phys as isize,
        None       => ENOMEM,
    }
}

/// `free_page(phys)` — Free a previously allocated physical frame.
fn sys_free_page(phys: u64) -> isize {
    memory::free_frame(phys);
    0
}

/// `spawn(fn_ptr, name_ptr, ring_flag)` — Spawn a new task.
/// ring_flag: 0 = Ring 0 (kernel task only for now).
/// Ring 3 spawn via syscall é desabilitado até termos um mecanismo
/// para passar dados ELF do espaço de usuário (ex: via filesystem).
fn sys_spawn(fn_ptr: usize, _name_ptr: *const u8, ring_flag: usize) -> isize {
    if ring_flag == 3 {
        // Spawn de processo Ring 3 via syscall não é mais suportado diretamente.
        // O processo precisa ser carregado via ELF (módulo Limine ou futuro filesystem).
        return -1;
    }
    let entry: fn() = unsafe { core::mem::transmute(fn_ptr) };
    let sched = scheduler::get();
    let pid = sched.add_task("spawned-k0", entry);
    ipc::register(pid);
    pid as isize
}

/// `waitpid(pid)` — Wait for a task to become Zombie, collect it and return exit code.
///
/// Se o alvo já terminou, retorna imediatamente.
/// Caso contrário, bloqueia via `WaitingOn(pid)` e cede a CPU.
/// O escalonador desbloqueará esta tarefa quando o alvo chamar `exit`.
fn sys_waitpid(pid: usize) -> isize {
    let my_pid = scheduler::current_pid();
    let sched = scheduler::get();

    // Tenta coletar imediatamente
    if let Some(code) = sched.collect_zombie(pid) {
        return code as isize;
    }

    // Bloqueia esperando o alvo terminar
    if !sched.wait_on(my_pid, pid) {
        // Tarefa existe mas ainda não terminou — yield para ser acordado depois
        unsafe { core::arch::asm!("int 32", options(nomem, nostack)); }

        // Ao acordar, o alvo já virou Zombie. Coleta.
        let sched = scheduler::get();
        if let Some(code) = sched.collect_zombie(pid) {
            return code as isize;
        }
    }

    // Se wait_on retornou true, o alvo já estava Zombie
    let sched = scheduler::get();
    match sched.collect_zombie(pid) {
        Some(code) => code as isize,
        None => EINVAL, // Tarefa não encontrada
    }
}

/// `sleep(ms)` — Coloca a tarefa atual em estado de dormência por pelo menos `ms` milissegundos.
/// A tarefa consome rigorosamente 0 ciclos de CPU enquanto dorme.
fn sys_sleep(ms: usize) -> isize {
    // 100 Hz timer = 10 ms por tick
    let ticks = (ms as u64 + 9) / 10;
    let wake_tick = crate::timer::ticks().saturating_add(ticks);
    let pid = scheduler::current_pid();
    scheduler::get().sleep_task(pid, wake_tick);
    sys_yield();
    0
}

/// `sys_jda_shm_alloc(is_read_only)` — Aloca um buffer físico no pool do JDA.
/// Retorna o identificador do buffer (handle >= 0), ou código de erro negativo.
fn sys_jda_shm_alloc(is_read_only: bool) -> isize {
    let pid = scheduler::current_pid();
    let sched = scheduler::get();
    if let Some(Some(task)) = sched.tasks.get_mut(pid) {
        if let Err(e) = task.resources.check_and_alloc_jda(&task.limits) {
            return e;
        }
    }
    let mut pool = crate::jda::shmem::SHARED_BUFFER_POOL.lock();
    match pool.allocate(pid, is_read_only) {
        Ok(handle) => handle as isize,
        Err(_) => {
            if let Some(Some(task)) = scheduler::get().tasks.get_mut(pid) {
                task.resources.release_jda();
            }
            ENOMEM
        }
    }
}

/// `sys_jda_shm_release(handle)` — Libera a referência do buffer físico compartilhado no JDA.
fn sys_jda_shm_release(handle: usize) -> isize {
    let pid = scheduler::current_pid();
    let mut pool = crate::jda::shmem::SHARED_BUFFER_POOL.lock();
    match pool.release(handle) {
        Ok(()) => {
            if let Some(Some(task)) = scheduler::get().tasks.get_mut(pid) {
                task.resources.release_jda();
            }
            0
        }
        Err(_) => EINVAL,
    }
}

/// `sys_jda_submit(job_type, handle, len)` — Submete um job de processamento de fluxo de dados (JDA).
/// Processa o buffer físico Zero-Copy identificado por `handle` sem duplicar memória.
/// Retorna o resultado numérico do processamento (ex: checksum ou soma acumulada) ou erro negativo.
fn sys_jda_submit(job_type: usize, handle: usize, len: usize) -> isize {
    let pool = crate::jda::shmem::SHARED_BUFFER_POOL.lock();
    let phys = match pool.get_phys_addr(handle) {
        Some(p) => p,
        None => return EINVAL,
    };
    let data_len = len.min(4096);
    let virt = crate::memory::paging::phys_to_virt(phys) as *mut u8;

    let res = match job_type {
        // Job 1: Checksum Fletcher32
        1 => {
            let slice = unsafe { core::slice::from_raw_parts(virt as *const u8, data_len) };
            let mut c0 = 0u32;
            let mut c1 = 0u32;
            for &b in slice {
                c0 = (c0 + b as u32) % 255;
                c1 = (c1 + c0) % 255;
            }
            ((c1 << 8) | c0) as isize
        }
        // Job 2: Acumulador de bytes (Soma)
        2 => {
            let slice = unsafe { core::slice::from_raw_parts(virt as *const u8, data_len) };
            let mut sum: u32 = 0;
            for &b in slice {
                sum = sum.wrapping_add(b as u32);
            }
            sum as isize
        }
        // Job 3: Transformação XOR in-place com chave 0x5A
        3 => {
            let slice = unsafe { core::slice::from_raw_parts_mut(virt, data_len) };
            for b in slice.iter_mut() {
                *b ^= 0x5A;
            }
            data_len as isize
        }
        _ => return ENOSYS,
    };

    crate::jda::telemetry::record_data_transfer(data_len, true);
    crate::jda::telemetry::record_operation_completed(100);

    res
}

// ── Aladdin Graphics Syscalls ─────────────────────────────────────────────────

/// `sys_window_create(title_ptr, title_len, packed_dim)` — Cria uma janela no Aladdin Desktop.
/// `packed_dim`: `(width << 16) | (height & 0xFFFF)`.
/// Retorna o ID da janela (>= 0) em caso de sucesso, ou código de erro negativo.
fn sys_window_create(title_ptr: *const u8, title_len: usize, packed_dim: usize) -> isize {
    let pid = scheduler::current_pid();
    if is_current_task_user() {
        if let Err(e) = validate_user_buffer(title_ptr, title_len, false) {
            return e;
        }
    }
    if title_ptr.is_null() || title_len == 0 || title_len > 48 {
        return EINVAL;
    }
    let title_slice = unsafe { core::slice::from_raw_parts(title_ptr, title_len) };
    let title_str = match core::str::from_utf8(title_slice) {
        Ok(s) => s,
        Err(_) => return EINVAL,
    };
    let width = (packed_dim >> 16) & 0xFFFF;
    let height = packed_dim & 0xFFFF;
    if width == 0 || height == 0 || width > 1024 || height > 768 {
        return EINVAL;
    }

    match crate::gui::window_create(pid, title_str, width, height) {
        Ok(id) => id as isize,
        Err(err) => err,
    }
}

/// `sys_window_update(win_id, buffer_ptr, buffer_len_bytes)` — Apresenta pixels na janela.
fn sys_window_update(win_id: usize, buffer_ptr: *const u32, buffer_len_bytes: usize) -> isize {
    let pid = scheduler::current_pid();
    if is_current_task_user() {
        if let Err(e) = validate_user_buffer(buffer_ptr as *const u8, buffer_len_bytes, false) {
            return e;
        }
    }
    if buffer_ptr.is_null() || buffer_len_bytes == 0 || (buffer_len_bytes % 4) != 0 {
        return EINVAL;
    }
    let pixel_count = buffer_len_bytes / 4;
    let pixel_slice = unsafe { core::slice::from_raw_parts(buffer_ptr, pixel_count) };

    match crate::gui::window_update(pid, win_id, pixel_slice) {
        Ok(()) => 0,
        Err(err) => err,
    }
}

/// `sys_window_close(win_id)` — Fecha a janela pertencente ao processo.
fn sys_window_close(win_id: usize) -> isize {
    let pid = scheduler::current_pid();
    match crate::gui::window_close(pid, win_id) {
        Ok(()) => 0,
        Err(err) => err,
    }
}

/// `sys_res_stat(target_pid, stat_buf_ptr, buf_len)` — Consulta as métricas de recursos de um processo.
/// Se target_pid == 0, consulta a própria tarefa corrente.
fn sys_res_stat(target_pid: usize, stat_buf_ptr: *mut crate::resources::ProcessResourceStat, buf_len: usize) -> isize {
    let stat_size = core::mem::size_of::<crate::resources::ProcessResourceStat>();
    if stat_buf_ptr.is_null() || buf_len < stat_size {
        return EINVAL;
    }
    if is_current_task_user() {
        if let Err(e) = validate_user_buffer(stat_buf_ptr as *const u8, stat_size, true) {
            return e;
        }
    }

    let pid = if target_pid == 0 {
        scheduler::current_pid()
    } else {
        target_pid
    };

    let sched = scheduler::get();
    if let Some(Some(task)) = sched.tasks.get(pid) {
        let state_val = match task.state {
            scheduler::TaskState::Ready => 1,
            scheduler::TaskState::Running => 2,
            scheduler::TaskState::Blocked => 3,
            scheduler::TaskState::Sleeping(_) => 4,
            scheduler::TaskState::WaitingOn(_) => 5,
            scheduler::TaskState::Zombie => 6,
            scheduler::TaskState::Completed => 7,
        };

        let stat = crate::resources::ProcessResourceStat {
            pid: task.id as u32,
            state: state_val,
            allocated_frames: task.resources.allocated_frames as u32,
            peak_allocated_frames: task.resources.peak_allocated_frames as u32,
            open_fds: task.resources.open_fds as u32,
            open_windows: task.resources.open_windows as u32,
            open_jda_buffers: task.resources.open_jda_buffers as u32,
            open_sockets: task.resources.open_sockets as u32,
            cpu_ticks: task.cpu_ticks,
            ctx_switches: task.ctx_switches,
            limit_violations: task.resources.limit_violations,
            last_violation_code: task.resources.last_violation_code,
            max_frames: task.limits.max_frames as u32,
            max_fds: task.limits.max_fds as u32,
            max_windows: task.limits.max_windows as u32,
            max_sockets: task.limits.max_sockets as u32,
        };

        unsafe {
            stat_buf_ptr.write_volatile(stat);
        }
        0
    } else {
        -3 // ESRCH (No such process)
    }
}

// ── Syscalls de Rede e Sockets (Camadas 3 e 4) ────────────────────────────────

/// `sys_socket(domain, type, protocol)` — Cria um endpoint de comunicação de rede.
fn sys_socket(domain: u32, sock_type: u32, protocol: u32) -> isize {
    let pid = scheduler::current_pid();
    match crate::net::socket::create_socket(pid, domain, sock_type, protocol) {
        Ok(fd) => fd as isize,
        Err(e) => e,
    }
}

/// `sys_connect(sockfd, addr_ptr, addrlen)` — Conecta a um IP e porta remotos.
fn sys_connect(sockfd: usize, addr_ptr: *const u8, addrlen: usize) -> isize {
    let pid = scheduler::current_pid();
    let sock_addr_size = core::mem::size_of::<crate::net::socket::SockAddrIn>();
    if addr_ptr.is_null() || addrlen < sock_addr_size {
        return EINVAL;
    }
    if is_current_task_user() {
        if let Err(e) = validate_user_buffer(addr_ptr, sock_addr_size, false) {
            return e;
        }
    }
    let addr = unsafe { &*(addr_ptr as *const crate::net::socket::SockAddrIn) };
    let remote_port = u16::from_be(addr.sin_port);
    match crate::net::socket::connect_socket(pid, sockfd, addr.sin_addr, remote_port) {
        Ok(()) => 0,
        Err(e) => e,
    }
}

/// `sys_bind(sockfd, addr_ptr, addrlen)` — Associa socket a uma porta local.
fn sys_bind(_sockfd: usize, _addr_ptr: *const u8, _addrlen: usize) -> isize {
    0
}

/// `sys_listen(sockfd, backlog)` — Habilita escuta de conexões.
fn sys_listen(_sockfd: usize, _backlog: usize) -> isize {
    0
}

/// `sys_accept(sockfd, addr_ptr, addrlen)` — Aceita conexão de entrada.
fn sys_accept(_sockfd: usize, _addr_ptr: *mut u8, _addrlen: usize) -> isize {
    0
}

/// `sys_send(sockfd, buf_ptr, len, flags)` — Envia dados pelo socket.
fn sys_send(sockfd: usize, buf_ptr: *const u8, len: usize, _flags: usize) -> isize {
    let pid = scheduler::current_pid();
    if buf_ptr.is_null() || len == 0 {
        return EINVAL;
    }
    if is_current_task_user() {
        if let Err(e) = validate_user_buffer(buf_ptr, len, false) {
            return e;
        }
    }
    let data = unsafe { core::slice::from_raw_parts(buf_ptr, len) };
    match crate::net::socket::send_socket(pid, sockfd, data) {
        Ok(sent) => sent as isize,
        Err(e) => e,
    }
}

/// `sys_recv(sockfd, buf_ptr, len, flags)` — Recebe dados do socket.
fn sys_recv(_sockfd: usize, buf_ptr: *mut u8, len: usize, _flags: usize) -> isize {
    if buf_ptr.is_null() || len == 0 {
        return EINVAL;
    }
    if is_current_task_user() {
        if let Err(e) = validate_user_buffer(buf_ptr as *const u8, len, true) {
            return e;
        }
    }
    0
}

// ── Kernel-internal invoke ────────────────────────────────────────────────────

/// Invoke a syscall directly from kernel code (no `int 0x80`).
#[inline]
pub fn invoke(nr: usize, arg0: usize, arg1: usize, arg2: usize) -> isize {
    syscall_dispatch(nr, arg0, arg1, arg2)
}
