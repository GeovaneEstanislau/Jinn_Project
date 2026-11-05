//! Aplicativo de Usuário Gráfico Independente em Ring 3 (Jinn OS & Aladdin Desktop)
//!
//! Este binário executa inteiramente em espaço de usuário (Ring 3), comunicando-se
//! com o microkernel Jinn OS e com o compositor visual do Aladdin Desktop
//! exclusivamente através da interface de chamadas de sistema (syscalls via `int 0x80`).

#![no_std]
#![no_main]

use aladdin_sdk::{
    exit, open_window, print, println, sleep,
    ClientWindowEvent, EVENT_WINDOW_CLOSE, EVENT_KEY_DOWN, EVENT_MOUSE_DOWN
};

const WIN_WIDTH: u32 = 320;
const WIN_HEIGHT: u32 = 200;

// Framebuffer alocado em .bss (0 bytes no arquivo ELF do disco, mapeado em Ring 3 com USER_RW)
static mut FRAMEBUFFER: [u32; (WIN_WIDTH * WIN_HEIGHT) as usize] = [0; (WIN_WIDTH * WIN_HEIGHT) as usize];

#[no_mangle]
pub extern "C" fn _start() -> ! {
    println("[ola_grafico] ==========================================");
    println("[ola_grafico] Processo Grafico Externo Ring 3 Iniciado!");
    println("[ola_grafico] Solicitando criacao de janela 320x200 ao Aladdin Compositor...");

    let win = match open_window("App Externo U3", WIN_WIDTH, WIN_HEIGHT) {
        Ok(w) => {
            println("[ola_grafico] [V] Janela criada com sucesso no Compositor!");
            w
        }
        Err(_err) => {
            println("[ola_grafico] [X] Falha ao criar janela!");
            exit(1);
        }
    };

    println("[ola_grafico] Desenhando padrao visual e gradiente no framebuffer...");
    let fb = unsafe { &mut FRAMEBUFFER };

    // Desenha gradiente Cyber-Neon com moldura geométrica
    for y in 0..WIN_HEIGHT {
        for x in 0..WIN_WIDTH {
            let idx = (y * WIN_WIDTH + x) as usize;
            
            // Borda de 2 pixels
            if x < 2 || x >= WIN_WIDTH - 2 || y < 2 || y >= WIN_HEIGHT - 2 {
                fb[idx] = 0x0000_E5FF; // Ciano Neon
            } else if y < 24 {
                // Barra de cabeçalho interna da janela
                fb[idx] = 0x001E_293B; // Azul ardósia escuro
            } else {
                // Fundo com gradiente suave (X e Y)
                let r = ((x * 180) / WIN_WIDTH) & 0xFF;
                let g = ((y * 120) / WIN_HEIGHT) & 0xFF;
                let b = (200 - ((x * 100) / WIN_WIDTH)) & 0xFF;
                
                // Padrão geométrico sutil
                let grid = ((x / 16) + (y / 16)) % 2 == 0;
                let color = if grid {
                    ((r / 2) << 16) | ((g / 2) << 8) | (b / 2)
                } else {
                    (r << 16) | (g << 8) | b
                };

                // Caixa central de destaque (320x200 centro: 60..260, 60..150)
                if x >= 60 && x <= 260 && y >= 60 && y <= 150 {
                    if x == 60 || x == 260 || y == 60 || y == 150 {
                        fb[idx] = 0x0010_B981; // Verde Esmeralda
                    } else {
                        fb[idx] = 0x000F_172A; // Azul Profundo
                    }
                } else {
                    fb[idx] = color;
                }
            }
        }
    }

    // Desenha um padrão geométrico em X no centro da caixa
    for d in 0..60 {
        let x1 = 160 - 30 + d;
        let y1 = 105 - 30 + d;
        if x1 < WIN_WIDTH && y1 < WIN_HEIGHT {
            fb[(y1 * WIN_WIDTH + x1) as usize] = 0x00F5_9E0B; // Âmbar
        }
        let x2 = 160 + 30 - d;
        let y2 = 105 - 30 + d;
        if x2 < WIN_WIDTH && y2 < WIN_HEIGHT {
            fb[(y2 * WIN_WIDTH + x2) as usize] = 0x00EC_4899; // Rosa Neon
        }
    }

    if let Err(_e) = win.blit(fb) {
        println("[ola_grafico] [X] Erro ao apresentar framebuffer via sys_window_blit");
    } else {
        println("[ola_grafico] [V] Framebuffer apresentado com sucesso via sys_window_blit!");
    }

    println("[ola_grafico] Entrando no loop de eventos. Feche a janela para encerrar...");

    let mut tick = 0u32;
    loop {
        // Coleta eventos da janela
        if let Some(ev) = win.poll_event() {
            if ev.event_type == EVENT_WINDOW_CLOSE {
                println("[ola_grafico] Evento EVENT_WINDOW_CLOSE recebido! Encerrando app...");
                break;
            } else if ev.event_type == EVENT_KEY_DOWN {
                println("[ola_grafico] Tecla pressionada na janela cliente!");
                if ev.button_or_key == 0x1B { // ESC
                    println("[ola_grafico] Tecla ESC detectada. Encerrando app...");
                    break;
                }
            }
        }

        // Animação sutil de pulso geométrico na moldura da caixa a cada ~100ms
        tick = tick.wrapping_add(1);
        if tick % 6 == 0 {
            let pulse_color = if (tick / 6) % 2 == 0 { 0x0000_E5FF } else { 0x0010_B981 };
            for x in 62..258 {
                fb[(62 * WIN_WIDTH + x) as usize] = pulse_color;
                fb[(148 * WIN_WIDTH + x) as usize] = pulse_color;
            }
            let _ = win.blit(fb);
        }

        sleep(16); // ~60 FPS sem consumo ocioso de CPU
    }

    let _ = win.close();
    println("[ola_grafico] Janela fechada. Saindo limpo com exit(0).");
    exit(0);
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    println("[ola_grafico] PANIC detectado no processo Ring 3! Abortando...");
    exit(101);
}
