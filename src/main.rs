#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

use core::panic::PanicInfo;

pub mod cpu;
pub mod elf;
pub mod font;
pub mod gdt;
pub mod interrupts;
pub mod ipc;
pub mod keyboard_buffer;
pub mod limine;
pub mod memory;
pub mod pit;
pub mod scheduler;
pub mod shell;
pub mod syscall;
pub mod syscall_user;
pub mod timer;
pub mod user_processes;
pub mod vfs;
pub mod vga;

use vga::{Color, Writer};

const KERNEL_NAME:    &str = "Jinn Microkernel";
const KERNEL_VERSION: &str = "0.0.3-userspace";

#[alloc_error_handler]
fn alloc_error(_layout: core::alloc::Layout) -> ! {
    let mut w = Writer::new();
    w.set_color(Color::LightRed, Color::Black);
    w.write_line("\n[!] KERNEL: Out of heap memory!");
    loop { unsafe { core::arch::asm!("hlt") }; }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let mut w = Writer::new();
    w.set_color(Color::LightRed, Color::Black);
    w.write_line("\n=======================================================");
    w.write_line(" [!] KERNEL PANIC");
    w.write_line("=======================================================");
    if let Some(loc) = info.location() {
        w.write_string("Arquivo : ");
        w.write_line(loc.file());
        w.write_string("Linha   : ");
        w.write_decimal(loc.line() as usize);
        w.write_line("");
    }
    loop { core::hint::spin_loop(); }
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let mut w = Writer::new();
    w.clear_screen();

    w.set_color(Color::Yellow, Color::Black);
    w.write_line("================================================================================");
    w.set_color(Color::LightCyan, Color::Black);
    w.write_string("  ");
    w.write_string(KERNEL_NAME);
    w.write_string(" v");
    w.write_line(KERNEL_VERSION);
    w.set_color(Color::Yellow, Color::Black);
    w.write_line("================================================================================");
    w.set_color(Color::LightGray, Color::Black);

    w.write_string("[+] GDT e TSS (Ring 3 Ready)...");
    gdt::init();
    w.set_color(Color::LightGreen, Color::Black);
    w.write_line(" OK");
    w.set_color(Color::LightGray, Color::Black);

    w.write_string("[+] IDT (48 vetores)...");
    interrupts::idt::init();
    w.set_color(Color::LightGreen, Color::Black);
    w.write_line(" OK");
    w.set_color(Color::LightGray, Color::Black);

    // Mascara NMI via porta do RTC (bit 7 do port 0x70 desabilita NMI em hardware)
    unsafe { core::arch::asm!("out dx, al", in("dx") 0x70u16, in("al") 0x80u8, options(nomem, nostack)); }

    w.write_string("[+] PIC legado + PIT Timer (100Hz)...");
    interrupts::pic::remap();
    crate::timer::init();
    interrupts::pic::unmask_irq(0);    // IRQ0 = Timer (preempcao futura)
    // IRQ1 (teclado) NAO habilitado: o shell usa polling direto em 0x60/0x64
    // Isso evita conflito entre o IRQ handler e o loop de polling do shell.
    w.set_color(Color::LightGreen, Color::Black);
    w.write_line(" OK");
    w.set_color(Color::LightGray, Color::Black);

    w.write_string("[+] PS/2 Keyboard...");
    interrupts::ps2_keyboard::init_and_flush();
    w.set_color(Color::LightGreen, Color::Black);
    w.write_line(" OK");
    w.set_color(Color::LightGray, Color::Black);

    w.write_string("[+] Alocador Fisico...");
    memory::init();
    w.set_color(Color::LightGreen, Color::Black);
    w.write_line(" OK");
    w.set_color(Color::LightGray, Color::Black);
    w.write_string("    Memoria Utilizavel : ");
    w.write_memory_size(memory::total_bytes() as u64);
    w.write_line("");

    w.write_string("[+] Kernel Heap (16 MiB)...");
    memory::heap::init();
    w.set_color(Color::LightGreen, Color::Black);
    w.write_line(" OK");
    w.set_color(Color::LightGray, Color::Black);

    w.write_string("[+] VFS + ramfs /dev...");
    vfs::ramfs::init_dev();
    vfs::mount("/dev", &vfs::ramfs::DEV_FS).ok();
    w.set_color(Color::LightGreen, Color::Black);
    w.write_line(" OK");
    w.set_color(Color::LightGray, Color::Black);

    w.write_string("[+] IPC...");
    ipc::register(0);
    w.set_color(Color::LightGreen, Color::Black);
    w.write_line(" OK");
    w.set_color(Color::LightGray, Color::Black);

    w.write_string("[+] Escalonador e Processos...");
    let sched = scheduler::get();

    // Tarefa Idle (Ring 0, prioridade Idle) — roda quando ninguém mais quer CPU
    let _idle_pid = sched.add_idle_task();

    // O Shell (Ring 0) como task normal do escalonador
    let _shell_pid = sched.add_task("shell[K]", shell::task_entry);

    // Processos de usuário (Ring 3)
    let u1_pid = sched.add_user_task("ola-mundo[U3]", user_processes::processo_ola_mundo);
    ipc::register(u1_pid);
    let u2_pid = sched.add_user_task("contador[U3]",  user_processes::processo_contador);
    ipc::register(u2_pid);
    let u3_pid = sched.add_user_task("ipc-echo[U3]",  user_processes::processo_ipc_echo);
    ipc::register(u3_pid);
    
    w.set_color(Color::LightGreen, Color::Black);
    w.write_line(" OK");
    w.set_color(Color::LightGray, Color::Black);

    w.set_color(Color::LightGreen, Color::Black);
    w.write_line("[SUCCESS] Jinn OS pronto - entregando controle ao Escalonador.");
    w.set_color(Color::LightGray, Color::Black);

    // Habilita interrupções. O APIC Timer vai disparar (IRQ0) e a primeira 
    // preempção vai abandonar a pilha de boot e iniciar as tasks (começando pelo shell).
    unsafe { core::arch::asm!("sti") };

    // Idle loop do processador (Thread 0 / Boot thread abandonada)
    loop {
        unsafe { core::arch::asm!("hlt") };
    }
}
