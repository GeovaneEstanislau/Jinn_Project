#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

use core::panic::PanicInfo;

pub mod cpu;
pub mod drivers;
pub mod elf;
pub mod font;
pub mod elf_tests;
pub mod gdt;
pub mod gui;
pub mod interrupts;
pub mod ipc;
pub mod jda;
pub mod keyboard_buffer;
pub mod limine;
pub mod memory;
pub mod net;
pub mod pit;
pub mod resources;
pub mod scheduler;
pub mod shell;
pub mod smp;
pub mod syscall;
pub mod syscall_user;
pub mod timer;
pub mod user_processes;
pub mod vfs;
pub mod vga;

use vga::{Color, Writer};

const KERNEL_NAME:    &str = "Jinn Microkernel";
const KERNEL_VERSION: &str = "0.0.3-userspace";

pub fn serial_write_byte(byte: u8) {
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") 0x3f8u16,
            in("al") byte,
            options(nomem, nostack, preserves_flags)
        );
    }
}

static SERIAL_LOCK: crate::memory::SpinLock<()> = crate::memory::SpinLock::new(());

pub fn serial_write(message: &[u8]) {
    let _guard = SERIAL_LOCK.lock();
    for &byte in message {
        serial_write_byte(byte);
    }
}

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
    serial_write_byte(b'J');
    serial_write_byte(b'I');
    serial_write_byte(b'N');
    serial_write_byte(b'N');
    serial_write_byte(b'\n');

    serial_write(b"VGA writer\n");
    let mut w = Writer::new();
    serial_write(b"VGA clear\n");
    w.clear_screen();
    serial_write(b"VGA ready\n");

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
    serial_write(b"GDT start\n");
    gdt::init();
    unsafe { cpu::enable_sse(); }
    serial_write(b"GDT done\n");
    w.set_color(Color::LightGreen, Color::Black);
    w.write_line(" OK");
    w.set_color(Color::LightGray, Color::Black);

    w.write_string("[+] IDT (48 vetores)...");
    serial_write(b"IDT start\n");
    interrupts::idt::init();
    serial_write(b"IDT done\n");
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

    w.write_string("[+] PS/2 Mouse (IRQ 12)...");
    interrupts::ps2_mouse::init();
    interrupts::pic::unmask_irq(12);
    w.set_color(Color::LightGreen, Color::Black);
    w.write_line(" OK");
    w.set_color(Color::LightGray, Color::Black);

    w.write_string("[+] Barramento PCI...");
    drivers::pci::init();
    w.set_color(Color::LightGreen, Color::Black);
    w.write_line(" OK");
    w.set_color(Color::LightGray, Color::Black);
    w.write_string("    Dispositivos PCI   : ");
    w.write_decimal(drivers::pci::device_count());
    w.write_line(" encontrados");

    w.write_string("[+] IDE / ATA Controller e Cache LRU...");
    drivers::ide::init();
    drivers::block::init_primary_cache();
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

    // Auto-detecta e monta partições FAT32 e Ext2 do disco primário IDE
    if let Ok(mbr) = drivers::ide::read_mbr(0) {
        if mbr.is_valid {
            let mut fat_mounted = false;
            let mut ext2_mounted = false;
            for part_opt in mbr.partitions.iter() {
                if let Some(part) = part_opt {
                    if !fat_mounted && (part.partition_type == 0x0B || part.partition_type == 0x0C) {
                        if vfs::fat32::mount_fat32_partition("/disk", 0, part.start_lba, part.sector_count as u64).is_ok() {
                            w.write_string("    Auto-montado FAT32 em /disk (LBA ");
                            w.write_decimal(part.start_lba as usize);
                            w.write_line(")");
                            fat_mounted = true;
                        }
                    } else if !ext2_mounted && part.partition_type == 0x83 {
                        if vfs::ext2::mount_ext2_partition("/ext2", 0, part.start_lba, part.sector_count as u64).is_ok() {
                            w.write_string("    Auto-montado Ext2 em /ext2 (LBA ");
                            w.write_decimal(part.start_lba as usize);
                            w.write_line(")");
                            ext2_mounted = true;
                        }
                    }
                }
            }
        }
    }

    w.write_string("[+] IPC...");
    ipc::register(0);
    w.set_color(Color::LightGreen, Color::Black);
    w.write_line(" OK");
    w.set_color(Color::LightGray, Color::Black);

    w.write_string("[+] JDA Engine & IoRing (Dataflow Architecture)...");
    if jda::init().is_ok() {
        let (passed, total) = jda::run_jda_selftests();
        if passed == total {
            w.set_color(Color::LightGreen, Color::Black);
            w.write_string(" OK (");
            w.write_decimal(passed);
            w.write_string("/");
            w.write_decimal(total);
            w.write_line(" aprovados)");
        } else {
            w.set_color(Color::LightRed, Color::Black);
            w.write_string(" FALHA (");
            w.write_decimal(passed);
            w.write_string("/");
            w.write_decimal(total);
            w.write_line(")");
        }
    } else {
        w.set_color(Color::Yellow, Color::Black);
        w.write_line(" WARN (Desativado)");
    }
    w.set_color(Color::LightGray, Color::Black);

    w.write_string("[+] Pilha de Rede & Firewall JinnWall...");
    net::init();
    w.set_color(Color::LightGreen, Color::Black);
    w.write_string(" OK ");
    w.set_color(Color::LightCyan, Color::Black);
    if let Some(dev) = drivers::net::virtio::get_device() {
        if dev.is_available {
            w.write_string("(VirtIO-Net @ 0x");
            w.write_hex_u16(dev.io_base);
            w.write_line(")");
        } else {
            w.write_line("(Loopback Virtual)");
        }
    } else {
        w.write_line("(Inativo)");
    }
    w.set_color(Color::LightGray, Color::Black);

    w.write_string("[+] Testes de Seguranca do ELF Loader...");
    let (passed, total) = elf_tests::run_elf_security_selftests();
    if passed == total {
        w.set_color(Color::LightGreen, Color::Black);
        w.write_string(" OK (");
        w.write_decimal(passed);
        w.write_string("/");
        w.write_decimal(total);
        w.write_line(" aprovados)");
        w.set_color(Color::LightGray, Color::Black);
    } else {
        w.set_color(Color::LightRed, Color::Black);
        w.write_string(" FALHA (");
        w.write_decimal(passed);
        w.write_string("/");
        w.write_decimal(total);
        w.write_line(")");
        w.set_color(Color::LightGray, Color::Black);
    }

    w.write_string("[+] Validacao de Memoria e Syscalls EFAULT...");
    let r_null = crate::syscall::validate_user_buffer(core::ptr::null(), 10, false);
    let r_kern = crate::syscall::validate_user_buffer(0xFFFF_8000_0000_0000 as *const u8, 10, false);
    let r_unmap = crate::syscall::validate_user_buffer(0x0000_1000_0000 as *const u8, 4096, false);
    if r_null == Err(crate::syscall::EINVAL) && r_kern == Err(crate::syscall::EFAULT) && r_unmap == Err(crate::syscall::EFAULT) {
        w.set_color(Color::LightGreen, Color::Black);
        w.write_line(" OK (NULL, Kernel-Space e Unmapped rejeitados)");
        w.set_color(Color::LightGray, Color::Black);
    } else {
        w.set_color(Color::LightRed, Color::Black);
        w.write_line(" FALHA");
        w.set_color(Color::LightGray, Color::Black);
    }

    w.write_string("[+] Testes RFC 1035 e Cache DNS...");
    let (dns_passed, dns_total) = net::dns::run_dns_selftests();
    if dns_passed == dns_total {
        w.set_color(Color::LightGreen, Color::Black);
        w.write_string(" OK (");
        w.write_decimal(dns_passed);
        w.write_string("/");
        w.write_decimal(dns_total);
        w.write_line(" aprovados)");
        w.set_color(Color::LightGray, Color::Black);
    } else {
        w.set_color(Color::LightRed, Color::Black);
        w.write_string(" FALHA (");
        w.write_decimal(dns_passed);
        w.write_string("/");
        w.write_decimal(dns_total);
        w.write_line(")");
        w.set_color(Color::LightGray, Color::Black);
    }

    w.write_string("[+] Testes de Politicas e SPI JinnWall...");
    let (jw_passed, jw_total) = net::jinnwall::run_jinnwall_selftests();
    if jw_passed == jw_total {
        w.set_color(Color::LightGreen, Color::Black);
        w.write_string(" OK (");
        w.write_decimal(jw_passed);
        w.write_string("/");
        w.write_decimal(jw_total);
        w.write_line(" aprovados)");
        w.set_color(Color::LightGray, Color::Black);
    } else {
        w.set_color(Color::LightRed, Color::Black);
        w.write_string(" FALHA (");
        w.write_decimal(jw_passed);
        w.write_string("/");
        w.write_decimal(jw_total);
        w.write_line(")");
        w.set_color(Color::LightGray, Color::Black);
    }

    w.write_string("[+] Testes RFC 793 e Protocolo TCP...");
    let (tcp_passed, tcp_total) = net::tcp::run_tcp_selftests();
    if tcp_passed == tcp_total {
        w.set_color(Color::LightGreen, Color::Black);
        w.write_string(" OK (");
        w.write_decimal(tcp_passed);
        w.write_string("/");
        w.write_decimal(tcp_total);
        w.write_line(" aprovados)");
        w.set_color(Color::LightGray, Color::Black);
    } else {
        w.set_color(Color::LightRed, Color::Black);
        w.write_string(" FALHA (");
        w.write_decimal(tcp_passed);
        w.write_string("/");
        w.write_decimal(tcp_total);
        w.write_line(")");
        w.set_color(Color::LightGray, Color::Black);
    }

    w.write_string("[+] Multiprocessamento Simetrico (SMP)...");
    let online_cpus = smp::init();
    if online_cpus > 1 {
        w.set_color(Color::LightGreen, Color::Black);
        w.write_string(" OK (");
        w.write_decimal(online_cpus);
        w.write_line(" nucleos ativos)");
    } else {
        w.set_color(Color::Yellow, Color::Black);
        w.write_line(" 1 nucleo ativo (Uniprocessador)");
    }
    w.set_color(Color::LightGray, Color::Black);

    w.write_string("[+] Escalonador e Processos...");
    let sched = scheduler::get();

    // Tarefa Idle dedicada para o BSP (PID 0)
    let _idle_pid = sched.add_idle_task();
    sched.core_idle[0] = 0;

    // Tarefas Idle dedicadas para os núcleos AP secundários (alocadas nos slots 31, 30, 29...)
    for c in 1..online_cpus {
        let ap_idle = sched.add_ap_idle_task(c);
        sched.core_idle[c] = ap_idle;
    }

    // O Shell (Ring 0) como task normal do escalonador
    let _shell_pid = sched.add_task("shell[K]", shell::task_entry);

    // Processos de usuário (Ring 3) — carregados via ELF Loader
    let mod_count = limine::module_count();
    w.write_string("\n    Modulos Limine: ");
    w.write_decimal(mod_count);
    w.write_line("");

    if mod_count > 0 {
        if let Some(elf_data) = limine::get_module(0) {
            w.write_string("    ELF modulo[0]: ");
            w.write_decimal(elf_data.len());
            w.write_line(" bytes");
            let u1_pid = sched.add_user_task("ola-mundo[U3]", elf_data);
            if u1_pid > 0 {
                ipc::register(u1_pid);
                w.write_string("    Processo Ring 3 criado: PID ");
                w.write_decimal(u1_pid);
                w.write_line(" (Suite Principal)");
            } else {
                w.set_color(Color::LightRed, Color::Black);
                w.write_line("    [ERRO] Falha ao carregar ELF para Ring 3!");
                w.set_color(Color::LightGray, Color::Black);
            }

            let u2_pid = sched.add_user_task("worker[U3]", elf_data);
            if u2_pid > 0 {
                ipc::register(u2_pid);
                // Atribui cota de CPU de 25 ticks (250 ms) para a tarefa CPU-Hog
                if let Some(Some(t)) = sched.tasks.get_mut(u2_pid) {
                    t.limits.max_cpu_ticks = 25;
                }
                w.write_string("    Processo Ring 3 criado: PID ");
                w.write_decimal(u2_pid);
                w.write_line(" (Worker / CPU-Hog, Cota: 25 ticks)");
            }

            let u3_pid = sched.add_user_task("periodic[U3]", elf_data);
            if u3_pid > 0 {
                ipc::register(u3_pid);
                w.write_string("    Processo Ring 3 criado: PID ");
                w.write_decimal(u3_pid);
                w.write_line(" (Tarefa Periodica)");
            }
        }
    } else {
        w.write_line("    (Nenhum modulo ELF carregado pelo bootloader)");
    }
    
    w.set_color(Color::LightGreen, Color::Black);
    w.write_line(" OK");
    w.set_color(Color::LightGray, Color::Black);

    w.write_string("[+] Subsistema Grafico (Limine Framebuffer)...");
    if let Some(fb) = limine::framebuffer() {
        gui::init();
        w.set_color(Color::LightGreen, Color::Black);
        w.write_string(" OK ");
        w.set_color(Color::LightCyan, Color::Black);
        w.write_string("(");
        w.write_decimal(fb.width as usize);
        w.write_string("x");
        w.write_decimal(fb.height as usize);
        w.write_string("x");
        w.write_decimal(fb.bpp as usize);
        w.write_line(" bpp)");
        w.set_color(Color::LightGray, Color::Black);
    } else {
        w.set_color(Color::Yellow, Color::Black);
        w.write_line(" AVISO (Modo texto VGA legado)");
        w.set_color(Color::LightGray, Color::Black);
    }

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
