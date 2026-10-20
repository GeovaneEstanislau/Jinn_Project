#![no_std]
#![no_main]

use core::{arch::asm, panic::PanicInfo};

#[inline(always)]
unsafe fn syscall3(nr: u64, arg0: u64, arg1: u64, arg2: u64) -> i64 {
    let ret: i64;
    asm!(
        "int 0x80",
        inout("rax") nr => ret,
        in("rdi") arg0,
        in("rsi") arg1,
        in("rdx") arg2,
        options(nostack, nomem)
    );
    ret
}

#[inline(always)]
unsafe fn sys_write(fd: u64, buf: *const u8, len: u64) -> i64 {
    syscall3(1, fd, buf as u64, len)
}

#[inline(always)]
unsafe fn sys_sleep(ms: u64) -> i64 {
    syscall3(11, ms, 0, 0)
}

#[inline(always)]
unsafe fn sys_jda_shm_alloc(is_read_only: bool) -> i64 {
    syscall3(12, if is_read_only { 1 } else { 0 }, 0, 0)
}

#[inline(always)]
unsafe fn sys_jda_shm_release(handle: u64) -> i64 {
    syscall3(13, handle, 0, 0)
}

#[inline(always)]
unsafe fn sys_exit(code: i32) -> ! {
    loop {
        syscall3(0, code as u64, 0, 0);
    }
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let msg1 = b"[U3] ola-mundo ativo em Ring 3!\n";
    let msg2 = b"[U3] Solicitando buffer JDA Zero-Copy (Syscall 12 alloc)...\n";
    let msg3 = b"[U3] Buffer alocado! Liberando (Syscall 13 release)...\n";
    let msg4 = b"[U3] JDA Zero-Copy validado! Entrando em sono (500ms)...\n";
    let msg5 = b"[U3] Acordou com sucesso! Encerrando processo.\n";
    unsafe {
        let _ = sys_write(1, msg1.as_ptr(), msg1.len() as u64);
        let _ = sys_write(1, msg2.as_ptr(), msg2.len() as u64);

        let handle = sys_jda_shm_alloc(false);
        if handle >= 0 {
            let _ = sys_write(1, msg3.as_ptr(), msg3.len() as u64);
            let _ = sys_jda_shm_release(handle as u64);
            let _ = sys_write(1, msg4.as_ptr(), msg4.len() as u64);
        }

        let _ = sys_sleep(500);
        let _ = sys_write(1, msg5.as_ptr(), msg5.len() as u64);
        sys_exit(0);
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
