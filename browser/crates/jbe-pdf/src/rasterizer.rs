//! Pure native vector rasterizer rendering PDF draw commands into 32-bit RGBA surfaces.

extern crate alloc;
use alloc::vec;
use alloc::vec::Vec;
use crate::document::PdfPage;
use crate::interpreter::{DrawCommand, PdfInterpreter};
use jbe_core::Color;

/// 32-bit RGBA pixel surface representing a rasterized PDF page.
#[derive(Debug, Clone)]
pub struct PdfSurface {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub pixels: Vec<u32>, // 0xAARRGGBB format
}

impl PdfSurface {
    pub fn new(width: u32, height: u32, background: Color) -> Self {
        let size = (width as usize) * (height as usize);
        let bg_u32 = background.to_u32();
        Self {
            width,
            height,
            stride: width,
            pixels: vec![bg_u32; size],
        }
    }

    #[inline(always)]
    pub fn set_pixel(&mut self, x: i32, y: i32, color: Color) {
        if x >= 0 && x < self.width as i32 && y >= 0 && y < self.height as i32 {
            let idx = (y as usize) * (self.stride as usize) + (x as usize);
            if color.a == 255 {
                self.pixels[idx] = color.to_u32();
            } else if color.a > 0 {
                // Alpha blend
                let existing = self.pixels[idx];
                let ex_r = (existing >> 16) & 0xFF;
                let ex_g = (existing >> 8) & 0xFF;
                let ex_b = existing & 0xFF;

                let alpha = color.a as u32;
                let inv_a = 255 - alpha;

                let r = (color.r as u32 * alpha + ex_r * inv_a) / 255;
                let g = (color.g as u32 * alpha + ex_g * inv_a) / 255;
                let b = (color.b as u32 * alpha + ex_b * inv_a) / 255;

                self.pixels[idx] = 0xFF000000 | (r << 16) | (g << 8) | b;
            }
        }
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Color) {
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(self.width as i32);
        let y1 = (y + h).min(self.height as i32);

        for cy in y0..y1 {
            for cx in x0..x1 {
                self.set_pixel(cx, cy, color);
            }
        }
    }

    pub fn draw_line(&mut self, mut x0: i32, mut y0: i32, x1: i32, y1: i32, color: Color, width: f32) {
        let half_w = (width / 2.0).ceil() as i32;
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            if half_w <= 1 {
                self.set_pixel(x0, y0, color);
            } else {
                for ox in -half_w..=half_w {
                    for oy in -half_w..=half_w {
                        if ox * ox + oy * oy <= half_w * half_w {
                            self.set_pixel(x0 + ox, y0 + oy, color);
                        }
                    }
                }
            }

            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    /// Renders text glyphs using built-in 8x16 font glyph patterns.
    pub fn draw_text(&mut self, x0: i32, y0: i32, text: &str, font_size: f32, color: Color) {
        let scale = (font_size / 16.0).max(0.5);
        let mut cur_x = x0 as f32;

        for ch in text.chars() {
            let glyph = Self::get_glyph_bitmap(ch);
            for row in 0..16 {
                let bits = glyph[row];
                for col in 0..8 {
                    if (bits & (0x80 >> col)) != 0 {
                        // Draw scaled pixel box
                        let px = cur_x + (col as f32) * scale;
                        let py = y0 as f32 + (row as f32) * scale;
                        let w = scale.ceil() as i32;
                        let h = scale.ceil() as i32;
                        self.fill_rect(px as i32, py as i32, w, h, color);
                    }
                }
            }
            cur_x += 8.0 * scale;
        }
    }

    fn get_glyph_bitmap(c: char) -> [u8; 16] {
        match c {
            'A' => [
                0x00, 0x18, 0x24, 0x42, 0x42, 0x42, 0x7E, 0x42, 0x42, 0x42, 0x42, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            'B' => [
                0x00, 0x7C, 0x42, 0x42, 0x42, 0x7C, 0x42, 0x42, 0x42, 0x7C, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            'C' => [
                0x00, 0x3C, 0x42, 0x40, 0x40, 0x40, 0x40, 0x40, 0x42, 0x3C, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            'D' => [
                0x00, 0x78, 0x44, 0x42, 0x42, 0x42, 0x42, 0x42, 0x44, 0x78, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            'E' => [
                0x00, 0x7E, 0x40, 0x40, 0x40, 0x7C, 0x40, 0x40, 0x40, 0x7E, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            'F' => [
                0x00, 0x7E, 0x40, 0x40, 0x40, 0x7C, 0x40, 0x40, 0x40, 0x40, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            'G' => [
                0x00, 0x3C, 0x42, 0x40, 0x40, 0x4E, 0x42, 0x42, 0x42, 0x3C, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            'H' => [
                0x00, 0x42, 0x42, 0x42, 0x42, 0x7E, 0x42, 0x42, 0x42, 0x42, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            'I' => [
                0x00, 0x3C, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            'J' => [
                0x00, 0x1E, 0x06, 0x06, 0x06, 0x06, 0x06, 0x46, 0x46, 0x3C, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            'K' => [
                0x00, 0x42, 0x44, 0x48, 0x70, 0x70, 0x48, 0x44, 0x42, 0x42, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            'L' => [
                0x00, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x7E, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            'M' => [
                0x00, 0x63, 0x77, 0x5B, 0x49, 0x41, 0x41, 0x41, 0x41, 0x41, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            'N' => [
                0x00, 0x42, 0x62, 0x52, 0x4A, 0x46, 0x42, 0x42, 0x42, 0x42, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            'O' => [
                0x00, 0x3C, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x3C, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            'P' => [
                0x00, 0x7C, 0x42, 0x42, 0x42, 0x7C, 0x40, 0x40, 0x40, 0x40, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            'Q' => [
                0x00, 0x3C, 0x42, 0x42, 0x42, 0x42, 0x42, 0x4A, 0x44, 0x3A, 0x01, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            'R' => [
                0x00, 0x7C, 0x42, 0x42, 0x42, 0x7C, 0x48, 0x44, 0x42, 0x42, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            'S' => [
                0x00, 0x3C, 0x42, 0x40, 0x38, 0x04, 0x02, 0x42, 0x42, 0x3C, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            'T' => [
                0x00, 0x7E, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            'U' => [
                0x00, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x3C, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            'V' => [
                0x00, 0x42, 0x42, 0x42, 0x42, 0x42, 0x24, 0x24, 0x18, 0x18, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            'W' => [
                0x00, 0x41, 0x41, 0x41, 0x41, 0x49, 0x5B, 0x77, 0x63, 0x41, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            'X' => [
                0x00, 0x42, 0x42, 0x24, 0x18, 0x18, 0x24, 0x42, 0x42, 0x42, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            'Y' => [
                0x00, 0x42, 0x42, 0x24, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            'Z' => [
                0x00, 0x7E, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x40, 0x7E, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            '0'..='9' => {
                let digit = c as u8 - b'0';
                match digit {
                    0 => [
                        0x00, 0x3C, 0x42, 0x46, 0x4A, 0x52, 0x62, 0x42, 0x42, 0x3C, 0x00, 0x00,
                        0x00, 0x00, 0x00, 0x00,
                    ],
                    1 => [
                        0x00, 0x18, 0x28, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x3E, 0x00, 0x00,
                        0x00, 0x00, 0x00, 0x00,
                    ],
                    2 => [
                        0x00, 0x3C, 0x42, 0x02, 0x04, 0x18, 0x20, 0x40, 0x40, 0x7E, 0x00, 0x00,
                        0x00, 0x00, 0x00, 0x00,
                    ],
                    3 => [
                        0x00, 0x3C, 0x42, 0x02, 0x1C, 0x02, 0x02, 0x02, 0x42, 0x3C, 0x00, 0x00,
                        0x00, 0x00, 0x00, 0x00,
                    ],
                    4 => [
                        0x00, 0x08, 0x18, 0x28, 0x48, 0x7E, 0x08, 0x08, 0x08, 0x08, 0x00, 0x00,
                        0x00, 0x00, 0x00, 0x00,
                    ],
                    5 => [
                        0x00, 0x7E, 0x40, 0x7C, 0x02, 0x02, 0x02, 0x02, 0x42, 0x3C, 0x00, 0x00,
                        0x00, 0x00, 0x00, 0x00,
                    ],
                    6 => [
                        0x00, 0x3C, 0x40, 0x40, 0x7C, 0x42, 0x42, 0x42, 0x42, 0x3C, 0x00, 0x00,
                        0x00, 0x00, 0x00, 0x00,
                    ],
                    7 => [
                        0x00, 0x7E, 0x02, 0x04, 0x08, 0x10, 0x10, 0x20, 0x20, 0x20, 0x00, 0x00,
                        0x00, 0x00, 0x00, 0x00,
                    ],
                    8 => [
                        0x00, 0x3C, 0x42, 0x42, 0x3C, 0x42, 0x42, 0x42, 0x42, 0x3C, 0x00, 0x00,
                        0x00, 0x00, 0x00, 0x00,
                    ],
                    _ => [
                        0x00, 0x3C, 0x42, 0x42, 0x42, 0x3E, 0x02, 0x02, 0x42, 0x3C, 0x00, 0x00,
                        0x00, 0x00, 0x00, 0x00,
                    ],
                }
            }
            ':' => [
                0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            '.' => [
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            '-' => [
                0x00, 0x00, 0x00, 0x00, 0x00, 0x7E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
            ' ' => [0u8; 16],
            _ => {
                // Lowercase letters mapped to uppercase bitmap or standard square box
                if c.is_ascii_lowercase() {
                    Self::get_glyph_bitmap(c.to_ascii_uppercase())
                } else {
                    [
                        0x00, 0x7E, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x7E, 0x00, 0x00,
                        0x00, 0x00, 0x00, 0x00,
                    ]
                }
            }
        }
    }
}

pub struct PdfRasterizer;

impl PdfRasterizer {
    /// Rasterizes a PDF page into a 32-bit pixel surface buffer.
    pub fn render_page(page: &PdfPage, target_scale: f32) -> PdfSurface {
        let scale = target_scale.max(0.1);
        let width = ((page.media_box.width * scale).ceil() as u32).max(1);
        let height = ((page.media_box.height * scale).ceil() as u32).max(1);

        let mut surface = PdfSurface::new(width, height, Color::rgba(255, 255, 255, 255)); // White page

        let mut interpreter = PdfInterpreter::new();
        for stream in &page.content_streams {
            interpreter.interpret_stream(stream);
        }

        let media_x = page.media_box.x;
        let media_y = page.media_box.y;
        let media_h = page.media_box.height;

        for cmd in interpreter.commands {
            match cmd {
                DrawCommand::FillRect { x, y, w, h, color } => {
                    let sx = ((x - media_x) * scale) as i32;
                    let sy = ((media_y + media_h - (y + h)) * scale) as i32;
                    let sw = (w * scale).ceil() as i32;
                    let sh = (h * scale).ceil() as i32;
                    surface.fill_rect(sx, sy, sw, sh, color);
                }
                DrawCommand::StrokeRect {
                    x,
                    y,
                    w,
                    h,
                    color,
                    line_width,
                } => {
                    let sx = ((x - media_x) * scale) as i32;
                    let sy = ((media_y + media_h - (y + h)) * scale) as i32;
                    let sw = (w * scale).ceil() as i32;
                    let sh = (h * scale).ceil() as i32;
                    let lw = (line_width * scale).max(1.0);

                    surface.draw_line(sx, sy, sx + sw, sy, color, lw);
                    surface.draw_line(sx + sw, sy, sx + sw, sy + sh, color, lw);
                    surface.draw_line(sx + sw, sy + sh, sx, sy + sh, color, lw);
                    surface.draw_line(sx, sy + sh, sx, sy, color, lw);
                }
                DrawCommand::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    color,
                    line_width,
                } => {
                    let sx1 = ((x1 - media_x) * scale) as i32;
                    let sy1 = ((media_y + media_h - y1) * scale) as i32;
                    let sx2 = ((x2 - media_x) * scale) as i32;
                    let sy2 = ((media_y + media_h - y2) * scale) as i32;
                    let lw = (line_width * scale).max(1.0);
                    surface.draw_line(sx1, sy1, sx2, sy2, color, lw);
                }
                DrawCommand::Text {
                    x,
                    y,
                    text,
                    font_size,
                    color,
                } => {
                    let sx = ((x - media_x) * scale) as i32;
                    // PDF text baseline is y, glyph top is y + font_size
                    let sy = ((media_y + media_h - (y + font_size)) * scale) as i32;
                    surface.draw_text(sx, sy, &text, font_size * scale, color);
                }
            }
        }

        surface
    }
}
