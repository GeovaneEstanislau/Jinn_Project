/// Interrupt subsystem module.
///
/// Provides:
///   - `idt`          — Interrupt Descriptor Table setup
///   - `pic`          — 8259A PIC remapping and IRQ masking
///   - `ps2_keyboard` — PS/2 keyboard IRQ driver
///
/// The central dispatcher functions (`irq_dispatch`, `exception_dispatch`)
/// live here so that `stubs.s` can call them with a simple `call` without
/// needing to know the module structure.

pub mod idt;
pub mod pic;
pub mod ps2_keyboard;
pub mod ps2_mouse;
pub mod apic;

use crate::timer;

/// Central IRQ dispatcher, called from `irq_common` in stubs.s.
///
/// `vector` is the raw CPU vector (32 = IRQ0/timer, 33 = IRQ1/keyboard, …).
/// `frame`  is the stack pointer at the point where all GPRs were saved.
///
/// Returns 0 to keep the current task, or a new RSP for a context switch.
#[no_mangle]
pub extern "C" fn irq_dispatch(vector: u64, _frame: *mut usize) -> usize {
    match vector {
        32 => {
            // IRQ0 — PIT timer via legacy PIC
            timer::tick();
            pic::send_eoi(0);
            
            // --- DEBUG INDICATOR ---
            unsafe {
                if let Some(fb) = crate::limine::framebuffer() {
                    let width = fb.width as usize;
                    let pitch = fb.pitch as usize;
                    let ticks = timer::ticks();
                    let color = if (ticks % 100) < 50 { 0x0000FF00 } else { 0x00000000 };
                    for y in 0..8 {
                        for x in 0..8 {
                            let pixel_offset = y * pitch + (width - 20 + x) * 4;
                            core::ptr::write_volatile(
                                fb.address.add(pixel_offset) as *mut u32,
                                color,
                            );
                        }
                    }
                }
            }
            // -----------------------
            
            crate::scheduler::get().preempt(_frame as usize)
        }
        33 => {
            // IRQ1 — PS/2 Keyboard (EOI sent inside the handler)
            ps2_keyboard::kbd_irq_handler();
            0
        }
        44 => {
            // IRQ12 — PS/2 Mouse (EOI sent inside handler)
            ps2_mouse::mouse_irq_handler();
            0
        }
        34..=47 => {
            // Unhandled IRQ: send EOI and ignore
            let irq = (vector - 32) as u8;
            pic::send_eoi(irq);
            0
        }
        _ => 0,
    }
}

/// Central exception dispatcher, called from `exception_common` in stubs.s.
///
/// `vector`     — exception number (0–31)
/// `error_code` — pushed by CPU for some exceptions, 0 for others
/// `frame`      — stack pointer at exception site
///
/// Retorna 0 se a exceção foi fatal para o kernel (halt), ou o `saved_rsp`
/// da próxima tarefa se a falha veio de Ring 3 e foi recuperada pelo escalonador.
#[no_mangle]
pub extern "C" fn exception_dispatch(vector: u64, error_code: u64, frame: *mut usize) -> usize {
    let cs = unsafe { *frame.add(18) };
    let rip = unsafe { *frame.add(17) };
    let is_user = (cs & 3) == 3;

    if is_user {
        let current_pid = crate::scheduler::current_pid();
        let mut cr2: u64 = 0;
        if vector == 14 {
            unsafe { core::arch::asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack)); }
        }

        let mut writer = crate::vga::Writer::new();
        writer.set_color(crate::vga::Color::LightRed, crate::vga::Color::Black);
        writer.write_line("\n====================================================");
        writer.write_string(" [!] FALHA DE PROCESSO EM RING 3 (PID ");
        writer.write_decimal(current_pid);
        writer.write_string("): ");
        writer.write_string(exception_name(vector));
        writer.write_string(" (#");
        writer.write_decimal(vector as usize);
        writer.write_line(")");
        writer.write_string("     RIP: ");
        writer.write_hex_u64(rip as u64);
        if vector == 14 {
            writer.write_string("  CR2 (endereco invalido): ");
            writer.write_hex_u64(cr2);
        }
        if error_code != 0 {
            writer.write_string("  Error Code: ");
            writer.write_hex_u64(error_code);
        }
        writer.write_line("");
        writer.write_line("     Encerrando processo com segfault e recuperando escalonador...");
        writer.write_line("====================================================");
        writer.set_color(crate::vga::Color::LightGray, crate::vga::Color::Black);

        // Termina a tarefa faltosa (libera PML4 e FDs)
        let sched = crate::scheduler::get();
        sched.exit_task(current_pid, 128 + vector as i32);
        crate::ipc::unregister(current_pid);

        // Força o escalonador a selecionar a próxima tarefa pronta
        return sched.preempt(frame as usize);
    }

    // Se a falha ocorreu em Ring 0 (código do kernel), pânico irrecuperável:
    crate::serial_write(b"\n[!] KERNEL PANIC: CPU EXCEPTION in Ring 0! Vector: ");
    let vec_byte = b'0' + (vector as u8);
    crate::serial_write(&[vec_byte, b'\n']);

    let mut writer = crate::vga::Writer::new();
    writer.set_color(crate::vga::Color::LightRed, crate::vga::Color::Black);
    writer.write_line("\n====================================================");
    writer.write_string(" [!] KERNEL PANIC — CPU EXCEPTION #");
    writer.write_decimal(vector as usize);
    let name = exception_name(vector);
    writer.write_string(" — ");
    writer.write_line(name);
    writer.write_string("     RIP: ");
    writer.write_hex_u64(rip as u64);
    if vector == 14 {
        let mut cr2: u64 = 0;
        unsafe { core::arch::asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack)); }
        writer.write_string("     CR2: ");
        writer.write_hex_u64(cr2);
    }
    if error_code != 0 {
        writer.write_string("     Error Code: ");
        writer.write_hex_u64(error_code);
    }
    writer.write_line("");
    writer.write_line("====================================================");
    writer.write_line(" Kernel parado.");
    0
}

fn exception_name(vec: u64) -> &'static str {
    match vec {
        0  => "Divide-by-Zero",
        1  => "Debug",
        2  => "NMI",
        3  => "Breakpoint",
        4  => "Overflow",
        5  => "Bound Range Exceeded",
        6  => "Invalid Opcode",
        7  => "Device Not Available",
        8  => "Double Fault",
        10 => "Invalid TSS",
        11 => "Segment Not Present",
        12 => "Stack-Segment Fault",
        13 => "General Protection Fault",
        14 => "Page Fault",
        16 => "x87 FP Exception",
        17 => "Alignment Check",
        18 => "Machine Check",
        19 => "SIMD FP Exception",
        _  => "Reserved/Unknown",
    }
}


