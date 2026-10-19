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
use crate::vfs;
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
        "spawn" => cmd_spawn(w, args),

        // ── Escalonador ───────────────────────────────────────────────────────
        "sched" => cmd_sched(w),
        "uptime" => cmd_uptime(w),

        // ── Interface Gráfica ─────────────────────────────────────────────────
        "gui" | "desktop" | "startx" => cmd_gui(w),

        // ── Armazenamento / Discos ─────────────────────────────────────────────
        "disk" | "disks" | "ata" => cmd_disk(w),

        // ── Barramento de Hardware ─────────────────────────────────────────────
        "lspci" | "pci" => cmd_lspci(w),

        // ── Sistema de Arquivos (VFS & FAT32) ──────────────────────────────────
        "ls" | "dir" => cmd_ls(w, args),
        "cat" => cmd_cat(w, args),
        "mounts" => cmd_mounts(w),
        "mount_fat" => cmd_mount_fat(w, args),

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
    w.write_line("  spawn <demo>   — inicia processo de usuario (hello|counter|ipc|wait)");
    w.write_line("  sched          — estatisticas do escalonador");
    w.write_line("  uptime         — ticks de timer desde o boot");
    w.write_line("  disk           — detecta discos IDE/ATA e particoes MBR (alias: ata)");
    w.write_line("  lspci          — lista dispositivos no barramento PCI (alias: pci)");
    w.write_line("  ls [path]      — lista conteudo de diretorio no VFS (ex: ls /dev)");
    w.write_line("  cat <file>     — exibe conteudo de arquivo no VFS");
    w.write_line("  mounts         — lista pontos de montagem ativos no VFS");
    w.write_line("  mount_fat [lba]— monta particao FAT32 do disco em /disk");
    w.write_line("  gui            — inicia a interface grafica com janelas e mouse (alias: desktop)");
    w.write_line("  reboot         — reinicia o sistema (triple fault)");
    w.write_line("");
    w.set_color(Color::DarkGray, Color::Black);
    w.write_line("  [K] = privilegio Ring 0 — acesso direto ao kernel");
    w.set_color(Color::LightGray, Color::Black);
}

/// `gui` / `desktop` — Inicia o ambiente gráfico interativo com janelas e mouse.
fn cmd_gui(w: &mut Writer) {
    w.set_color(Color::LightCyan, Color::Black);
    w.write_line("Iniciando ambiente grafico Jinn Desktop...");
    w.set_color(Color::LightGray, Color::Black);
    crate::gui::run_desktop();
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
            scheduler::TaskState::Sleeping(_) => ("Dormindo ", Color::LightBlue),
            scheduler::TaskState::WaitingOn(_) => ("Esperando", Color::LightCyan),
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

/// `spawn <arquivo.elf | caminho | nome>` — Inicia um processo de usuário a partir do VFS ou módulo ELF.
fn cmd_spawn(w: &mut Writer, args: &str) {
    let target = args.trim();
    if target.is_empty() {
        w.set_color(Color::LightRed, Color::Black);
        w.write_line("Uso: spawn <arquivo.elf | caminho | nome>");
        w.write_line("  Ex: spawn /disk/ola.elf");
        w.write_line("      spawn /disk/ola-mundo.elf");
        w.write_line("      spawn ola-mundo");
        w.set_color(Color::LightGray, Color::Black);
        return;
    }

    // Buffer estático de carregamento de binários ELF do VFS (até 64 KiB)
    static mut ELF_LOAD_BUF: [u8; 64 * 1024] = [0u8; 64 * 1024];
    let mut loaded_via_vfs = false;
    let mut elf_len = 0;

    // 1. Se começa com '/', tenta carregar diretamente do caminho absoluto no VFS
    if target.starts_with('/') {
        unsafe {
            let buf_ptr = core::ptr::addr_of_mut!(ELF_LOAD_BUF);
            if let Ok(n) = vfs::read_file(target, &mut (*buf_ptr)) {
                elf_len = n;
                loaded_via_vfs = true;
            }
        }
    } else {
        // Tenta buscar no ponto de montagem /disk/
        let mut path_buf = [0u8; 64];
        let base = b"/disk/";
        let mut idx = 0;
        for &b in base {
            path_buf[idx] = b;
            idx += 1;
        }
        for b in target.bytes() {
            if idx < path_buf.len() - 5 {
                path_buf[idx] = b;
                idx += 1;
            }
        }
        if let Ok(p) = core::str::from_utf8(&path_buf[..idx]) {
            unsafe {
                let buf_ptr = core::ptr::addr_of_mut!(ELF_LOAD_BUF);
                if let Ok(n) = vfs::read_file(p, &mut (*buf_ptr)) {
                    elf_len = n;
                    loaded_via_vfs = true;
                }
            }
        }
        if !loaded_via_vfs {
            // Tenta adicionar a extensão .elf
            let mut idx2 = idx;
            for &b in b".elf" {
                if idx2 < path_buf.len() {
                    path_buf[idx2] = b;
                    idx2 += 1;
                }
            }
            if let Ok(p2) = core::str::from_utf8(&path_buf[..idx2]) {
                unsafe {
                    let buf_ptr = core::ptr::addr_of_mut!(ELF_LOAD_BUF);
                    if let Ok(n) = vfs::read_file(p2, &mut (*buf_ptr)) {
                        elf_len = n;
                        loaded_via_vfs = true;
                    }
                }
            }
        }
    }

    let sched = scheduler::get();
    let pid = if loaded_via_vfs {
        let elf_slice = unsafe {
            let buf_ptr = core::ptr::addr_of!(ELF_LOAD_BUF);
            core::slice::from_raw_parts((*buf_ptr).as_ptr(), elf_len)
        };
        w.set_color(Color::LightCyan, Color::Black);
        w.write_string("[VFS] Executavel carregado do disco: ");
        w.write_decimal(elf_len);
        w.write_line(" bytes");
        sched.add_user_task("disk-app[U3]", elf_slice)
    } else {
        // Fallback: carregar módulo ELF injetado pelo bootloader Limine
        match crate::limine::get_module(0) {
            Some(data) => {
                let name = match target {
                    "hello" | "ola" | "ola-mundo" => "ola-mundo[U3]",
                    _ => "user-app[U3]",
                };
                sched.add_user_task(name, data)
            }
            None => {
                w.set_color(Color::LightRed, Color::Black);
                w.write_line("[ERRO] Arquivo nao encontrado no VFS e nenhum modulo no bootloader.");
                w.set_color(Color::LightGray, Color::Black);
                return;
            }
        }
    };

    if pid == 0 {
        w.set_color(Color::LightRed, Color::Black);
        w.write_line("[ERRO] Falha ao carregar processo ELF para Ring 3.");
        w.set_color(Color::LightGray, Color::Black);
        return;
    }
    crate::ipc::register(pid);

    w.set_color(Color::LightGreen, Color::Black);
    w.write_string("[OK] Processo de usuario criado: PID=");
    w.write_decimal(pid);
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
    let ctx_sw  = scheduler::context_switches();

    w.set_color(Color::Yellow, Color::Black);
    w.write_line("=== Estatisticas do Escalonador ===");
    w.set_color(Color::LightGray, Color::Black);

    w.write_string("  Preempcoes totais  : ");
    w.write_decimal(ticks as usize);
    w.write_line("");

    w.write_string("  Trocas de contexto : ");
    w.write_decimal(ctx_sw as usize);
    w.write_line("");

    w.write_string("  Tarefas ativas     : ");
    w.write_decimal(count);
    w.write_string(" / ");
    w.write_decimal(scheduler::MAX_TASKS);
    w.write_line("");

    w.write_string("  Tarefa atual       : PID=");
    w.write_decimal(cur_pid);
    w.write_line("");

    w.write_string("  Ticks de timer     : ");
    w.write_decimal(timer::ticks() as usize);
    w.write_line("");

    // Tabela de CPU ticks por tarefa
    w.set_color(Color::DarkGray, Color::Black);
    w.write_line("  --- CPU por Tarefa ---");
    w.set_color(Color::LightGray, Color::Black);
    for task in sched.tasks.iter().flatten() {
        w.write_string("    PID ");
        w.write_decimal(task.id);
        w.write_string(" (");
        w.write_string(task.name);
        w.write_string("): ");
        w.write_decimal(task.cpu_ticks as usize);
        w.write_string(" ticks, ");
        w.write_decimal(task.ctx_switches as usize);
        w.write_line(" ctx switches");
    }

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

/// `disk` — Identifica e inspeciona discos IDE/ATA e tabela de particoes MBR.
fn cmd_disk(w: &mut Writer) {
    w.set_color(Color::Yellow, Color::Black);
    w.write_line("=== Sub-sistema de Armazenamento IDE/ATA (Primary Bus) ===");
    w.set_color(Color::LightGray, Color::Black);

    let mut found = 0;
    for drive in 0..=1 {
        let drive_name = if drive == 0 { "Master (0)" } else { "Slave (1)" };
        w.set_color(Color::LightCyan, Color::Black);
        w.write_string("IDE Primary ");
        w.write_string(drive_name);
        w.write_string(": ");

        match crate::drivers::ide::identify(drive) {
            Ok(info) => {
                found += 1;
                w.set_color(Color::LightGreen, Color::Black);
                if info.is_atapi {
                    w.write_string("Dispositivo ATAPI (CD-ROM/DVD)");
                } else {
                    w.write_string("Disco Rigido ATA");
                }
                w.write_line("");
                w.set_color(Color::LightGray, Color::Black);

                // Modelo
                w.write_string("  Modelo   : ");
                if let Ok(model_str) = core::str::from_utf8(&info.model) {
                    w.write_line(model_str.trim());
                } else {
                    w.write_line("N/A");
                }

                // Serial
                w.write_string("  Serial   : ");
                if let Ok(ser_str) = core::str::from_utf8(&info.serial) {
                    w.write_line(ser_str.trim());
                } else {
                    w.write_line("N/A");
                }

                // Capacidade
                w.write_string("  Capacidade: ");
                w.write_decimal(info.size_mb as usize);
                w.write_string(" MB (");
                w.write_decimal(info.sectors_lba28 as usize);
                w.write_line(" setores LBA28)");

                // Inspeciona MBR
                match crate::drivers::ide::read_mbr(drive) {
                    Ok(mbr) => {
                        if mbr.is_valid {
                            w.set_color(Color::LightCyan, Color::Black);
                            w.write_line("  Particoes MBR (Setor 0 valido):");
                            w.set_color(Color::LightGray, Color::Black);
                            let mut part_count = 0;
                            for (idx, part_opt) in mbr.partitions.iter().enumerate() {
                                if let Some(part) = part_opt {
                                    part_count += 1;
                                    w.write_string("    [P");
                                    w.write_decimal(idx + 1);
                                    w.write_string("] ");
                                    if part.bootable {
                                        w.set_color(Color::LightGreen, Color::Black);
                                        w.write_string("*BOOT* ");
                                        w.set_color(Color::LightGray, Color::Black);
                                    }
                                    w.write_string(crate::drivers::ide::partition_type_name(part.partition_type));
                                    w.write_string(" | LBA ");
                                    w.write_decimal(part.start_lba as usize);
                                    w.write_string("..+");
                                    w.write_decimal(part.sector_count as usize);
                                    w.write_string(" (");
                                    w.write_decimal(part.size_mb as usize);
                                    w.write_line(" MB)");
                                }
                            }
                            if part_count == 0 {
                                w.write_line("    (Nenhuma particao definida no MBR)");
                            }
                        } else {
                            w.set_color(Color::DarkGray, Color::Black);
                            w.write_line("  Setor 0: Sem assinatura de boot MBR (0x55AA)");
                            w.set_color(Color::LightGray, Color::Black);
                        }
                    }
                    Err(e) => {
                        w.set_color(Color::LightRed, Color::Black);
                        w.write_string("  Falha ao ler MBR: ");
                        w.write_line(e);
                        w.set_color(Color::LightGray, Color::Black);
                    }
                }
            }
            Err(e) => {
                w.set_color(Color::DarkGray, Color::Black);
                w.write_line(e);
                w.set_color(Color::LightGray, Color::Black);
            }
        }
    }

    if found == 0 {
        w.set_color(Color::DarkGray, Color::Black);
        w.write_line("Nenhum drive IDE conectado no canal primario.");
        w.set_color(Color::LightGray, Color::Black);
    }
}

/// `lspci` — Lista todos os dispositivos detectados no barramento PCI.
fn cmd_lspci(w: &mut Writer) {
    w.set_color(Color::Yellow, Color::Black);
    w.write_line("=== Dispositivos PCI Detectados (Config Space #1) ===");
    w.set_color(Color::LightGray, Color::Black);

    let devs = crate::drivers::pci::devices();
    if devs.is_empty() {
        w.write_line("Nenhum dispositivo PCI detectado.");
        return;
    }

    for dev_opt in devs.iter() {
        if let Some(dev) = dev_opt {
            w.set_color(Color::DarkGray, Color::Black);
            w.write_string("[");
            if dev.bus < 10 { w.write_string("0"); }
            w.write_decimal(dev.bus as usize);
            w.write_string(":");
            if dev.slot < 10 { w.write_string("0"); }
            w.write_decimal(dev.slot as usize);
            w.write_string(".");
            w.write_decimal(dev.func as usize);
            w.write_string("] ");

            w.set_color(Color::LightCyan, Color::Black);
            w.write_hex_u16(dev.vendor_id);
            w.write_string(":");
            w.write_hex_u16(dev.device_id);
            w.write_string(" ");

            w.set_color(Color::White, Color::Black);
            w.write_string(crate::drivers::pci::class_name(dev.class_code, dev.subclass));

            w.set_color(Color::LightGreen, Color::Black);
            w.write_string(" (");
            w.write_string(crate::drivers::pci::vendor_name(dev.vendor_id));
            w.write_line(")");

            w.set_color(Color::LightGray, Color::Black);
        }
    }
}

/// `ls` — Lista o conteúdo de um diretório no VFS.
fn cmd_ls(w: &mut Writer, args: &str) {
    let path = if args.is_empty() { "/" } else { args };
    w.set_color(Color::Yellow, Color::Black);
    w.write_string("Conteudo de ");
    w.write_string(path);
    w.write_line(":");
    w.set_color(Color::LightGray, Color::Black);

    let mut count = 0;
    let res = crate::vfs::readdir(path, |name, node_type| {
        count += 1;
        match node_type {
            crate::vfs::NodeType::Directory => {
                w.set_color(Color::LightCyan, Color::Black);
                w.write_string("  [DIR]  ");
                w.write_string(name);
                w.write_line("/");
            }
            crate::vfs::NodeType::CharDevice | crate::vfs::NodeType::BlockDevice => {
                w.set_color(Color::Yellow, Color::Black);
                w.write_string("  [DEV]  ");
                w.write_line(name);
            }
            _ => {
                w.set_color(Color::White, Color::Black);
                w.write_string("  [FILE] ");
                w.write_line(name);
            }
        }
        w.set_color(Color::LightGray, Color::Black);
    });

    if let Err(e) = res {
        w.set_color(Color::LightRed, Color::Black);
        w.write_string("Erro ao listar diretorio: ");
        w.write_line(match e {
            crate::vfs::VfsError::NotFound => "Caminho nao encontrado",
            crate::vfs::VfsError::NotADirectory => "Nao e um diretorio",
            _ => "Falha de VFS / I/O",
        });
        w.set_color(Color::LightGray, Color::Black);
    } else if count == 0 {
        w.set_color(Color::DarkGray, Color::Black);
        w.write_line("  (Diretorio vazio)");
        w.set_color(Color::LightGray, Color::Black);
    }
}

/// `cat` — Exibe o conteúdo de um arquivo de texto no VFS.
fn cmd_cat(w: &mut Writer, args: &str) {
    if args.is_empty() {
        w.set_color(Color::Yellow, Color::Black);
        w.write_line("Uso: cat <caminho_do_arquivo>");
        w.set_color(Color::LightGray, Color::Black);
        return;
    }

    match crate::vfs::open(args, crate::vfs::O_RDONLY) {
        Ok(handle) => {
            let mut buf = [0u8; 512];
            let mut offset = 0u64;
            loop {
                match crate::vfs::read(args, handle, offset, &mut buf) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        for &byte in &buf[..n] {
                            w.write_byte(byte);
                        }
                        offset += n as u64;
                    }
                    Err(_) => {
                        w.set_color(Color::LightRed, Color::Black);
                        w.write_line("\n[Erro de leitura]");
                        w.set_color(Color::LightGray, Color::Black);
                        break;
                    }
                }
            }
            let _ = crate::vfs::close(args, handle);
        }
        Err(e) => {
            w.set_color(Color::LightRed, Color::Black);
            w.write_string("Erro ao abrir arquivo '");
            w.write_string(args);
            w.write_string("': ");
            w.write_line(match e {
                crate::vfs::VfsError::NotFound => "Nao encontrado",
                crate::vfs::VfsError::IsADirectory => "E um diretorio",
                _ => "Erro de acesso",
            });
            w.set_color(Color::LightGray, Color::Black);
        }
    }
}

/// `mounts` — Lista pontos de montagem no VFS.
fn cmd_mounts(w: &mut Writer) {
    w.set_color(Color::Yellow, Color::Black);
    w.write_line("=== Pontos de Montagem do VFS ===");
    w.set_color(Color::LightGray, Color::Black);

    crate::vfs::list_mounts(|prefix, fs_name| {
        w.set_color(Color::LightCyan, Color::Black);
        w.write_string("  ");
        w.write_string(prefix);
        w.set_color(Color::DarkGray, Color::Black);
        w.write_string(" -> tipo: ");
        w.set_color(Color::White, Color::Black);
        w.write_line(fs_name);
        w.set_color(Color::LightGray, Color::Black);
    });
}

/// `mount_fat` — Monta partição FAT32 do disco em /disk.
fn cmd_mount_fat(w: &mut Writer, args: &str) {
    let drive = 0u8; // Primary Master
    let mut target_lba = 0u32;

    if !args.is_empty() {
        let mut val = 0u32;
        let mut valid = false;
        for b in args.bytes() {
            if (b'0'..=b'9').contains(&b) {
                val = val * 10 + (b - b'0') as u32;
                valid = true;
            } else {
                valid = false;
                break;
            }
        }
        if valid {
            target_lba = val;
        }
    } else {
        match crate::drivers::ide::read_mbr(drive) {
            Ok(mbr) if mbr.is_valid => {
                for part_opt in mbr.partitions.iter().flatten() {
                    if part_opt.partition_type == 0x0B || part_opt.partition_type == 0x0C {
                        target_lba = part_opt.start_lba;
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    w.set_color(Color::LightCyan, Color::Black);
    w.write_string("Montando FAT32 no Drive 0, LBA ");
    w.write_decimal(target_lba as usize);
    w.write_line(" em /disk...");
    w.set_color(Color::LightGray, Color::Black);

    match crate::vfs::fat32::mount_fat32_partition("/disk", drive, target_lba) {
        Ok(()) => {
            w.set_color(Color::LightGreen, Color::Black);
            w.write_line("[OK] Volume FAT32 montado com sucesso em /disk!");
            w.set_color(Color::LightGray, Color::Black);
            w.write_line("Use 'ls /disk' para ver os arquivos.");
        }
        Err(e) => {
            w.set_color(Color::LightRed, Color::Black);
            w.write_string("[Falha] ");
            w.write_line(e);
            w.set_color(Color::LightGray, Color::Black);
        }
    }
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

/// Ponto de entrada do shell como uma task (Ring 0).
pub fn task_entry() {
    let mut w = Writer::new();
    w.set_position(21, 0);
    run(&mut w);
}

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
            if (status & 1) != 0 && (status & 0x20) == 0 {
                let sc: u8;
                core::arch::asm!(
                    "in al, dx",
                    out("al") sc,
                    in("dx") 0x60u16,
                    options(nomem, nostack)
                );
                crate::interrupts::ps2_keyboard::process_scancode(sc);
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

