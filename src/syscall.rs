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

use crate::ipc::{self, Message};
use crate::memory;
use crate::scheduler;

// ── Error Codes ───────────────────────────────────────────────────────────────
pub const ENOSYS:   isize = -38;  // Function not implemented
pub const EINVAL:   isize = -22;  // Invalid argument
pub const ENOMEM:   isize = -12;  // Out of memory
pub const ENOBUFS:  isize = -105; // No buffer space available
pub const EPERM:    isize = -1;   // Operation not permitted

// ── Dispatcher ────────────────────────────────────────────────────────────────

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
/// fd 1 = stdout (VGA), fd 2 = stderr (VGA red).
fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    if buf.is_null() || len == 0 { return EINVAL; }
    if len > 65536 { return EINVAL; }

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
        _ => EINVAL,
    }
}

/// `read(fd, buf, len)` — Read up to `len` bytes into `buf`.
/// fd 0 = stdin (keyboard buffer).
fn sys_read(fd: usize, buf: *mut u8, len: usize) -> isize {
    if buf.is_null() || len == 0 { return EINVAL; }

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
        _ => EINVAL,
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
/// ring_flag: 0 = Ring 0 (kernel task), 3 = Ring 3 (user task).
fn sys_spawn(fn_ptr: usize, _name_ptr: *const u8, ring_flag: usize) -> isize {
    let entry: fn() = unsafe { core::mem::transmute(fn_ptr) };
    let sched = scheduler::get();
    let pid = if ring_flag == 3 {
        let pid = sched.add_user_task("spawned-u3", entry);
        ipc::register(pid);
        pid
    } else {
        let pid = sched.add_task("spawned-k0", entry);
        ipc::register(pid);
        pid
    };
    pid as isize
}

/// `waitpid(pid)` — Poll until task is Zombie, collect it and return exit code.
fn sys_waitpid(pid: usize) -> isize {
    loop {
        let sched = scheduler::get();
        if let Some(code) = sched.collect_zombie(pid) {
            return code as isize;
        }
        // Tarefa não terminou ainda — yield e tenta de novo
        unsafe { core::arch::asm!("int 32", options(nomem, nostack)); }
    }
}

// ── Kernel-internal invoke ────────────────────────────────────────────────────

/// Invoke a syscall directly from kernel code (no `int 0x80`).
#[inline]
pub fn invoke(nr: usize, arg0: usize, arg1: usize, arg2: usize) -> isize {
    syscall_dispatch(nr, arg0, arg1, arg2)
}
