/// Jinn OS — Shell do Kernel (Ring 0)
///
/// Executa no contexto do kernel (Ring 0) com acesso direto às APIs internas.
/// Este shell NÃO usa syscalls — ele chama as funções do kernel diretamente,
/// pois é uma tarefa de kernel e tem todos os privilégios.
///
/// # Distinção Ring 0 vs Ring 3
///
/// | Aspecto              | Shell (Ring 0)              | user_processes (Ring 3)       |
/// |----------------------|-----------------------------|-------------------------------|
/// | Acesso à memória     | Irrestrito                  | Apenas próprio espaço virtual |
/// | Acesso a hardware    | Direto (in/out, mmap, etc.) | Bloqueado (#GP)               |
/// | Comunicação kernel   | Chamadas diretas            | Syscalls via `int 0x80`       |
/// | Leitura de meminfo   | `memory::total_bytes()`     | `sys_alloc_page` indiretamente|
/// | Listagem de processos| `scheduler::get()`          | Não disponível                |
///
/// # Comandos disponíveis (Ring 0 — acesso direto ao kernel)
///
/// | Comando       | Descrição                                          |
/// |---------------|----------------------------------------------------|
/// | `help`        | Lista todos os comandos                            |
/// | `clear`       | Limpa a tela                                       |
/// | `meminfo`     | Estatísticas de memória física e heap              |
/// | `uname`       | Nome e versão do kernel                            |
/// | `ps`          | Lista todas as tarefas com PID, Ring, prio, estado |
/// | `kill <pid>`  | Finaliza uma tarefa (marca como Zombie)            |
/// | `uptime`      | Exibe ticks de timer desde o boot                  |
/// | `sched`       | Estatísticas do escalonador                        |
/// | `reboot`      | Reinicia via triple fault (debug)                  |

use crate::keyboard_buffer;
use crate::memory;
use crate::scheduler;
use crate::timer;
use crate::vga::{Color, Writer};

const MAX_LINE: usize = 128;

// ── Prompt ────────────────────────────────────────────────────────────────────

/// Desenha o prompt `jinn[K]> ` indicando execução em Ring 0.
fn print_prompt(w: &mut Writer) {
    w.set_color(Color::LightCyan, Color::Black);
    w.write_string("jinn");
    w.set_color(Color::DarkGray, Color::Black);
    w.write_string("[K]");
    w.set_color(Color::White, Color::Black);
    w.write_string("> ");
    w.set_color(Color::LightGray, Color::Black);
}

// ── Dispatcher ────────────────────────────────────────────────────────────────

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
        // ── Informações gerais ────────────────────────────────────────────────
        "help" => cmd_help(w),
        "clear" => w.clear_screen(),
        "uname" => cmd_uname(w),

        // ── Memória ───────────────────────────────────────────────────────────
        "meminfo" => cmd_meminfo(w),

        // ── Processos (acesso direto ao scheduler — Ring 0) ───────────────────
        "ps" | "tasks" => cmd_ps(w),
        "kill" => cmd_kill(w, args),

        // ── Escalonador ───────────────────────────────────────────────────────
        "sched" => cmd_sched(w),
        "uptime" => cmd_uptime(w),

        // ── Sistema ───────────────────────────────────────────────────────────
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

// ── Implementações dos comandos ───────────────────────────────────────────────

/// `help` — Lista todos os comandos disponíveis.
fn cmd_help(w: &mut Writer) {
    w.set_color(Color::Yellow, Color::Black);
    w.write_line("Comandos do Kernel (Ring 0):");
    w.set_color(Color::LightGray, Color::Black);
    w.write_line("  help           — exibe esta lista");
    w.write_line("  clear          — limpa a tela");
    w.write_line("  uname          — nome e versao do kernel");
    w.write_line("  meminfo        — estatisticas de memoria");
    w.write_line("  ps             — lista tarefas (alias: tasks)");
    w.write_line("  kill <pid>     — finaliza tarefa pelo PID");
    w.write_line("  sched          — estatisticas do escalonador");
    w.write_line("  uptime         — ticks de timer desde o boot");
    w.write_line("  reboot         — reinicia o sistema (triple fault)");
    w.write_line("");
    w.set_color(Color::DarkGray, Color::Black);
    w.write_line("  [K] = privilegio Ring 0 — acesso direto ao kernel");
    w.set_color(Color::LightGray, Color::Black);
}

/// `uname` — Nome e versão do kernel.
fn cmd_uname(w: &mut Writer) {
    w.set_color(Color::LightCyan, Color::Black);
    w.write_string("Jinn Microkernel v0.0.3-userspace ");
    w.set_color(Color::DarkGray, Color::Black);
    w.write_line("(x86_64, no_std, bare-metal, Ring 0/3)");
    w.set_color(Color::LightGray, Color::Black);
}

/// `meminfo` — Estatísticas de memória física e heap.
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

/// `ps` / `tasks` — Lista todas as tarefas do escalonador.
///
/// Acessa `scheduler::get()` diretamente — só possível em Ring 0.
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

        // Nome (até 18 chars)
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

/// `kill <pid>` — Finaliza uma tarefa pelo PID.
///
/// Marca como Zombie — só uma tarefa de kernel (Ring 0) tem autoridade para
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

/// `sched` — Estatísticas do escalonador.
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

/// `uptime` — Ticks de timer desde o boot.
fn cmd_uptime(w: &mut Writer) {
    let ticks = timer::ticks();
    // APIC configurado em 100 Hz → ticks / 100 = segundos
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

/// `reboot` — Reinicia via triple fault (técnica de debug em bare-metal).
fn cmd_reboot(w: &mut Writer) -> ! {
    w.set_color(Color::LightRed, Color::Black);
    w.write_line("Reiniciando via triple fault...");
    // Causa triple fault: carrega IDT inválido e dispara interrupção
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

// ── Shell principal ────────────────────────────────────────────────────────────

/// Ponto de entrada do shell — nunca retorna.
pub fn run(w: &mut Writer) -> ! {
    w.set_color(Color::LightGray, Color::Black);
    w.write_line("");
    w.set_color(Color::DarkGray, Color::Black);
    w.write_line("  Shell do Kernel [Ring 0] — acesso total ao hardware.");
    w.write_line("  Digite 'help' para ver os comandos disponíveis.");
    w.set_color(Color::LightGray, Color::Black);
    w.write_line("");

    print_prompt(w);

    let mut line    = [0u8; MAX_LINE];
    let mut line_len = 0usize;

    loop {
        // Buffer de teclado (interrupção IRQ1 via APIC)
        if let Some(ch) = keyboard_buffer::pop() {
            handle_char(w, ch, &mut line, &mut line_len);
            continue;
        }

        // Fallback de polling: lê porta 0x64 diretamente (Ring 0 pode)
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

// ── Processamento de caracteres ───────────────────────────────────────────────

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
            // Escape — cancela linha atual
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

// ── Helpers de formatação ─────────────────────────────────────────────────────

/// Escreve um número decimal alinhado à direita em `width` colunas.
fn write_padded_decimal(w: &mut Writer, n: usize, width: usize) {
    // Conta dígitos
    let mut digits = 0usize;
    let mut tmp = if n == 0 { 1 } else { n };
    while tmp > 0 { digits += 1; tmp /= 10; }
    for _ in digits..width {
        w.write_byte(b' ');
    }
    w.write_decimal(n);
}

