#![no_std]
#![no_main]

use core::fmt::Write;
use core::panic::PanicInfo;

mod vga_buffer;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    let mut writer = vga_buffer::Writer::new();
    let _ = writer.write_str("Kernel panic!\n");
    loop {}
}

#[unsafe(export_name = "_start")]
pub extern "C" fn _start() -> ! {
    let mut writer = vga_buffer::Writer::new();
    let _ = writer.write_str("Jinn Kernel 0.0.1 Boot Successful\n");
    let _ = writer.write_str("Bem-vindo ao kernel Jinn em Rust!\n");

    loop {}
}