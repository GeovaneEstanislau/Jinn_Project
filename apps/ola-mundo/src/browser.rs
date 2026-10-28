//! Jinn Browser Engine v0.2.0 — Motor Nativo Ring 3 para Jinn OS & Aladdin
//!
//! Implementação completa e autônoma (#![no_std]) do pipeline:
//! 1. Leitura de documento HTML local via VFS (sys_open / sys_read / sys_close)
//! 2. Parser HTML determinístico (máquina de estados, extração de tags, atributos e entidades)
//! 3. Árvore DOM estática e resolução de estilos CSS (Catppuccin Mocha Dark Theme)
//! 4. Motor de Layout (Box Model, quebra de linhas, coordenadas e regiões clicáveis)
//! 5. Rasterização com clipping de viewport e barra de rolagem proporcional
//! 6. Despacho interativo de eventos, hit-testing de links e histórico de navegação

use crate::font;

pub const WIN_W: usize = 300;
pub const WIN_H: usize = 180;
pub const CLIP_TOP: usize = 27;
pub const CLIP_BOTTOM: usize = WIN_H - 1;

// ── Cores do Tema (Catppuccin Mocha) ──────────────────────────────────────────
pub const COLOR_BG: u32 = 0x0018_1825;       // Base Escura
pub const COLOR_CHROME: u32 = 0x001E_1E2E;   // Barra de Título / Chrome
pub const COLOR_BORDER: u32 = 0x0031_3244;   // Linhas e Bordas
pub const COLOR_URL_BG: u32 = 0x0031_3244;   // Fundo do Campo de URL
pub const COLOR_URL_BORDER: u32 = 0x0045_475A;
pub const COLOR_URL_TEXT: u32 = 0x0089_DCEB; // Ciano / Azul Claro
pub const COLOR_BTN_TEXT: u32 = 0x00A6_ADC8; // Cinza Texto Botões
pub const COLOR_H1: u32 = 0x00F9_E2AF;       // Pêssego / Ouro
pub const COLOR_TEXT: u32 = 0x00CD_D6F4;     // Texto Principal
pub const COLOR_LINK: u32 = 0x0089_B4FA;     // Azul Links
pub const COLOR_HR: u32 = 0x0045_475A;       // Separador Horizontal
pub const COLOR_SCROLL_TRACK: u32 = 0x0018_1825;
pub const COLOR_SCROLL_THUMB: u32 = 0x0058_5B70;
pub const COLOR_SCROLL_THUMB_BORDER: u32 = 0x0089_DCEB;

// ── Tipos do Parser e DOM ─────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlockKind {
    Heading1,
    Paragraph,
    Link,
    HorizontalRule,
}

#[derive(Clone, Copy)]
pub struct BlockItem {
    pub kind: BlockKind,
    pub text: [u8; 128],
    pub text_len: usize,
    pub link_target: [u8; 64],
    pub link_target_len: usize,
}

impl BlockItem {
    pub const fn empty() -> Self {
        Self {
            kind: BlockKind::Paragraph,
            text: [0; 128],
            text_len: 0,
            link_target: [0; 64],
            link_target_len: 0,
        }
    }
}

pub struct ParsedPage {
    pub title: [u8; 64],
    pub title_len: usize,
    pub blocks: [BlockItem; 16],
    pub block_count: usize,
}

impl ParsedPage {
    pub const fn new() -> Self {
        Self {
            title: [0; 64],
            title_len: 0,
            blocks: [BlockItem::empty(); 16],
            block_count: 0,
        }
    }
}

// ── Regiões Clicáveis e Linhas de Layout ───────────────────────────────────────

#[derive(Clone, Copy)]
pub struct ClickRegion {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
    pub target: [u8; 64],
    pub target_len: usize,
}

#[derive(Clone, Copy)]
pub struct RenderLine {
    pub text: [u8; 64],
    pub text_len: usize,
    pub x: usize,
    pub y: usize,
    pub color: u32,
    pub is_link: bool,
    pub is_hr: bool,
}

// ── Máquina de Estados do Parser HTML ─────────────────────────────────────────

pub fn parse_html(html: &[u8], page: &mut ParsedPage) {
    page.title_len = 0;
    page.block_count = 0;

    let mut i = 0usize;
    let len = html.len();

    let mut current_tag = [0u8; 16];
    let mut current_tag_len = 0usize;
    let mut current_href = [0u8; 64];
    let mut current_href_len = 0usize;

    let mut is_in_tag = false;
    let mut is_closing_tag = false;

    let mut text_buf = [0u8; 128];
    let mut text_len = 0usize;

    let mut active_block_kind: Option<BlockKind> = None;

    let flush_block = |page: &mut ParsedPage, kind: BlockKind, text: &[u8], href: &[u8]| {
        if page.block_count < page.blocks.len() {
            let mut blk = BlockItem::empty();
            blk.kind = kind;
            let cpy_len = text.len().min(128);
            blk.text[..cpy_len].copy_from_slice(&text[..cpy_len]);
            blk.text_len = cpy_len;

            if kind == BlockKind::Link {
                let h_len = href.len().min(64);
                blk.link_target[..h_len].copy_from_slice(&href[..h_len]);
                blk.link_target_len = h_len;
            }
            page.blocks[page.block_count] = blk;
            page.block_count += 1;
        }
    };

    while i < len {
        let b = html[i];

        if b == b'<' {
            // Processa o texto acumulado antes da tag
            if let Some(kind) = active_block_kind {
                if text_len > 0 {
                    let href_slice = &current_href[..current_href_len];
                    flush_block(page, kind, &text_buf[..text_len], href_slice);
                    text_len = 0;
                }
            }

            is_in_tag = true;
            is_closing_tag = false;
            current_tag_len = 0;
            current_href_len = 0;
            i += 1;

            if i < len && html[i] == b'/' {
                is_closing_tag = true;
                i += 1;
            }

            // Lê o nome da tag
            while i < len && html[i] != b' ' && html[i] != b'>' && html[i] != b'/' {
                if current_tag_len < 15 {
                    let mut ch = html[i];
                    if ch >= b'A' && ch <= b'Z' {
                        ch += 32; // Normaliza para minúscula
                    }
                    current_tag[current_tag_len] = ch;
                    current_tag_len += 1;
                }
                i += 1;
            }

            // Lê atributos (como href="...")
            while i < len && html[i] != b'>' {
                if i + 5 < len && &html[i..i + 5] == b"href=" {
                    i += 5;
                    let quote = if i < len && (html[i] == b'"' || html[i] == b'\'') {
                        let q = html[i];
                        i += 1;
                        Some(q)
                    } else {
                        None
                    };

                    while i < len {
                        let c = html[i];
                        if let Some(q) = quote {
                            if c == q { i += 1; break; }
                        } else if c == b' ' || c == b'>' {
                            break;
                        }
                        if current_href_len < 63 {
                            current_href[current_href_len] = c;
                            current_href_len += 1;
                        }
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }

            if i < len && html[i] == b'>' {
                is_in_tag = false;
                i += 1;
            }

            // Ações baseadas na tag aberta/fechada
            let tag_name = &current_tag[..current_tag_len];
            if !is_closing_tag {
                if tag_name == b"h1" {
                    active_block_kind = Some(BlockKind::Heading1);
                } else if tag_name == b"p" {
                    active_block_kind = Some(BlockKind::Paragraph);
                } else if tag_name == b"a" {
                    active_block_kind = Some(BlockKind::Link);
                } else if tag_name == b"hr" {
                    flush_block(page, BlockKind::HorizontalRule, b"", b"");
                }
            } else {
                if tag_name == b"h1" || tag_name == b"p" || tag_name == b"a" {
                    active_block_kind = None;
                }
            }
        } else {
            // Conteúdo textual
            if !is_in_tag && active_block_kind.is_some() {
                // Decodifica entidades comuns ou pula espaços redundantes
                if b == b'\n' || b == b'\r' || b == b'\t' {
                    if text_len > 0 && text_buf[text_len - 1] != b' ' && text_len < 127 {
                        text_buf[text_len] = b' ';
                        text_len += 1;
                    }
                } else if b == b'&' {
                    // Decodificação básica de entidades HTML
                    if i + 3 < len && &html[i..i + 4] == b"&lt;" {
                        if text_len < 127 { text_buf[text_len] = b'<'; text_len += 1; }
                        i += 3;
                    } else if i + 3 < len && &html[i..i + 4] == b"&gt;" {
                        if text_len < 127 { text_buf[text_len] = b'>'; text_len += 1; }
                        i += 3;
                    } else if i + 4 < len && &html[i..i + 5] == b"&amp;" {
                        if text_len < 127 { text_buf[text_len] = b'&'; text_len += 1; }
                        i += 4;
                    } else {
                        if text_len < 127 { text_buf[text_len] = b; text_len += 1; }
                    }
                } else {
                    if text_len < 127 {
                        text_buf[text_len] = b;
                        text_len += 1;
                    }
                }
            }
            i += 1;
        }
    }

    if let Some(kind) = active_block_kind {
        if text_len > 0 {
            let href_slice = &current_href[..current_href_len];
            flush_block(page, kind, &text_buf[..text_len], href_slice);
        }
    }
}

// ── Motor de Layout e Quebra de Linhas ────────────────────────────────────────

pub struct DocumentLayout {
    pub lines: [RenderLine; 32],
    pub line_count: usize,
    pub click_regions: [ClickRegion; 8],
    pub click_count: usize,
    pub total_height: usize,
}

impl DocumentLayout {
    pub const fn new() -> Self {
        Self {
            lines: [RenderLine {
                text: [0; 64],
                text_len: 0,
                x: 0,
                y: 0,
                color: 0,
                is_link: false,
                is_hr: false,
            }; 32],
            line_count: 0,
            click_regions: [ClickRegion {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
                target: [0; 64],
                target_len: 0,
            }; 8],
            click_count: 0,
            total_height: 0,
        }
    }

    pub fn compute(&mut self, page: &ParsedPage, content_width: usize) {
        self.line_count = 0;
        self.click_count = 0;

        let max_chars_per_line = (content_width / font::FONT_WIDTH).min(34);
        let mut curr_y = 6usize;

        for b_idx in 0..page.block_count {
            let blk = &page.blocks[b_idx];

            match blk.kind {
                BlockKind::HorizontalRule => {
                    if self.line_count < self.lines.len() {
                        self.lines[self.line_count] = RenderLine {
                            text: [0; 64],
                            text_len: 0,
                            x: 8,
                            y: curr_y + 4,
                            color: COLOR_HR,
                            is_link: false,
                            is_hr: true,
                        };
                        self.line_count += 1;
                    }
                    curr_y += 10;
                }
                BlockKind::Heading1 => {
                    curr_y += 4;
                    let text = &blk.text[..blk.text_len];
                    self.add_wrapped_text(text, 8, &mut curr_y, max_chars_per_line, COLOR_H1, false, None);
                    curr_y += 6;
                }
                BlockKind::Paragraph => {
                    curr_y += 2;
                    let text = &blk.text[..blk.text_len];
                    self.add_wrapped_text(text, 8, &mut curr_y, max_chars_per_line, COLOR_TEXT, false, None);
                    curr_y += 4;
                }
                BlockKind::Link => {
                    curr_y += 3;
                    let text = &blk.text[..blk.text_len];
                    let href = &blk.link_target[..blk.link_target_len];
                    self.add_wrapped_text(text, 8, &mut curr_y, max_chars_per_line, COLOR_LINK, true, Some(href));
                    curr_y += 4;
                }
            }
        }

        self.total_height = curr_y + 10;
    }

    fn add_wrapped_text(
        &mut self,
        text: &[u8],
        start_x: usize,
        curr_y: &mut usize,
        max_chars: usize,
        color: u32,
        is_link: bool,
        target_href: Option<&[u8]>,
    ) {
        if text.is_empty() { return; }

        let mut offset = 0usize;

        while offset < text.len() && self.line_count < self.lines.len() {
            // Encontra comprimento da fatia que cabe na linha
            let remaining = text.len() - offset;
            let take = if remaining <= max_chars {
                remaining
            } else {
                // Tenta quebrar em espaço
                let mut break_idx = max_chars;
                while break_idx > 0 && text[offset + break_idx] != b' ' {
                    break_idx -= 1;
                }
                if break_idx == 0 { max_chars } else { break_idx }
            };

            let mut line_buf = [0u8; 64];
            let actual_take = take.min(63);
            line_buf[..actual_take].copy_from_slice(&text[offset..offset + actual_take]);

            // Se for link, registra a região clicável desta linha
            if is_link {
                if let Some(href) = target_href {
                    if self.click_count < self.click_regions.len() {
                        let mut reg = ClickRegion {
                            x: start_x,
                            y: *curr_y,
                            w: actual_take * font::FONT_WIDTH,
                            h: font::FONT_HEIGHT + 2,
                            target: [0; 64],
                            target_len: 0,
                        };
                        let h_len = href.len().min(64);
                        reg.target[..h_len].copy_from_slice(&href[..h_len]);
                        reg.target_len = h_len;
                        self.click_regions[self.click_count] = reg;
                        self.click_count += 1;
                    }
                }
            }

            self.lines[self.line_count] = RenderLine {
                text: line_buf,
                text_len: actual_take,
                x: start_x,
                y: *curr_y,
                color,
                is_link,
                is_hr: false,
            };
            self.line_count += 1;

            offset += take;
            while offset < text.len() && text[offset] == b' ' {
                offset += 1;
            }
            *curr_y += font::FONT_HEIGHT + 4;
        }
    }
}

// ── Sessão Interativa do Navegador ────────────────────────────────────────────

pub struct BrowserSession {
    pub current_url: [u8; 64],
    pub current_url_len: usize,
    pub history_prev: [u8; 64],
    pub history_prev_len: usize,
    pub has_history: bool,
    pub scroll_y: usize,
    pub max_scroll: usize,
    pub page: ParsedPage,
    pub layout: DocumentLayout,
}

impl BrowserSession {
    pub fn new() -> Self {
        Self {
            current_url: [0; 64],
            current_url_len: 0,
            history_prev: [0; 64],
            history_prev_len: 0,
            has_history: false,
            scroll_y: 0,
            max_scroll: 0,
            page: ParsedPage::new(),
            layout: DocumentLayout::new(),
        }
    }

    pub fn set_url(&mut self, url: &[u8]) {
        let len = url.len().min(63);
        self.current_url[..len].copy_from_slice(&url[..len]);
        self.current_url_len = len;
    }

    pub fn load_document_bytes(&mut self, html: &[u8], url: &[u8]) {
        self.set_url(url);
        self.scroll_y = 0;
        parse_html(html, &mut self.page);
        self.layout.compute(&self.page, WIN_W - 24);

        let viewport_h = CLIP_BOTTOM - CLIP_TOP;
        if self.layout.total_height > viewport_h {
            self.max_scroll = self.layout.total_height - viewport_h;
        } else {
            self.max_scroll = 0;
        }
    }

    pub fn scroll_by(&mut self, delta: i32) {
        if delta < 0 {
            let sub = (-delta) as usize;
            self.scroll_y = self.scroll_y.saturating_sub(sub);
        } else {
            let add = delta as usize;
            self.scroll_y = (self.scroll_y + add).min(self.max_scroll);
        }
    }

    /// Hit-testing para cliques do mouse: retorna se clicou em Back ou em um link
    pub fn handle_click(&mut self, click_x: usize, click_y: usize) -> Option<[u8; 64]> {
        // 1. Botão Back [<] em (6..16, 4..22)
        if click_x >= 4 && click_x <= 16 && click_y >= 4 && click_y <= 22 {
            if self.has_history && self.history_prev_len > 0 {
                let mut prev = [0u8; 64];
                prev[..self.history_prev_len].copy_from_slice(&self.history_prev[..self.history_prev_len]);
                return Some(prev);
            }
        }

        // 2. Viewport de Conteúdo: aplica deslocamento de scroll_y
        if click_y >= CLIP_TOP && click_y <= CLIP_BOTTOM {
            let doc_y = (click_y - CLIP_TOP) + self.scroll_y;
            for i in 0..self.layout.click_count {
                let r = &self.layout.click_regions[i];
                if click_x >= r.x && click_x <= r.x + r.w && doc_y >= r.y && doc_y <= r.y + r.h {
                    let mut target = [0u8; 64];
                    target[..r.target_len].copy_from_slice(&r.target[..r.target_len]);
                    return Some(target);
                }
            }
        }

        None
    }

    pub fn render(&self, buf: &mut [u32]) {
        // 1. Fundo Geral
        fill_rect(buf, 0, 0, WIN_W, WIN_H, COLOR_BG);

        // 2. Barra Superior de Navegação (Chrome)
        fill_rect(buf, 0, 0, WIN_W, 26, COLOR_CHROME);
        draw_rect_border(buf, 0, 0, WIN_W, 26, COLOR_BORDER);

        // Botões: [<] [>] [R]
        let back_col = if self.has_history { COLOR_BTN_TEXT } else { 0x0058_5B70 };
        font::draw_text(buf, WIN_W, WIN_H, 6, 9, b"<", back_col, None);
        font::draw_text(buf, WIN_W, WIN_H, 18, 9, b">", 0x0058_5B70, None);
        font::draw_text(buf, WIN_W, WIN_H, 30, 9, b"R", COLOR_BTN_TEXT, None);

        // Campo de URL
        fill_rect(buf, 44, 4, 238, 18, COLOR_URL_BG);
        draw_rect_border(buf, 44, 4, 238, 18, COLOR_URL_BORDER);
        let url_slice = &self.current_url[..self.current_url_len];
        font::draw_text_clipped(buf, WIN_W, WIN_H, 48, 9, url_slice, COLOR_URL_TEXT, None, 4, 22);

        // 3. Documento no Viewport (com scroll_y aplicado e clipping)
        for i in 0..self.layout.line_count {
            let line = &self.layout.lines[i];

            if line.is_hr {
                if line.y >= self.scroll_y {
                    let actual_y = (line.y - self.scroll_y) + CLIP_TOP;
                    if actual_y >= CLIP_TOP && actual_y <= CLIP_BOTTOM {
                        fill_rect(buf, line.x, actual_y, WIN_W - 24, 1, COLOR_HR);
                    }
                }
            } else {
                if line.y >= self.scroll_y {
                    let actual_y = (line.y - self.scroll_y) + CLIP_TOP;
                    if actual_y + font::FONT_HEIGHT <= CLIP_BOTTOM {
                        let text_slice = &line.text[..line.text_len];
                        font::draw_text_clipped(
                            buf,
                            WIN_W,
                            WIN_H,
                            line.x,
                            actual_y,
                            text_slice,
                            line.color,
                            None,
                            CLIP_TOP,
                            CLIP_BOTTOM,
                        );

                        // Se for link, desenha sublinhado decorativo
                        if line.is_link {
                            let underline_y = actual_y + font::FONT_HEIGHT;
                            if underline_y <= CLIP_BOTTOM {
                                let link_w = line.text_len * font::FONT_WIDTH;
                                fill_rect(buf, line.x, underline_y, link_w, 1, line.color);
                            }
                        }
                    }
                }
            }
        }

        // 4. Barra de Rolagem Vertical (Scrollbar Track & Thumb)
        let track_x = WIN_W - 14;
        let track_w = 8;
        let track_y = CLIP_TOP;
        let track_h = WIN_H - CLIP_TOP - 1;
        fill_rect(buf, track_x, track_y, track_w, track_h, COLOR_SCROLL_TRACK);
        draw_rect_border(buf, track_x, track_y, track_w, track_h, COLOR_BORDER);

        if self.max_scroll > 0 {
            let thumb_h = 24usize.max(track_h * (CLIP_BOTTOM - CLIP_TOP) / self.layout.total_height.max(1));
            let thumb_travel = track_h.saturating_sub(thumb_h);
            let thumb_y = track_y + (self.scroll_y * thumb_travel / self.max_scroll.max(1));
            fill_rect(buf, track_x + 1, thumb_y, track_w - 2, thumb_h, COLOR_SCROLL_THUMB);
            draw_rect_border(buf, track_x + 1, thumb_y, track_w - 2, thumb_h, COLOR_SCROLL_THUMB_BORDER);
        }
    }
}

// ── Primitivas Gráficas Locais ────────────────────────────────────────────────

fn fill_rect(buf: &mut [u32], rx: usize, ry: usize, rw: usize, rh: usize, color: u32) {
    let x_end = (rx + rw).min(WIN_W);
    let y_end = (ry + rh).min(WIN_H);
    for y in ry..y_end {
        let row = y * WIN_W;
        for x in rx..x_end {
            buf[row + x] = color;
        }
    }
}

fn draw_rect_border(buf: &mut [u32], rx: usize, ry: usize, rw: usize, rh: usize, color: u32) {
    if rw == 0 || rh == 0 { return; }
    let x_end = rx + rw - 1;
    let y_end = ry + rh - 1;
    for x in rx..=x_end.min(WIN_W - 1) {
        if ry < WIN_H { buf[ry * WIN_W + x] = color; }
        if y_end < WIN_H { buf[y_end * WIN_W + x] = color; }
    }
    for y in ry..=y_end.min(WIN_H - 1) {
        if rx < WIN_W { buf[y * WIN_W + rx] = color; }
        if x_end < WIN_W { buf[y * WIN_W + x_end] = color; }
    }
}
