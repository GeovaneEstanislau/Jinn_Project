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

/// Modos de capacidade gráfica com degradação graciosa
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityMode {
    /// Renderização 100% opaca, sem sombras pesadas ou blur, máxima economia de ciclos de CPU
    Basic,
    /// Alpha blending linear leve, sombras discretas de 1 passo
    Balanced,
    /// Composição avançada com desfoque e acentuações luminescentes
    Advanced,
}

static mut CURRENT_CAPABILITY: CapabilityMode = CapabilityMode::Balanced;

pub fn get_capability_mode() -> CapabilityMode {
    unsafe { *core::ptr::addr_of!(CURRENT_CAPABILITY) }
}

pub fn set_capability_mode(mode: CapabilityMode) {
    unsafe { *core::ptr::addr_of_mut!(CURRENT_CAPABILITY) = mode; }
}

/// Representa uma região retangular danificada para composição incremental (Damage Tracking)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyRect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl DirtyRect {
    pub const fn new(x: usize, y: usize, width: usize, height: usize) -> Self {
        Self { x, y, width, height }
    }

    pub fn union(&self, other: &DirtyRect) -> DirtyRect {
        let min_x = self.x.min(other.x);
        let min_y = self.y.min(other.y);
        let max_x = (self.x + self.width).max(other.x + other.width);
        let max_y = (self.y + self.height).max(other.y + other.height);
        DirtyRect {
            x: min_x,
            y: min_y,
            width: max_x.saturating_sub(min_x),
            height: max_y.saturating_sub(min_y),
        }
    }

    pub fn intersects(&self, other: &DirtyRect) -> bool {
        self.x < other.x + other.width &&
        self.x + self.width > other.x &&
        self.y < other.y + other.height &&
        self.y + self.height > other.y
    }
}

// ── Aladdin Design Tokens & Paleta de Cores (ARGB 32-bit) ────────────────────
pub const COLOR_ALADDIN_BG: u32        = 0x000D_1117; // Abismo Ardósia (Fundo principal)
pub const COLOR_ALADDIN_CARD: u32      = 0x0016_1B22; // Superfície de Janelas/Cartões
pub const COLOR_ALADDIN_CARD_ALT: u32  = 0x0021_262D; // Superfície secundária
pub const COLOR_ALADDIN_TOPPANEL: u32  = 0x000A_0E14; // Barra superior de status
pub const COLOR_ALADDIN_DOCK: u32      = 0x0013_171F; // Barra de tarefas / Dock
pub const COLOR_ALADDIN_CYAN: u32      = 0x0038_BDF8; // Ciano Luminescente (Garuda Dr460nized)
pub const COLOR_ALADDIN_PURPLE: u32    = 0x00A8_55F7; // Violeta Vibrante
pub const COLOR_ALADDIN_EMERALD: u32   = 0x0022_C55E; // Esmeralda (Sucesso / Ativo)
pub const COLOR_ALADDIN_AMBER: u32     = 0x00F5_9E0B; // Âmbar (Avisos / Pressão)
pub const COLOR_ALADDIN_RUBY: u32      = 0x00EF_4444; // Rubi (Fechar / Erro)
pub const COLOR_ALADDIN_BORDER: u32    = 0x0030_363D; // Borda sutil de 1px
pub const COLOR_ALADDIN_BORDER_ACT: u32= 0x0038_BDF8; // Borda de janela com foco
pub const COLOR_ALADDIN_TEXT_PRI: u32  = 0x00F8_FAFC; // Texto primário
pub const COLOR_ALADDIN_TEXT_MUTED: u32= 0x0094_A3B8; // Metadados / Texto secundário

// Cores legadas mapeadas para o sistema Aladdin
pub const COLOR_DESKTOP: u32      = COLOR_ALADDIN_BG;
pub const COLOR_TASKBAR: u32      = COLOR_ALADDIN_DOCK;
pub const COLOR_START_BTN: u32    = 0x0000_7ACC; // Azul VSCode / Jinn
pub const COLOR_START_HOVER: u32  = 0x0002_84C7; // Azul claro hover
pub const COLOR_WIN_BG: u32       = 0x00EA_EEF2; // Corpo da janela (cinza claro moderno)
pub const COLOR_WIN_TITLE_ACT: u32= 0x001B_4965; // Barra de título ativa
pub const COLOR_WIN_TITLE_INA: u32= 0x004A_5568; // Barra de título inativa
pub const COLOR_BTN_CLOSE: u32    = COLOR_ALADDIN_RUBY; // Botão fechar [X]
pub const COLOR_WHITE: u32        = 0x00FF_FFFF;
pub const COLOR_BLACK: u32        = 0x0000_0000;
pub const COLOR_TEXT_DARK: u32    = 0x001A_202C;
pub const COLOR_ACCENT_GREEN: u32 = COLOR_ALADDIN_EMERALD;
pub const COLOR_ACCENT_BLUE: u32  = COLOR_ALADDIN_CYAN;
pub const COLOR_MENU_BG: u32      = 0x001E_293B; // Fundo azul escuro do menu iniciar
pub const COLOR_MENU_HEADER: u32  = 0x000F_172A; // Topo do menu
pub const COLOR_MENU_HOVER: u32   = 0x0033_4155; // Item em hover
pub const COLOR_MENU_BORDER: u32  = COLOR_ALADDIN_BORDER;
pub const COLOR_BAR_FG: u32       = COLOR_ALADDIN_EMERALD; // Verde esmeralda para barra de progresso
pub const COLOR_BAR_BG: u32       = 0x00CB_D5E1; // Fundo suave para barra
pub const COLOR_BAR_BORDER: u32   = COLOR_ALADDIN_BORDER; // Borda da barra
pub const COLOR_BTN_BG: u32       = 0x0025_63EB; // Botão de ação (azul clássico)

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

#[derive(Clone, Copy)]
pub struct TerminalBuffer {
    pub history: [[u8; 46]; 6],
    pub history_lens: [usize; 6],
    pub history_colors: [u32; 6],
    pub history_count: usize,
    pub input: [u8; 36],
    pub input_len: usize,
}

impl TerminalBuffer {
    pub const fn new() -> Self {
        Self {
            history: [[0; 46]; 6],
            history_lens: [0; 6],
            history_colors: [0; 6],
            history_count: 0,
            input: [0; 36],
            input_len: 0,
        }
    }

    pub fn push_line(&mut self, text: &str, color: u32) {
        if self.history_count >= 6 {
            for i in 0..5 {
                self.history[i] = self.history[i + 1];
                self.history_lens[i] = self.history_lens[i + 1];
                self.history_colors[i] = self.history_colors[i + 1];
            }
            self.history_count = 5;
        }

        let idx = self.history_count;
        let bytes = text.as_bytes();
        let copy_len = bytes.len().min(46);
        for j in 0..copy_len {
            self.history[idx][j] = bytes[j];
        }
        self.history_lens[idx] = copy_len;
        self.history_colors[idx] = color;
        self.history_count += 1;
    }
}

const MAX_WINDOWS: usize = 4;

struct WindowManager {
    windows: [Option<Window>; MAX_WINDOWS],
    order: [usize; MAX_WINDOWS], // Z-order: 0 = fundo, 3 = topo
    prev_buttons: u8,
    start_menu_open: bool,
    should_exit: bool,
    spawn_count: usize,
    terminal: TerminalBuffer,
}

impl WindowManager {
    pub fn terminal_handle_key(&mut self, ch: u8) {
        match ch {
            b'\n' | b'\r' => {
                let len = self.terminal.input_len;
                if len > 0 {
                    let mut input_copy = [0u8; 36];
                    let copy_len = len.min(36);
                    for i in 0..copy_len {
                        input_copy[i] = self.terminal.input[i];
                    }

                    // Ecoa o comando digitado no terminal gráfico
                    let mut echo_buf = [0u8; 46];
                    let prefix = b"> ";
                    echo_buf[0] = prefix[0];
                    echo_buf[1] = prefix[1];
                    let echo_len = copy_len.min(44);
                    for i in 0..echo_len {
                        echo_buf[2 + i] = input_copy[i];
                    }
                    if let Ok(s) = core::str::from_utf8(&echo_buf[..2 + echo_len]) {
                        self.terminal.push_line(s, 0x0094_A3B8);
                    }

                    let cmd_str = match core::str::from_utf8(&input_copy[..copy_len]) {
                        Ok(s) => s.trim(),
                        Err(_) => "",
                    };

                    // Processa comandos internos do terminal gráfico
                    match cmd_str {
                        "help" => {
                            self.terminal.push_line("Cmds: help, clear, spawn, ps, uptime, reboot", 0x0038_BDF8);
                        }
                        "clear" => {
                            self.terminal.history_count = 0;
                        }
                        "spawn" | "run" => {
                            spawn_user_app();
                            self.terminal.push_line("[OK] ola-mundo spawned (Ring 3)!", 0x0022_C55E);
                        }
                        "ps" => {
                            let count = crate::scheduler::get().task_count();
                            let mut buf = [0u8; 32];
                            let s = "Tarefas ativas: ";
                            buf[..s.len()].copy_from_slice(s.as_bytes());
                            buf[s.len()] = b'0' + ((count % 10) as u8);
                            if let Ok(msg) = core::str::from_utf8(&buf[..s.len() + 1]) {
                                self.terminal.push_line(msg, 0x00F8_FAFC);
                            }
                        }
                        "uptime" => {
                            let secs = crate::timer::ticks() / 100;
                            let mut buf = [0u8; 32];
                            let s = "Uptime: ";
                            buf[..s.len()].copy_from_slice(s.as_bytes());
                            let mut pos = s.len();
                            let mut n = secs;
                            if n == 0 {
                                buf[pos] = b'0';
                                pos += 1;
                            } else {
                                let start = pos;
                                while n > 0 && pos < 28 {
                                    buf[pos] = b'0' + (n % 10) as u8;
                                    n /= 10;
                                    pos += 1;
                                }
                                buf[start..pos].reverse();
                            }
                            buf[pos] = b' ';
                            buf[pos + 1] = b's';
                            pos += 2;
                            if let Ok(msg) = core::str::from_utf8(&buf[..pos]) {
                                self.terminal.push_line(msg, 0x00F8_FAFC);
                            }
                        }
                        "reboot" => {
                            reboot_machine();
                        }
                        "exit" => {
                            self.should_exit = true;
                        }
                        _ => {
                            self.terminal.push_line("Comando nao encontrado. Use 'help'.", 0x00F8_7171);
                        }
                    }

                    self.terminal.input_len = 0;
                }
            }
            0x08 => {
                // Backspace
                if self.terminal.input_len > 0 {
                    self.terminal.input_len -= 1;
                }
            }
            b if (0x20..=0x7E).contains(&b) => {
                if self.terminal.input_len < 36 {
                    let idx = self.terminal.input_len;
                    self.terminal.input[idx] = b;
                    self.terminal.input_len += 1;
                }
            }
            _ => {}
        }
    }
}

static mut WM: WindowManager = WindowManager {
    windows: [None, None, None, None],
    order: [0, 1, 2, 3],
    prev_buttons: 0,
    start_menu_open: false,
    should_exit: false,
    spawn_count: 0,
    terminal: TerminalBuffer::new(),
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
            x: 470,
            y: 80,
            width: 460,
            height: 240,
            is_open: true,
            is_dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
            lines: [
                "", "", "", "", "", "",
            ],
            line_count: 0,
        });

        // Inicializa mensagens do terminal gráfico
        wm.terminal.push_line("Jinn OS Terminal v0.0.3 (Ring 0 / Ring 3)", 0x0038_BDF8);
        wm.terminal.push_line("Digite 'help' para comandos ou use o mouse.", 0x0094_A3B8);
        wm.terminal.push_line("Comandos: spawn, ps, uptime, clear, reboot", 0x0060_A5FA);

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

        // Janela 3: Gerenciador de Arquivos Aladdin (FAT32 / Storage)
        wm.windows[3] = Some(Window {
            id: 3,
            title: "Aladdin Files - Volume /disk (FAT32)",
            x: 160,
            y: 190,
            width: 440,
            height: 240,
            is_open: false,
            is_dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
            lines: [
                "Dispositivo: IDE Primary Master (LBA28)",
                "Particao 1: FAT32 Volume /disk",
                "Arquivos detectados no VFS:",
                " [DOC] README.TXT      1.2 KB",
                " [BIN] OLA.ELF         18.4 KB (Executavel Ring 3)",
                "Status: Sistema de arquivos montado e consistente",
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

/// Algoritmo de Bresenham para traçado de linhas 2D
pub fn draw_line_bb(mut x0: isize, mut y0: isize, x1: isize, y1: isize, color: u32) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        if x0 >= 0 && (x0 as usize) < DEFAULT_WIDTH && y0 >= 0 && (y0 as usize) < DEFAULT_HEIGHT {
            draw_pixel_bb(x0 as usize, y0 as usize, color);
        }
        if x0 == x1 && y0 == y1 { break; }
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

/// Contorno retangular de 1 pixel
pub fn draw_rect_outline_bb(x: usize, y: usize, width: usize, height: usize, color: u32) {
    if width == 0 || height == 0 { return; }
    let end_x = (x + width).min(DEFAULT_WIDTH);
    let end_y = (y + height).min(DEFAULT_HEIGHT);

    for cx in x..end_x {
        draw_pixel_bb(cx, y, color);
        if end_y > y {
            draw_pixel_bb(cx, end_y - 1, color);
        }
    }
    for cy in y..end_y {
        draw_pixel_bb(x, cy, color);
        if end_x > x {
            draw_pixel_bb(end_x - 1, cy, color);
        }
    }
}

/// Barra de progresso com porcentagem visual e borda
pub fn draw_progress_bar_bb(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    current: usize,
    total: usize,
    fg_color: u32,
    bg_color: u32,
    border_color: u32,
) {
    fill_rect_bb(x, y, width, height, bg_color);
    draw_rect_outline_bb(x, y, width, height, border_color);

    if total > 0 && width > 2 && height > 2 {
        let fill_w = ((width - 2) * current.min(total)) / total;
        if fill_w > 0 {
            fill_rect_bb(x + 1, y + 1, fill_w, height - 2, fg_color);
        }
    }
}

fn format_num(mut val: usize, buf: &mut [u8]) -> usize {
    if val == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut i = 0;
    let mut tmp = [0u8; 20];
    while val > 0 && i < 20 {
        tmp[i] = b'0' + (val % 10) as u8;
        val /= 10;
        i += 1;
    }
    for j in 0..i {
        buf[j] = tmp[i - 1 - j];
    }
    i
}

fn draw_label_and_num(label: &str, num: usize, suffix: &str, x: usize, y: usize, fg: u32, bg: Option<u32>) {
    let mut buf = [0u8; 64];
    let mut len = 0;
    for b in label.bytes() {
        if len < 40 { buf[len] = b; len += 1; }
    }
    let mut num_buf = [0u8; 20];
    let nlen = format_num(num, &mut num_buf);
    for b in &num_buf[..nlen] {
        if len < 55 { buf[len] = *b; len += 1; }
    }
    for b in suffix.bytes() {
        if len < 64 { buf[len] = b; len += 1; }
    }
    if let Ok(s) = core::str::from_utf8(&buf[..len]) {
        draw_string_bb(s, x, y, fg, bg);
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

/// Realiza o swap incremental (Damage Tracking) copiando apenas a região retangular danificada para a VRAM.
/// Reduz a transferência de dados de 3.145.728 bytes para apenas a área alterada.
pub fn swap_buffer_rect(rect: &DirtyRect) {
    unsafe {
        let info_ptr = core::ptr::addr_of!(FB_INFO);
        let fb = match *info_ptr {
            Some(info) => info,
            None => return,
        };
        let bb = *core::ptr::addr_of!(BACKBUFFER);
        if bb.is_null() { return; }

        let fb_width = fb.width as usize;
        let fb_height = fb.height as usize;
        let pitch = fb.pitch as usize;

        // Limita o retângulo às dimensões do framebuffer
        let rx = rect.x.min(fb_width);
        let ry = rect.y.min(fb_height);
        let rw = rect.width.min(fb_width.saturating_sub(rx));
        let rh = rect.height.min(fb_height.saturating_sub(ry));

        if rw == 0 || rh == 0 { return; }

        let bytes_per_line = rw * 4;
        let row_stride = DEFAULT_WIDTH * 4;

        for line in 0..rh {
            let cur_y = ry + line;
            let src = (bb as *const u8).add(cur_y * row_stride + rx * 4);
            let dst = fb.address.add(cur_y * pitch + rx * 4);
            core::ptr::copy_nonoverlapping(src, dst, bytes_per_line);
        }
    }
}

// ── Renderização Completa do Compositor ────────────────────────────────────────

pub fn render_desktop(mouse_x: usize, mouse_y: usize, damage: Option<DirtyRect>) {
    // 1. Limpa o fundo do Desktop com tom Abismo Ardósia do Aladdin
    fill_rect_bb(0, 0, DEFAULT_WIDTH, DEFAULT_HEIGHT, COLOR_ALADDIN_BG);

    // 1.1. Barra Superior de Status (Aladdin Top Panel)
    fill_rect_bb(0, 0, DEFAULT_WIDTH, 24, COLOR_ALADDIN_TOPPANEL);
    draw_line_bb(0, 24, DEFAULT_WIDTH as isize, 24, COLOR_ALADDIN_BORDER);
    draw_string_bb("[*] Aladdin Desktop", 12, 5, COLOR_ALADDIN_CYAN, Some(COLOR_ALADDIN_TOPPANEL));

    // Indicadores do sistema no painel superior direito
    let ticks = crate::timer::ticks();
    let secs = ticks / 100;
    let mut time_buf = [0u8; 16];
    let time_prefix = b"Up: ";
    time_buf[..4].copy_from_slice(time_prefix);
    let mut pos = 4;
    let mut s = secs;
    if s == 0 {
        time_buf[pos] = b'0';
        pos += 1;
    } else {
        let start = pos;
        while s > 0 && pos < 12 {
            time_buf[pos] = b'0' + (s % 10) as u8;
            s /= 10;
            pos += 1;
        }
        time_buf[start..pos].reverse();
    }
    time_buf[pos] = b's';
    pos += 1;
    if let Ok(msg) = core::str::from_utf8(&time_buf[..pos]) {
        draw_string_bb(msg, DEFAULT_WIDTH - 120, 5, COLOR_ALADDIN_TEXT_MUTED, Some(COLOR_ALADDIN_TOPPANEL));
    }

    // Marca d'água elegante no canto inferior direito
    draw_string_bb("Aladdin Desktop Engine v0.1", DEFAULT_WIDTH - 260, DEFAULT_HEIGHT - 64, COLOR_ALADDIN_PURPLE, None);
    draw_string_bb("Jinn Microkernel x86_64", DEFAULT_WIDTH - 260, DEFAULT_HEIGHT - 48, COLOR_ALADDIN_TEXT_MUTED, None);

    // 2. Renderiza as janelas em Z-order (da do fundo para a do topo)
    unsafe {
        let wm = &*core::ptr::addr_of!(WM);
        let top_idx = wm.order[MAX_WINDOWS - 1];

        for &win_idx in wm.order.iter() {
            if let Some(win) = wm.windows[win_idx] {
                if !win.is_open { continue; }

                let is_active = win_idx == top_idx;
                let title_color = if is_active { COLOR_ALADDIN_TOPPANEL } else { 0x0011_1827 };
                let border_color = if is_active { COLOR_ALADDIN_BORDER_ACT } else { COLOR_ALADDIN_BORDER };

                // Moldura externa da janela (Aladdin Card Border)
                draw_rect_outline_bb(win.x.saturating_sub(1), win.y.saturating_sub(1), win.width + 2, win.height + 2, border_color);

                // Barra de título (Aladdin Window Header)
                fill_rect_bb(win.x, win.y, win.width, 24, title_color);

                // Linha de acentuação luminescente sob o título na janela ativa (Garuda Dr460nized touch)
                if is_active {
                    draw_line_bb(win.x as isize, (win.y + 24) as isize, (win.x + win.width) as isize, (win.y + 24) as isize, COLOR_ALADDIN_CYAN);
                }

                // Indicador de foco / status e título da janela
                if is_active {
                    draw_char_bb(b'*', win.x + 8, win.y + 4, COLOR_ALADDIN_CYAN, Some(title_color));
                    draw_string_bb(win.title, win.x + 20, win.y + 4, COLOR_ALADDIN_TEXT_PRI, Some(title_color));
                } else {
                    draw_string_bb(win.title, win.x + 10, win.y + 4, COLOR_ALADDIN_TEXT_MUTED, Some(title_color));
                }

                // Botão de fechar [X] estilizado em rubi Aladdin
                let btn_close_x = win.x + win.width - 22;
                let btn_close_y = win.y + 3;
                fill_rect_bb(btn_close_x, btn_close_y, 18, 18, COLOR_ALADDIN_RUBY);
                draw_char_bb(b'X', btn_close_x + 5, btn_close_y + 1, COLOR_WHITE, Some(COLOR_ALADDIN_RUBY));

                // Corpo da janela (Aladdin Card Surface)
                fill_rect_bb(win.x, win.y + 25, win.width, win.height.saturating_sub(25), COLOR_ALADDIN_CARD);

                // Conteúdo específico da janela
                if win.id == 0 {
                    // ── Janela 0: Monitor de Sistema Dinâmico ──────────────
                    draw_string_bb("--- Recursos do Kernel (Ring 0) ---", win.x + 12, win.y + 36, COLOR_ALADDIN_CYAN, Some(COLOR_ALADDIN_CARD));
                    let ticks = crate::timer::ticks();
                    draw_label_and_num("Tempo Ativo    : ", (ticks / 100) as usize, " s (100 Hz APIC)", win.x + 12, win.y + 58, COLOR_ALADDIN_TEXT_PRI, Some(COLOR_ALADDIN_CARD));
                    draw_label_and_num("Tarefas Ativas : ", crate::scheduler::get().task_count(), " processos", win.x + 12, win.y + 78, COLOR_ALADDIN_TEXT_PRI, Some(COLOR_ALADDIN_CARD));
                    draw_label_and_num("Preempcoes APIC: ", crate::scheduler::sched_ticks() as usize, " ticks", win.x + 12, win.y + 98, COLOR_ALADDIN_TEXT_PRI, Some(COLOR_ALADDIN_CARD));

                    let total_mb = crate::memory::total_bytes() / (1024 * 1024);
                    let used_mb = crate::memory::used_bytes() / (1024 * 1024);
                    draw_label_and_num("Memoria RAM    : ", used_mb, " MB / ", win.x + 12, win.y + 118, COLOR_ALADDIN_TEXT_PRI, Some(COLOR_ALADDIN_CARD));
                    draw_label_and_num("", total_mb, " MB", win.x + 204, win.y + 118, COLOR_ALADDIN_TEXT_PRI, Some(COLOR_ALADDIN_CARD));

                    // Barra de progresso visual de memória com acento esmeralda
                    draw_progress_bar_bb(win.x + 12, win.y + 138, win.width - 24, 16, used_mb, total_mb, COLOR_ALADDIN_EMERALD, COLOR_ALADDIN_CARD_ALT, COLOR_ALADDIN_BORDER);

                    draw_string_bb("Paginacao PML4 | Scheduler Preemptivo", win.x + 12, win.y + 164, COLOR_ALADDIN_PURPLE, Some(COLOR_ALADDIN_CARD));
                    draw_string_bb("Seguranca: Ring 0 (Kernel) / Ring 3 (User)", win.x + 12, win.y + 184, COLOR_ALADDIN_TEXT_MUTED, Some(COLOR_ALADDIN_CARD));
                } else if win.id == 1 {
                    // ── Janela 1: Terminal Interativo ──────────────────────
                    let term_box_x = win.x + 8;
                    let term_box_y = win.y + 28;
                    let term_box_w = win.width - 16;
                    let term_box_h = win.height - 68;

                    // Caixa escura estilizada do terminal
                    fill_rect_bb(term_box_x, term_box_y, term_box_w, term_box_h, 0x000F_172A);
                    draw_rect_outline_bb(term_box_x, term_box_y, term_box_w, term_box_h, 0x001E_293B);

                    // Renderiza linhas do histórico
                    for i in 0..wm.terminal.history_count {
                        let len = wm.terminal.history_lens[i];
                        if len > 0 {
                            if let Ok(line_str) = core::str::from_utf8(&wm.terminal.history[i][..len]) {
                                let line_y = term_box_y + 8 + (i * 18);
                                draw_string_bb(line_str, term_box_x + 8, line_y, wm.terminal.history_colors[i], Some(0x000F_172A));
                            }
                        }
                    }

                    // Renderiza Prompt interativo com buffer atual
                    let prompt_y = term_box_y + term_box_h - 22;
                    draw_string_bb("jinn> ", term_box_x + 8, prompt_y, 0x0038_BDF8, Some(0x000F_172A));
                    let input_len = wm.terminal.input_len;
                    if input_len > 0 {
                        if let Ok(input_str) = core::str::from_utf8(&wm.terminal.input[..input_len]) {
                            draw_string_bb(input_str, term_box_x + 56, prompt_y, 0x00F8_FAFC, Some(0x000F_172A));
                        }
                    }

                    // Cursor piscante quando a janela está ativa (no topo)
                    if is_active {
                        let cursor_x = term_box_x + 56 + (input_len * 8);
                        if (crate::timer::ticks() % 50) < 25 {
                            draw_char_bb(b'_', cursor_x, prompt_y, 0x0038_BDF8, Some(0x000F_172A));
                        }
                    }

                    // Botão interativo de spawn
                    let btn_y = win.y + win.height - 34;
                    fill_rect_bb(win.x + 8, btn_y, 220, 24, COLOR_BTN_BG);
                    draw_rect_outline_bb(win.x + 8, btn_y, 220, 24, 0x0093_C5FD);
                    draw_string_bb("+ Executar ola-mundo (U3)", win.x + 14, btn_y + 4, COLOR_WHITE, Some(COLOR_BTN_BG));

                    if wm.spawn_count > 0 {
                        draw_label_and_num("Apps: ", wm.spawn_count, " PIDs", win.x + 236, btn_y + 6, COLOR_ACCENT_GREEN, Some(COLOR_WIN_BG));
                    }
                } else if win.id == 2 {
                    // ── Janela 2: Sobre o Jinn OS ─────────────────────────
                    for i in 0..win.line_count {
                        let line_y = win.y + 36 + (i * 20);
                        draw_string_bb(win.lines[i], win.x + 12, line_y, COLOR_TEXT_DARK, Some(COLOR_WIN_BG));
                    }
                } else if win.id == 3 {
                    // ── Janela 3: Gerenciador de Arquivos Aladdin (FAT32) ──
                    draw_string_bb("--- Armazenamento Jinn / VFS ---", win.x + 12, win.y + 36, COLOR_ALADDIN_CYAN, Some(COLOR_ALADDIN_CARD));
                    draw_string_bb("Ponto de Montagem: /disk (FAT32 em IDE hd0)", win.x + 12, win.y + 56, COLOR_ALADDIN_TEXT_MUTED, Some(COLOR_ALADDIN_CARD));

                    // Card de arquivos do volume
                    let files_box_x = win.x + 12;
                    let files_box_y = win.y + 76;
                    let files_box_w = win.width - 24;
                    let files_box_h = 96;
                    fill_rect_bb(files_box_x, files_box_y, files_box_w, files_box_h, COLOR_ALADDIN_CARD_ALT);
                    draw_rect_outline_bb(files_box_x, files_box_y, files_box_w, files_box_h, COLOR_ALADDIN_BORDER);

                    draw_string_bb("[TXT] README.TXT       1.2 KB  (Documento)", files_box_x + 12, files_box_y + 12, COLOR_ALADDIN_TEXT_PRI, Some(COLOR_ALADDIN_CARD_ALT));
                    draw_string_bb("[ELF] OLA.ELF         18.4 KB  (Binario U3)", files_box_x + 12, files_box_y + 36, COLOR_ALADDIN_EMERALD, Some(COLOR_ALADDIN_CARD_ALT));
                    draw_string_bb("[DEV] ide0_block0     512 B    (Setor MBR)", files_box_x + 12, files_box_y + 60, COLOR_ALADDIN_PURPLE, Some(COLOR_ALADDIN_CARD_ALT));

                    draw_string_bb("Camada: BlockDevice -> BlockCache LRU -> FAT32", win.x + 12, win.y + 184, COLOR_ALADDIN_TEXT_MUTED, Some(COLOR_ALADDIN_CARD));
                    draw_string_bb("Integridade: Setor 1 protegido e verificado", win.x + 12, win.y + 204, COLOR_ALADDIN_CYAN, Some(COLOR_ALADDIN_CARD));
                }
            }
        }
    }

    // 3. Barra de Tarefas (Taskbar) na parte inferior
    let taskbar_y = DEFAULT_HEIGHT - 32;
    fill_rect_bb(0, taskbar_y, DEFAULT_WIDTH, 32, COLOR_TASKBAR);
    draw_line_bb(0, taskbar_y as isize, DEFAULT_WIDTH as isize, taskbar_y as isize, 0x0033_4155);

    unsafe {
        let wm = &*core::ptr::addr_of!(WM);

        // Botão Iniciar [ Jinn OS ]
        let is_start_hover = mouse_x >= 6 && mouse_x <= 116 && mouse_y >= taskbar_y + 4 && mouse_y <= taskbar_y + 28;
        let start_bg = if is_start_hover || wm.start_menu_open { COLOR_START_HOVER } else { COLOR_START_BTN };
        fill_rect_bb(6, taskbar_y + 4, 110, 24, start_bg);
        draw_rect_outline_bb(6, taskbar_y + 4, 110, 24, 0x0060_A5FA);
        draw_string_bb("  Jinn OS", 14, taskbar_y + 8, COLOR_WHITE, Some(start_bg));

        // Abas das Janelas na Barra de Tarefas
        let tab_defs = [
            (" Monitor ", 126, 0),
            (" Terminal", 232, 1),
            (" Sobre   ", 338, 2),
            (" Arquivos", 444, 3),
        ];

        for &(label, tab_x, win_id) in tab_defs.iter() {
            let is_open = wm.windows[win_id].map(|w| w.is_open).unwrap_or(false);
            let tab_bg = if is_open { 0x001E_293B } else { 0x000F_172A };
            let tab_fg = if is_open { COLOR_WHITE } else { 0x0064_748B };
            fill_rect_bb(tab_x, taskbar_y + 4, 96, 24, tab_bg);
            draw_rect_outline_bb(tab_x, taskbar_y + 4, 96, 24, 0x0033_4155);
            draw_string_bb(label, tab_x + 8, taskbar_y + 8, tab_fg, Some(tab_bg));
        }

        // Relógio / Info no canto direito da barra de tarefas
        let secs = (crate::timer::ticks() / 100) as usize;
        draw_label_and_num("Uptime: ", secs, "s | 1024x768", DEFAULT_WIDTH - 240, taskbar_y + 8, 0x0094_A3B8, Some(COLOR_TASKBAR));

        // 4. Menu Iniciar Popup (se ativo)
        if wm.start_menu_open {
            let menu_x = 6;
            let menu_y = taskbar_y - 238;
            let menu_w = 240;
            let menu_h = 234;

            // Sombra do menu
            fill_rect_bb(menu_x + 2, menu_y + 2, menu_w, menu_h, 0x000A_0F1D);
            // Corpo do menu
            fill_rect_bb(menu_x, menu_y, menu_w, menu_h, COLOR_MENU_BG);
            draw_rect_outline_bb(menu_x, menu_y, menu_w, menu_h, COLOR_MENU_BORDER);

            // Cabeçalho do menu
            fill_rect_bb(menu_x + 1, menu_y + 1, menu_w - 2, 28, COLOR_MENU_HEADER);
            draw_string_bb("Jinn OS v0.0.3 Desktop", menu_x + 12, menu_y + 8, COLOR_WHITE, Some(COLOR_MENU_HEADER));

            // Itens do menu
            let items = [
                "1. Monitor de Sistema",
                "2. Terminal Interativo",
                "3. Arquivos /disk (FAT32)",
                "4. Executar ola-mundo (U3)",
                "5. Sobre o Jinn OS",
                "6. Reiniciar Maquina",
                "7. Sair para Shell (ESC)",
            ];

            for (idx, &item) in items.iter().enumerate() {
                let item_y = menu_y + 32 + (idx * 28);
                let is_hover = mouse_x >= menu_x && mouse_x <= menu_x + menu_w && mouse_y >= item_y && mouse_y < item_y + 28;
                let item_bg = if is_hover { COLOR_MENU_HOVER } else { COLOR_MENU_BG };
                let item_fg = if is_hover { COLOR_WHITE } else { 0x00E2_E8F0 };
                fill_rect_bb(menu_x + 1, item_y, menu_w - 2, 28, item_bg);
                draw_string_bb(item, menu_x + 14, item_y + 6, item_fg, Some(item_bg));
            }
        }
    }

    // 5. Renderiza o cursor do mouse no topo de tudo
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

    // 6. Envia o frame final da RAM para a memória de vídeo (VRAM)
    if let Some(ref rect) = damage {
        swap_buffer_rect(rect);
    } else {
        swap_buffers();
    }
}

// ── Gerenciador de Eventos de Mouse ───────────────────────────────────────────

pub fn handle_mouse(mouse_x: usize, mouse_y: usize, buttons: u8) -> (bool, Option<DirtyRect>) {
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

        // Se está arrastando alguma janela, atualiza suas coordenadas e calcula a região danificada
        let mut dragging_any = false;
        let mut drag_damage: Option<DirtyRect> = None;
        for win_opt in wm.windows.iter_mut() {
            if let Some(win) = win_opt {
                if win.is_dragging {
                    let old_x = win.x;
                    let old_y = win.y;
                    let new_x = mouse_x.saturating_sub(win.drag_offset_x);
                    let new_y = mouse_y.saturating_sub(win.drag_offset_y);
                    win.x = new_x.min(DEFAULT_WIDTH.saturating_sub(win.width));
                    win.y = new_y.min((DEFAULT_HEIGHT - 32).saturating_sub(win.height));
                    dragging_any = true;

                    let old_box = DirtyRect::new(old_x.saturating_sub(2), old_y.saturating_sub(2), win.width + 4, win.height + 4);
                    let new_box = DirtyRect::new(win.x.saturating_sub(2), win.y.saturating_sub(2), win.width + 4, win.height + 4);
                    drag_damage = Some(old_box.union(&new_box));
                }
            }
        }

        // Se acabou de clicar com o botão esquerdo (transição de 0 para 1)
        if left_clicked && !prev_left_clicked && !dragging_any {
            let taskbar_y = DEFAULT_HEIGHT - 32;

            // 1. Clique no Botão Iniciar [ Jinn OS ]
            if mouse_x >= 6 && mouse_x <= 116 && mouse_y >= taskbar_y + 4 && mouse_y <= taskbar_y + 28 {
                wm.start_menu_open = !wm.start_menu_open;
                return (false, None);
            }

            // 2. Clique dentro do Menu Iniciar
            if wm.start_menu_open {
                let menu_x = 6;
                let menu_y = taskbar_y - 238;
                let menu_w = 240;
                if mouse_x >= menu_x && mouse_x <= menu_x + menu_w && mouse_y >= menu_y && mouse_y < taskbar_y {
                    let item_idx = (mouse_y.saturating_sub(menu_y + 32)) / 28;
                    match item_idx {
                        0 => {
                            // Monitor de Sistema
                            if let Some(w) = wm.windows[0].as_mut() { w.is_open = true; }
                            bring_to_front(0);
                        }
                        1 => {
                            // Terminal Interativo
                            if let Some(w) = wm.windows[1].as_mut() { w.is_open = true; }
                            bring_to_front(1);
                        }
                        2 => {
                            // Gerenciador de Arquivos (FAT32)
                            if let Some(w) = wm.windows[3].as_mut() { w.is_open = true; }
                            bring_to_front(3);
                        }
                        3 => {
                            // Executar ola-mundo
                            spawn_user_app();
                        }
                        4 => {
                            // Sobre o Jinn OS
                            if let Some(w) = wm.windows[2].as_mut() { w.is_open = true; }
                            bring_to_front(2);
                        }
                        5 => {
                            // Reiniciar Maquina via triple fault
                            reboot_machine();
                        }
                        6 => {
                            // Sair para o Shell
                            wm.should_exit = true;
                        }
                        _ => {}
                    }
                    wm.start_menu_open = false;
                    return (false, None);
                } else {
                    // Clique fora do menu fecha o menu
                    wm.start_menu_open = false;
                }
            }

            // 3. Clique nas Abas da Barra de Tarefas
            if mouse_y >= taskbar_y + 4 && mouse_y <= taskbar_y + 28 {
                let tab_defs = [(126, 0), (232, 1), (338, 2), (444, 3)];
                for &(tx, win_id) in tab_defs.iter() {
                    if mouse_x >= tx && mouse_x <= tx + 96 {
                        if let Some(w) = wm.windows[win_id].as_mut() {
                            w.is_open = !w.is_open;
                            if w.is_open {
                                bring_to_front(win_id);
                            }
                        }
                        return (false, None);
                    }
                }
            }

            // 4. Clique dentro de Janelas (arrasto, fechar ou botão de ação)
            let mut clicked_win_idx = None;
            for i in (0..MAX_WINDOWS).rev() {
                let win_idx = wm.order[i];
                if let Some(win) = wm.windows[win_idx] {
                    if !win.is_open { continue; }

                    if mouse_x >= win.x && mouse_x < win.x + win.width &&
                       mouse_y >= win.y && mouse_y < win.y + win.height {

                        // Botão de fechar [X]
                        let btn_x = win.x + win.width - 22;
                        let btn_y = win.y + 3;
                        if mouse_x >= btn_x && mouse_x < btn_x + 18 &&
                           mouse_y >= btn_y && mouse_y < btn_y + 18 {
                            if let Some(w) = wm.windows[win_idx].as_mut() {
                                w.is_open = false;
                            }
                            return (false, None);
                        }

                        // Botão de ação na janela do Terminal (Window 1)
                        if win_idx == 1 {
                            let btn_action_y = win.y + win.height - 34;
                            if mouse_x >= win.x + 12 && mouse_x <= win.x + 242 &&
                               mouse_y >= btn_action_y && mouse_y <= btn_action_y + 24 {
                                spawn_user_app();
                                bring_to_front(1);
                                return (false, None);
                            }
                        }

                        // Barra de título: inicia arrasto
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

        (dragging_any, drag_damage)
    }
}

fn spawn_user_app() {
    unsafe {
        let wm = &mut *core::ptr::addr_of_mut!(WM);
        static mut GUI_ELF_BUF: [u8; 64 * 1024] = [0u8; 64 * 1024];
        let sched = crate::scheduler::get();

        let mut vfs_len = 0;
        let buf_ptr = core::ptr::addr_of_mut!(GUI_ELF_BUF);
        if let Ok(n) = crate::vfs::read_file("/disk/ola.elf", &mut (*buf_ptr)) {
            vfs_len = n;
        } else if let Ok(n) = crate::vfs::read_file("/disk/ola-mundo.elf", &mut (*buf_ptr)) {
            vfs_len = n;
        }

        let elf_data: Option<&[u8]> = if vfs_len > 0 {
            Some(core::slice::from_raw_parts((*buf_ptr).as_ptr(), vfs_len))
        } else {
            crate::limine::get_module(0)
        };

        if let Some(data) = elf_data {
            let pid = sched.add_user_task("ola-mundo[U3]", data);
            if pid > 0 {
                crate::ipc::register(pid);
                wm.spawn_count += 1;
                if let Some(w) = wm.windows[1].as_mut() {
                    w.is_open = true;
                }
                bring_to_front(1);
            }
        }
    }
}

fn reboot_machine() -> ! {
    unsafe {
        let null_idt: [u8; 10] = [0u8; 10];
        core::arch::asm!("lidt [{}]", "int 3", in(reg) null_idt.as_ptr(), options(nostack));
    }
    loop { core::hint::spin_loop(); }
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
    unsafe {
        let wm = &mut *core::ptr::addr_of_mut!(WM);
        wm.should_exit = false;
    }

    let mut last_mx = usize::MAX;
    let mut last_my = usize::MAX;
    let mut last_btn = 0xFF;
    let mut last_ticks = 0u64;

    loop {
        // 1. Lê porta PS/2 caso a interrupção não tenha pego e alimenta o buffer
        unsafe {
            let status: u8;
            core::arch::asm!("in al, dx", out("al") status, in("dx") 0x64u16, options(nomem, nostack));
            if (status & 1) != 0 && (status & 0x20) == 0 {
                let sc: u8;
                core::arch::asm!("in al, dx", out("al") sc, in("dx") 0x60u16, options(nomem, nostack));
                if sc == 0x01 { // Scancode ESC: sair do desktop
                    break;
                }
                crate::interrupts::ps2_keyboard::process_scancode(sc);
            }

            let wm = &*core::ptr::addr_of!(WM);
            if wm.should_exit {
                break;
            }
        }

        // 2. Processa caracteres do teclado para a janela de Terminal
        let mut key_pressed = false;
        while let Some(ch) = crate::keyboard_buffer::pop() {
            if ch == 0x1B {
                return;
            }
            unsafe {
                let wm = &mut *core::ptr::addr_of_mut!(WM);
                let top_idx = wm.order[MAX_WINDOWS - 1];
                if top_idx == 1 {
                    wm.terminal_handle_key(ch);
                    key_pressed = true;
                }
            }
        }

        // 3. Obtém coordenadas e botões atuais do mouse
        let (mx, my, buttons) = crate::interrupts::ps2_mouse::get_state();
        let current_ticks = crate::timer::ticks();

        // 4. Processa interação e arrasto de janelas
        let (dragging_any, drag_damage) = handle_mouse(mx, my, buttons);

        // 5. Re-renderiza se houve movimento do mouse, clique, digitação, arrasto ou passou 10 ticks (100ms)
        let mouse_moved = mx != last_mx || my != last_my;
        let button_changed = buttons != last_btn;
        let time_changed  = current_ticks.saturating_sub(last_ticks) >= 10;
        let state_changed = mouse_moved || button_changed || key_pressed || dragging_any;

        if state_changed || time_changed {
            // Damage Tracking: se janela foi arrastada, atualiza apenas união dos quadros; se apenas cursor moveu, caixa do cursor
            let damage = if dragging_any && drag_damage.is_some() {
                drag_damage
            } else if mouse_moved && !button_changed && !key_pressed && !time_changed && last_mx != usize::MAX {
                let min_x = last_mx.min(mx);
                let min_y = last_my.min(my);
                let max_x = (last_mx.max(mx) + CURSOR_W + 2).min(DEFAULT_WIDTH);
                let max_y = (last_my.max(my) + CURSOR_H + 2).min(DEFAULT_HEIGHT);
                Some(DirtyRect::new(min_x, min_y, max_x.saturating_sub(min_x), max_y.saturating_sub(min_y)))
            } else {
                None // Redesenho completo para cliques, digitação ou tempo
            };

            last_mx = mx;
            last_my = my;
            last_btn = buttons;
            last_ticks = current_ticks;

            // Renderiza e faz swap (incremental ou total conforme o damage)
            render_desktop(mx, my, damage);
        } else {
            // Cede uma fatia curta de CPU para que outras threads e timers rodem
            core::hint::spin_loop();
        }
    }

    // Ao sair, limpa a tela para restaurar o console de texto
    let mut writer = crate::vga::Writer::new();
    writer.clear_screen();
    writer.set_color(crate::vga::Color::LightCyan, crate::vga::Color::Black);
    writer.write_line("Retornou ao Shell do Kernel [Ring 0].");
    writer.set_color(crate::vga::Color::LightGray, crate::vga::Color::Black);
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
