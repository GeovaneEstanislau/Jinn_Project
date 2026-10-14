use crate::limine;
use crate::vga::{Color, color_to_u32_fb};
use crate::font;

#[derive(Clone, Copy)]
pub struct FramebufferInfo {
    pub address: *mut u8,
    pub width: u64,
    pub height: u64,
    pub pitch: u64,
    pub bpp: u16,
    pub red_mask_shift: u8,
    pub green_mask_shift: u8,
    pub blue_mask_shift: u8,
}

static mut FB_INFO: Option<FramebufferInfo> = None;
static mut BACKBUFFER: *mut u32 = core::ptr::null_mut();

// Dimensões padrão
const DEFAULT_WIDTH: usize = 1024;
const DEFAULT_HEIGHT: usize = 768;

// Cores da GUI (ARGB)
pub const COLOR_DESKTOP: u32      = 0x0016_202C; // Azul ardósia escuro
pub const COLOR_TASKBAR: u32      = 0x0010_171E; // Barra inferior escura
pub const COLOR_START_BTN: u32    = 0x0000_7ACC; // Azul VSCode / Jinn
pub const COLOR_WIN_BG: u32       = 0x00EA_EEF2; // Corpo da janela (cinza claro moderno)
pub const COLOR_WIN_TITLE_ACT: u32= 0x001B_4965; // Barra de título ativa
pub const COLOR_WIN_TITLE_INA: u32= 0x004A_5568; // Barra de título inativa
pub const COLOR_BTN_CLOSE: u32    = 0x00E5_3E3E; // Botão fechar [X] vermelho
pub const COLOR_WHITE: u32        = 0x00FF_FFFF;
pub const COLOR_BLACK: u32        = 0x0000_0000;
pub const COLOR_TEXT_DARK: u32    = 0x001A_202C;
pub const COLOR_ACCENT_GREEN: u32 = 0x0038_A169;

// Sprite do cursor de mouse (11x16)
const CURSOR_W: usize = 11;
const CURSOR_H: usize = 16;
const CURSOR_BITMAP: [&str; 16] = [
    "#..........",
    "##.........",
    "#*#........",
    "#**#.......",
    "#***#......",
    "#****#.....",
    "#*****#....",
    "#******#...",
    "#*******#..",
    "#*****####.",
    "#**#**#....",
    "#*#.#**#...",
    "##..#**#...",
    "#....#**#..",
    ".....#**#..",
    "......##...",
];

// Estado do cursor para modo direto (sem backbuffer)
struct CursorState {
    x: usize,
    y: usize,
    saved_bg: [u32; CURSOR_W * CURSOR_H],
    valid: bool,
}

static mut CURSOR: CursorState = CursorState {
    x: 0,
    y: 0,
    saved_bg: [0; CURSOR_W * CURSOR_H],
    valid: false,
};

// ── Estruturas de Janela e Compositor ──────────────────────────────────────────

#[derive(Clone, Copy)]
pub struct Window {
    pub id: usize,
    pub title: &'static str,
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub is_open: bool,
    pub is_dragging: bool,
    pub drag_offset_x: usize,
    pub drag_offset_y: usize,
    pub lines: [&'static str; 6],
    pub line_count: usize,
}

const MAX_WINDOWS: usize = 3;

struct WindowManager {
    windows: [Option<Window>; MAX_WINDOWS],
    order: [usize; MAX_WINDOWS], // Z-order: 0 = fundo, 2 = topo
    prev_buttons: u8,
}

static mut WM: WindowManager = WindowManager {
    windows: [None, None, None],
    order: [0, 1, 2],
    prev_buttons: 0,
};

// ── Inicialização ─────────────────────────────────────────────────────────────

pub fn init() {
    unsafe {
        if let Some(fb) = limine::framebuffer() {
            let info = FramebufferInfo {
                address: fb.address,
                width: fb.width,
                height: fb.height,
                pitch: fb.pitch,
                bpp: fb.bpp,
                red_mask_shift: fb.red_mask_shift,
                green_mask_shift: fb.green_mask_shift,
                blue_mask_shift: fb.blue_mask_shift,
            };
            *core::ptr::addr_of_mut!(FB_INFO) = Some(info);

            // Aloca o Backbuffer contíguo no heap do kernel (3 MiB para 1024x768x4)
            let total_bytes = (fb.width * fb.height * 4) as usize;
            let layout = core::alloc::Layout::from_size_align(total_bytes, 16)
                .expect("Falha no layout do backbuffer");
            let buf_ptr = alloc::alloc::alloc_zeroed(layout) as *mut u32;
            if !buf_ptr.is_null() {
                *core::ptr::addr_of_mut!(BACKBUFFER) = buf_ptr;
            }

            // Inicializa janelas padrão
            init_default_windows();
        }
    }
}

fn init_default_windows() {
    unsafe {
        let wm = &mut *core::ptr::addr_of_mut!(WM);

        // Janela 0: Monitor de Sistema
        wm.windows[0] = Some(Window {
            id: 0,
            title: "Jinn OS - Monitor de Sistema",
            x: 60,
            y: 60,
            width: 380,
            height: 220,
            is_open: true,
            is_dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
            lines: [
                "Kernel: Jinn Microkernel v0.0.3",
                "Arquitetura: x86_64 Bare-Metal",
                "Temporizador: Local APIC (100 Hz)",
                "Paginacao: PML4 de 4 Niveis Ativa",
                "Isolamento: Ring 0 Kernel / Ring 3 Userspace",
                "Status: Operando em Alta Preempcao",
            ],
            line_count: 6,
        });

        // Janela 1: Terminal / Console
        wm.windows[1] = Some(Window {
            id: 1,
            title: "Terminal Interativo (Ring 3 Ready)",
            x: 480,
            y: 100,
            width: 440,
            height: 220,
            is_open: true,
            is_dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
            lines: [
                "jinn@kernel:~# ps",
                "PID 0: idle[K]        - Estado: Ready",
                "PID 1: shell[K]       - Estado: Running",
                "PID 2: ola-mundo[U3]  - Estado: Pronto (Ring 3)",
                "jinn@kernel:~# spawn ola-mundo",
                "[OK] Processo isolado carregado com sucesso!",
            ],
            line_count: 6,
        });

        // Janela 2: Sobre o Jinn OS
        wm.windows[2] = Some(Window {
            id: 2,
            title: "Sobre o Jinn OS",
            x: 240,
            y: 330,
            width: 420,
            height: 200,
            is_open: true,
            is_dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
            lines: [
                "Jinn OS - Sistema Operacional em Rust",
                "Foco: Leveza, responsividade e preempcao.",
                "Zero abstracoes pesadas no escalonador.",
                "Comunidade: github.com/GeovaneEstanislau/Jinn_Project",
                "Enalteca a tecnologia brasileira!",
                "Pressione [ESC] para retornar ao Shell texto.",
            ],
            line_count: 6,
        });
    }
}

// ── Funções de Primitivas no Backbuffer (RAM) ──────────────────────────────────

#[inline]
pub fn draw_pixel_bb(x: usize, y: usize, color: u32) {
    unsafe {
        let bb = *core::ptr::addr_of!(BACKBUFFER);
        if !bb.is_null() && x < DEFAULT_WIDTH && y < DEFAULT_HEIGHT {
            core::ptr::write_volatile(bb.add(y * DEFAULT_WIDTH + x), color);
        }
    }
}

pub fn fill_rect_bb(x: usize, y: usize, width: usize, height: usize, color: u32) {
    let end_x = (x + width).min(DEFAULT_WIDTH);
    let end_y = (y + height).min(DEFAULT_HEIGHT);

    for cy in y..end_y {
        for cx in x..end_x {
            draw_pixel_bb(cx, cy, color);
        }
    }
}

pub fn draw_char_bb(ch: u8, x: usize, y: usize, fg: u32, bg: Option<u32>) {
    let glyph = font::get_glyph(ch);
    for (y_offset, row_byte) in glyph.iter().enumerate() {
        let cy = y + y_offset;
        if cy >= DEFAULT_HEIGHT { break; }
        for x_offset in 0..8 {
            let cx = x + x_offset;
            if cx >= DEFAULT_WIDTH { break; }

            if (row_byte & (1 << (7 - x_offset))) != 0 {
                draw_pixel_bb(cx, cy, fg);
            } else if let Some(bg_color) = bg {
                draw_pixel_bb(cx, cy, bg_color);
            }
        }
    }
}

pub fn draw_string_bb(text: &str, x: usize, y: usize, fg: u32, bg: Option<u32>) {
    let mut cx = x;
    for byte in text.bytes() {
        let ch = match byte {
            0x20..=0x7e => byte,
            _ => 0xfe,
        };
        draw_char_bb(ch, cx, y, fg, bg);
        cx += 8;
    }
}

// ── Cópia Rápida Backbuffer → VRAM (Swap Buffers) ─────────────────────────────

pub fn swap_buffers() {
    unsafe {
        let info_ptr = core::ptr::addr_of!(FB_INFO);
        let fb = match *info_ptr {
            Some(info) => info,
            None => return,
        };
        let bb = *core::ptr::addr_of!(BACKBUFFER);
        if bb.is_null() { return; }

        let width = fb.width as usize;
        let height = fb.height as usize;
        let pitch = fb.pitch as usize;
        let bytes_per_row = width * 4;

        if pitch == bytes_per_row {
            core::ptr::copy_nonoverlapping(
                bb as *const u8,
                fb.address,
                width * height * 4,
            );
        } else {
            for y in 0..height {
                let src = (bb as *const u8).add(y * bytes_per_row);
                let dst = fb.address.add(y * pitch);
                core::ptr::copy_nonoverlapping(src, dst, bytes_per_row);
            }
        }
    }
}

// ── Renderização Completa do Compositor ────────────────────────────────────────

pub fn render_desktop(mouse_x: usize, mouse_y: usize) {
    // 1. Limpa o fundo do Desktop
    fill_rect_bb(0, 0, DEFAULT_WIDTH, DEFAULT_HEIGHT, COLOR_DESKTOP);

    // 2. Renderiza as janelas em Z-order (da do fundo para a do topo)
    unsafe {
        let wm = &*core::ptr::addr_of!(WM);
        let top_idx = wm.order[MAX_WINDOWS - 1];

        for &win_idx in wm.order.iter() {
            if let Some(win) = wm.windows[win_idx] {
                if !win.is_open { continue; }

                let is_active = win_idx == top_idx;
                let title_color = if is_active { COLOR_WIN_TITLE_ACT } else { COLOR_WIN_TITLE_INA };

                // Moldura externa (sombra/borda preta de 1px)
                fill_rect_bb(win.x.saturating_sub(1), win.y.saturating_sub(1), win.width + 2, win.height + 2, 0x000F172A);

                // Barra de título
                fill_rect_bb(win.x, win.y, win.width, 24, title_color);

                // Título da janela
                draw_string_bb(win.title, win.x + 8, win.y + 4, COLOR_WHITE, Some(title_color));

                // Botão de fechar [X]
                let btn_close_x = win.x + win.width - 22;
                let btn_close_y = win.y + 3;
                fill_rect_bb(btn_close_x, btn_close_y, 18, 18, COLOR_BTN_CLOSE);
                draw_char_bb(b'X', btn_close_x + 5, btn_close_y + 1, COLOR_WHITE, Some(COLOR_BTN_CLOSE));

                // Corpo da janela
                fill_rect_bb(win.x, win.y + 24, win.width, win.height.saturating_sub(24), COLOR_WIN_BG);

                // Linhas de texto do conteúdo
                for i in 0..win.line_count {
                    let line_y = win.y + 36 + (i * 18);
                    draw_string_bb(win.lines[i], win.x + 12, line_y, COLOR_TEXT_DARK, Some(COLOR_WIN_BG));
                }
            }
        }
    }

    // 3. Barra de Tarefas (Taskbar) na parte inferior
    let taskbar_y = DEFAULT_HEIGHT - 32;
    fill_rect_bb(0, taskbar_y, DEFAULT_WIDTH, 32, COLOR_TASKBAR);

    // Botão Iniciar [ Jinn OS ]
    fill_rect_bb(6, taskbar_y + 4, 100, 24, COLOR_START_BTN);
    draw_string_bb(" Jinn OS", 12, taskbar_y + 8, COLOR_WHITE, Some(COLOR_START_BTN));

    // Relógio / Info no canto direito da barra de tarefas
    draw_string_bb("1024x768 | Mouse PS/2 Ativo | Modo Grafico", DEFAULT_WIDTH - 380, taskbar_y + 8, 0x0094_A3B8, Some(COLOR_TASKBAR));

    // 4. Renderiza o cursor do mouse no topo de tudo
    for (cy, line) in CURSOR_BITMAP.iter().enumerate() {
        for (cx, ch) in line.bytes().enumerate() {
            let px = mouse_x + cx;
            let py = mouse_y + cy;
            match ch {
                b'#' => draw_pixel_bb(px, py, COLOR_BLACK),
                b'*' => draw_pixel_bb(px, py, COLOR_WHITE),
                _ => {} // Transparente
            }
        }
    }

    // 5. Envia o frame final da RAM para a memória de vídeo (VRAM)
    swap_buffers();
}

// ── Gerenciador de Eventos de Mouse ───────────────────────────────────────────

pub fn handle_mouse(mouse_x: usize, mouse_y: usize, buttons: u8) {
    unsafe {
        let wm = &mut *core::ptr::addr_of_mut!(WM);
        let left_clicked = (buttons & 1) != 0;
        let prev_left_clicked = (wm.prev_buttons & 1) != 0;
        wm.prev_buttons = buttons;

        // Se soltou o botão esquerdo, encerra qualquer arrasto ativo
        if !left_clicked {
            for win_opt in wm.windows.iter_mut() {
                if let Some(win) = win_opt {
                    win.is_dragging = false;
                }
            }
        }

        // Se está arrastando alguma janela, atualiza suas coordenadas
        let mut dragging_any = false;
        for win_opt in wm.windows.iter_mut() {
            if let Some(win) = win_opt {
                if win.is_dragging {
                    let new_x = mouse_x.saturating_sub(win.drag_offset_x);
                    let new_y = mouse_y.saturating_sub(win.drag_offset_y);
                    win.x = new_x.min(DEFAULT_WIDTH.saturating_sub(win.width));
                    win.y = new_y.min((DEFAULT_HEIGHT - 32).saturating_sub(win.height));
                    dragging_any = true;
                }
            }
        }

        // Se acabou de clicar com o botão esquerdo (transição de 0 para 1)
        if left_clicked && !prev_left_clicked && !dragging_any {
            // Percorre as janelas da frente para trás (ordem inversa de Z-order)
            let mut clicked_win_idx = None;
            for i in (0..MAX_WINDOWS).rev() {
                let win_idx = wm.order[i];
                if let Some(win) = wm.windows[win_idx] {
                    if !win.is_open { continue; }

                    // Verifica clique na janela
                    if mouse_x >= win.x && mouse_x < win.x + win.width &&
                       mouse_y >= win.y && mouse_y < win.y + win.height {
                        
                        // Verifica se clicou no botão de Fechar [X]
                        let btn_x = win.x + win.width - 22;
                        let btn_y = win.y + 3;
                        if mouse_x >= btn_x && mouse_x < btn_x + 18 &&
                           mouse_y >= btn_y && mouse_y < btn_y + 18 {
                            if let Some(w) = wm.windows[win_idx].as_mut() {
                                w.is_open = false;
                            }
                            return;
                        }

                        // Se clicou na barra de título (y até y + 24), inicia arrasto
                        if mouse_y < win.y + 24 {
                            if let Some(w) = wm.windows[win_idx].as_mut() {
                                w.is_dragging = true;
                                w.drag_offset_x = mouse_x - win.x;
                                w.drag_offset_y = mouse_y - win.y;
                            }
                        }

                        clicked_win_idx = Some(win_idx);
                        break;
                    }
                }
            }

            // Se clicou em uma janela, traz ela para o topo do Z-order
            if let Some(idx) = clicked_win_idx {
                bring_to_front(idx);
            }
        }
    }
}

fn bring_to_front(target_idx: usize) {
    unsafe {
        let wm = &mut *core::ptr::addr_of_mut!(WM);
        let mut current_pos = 0;
        for (i, &idx) in wm.order.iter().enumerate() {
            if idx == target_idx {
                current_pos = i;
                break;
            }
        }

        // Move a janela para o final do array (topo do Z-order)
        for i in current_pos..(MAX_WINDOWS - 1) {
            wm.order[i] = wm.order[i + 1];
        }
        wm.order[MAX_WINDOWS - 1] = target_idx;
    }
}

// ── Loop Principal do Ambiente Gráfico ────────────────────────────────────────

pub fn run_desktop() {
    loop {
        // 1. Verifica se o usuário pressionou ESC (0x1B) para sair de volta para o Shell
        if let Some(ch) = crate::keyboard_buffer::pop() {
            if ch == 0x1B {
                break;
            }
        }

        // Lê porta PS/2 caso a interrupção não tenha pego
        unsafe {
            let status: u8;
            core::arch::asm!("in al, dx", out("al") status, in("dx") 0x64u16, options(nomem, nostack));
            if (status & 1) != 0 && (status & 0x20) == 0 {
                let sc: u8;
                core::arch::asm!("in al, dx", out("al") sc, in("dx") 0x60u16, options(nomem, nostack));
                if sc == 0x01 { // Scancode ESC
                    break;
                }
            }
        }

        // 2. Obtém coordenadas e botões atuais do mouse
        let (mx, my, buttons) = crate::interrupts::ps2_mouse::get_state();

        // 3. Processa interação e arrasto de janelas
        handle_mouse(mx, my, buttons);

        // 4. Renderiza a cena completa no Backbuffer e faz Swap para a tela
        render_desktop(mx, my);

        // 5. Cede uma fatia curta de CPU para que outras threads e timers rodem
        core::hint::spin_loop();
    }

    // Ao sair, limpa a tela para restaurar o console de texto
    let mut writer = crate::vga::Writer::new();
    writer.clear_screen();
}

// ── Funções de Compatibilidade Legadas ────────────────────────────────────────

pub fn draw_pixel(x: usize, y: usize, color: u32) {
    unsafe {
        let info_ptr = core::ptr::addr_of!(FB_INFO);
        if let Some(fb) = *info_ptr {
            if x < fb.width as usize && y < fb.height as usize {
                let pixel_offset = y * (fb.pitch as usize) + x * ((fb.bpp as usize) / 8);
                core::ptr::write_volatile(fb.address.add(pixel_offset) as *mut u32, color);
            }
        }
    }
}

pub fn read_pixel(x: usize, y: usize) -> u32 {
    unsafe {
        let info_ptr = core::ptr::addr_of!(FB_INFO);
        if let Some(fb) = *info_ptr {
            if x < fb.width as usize && y < fb.height as usize {
                let pixel_offset = y * (fb.pitch as usize) + x * ((fb.bpp as usize) / 8);
                return core::ptr::read_volatile(fb.address.add(pixel_offset) as *const u32);
            }
        }
    }
    0
}

pub fn fill_rect(x: usize, y: usize, width: usize, height: usize, color: Color) {
    unsafe {
        if let Some(fb) = limine::framebuffer() {
            let color_u32 = color_to_u32_fb(color, fb);
            let end_x = (x + width).min(fb.width as usize);
            let end_y = (y + height).min(fb.height as usize);
            for cy in y..end_y {
                for cx in x..end_x {
                    let pixel_offset = cy * (fb.pitch as usize) + cx * ((fb.bpp as usize) / 8);
                    core::ptr::write_volatile(fb.address.add(pixel_offset) as *mut u32, color_u32);
                }
            }
        }
    }
}

pub fn draw_string_at(text: &str, x: usize, y: usize, fg: Color, bg: Option<Color>) {
    let mut cx = x;
    for byte in text.bytes() {
        let ch = match byte {
            0x20..=0x7e => byte,
            _ => 0xfe,
        };
        draw_char_at(ch, cx, y, fg, bg);
        cx += 8;
    }
}

pub fn draw_char_at(ch: u8, x: usize, y: usize, fg: Color, bg: Option<Color>) {
    unsafe {
        if let Some(fb) = limine::framebuffer() {
            let glyph = font::get_glyph(ch);
            let fg_u32 = color_to_u32_fb(fg, fb);
            let bg_u32 = bg.map(|c| color_to_u32_fb(c, fb));
            
            for (y_offset, row_byte) in glyph.iter().enumerate() {
                let cy = y + y_offset;
                if cy >= fb.height as usize { break; }
                for x_offset in 0..8 {
                    let cx = x + x_offset;
                    if cx >= fb.width as usize { break; }

                    if (row_byte & (1 << (7 - x_offset))) != 0 {
                        let pixel_offset = cy * (fb.pitch as usize) + cx * ((fb.bpp as usize) / 8);
                        core::ptr::write_volatile(fb.address.add(pixel_offset) as *mut u32, fg_u32);
                    } else if let Some(bg_color) = bg_u32 {
                        let pixel_offset = cy * (fb.pitch as usize) + cx * ((fb.bpp as usize) / 8);
                        core::ptr::write_volatile(fb.address.add(pixel_offset) as *mut u32, bg_color);
                    }
                }
            }
        }
    }
}

pub fn draw_window(x: usize, y: usize, width: usize, height: usize, title: &str) {
    fill_rect(x, y, width, height, Color::LightGray);
    fill_rect(x, y, width, 20, Color::Blue);
    draw_string_at(title, x + 5, y + 2, Color::White, Some(Color::Blue));
    fill_rect(x + width - 20, y + 2, 16, 16, Color::LightRed);
    draw_string_at("X", x + width - 16, y + 2, Color::White, Some(Color::LightRed));
}

pub fn update_cursor(new_x: usize, new_y: usize) {
    unsafe {
        let cursor = &mut *core::ptr::addr_of_mut!(CURSOR);
        let info_ptr = core::ptr::addr_of!(FB_INFO);
        let fb = match *info_ptr {
            Some(info) => info,
            None => return,
        };

        if cursor.valid {
            for cy in 0..CURSOR_H {
                for cx in 0..CURSOR_W {
                    let px = cursor.x + cx;
                    let py = cursor.y + cy;
                    if px < fb.width as usize && py < fb.height as usize {
                        let bg_color = cursor.saved_bg[cy * CURSOR_W + cx];
                        draw_pixel(px, py, bg_color);
                    }
                }
            }
        }

        cursor.x = new_x;
        cursor.y = new_y;
        for cy in 0..CURSOR_H {
            for cx in 0..CURSOR_W {
                let px = new_x + cx;
                let py = new_y + cy;
                cursor.saved_bg[cy * CURSOR_W + cx] = read_pixel(px, py);
            }
        }
        cursor.valid = true;

        for (cy, line) in CURSOR_BITMAP.iter().enumerate() {
            for (cx, ch) in line.bytes().enumerate() {
                let px = new_x + cx;
                let py = new_y + cy;
                if px < fb.width as usize && py < fb.height as usize {
                    match ch {
                        b'#' => draw_pixel(px, py, 0x00000000),
                        b'*' => draw_pixel(px, py, 0x00FFFFFF),
                        _ => {}
                    }
                }
            }
        }
    }
}
