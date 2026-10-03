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
unsafe fn sys_exit(code: i32) -> ! {
    syscall3(0, code as u64, 0, 0);
    loop {
        asm!("hlt");
    }
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let msg = b"ola-mundo\n";
    unsafe {
        let _ = sys_write(1, msg.as_ptr(), msg.len() as u64);
        sys_exit(0);
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
