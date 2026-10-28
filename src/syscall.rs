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
///   17 — stat(path, len, stat_ptr) Query file or directory metadata
///   18 — fstat(fd, stat_ptr)      Query open file or socket descriptor metadata
///   19 — lseek(fd, offset, whence) Reposition file read/write offset
///   20 — window_create(title, len, dim) Create Aladdin desktop window
///   21 — window_update(id, buf, len)   Update Aladdin window client buffer
///   22 — window_close(id)              Close Aladdin desktop window
///   25 — res_stat(pid, stat_ptr, len)  Query process resource limits & telemetry
///   30 — socket(domain, type, proto)   Create network socket endpoint
///   31 — connect(fd, addr, len)        Connect to remote network peer
///   32 — bind(fd, addr, len)           Bind socket to local port
///   33 — listen(fd, backlog)           Set TCP socket to passive listen
///   34 — accept(fd, addr, len)         Accept incoming TCP connection
///   35 — send(fd, buf, len, flags)     Send data over socket
///   36 — recv(fd, buf, len, flags)     Receive data from socket
///   37 — jinnwall_ctrl(cmd, arg0, arg1) Firewall JinnWall control & telemetry

use crate::ipc::{self, Message};
use crate::memory;
use crate::scheduler;

// ── Error Codes ───────────────────────────────────────────────────────────────
pub const EPERM:        isize = -1;   // Operation not permitted
pub const ENOENT:       isize = -2;   // No such file or directory
pub const ESRCH:        isize = -3;   // No such process
pub const EIO:          isize = -5;   // I/O error
pub const EBADF:        isize = -9;   // Bad file descriptor
pub const ECHILD:       isize = -10;  // No child processes
pub const EAGAIN:       isize = -11;  // Resource temporarily unavailable / Would block
pub const ENOMEM:       isize = -12;  // Out of memory
pub const EFAULT:       isize = -14;  // Bad address
pub const EBUSY:        isize = -16;  // Device or resource busy
pub const EINVAL:       isize = -22;  // Invalid argument
pub const EMFILE:       isize = -24;  // Too many open files
pub const ENOSPC:       isize = -28;  // No space left on device
pub const ESPIPE:       isize = -29;  // Illegal seek
pub const ENOSYS:       isize = -38;  // Function not implemented
pub const EADDRINUSE:   isize = -98;  // Address already in use
pub const ECONNRESET:   isize = -104; // Connection reset by peer
pub const ENOBUFS:      isize = -105; // No buffer space available
pub const ETIMEDOUT:    isize = -110; // Connection timed out
pub const ECONNREFUSED: isize = -111; // Connection refused
pub const EDQUOT:       isize = -122; // Quota exceeded

// ── Whence Flags (lseek) ─────────────────────────────────────────────────────
pub const SEEK_SET: usize = 0;
pub const SEEK_CUR: usize = 1;
pub const SEEK_END: usize = 2;

// ── VFS File Metadata (stat/fstat) ───────────────────────────────────────────
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStat {
    pub size: u64,          // Tamanho do arquivo em bytes
    pub inode: u64,         // Número do inode ou identificador único
    pub node_type: u32,     // 0 = File, 1 = Directory, 2 = CharDevice, 3 = BlockDevice, 4 = Symlink, 5 = Socket
    pub mode: u32,          // Modo POSIX (ex: 0o100644, 0o040755)
    pub blksize: u32,       // Tamanho de bloco ideal (512)
    pub blocks: u32,        // Blocos de 512B alocados
    pub atime: u64,         // Timestamp de acesso
    pub mtime: u64,         // Timestamp de modificação
    pub ctime: u64,         // Timestamp de criação
}

// ── JinnWall Firewall Telemetry (jinnwall_ctrl) ──────────────────────────────
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JinnWallStats {
    pub inspected: u64,      // Total de pacotes avaliados
    pub accepted: u64,       // Total de pacotes aceitos
    pub dropped: u64,        // Total de pacotes descartados
    pub active_conns: u32,   // Conexões ativas no conntrack
    pub profile: u32,        // 0 = PublicSecure, 1 = Development
}

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
            scheduler::set_yielded();
            scheduler::get().preempt(frame as usize)
        }
        11 => {
            // sys_sleep: coloca a tarefa em dormência e cede a CPU de forma previsível
            let ret = sys_sleep(arg0);
            unsafe { *frame.add(14) = ret as usize; }
            scheduler::set_yielded();
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
        17 => sys_stat(arg0 as *const u8, arg1, arg2 as *mut FileStat),
        18 => sys_fstat(arg0, arg1 as *mut FileStat),
        19 => sys_lseek(arg0, arg1 as i64, arg2),
        20 => sys_window_create(arg0 as *const u8, arg1, arg2),
        21 => sys_window_update(arg0, arg1 as *const u32, arg2),
        22 => sys_window_close(arg0),
        23 => sys_sync(),
        25 => sys_res_stat(arg0, arg1 as *mut crate::resources::ProcessResourceStat, arg2),
        26 => sys_jda_shm_share(arg0, arg1, arg2 != 0),
        27 => sys_jda_ring_submit(arg0 as *const crate::jda::ring::JdaSqe, arg1),
        28 => sys_jda_ring_poll(arg0 as *mut crate::jda::ring::JdaCqe, arg1),
        29 => sys_jda_shm_get_vaddr(arg0),
        30 => sys_socket(arg0 as u32, arg1 as u32, arg2 as u32),
        31 => sys_connect(arg0, arg1 as *const u8, arg2),
        32 => sys_bind(arg0, arg1 as *const u8, arg2),
        33 => sys_listen(arg0, arg1),
        34 => sys_accept(arg0, arg1 as *mut u8, arg2),
        35 => sys_send(arg0, arg1 as *const u8, arg2, 0),
        36 => sys_recv(arg0, arg1 as *mut u8, arg2, 0),
        37 => sys_jinnwall_ctrl(arg0, arg1, arg2),
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
                if let Some(desc) = &mut task.fds[fd] {
                    match desc {
                        scheduler::TaskDescriptor::File(fd_entry) => {
                            if let Ok(p_str) = core::str::from_utf8(&fd_entry.path[..fd_entry.path_len]) {
                                let slice = unsafe { core::slice::from_raw_parts(buf, len) };
                                match crate::vfs::write(p_str, fd_entry.vfs_handle, fd_entry.offset, slice) {
                                    Ok(n) => {
                                        fd_entry.offset += n as u64;
                                        n as isize
                                    }
                                    Err(e) => match e {
                                        crate::vfs::VfsError::NoSpace => ENOSPC,
                                        crate::vfs::VfsError::PermissionDenied => EPERM,
                                        crate::vfs::VfsError::InvalidPath => EINVAL,
                                        crate::vfs::VfsError::IoError => EIO,
                                        _ => EIO,
                                    },
                                }
                            } else {
                                EBADF
                            }
                        }
                        scheduler::TaskDescriptor::Socket(sock_idx) => {
                            let sock_id = *sock_idx;
                            let slice = unsafe { core::slice::from_raw_parts(buf, len) };
                            match crate::net::socket::send_socket(pid, sock_id, slice) {
                                Ok(n) => n as isize,
                                Err(e) => e,
                            }
                        }
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
/// fd 0 = stdin (keyboard buffer), fd 3..7 = VFS files ou sockets de rede.
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
                if let Some(desc) = &mut task.fds[fd] {
                    match desc {
                        scheduler::TaskDescriptor::File(fd_entry) => {
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
                        }
                        scheduler::TaskDescriptor::Socket(sock_idx) => {
                            let sock_id = *sock_idx;
                            let slice = unsafe { core::slice::from_raw_parts_mut(buf, len) };
                            match crate::net::socket::recv_socket(pid, sock_id, slice) {
                                Ok(n) => n as isize,
                                Err(e) => e,
                            }
                        }
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
                task.fds[fd] = Some(scheduler::TaskDescriptor::File(scheduler::TaskFileDescriptor {
                    vfs_handle,
                    path: path_buf,
                    path_len,
                    offset: 0,
                    flags,
                }));
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

/// `close(fd)` — Close a previously opened file or socket descriptor from the unified table.
fn sys_close(fd: usize) -> isize {
    if fd < 3 || fd >= scheduler::MAX_TASK_FDS {
        return EBADF;
    }

    let pid = scheduler::current_pid();
    let sched = scheduler::get();
    if let Some(Some(task)) = sched.tasks.get_mut(pid) {
        if let Some(desc) = task.fds[fd].take() {
            match desc {
                scheduler::TaskDescriptor::File(entry) => {
                    task.resources.close_fd();
                    if let Ok(p_str) = core::str::from_utf8(&entry.path[..entry.path_len]) {
                        let _ = crate::vfs::close(p_str, entry.vfs_handle);
                    }
                    0
                }
                scheduler::TaskDescriptor::Socket(sock_idx) => {
                    match crate::net::socket::close_socket(pid, sock_idx) {
                        Ok(()) => 0,
                        Err(e) => e,
                    }
                }
            }
        } else {
            EBADF // Previne double-close e descritor não alocado
        }
    } else {
        EBADF
    }
}

const _: () = {
    assert!(core::mem::size_of::<FileStat>() == 56);
    assert!(core::mem::align_of::<FileStat>() == 8);
    assert!(core::mem::size_of::<JinnWallStats>() == 32);
    assert!(core::mem::align_of::<JinnWallStats>() == 8);
};

/// Converte metadados internos do VFS (`vfs::Stat`) para a estrutura normativa `FileStat`.
///
/// # Semântica de Timestamps:
/// `atime`, `mtime` e `ctime` registram **ticks monotônicos do PIT (100 Hz = 10 ms por tick)**
/// contados a partir da inicialização do microkernel. Não representam segundos de calendário Unix Epoch
/// devido à ausência de driver de RTC na arquitetura atual.
fn map_vfs_stat_to_filestat(stat: &crate::vfs::Stat) -> FileStat {
    let (node_type, mode) = match stat.node_type {
        crate::vfs::NodeType::File => (0u32, 0o100644u32),
        crate::vfs::NodeType::Directory => (1u32, 0o040755u32),
        crate::vfs::NodeType::CharDevice => (2u32, 0o020666u32),
        crate::vfs::NodeType::BlockDevice => (3u32, 0o060660u32),
        crate::vfs::NodeType::Symlink => (4u32, 0o120777u32),
    };
    FileStat {
        size: stat.size,
        inode: stat.inode,
        node_type,
        mode,
        blksize: 512,
        blocks: ((stat.size.saturating_add(511)) / 512) as u32,
        atime: crate::timer::ticks(),
        mtime: crate::timer::ticks(),
        ctime: crate::timer::ticks(),
    }
}

/// `stat(path_ptr, path_len, stat_ptr)` — Consulta metadados de arquivo ou diretório pelo caminho.
fn sys_stat(path_ptr: *const u8, path_len: usize, stat_ptr: *mut FileStat) -> isize {
    if path_ptr.is_null() || stat_ptr.is_null() || path_len == 0 || path_len > crate::vfs::MAX_PATH_LEN {
        return EINVAL;
    }
    if is_current_task_user() {
        if let Err(e) = validate_user_buffer(path_ptr, path_len, false) {
            return e;
        }
        if let Err(e) = validate_user_buffer(stat_ptr as *const u8, core::mem::size_of::<FileStat>(), true) {
            return e;
        }
    }
    let mut path_buf = [0u8; crate::vfs::MAX_PATH_LEN];
    unsafe {
        core::ptr::copy_nonoverlapping(path_ptr, path_buf.as_mut_ptr(), path_len);
    }
    let raw_str = match core::str::from_utf8(&path_buf[..path_len]) {
        Ok(s) => s,
        Err(_) => return EINVAL,
    };
    let path_str = raw_str.trim_end_matches('\0');
    if path_str.is_empty() || !path_str.starts_with('/') {
        return ENOENT;
    }

    match crate::vfs::stat(path_str) {
        Ok(vfs_stat) => {
            let fstat = map_vfs_stat_to_filestat(&vfs_stat);
            unsafe {
                core::ptr::write_unaligned(stat_ptr, fstat);
            }
            0
        }
        Err(e) => match e {
            crate::vfs::VfsError::NotFound => ENOENT,
            crate::vfs::VfsError::PermissionDenied => EPERM,
            crate::vfs::VfsError::InvalidPath => EINVAL,
            crate::vfs::VfsError::NotADirectory => ENOENT,
            _ => ENOENT,
        },
    }
}

/// `fstat(fd, stat_ptr)` — Consulta metadados de descritor aberto (arquivo, dispositivo ou socket).
fn sys_fstat(fd: usize, stat_ptr: *mut FileStat) -> isize {
    if stat_ptr.is_null() {
        return EINVAL;
    }
    if fd >= scheduler::MAX_TASK_FDS {
        return EBADF;
    }
    if is_current_task_user() {
        if let Err(e) = validate_user_buffer(stat_ptr as *const u8, core::mem::size_of::<FileStat>(), true) {
            return e;
        }
    }

    // Suporte aos descritores padrão do sistema (0 = stdin, 1 = stdout, 2 = stderr)
    // Tratados como dispositivos de caractere (S_IFCHR)
    if fd < 3 {
        let fstat = FileStat {
            size: 0,
            inode: fd as u64,
            node_type: 2, // CharDevice
            mode: 0o020666,
            blksize: 512,
            blocks: 0,
            atime: crate::timer::ticks(),
            mtime: crate::timer::ticks(),
            ctime: crate::timer::ticks(),
        };
        unsafe {
            core::ptr::write_unaligned(stat_ptr, fstat);
        }
        return 0;
    }

    let pid = scheduler::current_pid();
    let sched = scheduler::get();
    let task = match sched.tasks.get(pid).and_then(|t| t.as_ref()) {
        Some(t) => t,
        None => return EPERM,
    };

    let desc = match task.fds[fd] {
        Some(ref d) => d,
        None => return EBADF,
    };

    match desc {
        scheduler::TaskDescriptor::File(entry) => {
            let raw_str = match core::str::from_utf8(&entry.path[..entry.path_len]) {
                Ok(s) => s,
                Err(_) => return EBADF,
            };
            let path_str = raw_str.trim_end_matches('\0');
            match crate::vfs::stat(path_str) {
                Ok(vfs_stat) => {
                    let fstat = map_vfs_stat_to_filestat(&vfs_stat);
                    unsafe {
                        core::ptr::write_unaligned(stat_ptr, fstat);
                    }
                    0
                }
                Err(_) => ENOENT,
            }
        }
        scheduler::TaskDescriptor::Socket(sock_idx) => {
            let fstat = FileStat {
                size: 0,
                inode: *sock_idx as u64,
                node_type: 5, // Socket
                mode: 0o140666,
                blksize: 512,
                blocks: 0,
                atime: crate::timer::ticks(),
                mtime: crate::timer::ticks(),
                ctime: crate::timer::ticks(),
            };
            unsafe {
                core::ptr::write_unaligned(stat_ptr, fstat);
            }
            0
        }
    }
}

/// `lseek(fd, offset, whence)` — Reposiciona o deslocamento de leitura/escrita do descritor de arquivo.
fn sys_lseek(fd: usize, offset: i64, whence: usize) -> isize {
    // Console streams (0 = stdin, 1 = stdout, 2 = stderr) são dispositivos de caractere não posicionáveis
    if fd < 3 {
        return ESPIPE;
    }
    if fd >= scheduler::MAX_TASK_FDS {
        return EBADF;
    }

    let pid = scheduler::current_pid();
    let sched = scheduler::get();
    let task = match sched.tasks.get_mut(pid).and_then(|t| t.as_mut()) {
        Some(t) => t,
        None => return EPERM,
    };

    let desc = match task.fds[fd].as_mut() {
        Some(d) => d,
        None => return EBADF,
    };

    let entry = match desc {
        scheduler::TaskDescriptor::File(ref mut e) => e,
        scheduler::TaskDescriptor::Socket(_) => return ESPIPE,
    };

    let new_offset: i64 = match whence {
        SEEK_SET => offset,
        SEEK_CUR => {
            let curr = match i64::try_from(entry.offset) {
                Ok(c) => c,
                Err(_) => return EINVAL,
            };
            match curr.checked_add(offset) {
                Some(val) => val,
                None => return EINVAL,
            }
        }
        SEEK_END => {
            let raw_str = match core::str::from_utf8(&entry.path[..entry.path_len]) {
                Ok(s) => s,
                Err(_) => return EBADF,
            };
            let path_str = raw_str.trim_end_matches('\0');
            let file_size = match crate::vfs::stat(path_str) {
                Ok(s) => match i64::try_from(s.size) {
                    Ok(sz) => sz,
                    Err(_) => return EINVAL,
                },
                Err(_) => return ENOENT,
            };
            match file_size.checked_add(offset) {
                Some(val) => val,
                None => return EINVAL,
            }
        }
        _ => return EINVAL,
    };

    if new_offset < 0 {
        return EINVAL;
    }

    entry.offset = new_offset as u64;
    new_offset as isize
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

/// `alloc_page()` — Allocate one physical 4 KiB frame for the process.
/// In Ring 3, maps the frame into the task's user space (heap base: USER_HEAP_BASE)
/// and enforces `max_frames` limit and dynamic page metadata registration.
fn sys_alloc_page() -> isize {
    let pid = scheduler::current_pid();
    let sched = scheduler::get();
    if let Some(Some(task)) = sched.tasks.get_mut(pid) {
        // 1. Quota de frames verificada ANTES de comprometer o recurso
        if let Err(e) = task.resources.check_and_alloc_frames(1, &task.limits) {
            return e;
        }

        // 2. Procura um slot livre no vetor de páginas dinâmicas da tarefa
        let mut slot_idx = None;
        for (i, p) in task.dynamic_pages.iter().enumerate() {
            if *p == 0 {
                slot_idx = Some(i);
                break;
            }
        }
        let slot_idx = match slot_idx {
            Some(i) => i,
            None => {
                task.resources.free_frames(1);
                return ENOMEM;
            }
        };

        match memory::alloc_frame() {
            Some(phys) => {
                if task.ring == scheduler::Ring::User {
                    let pml4 = crate::memory::paging::phys_to_virt(task.pml4_phys) as *mut crate::memory::paging::PageTable;
                    // Procura endereço virtual livre estritamente na faixa USER_HEAP_BASE..USER_HEAP_MAX
                    let mut vaddr = scheduler::USER_HEAP_BASE;
                    let mut found = false;
                    while vaddr < scheduler::USER_HEAP_MAX {
                        if unsafe { crate::memory::paging::lookup_page(pml4, vaddr) }.is_none() {
                            found = true;
                            break;
                        }
                        vaddr += 4096;
                    }

                    if !found {
                        // Rollback integral se a faixa de heap virtual estiver esgotada
                        memory::free_frame(phys);
                        task.resources.free_frames(1);
                        return ENOMEM;
                    }

                    unsafe {
                        crate::memory::paging::map_page(pml4, vaddr, phys, crate::memory::paging::USER_RW);
                        let virt_ptr = crate::memory::paging::phys_to_virt(phys) as *mut u8;
                        core::ptr::write_bytes(virt_ptr, 0, 4096);
                    }

                    // Registra a propriedade da página dinâmica na tarefa
                    task.dynamic_pages[slot_idx] = vaddr;
                    vaddr as isize
                } else {
                    phys as isize
                }
            }
            None => {
                task.resources.free_frames(1);
                ENOMEM
            }
        }
    } else {
        ENOMEM
    }
}

/// `free_page(addr)` — Free a previously allocated page or frame.
/// Em Ring 3, exige validação de propriedade e alinhamento, rejeitando páginas
/// pertencentes à pilha, ELF, kernel ou já liberadas (proteção contra double-free).
fn sys_free_page(addr: u64) -> isize {
    // 1. Alinhamento obrigatório de 4 KiB
    if (addr & 0xFFF) != 0 {
        return EINVAL;
    }

    let pid = scheduler::current_pid();
    let sched = scheduler::get();
    if let Some(Some(task)) = sched.tasks.get_mut(pid) {
        if task.ring == scheduler::Ring::User {
            // 2. Rejeição estrita de faixa: deve pertencer exclusivamente à região do heap dinâmico
            if addr < scheduler::USER_HEAP_BASE || addr >= scheduler::USER_HEAP_MAX {
                return EFAULT; // Rejeita pilha, ELF, kernel e endereços arbitrários
            }

            // 3. Validação de PROPRIEDADE: verifica se o endereço está registrado no vetor de páginas dinâmicas
            let mut found_slot = None;
            for (i, &p) in task.dynamic_pages.iter().enumerate() {
                if p == addr {
                    found_slot = Some(i);
                    break;
                }
            }

            let slot_idx = match found_slot {
                Some(i) => i,
                None => return EFAULT, // Página não alocada ou já liberada (proteção contra double-free)
            };

            let pml4 = crate::memory::paging::phys_to_virt(task.pml4_phys) as *mut crate::memory::paging::PageTable;
            if let Some((phys, _)) = unsafe { crate::memory::paging::lookup_page(pml4, addr) } {
                unsafe { crate::memory::paging::unmap_page(pml4, addr); }
                memory::free_frame(phys);
                task.dynamic_pages[slot_idx] = 0; // Remove do registro de páginas dinâmicas
                task.resources.free_frames(1);    // Atualiza contabilidade
                0
            } else {
                task.dynamic_pages[slot_idx] = 0;
                EFAULT
            }
        } else {
            task.resources.free_frames(1);
            memory::free_frame(addr);
            0
        }
    } else {
        EINVAL
    }
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
    match pool.release(pid, handle) {
        Ok(()) => {
            if let Some(Some(task)) = scheduler::get().tasks.get_mut(pid) {
                task.resources.release_jda();
            }
            0
        }
        Err(_) => EPERM,
    }
}

/// `sys_jda_submit(job_type, handle, len)` — Submete um job de processamento de fluxo de dados (JDA).
/// Processa o buffer físico Zero-Copy identificado por `handle` sem duplicar memória.
/// Retorna o resultado numérico do processamento (ex: checksum ou soma acumulada) ou erro negativo.
fn sys_jda_submit(job_type: usize, handle: usize, len: usize) -> isize {
    let pid = scheduler::current_pid();
    let pool = crate::jda::shmem::SHARED_BUFFER_POOL.lock();
    let phys = match pool.get_phys_addr(pid, handle) {
        Some(p) => p,
        None => return EPERM,
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

/// `sys_jda_shm_share(handle, target_pid, make_owner_ro)` — Compartilha um buffer físico com outro processo.
/// Mapeia a página no PML4 de `target_pid` com privilégio USER_RO. Se `make_owner_ro` for true,
/// rebaixa também a página do proprietário para somente-leitura (USER_RO) para garantir imutabilidade.
/// Retorna o endereço virtual no processo alvo (>= 0), ou código de erro negativo.
fn sys_jda_shm_share(handle: usize, target_pid: usize, make_owner_ro: bool) -> isize {
    let pid = scheduler::current_pid();
    let mut pool = crate::jda::shmem::SHARED_BUFFER_POOL.lock();
    match pool.share(pid, handle, target_pid, make_owner_ro) {
        Ok(vaddr) => vaddr as isize,
        Err(_) => EPERM,
    }
}

/// `sys_jda_shm_get_vaddr(handle)` — Obtém o endereço virtual do buffer mapeado no PML4 do processo.
/// Permite acesso direto de leitura/escrita com zero-copy a partir do espaço de usuário Ring 3.
fn sys_jda_shm_get_vaddr(handle: usize) -> isize {
    let pid = scheduler::current_pid();
    let pool = crate::jda::shmem::SHARED_BUFFER_POOL.lock();
    match pool.get_user_vaddr(pid, handle) {
        Some(vaddr) => vaddr as isize,
        None => EPERM,
    }
}

/// `sys_jda_ring_submit(sqe_ptr, count)` — Submete um lote de operações (SQEs) ao anel assíncrono io_uring do JDA.
/// Despacha I/O de disco e computação de forma assíncrona desacoplada da CPU sequencial.
fn sys_jda_ring_submit(sqe_ptr: *const crate::jda::ring::JdaSqe, count: usize) -> isize {
    let pid = scheduler::current_pid();
    if count == 0 || count > crate::jda::ring::JDA_RING_SIZE {
        return EINVAL;
    }
    if is_current_task_user() {
        if let Err(e) = validate_user_buffer(sqe_ptr as *const u8, count * core::mem::size_of::<crate::jda::ring::JdaSqe>(), false) {
            return e;
        }
    }
    let slice = unsafe { core::slice::from_raw_parts(sqe_ptr, count) };
    let mut ring = crate::jda::ring::JDA_RING.lock();
    let mut submitted = 0;
    for &sqe in slice {
        if ring.submit(sqe).is_ok() {
            submitted += 1;
        } else {
            break;
        }
    }
    // Despacha operações pendentes no anel
    ring.dispatch_pending(pid);
    submitted as isize
}

/// `sys_jda_ring_poll(cqe_ptr, max_count)` — Coleta entradas concluídas (CQEs) do anel assíncrono do JDA.
/// Retorna o número de conclusões lidas com sucesso (>= 0), ou código de erro negativo.
fn sys_jda_ring_poll(cqe_ptr: *mut crate::jda::ring::JdaCqe, max_count: usize) -> isize {
    if max_count == 0 || max_count > crate::jda::ring::JDA_RING_SIZE {
        return EINVAL;
    }
    if is_current_task_user() {
        if let Err(e) = validate_user_buffer(cqe_ptr as *const u8, max_count * core::mem::size_of::<crate::jda::ring::JdaCqe>(), true) {
            return e;
        }
    }
    let slice = unsafe { core::slice::from_raw_parts_mut(cqe_ptr, max_count) };
    let mut ring = crate::jda::ring::JDA_RING.lock();
    let mut polled = 0;
    for cqe in slice.iter_mut() {
        if let Some(c) = ring.poll_completion() {
            *cqe = c;
            polled += 1;
        } else {
            break;
        }
    }
    polled as isize
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

/// `sys_sync()` — Sincroniza todos os sistemas de arquivos montados e descarrega a cache de blocos primária para a mídia não volátil.
fn sys_sync() -> isize {
    match crate::vfs::sync_all() {
        Ok(()) => 0,
        Err(_) => EIO,
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
            max_cpu_ticks: task.limits.max_cpu_ticks,
            irq_preemptions: task.irq_preemptions,
            voluntary_yields: task.voluntary_yields,
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

/// `sys_socket(domain, type, protocol)` — Cria um endpoint de comunicação de rede e associa a um descritor unificado.
fn sys_socket(domain: u32, sock_type: u32, protocol: u32) -> isize {
    let pid = scheduler::current_pid();
    let sock_idx = match crate::net::socket::create_socket(pid, domain, sock_type, protocol) {
        Ok(idx) => idx,
        Err(e) => return e,
    };

    let sched = scheduler::get();
    if let Some(Some(task)) = sched.tasks.get_mut(pid) {
        for fd in 3..scheduler::MAX_TASK_FDS {
            if task.fds[fd].is_none() {
                task.fds[fd] = Some(scheduler::TaskDescriptor::Socket(sock_idx));
                return fd as isize;
            }
        }
        let _ = crate::net::socket::close_socket(pid, sock_idx);
        EMFILE
    } else {
        let _ = crate::net::socket::close_socket(pid, sock_idx);
        EPERM
    }
}

/// `sys_connect(fd, addr_ptr, addrlen)` — Conecta a um IP e porta remotos usando descritor unificado.
fn sys_connect(fd: usize, addr_ptr: *const u8, addrlen: usize) -> isize {
    if fd < 3 || fd >= scheduler::MAX_TASK_FDS {
        return EBADF;
    }
    let pid = scheduler::current_pid();
    let sock_idx = {
        let sched = scheduler::get();
        if let Some(Some(task)) = sched.tasks.get(pid) {
            match task.fds[fd] {
                Some(scheduler::TaskDescriptor::Socket(idx)) => idx,
                _ => return EBADF,
            }
        } else {
            return EPERM;
        }
    };

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
    match crate::net::socket::connect_socket(pid, sock_idx, addr.sin_addr, remote_port) {
        Ok(()) => 0,
        Err(e) => e,
    }
}

/// `sys_bind(fd, addr_ptr, addrlen)` — Associa socket a uma porta local.
fn sys_bind(fd: usize, addr_ptr: *const u8, addrlen: usize) -> isize {
    if fd < 3 || fd >= scheduler::MAX_TASK_FDS {
        return EBADF;
    }
    let pid = scheduler::current_pid();
    let sock_idx = {
        let sched = scheduler::get();
        if let Some(Some(task)) = sched.tasks.get(pid) {
            match task.fds[fd] {
                Some(scheduler::TaskDescriptor::Socket(idx)) => idx,
                _ => return EBADF,
            }
        } else {
            return EPERM;
        }
    };

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
    let local_port = u16::from_be(addr.sin_port);
    match crate::net::socket::bind_socket(pid, sock_idx, local_port) {
        Ok(()) => 0,
        Err(e) => e,
    }
}

/// `sys_listen(sockfd, backlog)` — Habilita escuta de conexões passivas (TCP).
fn sys_listen(fd: usize, backlog: usize) -> isize {
    if fd < 3 || fd >= scheduler::MAX_TASK_FDS {
        return EBADF;
    }
    let pid = scheduler::current_pid();
    let sock_idx = {
        let sched = scheduler::get();
        if let Some(Some(task)) = sched.tasks.get(pid) {
            match task.fds[fd] {
                Some(scheduler::TaskDescriptor::Socket(idx)) => idx,
                _ => return EBADF,
            }
        } else {
            return EPERM;
        }
    };

    match crate::net::socket::listen_socket(pid, sock_idx, backlog) {
        Ok(()) => 0,
        Err(e) => e,
    }
}

/// `sys_accept(sockfd, addr_ptr, addrlen)` — Aceita conexão de entrada TCP estabelecida.
fn sys_accept(fd: usize, addr_ptr: *mut u8, addrlen: usize) -> isize {
    if fd < 3 || fd >= scheduler::MAX_TASK_FDS {
        return EBADF;
    }
    let pid = scheduler::current_pid();
    let listening_sock_idx = {
        let sched = scheduler::get();
        if let Some(Some(task)) = sched.tasks.get(pid) {
            match task.fds[fd] {
                Some(scheduler::TaskDescriptor::Socket(idx)) => idx,
                _ => return EBADF,
            }
        } else {
            return EPERM;
        }
    };

    let sock_addr_size = core::mem::size_of::<crate::net::socket::SockAddrIn>();
    if !addr_ptr.is_null() {
        if addrlen < sock_addr_size {
            return EINVAL;
        }
        if is_current_task_user() {
            if let Err(e) = validate_user_buffer(addr_ptr as *const u8, sock_addr_size, true) {
                return e;
            }
        }
    }

    let sched = scheduler::get();
    let task = match sched.tasks.get_mut(pid).and_then(|t| t.as_mut()) {
        Some(t) => t,
        None => return EPERM,
    };

    // Encontra um FD livre para o socket filho aceito
    let mut free_fd = None;
    for cand in 3..scheduler::MAX_TASK_FDS {
        if task.fds[cand].is_none() {
            free_fd = Some(cand);
            break;
        }
    }
    let new_fd = match free_fd {
        Some(f) => f,
        None => return EMFILE,
    };

    if let Err(e) = task.resources.check_and_open_socket(&task.limits) {
        return e;
    }

    let child_sock_idx = match crate::net::socket::accept_socket(pid, listening_sock_idx) {
        Ok(idx) => idx,
        Err(e) => {
            task.resources.close_socket();
            return e;
        }
    };

    // Preenche informações do cliente conectado se o ponteiro foi fornecido
    if !addr_ptr.is_null() && addrlen >= sock_addr_size {
        if let Some((remote_ip, remote_port)) = crate::net::socket::get_socket_peer(child_sock_idx) {
            let peer_addr = addr_ptr as *mut crate::net::socket::SockAddrIn;
            unsafe {
                (*peer_addr).sin_family = crate::net::socket::AF_INET as u16;
                (*peer_addr).sin_port = remote_port.to_be();
                (*peer_addr).sin_addr = remote_ip;
                (*peer_addr).sin_zero = [0u8; 8];
            }
        }
    }

    task.fds[new_fd] = Some(scheduler::TaskDescriptor::Socket(child_sock_idx));
    new_fd as isize
}

/// `sys_send(fd, buf_ptr, len, flags)` — Envia dados pelo socket usando descritor unificado.
fn sys_send(fd: usize, buf_ptr: *const u8, len: usize, _flags: usize) -> isize {
    if fd < 3 || fd >= scheduler::MAX_TASK_FDS {
        return EBADF;
    }
    let pid = scheduler::current_pid();
    let sock_idx = {
        let sched = scheduler::get();
        if let Some(Some(task)) = sched.tasks.get(pid) {
            match task.fds[fd] {
                Some(scheduler::TaskDescriptor::Socket(idx)) => idx,
                _ => return EBADF,
            }
        } else {
            return EPERM;
        }
    };

    if buf_ptr.is_null() || len == 0 {
        return EINVAL;
    }
    if is_current_task_user() {
        if let Err(e) = validate_user_buffer(buf_ptr, len, false) {
            return e;
        }
    }
    let data = unsafe { core::slice::from_raw_parts(buf_ptr, len) };
    match crate::net::socket::send_socket(pid, sock_idx, data) {
        Ok(sent) => sent as isize,
        Err(e) => e,
    }
}

/// `sys_recv(fd, buf_ptr, len, flags)` — Recebe dados do socket usando descritor unificado.
fn sys_recv(fd: usize, buf_ptr: *mut u8, len: usize, _flags: usize) -> isize {
    if fd < 3 || fd >= scheduler::MAX_TASK_FDS {
        return EBADF;
    }
    let pid = scheduler::current_pid();
    let sock_idx = {
        let sched = scheduler::get();
        if let Some(Some(task)) = sched.tasks.get(pid) {
            match task.fds[fd] {
                Some(scheduler::TaskDescriptor::Socket(idx)) => idx,
                _ => return EBADF,
            }
        } else {
            return EPERM;
        }
    };

    if buf_ptr.is_null() || len == 0 {
        return EINVAL;
    }
    if is_current_task_user() {
        if let Err(e) = validate_user_buffer(buf_ptr as *const u8, len, true) {
            return e;
        }
    }
    let dest = unsafe { core::slice::from_raw_parts_mut(buf_ptr, len) };
    match crate::net::socket::recv_socket(pid, sock_idx, dest) {
        Ok(read_bytes) => read_bytes as isize,
        Err(e) => e,
    }
}

/// `sys_jinnwall_ctrl(cmd, arg0, arg1)` — Controle e telemetria do firewall JinnWall SPI.
/// cmd = 0: Consulta perfil ativo (0 = PublicSecure, 1 = Development)
/// cmd = 1: Define perfil ativo (arg0: 0 = PublicSecure, 1 = Development)
/// cmd = 2: Coleta estatísticas do firewall (arg0: *mut JinnWallStats)
fn sys_jinnwall_ctrl(cmd: usize, arg0: usize, _arg1: usize) -> isize {
    match cmd {
        0 => {
            let prof = unsafe { crate::net::jinnwall::JINNWALL.profile() };
            match prof {
                crate::net::jinnwall::FirewallProfile::PublicSecure => 0,
                crate::net::jinnwall::FirewallProfile::Development => 1,
            }
        }
        1 => {
            let prof = match arg0 {
                0 => crate::net::jinnwall::FirewallProfile::PublicSecure,
                1 => crate::net::jinnwall::FirewallProfile::Development,
                _ => return EINVAL,
            };
            unsafe {
                crate::net::jinnwall::JINNWALL.set_profile(prof);
            }
            0
        }
        2 => {
            let ptr = arg0 as *mut JinnWallStats;
            if ptr.is_null() {
                return EINVAL;
            }
            if is_current_task_user() {
                if let Err(e) = validate_user_buffer(ptr as *const u8, core::mem::size_of::<JinnWallStats>(), true) {
                    return e;
                }
            }
            let (inspected, accepted, dropped, active_conns, profile) = unsafe {
                let jw = &crate::net::jinnwall::JINNWALL;
                let p = match jw.profile() {
                    crate::net::jinnwall::FirewallProfile::PublicSecure => 0u32,
                    crate::net::jinnwall::FirewallProfile::Development => 1u32,
                };
                (jw.inspected(), jw.accepted(), jw.drops(), jw.active_connections() as u32, p)
            };
            let stats = JinnWallStats {
                inspected,
                accepted,
                dropped,
                active_conns,
                profile,
            };
            unsafe {
                core::ptr::write_unaligned(ptr, stats);
            }
            0
        }
        _ => EINVAL,
    }
}


// ── Kernel-internal invoke ────────────────────────────────────────────────────

/// Invoke a syscall directly from kernel code (no `int 0x80`).
#[inline]
pub fn invoke(nr: usize, arg0: usize, arg1: usize, arg2: usize) -> isize {
    syscall_dispatch(nr, arg0, arg1, arg2)
}
