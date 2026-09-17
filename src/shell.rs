/// Jinn OS â€” Shell do Kernel (Ring 0)
///
/// Executa no contexto do kernel (Ring 0) com acesso direto Ã s APIs internas.
/// Este shell NÃƒO usa syscalls â€” ele chama as funÃ§Ãµes do kernel diretamente,
/// pois Ã© uma tarefa de kernel e tem todos os privilÃ©gios.
///
/// # DistinÃ§Ã£o Ring 0 vs Ring 3
///
/// | Aspecto              | Shell (Ring 0)              | user_processes (Ring 3)       |
/// |----------------------|-----------------------------|-------------------------------|
/// | Acesso Ã  memÃ³ria     | Irrestrito                  | Apenas prÃ³prio espaÃ§o virtual |
/// | Acesso a hardware    | Direto (in/out, mmap, etc.) | Bloqueado (#GP)               |
/// | ComunicaÃ§Ã£o kernel   | Chamadas diretas            | Syscalls via `int 0x80`       |
/// | Leitura de meminfo   | `memory::total_bytes()`     | `sys_alloc_page` indiretamente|
/// | Listagem de processos| `scheduler::get()`          | NÃ£o disponÃ­vel                |
///
/// # Comandos disponÃ­veis (Ring 0 â€” acesso direto ao kernel)
///
/// | Comando       | DescriÃ§Ã£o                                          |
/// |---------------|----------------------------------------------------|
/// | `help`        | Lista todos os comandos                            |
/// | `clear`       | Limpa a tela                                       |
/// | `meminfo`     | EstatÃ­sticas de memÃ³ria fÃ­sica e heap              |
/// | `uname`       | Nome e versÃ£o do kernel                            |
/// | `ps`          | Lista todas as tarefas com PID, Ring, prio, estado |
/// | `kill <pid>`  | Finaliza uma tarefa (marca como Zombie)            |
/// | `uptime`      | Exibe ticks de timer desde o boot                  |
/// | `sched`       | EstatÃ­sticas do escalonador                        |
/// | `reboot`      | Reinicia via triple fault (debug)                  |

use crate::keyboard_buffer;
use crate::memory;
use crate::scheduler;
use crate::timer;
use crate::vga::{Color, Writer};

const MAX_LINE: usize = 128;

// â”€â”€ Prompt â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Desenha o prompt `jinn[K]> ` indicando execuÃ§Ã£o em Ring 0.
fn print_prompt(w: &mut Writer) {
    w.set_color(Color::LightCyan, Color::Black);
    w.write_string("jinn");
    w.set_color(Color::DarkGray, Color::Black);
    w.write_string("[K]");
    w.set_color(Color::White, Color::Black);
    w.write_string("> ");
    w.set_color(Color::LightGray, Color::Black);
}

// â”€â”€ Dispatcher â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Processa uma linha de comando completa (sem newline).
fn dispatch(w: &mut Writer, line: &[u8], len: usize) {
    if len == 0 {
        return;
    }
    let cmd = core::str::from_utf8(&line[..len]).unwrap_or("").trim();

    // Separa o primeiro token (comando) dos argumentos
    let (verb, args) = match cmd.find(' ') {
        Some(i) => (&cmd[..i], cmd[i + 1..].trim()),
        None    => (cmd, ""),
    };

    match verb {
        // â”€â”€ InformaÃ§Ãµes gerais â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        "help" => cmd_help(w),
        "clear" => w.clear_screen(),
        "uname" => cmd_uname(w),

        // â”€â”€ MemÃ³ria â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        "meminfo" => cmd_meminfo(w),

        // â”€â”€ Processos (acesso direto ao scheduler â€” Ring 0) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        "ps" | "tasks" => cmd_ps(w),
        "kill" => cmd_kill(w, args),

        // â”€â”€ Escalonador â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        "sched" => cmd_sched(w),
        "uptime" => cmd_uptime(w),

        // â”€â”€ Sistema â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        "metrics" => cmd_metrics(w),
        "reboot" => cmd_reboot(w),

        _ => {
            w.set_color(Color::LightRed, Color::Black);
            w.write_string("Comando nao encontrado: ");
            w.set_color(Color::LightGray, Color::Black);
            w.write_line(cmd);
            w.set_color(Color::DarkGray, Color::Black);
            w.write_line("  Digite 'help' para ver os comandos.");
            w.set_color(Color::LightGray, Color::Black);
        }
    }
}

// â”€â”€ ImplementaÃ§Ãµes dos comandos â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// `help` â€” Lista todos os comandos disponÃ­veis.
fn cmd_help(w: &mut Writer) {
    w.set_color(Color::Yellow, Color::Black);
    w.write_line("Comandos do Kernel (Ring 0):");
    w.set_color(Color::LightGray, Color::Black);
    w.write_line("  help           â€” exibe esta lista");
    w.write_line("  clear          â€” limpa a tela");
    w.write_line("  uname          â€” nome e versao do kernel");
    w.write_line("  meminfo        â€” estatisticas de memoria");
    w.write_line("  ps             â€” lista tarefas (alias: tasks)");
    w.write_line("  kill <pid>     â€” finaliza tarefa pelo PID");
    w.write_line("  sched          â€” estatisticas do escalonador");
    w.write_line("  uptime         â€” ticks de timer desde o boot");
    w.write_line("  reboot         â€” reinicia o sistema (triple fault)");
    w.write_line("");
    w.set_color(Color::DarkGray, Color::Black);
    w.write_line("  [K] = privilegio Ring 0 â€” acesso direto ao kernel");
    w.set_color(Color::LightGray, Color::Black);
}

/// `uname` â€” Nome e versÃ£o do kernel.
fn cmd_uname(w: &mut Writer) {
    w.set_color(Color::LightCyan, Color::Black);
    w.write_string("Jinn Microkernel v0.0.3-userspace ");
    w.set_color(Color::DarkGray, Color::Black);
    w.write_line("(x86_64, no_std, bare-metal, Ring 0/3)");
    w.set_color(Color::LightGray, Color::Black);
}

/// `meminfo` â€” EstatÃ­sticas de memÃ³ria fÃ­sica e heap.
fn cmd_meminfo(w: &mut Writer) {
    w.set_color(Color::Yellow, Color::Black);
    w.write_line("=== Informacoes de Memoria ===");
    w.set_color(Color::LightGray, Color::Black);

    w.write_string("  Mem. fisica  : ");
    w.write_memory_size(memory::total_bytes() as u64);
    w.write_line("");

    w.write_string("  Heap usado   : ");
    w.write_memory_size(memory::used_bytes() as u64);
    w.write_line("");

    let free = memory::total_bytes().saturating_sub(memory::used_bytes());
    w.write_string("  Heap livre   : ");
    w.write_memory_size(free as u64);
    w.write_line("");

    w.set_color(Color::DarkGray, Color::Black);
    w.write_line("  [Ring 0: acesso direto sem syscall]");
    w.set_color(Color::LightGray, Color::Black);
}

/// `ps` / `tasks` â€” Lista todas as tarefas do escalonador.
///
/// Acessa `scheduler::get()` diretamente â€” sÃ³ possÃ­vel em Ring 0.
fn cmd_ps(w: &mut Writer) {
    let sched = scheduler::get();

    w.set_color(Color::Yellow, Color::Black);
    w.write_line("PID  NOME                RING  PRIO      ESTADO");
    w.write_line("---  ------------------  ----  --------  ----------");
    w.set_color(Color::LightGray, Color::Black);

    let current_pid = scheduler::current_pid();
    let mut count = 0usize;

    for slot in sched.tasks.iter() {
        let task = match slot {
            Some(t) => t,
            None    => continue,
        };
        count += 1;

        // PID
        w.set_color(Color::LightCyan, Color::Black);
        write_padded_decimal(w, task.id, 3);
        w.set_color(Color::LightGray, Color::Black);
        w.write_string("  ");

        // Nome (atÃ© 18 chars)
        let name = task.name;
        let name_len = name.len().min(18);
        w.write_string(&name[..name_len]);
        for _ in name_len..18 {
            w.write_byte(b' ');
        }
        w.write_string("  ");

        // Ring
        match task.ring {
            scheduler::Ring::Kernel => {
                w.set_color(Color::LightBlue, Color::Black);
                w.write_string("K(0)");
            }
            scheduler::Ring::User => {
                w.set_color(Color::LightGreen, Color::Black);
                w.write_string("U(3)");
            }
        }
        w.set_color(Color::LightGray, Color::Black);
        w.write_string("  ");

        // Prioridade
        let prio_str = match task.priority {
            scheduler::TaskPriority::Idle     => "Idle    ",
            scheduler::TaskPriority::Normal   => "Normal  ",
            scheduler::TaskPriority::RealTime => "RealTime",
        };
        w.write_string(prio_str);
        w.write_string("  ");

        // Estado (com marcador se for a tarefa atual)
        let (state_str, state_color) = match task.state {
            scheduler::TaskState::Ready     => ("Pronto   ", Color::White),
            scheduler::TaskState::Running   => ("Executando", Color::LightGreen),
            scheduler::TaskState::Blocked   => ("Bloqueado", Color::LightRed),
            scheduler::TaskState::Zombie    => ("Zombie   ", Color::Yellow),
            scheduler::TaskState::Completed => ("Concluido", Color::DarkGray),
        };
        w.set_color(state_color, Color::Black);
        w.write_string(state_str);

        // Marca a tarefa atual com uma seta
        if task.id == current_pid {
            w.set_color(Color::LightCyan, Color::Black);
            w.write_string(" <-- voce");
        }

        w.set_color(Color::LightGray, Color::Black);
        w.write_line("");
    }

    w.set_color(Color::DarkGray, Color::Black);
    w.write_string("  Total: ");
    w.write_decimal(count);
    w.write_string(" tarefa(s) | [Ring 0: acesso direto ao scheduler]");
    w.write_line("");
    w.set_color(Color::LightGray, Color::Black);
}

/// `kill <pid>` â€” Finaliza uma tarefa pelo PID.
///
/// Marca como Zombie â€” sÃ³ uma tarefa de kernel (Ring 0) tem autoridade para
/// fazer isso sem passar por syscall.
fn cmd_kill(w: &mut Writer, args: &str) {
    let pid_str = args.trim();
    if pid_str.is_empty() {
        w.set_color(Color::LightRed, Color::Black);
        w.write_line("Uso: kill <pid>");
        w.set_color(Color::LightGray, Color::Black);
        return;
    }

    // Parse simples de inteiro decimal
    let mut pid: usize = 0;
    let mut valid = false;
    for ch in pid_str.bytes() {
        if ch >= b'0' && ch <= b'9' {
            pid = pid * 10 + (ch - b'0') as usize;
            valid = true;
        } else {
            valid = false;
            break;
        }
    }

    if !valid {
        w.set_color(Color::LightRed, Color::Black);
        w.write_string("PID invalido: ");
        w.write_line(pid_str);
        w.set_color(Color::LightGray, Color::Black);
        return;
    }

    let current_pid = scheduler::current_pid();
    if pid == current_pid {
        w.set_color(Color::LightRed, Color::Black);
        w.write_line("Nao e possivel matar o shell!");
        w.set_color(Color::LightGray, Color::Black);
        return;
    }

    let sched = scheduler::get();
    if sched.tasks.get(pid).and_then(|t| t.as_ref()).is_some() {
        sched.exit_task(pid, -1);
        w.set_color(Color::LightGreen, Color::Black);
        w.write_string("[OK] Tarefa PID=");
        w.write_decimal(pid);
        w.write_line(" marcada como Zombie.");
    } else {
        w.set_color(Color::LightRed, Color::Black);
        w.write_string("PID nao encontrado: ");
        w.write_decimal(pid);
        w.write_line("");
    }
    w.set_color(Color::LightGray, Color::Black);
}

/// `sched` â€” EstatÃ­sticas do escalonador.
fn cmd_sched(w: &mut Writer) {
    let sched   = scheduler::get();
    let ticks   = scheduler::sched_ticks();
    let cur_pid = scheduler::current_pid();
    let count   = sched.task_count();

    w.set_color(Color::Yellow, Color::Black);
    w.write_line("=== Estatisticas do Escalonador ===");
    w.set_color(Color::LightGray, Color::Black);

    w.write_string("  Preempcoes totais : ");
    w.write_decimal(ticks as usize);
    w.write_line("");

    w.write_string("  Tarefas ativas    : ");
    w.write_decimal(count);
    w.write_string(" / ");
    w.write_decimal(scheduler::MAX_TASKS);
    w.write_line("");

    w.write_string("  Tarefa atual      : PID=");
    w.write_decimal(cur_pid);
    w.write_line("");

    w.write_string("  Ticks de timer    : ");
    w.write_decimal(timer::ticks() as usize);
    w.write_line("");

    w.set_color(Color::DarkGray, Color::Black);
    w.write_line("  [Ring 0: dados lidos diretamente do kernel]");
    w.set_color(Color::LightGray, Color::Black);
}

/// `uptime` â€” Ticks de timer desde o boot.
fn cmd_uptime(w: &mut Writer) {
    let ticks = timer::ticks();
    // APIC configurado em 100 Hz â†’ ticks / 100 = segundos
    let secs  = ticks / 100;
    let frac  = ticks % 100;

    w.set_color(Color::LightCyan, Color::Black);
    w.write_string("Uptime: ");
    w.write_decimal(ticks as usize);
    w.write_string(" ticks (");
    w.write_decimal(secs as usize);
    w.write_string(".");
    if frac < 10 { w.write_string("0"); }
    w.write_decimal(frac as usize);
    w.write_line("s a 100Hz)");
    w.set_color(Color::LightGray, Color::Black);
}

/// `reboot` â€” Reinicia via triple fault (tÃ©cnica de debug em bare-metal).
fn cmd_reboot(w: &mut Writer) -> ! {
    w.set_color(Color::LightRed, Color::Black);
    w.write_line("Reiniciando via triple fault...");
    // Causa triple fault: carrega IDT invÃ¡lido e dispara interrupÃ§Ã£o
    unsafe {
        let null_idt: [u8; 10] = [0u8; 10];
        core::arch::asm!(
            "lidt [{}]",
            "int 3",
            in(reg) null_idt.as_ptr(),
            options(nostack)
        );
    }
    loop { core::hint::spin_loop(); }
}

// â”€â”€ Shell principal â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Ponto de entrada do shell â€” nunca retorna.
pub fn run(w: &mut Writer) -> ! {
    w.set_color(Color::LightGray, Color::Black);
    w.write_line("");
    w.set_color(Color::DarkGray, Color::Black);
    w.write_line("  Shell do Kernel [Ring 0] â€” acesso total ao hardware.");
    w.write_line("  Digite 'help' para ver os comandos disponÃ­veis.");
    w.set_color(Color::LightGray, Color::Black);
    w.write_line("");

    print_prompt(w);

    let mut line    = [0u8; MAX_LINE];
    let mut line_len = 0usize;

    loop {
        // Buffer de teclado (interrupÃ§Ã£o IRQ1 via APIC)
        if let Some(ch) = keyboard_buffer::pop() {
            handle_char(w, ch, &mut line, &mut line_len);
            continue;
        }

        // Fallback de polling: lÃª porta 0x64 diretamente (Ring 0 pode)
        unsafe {
            let status: u8;
            core::arch::asm!(
                "in al, dx",
                out("al") status,
                in("dx") 0x64u16,
                options(nomem, nostack)
            );
            if (status & 1) != 0 {
                let sc: u8;
                core::arch::asm!(
                    "in al, dx",
                    out("al") sc,
                    in("dx") 0x60u16,
                    options(nomem, nostack)
                );
                w.write_byte(b'*'); crate::interrupts::ps2_keyboard::process_scancode(sc);
                continue;
            }
        }

        core::hint::spin_loop();
    }
}

// â”€â”€ Processamento de caracteres â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

fn handle_char(w: &mut Writer, ch: u8, line: &mut [u8], line_len: &mut usize) {
    match ch {
        b'\n' => {
            w.set_color(Color::LightGray, Color::Black);
            w.write_byte(b'\n');
            dispatch(w, line, *line_len);
            *line_len = 0;
            w.write_byte(b'\n');
            print_prompt(w);
        }
        0x08 => {
            // Backspace
            if *line_len > 0 {
                *line_len -= 1;
                w.backspace();
            }
        }
        0x1B => {
            // Escape â€” cancela linha atual
            if *line_len > 0 {
                w.write_byte(b'\n');
                *line_len = 0;
                print_prompt(w);
            }
        }
        ch if ch >= 0x20 && ch < 0x7F => {
            if *line_len < MAX_LINE {
                line[*line_len] = ch;
                *line_len += 1;
                w.set_color(Color::LightGray, Color::Black);
                w.write_byte(ch);
            }
        }
        _ => {}
    }
}

// â”€â”€ Helpers de formataÃ§Ã£o â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Escreve um nÃºmero decimal alinhado Ã  direita em `width` colunas.
fn write_padded_decimal(w: &mut Writer, n: usize, width: usize) {
    // Conta dÃ­gitos
    let mut digits = 0usize;
    let mut tmp = if n == 0 { 1 } else { n };
    while tmp > 0 { digits += 1; tmp /= 10; }
    for _ in digits..width {
        w.write_byte(b' ');
    }
    w.write_decimal(n);
}



/// metrics - Monitoramento em tempo real (Fase 3.1)
fn cmd_metrics(w: &mut Writer) {
    w.set_color(crate::vga::Color::LightCyan, crate::vga::Color::Black);
    w.write_line("PID   CPU Ticks   IPC TX      IPC RX      Page Faults");
    w.write_line("-------------------------------------------------------");
    w.set_color(crate::vga::Color::LightGray, crate::vga::Color::Black);

    let sched = crate::scheduler::get();
    for i in 0..crate::telemetry::MAX_METRICS {
        if let Some((ticks, tx, rx, pf)) = crate::telemetry::get_metrics(i) {
            let is_active = i < crate::scheduler::MAX_TASKS && sched.tasks[i].is_some();
            if is_active || ticks > 0 || tx > 0 || rx > 0 || pf > 0 {
                write_padded_decimal(w, i, 3);
                w.write_string("   ");
                write_padded_decimal(w, ticks as usize, 9);
                w.write_string("   ");
                write_padded_decimal(w, tx as usize, 8);
                w.write_string("   ");
                write_padded_decimal(w, rx as usize, 8);
                w.write_string("   ");
                write_padded_decimal(w, pf as usize, 11);
                w.write_line("");
            }
        }
    }
}
