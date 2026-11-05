//! Aladdin Desktop SDK — Primitivas de Desenho e Gerenciamento de Buffers

/// Representação de Cor em formato ARGB / XRGB (32 bits)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(pub u32);

impl Color {
    pub const BLACK:      Color = Color(0x0000_0000);
    pub const WHITE:      Color = Color(0x00FF_FFFF);
    pub const RED:        Color = Color(0x00FF_3B30);
    pub const GREEN:      Color = Color(0x0034_C759);
    pub const BLUE:       Color = Color(0x0000_7AFF);
    pub const CYAN:       Color = Color(0x0032_ADE6);
    pub const MAGENTA:    Color = Color(0x00AF_52DE);
    pub const YELLOW:     Color = Color(0x00FF_CC00);
    pub const ORANGE:     Color = Color(0x00FF_9500);
    pub const DARK_GRAY:  Color = Color(0x001F_2937);
    pub const MID_GRAY:   Color = Color(0x0037_4151);
    pub const LIGHT_GRAY: Color = Color(0x009C_A3AF);
    pub const CARD_BG:    Color = Color(0x000F_172A);
    pub const TEAL:       Color = Color(0x0000_D2B4);

    #[inline(always)]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color(((r as u32) << 16) | ((g as u32) << 8) | (b as u32))
    }

    #[inline(always)]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color(((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32))
    }
}

/// Canvas 2D encapsulando um buffer linear de pixels `u32`
pub struct Canvas<'a> {
    pub buffer: &'a mut [u32],
    pub width: usize,
    pub height: usize,
}

impl<'a> Canvas<'a> {
    pub fn new(buffer: &'a mut [u32], width: usize, height: usize) -> Option<Self> {
        let expected = width.checked_mul(height)?;
        if buffer.len() < expected {
            None
        } else {
            Some(Self { buffer, width, height })
        }
    }

    #[inline(always)]
    pub fn clear(&mut self, color: Color) {
        self.buffer.fill(color.0);
    }

    #[inline(always)]
    pub fn set_pixel(&mut self, x: usize, y: usize, color: Color) {
        if x < self.width && y < self.height {
            self.buffer[y * self.width + x] = color.0;
        }
    }

    pub fn fill_rect(&mut self, rx: usize, ry: usize, rw: usize, rh: usize, color: Color) {
        let x_end = (rx + rw).min(self.width);
        let y_end = (ry + rh).min(self.height);
        for y in ry..y_end {
            let offset = y * self.width;
            for x in rx..x_end {
                self.buffer[offset + x] = color.0;
            }
        }
    }

    pub fn draw_rect(&mut self, rx: usize, ry: usize, rw: usize, rh: usize, color: Color) {
        if rw == 0 || rh == 0 { return; }
        let x2 = rx + rw - 1;
        let y2 = ry + rh - 1;
        for x in rx..=x2.min(self.width.saturating_sub(1)) {
            self.set_pixel(x, ry, color);
            self.set_pixel(x, y2, color);
        }
        for y in ry..=y2.min(self.height.saturating_sub(1)) {
            self.set_pixel(rx, y, color);
            self.set_pixel(x2, y, color);
        }
    }

    pub fn draw_line(&mut self, x0: isize, y0: isize, x1: isize, y1: isize, color: Color) {
        let mut x0 = x0;
        let mut y0 = y0;
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            if x0 >= 0 && y0 >= 0 && (x0 as usize) < self.width && (y0 as usize) < self.height {
                self.set_pixel(x0 as usize, y0 as usize, color);
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

    pub fn draw_gradient(&mut self, c_top: Color, c_bottom: Color) {
        if self.height == 0 { return; }
        let r1 = ((c_top.0 >> 16) & 0xFF) as i32;
        let g1 = ((c_top.0 >> 8) & 0xFF) as i32;
        let b1 = (c_top.0 & 0xFF) as i32;

        let r2 = ((c_bottom.0 >> 16) & 0xFF) as i32;
        let g2 = ((c_bottom.0 >> 8) & 0xFF) as i32;
        let b2 = (c_bottom.0 & 0xFF) as i32;

        for y in 0..self.height {
            let t = y as i32;
            let h = self.height as i32;
            let r = (r1 + (r2 - r1) * t / h) as u32;
            let g = (g1 + (g2 - g1) * t / h) as u32;
            let b = (b1 + (b2 - b1) * t / h) as u32;
            let color = (r << 16) | (g << 8) | b;
            let row_start = y * self.width;
            let row_end = row_start + self.width;
            self.buffer[row_start..row_end].fill(color);
        }
    }
}
