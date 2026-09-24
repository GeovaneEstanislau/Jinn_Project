/// Jinn OS — Processos de Usuário (Ring 3)
///
/// Este módulo contém as **funções de entrada** para processos que executam
/// em Ring 3 (modo de usuário). Eles NÃO podem acessar hardware diretamente —
/// toda operação privilegiada passa pela interface de syscalls (veja
/// `syscall_user.rs`).
///
/// # Como funciona a transição Ring 0 → Ring 3
///
/// 1. O escalonador cria a tarefa com `add_user_task()`.
/// 2. Um frame de `iretq` é montado com `CS = USER_CODE_SEL` (DPL 3) e
///    `RSP` apontando para a pilha de usuário alocada em memória virtual.
/// 3. Na primeira preempção do APIC Timer, o contexto é restaurado via
///    `iretq`, que carrega CS com DPL 3 e muda o modo de execução.
/// 4. A partir daí, qualquer tentativa de acessar I/O diretamente ou
///    executar instruções privilegiadas gera uma #GP (General Protection Fault).
///
/// # Processos disponíveis
///
/// | Função                  | Descrição                                      |
/// |-------------------------|------------------------------------------------|
/// | `processo_ola_mundo`    | Imprime mensagem e faz loop de yield           |
/// | `processo_contador`     | Conta até N, imprime progresso e faz exit(0)  |
/// | `processo_ipc_echo`     | Aguarda mensagem IPC e ecoa de volta           |

use crate::syscall_user::*;

// ── Processo 1: Olá Mundo ─────────────────────────────────────────────────────

/// **Primeiro processo Ring 3 do Jinn OS — demonstração histórica.**
///
/// 1. Obtém seu PID via `getpid`.
/// 2. Imprime uma saudação via `write`.
/// 3. Entra em loop cedendo a CPU — demonstra que o scheduler preemptivo
///    continua funcionando mesmo com processos de usuário ativos.
pub fn processo_ola_mundo() {
    unsafe {
        let pid = sys_getpid();

        // Identifica o processo na tela
        sys_write_str(b"[Ring 3] ola-mundo: processo de usuario ativo! PID=");
        write_u64_decimal(pid);
        sys_write_str(b"\n");

        let mut ticks: u64 = 0;
        loop {
            // A cada 500 yields, imprime um heartbeat
            if ticks % 500 == 0 {
                sys_write_str(b"[Ring 3] ola-mundo: cedendo CPU (tick=");
                write_u64_decimal(ticks);
                sys_write_str(b")\n");
            }
            ticks = ticks.wrapping_add(1);
            sys_yield();
        }
    }
}

// ── Processo 2: Contador ──────────────────────────────────────────────────────

/// **Processo que conta até um limite e termina com `exit(0)`.**
///
/// Demonstra o ciclo de vida completo:
/// - Início → trabalho → `exit` → estado `Zombie` → coleta pelo kernel.
pub fn processo_contador() {
    const LIMITE: u64 = 5;

    unsafe {
        let pid = sys_getpid();

        sys_write_str(b"[Ring 3] contador: iniciando (PID=");
        write_u64_decimal(pid);
        sys_write_str(b", limite=");
        write_u64_decimal(LIMITE);
        sys_write_str(b")\n");

        for i in 1..=LIMITE {
            sys_write_str(b"[Ring 3] contador: passo ");
            write_u64_decimal(i);
            sys_write_str(b" / ");
            write_u64_decimal(LIMITE);
            sys_write_str(b"\n");

            // Cede CPU entre cada passo — não quer monopolizar
            sys_yield();
        }

        sys_write_str(b"[Ring 3] contador: concluido! Finalizando com exit(0).\n");
        sys_exit(0);
    }
}

// ── Processo 3: IPC Echo ──────────────────────────────────────────────────────

/// **Servidor IPC simples — aguarda mensagens e ecoa de volta.**
///
/// Demonstra comunicação inter-processo no Jinn OS:
/// - Aguarda mensagens IPC na sua fila.
/// - Ecoa o payload da mensagem de volta para o remetente.
/// - Encerra ao receber uma mensagem com `tag = 0xDEAD`.
pub fn processo_ipc_echo() {
    unsafe {
        let pid = sys_getpid();

        sys_write_str(b"[Ring 3] ipc-echo: aguardando mensagens (PID=");
        write_u64_decimal(pid);
        sys_write_str(b")\n");

        // Buffer para receber mensagens IPC (tamanho da struct Message no kernel)
        let mut buf = [0u8; 128];
        let mut idle_yields: u64 = 0;

        loop {
            let n = sys_ipc_recv(buf.as_mut_ptr(), buf.len() as u64);

            if n > 0 {
                // Mensagem recebida — lê o from_pid (bytes 0..4) e tag (bytes 4..8)
                let from_pid = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
                let tag      = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);

                sys_write_str(b"[Ring 3] ipc-echo: mensagem de PID=");
                write_u64_decimal(from_pid as u64);
                sys_write_str(b" tag=");
                write_u64_decimal(tag as u64);
                sys_write_str(b"\n");

                // Tag de encerramento
                if tag == 0xDEAD {
                    sys_write_str(b"[Ring 3] ipc-echo: tag DEAD recebido. Encerrando.\n");
                    sys_exit(0);
                }

                // Ecoa de volta para o remetente
                sys_ipc_send(from_pid, tag, buf[8..].as_ptr());
                idle_yields = 0;
            } else {
                // Sem mensagens: aguarda cooperativamente
                idle_yields = idle_yields.wrapping_add(1);
                if idle_yields % 2000 == 0 {
                    sys_write_str(b"[Ring 3] ipc-echo: aguardando...\n");
                }
                sys_yield();
            }
        }
    }
}

// ── Helpers de formatação (Ring 3, sem acesso ao kernel) ─────────────────────


/// Demonstrates spawning a child process and waiting for its exit using `waitpid`.
pub fn processo_wait_demo() {
    unsafe {
        // Spawn child task that runs `processo_ola_mundo`
        let child_fn = processo_ola_mundo as *const () as usize;
        let child_pid = sys_spawn_user(child_fn) as u64;
        sys_write_str(b"[Ring 3] wait-demo: spawned child PID=");
        write_u64_decimal(child_pid);
        sys_write_str(b"\n");

        // Wait for the child to finish and retrieve its exit code
        let exit_code = sys_waitpid(child_pid);
        sys_write_str(b"[Ring 3] wait-demo: child exited with code ");
        write_u64_decimal(exit_code as u64);
        sys_write_str(b"\n");
    }
}

///
/// Implementação própria porque código Ring 3 não pode usar `core::fmt`
/// com a nossa VGA Writer (que é Ring 0). Tudo precisa passar por `write`.
unsafe fn write_u64_decimal(mut n: u64) {
    if n == 0 {
        sys_write_str(b"0");
        return;
    }
    let mut buf = [0u8; 20]; // u64 max tem 20 dígitos
    let mut i = 20usize;
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    sys_write(1, buf[i..].as_ptr(), (20 - i) as u64);
}
