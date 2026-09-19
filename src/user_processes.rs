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

/// Imprime um número `u64` em decimal via syscall `write`.
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

// ── Processo 4: Keyboard Driver ───────────────────────────────────────────────

/// Shifted US-QWERTY Scancode Set 1 → ASCII (Ring 3).
static SCANCODE_MAP_NORMAL_R3: [u8; 88] = [
    0,     0x1B,  b'1',  b'2',  b'3',  b'4',  b'5',  b'6',  b'7',  b'8',  b'9',  b'0',  b'-',  b'=',  0x08,  b'\t',
    b'q',  b'w',  b'e',  b'r',  b't',  b'y',  b'u',  b'i',  b'o',  b'p',  b'[',  b']',  b'\n', 0,     b'a',  b's',
    b'd',  b'f',  b'g',  b'h',  b'j',  b'k',  b'l',  b';',  b'\'', b'`',  0,     b'\\', b'z',  b'x',  b'c',  b'v',
    b'b',  b'n',  b'm',  b',',  b'.',  b'/',  0,     b'*',  0,     b' ',  0,     0,     0,     0,     0,     0,
    0,     0,     0,     0,     0,     0,     0,     b'7',  b'8',  b'9',  b'-',  b'4',  b'5',  b'6',  b'+',  b'1',
    b'2',  b'3',  b'0',  b'.',  0,     0,     0,     0,
];

static SCANCODE_MAP_SHIFT_R3: [u8; 88] = [
    0,     0x1B,  b'!',  b'@',  b'#',  b'$',  b'%',  b'^',  b'&',  b'*',  b'(',  b')',  b'_',  b'+',  0x08,  b'\t',
    b'Q',  b'W',  b'E',  b'R',  b'T',  b'Y',  b'U',  b'I',  b'O',  b'P',  b'{',  b'}',  b'\n', 0,     b'A',  b'S',
    b'D',  b'F',  b'G',  b'H',  b'J',  b'K',  b'L',  b':',  b'"',  b'~',  0,     b'|',  b'Z',  b'X',  b'C',  b'V',
    b'B',  b'N',  b'M',  b'<',  b'>',  b'?',  0,     b'*',  0,     b' ',  0,     0,     0,     0,     0,     0,
    0,     0,     0,     0,     0,     0,     0,     b'7',  b'8',  b'9',  b'-',  b'4',  b'5',  b'6',  b'+',  b'1',
    b'2',  b'3',  b'0',  b'.',  0,     0,     0,     0,
];

/// **Driver de Teclado em User-Space (Ring 3)**
///
/// Prova do conceito de Microkernel: o processo registra IRQ 1.
/// O Kernel capta a interrupção de hardware e manda via IPC.
/// Este driver lê a mensagem IPC, traduz o scancode para ASCII e envia para `stdin` via `sys_write(0)`.
pub fn processo_keyboard_driver() {
    unsafe {
        let pid = sys_getpid();
        sys_write_str(b"[Ring 3] kbd-driver: PID=");
        write_u64_decimal(pid);
        sys_write_str(b" assumindo IRQ 1 (Teclado)!\n");

        if sys_register_irq(1) < 0 {
            sys_write_str(b"[!] kbd-driver falhou ao registrar IRQ 1.\n");
            sys_exit(1);
        }

        let mut shift_down = false;
        let mut caps_lock = false;

        let mut buf = [0u8; 128];
        loop {
            let bytes_lidos = sys_ipc_recv(buf.as_mut_ptr(), buf.len() as u64);
            if bytes_lidos > 0 {
                let tag_ptr = buf.as_ptr().add(8) as *const u32;
                let tag = core::ptr::read_unaligned(tag_ptr);

                if tag == 0x180 { // TAG_IRQ
                    let sc = buf[16]; // payload[0]
                    
                    let is_release = (sc & 0x80) != 0;
                    let make_code = sc & 0x7F;

                    match make_code {
                        0x2A | 0x36 => { shift_down = !is_release; } // Shift L/R
                        0x3A if !is_release => { caps_lock = !caps_lock; }
                        code if !is_release && (code as usize) < SCANCODE_MAP_NORMAL_R3.len() => {
                            let shifted = shift_down ^ (caps_lock && code >= 0x10 && code <= 0x32);
                            let ch = if shifted {
                                SCANCODE_MAP_SHIFT_R3[code as usize]
                            } else {
                                SCANCODE_MAP_NORMAL_R3[code as usize]
                            };
                            
                            if ch != 0 {
                                // Envia caractere ao kernel (keyboard_buffer)
                                sys_write(0, &ch as *const u8, 1);
                            }
                        }
                        _ => {}
                    }
                }
            } else {
                sys_yield();
            }
        }
    }
}
