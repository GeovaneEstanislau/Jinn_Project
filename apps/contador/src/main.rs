//! Aplicativo Independente em Ring 3 — Contador Visual & Teste de Concorrência
//!
//! Demonstra que múltiplos processos Ring 3 isolados podem alocar suas próprias janelas
//! no Aladdin Desktop de forma concorrente, com isolamento total de buffers e eventos.

#![no_std]
#![no_main]

use aladdin_sdk::prelude::*;

const WIN_WIDTH: u32 = 240;
const WIN_HEIGHT: u32 = 160;

// Framebuffer alocado em .bss da tarefa (isolado do kernel e de outras tarefas)
static mut FRAMEBUFFER: [u32; (WIN_WIDTH * WIN_HEIGHT) as usize] = [0; (WIN_WIDTH * WIN_HEIGHT) as usize];

#[no_mangle]
pub extern "C" fn _start() -> ! {
    println("[contador] ==========================================");
    println("[contador] Processo Ring 3 'Contador' Inicializado!");
    print("[contador] PID da tarefa: ");
    print_num(getpid());
    println("");

    let win = match open_window("Contador Ring 3", WIN_WIDTH, WIN_HEIGHT) {
        Ok(w) => {
            println("[contador] [V] Janela criada com sucesso no Compositor!");
            w
        }
        Err(_err) => {
            println("[contador] [X] Erro ao criar janela do Contador!");
            exit(2);
        }
    };

    let fb = unsafe { &mut *core::ptr::addr_of_mut!(FRAMEBUFFER) };
    let mut canvas = Canvas::new(fb, WIN_WIDTH as usize, WIN_HEIGHT as usize)
        .expect("Canvas deve corresponder ao buffer");

    println("[contador] Loop de eventos iniciado.");
    println("[contador] Interacoes:");
    println("[contador]   - Clique do mouse: adiciona +10 ao valor");
    println("[contador]   - Tecla 'R': reinicia contador");
    println("[contador]   - Tecla ESC ou [X]: encerra processo");

    let mut counter = 0u32;
    let mut speed = 1u32;
    let mut step = 0u32;

    loop {
        // Coleta e processa eventos isolados desta janela
        while let Some(ev) = win.poll_event() {
            if ev.is_close() {
                println("[contador] [V] EVENT_WINDOW_CLOSE recebido. Encerrando...");
                let _ = win.close();
                exit(0);
            }

            if let Some(key) = ev.key_code() {
                if key == 0x1B { // ESC
                    println("[contador] [V] ESC pressionado. Finalizando...");
                    let _ = win.close();
                    exit(0);
                } else if key == 0x52 || key == 0x72 { // 'R' ou 'r'
                    counter = 0;
                    println("[contador] Contador reiniciado para zero.");
                } else if key == 0x41 || key == 0x61 { // 'A' ou 'a'
                    speed = if speed == 1 { 5 } else { 1 };
                    print("[contador] Velocidade alternada: x");
                    print_num(speed as u64);
                    println("");
                }
            }

            if ev.is_mouse_down() {
                counter = counter.wrapping_add(10);
                println("[contador] Clique detectado (+10 adicionado ao contador).");
            }
        }

        step = step.wrapping_add(1);
        counter = counter.wrapping_add(speed);

        // Renderiza interface visual
        canvas.clear(Color::rgb(15, 23, 42)); // Fundo azul escuro

        // Moldura externa da janela
        canvas.draw_rect(0, 0, WIN_WIDTH as usize, WIN_HEIGHT as usize, Color::rgb(56, 189, 248));
        canvas.draw_rect(1, 1, (WIN_WIDTH - 2) as usize, (WIN_HEIGHT - 2) as usize, Color::rgb(56, 189, 248));

        // Cabeçalho da aplicação
        canvas.fill_rect(8, 8, (WIN_WIDTH - 16) as usize, 24, Color::rgb(30, 41, 59));
        canvas.draw_rect(8, 8, (WIN_WIDTH - 16) as usize, 24, Color::rgb(100, 116, 139));

        // Painel do display de valor
        canvas.fill_rect(16, 42, (WIN_WIDTH - 32) as usize, 55, Color::rgb(2, 6, 23));
        canvas.draw_rect(16, 42, (WIN_WIDTH - 32) as usize, 55, Color::rgb(34, 197, 94));

        // Barra de progresso / gauge animado (0 a 200 pixels)
        let gauge_max = (WIN_WIDTH - 48) as usize;
        let gauge_fill = ((counter % 1000) as usize * gauge_max) / 1000;
        canvas.fill_rect(24, 108, gauge_max, 14, Color::rgb(30, 41, 59));
        canvas.fill_rect(24, 108, gauge_fill, 14, Color::rgb(16, 185, 129));
        canvas.draw_rect(24, 108, gauge_max, 14, Color::rgb(52, 211, 153));

        // Gráfico de pulso decorativo
        let pulse_x = 24 + ((step as usize * 3) % gauge_max);
        canvas.fill_rect(pulse_x.saturating_sub(4), 128, 8, 8, Color::rgb(244, 63, 94));

        // Apresenta pixels no compositor
        let _ = win.blit(canvas.buffer);

        sleep(33); // ~30 FPS
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    println("[contador] [X] PANIC detectado no processo Contador! Abortando...");
    exit(102);
}

