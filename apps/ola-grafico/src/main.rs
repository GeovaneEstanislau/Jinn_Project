//! Aplicativo de Usuário Gráfico Independente em Ring 3 (Jinn OS & Aladdin Desktop)
//!
//! Este binário executa inteiramente em espaço de usuário (Ring 3), comunicando-se
//! com o microkernel Jinn OS e com o compositor visual do Aladdin Desktop
//! exclusivamente através da interface de chamadas de sistema (syscalls via `int 0x80`).

#![no_std]
#![no_main]

use aladdin_sdk::prelude::*;

const WIN_WIDTH: u32 = 320;
const WIN_HEIGHT: u32 = 200;

// Framebuffer alocado em .bss (0 bytes no arquivo ELF do disco, mapeado em Ring 3 com USER_RW)
static mut FRAMEBUFFER: [u32; (WIN_WIDTH * WIN_HEIGHT) as usize] = [0; (WIN_WIDTH * WIN_HEIGHT) as usize];

#[no_mangle]
pub extern "C" fn _start() -> ! {
    println("[ola_grafico] ==========================================");
    println("[ola_grafico] Processo Grafico Externo Ring 3 Iniciado!");
    print("[ola_grafico] PID do processo: ");
    print_num(getpid());
    println("");
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
    let fb = unsafe { &mut *core::ptr::addr_of_mut!(FRAMEBUFFER) };
    let mut canvas = Canvas::new(fb, WIN_WIDTH as usize, WIN_HEIGHT as usize)
        .expect("Canvas deve corresponder ao buffer");

    // Desenha gradiente inicial
    canvas.draw_gradient(Color::CARD_BG, Color::rgb(15, 35, 65));
    canvas.draw_rect(0, 0, WIN_WIDTH as usize, WIN_HEIGHT as usize, Color::CYAN);
    canvas.draw_rect(1, 1, (WIN_WIDTH - 2) as usize, (WIN_HEIGHT - 2) as usize, Color::CYAN);

    // Caixa central de destaque
    canvas.fill_rect(50, 45, 220, 110, Color::rgb(20, 30, 48));
    canvas.draw_rect(50, 45, 220, 110, Color::GREEN);

    if let Err(_e) = win.blit(canvas.buffer) {
        println("[ola_grafico] [X] Erro ao apresentar framebuffer via sys_window_blit");
    } else {
        println("[ola_grafico] [V] Framebuffer apresentado com sucesso via sys_window_blit!");
    }

    println("[ola_grafico] Entrando no loop de eventos.");
    println("[ola_grafico] Comandos interativos:");
    println("[ola_grafico]   - Clique do mouse: desenha marcador no ponto do cursor");
    println("[ola_grafico]   - Barra de Espaco: alterna esquema de cores do degradê");
    println("[ola_grafico]   - Tecla ESC ou Botao [X]: encerra o aplicativo limpo");

    let mut tick = 0u32;
    let mut color_mode = 0u8;
    let mut click_marker: Option<(usize, usize)> = None;

    loop {
        // Coleta e processa eventos isolados da janela
        while let Some(ev) = win.poll_event() {
            if ev.is_close() {
                println("[ola_grafico] [V] Evento EVENT_WINDOW_CLOSE recebido! Encerrando...");
                let _ = win.close();
                println("[ola_grafico] [V] Recursos de janela liberados. Saindo com exit(0).");
                exit(0);
            }

            if let Some(key) = ev.key_code() {
                print("[ola_grafico] Evento de Teclado detectado: key=");
                print_hex(key as u64);
                println("");

                if key == 0x1B { // ESC
                    println("[ola_grafico] Tecla ESC detectada. Finalizando processo...");
                    let _ = win.close();
                    println("[ola_grafico] [V] Janela destruida e recursos liberados. Saindo com exit(0).");
                    exit(0);
                } else if key == 0x20 { // Space
                    color_mode = (color_mode + 1) % 3;
                    println("[ola_grafico] Esquema de cores alternado pelo usuario.");
                }
            }

            if let Some((mx, my)) = ev.mouse_coords() {
                if ev.is_mouse_down() {
                    print("[ola_grafico] Clique do mouse na janela: x=");
                    print_num(mx as u64);
                    print(", y=");
                    print_num(my as u64);
                    println("");
                    click_marker = Some((mx as usize, my as usize));
                }
            }
        }

        tick = tick.wrapping_add(1);

        // Atualização gráfica animada (~30 FPS)
        if tick % 2 == 0 {
            let (c_top, c_bot) = match color_mode {
                0 => (Color::rgb(15, 23, 42), Color::rgb(30, 58, 138)),
                1 => (Color::rgb(30, 20, 50), Color::rgb(88, 28, 135)),
                _ => (Color::rgb(10, 40, 30), Color::rgb(6, 78, 59)),
            };

            canvas.draw_gradient(c_top, c_bot);

            // Moldura externa
            canvas.draw_rect(0, 0, WIN_WIDTH as usize, WIN_HEIGHT as usize, Color::CYAN);

            // Caixa central
            canvas.fill_rect(50, 45, 220, 110, Color::rgb(20, 30, 48));
            canvas.draw_rect(50, 45, 220, 110, Color::GREEN);

            // Retângulo dinâmico oscilante
            let phase = ((tick / 2) % 60) as usize;
            let bar_w = 40 + (phase * 2).min(100);
            canvas.fill_rect(80, 85, bar_w, 20, Color::YELLOW);
            canvas.draw_rect(80, 85, bar_w, 20, Color::ORANGE);

            // Marcador de clique do mouse (se o usuário clicou)
            if let Some((cx, cy)) = click_marker {
                if cx >= 4 && cy >= 4 && cx + 4 < WIN_WIDTH as usize && cy + 4 < WIN_HEIGHT as usize {
                    canvas.fill_rect(cx - 4, cy - 4, 9, 9, Color::RED);
                    canvas.draw_rect(cx - 6, cy - 6, 13, 13, Color::WHITE);
                }
            }

            let _ = win.blit(canvas.buffer);
        }

        sleep(16); // 16 ms sleep
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    println("[ola_grafico] [X] PANIC detectado no processo Ring 3! Abortando...");
    exit(101);
}
