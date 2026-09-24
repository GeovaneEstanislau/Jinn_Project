#![allow(dead_code)]

use core::fmt;

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct ColorCode(u8);

impl ColorCode {
    pub const fn new(foreground: Color, background: Color) -> Self {
        ColorCode((background as u8) << 4 | (foreground as u8))
    }
}

fn color_to_u32_fb(color: Color, fb: &crate::limine::LimineFramebuffer) -> u32 {
    let (r, g, b) = match color {
        Color::Black => (0x00, 0x00, 0x00),
        Color::Blue => (0x00, 0x00, 0xAA),
        Color::Green => (0x00, 0xAA, 0x00),
        Color::Cyan => (0x00, 0xAA, 0xAA),
        Color::Red => (0xAA, 0x00, 0x00),
        Color::Magenta => (0xAA, 0x00, 0xAA),
        Color::Brown => (0xAA, 0x55, 0x00),
        Color::LightGray => (0xAA, 0xAA, 0xAA),
        Color::DarkGray => (0x55, 0x55, 0x55),
        Color::LightBlue => (0x55, 0x55, 0xFF),
        Color::LightGreen => (0x55, 0xFF, 0x55),
        Color::LightCyan => (0x55, 0xFF, 0xFF),
        Color::LightRed => (0xFF, 0x55, 0x55),
        Color::Pink => (0xFF, 0x55, 0xFF),
        Color::Yellow => (0xFF, 0xFF, 0x55),
        Color::White => (0xFF, 0xFF, 0xFF),
    };
    ((r as u32) << fb.red_mask_shift) |
    ((g as u32) << fb.green_mask_shift) |
    ((b as u32) << fb.blue_mask_shift)
}

pub struct Writer {
    column_position: usize,
    row_position: usize,
    fg_color: Color,
    bg_color: Color,
    fb: Option<&'static mut crate::limine::LimineFramebuffer>,
}

impl Writer {
    pub fn new() -> Writer {
        Writer {
            column_position: 0,
            row_position: 0,
            fg_color: Color::LightGreen,
            bg_color: Color::Black,
            fb: crate::limine::framebuffer(),
        }
    }

    fn max_cols(&self) -> usize {
        self.fb.as_ref().map(|fb| (fb.width as usize) / 8).unwrap_or(80)
    }

    fn max_rows(&self) -> usize {
        self.fb.as_ref().map(|fb| (fb.height as usize) / 8).unwrap_or(25)
    }

    pub fn set_color(&mut self, fg: Color, bg: Color) {
        self.fg_color = fg;
        self.bg_color = bg;
    }

    pub fn clear_screen(&mut self) {
        let rows = self.max_rows();
        for row in 0..rows {
            self.clear_row(row);
        }
        self.column_position = 0;
        self.row_position = 0;
    }

    pub fn write_line(&mut self, s: &str) {
        self.write_string(s);
        self.write_byte(b'\n');
    }

    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                0x20..=0x7e | b'\n' => self.write_byte(byte),
                _ => self.write_byte(0xfe),
            }
        }
    }

    pub fn write_colored(&mut self, s: &str, fg: Color, bg: Color) {
        let prev_fg = self.fg_color;
        let prev_bg = self.bg_color;
        self.set_color(fg, bg);
        self.write_string(s);
        self.set_color(prev_fg, prev_bg);
    }

    pub fn write_decimal(&mut self, mut value: usize) {
        if value == 0 {
            self.write_byte(b'0');
            return;
        }
        let mut buf = [0u8; 20];
        let mut i = 0;
        while value > 0 {
            buf[i] = b'0' + ((value % 10) as u8);
            value /= 10;
            i += 1;
        }
        while i > 0 {
            i -= 1;
            self.write_byte(buf[i]);
        }
    }

    pub fn write_hex_u64(&mut self, val: u64) {
        self.write_string("0x");
        for i in (0..16).rev() {
            let nibble = ((val >> (i * 4)) & 0xf) as u8;
            let ch = if nibble < 10 {
                b'0' + nibble
            } else {
                b'a' + (nibble - 10)
            };
            self.write_byte(ch);
        }
    }

    pub fn write_hex_u32(&mut self, val: u32) {
        self.write_string("0x");
        for i in (0..8).rev() {
            let nibble = ((val >> (i * 4)) & 0xf) as u8;
            let ch = if nibble < 10 {
                b'0' + nibble
            } else {
                b'a' + (nibble - 10)
            };
            self.write_byte(ch);
        }
    }

    pub fn write_hex_u16(&mut self, val: u16) {
        self.write_string("0x");
        for i in (0..4).rev() {
            let nibble = ((val >> (i * 4)) & 0xf) as u8;
            let ch = if nibble < 10 {
                b'0' + nibble
            } else {
                b'a' + (nibble - 10)
            };
            self.write_byte(ch);
        }
    }

    pub fn write_hex_u8(&mut self, val: u8) {
        self.write_string("0x");
        for i in (0..2).rev() {
            let nibble = ((val >> (i * 4)) & 0xf) as u8;
            let ch = if nibble < 10 {
                b'0' + nibble
            } else {
                b'a' + (nibble - 10)
            };
            self.write_byte(ch);
        }
    }

    pub fn write_memory_size(&mut self, bytes: u64) {
        const GB: u64 = 1024 * 1024 * 1024;
        const MB: u64 = 1024 * 1024;
        const KB: u64 = 1024;

        if bytes >= GB {
            self.write_decimal((bytes / GB) as usize);
            let rem = (bytes % GB) / (GB / 10);
            if rem > 0 {
                self.write_string(".");
                self.write_decimal(rem as usize);
            }
            self.write_string(" GB");
        } else if bytes >= MB {
            self.write_decimal((bytes / MB) as usize);
            let rem = (bytes % MB) / (MB / 10);
            if rem > 0 {
                self.write_string(".");
                self.write_decimal(rem as usize);
            }
            self.write_string(" MB");
        } else if bytes >= KB {
            self.write_decimal((bytes / KB) as usize);
            self.write_string(" KB");
        } else {
            self.write_decimal(bytes as usize);
            self.write_string(" B");
        }
    }

    pub fn write_feature_tag(&mut self, name: &str, enabled: bool) {
        if enabled {
            self.write_colored("[", Color::DarkGray, Color::Black);
            self.write_colored(name, Color::LightCyan, Color::Black);
            self.write_colored("] ", Color::DarkGray, Color::Black);
        }
    }

    pub fn write_at(&mut self, row: usize, col: usize, s: &str, pad_width: usize) {
        let max_rows = self.max_rows();
        let max_cols = self.max_cols();
        let row = row.min(max_rows.saturating_sub(1));
        let mut c = col;
        for byte in s.bytes() {
            if c >= max_cols {
                break;
            }
            let ch = match byte {
                0x20..=0x7e => byte,
                _ => 0xfe,
            };
            self.draw_char(ch, c, row);
            c += 1;
        }
        let end = (col + pad_width).min(max_cols);
        while c < end {
            self.draw_char(b' ', c, row);
            c += 1;
        }
    }

    pub fn write_decimal_at(&mut self, row: usize, col: usize, mut value: usize, pad_width: usize) {
        let max_rows = self.max_rows();
        let max_cols = self.max_cols();
        let row = row.min(max_rows.saturating_sub(1));
        let mut buf = [0u8; 20];
        let mut i = 0;
        if value == 0 {
            buf[0] = b'0';
            i = 1;
        } else {
            while value > 0 {
                buf[i] = b'0' + (value % 10) as u8;
                value /= 10;
                i += 1;
            }
            buf[..i].reverse();
        }
        let mut c = col;
        for &byte in &buf[..i] {
            if c >= max_cols {
                break;
            }
            self.draw_char(byte, c, row);
            c += 1;
        }
        let end = (col + pad_width).min(max_cols);
        while c < end {
            self.draw_char(b' ', c, row);
            c += 1;
        }
    }

    fn draw_char(&mut self, ch: u8, col: usize, row: usize) {
        if let Some(fb) = self.fb.as_mut() {
            let glyph = crate::font::get_glyph(ch);
            let fg = color_to_u32_fb(self.fg_color, fb);
            let bg = color_to_u32_fb(self.bg_color, fb);

            let x_base = col * 8;
            let y_base = row * 8;

            for (y_offset, row_byte) in glyph.iter().enumerate() {
                let y = y_base + y_offset;
                if y >= fb.height as usize {
                    break;
                }
                for x_offset in 0..8 {
                    let x = x_base + x_offset;
                    if x >= fb.width as usize {
                        break;
                    }

                    let color = if (row_byte & (1 << (7 - x_offset))) != 0 {
                        fg
                    } else {
                        bg
                    };

                    let pixel_offset = y * (fb.pitch as usize) + x * ((fb.bpp as usize) / 8);
                    unsafe {
                        core::ptr::write_volatile(
                            fb.address.add(pixel_offset) as *mut u32,
                            color,
                        );
                    }
                }
            }
        } else {
            // ── VGA text mode fallback via HHDM ──────────────────────────
            // Limine BIOS maps all physical memory at hhdm_offset, so the
            // VGA text buffer (physical 0xB8000) is at hhdm_offset+0xB8000.
            let col = col.min(79);
            let row = row.min(24);
            let hhdm = crate::limine::hhdm_offset();
            let vga = (hhdm + 0xB8000) as *mut u16;
            let attr = ((self.bg_color as u16) << 12) | ((self.fg_color as u16) << 8);
            let entry = attr | (ch as u16);
            unsafe {
                core::ptr::write_volatile(vga.add(row * 80 + col), entry);
            }
        }
    }

    pub fn write_byte(&mut self, byte: u8) {
        unsafe {
            core::arch::asm!("out dx, al", in("dx") 0x3f8u16, in("al") byte, options(nomem, nostack, preserves_flags));
        }
        match byte {
            b'\n' => self.new_line(),
            byte => {
                let max_cols = self.max_cols();
                if self.column_position >= max_cols {
                    self.new_line();
                }
                let row = self.row_position;
                let col = self.column_position;
                self.draw_char(byte, col, row);
                self.column_position += 1;
            }
        }
    }

    /// Erase the last typed character (backspace).
    pub fn backspace(&mut self) {
        if self.column_position > 0 {
            self.column_position -= 1;
            let row = self.row_position;
            let col = self.column_position;
            // Overwrite with background-coloured space
            let saved_fg = self.fg_color;
            let saved_bg = self.bg_color;
            self.set_color(saved_fg, Color::Black);
            self.draw_char(b' ', col, row);
            self.fg_color = saved_fg;
            self.bg_color = saved_bg;
        }
    }

    fn new_line(&mut self) {
        let max_rows = self.max_rows();
        if self.row_position < max_rows - 1 {
            self.row_position += 1;
        } else {
            if let Some(fb) = self.fb.as_mut() {
                let pitch = fb.pitch as usize;
                let bytes_to_copy = ((fb.height as usize) - 8) * pitch;

                unsafe {
                    core::ptr::copy(
                        fb.address.add(8 * pitch),
                        fb.address,
                        bytes_to_copy,
                    );
                }
            }
            self.clear_row(max_rows - 1);
        }
        self.column_position = 0;
    }

    fn clear_row(&mut self, row: usize) {
        if let Some(fb) = self.fb.as_mut() {
            let bg = color_to_u32_fb(self.bg_color, fb);
            let y_base = row * 8;
            for y_offset in 0..8 {
                let y = y_base + y_offset;
                if y >= fb.height as usize {
                    break;
                }
                for x in 0..(fb.width as usize) {
                    let pixel_offset = y * (fb.pitch as usize) + x * ((fb.bpp as usize) / 8);
                    unsafe {
                        core::ptr::write_volatile(
                            fb.address.add(pixel_offset) as *mut u32,
                            bg,
                        );
                    }
                }
            }
        } else {
            // VGA text mode fallback
            let row = row.min(24);
            let hhdm = crate::limine::hhdm_offset();
            let vga = (hhdm + 0xB8000) as *mut u16;
            let attr = ((self.bg_color as u16) << 12) | ((self.fg_color as u16) << 8);
            let blank = attr | 0x20; // space character
            unsafe {
                for col in 0..80usize {
                    core::ptr::write_volatile(vga.add(row * 80 + col), blank);
                }
            }
        }
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}
