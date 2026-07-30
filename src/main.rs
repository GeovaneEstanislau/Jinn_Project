#![no_std]
#![no_main]

use core::panic::PanicInfo;

mod memory;
mod scheduler;
mod timer;
mod vga;

use vga::Writer;

const KERNEL_NAME: &str = "Jinn Kernel";
const KERNEL_VERSION: &str = "0.0.1";
const PRELOAD_ENABLED: bool = true;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    let mut writer = Writer::new();
    writer.write_line("Kernel panic! Reiniciando...");
    loop {}
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let mut writer = Writer::new();
    writer.clear_screen();

    writer.write_line("Inicializando Jinn Kernel...");
    writer.write_line("Configurações de desempenho: ");
    if PRELOAD_ENABLED {
        writer.write_line("  - Pré-carregamento habilitado");
    }
    writer.write_line("  - Sem carregamento dinâmico de configuração desnecessária");

    memory::init();
    timer::init();

    let scheduler = scheduler::Scheduler::new();
    scheduler.add_task("Loader");
    scheduler.add_task("Worker");
    scheduler.add_task("Monitor");

    writer.write_line("Memória inicializada.");
    writer.write_string("Heap disponível: ");
    writer.write_decimal(memory::total_bytes());
    writer.write_line(" bytes");

    writer.write_string("Memória usada: ");
    writer.write_decimal(memory::used_bytes());
    writer.write_line(" bytes");

    writer.write_line("Agendador de tarefas criado.");
    scheduler.print_status(&mut writer);

    loop {
        timer::tick();
        let ticks = timer::ticks();
        writer.write_string("Tick: ");
        writer.write_decimal(ticks as usize);
        writer.write_line("");

        scheduler.schedule();

        for _ in 0..1000000 {
            core::hint::spin_loop();
        }
    }
}
