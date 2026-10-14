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
        }
    }
}

#[inline]
pub fn draw_pixel(x: usize, y: usize, color: u32) {
    unsafe {
        let info_ptr = core::ptr::addr_of!(FB_INFO);
        if let Some(fb) = *info_ptr {
            if x < fb.width as usize && y < fb.height as usize {
                let pixel_offset = y * (fb.pitch as usize) + x * ((fb.bpp as usize) / 8);
                core::ptr::write_volatile(
                    fb.address.add(pixel_offset) as *mut u32,
                    color,
                );
            }
        }
    }
}

#[inline]
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
            
            let end_x = core::cmp::min(x + width, fb.width as usize);
            let end_y = core::cmp::min(y + height, fb.height as usize);
            
            for cy in y..end_y {
                for cx in x..end_x {
                    let pixel_offset = cy * (fb.pitch as usize) + cx * ((fb.bpp as usize) / 8);
                    core::ptr::write_volatile(
                        fb.address.add(pixel_offset) as *mut u32,
                        color_u32,
                    );
                }
            }
        }
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
                if cy >= fb.height as usize {
                    break;
                }
                for x_offset in 0..8 {
                    let cx = x + x_offset;
                    if cx >= fb.width as usize {
                        break;
                    }

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

pub fn draw_window(x: usize, y: usize, width: usize, height: usize, title: &str) {
    // Fundo da janela (Cinza Claro)
    fill_rect(x, y, width, height, Color::LightGray);
    
    // Barra de título (Azul Escuro)
    fill_rect(x, y, width, 20, Color::Blue);
    
    // Título da janela (Branco na barra de título)
    draw_string_at(title, x + 5, y + 2, Color::White, Some(Color::Blue));
    
    // Botão de Fechar [X] (Fundo Vermelho Claro)
    fill_rect(x + width - 20, y + 2, 16, 16, Color::LightRed);
    draw_string_at("X", x + width - 16, y + 2, Color::White, Some(Color::LightRed));
}

/// Atualiza a posição do cursor do mouse na tela.
/// Restaura os pixels anteriores do fundo e renderiza o sprite do cursor na nova posição.
pub fn update_cursor(new_x: usize, new_y: usize) {
    unsafe {
        let cursor = &mut *core::ptr::addr_of_mut!(CURSOR);
        let info_ptr = core::ptr::addr_of!(FB_INFO);
        let fb = match *info_ptr {
            Some(info) => info,
            None => return,
        };

        // 1. Restaura o fundo anterior onde o cursor estava
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

        // 2. Salva o novo fundo que ficará embaixo do cursor
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

        // 3. Renderiza o cursor por cima
        for (cy, line) in CURSOR_BITMAP.iter().enumerate() {
            for (cx, ch) in line.bytes().enumerate() {
                let px = new_x + cx;
                let py = new_y + cy;
                if px < fb.width as usize && py < fb.height as usize {
                    match ch {
                        b'#' => draw_pixel(px, py, 0x00000000), // Borda preta
                        b'*' => draw_pixel(px, py, 0x00FFFFFF), // Corpo branco
                        _ => {} // Transparente: mantém o fundo intacto
                    }
                }
            }
        }
    }
}
