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
    pub is_minimized: bool,
    pub is_maximized: bool,
    pub saved_x: usize,
    pub saved_y: usize,
    pub saved_width: usize,
    pub saved_height: usize,
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

#[derive(Clone, Copy)]
pub struct CachedFileEntry {
    pub name: [u8; 16],
    pub name_len: usize,
    pub is_elf: bool,
    pub is_txt: bool,
}

impl CachedFileEntry {
    pub const fn empty() -> Self {
        Self {
            name: [0; 16],
            name_len: 0,
            is_elf: false,
            is_txt: false,
        }
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
    preview_active: bool,
    preview_filename: [u8; 16],
    preview_filename_len: usize,
    preview_content: [u8; 256],
    preview_content_len: usize,
    file_cache: [CachedFileEntry; 6],
    file_cache_count: usize,
    file_cache_valid: bool,
}

impl WindowManager {
    pub fn refresh_file_cache(&mut self) {
        self.file_cache_count = 0;
        let mut count = 0;
        let cache_ptr = self.file_cache.as_mut_ptr();

        let _ = crate::vfs::readdir("/disk", |name, _| {
            if count < 6 {
                let n_len = name.len().min(15);
                unsafe {
                    let slot = &mut *cache_ptr.add(count);
                    slot.name[..n_len].copy_from_slice(&name.as_bytes()[..n_len]);
                    slot.name_len = n_len;
                    slot.is_elf = name.ends_with(".elf") || name.ends_with(".ELF");
                    slot.is_txt = name.ends_with(".txt") || name.ends_with(".TXT");
                }
                count += 1;
            }
        });

        self.file_cache_count = count;
        self.file_cache_valid = true;
    }

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

                    let (cmd, arg) = if let Some(idx) = cmd_str.find(' ') {
                        (&cmd_str[..idx], cmd_str[idx + 1..].trim())
                    } else {
                        (cmd_str, "")
                    };

                    // Processa comandos internos do terminal gráfico
                    match cmd {
                        "help" => {
                            self.terminal.push_line("Cmds: help, clear, ps, mem, uptime, ls, cat, touch, rm, sync, spawn, reboot", 0x0038_BDF8);
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
                        "mem" => {
                            let total_mb = crate::memory::total_bytes() / (1024 * 1024);
                            let used_mb = crate::memory::used_bytes() / (1024 * 1024);
                            let free_mb = total_mb.saturating_sub(used_mb);
                            let mut mem_buf = [0u8; 46];
                            let s_ram = "RAM: ";
                            mem_buf[..s_ram.len()].copy_from_slice(s_ram.as_bytes());
                            let mut pos = s_ram.len();
                            pos += format_num(used_mb, &mut mem_buf[pos..]);
                            mem_buf[pos] = b'/';
                            pos += 1;
                            pos += format_num(total_mb, &mut mem_buf[pos..]);
                            let s_end = " MB (Livre: ";
                            mem_buf[pos..pos + s_end.len()].copy_from_slice(s_end.as_bytes());
                            pos += s_end.len();
                            pos += format_num(free_mb, &mut mem_buf[pos..]);
                            let s_close = "M)";
                            mem_buf[pos..pos + s_close.len()].copy_from_slice(s_close.as_bytes());
                            pos += s_close.len();
                            if let Ok(msg) = core::str::from_utf8(&mem_buf[..pos]) {
                                self.terminal.push_line(msg, 0x0022_C55E);
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
                        "ls" => {
                            self.refresh_file_cache();
                            self.terminal.push_line("Arquivos em /disk:", 0x0038_BDF8);
                            for i in 0..self.file_cache_count {
                                let entry = &self.file_cache[i];
                                if let Ok(name_str) = core::str::from_utf8(&entry.name[..entry.name_len]) {
                                    self.terminal.push_line(name_str, if entry.is_elf { 0x0022_C55E } else { 0x00F8_FAFC });
                                }
                            }
                        }
                        "touch" => {
                            if arg.is_empty() {
                                self.terminal.push_line("Uso: touch <arquivo>", 0x00F8_7171);
                            } else {
                                let mut path_buf = [0u8; 32];
                                let prefix = b"/disk/";
                                path_buf[..6].copy_from_slice(prefix);
                                let arg_bytes = arg.as_bytes();
                                let arg_len = arg_bytes.len().min(24);
                                path_buf[6..6 + arg_len].copy_from_slice(&arg_bytes[..arg_len]);
                                if let Ok(p) = core::str::from_utf8(&path_buf[..6 + arg_len]) {
                                    match crate::vfs::create(p) {
                                        Ok(()) => {
                                            let _ = crate::vfs::sync_all();
                                            self.refresh_file_cache();
                                            self.terminal.push_line("[OK] Arquivo criado em /disk!", 0x0022_C55E);
                                        }
                                        Err(_) => {
                                            self.terminal.push_line("[ERRO] Falha ao criar arquivo.", 0x00F8_7171);
                                        }
                                    }
                                }
                            }
                        }
                        "cat" => {
                            if arg.is_empty() {
                                self.terminal.push_line("Uso: cat <arquivo>", 0x00F8_7171);
                            } else {
                                let mut path_buf = [0u8; 32];
                                let prefix = b"/disk/";
                                path_buf[..6].copy_from_slice(prefix);
                                let arg_bytes = arg.as_bytes();
                                let arg_len = arg_bytes.len().min(24);
                                path_buf[6..6 + arg_len].copy_from_slice(&arg_bytes[..arg_len]);
                                if let Ok(p) = core::str::from_utf8(&path_buf[..6 + arg_len]) {
                                    let mut read_buf = [0u8; 128];
                                    match crate::vfs::read_file(p, &mut read_buf) {
                                        Ok(n) => {
                                            if n > 0 {
                                                let line_len = read_buf[..n].iter().position(|&b| b == b'\n').unwrap_or(n).min(44);
                                                if let Ok(content_str) = core::str::from_utf8(&read_buf[..line_len]) {
                                                    self.terminal.push_line(content_str, 0x00F8_FAFC);
                                                }
                                            } else {
                                                self.terminal.push_line("(Arquivo vazio)", 0x0094_A3B8);
                                            }
                                        }
                                        Err(_) => {
                                            self.terminal.push_line("[ERRO] Arquivo nao encontrado.", 0x00F8_7171);
                                        }
                                    }
                                }
                            }
                        }
                        "rm" => {
                            if arg.is_empty() {
                                self.terminal.push_line("Uso: rm <arquivo>", 0x00F8_7171);
                            } else {
                                let mut path_buf = [0u8; 32];
                                let prefix = b"/disk/";
                                path_buf[..6].copy_from_slice(prefix);
                                let arg_bytes = arg.as_bytes();
                                let arg_len = arg_bytes.len().min(24);
                                path_buf[6..6 + arg_len].copy_from_slice(&arg_bytes[..arg_len]);
                                if let Ok(p) = core::str::from_utf8(&path_buf[..6 + arg_len]) {
                                    match crate::vfs::remove(p) {
                                        Ok(()) => {
                                            let _ = crate::vfs::sync_all();
                                            self.refresh_file_cache();
                                            self.terminal.push_line("[OK] Arquivo removido de /disk!", 0x0022_C55E);
                                        }
                                        Err(_) => {
                                            self.terminal.push_line("[ERRO] Falha ao remover arquivo.", 0x00F8_7171);
                                        }
                                    }
                                }
                            }
                        }
                        "sync" => {
                            let _ = crate::vfs::sync_all();
                            self.terminal.push_line("[OK] VFS & Cache sincronizados!", 0x0022_C55E);
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
    preview_active: false,
    preview_filename: [0u8; 16],
    preview_filename_len: 0,
    preview_content: [0u8; 256],
    preview_content_len: 0,
    file_cache: [CachedFileEntry::empty(); 6],
    file_cache_count: 0,
    file_cache_valid: false,
};

/// Verifica se o framebuffer e o backbuffer do ambiente gráfico estão inicializados e disponíveis.
pub fn is_available() -> bool {
    unsafe {
        let info_ptr = core::ptr::addr_of!(FB_INFO);
        let bb_ptr = core::ptr::addr_of!(BACKBUFFER);
        (*info_ptr).is_some() && !(*bb_ptr).is_null()
    }
}

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
            is_minimized: false,
            is_maximized: false,
            saved_x: 60,
            saved_y: 60,
            saved_width: 380,
            saved_height: 220,
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
            is_minimized: false,
            is_maximized: false,
            saved_x: 470,
            saved_y: 80,
            saved_width: 460,
            saved_height: 240,
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
            is_minimized: false,
            is_maximized: false,
            saved_x: 240,
            saved_y: 330,
            saved_width: 420,
            saved_height: 200,
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
            is_minimized: false,
            is_maximized: false,
            saved_x: 160,
            saved_y: 190,
            saved_width: 440,
            saved_height: 240,
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
    unsafe {
        let bb = *core::ptr::addr_of!(BACKBUFFER);
        if bb.is_null() || x >= DEFAULT_WIDTH || y >= DEFAULT_HEIGHT { return; }
        let end_x = (x + width).min(DEFAULT_WIDTH);
        let end_y = (y + height).min(DEFAULT_HEIGHT);
        let row_len = end_x - x;
        if row_len == 0 { return; }

        for cy in y..end_y {
            let row_ptr = bb.add(cy * DEFAULT_WIDTH + x);
            for i in 0..row_len {
                core::ptr::write_volatile(row_ptr.add(i), color);
            }
        }
    }
}

pub fn draw_char_bb(ch: u8, x: usize, y: usize, fg: u32, bg: Option<u32>) {
    unsafe {
        let bb = *core::ptr::addr_of!(BACKBUFFER);
        if bb.is_null() || x >= DEFAULT_WIDTH || y >= DEFAULT_HEIGHT { return; }
        let glyph = font::get_glyph(ch);
        let max_y = (y + 16).min(DEFAULT_HEIGHT);
        let max_x = (x + 8).min(DEFAULT_WIDTH);

        for (y_offset, row_byte) in glyph.iter().enumerate() {
            let cy = y + y_offset;
            if cy >= max_y { break; }
            let row_ptr = bb.add(cy * DEFAULT_WIDTH);
            for x_offset in 0..8 {
                let cx = x + x_offset;
                if cx >= max_x { break; }

                if (row_byte & (1 << (7 - x_offset))) != 0 {
                    core::ptr::write_volatile(row_ptr.add(cx), fg);
                } else if let Some(bg_color) = bg {
                    core::ptr::write_volatile(row_ptr.add(cx), bg_color);
                }
            }
        }
    }
}

/// Restaura os pixels originais sob o cursor no Backbuffer (desenho do cursor apagado).
pub unsafe fn erase_cursor_bb() -> Option<DirtyRect> {
    let cursor = &mut *core::ptr::addr_of_mut!(CURSOR);
    let bb = *core::ptr::addr_of!(BACKBUFFER);
    if !cursor.valid || bb.is_null() {
        return None;
    }

    let cx_base = cursor.x;
    let cy_base = cursor.y;
    for cy in 0..CURSOR_H {
        let py = cy_base + cy;
        if py >= DEFAULT_HEIGHT { break; }
        let row_ptr = bb.add(py * DEFAULT_WIDTH);
        for cx in 0..CURSOR_W {
            let px = cx_base + cx;
            if px >= DEFAULT_WIDTH { break; }
            let bg_color = cursor.saved_bg[cy * CURSOR_W + cx];
            core::ptr::write_volatile(row_ptr.add(px), bg_color);
        }
    }

    cursor.valid = false;
    Some(DirtyRect::new(cx_base, cy_base, CURSOR_W, CURSOR_H))
}

/// Salva o fundo do Backbuffer sob as novas coordenadas e renderiza o sprite do cursor (< 5 µs).
pub unsafe fn draw_cursor_bb(new_x: usize, new_y: usize) -> DirtyRect {
    let cursor = &mut *core::ptr::addr_of_mut!(CURSOR);
    let bb = *core::ptr::addr_of!(BACKBUFFER);

    let cx_base = new_x.min(DEFAULT_WIDTH.saturating_sub(CURSOR_W));
    let cy_base = new_y.min(DEFAULT_HEIGHT.saturating_sub(CURSOR_H));

    cursor.x = cx_base;
    cursor.y = cy_base;

    if !bb.is_null() {
        // 1. Salva os pixels de fundo
        for cy in 0..CURSOR_H {
            let py = cy_base + cy;
            if py >= DEFAULT_HEIGHT { break; }
            let row_ptr = bb.add(py * DEFAULT_WIDTH);
            for cx in 0..CURSOR_W {
                let px = cx_base + cx;
                if px >= DEFAULT_WIDTH { break; }
                cursor.saved_bg[cy * CURSOR_W + cx] = core::ptr::read_volatile(row_ptr.add(px));
            }
        }

        // 2. Escreve os pixels do cursor
        for (cy, line) in CURSOR_BITMAP.iter().enumerate() {
            let py = cy_base + cy;
            if py >= DEFAULT_HEIGHT { break; }
            let row_ptr = bb.add(py * DEFAULT_WIDTH);
            for (cx, ch) in line.bytes().enumerate() {
                let px = cx_base + cx;
                if px >= DEFAULT_WIDTH { break; }
                match ch {
                    b'#' => core::ptr::write_volatile(row_ptr.add(px), COLOR_BLACK),
                    b'*' => core::ptr::write_volatile(row_ptr.add(px), COLOR_WHITE),
                    _ => {} // Transparente
                }
            }
        }
    }

    cursor.valid = true;
    DirtyRect::new(cx_base, cy_base, CURSOR_W, CURSOR_H)
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

pub fn render_desktop_scene(mouse_x: usize, mouse_y: usize) {
    // 1. Limpa o fundo do Desktop com tom Abismo Ardósia do Aladdin
    fill_rect_bb(0, 0, DEFAULT_WIDTH, DEFAULT_HEIGHT, COLOR_ALADDIN_BG);

    // 1.1. Barra Superior Wingpanel (elementary OS 7 inspired)
    fill_rect_bb(0, 0, DEFAULT_WIDTH, 26, COLOR_ALADDIN_TOPPANEL);
    draw_line_bb(0, 26, DEFAULT_WIDTH as isize, 26, COLOR_ALADDIN_BORDER);

    unsafe {
        let wm = &*core::ptr::addr_of!(WM);

        // Launcher Pill [ # Aladdin ]
        let launcher_bg = if wm.start_menu_open { COLOR_ALADDIN_CARD_ALT } else { COLOR_ALADDIN_TOPPANEL };
        let launcher_bd = if wm.start_menu_open { COLOR_ALADDIN_CYAN } else { COLOR_ALADDIN_BORDER };
        fill_rect_bb(8, 3, 114, 20, launcher_bg);
        draw_rect_outline_bb(8, 3, 114, 20, launcher_bd);
        draw_string_bb("[#] Aladdin", 16, 5, COLOR_ALADDIN_CYAN, Some(launcher_bg));

        // Center Pill: Uptime & APIC Timer
        let secs = (crate::timer::ticks() / 100) as usize;
        fill_rect_bb(440, 3, 144, 20, COLOR_ALADDIN_CARD_ALT);
        draw_rect_outline_bb(440, 3, 144, 20, COLOR_ALADDIN_BORDER);
        draw_label_and_num("Up: ", secs, "s | 100Hz", 448, 5, COLOR_ALADDIN_CYAN, Some(COLOR_ALADDIN_CARD_ALT));

        // Right Pill 1: RAM Telemetry
        let total_mb = crate::memory::total_bytes() / (1024 * 1024);
        let used_mb = crate::memory::used_bytes() / (1024 * 1024);
        fill_rect_bb(600, 3, 120, 20, COLOR_ALADDIN_CARD_ALT);
        draw_rect_outline_bb(600, 3, 120, 20, COLOR_ALADDIN_BORDER);
        draw_label_and_num("RAM: ", used_mb, "/", 608, 5, COLOR_ALADDIN_EMERALD, Some(COLOR_ALADDIN_CARD_ALT));
        draw_label_and_num("", total_mb, "M", 672, 5, COLOR_ALADDIN_EMERALD, Some(COLOR_ALADDIN_CARD_ALT));

        // Right Pill 2: JSA Storage Profile
        let prof = crate::drivers::block::get_storage_profile();
        let (prof_name, prof_fg) = match prof {
            crate::drivers::block::StorageProfile::Aladdin => ("JSA: Aladdin", COLOR_ALADDIN_CYAN),
            crate::drivers::block::StorageProfile::Simbad => ("JSA: Simbad", COLOR_ALADDIN_EMERALD),
            crate::drivers::block::StorageProfile::Workstation => ("JSA: Workst", COLOR_ALADDIN_PURPLE),
            crate::drivers::block::StorageProfile::TerminalPos => ("JSA: POS", COLOR_ALADDIN_RUBY),
        };
        fill_rect_bb(726, 3, 124, 20, COLOR_ALADDIN_CARD_ALT);
        draw_rect_outline_bb(726, 3, 124, 20, COLOR_ALADDIN_BORDER);
        draw_string_bb(prof_name, 734, 5, prof_fg, Some(COLOR_ALADDIN_CARD_ALT));

        // Right Pill 3: FX Mode (Capability)
        let mode = get_capability_mode();
        let (mode_name, mode_fg) = match mode {
            CapabilityMode::Basic => ("FX: Eco", COLOR_ALADDIN_TEXT_MUTED),
            CapabilityMode::Balanced => ("FX: Bal", COLOR_ALADDIN_CYAN),
            CapabilityMode::Advanced => ("FX: Glow", COLOR_ALADDIN_PURPLE),
        };
        fill_rect_bb(856, 3, 84, 20, COLOR_ALADDIN_CARD_ALT);
        draw_rect_outline_bb(856, 3, 84, 20, COLOR_ALADDIN_BORDER);
        draw_string_bb(mode_name, 864, 5, mode_fg, Some(COLOR_ALADDIN_CARD_ALT));

        // Right Pill 4: Sair Button
        fill_rect_bb(946, 3, 70, 20, COLOR_ALADDIN_RUBY);
        draw_rect_outline_bb(946, 3, 70, 20, 0x00F8_7171);
        draw_string_bb("[X] Sair", 952, 5, COLOR_WHITE, Some(COLOR_ALADDIN_RUBY));

        // Marca d'água elegante no fundo
        draw_string_bb("Aladdin Desktop 7.0 | Jinn Microkernel x86_64", DEFAULT_WIDTH - 370, DEFAULT_HEIGHT - 16, COLOR_ALADDIN_BORDER, None);

        // 2. Renderiza as janelas em Z-order (da do fundo para a do topo)
        let top_idx = wm.order[MAX_WINDOWS - 1];

        for &win_idx in wm.order.iter() {
            if let Some(win) = wm.windows[win_idx] {
                if !win.is_open || win.is_minimized { continue; }

                let is_active = win_idx == top_idx;
                let title_color = if is_active { COLOR_ALADDIN_TOPPANEL } else { 0x0011_1827 };
                let border_color = if is_active { COLOR_ALADDIN_BORDER_ACT } else { COLOR_ALADDIN_BORDER };

                // Sombras projetadas (Drop Shadows) conforme CapabilityMode
                if mode == CapabilityMode::Advanced {
                    fill_rect_bb(win.x + 4, win.y + 4, win.width, win.height, 0x0005_0710);
                    fill_rect_bb(win.x + 2, win.y + 2, win.width, win.height, 0x000A_0F1D);
                } else if mode == CapabilityMode::Balanced && is_active {
                    fill_rect_bb(win.x + 3, win.y + 3, win.width, win.height, 0x000A_0F1D);
                }

                // Moldura externa da janela (Aladdin Card Border)
                draw_rect_outline_bb(win.x.saturating_sub(1), win.y.saturating_sub(1), win.width + 2, win.height + 2, border_color);

                // Barra de título (Aladdin Window Header)
                fill_rect_bb(win.x, win.y, win.width, 24, title_color);

                // Linha de acentuação luminescente sob o título na janela ativa (Garuda Dr460nized touch)
                if is_active {
                    draw_line_bb(win.x as isize, (win.y + 24) as isize, (win.x + win.width) as isize, (win.y + 24) as isize, COLOR_ALADDIN_CYAN);
                    if mode == CapabilityMode::Advanced {
                        draw_line_bb(win.x as isize, (win.y + 25) as isize, (win.x + win.width) as isize, (win.y + 25) as isize, COLOR_ALADDIN_PURPLE);
                    }
                }

                // Indicador de foco / status e título da janela
                if is_active {
                    draw_char_bb(b'*', win.x + 8, win.y + 4, COLOR_ALADDIN_CYAN, Some(title_color));
                    draw_string_bb(win.title, win.x + 20, win.y + 4, COLOR_ALADDIN_TEXT_PRI, Some(title_color));
                } else {
                    draw_string_bb(win.title, win.x + 10, win.y + 4, COLOR_ALADDIN_TEXT_MUTED, Some(title_color));
                }

                // Trio de botões de controle de janela (Minimizar [-], Maximizar/Restaurar [+]/[^], Fechar [X])
                let btn_y = win.y + 4;

                // 1. Botão de minimizar [-] em âmbar Aladdin
                let btn_min_x = win.x + win.width - 64;
                fill_rect_bb(btn_min_x, btn_y, 16, 16, COLOR_ALADDIN_AMBER);
                draw_char_bb(b'-', btn_min_x + 4, btn_y, COLOR_WHITE, Some(COLOR_ALADDIN_AMBER));

                // 2. Botão de maximizar/restaurar [+] / [^] em esmeralda Aladdin
                let btn_max_x = win.x + win.width - 44;
                let max_char = if win.is_maximized { b'^' } else { b'+' };
                fill_rect_bb(btn_max_x, btn_y, 16, 16, COLOR_ALADDIN_EMERALD);
                draw_char_bb(max_char, btn_max_x + 4, btn_y, COLOR_WHITE, Some(COLOR_ALADDIN_EMERALD));

                // 3. Botão de fechar [X] estilizado em rubi Aladdin
                let btn_close_x = win.x + win.width - 24;
                fill_rect_bb(btn_close_x, btn_y, 16, 16, COLOR_ALADDIN_RUBY);
                draw_char_bb(b'X', btn_close_x + 4, btn_y, COLOR_WHITE, Some(COLOR_ALADDIN_RUBY));

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
                    let btn_action_y = win.y + win.height - 34;
                    fill_rect_bb(win.x + 8, btn_action_y, 220, 24, COLOR_BTN_BG);
                    draw_rect_outline_bb(win.x + 8, btn_action_y, 220, 24, 0x0093_C5FD);
                    draw_string_bb("+ Executar ola-mundo (U3)", win.x + 14, btn_action_y + 4, COLOR_WHITE, Some(COLOR_BTN_BG));

                    if wm.spawn_count > 0 {
                        draw_label_and_num("Apps: ", wm.spawn_count, " PIDs", win.x + 236, btn_action_y + 6, COLOR_ACCENT_GREEN, Some(COLOR_ALADDIN_CARD));
                    }
                } else if win.id == 2 {
                    // ── Janela 2: Sobre o Jinn OS ─────────────────────────
                    for i in 0..win.line_count {
                        let line_y = win.y + 36 + (i * 20);
                        draw_string_bb(win.lines[i], win.x + 12, line_y, COLOR_ALADDIN_TEXT_PRI, Some(COLOR_ALADDIN_CARD));
                    }
                } else if win.id == 3 {
                    // ── Janela 3: Gerenciador de Arquivos Aladdin (FAT32) ──
                    draw_string_bb("--- Armazenamento Jinn / VFS ---", win.x + 12, win.y + 36, COLOR_ALADDIN_CYAN, Some(COLOR_ALADDIN_CARD));
                    draw_string_bb("Ponto de Montagem: /disk (FAT32 em IDE hd0)", win.x + 12, win.y + 54, COLOR_ALADDIN_TEXT_MUTED, Some(COLOR_ALADDIN_CARD));

                    if wm.preview_active {
                        // Painel de Visualização de Arquivo Integrado (Quick Look / File Preview)
                        let p_x = win.x + 12;
                        let p_y = win.y + 72;
                        let p_w = win.width - 24;
                        let p_h = 104;

                        fill_rect_bb(p_x, p_y, p_w, p_h, 0x000F_172A);
                        draw_rect_outline_bb(p_x, p_y, p_w, p_h, COLOR_ALADDIN_BORDER_ACT);

                        // Barra de topo do preview
                        fill_rect_bb(p_x + 1, p_y + 1, p_w - 2, 20, 0x001E_293B);
                        draw_string_bb("Preview: /disk/", p_x + 6, p_y + 3, COLOR_ALADDIN_CYAN, Some(0x001E_293B));
                        let fn_len = wm.preview_filename_len.min(16);
                        if let Ok(fn_str) = core::str::from_utf8(&wm.preview_filename[..fn_len]) {
                            draw_string_bb(fn_str, p_x + 126, p_y + 3, COLOR_WHITE, Some(0x001E_293B));
                        }

                        // Botão Excluir no Preview
                        let btn_c_w = 76;
                        let btn_c_x = p_x + p_w - btn_c_w - 4;

                        let btn_dp_w = 64;
                        let btn_dp_x = btn_c_x - btn_dp_w - 4;
                        fill_rect_bb(btn_dp_x, p_y + 2, btn_dp_w, 16, 0x002D_1215);
                        draw_rect_outline_bb(btn_dp_x, p_y + 2, btn_dp_w, 16, 0x00EF_4444);
                        draw_string_bb("Excluir", btn_dp_x + 5, p_y + 3, 0x00F8_7171, Some(0x002D_1215));

                        // Botão Fechar Preview
                        fill_rect_bb(btn_c_x, p_y + 2, btn_c_w, 16, COLOR_ALADDIN_RUBY);
                        draw_string_bb("X Fechar", btn_c_x + 6, p_y + 3, COLOR_WHITE, Some(COLOR_ALADDIN_RUBY));

                        // Renderiza conteúdo do preview (até 4 linhas)
                        let c_len = wm.preview_content_len.min(256);
                        let mut line_y = p_y + 24;
                        let mut line_start = 0;
                        let mut rendered_lines = 0;
                        for i in 0..c_len {
                            let b = wm.preview_content[i];
                            let is_break = b == b'\n' || (i - line_start) >= 46 || i == c_len - 1;
                            if is_break {
                                let end_i = if b == b'\n' { i } else { i + 1 };
                                if end_i > line_start && rendered_lines < 4 {
                                    if let Ok(line_str) = core::str::from_utf8(&wm.preview_content[line_start..end_i]) {
                                        draw_string_bb(line_str, p_x + 8, line_y, COLOR_ALADDIN_TEXT_PRI, Some(0x000F_172A));
                                        line_y += 18;
                                        rendered_lines += 1;
                                    }
                                }
                                line_start = if b == b'\n' { i + 1 } else { i + 1 };
                            }
                        }

                        draw_string_bb("Modo: Somente Leitura | UTF-8", win.x + 12, win.y + 182, COLOR_ALADDIN_TEXT_MUTED, Some(COLOR_ALADDIN_CARD));
                    } else {
                        // Card de arquivos do volume (LÊ DO CACHE, ZERO DISK I/O NO RENDER LOOP)
                        let files_box_x = win.x + 12;
                        let files_box_y = win.y + 74;
                        let files_box_w = win.width - 24;
                        let files_box_h = 100;
                        fill_rect_bb(files_box_x, files_box_y, files_box_w, files_box_h, COLOR_ALADDIN_CARD_ALT);
                        draw_rect_outline_bb(files_box_x, files_box_y, files_box_w, files_box_h, COLOR_ALADDIN_BORDER);

                        draw_string_bb("Arquivos em /disk:", files_box_x + 8, files_box_y + 8, COLOR_ALADDIN_CYAN, Some(COLOR_ALADDIN_CARD_ALT));

                        // Botão [+ Novo]
                        let btn_new_w = 60;
                        let btn_rf_w = 88;
                        let btn_rf_x = files_box_x + files_box_w - btn_rf_w - 8;
                        let btn_new_x = btn_rf_x - btn_new_w - 6;

                        fill_rect_bb(btn_new_x, files_box_y + 6, btn_new_w, 18, 0x001E_293B);
                        draw_rect_outline_bb(btn_new_x, files_box_y + 6, btn_new_w, 18, COLOR_ALADDIN_EMERALD);
                        draw_string_bb("+ Novo", btn_new_x + 6, files_box_y + 8, COLOR_ALADDIN_EMERALD, Some(0x001E_293B));

                        // Botão [Atualizar]
                        fill_rect_bb(btn_rf_x, files_box_y + 6, btn_rf_w, 18, 0x001E_293B);
                        draw_rect_outline_bb(btn_rf_x, files_box_y + 6, btn_rf_w, 18, COLOR_ALADDIN_BORDER);
                        draw_string_bb("[Atualizar]", btn_rf_x + 6, files_box_y + 8, COLOR_ALADDIN_TEXT_PRI, Some(0x001E_293B));

                        if wm.file_cache_count == 0 {
                            draw_string_bb("(Nenhum arquivo detectado no volume /disk)", files_box_x + 12, files_box_y + 40, COLOR_ALADDIN_TEXT_MUTED, Some(COLOR_ALADDIN_CARD_ALT));
                        } else {
                            for i in 0..wm.file_cache_count.min(3) {
                                let entry = &wm.file_cache[i];
                                let item_y = files_box_y + 32 + (i * 22);
                                let tag = if entry.is_elf { "[ELF] " } else if entry.is_txt { "[TXT] " } else { "[ARQ] " };
                                let tag_color = if entry.is_elf { COLOR_ALADDIN_EMERALD } else { COLOR_ALADDIN_CYAN };
                                draw_string_bb(tag, files_box_x + 8, item_y, tag_color, Some(COLOR_ALADDIN_CARD_ALT));
                                if let Ok(name_str) = core::str::from_utf8(&entry.name[..entry.name_len]) {
                                    draw_string_bb(name_str, files_box_x + 52, item_y, COLOR_ALADDIN_TEXT_PRI, Some(COLOR_ALADDIN_CARD_ALT));
                                }

                                // Botões de ação
                                let btn_w = 64;
                                let btn_h = 16;
                                let btn_x = files_box_x + files_box_w - btn_w - 8;

                                if entry.is_elf {
                                    fill_rect_bb(btn_x, item_y - 1, btn_w, btn_h, COLOR_BTN_BG);
                                    draw_rect_outline_bb(btn_x, item_y - 1, btn_w, btn_h, 0x0093_C5FD);
                                    draw_string_bb("> Rodar", btn_x + 6, item_y, COLOR_WHITE, Some(COLOR_BTN_BG));
                                } else if entry.is_txt {
                                    // Botão [Ler VFS]
                                    fill_rect_bb(btn_x, item_y - 1, btn_w, btn_h, 0x001E_293B);
                                    draw_rect_outline_bb(btn_x, item_y - 1, btn_w, btn_h, COLOR_ALADDIN_BORDER);
                                    draw_string_bb("Ler VFS", btn_x + 5, item_y, COLOR_ALADDIN_CYAN, Some(0x001E_293B));

                                    // Botão [Del]
                                    let btn_del_w = 40;
                                    let btn_del_x = btn_x - btn_del_w - 6;
                                    fill_rect_bb(btn_del_x, item_y - 1, btn_del_w, btn_h, 0x002D_1215);
                                    draw_rect_outline_bb(btn_del_x, item_y - 1, btn_del_w, btn_h, 0x00EF_4444);
                                    draw_string_bb("Del", btn_del_x + 8, item_y, 0x00F8_7171, Some(0x002D_1215));
                                }
                            }
                        }

                        let prof = crate::drivers::block::get_storage_profile();
                        let policy = crate::drivers::block::get_storage_policy();
                        draw_string_bb(prof.name(), win.x + 12, win.y + 184, COLOR_ALADDIN_CYAN, Some(COLOR_ALADDIN_CARD));
                        let strat_str = match policy.write_strategy {
                            crate::drivers::block::WriteStrategy::WriteThrough => "Politica: Write-Through | Cache LRU: 16 KB",
                            crate::drivers::block::WriteStrategy::WriteBack => "Politica: Write-Back + Flush | Cache: 32 KB",
                        };
                        draw_string_bb(strat_str, win.x + 12, win.y + 204, COLOR_ALADDIN_TEXT_MUTED, Some(COLOR_ALADDIN_CARD));
                    }
                }
            }
        }

        // 3. Elementary OS 7 Plank Dock (Bottom Dock)
        let dock_x = 272;
        let dock_y = 714;
        let dock_w = 480;
        let dock_h = 44;

        fill_rect_bb(dock_x, dock_y, dock_w, dock_h, COLOR_ALADDIN_DOCK);
        draw_rect_outline_bb(dock_x, dock_y, dock_w, dock_h, COLOR_ALADDIN_CYAN);

        let dock_slots = [
            ("[#] Monitor", 278, 0),
            ("[>] Console", 372, 1),
            ("[~] Arquivos", 466, 3),
            ("[*] Apps U3", 560, usize::MAX),
            ("[?] Sobre", 654, 2),
        ];

        for &(label, slot_x, win_id) in dock_slots.iter() {
            let is_hover = mouse_x >= slot_x && mouse_x < slot_x + 90 && mouse_y >= dock_y && mouse_y < dock_y + dock_h;
            let slot_bg = if is_hover { COLOR_ALADDIN_CARD_ALT } else { COLOR_ALADDIN_DOCK };
            fill_rect_bb(slot_x, dock_y + 4, 88, 34, slot_bg);
            draw_string_bb(label, slot_x + 6, dock_y + 12, COLOR_ALADDIN_TEXT_PRI, Some(slot_bg));

            // Indicador de app ativa (Elementary OS dot/bar indicator)
            if win_id != usize::MAX {
                if let Some(w) = wm.windows[win_id] {
                    if w.is_open && !w.is_minimized && top_idx == win_id {
                        // Ativo e com foco: traço ciano luminescente
                        fill_rect_bb(slot_x + 20, dock_y + 36, 48, 2, COLOR_ALADDIN_CYAN);
                    } else if w.is_open && !w.is_minimized {
                        // Aberto em segundo plano: ponto esmeralda
                        fill_rect_bb(slot_x + 38, dock_y + 36, 12, 2, COLOR_ALADDIN_EMERALD);
                    } else if w.is_minimized {
                        // Minimizado: ponto âmbar
                        fill_rect_bb(slot_x + 38, dock_y + 36, 12, 2, COLOR_ALADDIN_AMBER);
                    }
                }
            }
        }

        // 4. Menu Iniciar Aladdin Applications Popup (se ativo)
        if wm.start_menu_open {
            let menu_x = 8;
            let menu_y = 28;
            let menu_w = 260;
            let menu_h = 244;

            // Sombra e fundo
            fill_rect_bb(menu_x + 3, menu_y + 3, menu_w, menu_h, 0x0005_0710);
            fill_rect_bb(menu_x, menu_y, menu_w, menu_h, 0x0011_1827);
            draw_rect_outline_bb(menu_x, menu_y, menu_w, menu_h, COLOR_ALADDIN_CYAN);

            // Cabeçalho
            fill_rect_bb(menu_x + 1, menu_y + 1, menu_w - 2, 24, 0x000F_172A);
            draw_string_bb("Aladdin OS 7 Apps", menu_x + 10, menu_y + 5, COLOR_ALADDIN_CYAN, Some(0x000F_172A));

            let items = [
                "1. Monitor de Recursos",
                "2. Terminal Interativo",
                "3. Arquivos /disk (FAT32)",
                "4. Executar ola-mundo (U3)",
                "5. Sobre o Jinn OS",
                "6. Ciclar Perfil JSA",
                "7. Reiniciar Maquina",
                "8. Sair para Shell (ESC)",
            ];

            for (idx, &item) in items.iter().enumerate() {
                let item_y = menu_y + 28 + (idx * 26);
                let is_hover = mouse_x >= menu_x && mouse_x <= menu_x + menu_w && mouse_y >= item_y && mouse_y < item_y + 26;
                let item_bg = if is_hover { COLOR_ALADDIN_CARD_ALT } else { 0x0011_1827 };
                let item_fg = if is_hover { COLOR_ALADDIN_CYAN } else { COLOR_ALADDIN_TEXT_PRI };
                fill_rect_bb(menu_x + 2, item_y, menu_w - 4, 25, item_bg);
                draw_string_bb(item, menu_x + 10, item_y + 5, item_fg, Some(item_bg));
            }
        }
    }
}

pub fn render_desktop(mouse_x: usize, mouse_y: usize, damage: Option<DirtyRect>) {
    render_desktop_scene(mouse_x, mouse_y);
    unsafe { draw_cursor_bb(mouse_x, mouse_y); }
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

        // Se está arrastando alguma janela, atualiza suas coordenadas
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
                    win.y = new_y.min((DEFAULT_HEIGHT - 48).saturating_sub(win.height));
                    dragging_any = true;

                    let old_box = DirtyRect::new(old_x.saturating_sub(2), old_y.saturating_sub(2), win.width + 4, win.height + 4);
                    let new_box = DirtyRect::new(win.x.saturating_sub(2), win.y.saturating_sub(2), win.width + 4, win.height + 4);
                    drag_damage = Some(old_box.union(&new_box));
                }
            }
        }

        // Se acabou de clicar com o botão esquerdo (transição de 0 para 1)
        if left_clicked && !prev_left_clicked && !dragging_any {
            // 1. Wingpanel Superior (y: 0..26)
            if mouse_y < 26 {
                // Launcher Pill [# Aladdin]
                if mouse_x >= 8 && mouse_x <= 122 {
                    wm.start_menu_open = !wm.start_menu_open;
                    return (false, None);
                }

                // RAM Pill
                if mouse_x >= 600 && mouse_x <= 720 {
                    if let Some(w) = wm.windows[0].as_mut() {
                        w.is_open = true;
                        w.is_minimized = false;
                    }
                    bring_to_front(0);
                    return (false, None);
                }

                // JSA Storage Profile Pill (Aladdin -> Simbad -> Workstation -> POS)
                if mouse_x >= 726 && mouse_x <= 850 {
                    let next_prof = match crate::drivers::block::get_storage_profile() {
                        crate::drivers::block::StorageProfile::Aladdin => crate::drivers::block::StorageProfile::Simbad,
                        crate::drivers::block::StorageProfile::Simbad => crate::drivers::block::StorageProfile::Workstation,
                        crate::drivers::block::StorageProfile::Workstation => crate::drivers::block::StorageProfile::TerminalPos,
                        crate::drivers::block::StorageProfile::TerminalPos => crate::drivers::block::StorageProfile::Aladdin,
                    };
                    crate::drivers::block::set_storage_profile(next_prof);
                    return (false, None);
                }

                // FX Mode Pill (Basic -> Balanced -> Advanced)
                if mouse_x >= 856 && mouse_x <= 940 {
                    let next_mode = match get_capability_mode() {
                        CapabilityMode::Basic => CapabilityMode::Balanced,
                        CapabilityMode::Balanced => CapabilityMode::Advanced,
                        CapabilityMode::Advanced => CapabilityMode::Basic,
                    };
                    set_capability_mode(next_mode);
                    return (false, None);
                }

                // Sair Button
                if mouse_x >= 946 && mouse_x <= 1016 {
                    wm.should_exit = true;
                    return (false, None);
                }
            }

            // 2. Menu Iniciar Aladdin Apps Popup
            if wm.start_menu_open {
                let menu_x = 8;
                let menu_y = 28;
                let menu_w = 260;
                let menu_h = 244;
                if mouse_x >= menu_x && mouse_x <= menu_x + menu_w && mouse_y >= menu_y && mouse_y <= menu_y + menu_h {
                    if mouse_y >= menu_y + 28 {
                        let item_idx = (mouse_y.saturating_sub(menu_y + 28)) / 26;
                        match item_idx {
                            0 => {
                                if let Some(w) = wm.windows[0].as_mut() { w.is_open = true; w.is_minimized = false; }
                                bring_to_front(0);
                            }
                            1 => {
                                if let Some(w) = wm.windows[1].as_mut() { w.is_open = true; w.is_minimized = false; }
                                bring_to_front(1);
                            }
                            2 => {
                                if let Some(w) = wm.windows[3].as_mut() { w.is_open = true; w.is_minimized = false; }
                                bring_to_front(3);
                            }
                            3 => {
                                spawn_user_app();
                            }
                            4 => {
                                if let Some(w) = wm.windows[2].as_mut() { w.is_open = true; w.is_minimized = false; }
                                bring_to_front(2);
                            }
                            5 => {
                                let next_prof = match crate::drivers::block::get_storage_profile() {
                                    crate::drivers::block::StorageProfile::Aladdin => crate::drivers::block::StorageProfile::Simbad,
                                    crate::drivers::block::StorageProfile::Simbad => crate::drivers::block::StorageProfile::Workstation,
                                    crate::drivers::block::StorageProfile::Workstation => crate::drivers::block::StorageProfile::TerminalPos,
                                    crate::drivers::block::StorageProfile::TerminalPos => crate::drivers::block::StorageProfile::Aladdin,
                                };
                                crate::drivers::block::set_storage_profile(next_prof);
                            }
                            6 => {
                                reboot_machine();
                            }
                            7 => {
                                wm.should_exit = true;
                            }
                            _ => {}
                        }
                    }
                    wm.start_menu_open = false;
                    return (false, None);
                } else {
                    wm.start_menu_open = false;
                }
            }

            // 3. Elementary OS 7 Plank Dock (y: 714..758)
            let dock_y = 714;
            let dock_h = 44;
            if mouse_y >= dock_y && mouse_y <= dock_y + dock_h {
                let dock_slots = [
                    (278, 0),
                    (372, 1),
                    (466, 3),
                    (560, usize::MAX),
                    (654, 2),
                ];

                for &(slot_x, win_id) in dock_slots.iter() {
                    if mouse_x >= slot_x && mouse_x < slot_x + 90 {
                        if win_id == usize::MAX {
                            spawn_user_app();
                            bring_to_front(1);
                        } else {
                            let top_idx = wm.order[MAX_WINDOWS - 1];
                            if let Some(w) = wm.windows[win_id].as_mut() {
                                if !w.is_open {
                                    w.is_open = true;
                                    w.is_minimized = false;
                                    bring_to_front(win_id);
                                } else if w.is_minimized {
                                    w.is_minimized = false;
                                    bring_to_front(win_id);
                                } else if top_idx == win_id {
                                    w.is_minimized = true;
                                } else {
                                    bring_to_front(win_id);
                                }
                            }
                        }
                        return (false, None);
                    }
                }
            }

            // 4. Clique dentro de Janelas (arrasto, controles ou botões internos)
            let mut clicked_win_idx = None;
            for i in (0..MAX_WINDOWS).rev() {
                let win_idx = wm.order[i];
                if let Some(win) = wm.windows[win_idx] {
                    if !win.is_open || win.is_minimized { continue; }

                    if mouse_x >= win.x && mouse_x < win.x + win.width &&
                       mouse_y >= win.y && mouse_y < win.y + win.height {

                        let btn_y = win.y + 4;

                        // 1. Botão de minimizar [-]
                        let btn_min_x = win.x + win.width - 64;
                        if mouse_x >= btn_min_x && mouse_x < btn_min_x + 16 &&
                           mouse_y >= btn_y && mouse_y < btn_y + 16 {
                            if let Some(w) = wm.windows[win_idx].as_mut() {
                                w.is_minimized = true;
                            }
                            return (false, None);
                        }

                        // 2. Botão de maximizar/restaurar [+] / [^]
                        let btn_max_x = win.x + win.width - 44;
                        if mouse_x >= btn_max_x && mouse_x < btn_max_x + 16 &&
                           mouse_y >= btn_y && mouse_y < btn_y + 16 {
                            if let Some(w) = wm.windows[win_idx].as_mut() {
                                if w.is_maximized {
                                    w.x = w.saved_x;
                                    w.y = w.saved_y;
                                    w.width = w.saved_width;
                                    w.height = w.saved_height;
                                    w.is_maximized = false;
                                } else {
                                    w.saved_x = w.x;
                                    w.saved_y = w.y;
                                    w.saved_width = w.width;
                                    w.saved_height = w.height;
                                    w.x = 0;
                                    w.y = 26;
                                    w.width = DEFAULT_WIDTH;
                                    w.height = DEFAULT_HEIGHT - 26;
                                    w.is_maximized = true;
                                }
                            }
                            return (false, None);
                        }

                        // 3. Botão de fechar [X]
                        let btn_close_x = win.x + win.width - 24;
                        if mouse_x >= btn_close_x && mouse_x < btn_close_x + 16 &&
                           mouse_y >= btn_y && mouse_y < btn_y + 16 {
                            if let Some(w) = wm.windows[win_idx].as_mut() {
                                w.is_open = false;
                                w.is_minimized = false;
                            }
                            return (false, None);
                        }

                        // Botão de ação na janela do Terminal (Window 1)
                        if win_idx == 1 {
                            let btn_action_y = win.y + win.height - 34;
                            if mouse_x >= win.x + 8 && mouse_x <= win.x + 228 &&
                               mouse_y >= btn_action_y && mouse_y <= btn_action_y + 24 {
                                spawn_user_app();
                                bring_to_front(1);
                                return (false, None);
                            }
                        }

                        // Botões de ação na janela do Gerenciador de Arquivos (Window 3)
                        if win_idx == 3 {
                            if wm.preview_active {
                                let p_x = win.x + 12;
                                let p_y = win.y + 72;
                                let p_w = win.width - 24;
                                let btn_c_w = 76;
                                let btn_c_x = p_x + p_w - btn_c_w - 4;
                                let btn_dp_w = 64;
                                let btn_dp_x = btn_c_x - btn_dp_w - 4;

                                // Clique no botão [Excluir] do Preview
                                if mouse_x >= btn_dp_x && mouse_x <= btn_dp_x + btn_dp_w &&
                                   mouse_y >= p_y + 2 && mouse_y <= p_y + 18 {
                                    let mut path_buf = [0u8; 32];
                                    let prefix = b"/disk/";
                                    path_buf[..6].copy_from_slice(prefix);
                                    let n_len = wm.preview_filename_len.min(24);
                                    path_buf[6..6 + n_len].copy_from_slice(&wm.preview_filename[..n_len]);

                                    if let Ok(path_str) = core::str::from_utf8(&path_buf[..6 + n_len]) {
                                        match crate::vfs::remove(path_str) {
                                            Ok(()) => {
                                                let _ = crate::vfs::sync_all();
                                                wm.refresh_file_cache();
                                                wm.preview_active = false;
                                                let mut msg_buf = [0u8; 48];
                                                let pfx = "[OK] Removido: ";
                                                msg_buf[..pfx.len()].copy_from_slice(pfx.as_bytes());
                                                let mut pos = pfx.len();
                                                msg_buf[pos..pos + n_len].copy_from_slice(&wm.preview_filename[..n_len]);
                                                pos += n_len;
                                                if let Ok(msg) = core::str::from_utf8(&msg_buf[..pos]) {
                                                    wm.terminal.push_line(msg, 0x0022_C55E);
                                                }
                                            }
                                            Err(_) => {
                                                wm.terminal.push_line("[ERRO] Falha ao excluir arquivo.", 0x00F8_7171);
                                            }
                                        }
                                    }
                                    return (false, None);
                                }

                                // Clique no botão [X Fechar] do Preview
                                if mouse_x >= btn_c_x && mouse_x <= btn_c_x + btn_c_w &&
                                   mouse_y >= p_y + 2 && mouse_y <= p_y + 18 {
                                    wm.preview_active = false;
                                    return (false, None);
                                }
                            } else {
                                let files_box_x = win.x + 12;
                                let files_box_y = win.y + 74;
                                let files_box_w = win.width - 24;

                                // Botão [+ Novo] no header do box de arquivos
                                let btn_new_w = 60;
                                let btn_rf_w = 88;
                                let btn_rf_x = files_box_x + files_box_w - btn_rf_w - 8;
                                let btn_new_x = btn_rf_x - btn_new_w - 6;

                                if mouse_x >= btn_new_x && mouse_x <= btn_new_x + btn_new_w &&
                                   mouse_y >= files_box_y + 6 && mouse_y <= files_box_y + 24 {
                                    let candidates = [
                                        ("/disk/NOTA.TXT", "NOTA.TXT"),
                                        ("/disk/DOC.TXT", "DOC.TXT"),
                                        ("/disk/IDEIA.TXT", "IDEIA.TXT"),
                                        ("/disk/LOG.TXT", "LOG.TXT"),
                                    ];
                                    let mut created = false;
                                    for (path, name) in &candidates {
                                        if crate::vfs::stat(path).is_err() {
                                            if crate::vfs::create(path).is_ok() {
                                                let initial = b"Arquivo criado via Aladdin Desktop 7.0 no Jinn OS!\nArmazenamento FAT32 + Cache LRU ativo.\n";
                                                let _ = crate::vfs::write_file(path, initial);
                                                let _ = crate::vfs::sync_all();
                                                wm.refresh_file_cache();
                                                let mut msg_buf = [0u8; 48];
                                                let pfx = "[OK] Criado: ";
                                                msg_buf[..pfx.len()].copy_from_slice(pfx.as_bytes());
                                                let mut pos = pfx.len();
                                                let n_bytes = name.as_bytes();
                                                msg_buf[pos..pos + n_bytes.len()].copy_from_slice(n_bytes);
                                                pos += n_bytes.len();
                                                if let Ok(msg) = core::str::from_utf8(&msg_buf[..pos]) {
                                                    wm.terminal.push_line(msg, 0x0022_C55E);
                                                }
                                                created = true;
                                                break;
                                            }
                                        }
                                    }
                                    if !created {
                                        wm.terminal.push_line("[AVISO] Limite de novos arquivos atingido.", 0x00F5_9E0B);
                                    }
                                    return (false, None);
                                }

                                // Botão [Atualizar] no header do box de arquivos
                                if mouse_x >= btn_rf_x && mouse_x <= btn_rf_x + btn_rf_w &&
                                   mouse_y >= files_box_y + 6 && mouse_y <= files_box_y + 24 {
                                    wm.refresh_file_cache();
                                    return (false, None);
                                }

                                // Botões nos arquivos em cache
                                for i in 0..wm.file_cache_count.min(3) {
                                    let item_y = files_box_y + 32 + (i * 22);
                                    let btn_w = 64;
                                    let btn_h = 16;
                                    let btn_x = files_box_x + files_box_w - btn_w - 8;

                                    let (is_elf, is_txt, n_len, entry_name) = {
                                        let e = &wm.file_cache[i];
                                        (e.is_elf, e.is_txt, e.name_len.min(24), e.name)
                                    };

                                    // Botão Excluir (Del) para arquivos de texto
                                    if is_txt {
                                        let btn_del_w = 40;
                                        let btn_del_x = btn_x - btn_del_w - 6;
                                        if mouse_x >= btn_del_x && mouse_x <= btn_del_x + btn_del_w &&
                                           mouse_y >= item_y - 1 && mouse_y <= item_y + btn_h {
                                            let mut path_buf = [0u8; 32];
                                            let prefix = b"/disk/";
                                            path_buf[..6].copy_from_slice(prefix);
                                            path_buf[6..6 + n_len].copy_from_slice(&entry_name[..n_len]);

                                            if let Ok(path_str) = core::str::from_utf8(&path_buf[..6 + n_len]) {
                                                match crate::vfs::remove(path_str) {
                                                    Ok(()) => {
                                                        let _ = crate::vfs::sync_all();
                                                        wm.refresh_file_cache();
                                                        let mut msg_buf = [0u8; 48];
                                                        let pfx = "[OK] Removido: ";
                                                        msg_buf[..pfx.len()].copy_from_slice(pfx.as_bytes());
                                                        let mut pos = pfx.len();
                                                        msg_buf[pos..pos + n_len].copy_from_slice(&entry_name[..n_len]);
                                                        pos += n_len;
                                                        if let Ok(msg) = core::str::from_utf8(&msg_buf[..pos]) {
                                                            wm.terminal.push_line(msg, 0x0022_C55E);
                                                        }
                                                    }
                                                    Err(_) => {
                                                        wm.terminal.push_line("[ERRO] Falha ao remover arquivo.", 0x00F8_7171);
                                                    }
                                                }
                                            }
                                            return (false, None);
                                        }
                                    }

                                    // Botão Principal (Rodar ou Ler VFS)
                                    if mouse_x >= btn_x && mouse_x <= btn_x + btn_w &&
                                       mouse_y >= item_y - 1 && mouse_y <= item_y + btn_h {
                                        if is_elf {
                                            spawn_user_app();
                                            bring_to_front(1);
                                            return (false, None);
                                        } else if is_txt {
                                            let mut preview = [0u8; 256];
                                            let mut path_buf = [0u8; 32];
                                            let prefix = b"/disk/";
                                            path_buf[..6].copy_from_slice(prefix);
                                            path_buf[6..6 + n_len].copy_from_slice(&entry_name[..n_len]);

                                            if let Ok(path_str) = core::str::from_utf8(&path_buf[..6 + n_len]) {
                                                let n = crate::vfs::read_file(path_str, &mut preview).unwrap_or(0);
                                                wm.preview_content = preview;
                                                wm.preview_content_len = n;
                                                wm.preview_filename[..n_len].copy_from_slice(&entry_name[..n_len]);
                                                wm.preview_filename_len = n_len;
                                                wm.preview_active = true;
                                            }
                                            return (false, None);
                                        }
                                    }
                                }
                            }
                        }

                        // Barra de título: inicia arrasto (se não estiver maximizada)
                        if mouse_y < win.y + 24 {
                            if let Some(w) = wm.windows[win_idx].as_mut() {
                                if !w.is_maximized {
                                    w.is_dragging = true;
                                    w.drag_offset_x = mouse_x - win.x;
                                    w.drag_offset_y = mouse_y - win.y;
                                }
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
        if !wm.file_cache_valid {
            wm.refresh_file_cache();
        }
    }

    let (mut last_mx, mut last_my, mut last_btn) = crate::interrupts::ps2_mouse::get_state();
    let mut last_ticks = crate::timer::ticks();

    // Render inicial
    unsafe { erase_cursor_bb(); }
    render_desktop_scene(last_mx, last_my);
    unsafe { draw_cursor_bb(last_mx, last_my); }
    swap_buffers();

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

        // 2. Processa caracteres do buffer de teclado para o terminal interativo
        let mut key_pressed = false;
        while let Some(ch) = crate::keyboard_buffer::pop() {
            if ch == 0x1B { // ASCII ESC
                unsafe { (*core::ptr::addr_of_mut!(WM)).should_exit = true; }
                break;
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
        unsafe {
            if (*core::ptr::addr_of!(WM)).should_exit {
                break;
            }
        }

        // 3. Lê coordenadas e botões atuais do mouse
        let (mx, my, buttons) = crate::interrupts::ps2_mouse::get_state();
        let current_ticks = crate::timer::ticks();

        let (dragging_any, _drag_damage) = handle_mouse(mx, my, buttons);

        let mouse_moved = (mx != last_mx) || (my != last_my);
        let button_changed = buttons != last_btn;
        let time_changed = current_ticks.saturating_sub(last_ticks) >= 50; // atualiza telemetria a cada 500ms

        let need_scene_redraw = button_changed || key_pressed || dragging_any || time_changed;

        if need_scene_redraw {
            // A cena precisa ser renderizada novamente (janelas, botões, digitação ou tempo)
            unsafe { erase_cursor_bb(); }
            render_desktop_scene(mx, my);
            unsafe { draw_cursor_bb(mx, my); }
            swap_buffers();

            last_mx = mx;
            last_my = my;
            last_btn = buttons;
            last_ticks = current_ticks;
        } else if mouse_moved {
            // VIA RÁPIDA DE CURSOR (< 5 µs):
            // Não redesenha o desktop nem toca nos discos!
            unsafe {
                let old_box = erase_cursor_bb();
                let new_box = draw_cursor_bb(mx, my);
                let damage = match old_box {
                    Some(ob) => ob.union(&new_box),
                    None => new_box,
                };
                swap_buffer_rect(&damage);
            }
            last_mx = mx;
            last_my = my;
        } else {
            // Em repouso: cede ciclos de CPU para tarefas preemptivas e idle halt
            crate::scheduler::yield_now();
        }
    }

    // Ao sair, limpa a tela para restaurar o console de texto VGA
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
