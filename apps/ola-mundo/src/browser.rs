//! Jinn Browser Engine v0.3.0 — Motor Nativo Ring 3 para Jinn OS & Aladdin
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
    Heading2,
    Heading3,
    Paragraph,
    ListItem,
    Link,
    HorizontalRule,
}

#[derive(Clone, Copy)]
pub struct BlockItem {
    pub kind: BlockKind,
    pub text: [u8; 128],
    pub text_len: usize,
    pub link_target: [u8; 128],
    pub link_target_len: usize,
}

impl BlockItem {
    pub const fn empty() -> Self {
        Self {
            kind: BlockKind::Paragraph,
            text: [0; 128],
            text_len: 0,
            link_target: [0; 128],
            link_target_len: 0,
        }
    }
}

#[allow(dead_code)]
pub struct ParsedPage {
    pub title: [u8; 64],
    pub title_len: usize,
    pub blocks: [BlockItem; 32],
    pub block_count: usize,
}

impl ParsedPage {
    pub const fn new() -> Self {
        Self {
            title: [0; 64],
            title_len: 0,
            blocks: [BlockItem::empty(); 32],
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
    pub target: [u8; 128],
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

#[allow(unused_assignments)]
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
                } else if tag_name == b"h2" {
                    active_block_kind = Some(BlockKind::Heading2);
                } else if tag_name == b"h3" {
                    active_block_kind = Some(BlockKind::Heading3);
                } else if tag_name == b"p" || tag_name == b"div" {
                    active_block_kind = Some(BlockKind::Paragraph);
                } else if tag_name == b"li" {
                    active_block_kind = Some(BlockKind::ListItem);
                } else if tag_name == b"a" {
                    active_block_kind = Some(BlockKind::Link);
                } else if tag_name == b"hr" {
                    flush_block(page, BlockKind::HorizontalRule, b"", b"");
                }
            } else {
                if tag_name == b"h1" || tag_name == b"h2" || tag_name == b"h3" || tag_name == b"p" || tag_name == b"div" || tag_name == b"li" || tag_name == b"a" {
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
    pub lines: [RenderLine; 64],
    pub line_count: usize,
    pub click_regions: [ClickRegion; 16],
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
            }; 64],
            line_count: 0,
            click_regions: [ClickRegion {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
                target: [0; 128],
                target_len: 0,
            }; 16],
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
                BlockKind::Heading2 => {
                    curr_y += 3;
                    let text = &blk.text[..blk.text_len];
                    self.add_wrapped_text(text, 8, &mut curr_y, max_chars_per_line, 0x00F3_8BA8, false, None);
                    curr_y += 4;
                }
                BlockKind::Heading3 => {
                    curr_y += 2;
                    let text = &blk.text[..blk.text_len];
                    self.add_wrapped_text(text, 8, &mut curr_y, max_chars_per_line, 0x00A6_E3A1, false, None);
                    curr_y += 3;
                }
                BlockKind::Paragraph => {
                    curr_y += 2;
                    let text = &blk.text[..blk.text_len];
                    self.add_wrapped_text(text, 8, &mut curr_y, max_chars_per_line, COLOR_TEXT, false, None);
                    curr_y += 4;
                }
                BlockKind::ListItem => {
                    curr_y += 1;
                    let mut li_text = [0u8; 128];
                    li_text[0] = b'-';
                    li_text[1] = b' ';
                    let copy_len = blk.text_len.min(126);
                    li_text[2..2 + copy_len].copy_from_slice(&blk.text[..copy_len]);
                    self.add_wrapped_text(&li_text[..copy_len + 2], 8, &mut curr_y, max_chars_per_line, COLOR_TEXT, false, None);
                    curr_y += 2;
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
                            target: [0; 128],
                            target_len: 0,
                        };
                        let h_len = href.len().min(128);
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

// ── Ações Interativas do Navegador ──────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserAction {
    None,
    Back,
    Forward,
    Reload,
    NavigateTarget([u8; 128], usize),
    FocusAddress,
    Scroll(i32),
}

// ── Sessão Interativa do Navegador ────────────────────────────────────────────

pub const MAX_HISTORY: usize = 8;

pub struct BrowserSession {
    pub current_url: [u8; 128],
    pub current_url_len: usize,
    pub history_back: [[u8; 128]; MAX_HISTORY],
    pub history_back_lens: [usize; MAX_HISTORY],
    pub history_back_count: usize,
    pub history_fwd: [[u8; 128]; MAX_HISTORY],
    pub history_fwd_lens: [usize; MAX_HISTORY],
    pub history_fwd_count: usize,
    pub is_address_focused: bool,
    pub address_selected: bool,
    pub address_edit: [u8; 128],
    pub address_edit_len: usize,
    pub scroll_y: usize,
    pub max_scroll: usize,
    pub page: ParsedPage,
    pub layout: DocumentLayout,
}

impl BrowserSession {
    pub fn new() -> Self {
        Self {
            current_url: [0; 128],
            current_url_len: 0,
            history_back: [[0; 128]; MAX_HISTORY],
            history_back_lens: [0; MAX_HISTORY],
            history_back_count: 0,
            history_fwd: [[0; 128]; MAX_HISTORY],
            history_fwd_lens: [0; MAX_HISTORY],
            history_fwd_count: 0,
            is_address_focused: false,
            address_selected: false,
            address_edit: [0; 128],
            address_edit_len: 0,
            scroll_y: 0,
            max_scroll: 0,
            page: ParsedPage::new(),
            layout: DocumentLayout::new(),
        }
    }

    pub fn can_go_back(&self) -> bool {
        self.history_back_count > 0
    }

    pub fn can_go_forward(&self) -> bool {
        self.history_fwd_count > 0
    }

    pub fn set_url(&mut self, url: &[u8]) {
        let len = url.len().min(127);
        self.current_url[..len].copy_from_slice(&url[..len]);
        self.current_url[len] = 0;
        self.current_url_len = len;
    }

    pub fn push_back_current(&mut self) {
        if self.current_url_len == 0 {
            return;
        }
        if self.history_back_count < MAX_HISTORY {
            let idx = self.history_back_count;
            self.history_back[idx] = self.current_url;
            self.history_back_lens[idx] = self.current_url_len;
            self.history_back_count += 1;
        } else {
            for i in 0..MAX_HISTORY - 1 {
                self.history_back[i] = self.history_back[i + 1];
                self.history_back_lens[i] = self.history_back_lens[i + 1];
            }
            let idx = MAX_HISTORY - 1;
            self.history_back[idx] = self.current_url;
            self.history_back_lens[idx] = self.current_url_len;
        }
    }

    pub fn push_fwd_current(&mut self) {
        if self.current_url_len == 0 {
            return;
        }
        if self.history_fwd_count < MAX_HISTORY {
            let idx = self.history_fwd_count;
            self.history_fwd[idx] = self.current_url;
            self.history_fwd_lens[idx] = self.current_url_len;
            self.history_fwd_count += 1;
        } else {
            for i in 0..MAX_HISTORY - 1 {
                self.history_fwd[i] = self.history_fwd[i + 1];
                self.history_fwd_lens[i] = self.history_fwd_lens[i + 1];
            }
            let idx = MAX_HISTORY - 1;
            self.history_fwd[idx] = self.current_url;
            self.history_fwd_lens[idx] = self.current_url_len;
        }
    }

    /// Executa navegação Voltar (<). Salva URL atual no histórico Forward e retorna URL anterior.
    pub fn go_back(&mut self) -> Option<([u8; 128], usize)> {
        if !self.can_go_back() {
            return None;
        }
        self.push_fwd_current();
        self.history_back_count -= 1;
        let idx = self.history_back_count;
        let target = self.history_back[idx];
        let len = self.history_back_lens[idx];
        Some((target, len))
    }

    /// Executa navegação Avançar (>). Salva URL atual no histórico Back e retorna URL avançada.
    pub fn go_forward(&mut self) -> Option<([u8; 128], usize)> {
        if !self.can_go_forward() {
            return None;
        }
        self.push_back_current();
        self.history_fwd_count -= 1;
        let idx = self.history_fwd_count;
        let target = self.history_fwd[idx];
        let len = self.history_fwd_lens[idx];
        Some((target, len))
    }

    /// Inicia edição da barra de endereços
    pub fn focus_address(&mut self) {
        self.is_address_focused = true;
        self.address_selected = true;
        self.address_edit = self.current_url;
        self.address_edit_len = self.current_url_len;
    }

    /// Cancela edição da barra de endereços
    pub fn unfocus_address(&mut self) {
        self.is_address_focused = false;
        self.address_selected = false;
    }

    /// Processa pressionamento de tecla enquanto a barra de endereços está focada
    pub fn address_key(&mut self, key: u32) -> BrowserAction {
        if !self.is_address_focused {
            return BrowserAction::None;
        }

        // Enter (0x0A, 0x0D, ou scancode 0x1C)
        if key == 0x0A || key == 0x0D || key == 0x1C {
            self.is_address_focused = false;
            self.address_selected = false;
            if self.address_edit_len > 0 {
                return BrowserAction::NavigateTarget(self.address_edit, self.address_edit_len);
            }
            return BrowserAction::None;
        }

        // Backspace (0x08 ou scancode 0x0E)
        if key == 0x08 || key == 0x0E {
            if self.address_selected {
                self.address_selected = false;
                self.address_edit_len = 0;
                self.address_edit[0] = 0;
            } else if self.address_edit_len > 0 {
                self.address_edit_len -= 1;
                self.address_edit[self.address_edit_len] = 0;
            }
            return BrowserAction::None;
        }

        // Escape (0x1B ou scancode 0x01)
        if key == 0x1B || key == 0x01 {
            self.unfocus_address();
            return BrowserAction::None;
        }

        // Caracteres imprimíveis ASCII (32..=126)
        if (32..=126).contains(&key) {
            if self.address_selected {
                self.address_selected = false;
                self.address_edit[0] = key as u8;
                self.address_edit_len = 1;
                self.address_edit[1] = 0;
            } else if self.address_edit_len < 127 {
                self.address_edit[self.address_edit_len] = key as u8;
                self.address_edit_len += 1;
                self.address_edit[self.address_edit_len] = 0;
            }
            return BrowserAction::None;
        }

        BrowserAction::None
    }

    pub fn load_document_bytes(&mut self, html: &[u8], url: &[u8]) {
        self.set_url(url);
        self.scroll_y = 0;
        self.is_address_focused = false;
        self.address_selected = false;
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

    /// Hit-testing para cliques do mouse: despacha botões do chrome, barra de endereço, rolagem e links
    pub fn handle_click(&mut self, click_x: usize, click_y: usize) -> BrowserAction {
        // 1. Barra de ferramentas / Chrome (y: 3..24)
        if click_y >= 3 && click_y <= 24 {
            // Botão Back [<] em (4..16)
            if click_x >= 4 && click_x <= 16 {
                if self.can_go_back() {
                    return BrowserAction::Back;
                }
                return BrowserAction::None;
            }

            // Botão Forward [>] em (17..27)
            if click_x >= 17 && click_x <= 27 {
                if self.can_go_forward() {
                    return BrowserAction::Forward;
                }
                return BrowserAction::None;
            }

            // Botão Reload [R] em (28..40)
            if click_x >= 28 && click_x <= 40 {
                return BrowserAction::Reload;
            }

            // Barra de Endereço em (44..282)
            if click_x >= 44 && click_x <= 282 {
                self.focus_address();
                return BrowserAction::FocusAddress;
            }
        }

        // 2. Viewport de Conteúdo: aplica deslocamento de scroll_y
        if click_y >= CLIP_TOP && click_y <= CLIP_BOTTOM {
            // Clicar fora da barra de endereços cancela o foco
            if self.is_address_focused {
                self.unfocus_address();
            }

            // Se clicou na barra de rolagem (track_x = 286..294)
            if click_x >= WIN_W - 14 {
                let track_y = CLIP_TOP;
                let track_h = WIN_H - CLIP_TOP - 1;
                if self.max_scroll > 0 {
                    let thumb_h = 24usize.max(track_h * (CLIP_BOTTOM - CLIP_TOP) / self.layout.total_height.max(1));
                    let thumb_travel = track_h.saturating_sub(thumb_h);
                    let thumb_y = track_y + (self.scroll_y * thumb_travel / self.max_scroll.max(1));

                    if click_y < thumb_y {
                        return BrowserAction::Scroll(-40);
                    } else if click_y > thumb_y + thumb_h {
                        return BrowserAction::Scroll(40);
                    }
                }
                return BrowserAction::None;
            }

            // Hit-test em links do documento
            let doc_y = (click_y - CLIP_TOP) + self.scroll_y;
            for i in 0..self.layout.click_count {
                let r = &self.layout.click_regions[i];
                if click_x >= r.x && click_x <= r.x + r.w && doc_y >= r.y && doc_y <= r.y + r.h {
                    let mut target = [0u8; 128];
                    target[..r.target_len].copy_from_slice(&r.target[..r.target_len]);
                    return BrowserAction::NavigateTarget(target, r.target_len);
                }
            }
        }

        BrowserAction::None
    }

    pub fn render(&self, buf: &mut [u32]) {
        // 1. Fundo Geral
        fill_rect(buf, 0, 0, WIN_W, WIN_H, COLOR_BG);

        // 2. Barra Superior de Navegação (Chrome)
        fill_rect(buf, 0, 0, WIN_W, 26, COLOR_CHROME);
        draw_rect_border(buf, 0, 0, WIN_W, 26, COLOR_BORDER);

        // Botões: [<] [>] [R]
        let back_col = if self.can_go_back() { COLOR_BTN_TEXT } else { 0x0058_5B70 };
        let fwd_col = if self.can_go_forward() { COLOR_BTN_TEXT } else { 0x0058_5B70 };
        let reload_col = COLOR_URL_TEXT;

        font::draw_text(buf, WIN_W, WIN_H, 6, 9, b"<", back_col, None);
        font::draw_text(buf, WIN_W, WIN_H, 18, 9, b">", fwd_col, None);
        font::draw_text(buf, WIN_W, WIN_H, 30, 9, b"R", reload_col, None);

        // Campo de URL (Barra de Endereço)
        let border_col = if self.is_address_focused { COLOR_H1 } else { COLOR_URL_BORDER };
        fill_rect(buf, 44, 4, 238, 18, COLOR_URL_BG);
        draw_rect_border(buf, 44, 4, 238, 18, border_col);

        if self.is_address_focused {
            if self.address_selected && self.address_edit_len > 0 {
                let sel_w = (self.address_edit_len * font::FONT_WIDTH).min(234);
                fill_rect(buf, 46, 6, sel_w + 2, 14, 0x0045_475A);
                let edit_slice = &self.address_edit[..self.address_edit_len];
                font::draw_text_clipped(buf, WIN_W, WIN_H, 48, 9, edit_slice, 0x00CD_D6F4, None, 4, 22);
            } else {
                let edit_slice = &self.address_edit[..self.address_edit_len];
                font::draw_text_clipped(buf, WIN_W, WIN_H, 48, 9, edit_slice, COLOR_H1, None, 4, 22);
                // Desenha cursor sublinhado '_'
                let cursor_x = 48 + self.address_edit_len * font::FONT_WIDTH;
                if cursor_x + font::FONT_WIDTH <= 280 {
                    font::draw_text_clipped(buf, WIN_W, WIN_H, cursor_x, 9, b"_", COLOR_H1, None, 4, 22);
                }
            }
        } else {
            let url_slice = &self.current_url[..self.current_url_len];
            font::draw_text_clipped(buf, WIN_W, WIN_H, 48, 9, url_slice, COLOR_URL_TEXT, None, 4, 22);
        }

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


// ── Subsistema HTTP / URL do Jinn Browser 0.4.0 ───────────────────────────────

#[derive(Clone, Copy)]
pub struct ParsedHttpUrl {
    pub host: [u8; 64],
    pub host_len: usize,
    pub port: u16,
    pub path: [u8; 128],
    pub path_len: usize,
}

pub fn parse_http_url(url: &[u8]) -> Option<ParsedHttpUrl> {
    let mut clean = url;
    while !clean.is_empty() && (clean[0] == b' ' || clean[0] == b'\t') {
        clean = &clean[1..];
    }
    while !clean.is_empty() && (clean[clean.len() - 1] == b' ' || clean[clean.len() - 1] == b'\t' || clean[clean.len() - 1] == 0) {
        clean = &clean[..clean.len() - 1];
    }
    if clean.is_empty() {
        return None;
    }

    if clean.starts_with(b"http://") {
        clean = &clean[7..];
    } else if clean.starts_with(b"https://") {
        return None; // HTTPS tratado separadamente com mensagem informativa
    }

    // Separa host (+ porta) do caminho
    let (host_port_slice, path_slice) = match clean.iter().position(|&b| b == b'/') {
        Some(idx) => (&clean[..idx], &clean[idx..]),
        None => (clean, &b"/"[..]),
    };

    // Extrai porta se especificada (ex: host:8080)
    let (host_slice, port) = match host_port_slice.iter().position(|&b| b == b':') {
        Some(idx) => {
            let h = &host_port_slice[..idx];
            let p_str = &host_port_slice[idx + 1..];
            let mut p: u16 = 0;
            for &b in p_str {
                if b >= b'0' && b <= b'9' {
                    p = p.saturating_mul(10).saturating_add((b - b'0') as u16);
                } else {
                    break;
                }
            }
            if p == 0 { (h, 80) } else { (h, p) }
        }
        None => (host_port_slice, 80),
    };

    if host_slice.is_empty() || host_slice.len() > 63 {
        return None;
    }

    let mut parsed = ParsedHttpUrl {
        host: [0; 64],
        host_len: host_slice.len(),
        port,
        path: [0; 128],
        path_len: 0,
    };
    parsed.host[..host_slice.len()].copy_from_slice(host_slice);

    // Remove fragmento '#' do path enviado na requisição HTTP
    let req_path = match path_slice.iter().position(|&b| b == b'#') {
        Some(idx) => &path_slice[..idx],
        None => path_slice,
    };
    let p_len = req_path.len().min(127);
    parsed.path[..p_len].copy_from_slice(&req_path[..p_len]);
    parsed.path_len = p_len;

    Some(parsed)
}

/// Resolve URL relativa ou absoluta contra uma URL base existente
pub fn resolve_relative_url(base: &[u8], target: &[u8], out: &mut [u8; 128]) -> usize {
    let mut clean_target = target;
    while !clean_target.is_empty() && (clean_target[0] == b' ' || clean_target[0] == b'\t') {
        clean_target = &clean_target[1..];
    }
    while !clean_target.is_empty() && (clean_target[clean_target.len() - 1] == b' ' || clean_target[clean_target.len() - 1] == 0) {
        clean_target = &clean_target[..clean_target.len() - 1];
    }

    if clean_target.is_empty() {
        let len = base.len().min(127);
        out[..len].copy_from_slice(&base[..len]);
        out[len] = 0;
        return len;
    }

    // Se já for absoluta
    if clean_target.starts_with(b"http://") || clean_target.starts_with(b"https://")
        || clean_target.starts_with(b"about:") || clean_target.starts_with(b"file://")
        || clean_target.starts_with(b"/disk/")
    {
        let len = clean_target.len().min(127);
        out[..len].copy_from_slice(&clean_target[..len]);
        out[len] = 0;
        return len;
    }

    // Se começar com // (protocol-relative)
    if clean_target.starts_with(b"//") {
        let prefix = b"http:";
        let p_len = prefix.len();
        out[..p_len].copy_from_slice(prefix);
        let c_len = clean_target.len().min(127 - p_len);
        out[p_len..p_len + c_len].copy_from_slice(&clean_target[..c_len]);
        let total = p_len + c_len;
        out[total] = 0;
        return total;
    }

    // Se base começar com http://
    if base.starts_with(b"http://") {
        let after_scheme = &base[7..];
        let host_port_end = after_scheme.iter().position(|&b| b == b'/').unwrap_or(after_scheme.len());
        let host_prefix_len = 7 + host_port_end;

        if clean_target.starts_with(b"/") {
            // Relativo à raiz do host (ex: /about.html)
            let len = host_prefix_len.min(127);
            out[..len].copy_from_slice(&base[..len]);
            let t_len = clean_target.len().min(127 - len);
            out[len..len + t_len].copy_from_slice(&clean_target[..t_len]);
            let total = len + t_len;
            out[total] = 0;
            return total;
        } else {
            // Relativo ao diretório atual da URL base
            let last_slash = base.iter().rposition(|&b| b == b'/').unwrap_or(6);
            let pos = if last_slash >= 7 {
                let dir_len = last_slash + 1;
                let p = dir_len.min(127);
                out[..p].copy_from_slice(&base[..p]);
                p
            } else {
                let h_len = host_prefix_len.min(126);
                out[..h_len].copy_from_slice(&base[..h_len]);
                out[h_len] = b'/';
                h_len + 1
            };
            let t_len = clean_target.len().min(127 - pos);
            out[pos..pos + t_len].copy_from_slice(&clean_target[..t_len]);
            let total = pos + t_len;
            out[total] = 0;
            return total;
        }
    }

    // Fallback: local disk
    if clean_target.starts_with(b"/") {
        let pfx = b"/disk";
        out[..5].copy_from_slice(pfx);
        let c_len = clean_target.len().min(127 - 5);
        out[5..5 + c_len].copy_from_slice(&clean_target[..c_len]);
        let total = 5 + c_len;
        out[total] = 0;
        total
    } else {
        let pfx = b"/disk/";
        out[..6].copy_from_slice(pfx);
        let c_len = clean_target.len().min(127 - 6);
        out[6..6 + c_len].copy_from_slice(&clean_target[..c_len]);
        let total = 6 + c_len;
        out[total] = 0;
        total
    }
}

/// Constrói requisição HTTP/1.1 GET padrão
pub fn build_http_get_request(parsed: &ParsedHttpUrl, out: &mut [u8]) -> usize {
    let mut pos = 0usize;
    let append = |dst: &mut [u8], p: &mut usize, src: &[u8]| {
        let len = src.len().min(dst.len() - *p);
        dst[*p..*p + len].copy_from_slice(&src[..len]);
        *p += len;
    };

    append(out, &mut pos, b"GET ");
    append(out, &mut pos, &parsed.path[..parsed.path_len]);
    append(out, &mut pos, b" HTTP/1.1\r\nHost: ");
    append(out, &mut pos, &parsed.host[..parsed.host_len]);
    if parsed.port != 80 {
        append(out, &mut pos, b":");
        let mut port_buf = [0u8; 6];
        let mut port_val = parsed.port;
        let mut port_len = 0;
        if port_val == 0 {
            port_buf[0] = b'0';
            port_len = 1;
        } else {
            let mut tmp = [0u8; 6];
            let mut i = 0;
            while port_val > 0 {
                tmp[i] = b'0' + (port_val % 10) as u8;
                port_val /= 10;
                i += 1;
            }
            while i > 0 {
                i -= 1;
                port_buf[port_len] = tmp[i];
                port_len += 1;
            }
        }
        append(out, &mut pos, &port_buf[..port_len]);
    }
    append(out, &mut pos, b"\r\nUser-Agent: JinnBrowser/0.4.0 (Jinn OS x86_64; Aladdin Desktop)\r\n");
    append(out, &mut pos, b"Accept: text/html, text/plain, */*\r\n");
    append(out, &mut pos, b"Connection: close\r\n\r\n");
    pos
}

#[derive(Clone, Copy)]
pub struct HttpResponseHeader {
    pub status_code: u16,
    pub header_len: usize,
    pub content_length: Option<usize>,
    pub is_chunked: bool,
    pub location: [u8; 128],
    pub location_len: usize,
}

/// Analisa cabeçalhos da resposta HTTP recebida
pub fn parse_http_response_header(raw: &[u8]) -> Option<HttpResponseHeader> {
    // Procura término de cabeçalhos: \r\n\r\n ou \n\n
    let (hdr_end, delim_len) = if let Some(idx) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
        (idx, 4)
    } else if let Some(idx) = raw.windows(2).position(|w| w == b"\n\n") {
        (idx, 2)
    } else {
        return None;
    };

    let header_bytes = &raw[..hdr_end];
    let header_len = hdr_end + delim_len;

    // Status code: HTTP/1.x NNN
    if header_bytes.len() < 12 || !header_bytes.starts_with(b"HTTP/1.") {
        return None;
    }
    let code_slice = &header_bytes[9..12];
    let mut status_code: u16 = 0;
    for &b in code_slice {
        if b >= b'0' && b <= b'9' {
            status_code = status_code.saturating_mul(10).saturating_add((b - b'0') as u16);
        } else {
            return None;
        }
    }

    let mut content_length = None;
    let mut is_chunked = false;
    let mut location = [0u8; 128];
    let mut location_len = 0usize;

    // Itera linhas do cabeçalho
    for line in header_bytes.split(|&b| b == b'\n') {
        let mut clean_line = line;
        if !clean_line.is_empty() && clean_line[clean_line.len() - 1] == b'\r' {
            clean_line = &clean_line[..clean_line.len() - 1];
        }
        if clean_line.is_empty() {
            continue;
        }

        // Case-insensitive check para Content-Length
        if clean_line.len() > 15 && clean_line[..15].eq_ignore_ascii_case(b"content-length:") {
            let val = clean_line[15..].trim_ascii();
            let mut cl: usize = 0;
            for &b in val {
                if b >= b'0' && b <= b'9' {
                    cl = cl.saturating_mul(10).saturating_add((b - b'0') as usize);
                } else {
                    break;
                }
            }
            content_length = Some(cl);
        } else if clean_line.len() > 18 && clean_line[..18].eq_ignore_ascii_case(b"transfer-encoding:") {
            if clean_line[18..].contains_ignore_ascii_case(b"chunked") {
                is_chunked = true;
            }
        } else if clean_line.len() > 9 && clean_line[..9].eq_ignore_ascii_case(b"location:") {
            let val = clean_line[9..].trim_ascii();
            let len = val.len().min(127);
            location[..len].copy_from_slice(&val[..len]);
            location[len] = 0;
            location_len = len;
        }
    }

    Some(HttpResponseHeader {
        status_code,
        header_len,
        content_length,
        is_chunked,
        location,
        location_len,
    })
}

/// Decodifica payload com Transfer-Encoding: chunked (RFC 7230 / RFC 9112)
pub fn decode_chunked_body(raw: &[u8], out: &mut [u8]) -> Result<usize, ()> {
    let mut src_idx = 0usize;
    let mut dst_idx = 0usize;

    while src_idx < raw.len() {
        let line_end = match raw[src_idx..].iter().position(|&b| b == b'\n') {
            Some(p) => src_idx + p,
            None => break,
        };
        let mut hex_slice = &raw[src_idx..line_end];
        if !hex_slice.is_empty() && hex_slice[hex_slice.len() - 1] == b'\r' {
            hex_slice = &hex_slice[..hex_slice.len() - 1];
        }
        if let Some(semi) = hex_slice.iter().position(|&b| b == b';') {
            hex_slice = &hex_slice[..semi];
        }
        let hex_slice = hex_slice.trim_ascii();

        let mut chunk_size: usize = 0;
        for &b in hex_slice {
            let val = match b {
                b'0'..=b'9' => (b - b'0') as usize,
                b'a'..=b'f' => (b - b'a' + 10) as usize,
                b'A'..=b'F' => (b - b'A' + 10) as usize,
                _ => return Err(()),
            };
            chunk_size = chunk_size.checked_mul(16).ok_or(())?.checked_add(val).ok_or(())?;
        }

        src_idx = line_end + 1;
        if chunk_size == 0 {
            break;
        }

        if src_idx + chunk_size > raw.len() {
            let copy_len = (raw.len() - src_idx).min(out.len() - dst_idx);
            out[dst_idx..dst_idx + copy_len].copy_from_slice(&raw[src_idx..src_idx + copy_len]);
            dst_idx += copy_len;
            break;
        }

        let copy_len = chunk_size.min(out.len() - dst_idx);
        out[dst_idx..dst_idx + copy_len].copy_from_slice(&raw[src_idx..src_idx + copy_len]);
        dst_idx += copy_len;
        src_idx += chunk_size;

        if src_idx < raw.len() && raw[src_idx] == b'\r' {
            src_idx += 1;
        }
        if src_idx < raw.len() && raw[src_idx] == b'\n' {
            src_idx += 1;
        }
    }

    Ok(dst_idx)
}

/// Helper para trim de espaços ASCII em slice
#[allow(dead_code)]
pub trait SliceTrim {
    fn trim_ascii(&self) -> &Self;
    fn contains_ignore_ascii_case(&self, needle: &[u8]) -> bool;
}

impl SliceTrim for [u8] {
    fn trim_ascii(&self) -> &[u8] {
        let mut start = 0;
        while start < self.len() && (self[start] == b' ' || self[start] == b'\t' || self[start] == b'\r' || self[start] == b'\n') {
            start += 1;
        }
        let mut end = self.len();
        while end > start && (self[end - 1] == b' ' || self[end - 1] == b'\t' || self[end - 1] == b'\r' || self[end - 1] == b'\n' || self[end - 1] == 0) {
            end -= 1;
        }
        &self[start..end]
    }

    fn contains_ignore_ascii_case(&self, needle: &[u8]) -> bool {
        if needle.is_empty() { return true; }
        if self.len() < needle.len() { return false; }
        for w in self.windows(needle.len()) {
            if w.eq_ignore_ascii_case(needle) {
                return true;
            }
        }
        false
    }
}

/// Gera documento HTML formatado para erros de rede
pub fn format_net_error_html(url: &[u8], title: &[u8], msg: &[u8], detail: &[u8], out: &mut [u8]) -> usize {
    let mut pos = 0usize;
    let append = |dst: &mut [u8], p: &mut usize, src: &[u8]| {
        let len = src.len().min(dst.len() - *p);
        dst[*p..*p + len].copy_from_slice(&src[..len]);
        *p += len;
    };

    append(out, &mut pos, b"<!DOCTYPE html><html><head><title>");
    append(out, &mut pos, title);
    append(out, &mut pos, b"</title></head><body><h1>");
    append(out, &mut pos, title);
    append(out, &mut pos, b"</h1><p><b>Destino: </b>");
    append(out, &mut pos, url);
    append(out, &mut pos, b"</p><p>");
    append(out, &mut pos, msg);
    append(out, &mut pos, b"</p><p><i>Detalhes tecnicos: </i>");
    append(out, &mut pos, detail);
    append(out, &mut pos, b"</p><hr/><p><a href=\"index.htm\">Voltar ao Inicio</a></p></body></html>");
    pos
}

/// Gera documento HTML informativo sobre o status real de HTTPS/TLS
pub fn format_tls_info_html(url: &[u8], out: &mut [u8]) -> usize {
    let mut pos = 0usize;
    let append = |dst: &mut [u8], p: &mut usize, src: &[u8]| {
        let len = src.len().min(dst.len() - *p);
        dst[*p..*p + len].copy_from_slice(&src[..len]);
        *p += len;
    };

    append(out, &mut pos, b"<!DOCTYPE html><html><head><title>HTTPS / TLS em Desenvolvimento</title></head><body>");
    append(out, &mut pos, b"<h1>Conexao Segura (HTTPS)</h1>");
    append(out, &mut pos, b"<p>O suporte nativo a HTTPS/TLS 1.3 esta em fase de desenvolvimento no microkernel Jinn OS.</p>");
    append(out, &mut pos, b"<p><b>URL Solicitada: </b>");
    append(out, &mut pos, url);
    append(out, &mut pos, b"</p><p><b>Requisitos tecnicos pendentes para Ring 3:</b></p>");
    append(out, &mut pos, b"<p>- Criptografia assimetrica e simetrica (AES-GCM / ChaCha20) no-std</p>");
    append(out, &mut pos, b"<p>- Bundle de certificados raiz (Root CAs) e parser X.509</p>");
    append(out, &mut pos, b"<p>- Entropia criptografica (CSPRNG via syscall) e relogio RTC/SNTP</p>");
    append(out, &mut pos, b"<hr/><p><a href=\"index.htm\">Voltar ao Inicio</a></p></body></html>");
    pos
}
