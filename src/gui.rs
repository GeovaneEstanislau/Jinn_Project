use crate::limine::{self, LimineFramebuffer};
use crate::vga::{Color, color_to_u32_fb};
use crate::font;

static mut FRAMEBUFFER: Option<&'static mut LimineFramebuffer> = None;

pub fn init() {
    unsafe {
        FRAMEBUFFER = limine::framebuffer();
    }
}

pub fn draw_pixel(x: usize, y: usize, color: u32) {
    unsafe {
        if let Some(fb) = &mut FRAMEBUFFER {
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

pub fn fill_rect(x: usize, y: usize, width: usize, height: usize, color: Color) {
    unsafe {
        if let Some(fb) = &mut FRAMEBUFFER {
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
        if let Some(fb) = &mut FRAMEBUFFER {
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
        cx += 8; // A largura da nossa fonte é 8 pixels
    }
}

pub fn draw_window(x: usize, y: usize, width: usize, height: usize, title: &str) {
    // Fundo da janela (Cinza Claro)
    fill_rect(x, y, width, height, Color::LightGray);
    
    // Barra de título (Azul Escuro)
    fill_rect(x, y, width, 20, Color::Blue);
    
    // Título da janela (Branco, com fundo transparente na barra de título)
    draw_string_at(title, x + 5, y + 2, Color::White, Some(Color::Blue));
    
    // Botão de Fechar [X] (Fundo Vermelho Claro)
    fill_rect(x + width - 20, y + 2, 16, 16, Color::LightRed);
    draw_string_at("X", x + width - 16, y + 2, Color::White, Some(Color::LightRed));
}
