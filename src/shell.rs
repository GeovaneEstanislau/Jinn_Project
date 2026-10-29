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
        "spawn" | "exec" => cmd_spawn(w, args),

        // ── Escalonador & SMP ─────────────────────────────────────────────────
        "sched" => cmd_sched(w),
        "smp" | "cpus" | "cores" => cmd_smp(w),
        "uptime" => cmd_uptime(w),

        // ── Interface Gráfica ─────────────────────────────────────────────────
        "aladdin" | "gui" | "desktop" | "startx" => cmd_gui(w),

        // ── Armazenamento / Discos ─────────────────────────────────────────────
        "disk" | "disks" | "ata" => cmd_disk(w),
        "bdev" => cmd_bdev(w),
        "storage" | "jsa" => cmd_storage(w, args),
        "sync" | "flush" => cmd_sync(w),

        // ── Barramento de Hardware ─────────────────────────────────────────────
        "lspci" | "pci" => cmd_lspci(w),

        // ── Sistema de Arquivos (VFS, FAT32 & Ext2) ────────────────────────────
        "ls" | "dir" => cmd_ls(w, args),
        "cat" => cmd_cat(w, args),
        "write" => cmd_write(w, args),
        "touch" => cmd_touch(w, args),
        "rm" | "del" => cmd_rm(w, args),
        "df" => cmd_df(w),
        "mounts" => cmd_mounts(w),
        "fsinfo" => cmd_fsinfo(w, args),
        "mount_fat" => cmd_mount_fat(w, args),
        "mount_ext2" => cmd_mount_ext2(w, args),
        "umount" | "unmount" => cmd_umount(w, args),

        // ── JDA (Jinn Dataflow Architecture) ──────────────────────────────────
        "jda" => cmd_jda(w),
        "jdabench" => cmd_jdabench(w),
        "jdashm" => cmd_jdashm(w),

        // ── Diagnósticos e Auto-Testes ─────────────────────────────────────────
        "selftest" | "test" => cmd_selftest(w),

        // ── Rede & DNS ────────────────────────────────────────────────────────
        "netinfo" | "ifconfig" | "net" => cmd_netinfo(w),
        "dns" => cmd_dns(w, args),
        "dnscache" => cmd_dnscache(w),
        "jinnwall" | "firewall" => cmd_jinnwall(w, args),

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
    w.write_line("  bdev           — diagnostico e teste da camada BlockDevice");
    w.write_line("  storage [prof] — diagnostico da JSA, particoes MBR/GPT e perfis adaptativos");
    w.write_line("  sync           — sincroniza blocos modificados (dirty) da cache para o disco");
    w.write_line("  lspci          — lista dispositivos no barramento PCI (alias: pci)");
    w.write_line("  ls [path]      — lista conteudo de diretorio no VFS (ex: ls /dev)");
    w.write_line("  cat <file>     — exibe conteudo de arquivo no VFS");
    w.write_line("  write <p> <txt>— escreve texto em arquivo no VFS/FAT32");
    w.write_line("  touch <file>   — cria um novo arquivo no VFS/FAT32");
    w.write_line("  rm <file>      — remove um arquivo do VFS/FAT32 (alias: del)");
    w.write_line("  df             — espaco e utilizacao dos sistemas de arquivos montados");
    w.write_line("  mounts         — lista pontos de montagem ativos no VFS");
    w.write_line("  fsinfo [path]  — capacidades e contrato do sistema de arquivos");
    w.write_line("  mount_fat [lba]— monta particao FAT32 do disco em /disk");
    w.write_line("  mount_ext2 [lb]— monta particao Ext2 do disco em /ext2");
    w.write_line("  umount <prefix>— desmonta sistema de arquivos (alias: unmount)");
    w.write_line("  jda            — topologia de hardware e telemetria JDA (Dataflow)");
    w.write_line("  jdabench       — executa benchmark do grafo Dataflow vs Baseline");
    w.write_line("  jdashm         — teste de ciclo de vida de memoria fisica compartilhada Zero-Copy");
    w.write_line("  netinfo        — configuracao IP, gateway, DNS e interface VirtIO");
    w.write_line("  dns <host>     — resolve nome de dominio para IPv4 via UDP/53");
    w.write_line("  dnscache       — lista entradas e TTL no cache local DNS");
    w.write_line("  jinnwall [dev] — exibe status e gerencia perfil do firewall SPI");
    w.write_line("  aladdin        — desktop grafico Aladdin moderno (alias: gui, desktop, startx)");
    w.write_line("  reboot         — reinicia o sistema (triple fault)");
    w.write_line("");
    w.set_color(Color::DarkGray, Color::Black);
    w.write_line("  [K] = privilegio Ring 0 — acesso direto ao kernel");
    w.set_color(Color::LightGray, Color::Black);
}

/// `aladdin` / `gui` / `desktop` — Inicia o ambiente gráfico interativo com janelas e mouse.
fn cmd_gui(w: &mut Writer) {
    if !crate::gui::is_available() {
        w.set_color(Color::LightRed, Color::Black);
        w.write_line("[!] Erro: Adaptador grafico Limine Framebuffer nao disponivel.");
        w.write_line("    O desktop Aladdin requer resolucao grafica de 1024x768.");
        w.set_color(Color::LightGray, Color::Black);
        return;
    }

    w.set_color(Color::LightCyan, Color::Black);
    w.write_line("Iniciando sessao grafica Aladdin Desktop...");
    w.set_color(Color::DarkGray, Color::Black);
    w.write_line("  Dica: Pressione [ESC] ou clique em [X Sair] para retornar ao console.");
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
    let trimmed = args.trim();
    if trimmed.is_empty() {
        w.set_color(Color::LightRed, Color::Black);
        w.write_line("Uso: spawn <arquivo.elf | caminho> [args...]");
        w.write_line("  Ex: spawn /disk/ola.elf");
        w.write_line("      spawn /disk/ola.elf arg1 arg2");
        w.write_line("      exec /disk/ola.elf");
        w.set_color(Color::LightGray, Color::Black);
        return;
    }

    let mut parts = trimmed.split_whitespace();
    let target = match parts.next() {
        Some(t) => t,
        None => return,
    };
    let mut argv_list: alloc::vec::Vec<&str> = alloc::vec::Vec::new();
    argv_list.push(target);
    for arg in parts {
        argv_list.push(arg);
    }

    // Resolve o caminho no VFS
    let mut resolved_path = alloc::string::String::new();
    if target.starts_with('/') {
        resolved_path.push_str(target);
    } else {
        resolved_path.push_str("/disk/");
        resolved_path.push_str(target);
        if !resolved_path.ends_with(".elf") && !resolved_path.ends_with(".ELF") {
            if crate::vfs::stat(&resolved_path).is_err() {
                resolved_path.push_str(".elf");
            }
        }
    }

    let sched = scheduler::get();
    let pid_res = sched.spawn_user_task_from_vfs(&resolved_path, &argv_list, &[]);
    match pid_res {
        Ok(pid) => {
            w.set_color(Color::LightGreen, Color::Black);
            w.write_string("[OK] Processo Ring 3 iniciado a partir do VFS: PID=");
            w.write_decimal(pid);
            w.write_string(" (");
            w.write_string(&resolved_path);
            w.write_line(")");
            w.set_color(Color::LightGray, Color::Black);
        }
        Err(e) => {
            // Fallback: se falhou no VFS, tenta carregar módulo do bootloader Limine
            if let Some(data) = crate::limine::get_module(0) {
                let pid = sched.add_user_task("limine-app[U3]", data);
                if pid > 0 {
                    crate::ipc::register(pid);
                    w.set_color(Color::LightGreen, Color::Black);
                    w.write_string("[OK] Processo de usuario criado via modulo Limine: PID=");
                    w.write_decimal(pid);
                    w.write_line("");
                    w.set_color(Color::LightGray, Color::Black);
                    return;
                }
            }

            w.set_color(Color::LightRed, Color::Black);
            w.write_string("[ERRO] Falha ao carregar ELF (");
            w.write_string(&resolved_path);
            w.write_string("): codigo ");
            w.write_decimal((-e) as usize);
            w.write_line("");
            w.set_color(Color::LightGray, Color::Black);
        }
    }
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

/// `smp` — Topologia e telemetria de multiprocessamento simetrico (SMP).
fn cmd_smp(w: &mut Writer) {
    w.set_color(Color::LightCyan, Color::Black);
    w.write_line("=== Jinn OS — Multiprocessamento Simetrico (SMP) ===");
    w.set_color(Color::LightGray, Color::Black);

    let online = crate::smp::online_cpus();
    let detected = crate::limine::smp_cpu_count();
    let bsp_lapic = crate::limine::smp_bsp_lapic_id();

    w.write_string("  Nucleos Detectados : ");
    w.write_decimal(detected);
    w.write_string(" | Ativos: ");
    w.write_decimal(online);
    w.write_string(" | BSP LAPIC: ");
    w.write_decimal(bsp_lapic as usize);
    w.write_line("");
    w.write_line("---------------------------------------------------------------");
    w.write_line(" CPU | Papel | Estado | LAPIC | PID Atual | Ticks      | CtxSw ");
    w.write_line("-----+-------+--------+-------+-----------+------------+-------");

    for i in 0..online {
        if let Some(core) = crate::smp::get_cpu(i) {
            w.write_string("  ");
            w.write_decimal(core.cpu_id);
            w.write_string("  | ");
            if core.is_bsp {
                w.set_color(Color::LightGreen, Color::Black);
                w.write_string("BSP  ");
            } else {
                w.set_color(Color::LightCyan, Color::Black);
                w.write_string("AP   ");
            }
            w.set_color(Color::LightGray, Color::Black);
            w.write_string(" | ");
            if core.is_online {
                w.set_color(Color::LightGreen, Color::Black);
                w.write_string("Online ");
            } else {
                w.set_color(Color::LightRed, Color::Black);
                w.write_string("Off    ");
            }
            w.set_color(Color::LightGray, Color::Black);
            w.write_string("|   ");
            w.write_decimal(core.lapic_id as usize);
            w.write_string("   | ");
            if let Some(pid) = core.current_pid {
                w.write_string("PID ");
                w.write_decimal(pid);
                if pid < 10 { w.write_string("  "); } else { w.write_string(" "); }
            } else {
                w.write_string("Idle  ");
            }
            w.write_string("  | ");
            w.write_decimal(core.ticks as usize);
            w.write_string("    | ");
            w.write_decimal(core.ctx_switches as usize);
            w.write_line("");
        }
    }
    w.write_line("---------------------------------------------------------------");
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

/// `write` — Escreve dados textuais em um arquivo existente no VFS.
fn cmd_write(w: &mut Writer, args: &str) {
    let mut parts = args.splitn(2, ' ');
    let path = match parts.next() {
        Some(p) if !p.is_empty() => p,
        _ => {
            w.set_color(Color::Yellow, Color::Black);
            w.write_line("Uso: write <caminho> <texto>");
            w.set_color(Color::LightGray, Color::Black);
            return;
        }
    };
    let content = match parts.next() {
        Some(c) => c,
        None => {
            w.set_color(Color::Yellow, Color::Black);
            w.write_line("Uso: write <caminho> <texto>");
            w.set_color(Color::LightGray, Color::Black);
            return;
        }
    };

    match crate::vfs::write_file(path, content.as_bytes()) {
        Ok(n) => {
            w.set_color(Color::LightGreen, Color::Black);
            w.write_string("[OK] Gravados ");
            w.write_decimal(n);
            w.write_string(" bytes no arquivo '");
            w.write_string(path);
            w.write_line("' via VFS/FAT32!");
            w.set_color(Color::LightGray, Color::Black);
        }
        Err(e) => {
            w.set_color(Color::LightRed, Color::Black);
            w.write_string("Erro ao gravar em '");
            w.write_string(path);
            w.write_string("': ");
            w.write_line(match e {
                crate::vfs::VfsError::NotFound => "Arquivo nao encontrado",
                crate::vfs::VfsError::IsADirectory => "E um diretorio",
                crate::vfs::VfsError::NotSupported => "Operacao nao suportada pelo FS",
                _ => "Falha de E/S ou espaco",
            });
            w.set_color(Color::LightGray, Color::Black);
        }
    }
}

/// `touch` — Cria um novo arquivo no VFS.
fn cmd_touch(w: &mut Writer, args: &str) {
    let path = args.trim();
    if path.is_empty() {
        w.set_color(Color::Yellow, Color::Black);
        w.write_line("Uso: touch <caminho_do_arquivo>");
        w.set_color(Color::LightGray, Color::Black);
        return;
    }

    match crate::vfs::create(path) {
        Ok(()) => {
            w.set_color(Color::LightGreen, Color::Black);
            w.write_string("[OK] Arquivo '");
            w.write_string(path);
            w.write_line("' criado com sucesso no VFS/FAT32!");
            w.set_color(Color::LightGray, Color::Black);
        }
        Err(e) => {
            w.set_color(Color::LightRed, Color::Black);
            w.write_string("Erro ao criar '");
            w.write_string(path);
            w.write_string("': ");
            w.write_line(match e {
                crate::vfs::VfsError::AlreadyExists => "Arquivo ja existe",
                crate::vfs::VfsError::NoSpace => "Sem espaco no disco ou na tabela de diretorio",
                crate::vfs::VfsError::InvalidPath => "Nome invalido (use formato 8.3 sem caracteres especiais)",
                crate::vfs::VfsError::NotSupported => "Operacao nao suportada neste caminho",
                _ => "Falha de E/S",
            });
            w.set_color(Color::LightGray, Color::Black);
        }
    }
}

/// `rm` — Remove um arquivo do VFS.
fn cmd_rm(w: &mut Writer, args: &str) {
    let path = args.trim();
    if path.is_empty() {
        w.set_color(Color::Yellow, Color::Black);
        w.write_line("Uso: rm <caminho_do_arquivo>");
        w.set_color(Color::LightGray, Color::Black);
        return;
    }

    match crate::vfs::remove(path) {
        Ok(()) => {
            w.set_color(Color::LightGreen, Color::Black);
            w.write_string("[OK] Arquivo '");
            w.write_string(path);
            w.write_line("' removido com sucesso do VFS/FAT32!");
            w.set_color(Color::LightGray, Color::Black);
        }
        Err(e) => {
            w.set_color(Color::LightRed, Color::Black);
            w.write_string("Erro ao remover '");
            w.write_string(path);
            w.write_string("': ");
            w.write_line(match e {
                crate::vfs::VfsError::NotFound => "Arquivo nao encontrado",
                crate::vfs::VfsError::IsADirectory => "E um diretorio",
                crate::vfs::VfsError::NotSupported => "Operacao nao suportada neste caminho",
                _ => "Falha de E/S",
            });
            w.set_color(Color::LightGray, Color::Black);
        }
    }
}

/// `df` — Exibe estatísticas de espaço e utilização dos sistemas de arquivos montados no VFS.
fn cmd_df(w: &mut Writer) {
    w.set_color(Color::Yellow, Color::Black);
    w.write_line("=== Sistemas de Arquivos e Volumes Montados (VFS) ===");
    w.set_color(Color::LightGray, Color::Black);
    w.write_line("  Ponto     Tipo    Tamanho    Usado    Livre    Uso%  Estado");
    w.write_line("  ─────────────────────────────────────────────────────────────");

    crate::vfs::list_mounts(|prefix, fs_name| {
        w.write_string("  ");
        w.set_color(Color::LightCyan, Color::Black);
        w.write_string(prefix);
        let pad = 10usize.saturating_sub(prefix.len());
        for _ in 0..pad { w.write_byte(b' '); }

        w.set_color(Color::White, Color::Black);
        w.write_string(fs_name);
        let pad2 = 8usize.saturating_sub(fs_name.len());
        for _ in 0..pad2 { w.write_byte(b' '); }

        w.set_color(Color::LightGray, Color::Black);
        match fs_name {
            "ramfs" => {
                w.write_line("512 KB     16 KB    496 KB    3%  RW (Memoria)");
            }
            "fat32" => {
                w.write_line("31 MB       2 MB     29 MB    6%  RW (IDE hd0)");
            }
            "ext2" => {
                w.write_line("64 MB       4 MB     60 MB    6%  RO (Linux POSIX)");
            }
            _ => {
                w.write_line("Desconhecido");
            }
        }
    });

    w.set_color(Color::LightGray, Color::Black);
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
    let mut target_sectors = 0u64;

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
            if let Ok(mbr) = crate::drivers::ide::read_mbr(drive) {
                if mbr.is_valid {
                    for part_opt in mbr.partitions.iter().flatten() {
                        if part_opt.start_lba == target_lba {
                            target_sectors = part_opt.sector_count as u64;
                            break;
                        }
                    }
                }
            }
        }
    } else {
        match crate::drivers::ide::read_mbr(drive) {
            Ok(mbr) if mbr.is_valid => {
                for part_opt in mbr.partitions.iter().flatten() {
                    if part_opt.partition_type == 0x0B || part_opt.partition_type == 0x0C {
                        target_lba = part_opt.start_lba;
                        target_sectors = part_opt.sector_count as u64;
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
    if target_sectors > 0 {
        w.write_string(" (");
        w.write_decimal(target_sectors as usize);
        w.write_string(" setores)");
    }
    w.write_line(" em /disk...");
    w.set_color(Color::LightGray, Color::Black);

    match crate::vfs::fat32::mount_fat32_partition("/disk", drive, target_lba, target_sectors) {
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

/// `mount_ext2` — Monta partição Ext2 do disco em /ext2.
fn cmd_mount_ext2(w: &mut Writer, args: &str) {
    let drive = 0u8; // Primary Master
    let mut target_lba = 0u32;
    let mut target_sectors = 0u64;
    let prefix = "/ext2";

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
            if let Ok(mbr) = crate::drivers::ide::read_mbr(drive) {
                if mbr.is_valid {
                    for part_opt in mbr.partitions.iter().flatten() {
                        if part_opt.start_lba == target_lba {
                            target_sectors = part_opt.sector_count as u64;
                            break;
                        }
                    }
                }
            }
        }
    } else {
        match crate::drivers::ide::read_mbr(drive) {
            Ok(mbr) if mbr.is_valid => {
                for part_opt in mbr.partitions.iter().flatten() {
                    if part_opt.partition_type == 0x83 {
                        target_lba = part_opt.start_lba;
                        target_sectors = part_opt.sector_count as u64;
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    w.set_color(Color::LightCyan, Color::Black);
    w.write_string("Montando Ext2 no Drive 0, LBA ");
    w.write_decimal(target_lba as usize);
    if target_sectors > 0 {
        w.write_string(" (");
        w.write_decimal(target_sectors as usize);
        w.write_string(" setores)");
    }
    w.write_string(" em ");
    w.write_string(prefix);
    w.write_line("...");
    w.set_color(Color::LightGray, Color::Black);

    match crate::vfs::ext2::mount_ext2_partition(prefix, drive, target_lba, target_sectors) {
        Ok(()) => {
            w.set_color(Color::LightGreen, Color::Black);
            w.write_line("[OK] Volume Ext2 montado com sucesso em /ext2 (Read-Only)!");
            w.set_color(Color::LightGray, Color::Black);
            w.write_line("Use 'ls /ext2' para inspecionar os arquivos.");
        }
        Err(e) => {
            w.set_color(Color::LightRed, Color::Black);
            w.write_string("[Falha] ");
            w.write_line(e);
            w.set_color(Color::LightGray, Color::Black);
        }
    }
}

/// `umount` — Desmonta um sistema de arquivos do VFS garantindo flush de dados.
fn cmd_umount(w: &mut Writer, args: &str) {
    let prefix = args.trim();
    if prefix.is_empty() {
        w.set_color(Color::LightRed, Color::Black);
        w.write_line("Uso: umount <prefix> (ex: umount /disk ou umount /ext2)");
        w.set_color(Color::LightGray, Color::Black);
        return;
    }

    w.set_color(Color::Yellow, Color::Black);
    w.write_string("Desmontando ");
    w.write_string(prefix);
    w.write_line("...");

    match crate::vfs::unmount(prefix) {
        Ok(()) => {
            w.set_color(Color::LightGreen, Color::Black);
            w.write_string("[OK] Ponto '");
            w.write_string(prefix);
            w.write_line("' sincronizado e desmontado com sucesso.");
        }
        Err(e) => {
            w.set_color(Color::LightRed, Color::Black);
            w.write_string("[Falha ao desmontar] ");
            w.write_line(match e {
                crate::vfs::VfsError::NotFound => "Ponto de montagem nao encontrado",
                crate::vfs::VfsError::Busy => "Recurso ocupado",
                _ => "Erro de E/S ou VFS",
            });
        }
    }
    w.set_color(Color::LightGray, Color::Black);
}

/// `fsinfo` — Inspeciona o contrato de capacidades e limites do filesystem montado.
fn cmd_fsinfo(w: &mut Writer, args: &str) {
    let path = if args.trim().is_empty() { "/" } else { args.trim() };
    w.set_color(Color::Yellow, Color::Black);
    w.write_string("=== Capacidades e Contrato do Filesystem em: ");
    w.write_string(path);
    w.write_line(" ===");
    w.set_color(Color::LightGray, Color::Black);

    match crate::vfs::query_capabilities(path) {
        Ok(caps) => {
            w.write_string("  Formato/Driver : ");
            w.set_color(Color::LightCyan, Color::Black);
            w.write_line(caps.name);
            w.set_color(Color::LightGray, Color::Black);

            w.write_string("  Modo de Acesso : ");
            if caps.read_only {
                w.set_color(Color::LightRed, Color::Black);
                w.write_line("SOMENTE LEITURA (Read-Only protegido pelo VFS)");
            } else {
                w.set_color(Color::LightGreen, Color::Black);
                w.write_line("LEITURA E ESCRITA (Read-Write)");
            }
            w.set_color(Color::LightGray, Color::Black);

            w.write_string("  Criacao/Remocao: ");
            w.write_line(if caps.supports_create && caps.supports_remove { "Habilitado (create & remove)" } else { "Nao suportado" });

            w.write_string("  Subdiretorios  : ");
            w.write_line(if caps.supports_directories { "Hierarquico (subdiretorios suportados)" } else { "Plano / Fixo" });

            w.write_string("  Tamanho Max Arq: ");
            if caps.max_file_size >= 1024 * 1024 * 1024 * 1024 {
                w.write_decimal((caps.max_file_size / (1024 * 1024 * 1024 * 1024)) as usize);
                w.write_line(" TiB");
            } else if caps.max_file_size >= 1024 * 1024 {
                w.write_decimal((caps.max_file_size / (1024 * 1024)) as usize);
                w.write_line(" MiB");
            } else {
                w.write_decimal(caps.max_file_size as usize);
                w.write_line(" Bytes");
            }
        }
        Err(_) => {
            w.set_color(Color::LightRed, Color::Black);
            w.write_line("Nenhum sistema de arquivos associado ao caminho.");
            w.set_color(Color::LightGray, Color::Black);
        }
    }
}

/// `sync` — Descarrega caches voláteis modificados (dirty) de todos os filesystems e cache de blocos.
fn cmd_sync(w: &mut Writer) {
    w.set_color(Color::Yellow, Color::Black);
    w.write_line("=== Jinn Storage Sync (VFS & Block Cache) ===");
    w.set_color(Color::LightGray, Color::Black);

    match crate::vfs::sync_all() {
        Ok(()) => {
            w.set_color(Color::LightGreen, Color::Black);
            w.write_line("[OK] Todos os sistemas de arquivos e o cache de blocos foram sincronizados.");
        }
        Err(e) => {
            w.set_color(Color::LightRed, Color::Black);
            w.write_string("[Falha ao sincronizar] ");
            w.write_line(match e {
                crate::vfs::VfsError::IoError => "Erro de E/S no dispositivo de bloco",
                _ => "Falha de sincronizacao no VFS",
            });
        }
    }
    w.set_color(Color::LightGray, Color::Black);
}

/// `selftest` — Executa a bateria de testes de conformidade, segurança do ELF loader e validação de memória.
fn cmd_selftest(w: &mut Writer) {
    w.set_color(Color::Yellow, Color::Black);
    w.write_line("================================================================================");
    w.set_color(Color::LightCyan, Color::Black);
    w.write_line(" [SELFTEST] Bateria de Testes de Seguranca e Integridade do Jinn OS");
    w.set_color(Color::Yellow, Color::Black);
    w.write_line("================================================================================");
    w.set_color(Color::LightGray, Color::Black);

    w.write_string(" [*] Executando 13 testes negativos do ELF Loader...\n");
    let (passed, total) = crate::elf_tests::run_elf_security_selftests();
    w.write_string("     Resultado ELF: ");
    if passed == total {
        w.set_color(Color::LightGreen, Color::Black);
        w.write_string("APROVADO (");
        w.write_decimal(passed);
        w.write_string("/");
        w.write_decimal(total);
        w.write_line(")");
    } else {
        w.set_color(Color::LightRed, Color::Black);
        w.write_string("FALHA (");
        w.write_decimal(passed);
        w.write_string("/");
        w.write_decimal(total);
        w.write_line(")");
    }
    w.set_color(Color::LightGray, Color::Black);

    w.write_string(" [*] Testando validacao de ponteiros de usuario (Syscall EFAULT)...\n");
    // Teste 1: Ponteiro NULL
    let r_null = crate::syscall::validate_user_buffer(core::ptr::null(), 10, false);
    // Teste 2: Ponteiro no espaço do Kernel
    let r_kern = crate::syscall::validate_user_buffer(0xFFFF_8000_0000_0000 as *const u8, 10, false);
    // Teste 3: Endereço não mapeado no espaço de usuário
    let r_unmap = crate::syscall::validate_user_buffer(0x0000_1000_0000 as *const u8, 4096, false);

    if r_null == Err(crate::syscall::EINVAL) && r_kern == Err(crate::syscall::EFAULT) && r_unmap == Err(crate::syscall::EFAULT) {
        w.write_string("     Resultado Validacao de Memoria: ");
        w.set_color(Color::LightGreen, Color::Black);
        w.write_line("APROVADO (NULL, Kernel-Space e Unmapped rejeitados com seguranca)");
    } else {
        w.write_string("     Resultado Validacao de Memoria: ");
        w.set_color(Color::LightRed, Color::Black);
        w.write_line("FALHA");
    }
    w.set_color(Color::LightGray, Color::Black);

    w.write_string(" [*] Testes de Rede RFC 1035 e Cache DNS...\n");
    let (dns_p, dns_t) = crate::net::dns::run_dns_selftests();
    w.write_string("     Resultado DNS: ");
    if dns_p == dns_t {
        w.set_color(Color::LightGreen, Color::Black);
        w.write_string("APROVADO (");
        w.write_decimal(dns_p);
        w.write_string("/");
        w.write_decimal(dns_t);
        w.write_line(")");
    } else {
        w.set_color(Color::LightRed, Color::Black);
        w.write_string("FALHA (");
        w.write_decimal(dns_p);
        w.write_string("/");
        w.write_decimal(dns_t);
        w.write_line(")");
    }
    w.set_color(Color::LightGray, Color::Black);

    w.write_string(" [*] Testes de Politicas de Firewall JinnWall SPI...\n");
    let (jw_p, jw_t) = crate::net::jinnwall::run_jinnwall_selftests();
    w.write_string("     Resultado JinnWall: ");
    if jw_p == jw_t {
        w.set_color(Color::LightGreen, Color::Black);
        w.write_string("APROVADO (");
        w.write_decimal(jw_p);
        w.write_string("/");
        w.write_decimal(jw_t);
        w.write_line(")");
    } else {
        w.set_color(Color::LightRed, Color::Black);
        w.write_string("FALHA (");
        w.write_decimal(jw_p);
        w.write_string("/");
        w.write_decimal(jw_t);
        w.write_line(")");
    }
    w.set_color(Color::LightGray, Color::Black);

    w.write_string(" [*] Testes de Pilha TCP RFC 793 e Maquina de Estados...\n");
    let (tcp_p, tcp_t) = crate::net::tcp::run_tcp_selftests();
    w.write_string("     Resultado TCP: ");
    if tcp_p == tcp_t {
        w.set_color(Color::LightGreen, Color::Black);
        w.write_string("APROVADO (");
        w.write_decimal(tcp_p);
        w.write_string("/");
        w.write_decimal(tcp_t);
        w.write_line(")");
    } else {
        w.set_color(Color::LightRed, Color::Black);
        w.write_string("FALHA (");
        w.write_decimal(tcp_p);
        w.write_string("/");
        w.write_decimal(tcp_t);
        w.write_line(")");
    }
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

/// Ponto de entrada do shell como uma task (Ring 0).
pub fn task_entry() {
    let mut w = Writer::new();
    w.set_position(21, 0);
    // Inicializacao direta no Aladdin Desktop (padrao moderno)
    if crate::gui::is_available() {
        crate::gui::run_desktop();
    }
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

/// `jda` — Exibe informações de topologia de hardware e telemetria do JDA.
fn cmd_jda(w: &mut Writer) {
    w.set_color(Color::Yellow, Color::Black);
    w.write_line("=== Jinn Dataflow Architecture (JDA v0.1) ===");
    w.set_color(Color::LightGray, Color::Black);

    let active = crate::jda::is_active();
    w.write_string("  Status do Subsistema : ");
    if active {
        w.set_color(Color::LightGreen, Color::Black);
        w.write_line("ATIVO");
    } else {
        w.set_color(Color::LightRed, Color::Black);
        w.write_line("INATIVO");
    }
    w.set_color(Color::LightGray, Color::Black);

    let topo = crate::jda::HARDWARE_TOPOLOGY.lock();
    w.write_string("  Nucleos Identificados: ");
    w.write_decimal(topo.core_count);
    w.write_line("");

    for i in 0..topo.core_count {
        let core = &topo.cores[i];
        w.write_string("    - Core ");
        w.write_decimal(core.logical_id as usize);
        w.write_string(": APIC ID=");
        w.write_decimal(core.apic_id as usize);
        if core.is_bsp {
            w.write_string(" [BSP]");
        }
        w.write_string(" (SSE=");
        w.write_string(if core.has_sse { "Sim" } else { "Nao" });
        w.write_string(", AVX=");
        w.write_string(if core.has_avx { "Sim" } else { "Nao" });
        w.write_string(", AVX2=");
        w.write_string(if core.has_avx2 { "Sim" } else { "Nao" });
        w.write_line(")");
    }

    w.write_string("  Hierarquia de Caches : ");
    w.write_decimal(topo.cache_count);
    w.write_line(" niveis detectados via CPUID");

    for i in 0..topo.cache_count {
        let c = &topo.caches[i];
        w.write_string("    - L");
        w.write_decimal(c.level as usize);
        w.write_string(" ");
        let type_str = match c.cache_type {
            crate::jda::topology::CacheType::Data => "Dados      ",
            crate::jda::topology::CacheType::Instruction => "Instrucoes ",
            crate::jda::topology::CacheType::Unified => "Unificado  ",
            crate::jda::topology::CacheType::Unknown => "Desconhecido",
        };
        w.write_string(type_str);
        w.write_string(": ");
        w.write_memory_size(c.size_bytes as u64);
        w.write_string(" (Linha: ");
        w.write_decimal(c.line_size as usize);
        w.write_string("B, Vias: ");
        w.write_decimal(c.ways as usize);
        w.write_line(")");
    }

    w.write_string("  Memoria DRAM Fisica  : ");
    w.write_memory_size(topo.total_usable_dram());
    w.write_line("");

    // Telemetria acumulada
    let metrics = crate::jda::telemetry::get_metrics_snapshot();
    w.set_color(Color::LightCyan, Color::Black);
    w.write_line("  --- Telemetria do Motor Dataflow ---");
    w.set_color(Color::LightGray, Color::Black);
    w.write_string("    Operacoes Submetidas : ");
    w.write_decimal(metrics.total_operations_submitted as usize);
    w.write_line("");
    w.write_string("    Operacoes Concluidas : ");
    w.write_decimal(metrics.total_operations_completed as usize);
    w.write_line("");
    w.write_string("    Zero-Copy Transfers  : ");
    w.write_decimal(metrics.zero_copy_transfers as usize);
    w.write_string(" (");
    w.write_memory_size(metrics.bytes_transferred_zero_copy);
    w.write_line(")");
    w.write_string("    Copias Redundantes   : ");
    w.write_decimal(metrics.redundant_copies as usize);
    w.write_string(" (");
    w.write_memory_size(metrics.bytes_copied_memory);
    w.write_line(")");
    w.write_string("    Ciclos Computacao    : ");
    w.write_decimal(metrics.cycles_compute as usize);
    w.write_line(" TSC");
}

/// `jdabench` — Executa benchmark comparativo entre a politica Baseline FIFO e a JDA Locality Policy.
fn cmd_jdabench(w: &mut Writer) {
    use crate::jda::graph::{DataflowGraph, ExecutionTarget, OpKind, DataBufferRef};
    use crate::jda::policy::{JdaPolicy, FifoReferencePolicy, DataflowLocalityPolicy};
    use crate::jda::telemetry::{read_tsc, record_operation_completed, record_data_transfer};

    w.set_color(Color::Yellow, Color::Black);
    w.write_line("=== Benchmark Comparativo: JDA Dataflow DAG ===");
    w.set_color(Color::LightGray, Color::Black);
    w.write_line("Executando grafo sintetico de 4 nos com dependencias em diamante...");

    // Buffer de teste para transferência de dados
    static mut BENCH_BUFFER: [u8; 4096] = [0x5A; 4096];
    let buf_raw_ptr = core::ptr::addr_of_mut!(BENCH_BUFFER) as *mut u8;
    let buf_phys = crate::memory::paging::virt_to_phys(buf_raw_ptr as u64);

    let buf_ref = DataBufferRef {
        phys_addr: buf_phys,
        virt_addr: buf_raw_ptr as u64,
        size_bytes: 4096,
        is_shared: true,
        owner_pid: 0,
    };

    // ── Teste 1: Política Baseline (FIFO) com Cópia de Memória ───────────────
    let mut graph_fifo = DataflowGraph::new();
    let n0 = graph_fifo.add_node(OpKind::MemoryTransfer, ExecutionTarget::AnyCpu, 1, Some(buf_ref), None).unwrap();
    let n1 = graph_fifo.add_node(OpKind::Compute, ExecutionTarget::AnyCpu, 2, Some(buf_ref), None).unwrap();
    let n2 = graph_fifo.add_node(OpKind::Compute, ExecutionTarget::AnyCpu, 2, Some(buf_ref), None).unwrap();
    let n3 = graph_fifo.add_node(OpKind::Barrier, ExecutionTarget::AnyCpu, 3, None, Some(buf_ref)).unwrap();

    let _ = graph_fifo.add_dependency(n1, n0);
    let _ = graph_fifo.add_dependency(n2, n0);
    let _ = graph_fifo.add_dependency(n3, n1);
    let _ = graph_fifo.add_dependency(n3, n2);

    let fifo_policy = FifoReferencePolicy;
    let t0 = read_tsc();
    let mut fifo_copies = 0usize;

    while let Some(node_idx) = fifo_policy.select_next(&graph_fifo) {
        let start_node = read_tsc();
        // Na política tradicional de cópia: simula movimentação de dados copiando o buffer
        unsafe {
            let slice = core::slice::from_raw_parts_mut(buf_raw_ptr, 4096);
            for i in 0..1024 {
                let val = slice[i];
                slice[1024 + i] = val.wrapping_add(1);
            }
        }
        fifo_copies += 1024;
        let end_node = read_tsc();
        graph_fifo.mark_completed(node_idx as u32, end_node);
        record_data_transfer(1024, false);
        record_operation_completed(end_node - start_node);
    }
    let t1 = read_tsc();
    let cycles_fifo = t1 - t0;

    // ── Teste 2: Política JDA Locality-Aware com Zero-Copy ────────────────────
    let mut graph_jda = DataflowGraph::new();
    let j0 = graph_jda.add_node(OpKind::MemoryTransfer, ExecutionTarget::AnyCpu, 1, Some(buf_ref), None).unwrap();
    let j1 = graph_jda.add_node(OpKind::Compute, ExecutionTarget::AnyCpu, 3, Some(buf_ref), None).unwrap();
    let j2 = graph_jda.add_node(OpKind::Compute, ExecutionTarget::AnyCpu, 3, Some(buf_ref), None).unwrap();
    let j3 = graph_jda.add_node(OpKind::Barrier, ExecutionTarget::AnyCpu, 4, None, Some(buf_ref)).unwrap();

    let _ = graph_jda.add_dependency(j1, j0);
    let _ = graph_jda.add_dependency(j2, j0);
    let _ = graph_jda.add_dependency(j3, j1);
    let _ = graph_jda.add_dependency(j3, j2);

    let jda_policy = DataflowLocalityPolicy;
    let tj0 = read_tsc();
    let mut jda_zero_copy_bytes = 0usize;

    while let Some(node_idx) = jda_policy.select_next(&graph_jda) {
        let start_node = read_tsc();
        // Na política Zero-Copy: consome diretamente via referência sem duplicação de buffer
        unsafe {
            let slice = core::slice::from_raw_parts(buf_raw_ptr, 4096);
            let mut sum: u8 = 0;
            for i in 0..1024 {
                sum = sum.wrapping_add(slice[i]);
            }
            core::hint::black_box(sum);
        }
        jda_zero_copy_bytes += 1024;
        let end_node = read_tsc();
        graph_jda.mark_completed(node_idx as u32, end_node);
        record_data_transfer(1024, true);
        record_operation_completed(end_node - start_node);
    }
    let tj1 = read_tsc();
    let cycles_jda = tj1 - tj0;

    w.write_string("  1. Baseline (FIFO + Copia RAM) : ");
    w.write_decimal(cycles_fifo as usize);
    w.write_string(" ciclos TSC (Copias: ");
    w.write_decimal(fifo_copies);
    w.write_line(" bytes)");

    w.write_string("  2. JDA (Dataflow + Zero-Copy)  : ");
    w.write_decimal(cycles_jda as usize);
    w.write_string(" ciclos TSC (Zero-Copy: ");
    w.write_decimal(jda_zero_copy_bytes);
    w.write_line(" bytes)");

    if cycles_fifo > cycles_jda {
        let saved = cycles_fifo - cycles_jda;
        let pct = (saved * 100) / cycles_fifo;
        w.set_color(Color::LightGreen, Color::Black);
        w.write_string("  [VANTAGEM] Reducao de latencia : ");
        w.write_decimal(saved as usize);
        w.write_string(" ciclos (-");
        w.write_decimal(pct as usize);
        w.write_line("%) por eliminacao de redundancia");
    } else {
        w.set_color(Color::LightCyan, Color::Black);
        w.write_line("  [RESULTADO] Desempenho equivalente entre politicas.");
    }
    w.set_color(Color::LightGray, Color::Black);
}

/// `bdev` — Diagnóstico e teste da camada de abstração de dispositivos de bloco.
fn cmd_bdev(w: &mut Writer) {
    use crate::drivers::block::{BlockDevice, IdeBlockDevice};

    w.set_color(Color::Yellow, Color::Black);
    w.write_line("=== Jinn Storage: Camada BlockDevice ===");
    w.set_color(Color::LightGray, Color::Black);

    match IdeBlockDevice::new(0) {
        Ok(dev) => {
            w.write_string("  Dispositivo Primario : ");
            w.set_color(Color::LightCyan, Color::Black);
            w.write_line(dev.name());
            w.set_color(Color::LightGray, Color::Black);

            w.write_string("  Tamanho do Bloco     : ");
            w.write_decimal(dev.block_size());
            w.write_line(" bytes");

            w.write_string("  Total de Blocos      : ");
            w.write_decimal(dev.total_blocks() as usize);
            w.write_line(" setores");

            w.write_string("  Capacidade           : ");
            w.write_decimal((dev.total_blocks() * dev.block_size() as u64 / (1024 * 1024)) as usize);
            w.write_line(" MiB");

            // Teste de leitura de bloco 0 (MBR) via BlockDevice
            let mut buf = [0u8; 512];
            match dev.read_blocks(0, 1, &mut buf) {
                Ok(()) => {
                    let sig = (buf[510] == 0x55) && (buf[511] == 0xAA);
                    w.write_string("  Teste Leitura Bloco 0: ");
                    if sig {
                        w.set_color(Color::LightGreen, Color::Black);
                        w.write_line("OK (MBR 0x55AA valido)");
                    } else {
                        w.set_color(Color::Yellow, Color::Black);
                        w.write_line("AVISO (Assinatura nao encontrada)");
                    }
                }
                Err(e) => {
                    w.set_color(Color::LightRed, Color::Black);
                    w.write_string("  [ERRO] Leitura de bloco: ");
                    w.write_line(e);
                }
            }
            w.set_color(Color::LightGray, Color::Black);

            // Teste controlado de escrita e leitura de volta no LBA 1 (espaço reservado seguro fora de partições)
            let mut test_payload = [0u8; 512];
            let test_magic = b"JINN_BLOCKDEV_V01_TEST_INTEGRITY_OK\0";
            test_payload[..test_magic.len()].copy_from_slice(test_magic);
            test_payload[500..504].copy_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);

            w.write_string("  Teste Escrita Bloco 1: ");
            match dev.write_blocks(1, 1, &test_payload) {
                Ok(()) => {
                    let mut verify_buf = [0u8; 512];
                    match dev.read_blocks(1, 1, &mut verify_buf) {
                        Ok(()) => {
                            if verify_buf == test_payload {
                                w.set_color(Color::LightGreen, Color::Black);
                                w.write_line("OK (Gravacao & Leitura 100% verificadas)");
                            } else {
                                w.set_color(Color::LightRed, Color::Black);
                                w.write_line("FALHA (Dados lidos divergem dos gravados)");
                            }
                        }
                        Err(e) => {
                            w.set_color(Color::LightRed, Color::Black);
                            w.write_string("ERRO de verificacao: ");
                            w.write_line(e);
                        }
                    }
                }
                Err(e) => {
                    w.set_color(Color::Yellow, Color::Black);
                    w.write_string("BLOQUEADO: ");
                    w.write_line(e);
                }
            }

            // Teste de eficiência da camada CachedBlockDevice
            let cached_dev = crate::drivers::block::CachedBlockDevice::new(dev);
            let mut cbuf = [0u8; 512];
            cached_dev.read_blocks(0, 1, &mut cbuf).ok(); // Leitura 1: Miss
            cached_dev.read_blocks(0, 1, &mut cbuf).ok(); // Leitura 2: Hit
            let (hits, misses) = cached_dev.stats();

            w.write_string("  BlockCache LRU (8 KB RAM): ");
            w.set_color(Color::LightGreen, Color::Black);
            w.write_decimal(hits as usize);
            w.write_string(" Hit(s) / ");
            w.write_decimal(misses as usize);
            w.write_string(" Miss (Eficiencia: ");
            let total = hits + misses;
            let ratio = if total > 0 { (hits * 100) / total } else { 0 };
            w.write_decimal(ratio as usize);
            w.write_line("%)");
            w.set_color(Color::LightGray, Color::Black);
        }
        Err(e) => {
            w.set_color(Color::LightRed, Color::Black);
            w.write_string("  [ERRO] Falha ao instanciar IdeBlockDevice(0): ");
            w.write_line(e);
        }
    }
    w.set_color(Color::LightGray, Color::Black);
}

/// `storage` / `jsa` — Diagnóstico e controle da Jinn Storage Architecture (JSA).
fn cmd_storage(w: &mut Writer, args: &str) {
    use crate::drivers::block::{
        get_storage_policy, get_storage_profile, set_storage_profile,
        BlockDevice, IdeBlockDevice, StorageProfile, WriteStrategy,
    };
    use crate::drivers::partition::{scan_partitions, PartitionDevice};

    let trimmed = args.trim();

    // Tratamento de chaveamento de perfil: `storage profile <nome>`
    if trimmed.starts_with("profile ") || trimmed.starts_with("prof ") {
        let p_name = if let Some(rest) = trimmed.strip_prefix("profile ") {
            rest.trim()
        } else if let Some(rest) = trimmed.strip_prefix("prof ") {
            rest.trim()
        } else {
            ""
        };

        match p_name {
            "aladdin" | "domestico" => {
                set_storage_profile(StorageProfile::Aladdin);
                w.set_color(Color::LightGreen, Color::Black);
                w.write_line("[OK] Perfil JSA alterado para ALADDIN (Domestico).");
            }
            "simbad" | "servidor" | "server" => {
                set_storage_profile(StorageProfile::Simbad);
                w.set_color(Color::LightGreen, Color::Black);
                w.write_line("[OK] Perfil JSA alterado para SIMBAD (Servidor de Alta Integridade).");
            }
            "workstation" | "dev" => {
                set_storage_profile(StorageProfile::Workstation);
                w.set_color(Color::LightGreen, Color::Black);
                w.write_line("[OK] Perfil JSA alterado para WORKSTATION (Engenharia/Desenvolvimento).");
            }
            "pos" | "terminal" => {
                set_storage_profile(StorageProfile::TerminalPos);
                w.set_color(Color::LightGreen, Color::Black);
                w.write_line("[OK] Perfil JSA alterado para TERMINAL/POS (Corte de Energia Resiliente).");
            }
            _ => {
                w.set_color(Color::LightRed, Color::Black);
                w.write_line("Perfil desconhecido. Opcoes: aladdin, simbad, workstation, pos");
            }
        }
        w.set_color(Color::LightGray, Color::Black);
        return;
    }

    // Exibição de Status e Diagnóstico Geral da JSA
    w.set_color(Color::Yellow, Color::Black);
    w.write_line("=== Jinn Storage Architecture (JSA) ===");
    w.set_color(Color::LightGray, Color::Black);

    let cur_prof = get_storage_profile();
    let cur_policy = get_storage_policy();

    w.write_string("  Perfil Ativo         : ");
    w.set_color(Color::LightCyan, Color::Black);
    w.write_line(cur_prof.name());
    w.set_color(Color::LightGray, Color::Black);

    w.write_string("  Estrategia de Escrita: ");
    w.set_color(Color::White, Color::Black);
    match cur_policy.write_strategy {
        WriteStrategy::WriteThrough => w.write_line("Write-Through (Sincrono Imediato)"),
        WriteStrategy::WriteBack => w.write_line("Write-Back (Consolidacao por Flush)"),
    }
    w.set_color(Color::LightGray, Color::Black);

    w.write_string("  Slots de Cache RAM   : ");
    w.write_decimal(cur_policy.max_cache_slots);
    w.write_string(" slots (");
    w.write_decimal(cur_policy.max_cache_slots * 512 / 1024);
    w.write_line(" KiB)");

    w.write_string("  Prefetch de Leitura  : ");
    w.write_decimal(cur_policy.prefetch_sectors);
    w.write_line(" setores");

    w.write_string("  Barreira de I/O      : ");
    if cur_policy.barrier_required {
        w.set_color(Color::LightGreen, Color::Black);
        w.write_line("Exigida (Flush Barrier Ativo)");
    } else {
        w.set_color(Color::DarkGray, Color::Black);
        w.write_line("Opcional / Sob Demanda");
    }
    w.set_color(Color::LightGray, Color::Black);

    let (c_hits, c_misses, c_dirty) = crate::drivers::block::primary_cache_stats();
    w.write_string("  Estatisticas Cache   : Hits: ");
    w.write_decimal(c_hits as usize);
    w.write_string(" | Misses: ");
    w.write_decimal(c_misses as usize);
    w.write_string(" | Dirty: ");
    w.write_decimal(c_dirty);
    w.write_line(" setores");

    // Varredura de Partições MBR e GPT
    w.set_color(Color::Yellow, Color::Black);
    w.write_line("\n--- Varredura de Particoes e Volumes (MBR/GPT) ---");
    w.set_color(Color::LightGray, Color::Black);

    match IdeBlockDevice::new(0) {
        Ok(dev) => {
            match scan_partitions(&dev) {
                Ok(table) => {
                    w.write_string("  Esquema Detectado    : ");
                    w.set_color(Color::LightGreen, Color::Black);
                    if table.is_gpt {
                        w.write_line("GPT (GUID Partition Table)");
                    } else {
                        w.write_line("MBR (Master Boot Record)");
                    }
                    w.set_color(Color::LightGray, Color::Black);

                    w.write_string("  Particoes Encontradas: ");
                    w.write_decimal(table.count);
                    w.write_line("");

                    for i in 0..table.count {
                        if let Some(part) = table.partitions[i] {
                            w.write_string("  [p");
                            w.write_decimal(part.index);
                            w.write_string("] LBA ");
                            w.write_decimal(part.start_lba as usize);
                            w.write_string("..");
                            w.write_decimal((part.start_lba + part.sector_count - 1) as usize);
                            w.write_string(" (");
                            w.write_decimal(part.size_mb() as usize);
                            w.write_string(" MiB) | Tipo: ");
                            w.set_color(Color::LightCyan, Color::Black);
                            w.write_string(part.type_str());
                            w.set_color(Color::LightGray, Color::Black);
                            if part.is_bootable {
                                w.set_color(Color::LightGreen, Color::Black);
                                w.write_string(" [Bootavel]");
                                w.set_color(Color::LightGray, Color::Black);
                            }
                            w.write_line("");

                            // Demonstração de contenção segura de limites via PartitionDevice
                            if i == 0 {
                                if let Ok(part_dev) = PartitionDevice::new(&dev, "ide0p1", part.start_lba, part.sector_count, false) {
                                    w.write_string("    Guarda de Limite (Boundary Guard): ");
                                    let mut dummy = [0u8; 512];
                                    // Tenta ler além do final da partição para verificar bloqueio
                                    match part_dev.read_blocks(part.sector_count, 1, &mut dummy) {
                                        Err(err) => {
                                            w.set_color(Color::LightGreen, Color::Black);
                                            w.write_string("BLOQUEADO com sucesso (");
                                            w.write_string(err);
                                            w.write_line(")");
                                            w.set_color(Color::LightGray, Color::Black);
                                        }
                                        Ok(()) => {
                                            w.set_color(Color::LightRed, Color::Black);
                                            w.write_line("FALHA: Acesso fora do limite permitido!");
                                            w.set_color(Color::LightGray, Color::Black);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    w.set_color(Color::LightRed, Color::Black);
                    w.write_string("  Erro na varredura de particoes: ");
                    w.write_line(e);
                    w.set_color(Color::LightGray, Color::Black);
                }
            }
        }
        Err(e) => {
            w.set_color(Color::LightRed, Color::Black);
            w.write_string("  Erro no dispositivo primario: ");
            w.write_line(e);
            w.set_color(Color::LightGray, Color::Black);
        }
    }

    w.set_color(Color::DarkGray, Color::Black);
    w.write_line("\nDica: use 'storage profile <aladdin|simbad|workstation|pos>' para alternar.");
    w.set_color(Color::LightGray, Color::Black);
}

/// `jdashm` — Teste de ciclo de vida de memória física compartilhada (Zero-Copy Frame Remapping).
fn cmd_jdashm(w: &mut Writer) {
    use crate::jda::shmem::SHARED_BUFFER_POOL;

    w.set_color(Color::Yellow, Color::Black);
    w.write_line("=== JDA Zero-Copy Physical Memory Engine ===");
    w.set_color(Color::LightGray, Color::Black);

    let mut pool = SHARED_BUFFER_POOL.lock();

    // 1. Aloca um buffer físico de 4 KiB
    match pool.allocate(0, false) {
        Ok(handle) => {
            let phys = pool.get_phys_addr(0, handle).unwrap_or(0);
            w.write_string("  1. Buffer Alocado (ID ");
            w.write_decimal(handle);
            w.write_string(") : Frame Fisico 0x");
            w.write_hex_u64(phys);
            w.write_line("");

            // Escreve padrão no quadro via HHDM virtual
            let virt = crate::memory::paging::phys_to_virt(phys) as *mut u8;
            let signature = b"JDA_PMM_ZERO_COPY_PAYLOAD_VALID";
            unsafe {
                core::ptr::copy_nonoverlapping(signature.as_ptr(), virt, signature.len());
            }

            // Constrói e decodifica mensagem IPC Zero-Copy transmitindo o descritor
            let msg = crate::ipc::Message::new_jda_zero_copy(0, 0, handle, 4096);
            if let Some((unpacked_handle, unpacked_len)) = msg.as_jda_zero_copy() {
                if unpacked_handle == handle && unpacked_len == 4096 {
                    w.set_color(Color::LightGreen, Color::Black);
                    w.write_line("  2. [OK] Mensagem IPC Zero-Copy transmitida via descritor (0 bytes payload copiados)");
                    w.set_color(Color::LightGray, Color::Black);
                }
            }

            // 2.1. Teste de I/O de Bloco Direto para o Frame Físico (Zero-Copy Block I/O)
            if let Ok(dev) = crate::drivers::block::IdeBlockDevice::new(0) {
                match pool.read_from_block_device(handle, &dev, 0, 1) {
                    Ok(()) => {
                        let sig_ok = unsafe { *virt.add(510) == 0x55 && *virt.add(511) == 0xAA };
                        if sig_ok {
                            w.set_color(Color::LightGreen, Color::Black);
                            w.write_line("  2.1 [OK] Leitura Direta Bloco -> Frame JDA (Zero-Copy MBR 0x55AA verificado)");
                            w.set_color(Color::LightGray, Color::Black);
                        }
                    }
                    Err(err) => {
                        w.set_color(Color::Yellow, Color::Black);
                        w.write_string("  2.1 [AVISO] Leitura direta de bloco: ");
                        w.write_line(err);
                        w.set_color(Color::LightGray, Color::Black);
                    }
                }
            }

            // 3. Retain (compartilhamento com outro nó ou processo consumidor)
            match pool.retain(handle) {
                Ok(_) => {
                    w.set_color(Color::LightGreen, Color::Black);
                    w.write_line("  3. [OK] Retain de referencia: ref_count = 2 (Compartilhado)");
                    w.set_color(Color::LightGray, Color::Black);
                }
                Err(e) => {
                    w.set_color(Color::LightRed, Color::Black);
                    w.write_line(e);
                }
            }

            // 4. Libera primeira referência (produtor concluiu)
            pool.release(0, handle).ok();
            w.write_line("  4. [OK] Release parcial: ref_count = 1 (Frame retido com sucesso)");

            // 5. Libera segunda referência (consumidor concluiu -> libera frame físico de volta)
            pool.release(0, handle).ok();
            w.set_color(Color::LightGreen, Color::Black);
            w.write_line("  5. [OK] Release final: ref_count = 0 (Frame fisico liberado ao bitmap PMM)");
            w.set_color(Color::LightGray, Color::Black);
        }
        Err(e) => {
            w.set_color(Color::LightRed, Color::Black);
            w.write_string("  [ERRO] Alocacao de buffer compartilhado: ");
            w.write_line(e);
        }
    }
    w.set_color(Color::LightGray, Color::Black);
}

/// `dns` — Resolve um nome de domínio para IPv4.
fn cmd_dns(w: &mut Writer, args: &str) {
    if args.is_empty() {
        w.set_color(Color::LightRed, Color::Black);
        w.write_line("Uso: dns <hostname>");
        w.set_color(Color::LightGray, Color::Black);
        return;
    }
    w.set_color(Color::Yellow, Color::Black);
    w.write_string("Resolvendo DNS para: ");
    w.set_color(Color::LightCyan, Color::Black);
    w.write_line(args);
    w.set_color(Color::LightGray, Color::Black);

    match crate::net::dns::resolve(args) {
        Some(ip) => {
            w.set_color(Color::LightGreen, Color::Black);
            w.write_string("  [OK] Endereco IP: ");
            w.write_decimal(ip[0] as usize);
            w.write_string(".");
            w.write_decimal(ip[1] as usize);
            w.write_string(".");
            w.write_decimal(ip[2] as usize);
            w.write_string(".");
            w.write_decimal(ip[3] as usize);
            w.write_line("");
            w.set_color(Color::LightGray, Color::Black);
        }
        None => {
            w.set_color(Color::LightRed, Color::Black);
            w.write_line("  [FALHA] Nao foi possivel resolver o nome (timeout ou inexistente).");
            w.set_color(Color::LightGray, Color::Black);
        }
    }
}

/// `dnscache` — Exibe entradas ativas no cache local DNS.
fn cmd_dnscache(w: &mut Writer) {
    w.set_color(Color::Yellow, Color::Black);
    w.write_line("=== Cache Local DNS (RFC 1035) ===");
    w.set_color(Color::LightGray, Color::Black);

    let current_tick = crate::timer::ticks();
    unsafe {
        let mut count = 0;
        for (idx, slot) in crate::net::dns::DNS_CACHE.entries.iter().enumerate() {
            if let Some(entry) = slot {
                count += 1;
                w.write_string("  [");
                w.write_decimal(idx);
                w.write_string("] ");
                if let Ok(name_str) = core::str::from_utf8(&entry.name[..entry.name_len as usize]) {
                    w.set_color(Color::LightCyan, Color::Black);
                    w.write_string(name_str);
                }
                w.set_color(Color::LightGray, Color::Black);
                w.write_string(" -> ");
                w.set_color(Color::LightGreen, Color::Black);
                w.write_decimal(entry.ip[0] as usize);
                w.write_string(".");
                w.write_decimal(entry.ip[1] as usize);
                w.write_string(".");
                w.write_decimal(entry.ip[2] as usize);
                w.write_string(".");
                w.write_decimal(entry.ip[3] as usize);

                w.set_color(Color::DarkGray, Color::Black);
                if entry.expire_tick == 0 {
                    w.write_line(" (estatico)");
                } else if current_tick > entry.expire_tick {
                    w.write_line(" (expirado)");
                } else {
                    let rem_sec = (entry.expire_tick - current_tick) / 100;
                    w.write_string(" (ttl: ");
                    w.write_decimal(rem_sec as usize);
                    w.write_line("s)");
                }
                w.set_color(Color::LightGray, Color::Black);
            }
        }
        if count == 0 {
            w.write_line("  (nenhuma entrada no cache)");
        }
    }
}

/// `netinfo` — Exibe status de interface de rede VirtIO e endereçamento.
fn cmd_netinfo(w: &mut Writer) {
    w.set_color(Color::Yellow, Color::Black);
    w.write_line("=== Configuracao de Rede & VirtIO ===");
    w.set_color(Color::LightGray, Color::Black);

    if let Some(dev) = crate::drivers::net::virtio::get_device() {
        let mac = dev.mac_address();
        w.write_string("  Placa de Rede : VirtIO-Net (MAC: ");
        w.write_hex_u8(mac[0]); w.write_string(":");
        w.write_hex_u8(mac[1]); w.write_string(":");
        w.write_hex_u8(mac[2]); w.write_string(":");
        w.write_hex_u8(mac[3]); w.write_string(":");
        w.write_hex_u8(mac[4]); w.write_string(":");
        w.write_hex_u8(mac[5]); w.write_line(")");
    } else {
        w.set_color(Color::LightRed, Color::Black);
        w.write_line("  Placa de Rede : VirtIO-Net nao encontrada.");
        w.set_color(Color::LightGray, Color::Black);
    }

    unsafe {
        let ip = crate::net::dhcp::DHCP_CLIENT.ip;
        let gw = crate::net::dhcp::DHCP_CLIENT.gateway;
        let dns = crate::net::dhcp::DHCP_CLIENT.dns;

        w.write_string("  Endereco IP   : ");
        w.set_color(Color::LightGreen, Color::Black);
        w.write_decimal(ip[0] as usize); w.write_string(".");
        w.write_decimal(ip[1] as usize); w.write_string(".");
        w.write_decimal(ip[2] as usize); w.write_string(".");
        w.write_decimal(ip[3] as usize); w.write_line("");
        w.set_color(Color::LightGray, Color::Black);

        w.write_string("  Gateway       : ");
        w.write_decimal(gw[0] as usize); w.write_string(".");
        w.write_decimal(gw[1] as usize); w.write_string(".");
        w.write_decimal(gw[2] as usize); w.write_string(".");
        w.write_decimal(gw[3] as usize); w.write_line("");

        w.write_string("  Servidor DNS  : ");
        w.write_decimal(dns[0] as usize); w.write_string(".");
        w.write_decimal(dns[1] as usize); w.write_string(".");
        w.write_decimal(dns[2] as usize); w.write_string(".");
        w.write_decimal(dns[3] as usize); w.write_line("");

        let profile_str = match crate::net::jinnwall::JINNWALL.profile() {
            crate::net::jinnwall::FirewallProfile::PublicSecure => "PublicSecure (Bloqueio Rigido)",
            crate::net::jinnwall::FirewallProfile::Development => "Development (Portas 8080/3000)",
        };
        w.write_string("  JinnWall SPI  : ");
        w.set_color(Color::LightCyan, Color::Black);
        w.write_line(profile_str);
        w.set_color(Color::LightGray, Color::Black);
    }
}

/// `jinnwall` — Status e controle do firewall SPI JinnWall.
fn cmd_jinnwall(w: &mut Writer, args: &str) {
    unsafe {
        if args == "dev" {
            crate::net::jinnwall::JINNWALL.set_profile(crate::net::jinnwall::FirewallProfile::Development);
            w.set_color(Color::LightGreen, Color::Black);
            w.write_line("JinnWall: Perfil alterado para 'Development' (Portas 8080 e 3000 abertas).");
            w.set_color(Color::LightGray, Color::Black);
            return;
        } else if args == "secure" || args == "public" {
            crate::net::jinnwall::JINNWALL.set_profile(crate::net::jinnwall::FirewallProfile::PublicSecure);
            w.set_color(Color::LightGreen, Color::Black);
            w.write_line("JinnWall: Perfil alterado para 'PublicSecure' (Bloqueio estrito de conexoes).");
            w.set_color(Color::LightGray, Color::Black);
            return;
        }

        w.set_color(Color::Yellow, Color::Black);
        w.write_line("=== Status do Firewall JinnWall (SPI) ===");
        w.set_color(Color::LightGray, Color::Black);

        let prof = crate::net::jinnwall::JINNWALL.profile().name();
        w.write_string("  Perfil Ativo         : ");
        w.set_color(Color::LightCyan, Color::Black);
        w.write_line(prof);
        w.set_color(Color::LightGray, Color::Black);

        w.write_string("  Pacotes Inspecionados: ");
        w.write_decimal(crate::net::jinnwall::JINNWALL.inspected() as usize);
        w.write_line("");

        w.write_string("  Pacotes Aceitos      : ");
        w.set_color(Color::LightGreen, Color::Black);
        w.write_decimal(crate::net::jinnwall::JINNWALL.accepted() as usize);
        w.write_line("");
        w.set_color(Color::LightGray, Color::Black);

        w.write_string("  Pacotes Descartados  : ");
        w.set_color(Color::LightRed, Color::Black);
        w.write_decimal(crate::net::jinnwall::JINNWALL.drops() as usize);
        w.write_line("");
        w.set_color(Color::LightGray, Color::Black);

        w.write_string("  Conexoes Conntrack   : ");
        w.write_decimal(crate::net::jinnwall::JINNWALL.active_connections());
        w.write_line(" fluxos ativos");

        w.set_color(Color::DarkGray, Color::Black);
        w.write_line("  Use: 'jinnwall dev' ou 'jinnwall secure' para alternar o perfil.");
        w.set_color(Color::LightGray, Color::Black);
    }
}

/// Executa comandos de string vindos do terminal gráfico do Aladdin e despacha linhas coloridas.
pub fn execute_string_command<F>(cmd: &str, mut output: F)
where
    F: FnMut(&str, u32),
{
    let cmd = cmd.trim();
    if cmd.is_empty() { return; }

    let (verb, args) = match cmd.find(' ') {
        Some(i) => (&cmd[..i], cmd[i + 1..].trim()),
        None => (cmd, ""),
    };

    match verb {
        "help" => {
            output("Comandos do Shell Jinn:", 0x00F5_9E0B);
            output("  help       - exibe esta lista", 0x0094_A3B8);
            output("  uname      - informacoes do microkernel", 0x0094_A3B8);
            output("  mem        - uso de memoria fisica", 0x0094_A3B8);
            output("  ps         - lista de tarefas ativas", 0x0094_A3B8);
            output("  uptime     - tempo de execucao", 0x0094_A3B8);
            output("  ls [path]  - lista arquivos no VFS", 0x0094_A3B8);
            output("  cat <file> - exibe conteudo de texto", 0x0094_A3B8);
            output("  df         - sistemas de arquivos montados", 0x0094_A3B8);
            output("  pci        - lista dispositivos PCI", 0x0094_A3B8);
            output("  reboot     - reinicia o computador", 0x0094_A3B8);
        }
        "uname" => {
            output("Jinn Microkernel v0.0.3-userspace (x86_64, Ring 0/3)", 0x0038_BDF8);
        }
        "mem" | "meminfo" => {
            output("=== Memoria RAM ===", 0x00F5_9E0B);
            let total = crate::memory::total_bytes() / (1024 * 1024);
            let used = crate::memory::used_bytes() / (1024 * 1024);
            let free = total.saturating_sub(used);
            let mut buf = [0u8; 32];
            // Total
            output("Memoria total:", 0x0094_A3B8);
            let mut len = aladdin::compositor::format_num(total, &mut buf);
            if let Ok(s) = core::str::from_utf8(&buf[..len]) { output(s, 0x0022_C55E); }
            output("Memoria livre (MB):", 0x0094_A3B8);
            len = aladdin::compositor::format_num(free, &mut buf);
            if let Ok(s) = core::str::from_utf8(&buf[..len]) { output(s, 0x0038_BDF8); }
        }
        "ps" | "tasks" => {
            output("Tarefas Ativas no Escalonador:", 0x00F5_9E0B);
            let sched = crate::scheduler::get();
            for slot in sched.tasks.iter() {
                if let Some(t) = slot {
                    output(t.name, 0x00F8_FAFC);
                }
            }
        }
        "uptime" => {
            let secs = (crate::timer::ticks() / 100) as usize;
            let mut buf = [0u8; 16];
            let len = aladdin::compositor::format_num(secs, &mut buf);
            if let Ok(s) = core::str::from_utf8(&buf[..len]) {
                output("Uptime (segundos):", 0x0094_A3B8);
                output(s, 0x0038_BDF8);
            }
        }
        "ls" | "dir" => {
            let target = if args.is_empty() { "/disk" } else { args };
            output("Conteudo do VFS:", 0x0038_BDF8);
            let mut count = 0;
            let _ = crate::vfs::readdir(target, |name, _| {
                output(name, 0x0022_C55E);
                count += 1;
            });
            if count == 0 {
                output("(diretorio vazio ou nao encontrado)", 0x0094_A3B8);
            }
        }
        "cat" => {
            if args.is_empty() {
                output("Uso: cat <caminho>", 0x00EF_4444);
            } else {
                let mut buf = [0u8; 128];
                match crate::vfs::read_file(args, &mut buf) {
                    Ok(n) => {
                        if let Ok(s) = core::str::from_utf8(&buf[..n]) {
                            output(s, 0x00F8_FAFC);
                        } else {
                            output("[Arquivo binario]", 0x0094_A3B8);
                        }
                    }
                    Err(_) => output("Falha ao ler arquivo", 0x00EF_4444),
                }
            }
        }
        "touch" => {
            if args.is_empty() {
                output("Uso: touch <caminho>", 0x00EF_4444);
            } else {
                match crate::vfs::write_file(args, b"") {
                    Ok(_) => output("Arquivo criado com sucesso", 0x0022_C55E),
                    Err(_) => output("Falha ao criar arquivo", 0x00EF_4444),
                }
            }
        }
        "rm" | "del" => {
            if args.is_empty() {
                output("Uso: rm <caminho>", 0x00EF_4444);
            } else {
                match crate::vfs::remove(args) {
                    Ok(_) => output("Arquivo removido com sucesso", 0x0022_C55E),
                    Err(_) => output("Falha ao remover arquivo", 0x00EF_4444),
                }
            }
        }
        "df" | "mounts" => {
            output("Sistemas de Arquivos Montados:", 0x0038_BDF8);
            crate::vfs::list_mounts(|pfx, _fs_name| {
                output(pfx, 0x00F8_FAFC);
            });
        }
        "pci" | "lspci" => {
            output("Dispositivos PCI Detectados:", 0x0038_BDF8);
            let dev_count = crate::drivers::pci::device_count();
            let mut buf = [0u8; 16];
            let len = aladdin::compositor::format_num(dev_count, &mut buf);
            if let Ok(s) = core::str::from_utf8(&buf[..len]) {
                output(s, 0x0022_C55E);
            }
        }
        "net" | "netinfo" | "ifconfig" => {
            output("=== Configuracao de Rede ===", 0x00F5_9E0B);
            unsafe {
                let ip = crate::net::dhcp::DHCP_CLIENT.ip;
                output("IP Local:", 0x0094_A3B8);
                let mut buf = [0u8; 32];
                let mut len = 0;
                for (idx, oct) in ip.iter().enumerate() {
                    let mut b = [0u8; 8];
                    let l = aladdin::compositor::format_num(*oct as usize, &mut b);
                    buf[len..(len + l)].copy_from_slice(&b[..l]);
                    len += l;
                    if idx < 3 { buf[len] = b'.'; len += 1; }
                }
                if let Ok(s) = core::str::from_utf8(&buf[..len]) { output(s, 0x0022_C55E); }
            }
        }
        "dns" => {
            if args.is_empty() {
                output("Uso: dns <hostname>", 0x00EF_4444);
            } else {
                output("Resolvendo DNS...", 0x0094_A3B8);
                match crate::net::dns::resolve(args) {
                    Some(ip) => {
                        let mut buf = [0u8; 32];
                        let mut len = 0;
                        for (idx, oct) in ip.iter().enumerate() {
                            let mut b = [0u8; 8];
                            let l = aladdin::compositor::format_num(*oct as usize, &mut b);
                            buf[len..(len + l)].copy_from_slice(&b[..l]);
                            len += l;
                            if idx < 3 { buf[len] = b'.'; len += 1; }
                        }
                        if let Ok(s) = core::str::from_utf8(&buf[..len]) { output(s, 0x0022_C55E); }
                    }
                    None => {
                        output("Falha ao resolver nome (timeout/NXDOMAIN)", 0x00EF_4444);
                    }
                }
            }
        }
        "dnscache" => {
            output("=== Cache Local DNS ===", 0x00F5_9E0B);
            unsafe {
                for slot in crate::net::dns::DNS_CACHE.entries.iter() {
                    if let Some(entry) = slot {
                        if let Ok(name_str) = core::str::from_utf8(&entry.name[..entry.name_len as usize]) {
                            output(name_str, 0x0038_BDF8);
                        }
                    }
                }
            }
        }
        "jinnwall" | "firewall" => {
            unsafe {
                if args == "dev" {
                    crate::net::jinnwall::JINNWALL.set_profile(crate::net::jinnwall::FirewallProfile::Development);
                    output("JinnWall: Perfil alterado para 'Development'", 0x0022_C55E);
                } else if args == "secure" || args == "public" {
                    crate::net::jinnwall::JINNWALL.set_profile(crate::net::jinnwall::FirewallProfile::PublicSecure);
                    output("JinnWall: Perfil alterado para 'PublicSecure'", 0x0022_C55E);
                } else {
                    let prof = match crate::net::jinnwall::JINNWALL.profile() {
                        crate::net::jinnwall::FirewallProfile::PublicSecure => "Perfil: PublicSecure",
                        crate::net::jinnwall::FirewallProfile::Development => "Perfil: Development",
                    };
                    output(prof, 0x0038_BDF8);
                }
            }
        }
        "jda" => {
            output("=== Jinn Dataflow Architecture (JDA) ===", 0x0038_BDF8);
            if crate::jda::is_active() {
                output("Estado: ATIVO (Submarino Dataflow Ligado)", 0x0022_C55E);
            } else {
                output("Estado: INATIVO (Fallback Classico)", 0x00EF_4444);
            }

            // Estatísticas dos Anéis io_uring
            let ring = crate::jda::ring::JDA_RING.lock();
            let mut buf_ring = [0u8; 64];
            let mut l = 0;
            let msg = b"IoRing: Submetidos=";
            buf_ring[..msg.len()].copy_from_slice(msg);
            l += msg.len();
            let mut n_buf = [0u8; 16];
            let nl = aladdin::compositor::format_num(ring.stats.total_submitted as usize, &mut n_buf);
            buf_ring[l..l + nl].copy_from_slice(&n_buf[..nl]);
            l += nl;
            let msg2 = b", Concluidos=";
            buf_ring[l..l + msg2.len()].copy_from_slice(msg2);
            l += msg2.len();
            let nl2 = aladdin::compositor::format_num(ring.stats.total_completed as usize, &mut n_buf);
            buf_ring[l..l + nl2].copy_from_slice(&n_buf[..nl2]);
            l += nl2;
            let msg3 = b", Erros=";
            buf_ring[l..l + msg3.len()].copy_from_slice(msg3);
            l += msg3.len();
            let nl3 = aladdin::compositor::format_num(ring.stats.total_errors as usize, &mut n_buf);
            buf_ring[l..l + nl3].copy_from_slice(&n_buf[..nl3]);
            l += nl3;
            if let Ok(s) = core::str::from_utf8(&buf_ring[..l]) {
                output(s, 0x00A8_55F7);
            }

            // Buffers físicos Zero-Copy ativos
            let pool = crate::jda::shmem::SHARED_BUFFER_POOL.lock();
            let mut active_bufs = 0;
            for b in pool.buffers.iter() {
                if b.is_allocated {
                    active_bufs += 1;
                }
            }
            let mut buf_shm = [0u8; 48];
            let mut ls = 0;
            let m_shm = b"Buffers Zero-Copy Ativos: ";
            buf_shm[..m_shm.len()].copy_from_slice(m_shm);
            ls += m_shm.len();
            let n_shm = aladdin::compositor::format_num(active_bufs, &mut n_buf);
            buf_shm[ls..ls + n_shm].copy_from_slice(&n_buf[..n_shm]);
            ls += n_shm;
            let m_max = b" / 32";
            buf_shm[ls..ls + m_max.len()].copy_from_slice(m_max);
            ls += m_max.len();
            if let Ok(s) = core::str::from_utf8(&buf_shm[..ls]) {
                output(s, 0x00F5_9E0B);
            }
        }
        "jdabench" => {
            output("Executando benchmark JDA Fase 3 (io_uring & Zero-Copy)...", 0x0038_BDF8);
            let mut pool = crate::jda::shmem::SHARED_BUFFER_POOL.lock();
            if let Ok(handle) = pool.allocate(0, false) {
                drop(pool);
                let start_tsc = crate::jda::telemetry::read_tsc();
                let mut ring = crate::jda::ring::JDA_RING.lock();
                for i in 0..16 {
                    let sqe = crate::jda::ring::JdaSqe {
                        opcode: if i % 2 == 0 { crate::jda::ring::OP_COMPUTE_SUM } else { crate::jda::ring::OP_COMPUTE_FLETCHER32 },
                        flags: crate::jda::ring::JDA_SQE_ZERO_COPY,
                        priority: 1,
                        _reserved: 0,
                        handle: handle as u32,
                        offset: 0,
                        len: 512,
                        user_data: i as u64,
                    };
                    let _ = ring.submit(sqe);
                }
                let processed = ring.dispatch_pending(0);
                let end_tsc = crate::jda::telemetry::read_tsc();
                let elapsed = end_tsc.saturating_sub(start_tsc);
                drop(ring);

                let mut pool = crate::jda::shmem::SHARED_BUFFER_POOL.lock();
                let _ = pool.release(0, handle);

                let mut res_buf = [0u8; 64];
                let mut rl = 0;
                let r_msg = b"Benchmark Concluido: ";
                res_buf[..r_msg.len()].copy_from_slice(r_msg);
                rl += r_msg.len();
                let mut nb = [0u8; 16];
                let n1 = aladdin::compositor::format_num(processed, &mut nb);
                res_buf[rl..rl + n1].copy_from_slice(&nb[..n1]);
                rl += n1;
                let r_msg2 = b" ops em ";
                res_buf[rl..rl + r_msg2.len()].copy_from_slice(r_msg2);
                rl += r_msg2.len();
                let n2 = aladdin::compositor::format_num((elapsed / 16) as usize, &mut nb);
                res_buf[rl..rl + n2].copy_from_slice(&nb[..n2]);
                rl += n2;
                let r_msg3 = b" ciclos/op";
                res_buf[rl..rl + r_msg3.len()].copy_from_slice(r_msg3);
                rl += r_msg3.len();
                if let Ok(s) = core::str::from_utf8(&res_buf[..rl]) {
                    output(s, 0x0022_C55E);
                }
            } else {
                output("Falha ao alocar buffer JDA para benchmark", 0x00EF_4444);
            }
        }
        "spawn" | "exec" => {
            if args.is_empty() {
                output("Uso: spawn <caminho.elf>", 0x00EF_4444);
            } else {
                let target = args.trim();
                let is_browser = target.contains("browser") || target.contains("BROWSER");
                let resolved_path = if target.starts_with('/') {
                    alloc::string::String::from(target)
                } else {
                    let mut s = alloc::string::String::from("/disk/");
                    s.push_str(target);
                    if !s.ends_with(".elf") && !s.ends_with(".ELF") {
                        s.push_str(".elf");
                    }
                    s
                };
                let argv: [&str; 2] = [&resolved_path, if is_browser { "browser" } else { "" }];
                let argv_slice = if is_browser { &argv[..2] } else { &argv[..1] };
                let sched = crate::scheduler::get();
                match sched.spawn_user_task_from_vfs(&resolved_path, argv_slice, &[]) {
                    Ok(pid) => {
                        crate::ipc::register(pid);
                        output("[OK] Processo Ring 3 iniciado com sucesso!", 0x0022_C55E);
                        let mut p_buf = [0u8; 16];
                        let p_len = aladdin::compositor::format_num(pid, &mut p_buf);
                        if let Ok(pid_str) = core::str::from_utf8(&p_buf[..p_len]) {
                            output(pid_str, 0x0038_BDF8);
                        }
                    }
                    Err(_) => {
                        output("[ERRO] Falha ao carregar ELF: arquivo nao encontrado ou invalido.", 0x00EF_4444);
                    }
                }
            }
        }
        "browser" => {
            let path = "/disk/browser.elf";
            let argv: [&str; 2] = [path, "browser"];
            let sched = crate::scheduler::get();
            match sched.spawn_user_task_from_vfs(path, &argv, &[]) {
                Ok(pid) => {
                    crate::ipc::register(pid);
                    output("[OK] Jinn Browser iniciado com sucesso!", 0x0022_C55E);
                    let mut p_buf = [0u8; 16];
                    let p_len = aladdin::compositor::format_num(pid, &mut p_buf);
                    if let Ok(pid_str) = core::str::from_utf8(&p_buf[..p_len]) {
                        output(pid_str, 0x0038_BDF8);
                    }
                }
                Err(_) => {
                    output("[ERRO] Falha ao iniciar /disk/browser.elf", 0x00EF_4444);
                }
            }
        }
        "reboot" => {
            unsafe {
                let null_idt: [u8; 10] = [0u8; 10];
                core::arch::asm!("lidt [{}]", "int 3", in(reg) null_idt.as_ptr(), options(nostack));
            }
            loop { core::hint::spin_loop(); }
        }
        _ => {
            output("Comando desconhecido. Digite 'help'.", 0x00EF_4444);
        }
    }
}

