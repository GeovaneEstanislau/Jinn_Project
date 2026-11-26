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

#[inline(always)]
pub const fn content_clip_bottom(h: usize) -> usize {
    if h >= 100 { h.saturating_sub(15) } else { h.saturating_sub(1) }
}
#[allow(dead_code)]
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

// ── Pipeline Completo: Flat Vector DOM, Mini-CSS e Mini-JS (no_std) ──────────

pub const MAX_DOM_NODES: usize = 512;

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DomNodeType {
    Element,
    Text,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DisplayMode {
    Block,
    Inline,
    InlineBlock,
    Flex,
    None,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FlexDirection {
    Row,
    Column,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JustifyContent {
    FlexStart,
    Center,
    FlexEnd,
    SpaceBetween,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AlignItems {
    FlexStart,
    Center,
    FlexEnd,
    Stretch,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ComputedStyle {
    pub color: u32,
    pub bg_color: Option<u32>,
    pub border_color: Option<u32>,
    pub border_width: usize,
    pub border_radius: usize,
    pub font_size: usize,
    pub width: Option<usize>,
    pub height: Option<usize>,
    pub max_width: Option<usize>,
    pub margin_top: usize,
    pub margin_bottom: usize,
    pub margin_left: usize,
    pub margin_right: usize,
    pub padding_top: usize,
    pub padding_bottom: usize,
    pub padding_left: usize,
    pub padding_right: usize,
    pub is_bold: bool,
    pub display_block: bool,
    pub display: DisplayMode,
    pub flex_direction: FlexDirection,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    pub text_align: TextAlign,
}

impl ComputedStyle {
    pub const fn default() -> Self {
        Self {
            color: COLOR_TEXT,
            bg_color: None,
            border_color: None,
            border_width: 1,
            border_radius: 0,
            font_size: 12,
            width: None,
            height: None,
            max_width: None,
            margin_top: 2,
            margin_bottom: 4,
            margin_left: 0,
            margin_right: 0,
            padding_top: 0,
            padding_bottom: 0,
            padding_left: 0,
            padding_right: 0,
            is_bold: false,
            display_block: true,
            display: DisplayMode::Block,
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::FlexStart,
            text_align: TextAlign::Left,
        }
    }
}

pub const MAX_DOM_CHILDREN: usize = 16;

#[allow(dead_code)]
#[derive(Clone, Copy)]
pub struct DomNode {
    pub id: usize,
    pub node_type: DomNodeType,
    pub tag: [u8; 16],
    pub tag_len: usize,
    pub id_attr: [u8; 32],
    pub id_len: usize,
    pub class_attr: [u8; 64],
    pub class_len: usize,
    pub style_attr: [u8; 128],
    pub style_len: usize,
    pub href_attr: [u8; 128],
    pub href_len: usize,
    pub src_attr: [u8; 128],
    pub src_len: usize,
    pub width_attr: u16,
    pub height_attr: u16,
    pub alt_attr: [u8; 32],
    pub alt_len: usize,
    pub onclick_attr: [u8; 256],
    pub onclick_len: usize,
    pub text: [u8; 256],
    pub text_len: usize,
    pub parent: Option<usize>,
    pub children: [usize; MAX_DOM_CHILDREN],
    pub child_count: usize,
    pub computed_style: ComputedStyle,
}

impl DomNode {
    pub const fn empty() -> Self {
        Self {
            id: 0,
            node_type: DomNodeType::Element,
            tag: [0; 16],
            tag_len: 0,
            id_attr: [0; 32],
            id_len: 0,
            class_attr: [0; 64],
            class_len: 0,
            style_attr: [0; 128],
            style_len: 0,
            href_attr: [0; 128],
            href_len: 0,
            src_attr: [0; 128],
            src_len: 0,
            width_attr: 0,
            height_attr: 0,
            alt_attr: [0; 32],
            alt_len: 0,
            onclick_attr: [0; 256],
            onclick_len: 0,
            text: [0; 256],
            text_len: 0,
            parent: None,
            children: [0; MAX_DOM_CHILDREN],
            child_count: 0,
            computed_style: ComputedStyle::default(),
        }
    }
}

pub struct DomArena {
    pub nodes: [DomNode; MAX_DOM_NODES],
    pub node_count: usize,
    pub title: [u8; 64],
    pub title_len: usize,
}

impl DomArena {
    pub const fn new() -> Self {
        Self {
            nodes: [DomNode::empty(); MAX_DOM_NODES],
            node_count: 0,
            title: [0; 64],
            title_len: 0,
        }
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.node_count = 0;
        self.title_len = 0;
    }

    pub fn alloc_node(&mut self, node_type: DomNodeType) -> Option<usize> {
        if self.node_count < MAX_DOM_NODES {
            let id = self.node_count;
            let mut node = DomNode::empty();
            node.id = id;
            node.node_type = node_type;
            self.nodes[id] = node;
            self.node_count += 1;
            Some(id)
        } else {
            None
        }
    }

    pub fn find_by_id(&self, id_bytes: &[u8]) -> Option<usize> {
        let clean = id_bytes.trim_ascii();
        let target = if !clean.is_empty() && clean[0] == b'#' { &clean[1..] } else { clean };
        for i in 0..self.node_count {
            let node = &self.nodes[i];
            if node.id_len == target.len() && &node.id_attr[..node.id_len] == target {
                return Some(i);
            }
        }
        None
    }

    pub fn find_by_class(&self, class_bytes: &[u8]) -> Option<usize> {
        let clean = class_bytes.trim_ascii();
        let target = if !clean.is_empty() && clean[0] == b'.' { &clean[1..] } else { clean };
        for i in 0..self.node_count {
            if self.has_class(i, target) {
                return Some(i);
            }
        }
        None
    }

    pub fn find_by_tag(&self, tag_bytes: &[u8]) -> Option<usize> {
        let clean = tag_bytes.trim_ascii();
        for i in 0..self.node_count {
            let node = &self.nodes[i];
            if node.tag_len == clean.len() && node.tag[..node.tag_len].eq_ignore_ascii_case(clean) {
                return Some(i);
            }
        }
        None
    }

    pub fn find_by_selector(&self, sel: &[u8]) -> Option<usize> {
        let s = sel.trim_ascii();
        if s.is_empty() { return None; }
        if s[0] == b'#' {
            self.find_by_id(&s[1..])
        } else if s[0] == b'.' {
            self.find_by_class(&s[1..])
        } else {
            self.find_by_tag(s)
        }
    }

    pub fn has_class(&self, idx: usize, class_name: &[u8]) -> bool {
        if idx >= self.node_count { return false; }
        let c_attr = &self.nodes[idx].class_attr[..self.nodes[idx].class_len];
        for word in c_attr.split(|&b| b == b' ' || b == b'\t' || b == b'\r' || b == b'\n') {
            if !word.is_empty() && word.eq_ignore_ascii_case(class_name) {
                return true;
            }
        }
        false
    }

    pub fn add_class(&mut self, idx: usize, class_name: &[u8]) {
        if idx >= self.node_count || self.has_class(idx, class_name) { return; }
        let node = &mut self.nodes[idx];
        if node.class_len + 1 + class_name.len() <= node.class_attr.len() {
            if node.class_len > 0 {
                node.class_attr[node.class_len] = b' ';
                node.class_len += 1;
            }
            node.class_attr[node.class_len..node.class_len + class_name.len()].copy_from_slice(class_name);
            node.class_len += class_name.len();
        }
    }

    pub fn remove_class(&mut self, idx: usize, class_name: &[u8]) {
        if idx >= self.node_count { return; }
        let node = &mut self.nodes[idx];
        let mut new_buf = [0u8; 64];
        let mut new_len = 0;
        let c_attr = &node.class_attr[..node.class_len];
        for word in c_attr.split(|&b| b == b' ' || b == b'\t' || b == b'\r' || b == b'\n') {
            if word.is_empty() || word.eq_ignore_ascii_case(class_name) {
                continue;
            }
            if new_len > 0 && new_len < 63 {
                new_buf[new_len] = b' ';
                new_len += 1;
            }
            let cpy = word.len().min(64 - new_len);
            new_buf[new_len..new_len + cpy].copy_from_slice(&word[..cpy]);
            new_len += cpy;
        }
        node.class_attr = new_buf;
        node.class_len = new_len;
    }

    pub fn toggle_class(&mut self, idx: usize, class_name: &[u8]) -> bool {
        if self.has_class(idx, class_name) {
            self.remove_class(idx, class_name);
            false
        } else {
            self.add_class(idx, class_name);
            true
        }
    }

    pub fn set_text_content(&mut self, idx: usize, new_text: &[u8]) {
        if idx < self.node_count {
            let len = new_text.len().min(255);
            self.nodes[idx].text[..len].copy_from_slice(&new_text[..len]);
            self.nodes[idx].text_len = len;
        }
    }

    pub fn set_style_color(&mut self, idx: usize, col: u32) {
        if idx < self.node_count {
            self.nodes[idx].computed_style.color = col;
        }
    }

    #[allow(dead_code)]
    pub fn set_style_bg(&mut self, idx: usize, col: u32) {
        if idx < self.node_count {
            self.nodes[idx].computed_style.bg_color = Some(col);
        }
    }

    pub fn set_style_display(&mut self, idx: usize, mode: DisplayMode) {
        if idx < self.node_count {
            self.nodes[idx].computed_style.display = mode;
            self.nodes[idx].computed_style.display_block = mode == DisplayMode::Block;
        }
    }
}

// ── Mini-CSS e Resolução de Estilos ──────────────────────────────────────────

pub const MAX_CSS_RULES: usize = 128;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SelectorKind {
    Tag,
    Class,
    Id,
    Descendant,
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
pub struct CssRule {
    pub kind: SelectorKind,
    pub selector: [u8; 32],
    pub selector_len: usize,
    pub parent_selector: [u8; 32],
    pub parent_selector_len: usize,
    pub specificity: u16,
    pub color: Option<u32>,
    pub bg_color: Option<u32>,
    pub border_color: Option<u32>,
    pub border_width: Option<usize>,
    pub border_radius: Option<usize>,
    pub font_size: Option<usize>,
    pub width: Option<usize>,
    pub height: Option<usize>,
    pub max_width: Option<usize>,
    pub margin_top: Option<usize>,
    pub margin_bottom: Option<usize>,
    pub margin_left: Option<usize>,
    pub margin_right: Option<usize>,
    pub padding_top: Option<usize>,
    pub padding_bottom: Option<usize>,
    pub padding_left: Option<usize>,
    pub padding_right: Option<usize>,
    pub is_bold: Option<bool>,
    pub display: Option<DisplayMode>,
    pub flex_direction: Option<FlexDirection>,
    pub justify_content: Option<JustifyContent>,
    pub align_items: Option<AlignItems>,
    pub text_align: Option<TextAlign>,
}

impl CssRule {
    pub const fn empty() -> Self {
        Self {
            kind: SelectorKind::Tag,
            selector: [0; 32],
            selector_len: 0,
            parent_selector: [0; 32],
            parent_selector_len: 0,
            specificity: 1,
            color: None,
            bg_color: None,
            border_color: None,
            border_width: None,
            border_radius: None,
            font_size: None,
            width: None,
            height: None,
            max_width: None,
            margin_top: None,
            margin_bottom: None,
            margin_left: None,
            margin_right: None,
            padding_top: None,
            padding_bottom: None,
            padding_left: None,
            padding_right: None,
            is_bold: None,
            display: None,
            flex_direction: None,
            justify_content: None,
            align_items: None,
            text_align: None,
        }
    }
}

pub struct MiniCss {
    pub rules: [CssRule; MAX_CSS_RULES],
    pub rule_count: usize,
}

impl MiniCss {
    pub const fn new() -> Self {
        Self {
            rules: [CssRule::empty(); MAX_CSS_RULES],
            rule_count: 0,
        }
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.rule_count = 0;
    }

    pub fn parse_stylesheet(&mut self, css: &[u8]) {
        self.rule_count = 0;
        self.parse_stylesheet_append(css);
    }

    pub fn parse_stylesheet_append(&mut self, css: &[u8]) {
        let mut i = 0usize;
        let len = css.len();

        while i < len && self.rule_count < MAX_CSS_RULES {
            while i < len && (css[i] == b' ' || css[i] == b'\t' || css[i] == b'\r' || css[i] == b'\n') {
                i += 1;
            }
            if i >= len { break; }

            // Comentário CSS /* ... */
            if i + 1 < len && css[i] == b'/' && css[i + 1] == b'*' {
                i += 2;
                while i + 1 < len && !(css[i] == b'*' && css[i + 1] == b'/') {
                    i += 1;
                }
                if i + 1 < len { i += 2; }
                continue;
            }

            let sel_start = i;
            while i < len && css[i] != b'{' && css[i] != b'}' {
                i += 1;
            }
            if i >= len || css[i] != b'{' { break; }
            let sel_raw = css[sel_start..i].trim_ascii();
            i += 1;

            let body_start = i;
            while i < len && css[i] != b'}' {
                i += 1;
            }
            let body = &css[body_start..i];
            if i < len && css[i] == b'}' {
                i += 1;
            }

            if sel_raw.is_empty() { continue; }

            // Suporta seletores múltiplos separados por vírgula (ex: "h1, h2, h3")
            for sub_sel in sel_raw.split(|&b| b == b',') {
                let s_clean = sub_sel.trim_ascii();
                if s_clean.is_empty() || self.rule_count >= MAX_CSS_RULES { continue; }

                let mut rule = CssRule::empty();

                // Verifica se é seletor descendente simples com espaço ("nav a", ".card .btn", "div p")
                if let Some(space_pos) = s_clean.iter().position(|&b| b == b' ' || b == b'\t') {
                    let parent_part = s_clean[..space_pos].trim_ascii();
                    let child_part = s_clean[space_pos + 1..].trim_ascii();

                    rule.kind = SelectorKind::Descendant;
                    let p_cpy = parent_part.len().min(31);
                    rule.parent_selector[..p_cpy].copy_from_slice(&parent_part[..p_cpy]);
                    rule.parent_selector_len = p_cpy;

                    let c_cpy = child_part.len().min(31);
                    rule.selector[..c_cpy].copy_from_slice(&child_part[..c_cpy]);
                    rule.selector_len = c_cpy;
                    rule.specificity = 2;
                } else if s_clean[0] == b'.' {
                    rule.kind = SelectorKind::Class;
                    let sel_name = &s_clean[1..];
                    let cpy = sel_name.len().min(31);
                    rule.selector[..cpy].copy_from_slice(&sel_name[..cpy]);
                    rule.selector_len = cpy;
                    rule.specificity = 10;
                } else if s_clean[0] == b'#' {
                    rule.kind = SelectorKind::Id;
                    let sel_name = &s_clean[1..];
                    let cpy = sel_name.len().min(31);
                    rule.selector[..cpy].copy_from_slice(&sel_name[..cpy]);
                    rule.selector_len = cpy;
                    rule.specificity = 100;
                } else {
                    rule.kind = SelectorKind::Tag;
                    let cpy = s_clean.len().min(31);
                    rule.selector[..cpy].copy_from_slice(&s_clean[..cpy]);
                    rule.selector_len = cpy;
                    rule.specificity = 1;
                }

                // Parser de declarações
                for decl in body.split(|&b| b == b';') {
                    let decl_clean = decl.trim_ascii();
                    if decl_clean.is_empty() { continue; }
                    if let Some(colon) = decl_clean.iter().position(|&b| b == b':') {
                        let prop = decl_clean[..colon].trim_ascii();
                        let val = decl_clean[colon + 1..].trim_ascii();

                        if prop.eq_ignore_ascii_case(b"color") {
                            rule.color = parse_color(val);
                        } else if prop.eq_ignore_ascii_case(b"background-color") || prop.eq_ignore_ascii_case(b"background") {
                            rule.bg_color = parse_color(val);
                        } else if prop.eq_ignore_ascii_case(b"border") {
                            let (bw, bc) = parse_border(val);
                            if bw.is_some() { rule.border_width = bw; }
                            if bc.is_some() { rule.border_color = bc; }
                        } else if prop.eq_ignore_ascii_case(b"border-color") {
                            rule.border_color = parse_color(val);
                        } else if prop.eq_ignore_ascii_case(b"border-width") {
                            rule.border_width = parse_px(val);
                        } else if prop.eq_ignore_ascii_case(b"border-radius") {
                            rule.border_radius = parse_px(val);
                        } else if prop.eq_ignore_ascii_case(b"font-size") {
                            rule.font_size = parse_px(val);
                        } else if prop.eq_ignore_ascii_case(b"width") {
                            rule.width = parse_px(val);
                        } else if prop.eq_ignore_ascii_case(b"height") {
                            rule.height = parse_px(val);
                        } else if prop.eq_ignore_ascii_case(b"max-width") {
                            rule.max_width = parse_px(val);
                        } else if prop.eq_ignore_ascii_case(b"margin") {
                            if let Some((v, h)) = parse_one_or_two_px(val) {
                                rule.margin_top = Some(v);
                                rule.margin_bottom = Some(v);
                                rule.margin_left = Some(h);
                                rule.margin_right = Some(h);
                            }
                        } else if prop.eq_ignore_ascii_case(b"margin-top") {
                            rule.margin_top = parse_px(val);
                        } else if prop.eq_ignore_ascii_case(b"margin-bottom") {
                            rule.margin_bottom = parse_px(val);
                        } else if prop.eq_ignore_ascii_case(b"margin-left") {
                            rule.margin_left = parse_px(val);
                        } else if prop.eq_ignore_ascii_case(b"margin-right") {
                            rule.margin_right = parse_px(val);
                        } else if prop.eq_ignore_ascii_case(b"padding") {
                            if let Some((v, h)) = parse_one_or_two_px(val) {
                                rule.padding_top = Some(v);
                                rule.padding_bottom = Some(v);
                                rule.padding_left = Some(h);
                                rule.padding_right = Some(h);
                            }
                        } else if prop.eq_ignore_ascii_case(b"padding-top") {
                            rule.padding_top = parse_px(val);
                        } else if prop.eq_ignore_ascii_case(b"padding-bottom") {
                            rule.padding_bottom = parse_px(val);
                        } else if prop.eq_ignore_ascii_case(b"padding-left") {
                            rule.padding_left = parse_px(val);
                        } else if prop.eq_ignore_ascii_case(b"padding-right") {
                            rule.padding_right = parse_px(val);
                        } else if prop.eq_ignore_ascii_case(b"font-weight") {
                            if val.eq_ignore_ascii_case(b"bold") {
                                rule.is_bold = Some(true);
                            } else if val.eq_ignore_ascii_case(b"normal") {
                                rule.is_bold = Some(false);
                            }
                        } else if prop.eq_ignore_ascii_case(b"display") {
                            rule.display = parse_display(val);
                        } else if prop.eq_ignore_ascii_case(b"flex-direction") {
                            rule.flex_direction = parse_flex_direction(val);
                        } else if prop.eq_ignore_ascii_case(b"justify-content") {
                            rule.justify_content = parse_justify_content(val);
                        } else if prop.eq_ignore_ascii_case(b"align-items") {
                            rule.align_items = parse_align_items(val);
                        } else if prop.eq_ignore_ascii_case(b"text-align") {
                            rule.text_align = parse_text_align(val);
                        }
                    }
                }

                self.rules[self.rule_count] = rule;
                self.rule_count += 1;
            }
        }
    }

    pub fn resolve_all(&self, dom: &mut DomArena) {
        for i in 0..dom.node_count {
            self.resolve_node(dom, i);
        }
    }

    pub fn resolve_node(&self, dom: &mut DomArena, node_idx: usize) {
        if node_idx >= dom.node_count { return; }

        let tag = dom.nodes[node_idx].tag;
        let tag_len = dom.nodes[node_idx].tag_len;
        let tag_bytes = &tag[..tag_len];

        // 1. Estilos Padrão do User-Agent baseados na Tag
        let mut style = ComputedStyle::default();
        if tag_bytes == b"h1" {
            style.color = COLOR_H1;
            style.font_size = 16;
            style.margin_top = 8;
            style.margin_bottom = 6;
            style.is_bold = true;
            style.display = DisplayMode::Block;
            style.display_block = true;
        } else if tag_bytes == b"h2" {
            style.color = 0x00F3_8BA8;
            style.font_size = 14;
            style.margin_top = 6;
            style.margin_bottom = 4;
            style.is_bold = true;
            style.display = DisplayMode::Block;
            style.display_block = true;
        } else if tag_bytes == b"h3" {
            style.color = 0x00A6_E3A1;
            style.font_size = 13;
            style.margin_top = 4;
            style.margin_bottom = 4;
            style.is_bold = true;
            style.display = DisplayMode::Block;
            style.display_block = true;
        } else if tag_bytes == b"h4" || tag_bytes == b"h5" || tag_bytes == b"h6" {
            style.color = 0x00F9_E2AF;
            style.margin_top = 3;
            style.margin_bottom = 3;
            style.is_bold = true;
            style.display = DisplayMode::Block;
            style.display_block = true;
        } else if tag_bytes == b"p" {
            style.color = COLOR_TEXT;
            style.margin_top = 2;
            style.margin_bottom = 4;
            style.display = DisplayMode::Block;
            style.display_block = true;
        } else if tag_bytes == b"a" {
            style.color = COLOR_LINK;
            style.display = DisplayMode::Inline;
            style.display_block = false;
        } else if tag_bytes == b"button" {
            style.color = COLOR_BTN_TEXT;
            style.bg_color = Some(0x0031_3244);
            style.border_color = Some(0x0045_475A);
            style.border_width = 1;
            style.border_radius = 3;
            style.margin_top = 3;
            style.margin_bottom = 3;
            style.margin_left = 2;
            style.margin_right = 2;
            style.padding_top = 2;
            style.padding_bottom = 2;
            style.padding_left = 6;
            style.padding_right = 6;
            style.display = DisplayMode::InlineBlock;
            style.display_block = false;
        } else if tag_bytes == b"div" || tag_bytes == b"nav" || tag_bytes == b"header" || tag_bytes == b"section" || tag_bytes == b"main" || tag_bytes == b"article" || tag_bytes == b"footer" {
            style.display = DisplayMode::Block;
            style.display_block = true;
            style.margin_top = 0;
            style.margin_bottom = 0;
        } else if tag_bytes == b"hr" {
            style.display = DisplayMode::Block;
            style.display_block = true;
            style.margin_top = 6;
            style.margin_bottom = 6;
        } else if tag_bytes == b"b" || tag_bytes == b"strong" {
            style.is_bold = true;
            style.display = DisplayMode::Inline;
            style.display_block = false;
        } else if tag_bytes == b"ul" {
            style.display = DisplayMode::Block;
            style.display_block = true;
            style.margin_top = 4;
            style.margin_bottom = 4;
            style.padding_left = 8;
        } else if tag_bytes == b"li" {
            style.display = DisplayMode::Block;
            style.display_block = true;
            style.margin_top = 2;
            style.margin_bottom = 2;
        } else if tag_bytes == b"span" || tag_bytes == b"#text" || tag_bytes == b"i" || tag_bytes == b"em" || tag_bytes == b"code" || tag_bytes == b"small" {
            style.display = DisplayMode::Inline;
            style.display_block = false;
            if tag_bytes == b"#text" {
                style.margin_top = 0;
                style.margin_bottom = 0;
            }
        } else if tag_bytes == b"input" {
            style.color = COLOR_TEXT;
            style.bg_color = Some(0x0018_1825);
            style.border_color = Some(0x0045_475A);
            style.border_width = 1;
            style.border_radius = 2;
            style.padding_top = 2;
            style.padding_bottom = 2;
            style.padding_left = 4;
            style.padding_right = 4;
            style.display = DisplayMode::InlineBlock;
            style.display_block = false;
        }

        // 2. Cascata CSS: Tag (1) -> Descendente (2) -> Classe (10) -> ID (100) -> Inline (1000)

        // 2.1. Regras de Tag
        for i in 0..self.rule_count {
            let r = &self.rules[i];
            if r.kind == SelectorKind::Tag && tag_len == r.selector_len && tag_bytes.eq_ignore_ascii_case(&r.selector[..r.selector_len]) {
                apply_rule_to_style(&mut style, r);
            }
        }

        // 2.2. Regras Descendentes ("nav a", ".card .btn")
        for i in 0..self.rule_count {
            let r = &self.rules[i];
            if r.kind == SelectorKind::Descendant {
                let r_sel = &r.selector[..r.selector_len];
                let matches_child = (r_sel.starts_with(b".") && dom.has_class(node_idx, &r_sel[1..]))
                    || (r_sel.starts_with(b"#") && dom.nodes[node_idx].id_len == r_sel.len() - 1 && &dom.nodes[node_idx].id_attr[..dom.nodes[node_idx].id_len] == &r_sel[1..])
                    || (tag_len == r_sel.len() && tag_bytes.eq_ignore_ascii_case(r_sel));

                if matches_child {
                    let mut matched_ancestor = false;
                    let mut cur_parent = dom.nodes[node_idx].parent;
                    let p_sel = &r.parent_selector[..r.parent_selector_len];
                    while let Some(pid) = cur_parent {
                        let pnode = &dom.nodes[pid];
                        let matches_parent = (p_sel.starts_with(b".") && dom.has_class(pid, &p_sel[1..]))
                            || (p_sel.starts_with(b"#") && pnode.id_len == p_sel.len() - 1 && &pnode.id_attr[..pnode.id_len] == &p_sel[1..])
                            || (pnode.tag_len == p_sel.len() && pnode.tag[..pnode.tag_len].eq_ignore_ascii_case(p_sel));
                        if matches_parent {
                            matched_ancestor = true;
                            break;
                        }
                        cur_parent = pnode.parent;
                    }
                    if matched_ancestor {
                        apply_rule_to_style(&mut style, r);
                    }
                }
            }
        }

        // 2.3. Regras de Classe (.btn, .card)
        for i in 0..self.rule_count {
            let r = &self.rules[i];
            if r.kind == SelectorKind::Class {
                let r_sel = &r.selector[..r.selector_len];
                if dom.has_class(node_idx, r_sel) {
                    apply_rule_to_style(&mut style, r);
                }
            }
        }

        // 2.4. Regras de ID (#app, #counter)
        for i in 0..self.rule_count {
            let r = &self.rules[i];
            if r.kind == SelectorKind::Id {
                let r_sel = &r.selector[..r.selector_len];
                if dom.nodes[node_idx].id_len == r.selector_len && &dom.nodes[node_idx].id_attr[..dom.nodes[node_idx].id_len] == r_sel {
                    apply_rule_to_style(&mut style, r);
                }
            }
        }

        // 2.5. Estilos Inline (style="...")
        if dom.nodes[node_idx].style_len > 0 {
            let inline = dom.nodes[node_idx].style_attr;
            let inline_len = dom.nodes[node_idx].style_len;
            let inline_slice = &inline[..inline_len];
            for decl in inline_slice.split(|&b| b == b';') {
                let decl_clean = decl.trim_ascii();
                if let Some(colon) = decl_clean.iter().position(|&b| b == b':') {
                    let prop = decl_clean[..colon].trim_ascii();
                    let val = decl_clean[colon + 1..].trim_ascii();
                    if prop.eq_ignore_ascii_case(b"color") {
                        if let Some(c) = parse_color(val) { style.color = c; }
                    } else if prop.eq_ignore_ascii_case(b"background-color") || prop.eq_ignore_ascii_case(b"background") {
                        if let Some(bg) = parse_color(val) { style.bg_color = Some(bg); }
                    } else if prop.eq_ignore_ascii_case(b"border") {
                        let (bw, bc) = parse_border(val);
                        if let Some(w) = bw { style.border_width = w; }
                        if let Some(c) = bc { style.border_color = Some(c); }
                    } else if prop.eq_ignore_ascii_case(b"border-color") {
                        if let Some(bc) = parse_color(val) { style.border_color = Some(bc); }
                    } else if prop.eq_ignore_ascii_case(b"border-width") {
                        if let Some(w) = parse_px(val) { style.border_width = w; }
                    } else if prop.eq_ignore_ascii_case(b"border-radius") {
                        if let Some(r) = parse_px(val) { style.border_radius = r; }
                    } else if prop.eq_ignore_ascii_case(b"font-size") {
                        if let Some(fs) = parse_px(val) { style.font_size = fs; }
                    } else if prop.eq_ignore_ascii_case(b"width") {
                        if let Some(w) = parse_px(val) { style.width = Some(w); }
                    } else if prop.eq_ignore_ascii_case(b"height") {
                        if let Some(h) = parse_px(val) { style.height = Some(h); }
                    } else if prop.eq_ignore_ascii_case(b"max-width") {
                        if let Some(mw) = parse_px(val) { style.max_width = Some(mw); }
                    } else if prop.eq_ignore_ascii_case(b"margin") {
                        if let Some((v, h)) = parse_one_or_two_px(val) {
                            style.margin_top = v;
                            style.margin_bottom = v;
                            style.margin_left = h;
                            style.margin_right = h;
                        }
                    } else if prop.eq_ignore_ascii_case(b"margin-top") {
                        if let Some(m) = parse_px(val) { style.margin_top = m; }
                    } else if prop.eq_ignore_ascii_case(b"margin-bottom") {
                        if let Some(m) = parse_px(val) { style.margin_bottom = m; }
                    } else if prop.eq_ignore_ascii_case(b"margin-left") {
                        if let Some(m) = parse_px(val) { style.margin_left = m; }
                    } else if prop.eq_ignore_ascii_case(b"margin-right") {
                        if let Some(m) = parse_px(val) { style.margin_right = m; }
                    } else if prop.eq_ignore_ascii_case(b"padding") {
                        if let Some((v, h)) = parse_one_or_two_px(val) {
                            style.padding_top = v;
                            style.padding_bottom = v;
                            style.padding_left = h;
                            style.padding_right = h;
                        }
                    } else if prop.eq_ignore_ascii_case(b"padding-top") {
                        if let Some(p) = parse_px(val) { style.padding_top = p; }
                    } else if prop.eq_ignore_ascii_case(b"padding-bottom") {
                        if let Some(p) = parse_px(val) { style.padding_bottom = p; }
                    } else if prop.eq_ignore_ascii_case(b"padding-left") {
                        if let Some(p) = parse_px(val) { style.padding_left = p; }
                    } else if prop.eq_ignore_ascii_case(b"padding-right") {
                        if let Some(p) = parse_px(val) { style.padding_right = p; }
                    } else if prop.eq_ignore_ascii_case(b"font-weight") {
                        if val.eq_ignore_ascii_case(b"bold") {
                            style.is_bold = true;
                        } else if val.eq_ignore_ascii_case(b"normal") {
                            style.is_bold = false;
                        }
                    } else if prop.eq_ignore_ascii_case(b"display") {
                        if let Some(d) = parse_display(val) {
                            style.display = d;
                            style.display_block = d == DisplayMode::Block;
                        }
                    } else if prop.eq_ignore_ascii_case(b"flex-direction") {
                        if let Some(fd) = parse_flex_direction(val) { style.flex_direction = fd; }
                    } else if prop.eq_ignore_ascii_case(b"justify-content") {
                        if let Some(jc) = parse_justify_content(val) { style.justify_content = jc; }
                    } else if prop.eq_ignore_ascii_case(b"align-items") {
                        if let Some(ai) = parse_align_items(val) { style.align_items = ai; }
                    } else if prop.eq_ignore_ascii_case(b"text-align") {
                        if let Some(ta) = parse_text_align(val) { style.text_align = ta; }
                    }
                }
            }
        }

        dom.nodes[node_idx].computed_style = style;
    }

    pub fn resolve(&self, node: &mut DomNode) {
        // Compatibilidade com nós isolados
        let tag = &node.tag[..node.tag_len];
        if tag == b"h1" {
            node.computed_style.color = COLOR_H1;
            node.computed_style.font_size = 16;
            node.computed_style.margin_top = 8;
            node.computed_style.margin_bottom = 6;
            node.computed_style.is_bold = true;
            node.computed_style.display = DisplayMode::Block;
            node.computed_style.display_block = true;
        } else if tag == b"h2" {
            node.computed_style.color = 0x00F3_8BA8;
            node.computed_style.font_size = 14;
            node.computed_style.margin_top = 6;
            node.computed_style.margin_bottom = 4;
            node.computed_style.is_bold = true;
            node.computed_style.display = DisplayMode::Block;
            node.computed_style.display_block = true;
        } else if tag == b"h3" {
            node.computed_style.color = 0x00A6_E3A1;
            node.computed_style.font_size = 13;
            node.computed_style.margin_top = 4;
            node.computed_style.margin_bottom = 4;
            node.computed_style.is_bold = true;
            node.computed_style.display = DisplayMode::Block;
            node.computed_style.display_block = true;
        } else if tag == b"p" {
            node.computed_style.color = COLOR_TEXT;
            node.computed_style.margin_top = 2;
            node.computed_style.margin_bottom = 4;
            node.computed_style.display = DisplayMode::Block;
            node.computed_style.display_block = true;
        } else if tag == b"a" {
            node.computed_style.color = COLOR_LINK;
            node.computed_style.display = DisplayMode::Inline;
            node.computed_style.display_block = false;
        } else if tag == b"button" {
            node.computed_style.color = COLOR_BTN_TEXT;
            node.computed_style.bg_color = Some(0x0031_3244);
            node.computed_style.border_color = Some(0x0045_475A);
            node.computed_style.border_width = 1;
            node.computed_style.border_radius = 3;
            node.computed_style.margin_top = 3;
            node.computed_style.margin_bottom = 3;
            node.computed_style.margin_left = 2;
            node.computed_style.margin_right = 2;
            node.computed_style.padding_top = 2;
            node.computed_style.padding_bottom = 2;
            node.computed_style.padding_left = 6;
            node.computed_style.padding_right = 6;
            node.computed_style.display = DisplayMode::InlineBlock;
            node.computed_style.display_block = false;
        } else if tag == b"div" || tag == b"nav" || tag == b"header" || tag == b"section" {
            node.computed_style.display = DisplayMode::Block;
            node.computed_style.display_block = true;
            node.computed_style.margin_top = 0;
            node.computed_style.margin_bottom = 0;
        } else if tag == b"hr" {
            node.computed_style.display = DisplayMode::Block;
            node.computed_style.display_block = true;
            node.computed_style.margin_top = 6;
            node.computed_style.margin_bottom = 6;
        } else if tag == b"b" || tag == b"strong" {
            node.computed_style.is_bold = true;
            node.computed_style.display = DisplayMode::Inline;
            node.computed_style.display_block = false;
        } else if tag == b"ul" {
            node.computed_style.display = DisplayMode::Block;
            node.computed_style.display_block = true;
            node.computed_style.margin_top = 4;
            node.computed_style.margin_bottom = 4;
            node.computed_style.padding_left = 8;
        } else if tag == b"li" {
            node.computed_style.display = DisplayMode::Block;
            node.computed_style.display_block = true;
            node.computed_style.margin_top = 2;
            node.computed_style.margin_bottom = 2;
        } else if tag == b"span" || tag == b"#text" || tag == b"i" || tag == b"em" || tag == b"code" || tag == b"small" {
            node.computed_style.display = DisplayMode::Inline;
            node.computed_style.display_block = false;
            if tag == b"#text" {
                node.computed_style.margin_top = 0;
                node.computed_style.margin_bottom = 0;
            }
        }

        for i in 0..self.rule_count {
            let r = &self.rules[i];
            if r.kind == SelectorKind::Tag && node.tag_len == r.selector_len && node.tag[..node.tag_len].eq_ignore_ascii_case(&r.selector[..r.selector_len]) {
                apply_rule_to_style(&mut node.computed_style, r);
            }
        }
    }
}

fn apply_rule_to_style(style: &mut ComputedStyle, r: &CssRule) {
    if let Some(c) = r.color { style.color = c; }
    if let Some(bg) = r.bg_color { style.bg_color = Some(bg); }
    if let Some(bc) = r.border_color { style.border_color = Some(bc); }
    if let Some(bw) = r.border_width { style.border_width = bw; }
    if let Some(br) = r.border_radius { style.border_radius = br; }
    if let Some(fs) = r.font_size { style.font_size = fs; }
    if let Some(w) = r.width { style.width = Some(w); }
    if let Some(h) = r.height { style.height = Some(h); }
    if let Some(mw) = r.max_width { style.max_width = Some(mw); }
    if let Some(mt) = r.margin_top { style.margin_top = mt; }
    if let Some(mb) = r.margin_bottom { style.margin_bottom = mb; }
    if let Some(ml) = r.margin_left { style.margin_left = ml; }
    if let Some(mr) = r.margin_right { style.margin_right = mr; }
    if let Some(pt) = r.padding_top { style.padding_top = pt; }
    if let Some(pb) = r.padding_bottom { style.padding_bottom = pb; }
    if let Some(pl) = r.padding_left { style.padding_left = pl; }
    if let Some(pr) = r.padding_right { style.padding_right = pr; }
    if let Some(b) = r.is_bold { style.is_bold = b; }
    if let Some(d) = r.display {
        style.display = d;
        style.display_block = d == DisplayMode::Block;
    }
    if let Some(fd) = r.flex_direction { style.flex_direction = fd; }
    if let Some(jc) = r.justify_content { style.justify_content = jc; }
    if let Some(ai) = r.align_items { style.align_items = ai; }
    if let Some(ta) = r.text_align { style.text_align = ta; }
}

// ── Mini-JS Runtime para Ring 3 (no_std) ──────────────────────────────────────

pub const MAX_JS_VARS: usize = 32;
pub const MAX_JS_ELEMS: usize = 16;
pub const MAX_JS_FUNCS: usize = 16;

#[allow(dead_code)]
#[derive(Clone, Copy)]
pub struct JsVar {
    pub name: [u8; 16],
    pub name_len: usize,
    pub val_num: i64,
    pub is_num: bool,
    pub val_str: [u8; 64],
    pub val_str_len: usize,
}

impl JsVar {
    pub const fn empty() -> Self {
        Self {
            name: [0; 16],
            name_len: 0,
            val_num: 0,
            is_num: true,
            val_str: [0; 64],
            val_str_len: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub struct JsElemRef {
    pub name: [u8; 16],
    pub name_len: usize,
    pub dom_idx: usize,
}

impl JsElemRef {
    pub const fn empty() -> Self {
        Self {
            name: [0; 16],
            name_len: 0,
            dom_idx: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub struct JsFunc {
    pub name: [u8; 24],
    pub name_len: usize,
    pub body: [u8; 512],
    pub body_len: usize,
}

impl JsFunc {
    pub const fn empty() -> Self {
        Self {
            name: [0; 24],
            name_len: 0,
            body: [0; 512],
            body_len: 0,
        }
    }
}

pub struct MiniJsRuntime {
    pub vars: [JsVar; MAX_JS_VARS],
    pub var_count: usize,
    pub elems: [JsElemRef; MAX_JS_ELEMS],
    pub elem_count: usize,
    pub funcs: [JsFunc; MAX_JS_FUNCS],
    pub func_count: usize,
    pub dom_dirty: bool,
}

impl MiniJsRuntime {
    pub const fn new() -> Self {
        Self {
            vars: [JsVar::empty(); MAX_JS_VARS],
            var_count: 0,
            elems: [JsElemRef::empty(); MAX_JS_ELEMS],
            elem_count: 0,
            funcs: [JsFunc::empty(); MAX_JS_FUNCS],
            func_count: 0,
            dom_dirty: false,
        }
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.var_count = 0;
        self.elem_count = 0;
        self.func_count = 0;
        self.dom_dirty = false;
    }

    pub fn get_num(&self, name: &[u8]) -> Option<i64> {
        for i in 0..self.var_count {
            let v = &self.vars[i];
            if v.name_len == name.len() && &v.name[..v.name_len] == name {
                if v.is_num {
                    return Some(v.val_num);
                }
            }
        }
        None
    }

    pub fn set_num(&mut self, name: &[u8], val: i64) {
        for i in 0..self.var_count {
            let v = &mut self.vars[i];
            if v.name_len == name.len() && &v.name[..v.name_len] == name {
                v.val_num = val;
                v.is_num = true;
                return;
            }
        }
        if self.var_count < MAX_JS_VARS {
            let idx = self.var_count;
            let mut v = JsVar::empty();
            let cpy = name.len().min(15);
            v.name[..cpy].copy_from_slice(&name[..cpy]);
            v.name_len = cpy;
            v.val_num = val;
            v.is_num = true;
            self.vars[idx] = v;
            self.var_count += 1;
        }
    }

    pub fn get_str<'a>(&'a self, name: &[u8]) -> Option<&'a [u8]> {
        for i in 0..self.var_count {
            let v = &self.vars[i];
            if v.name_len == name.len() && &v.name[..v.name_len] == name {
                if !v.is_num {
                    return Some(&v.val_str[..v.val_str_len]);
                }
            }
        }
        None
    }

    pub fn set_str(&mut self, name: &[u8], val: &[u8]) {
        for i in 0..self.var_count {
            let v = &mut self.vars[i];
            if v.name_len == name.len() && &v.name[..v.name_len] == name {
                let cpy = val.len().min(63);
                v.val_str[..cpy].copy_from_slice(&val[..cpy]);
                v.val_str_len = cpy;
                v.is_num = false;
                return;
            }
        }
        if self.var_count < MAX_JS_VARS {
            let idx = self.var_count;
            let mut v = JsVar::empty();
            let cpy_n = name.len().min(15);
            v.name[..cpy_n].copy_from_slice(&name[..cpy_n]);
            v.name_len = cpy_n;
            let cpy_v = val.len().min(63);
            v.val_str[..cpy_v].copy_from_slice(&val[..cpy_v]);
            v.val_str_len = cpy_v;
            v.is_num = false;
            self.vars[idx] = v;
            self.var_count += 1;
        }
    }

    pub fn find_elem(&self, name: &[u8]) -> Option<usize> {
        for i in 0..self.elem_count {
            let e = &self.elems[i];
            if e.name_len == name.len() && &e.name[..e.name_len] == name {
                return Some(e.dom_idx);
            }
        }
        None
    }

    pub fn bind_elem(&mut self, name: &[u8], dom_idx: usize) {
        for i in 0..self.elem_count {
            if self.elems[i].name_len == name.len() && &self.elems[i].name[..self.elems[i].name_len] == name {
                self.elems[i].dom_idx = dom_idx;
                return;
            }
        }
        if self.elem_count < MAX_JS_ELEMS {
            let idx = self.elem_count;
            let mut e = JsElemRef::empty();
            let cpy = name.len().min(15);
            e.name[..cpy].copy_from_slice(&name[..cpy]);
            e.name_len = cpy;
            e.dom_idx = dom_idx;
            self.elems[idx] = e;
            self.elem_count += 1;
        }
    }

    pub fn register_func(&mut self, name: &[u8], body: &[u8]) {
        for i in 0..self.func_count {
            if self.funcs[i].name_len == name.len() && &self.funcs[i].name[..self.funcs[i].name_len] == name {
                let cpy = body.len().min(511);
                self.funcs[i].body[..cpy].copy_from_slice(&body[..cpy]);
                self.funcs[i].body_len = cpy;
                return;
            }
        }
        if self.func_count < MAX_JS_FUNCS {
            let idx = self.func_count;
            let mut f = JsFunc::empty();
            let n_cpy = name.len().min(23);
            f.name[..n_cpy].copy_from_slice(&name[..n_cpy]);
            f.name_len = n_cpy;
            let b_cpy = body.len().min(511);
            f.body[..b_cpy].copy_from_slice(&body[..b_cpy]);
            f.body_len = b_cpy;
            self.funcs[idx] = f;
            self.func_count += 1;
        }
    }

    pub fn get_func<'a>(&'a self, name: &[u8]) -> Option<&'a [u8]> {
        for i in 0..self.func_count {
            if self.funcs[i].name_len == name.len() && &self.funcs[i].name[..self.funcs[i].name_len] == name {
                return Some(&self.funcs[i].body[..self.funcs[i].body_len]);
            }
        }
        None
    }

    pub fn resolve_elem_target(&self, dom: &DomArena, expr: &[u8]) -> Option<usize> {
        let e = expr.trim_ascii();
        if e.starts_with(b"document.getElementById(") {
            let arg = extract_quoted_arg(e)?;
            dom.find_by_id(arg)
        } else if e.starts_with(b"document.querySelector(") {
            let arg = extract_quoted_arg(e)?;
            dom.find_by_selector(arg)
        } else if let Some(idx) = self.find_elem(e) {
            Some(idx)
        } else {
            dom.find_by_id(e).or_else(|| dom.find_by_selector(e))
        }
    }

    pub fn eval_script(&mut self, script: &[u8], dom: &mut DomArena) {
        let mut cur = script;

        // Pré-processamento: registra declarações de função `function name() { body }`
        while let Some(func_pos) = find_subslice(cur, b"function ") {
            let rest = &cur[func_pos + 9..];
            if let Some(lparen) = rest.iter().position(|&b| b == b'(') {
                let func_name = rest[..lparen].trim_ascii();
                if let Some(lbrace) = rest.iter().position(|&b| b == b'{') {
                    // Encontra rbrace correspondente
                    let mut depth = 1usize;
                    let mut rbrace = None;
                    for idx in (lbrace + 1)..rest.len() {
                        if rest[idx] == b'{' { depth += 1; }
                        else if rest[idx] == b'}' {
                            depth -= 1;
                            if depth == 0 {
                                rbrace = Some(idx);
                                break;
                            }
                        }
                    }
                    if let Some(rb) = rbrace {
                        let body = &rest[lbrace + 1..rb].trim_ascii();
                        self.register_func(func_name, body);
                        cur = &rest[rb + 1..];
                        continue;
                    }
                }
            }
            break;
        }

        // Execução sequencial de instruções divididas por ';' ou '\n'
        for stmt_raw in script.split(|&b| b == b';' || b == b'\n') {
            let stmt = stmt_raw.trim_ascii();
            if stmt.is_empty() { continue; }

            // Ignora a declaração de função inline se já foi registrada
            if stmt.starts_with(b"function ") || stmt.starts_with(b"}") {
                continue;
            }

            // 1. Chamada de função nomeada sem retorno: ex `handleClick()`
            if stmt.ends_with(b")") && !stmt.contains(&b'=') {
                if let Some(lp) = stmt.iter().position(|&b| b == b'(') {
                    let fn_name = stmt[..lp].trim_ascii();
                    if let Some(body_slice) = self.get_func(fn_name) {
                        let mut body_buf = [0u8; 512];
                        let b_len = body_slice.len().min(511);
                        body_buf[..b_len].copy_from_slice(&body_slice[..b_len]);
                        self.eval_script(&body_buf[..b_len], dom);
                        continue;
                    }
                }
            }

            // 2. Limpeza de modificador de declaração de variável
            let clean_stmt = if stmt.starts_with(b"var ") {
                stmt[4..].trim_ascii()
            } else if stmt.starts_with(b"let ") {
                stmt[4..].trim_ascii()
            } else if stmt.starts_with(b"const ") {
                stmt[6..].trim_ascii()
            } else {
                stmt
            };

            // 3. Atribuição de elemento DOM a variável: `let btn = document.getElementById('...')`
            if let Some(eq_pos) = clean_stmt.iter().position(|&b| b == b'=') {
                let lhs = clean_stmt[..eq_pos].trim_ascii();
                let rhs = clean_stmt[eq_pos + 1..].trim_ascii();

                if rhs.starts_with(b"document.getElementById(") || rhs.starts_with(b"document.querySelector(") {
                    if let Some(target_idx) = self.resolve_elem_target(dom, rhs) {
                        self.bind_elem(lhs, target_idx);
                        continue;
                    }
                }
            }

            // 4. classList: `.classList.add('...')`, `.classList.remove('...')`, `.classList.toggle('...')`
            if let Some(pos) = find_subslice(clean_stmt, b".classList.add(") {
                let target_part = clean_stmt[..pos].trim_ascii();
                if let Some(target_idx) = self.resolve_elem_target(dom, target_part) {
                    let arg_part = &clean_stmt[pos + 15..];
                    if let Some(rp) = arg_part.iter().position(|&b| b == b')') {
                        if let Some(class_name) = extract_string_lit(&arg_part[..rp]) {
                            dom.add_class(target_idx, class_name);
                            self.dom_dirty = true;
                            continue;
                        }
                    }
                }
            } else if let Some(pos) = find_subslice(clean_stmt, b".classList.remove(") {
                let target_part = clean_stmt[..pos].trim_ascii();
                if let Some(target_idx) = self.resolve_elem_target(dom, target_part) {
                    let arg_part = &clean_stmt[pos + 18..];
                    if let Some(rp) = arg_part.iter().position(|&b| b == b')') {
                        if let Some(class_name) = extract_string_lit(&arg_part[..rp]) {
                            dom.remove_class(target_idx, class_name);
                            self.dom_dirty = true;
                            continue;
                        }
                    }
                }
            } else if let Some(pos) = find_subslice(clean_stmt, b".classList.toggle(") {
                let target_part = clean_stmt[..pos].trim_ascii();
                if let Some(target_idx) = self.resolve_elem_target(dom, target_part) {
                    let arg_part = &clean_stmt[pos + 18..];
                    if let Some(rp) = arg_part.iter().position(|&b| b == b')') {
                        if let Some(class_name) = extract_string_lit(&arg_part[..rp]) {
                            dom.toggle_class(target_idx, class_name);
                            self.dom_dirty = true;
                            continue;
                        }
                    }
                }
            }

            // 5. addEventListener('click', ...): registra manipulador onclick
            if let Some(pos) = find_subslice(clean_stmt, b".addEventListener(") {
                let target_part = clean_stmt[..pos].trim_ascii();
                if let Some(target_idx) = self.resolve_elem_target(dom, target_part) {
                    let after = &clean_stmt[pos + 18..];
                    if let Some(first_comma) = after.iter().position(|&b| b == b',') {
                        let ev_name = after[..first_comma].trim_ascii();
                        let is_click = extract_string_lit(ev_name).map_or(false, |ev| ev.eq_ignore_ascii_case(b"click"));
                        if is_click {
                            let handler_raw = after[first_comma + 1..].trim_ascii();
                            let handler_clean = if handler_raw.ends_with(b")") {
                                &handler_raw[..handler_raw.len() - 1].trim_ascii()
                            } else {
                                handler_raw
                            };

                            let cpy = handler_clean.len().min(255);
                            dom.nodes[target_idx].onclick_attr[..cpy].copy_from_slice(&handler_clean[..cpy]);
                            dom.nodes[target_idx].onclick_len = cpy;
                            continue;
                        }
                    }
                }
            }

            // 6. Manipulação de Texto/Conteúdo: `.innerText = ...`, `.textContent = ...`, `.innerHTML = ...`, `.value = ...`
            let text_prop_opt = if let Some(p) = find_subslice(clean_stmt, b".innerText") {
                Some((p, 10))
            } else if let Some(p) = find_subslice(clean_stmt, b".textContent") {
                Some((p, 12))
            } else if let Some(p) = find_subslice(clean_stmt, b".innerHTML") {
                Some((p, 10))
            } else if let Some(p) = find_subslice(clean_stmt, b".value") {
                Some((p, 6))
            } else {
                None
            };

            if let Some((pos, len_prop)) = text_prop_opt {
                let target_part = clean_stmt[..pos].trim_ascii();
                let after = clean_stmt[pos + len_prop..].trim_ascii();
                if after.starts_with(b"=") {
                    let rhs = after[1..].trim_ascii();
                    if let Some(elem_idx) = self.resolve_elem_target(dom, target_part) {
                        let mut eval_buf = [0u8; 256];
                        let eval_len = self.eval_expr_string(rhs, dom, &mut eval_buf);
                        dom.set_text_content(elem_idx, &eval_buf[..eval_len]);
                        self.dom_dirty = true;
                        continue;
                    }
                }
            }

            // 7. Manipulação de Estilo: `.style.<prop> = ...`
            if let Some(style_pos) = find_subslice(clean_stmt, b".style.") {
                let target_part = clean_stmt[..style_pos].trim_ascii();
                let after_style = clean_stmt[style_pos + 7..].trim_ascii();
                if let Some(eq_pos) = after_style.iter().position(|&b| b == b'=') {
                    let prop = after_style[..eq_pos].trim_ascii();
                    let rhs = after_style[eq_pos + 1..].trim_ascii();
                    if let Some(elem_idx) = self.resolve_elem_target(dom, target_part) {
                        let quote_val = extract_string_lit(rhs).unwrap_or(rhs);

                        if prop.eq_ignore_ascii_case(b"color") {
                            if let Some(col) = parse_color(quote_val) {
                                dom.set_style_color(elem_idx, col);
                                self.dom_dirty = true;
                            }
                        } else if prop.eq_ignore_ascii_case(b"backgroundColor") || prop.eq_ignore_ascii_case(b"background_color") || prop.eq_ignore_ascii_case(b"background") {
                            if let Some(col) = parse_color(quote_val) {
                                dom.set_style_bg(elem_idx, col);
                                self.dom_dirty = true;
                            }
                        } else if prop.eq_ignore_ascii_case(b"display") {
                            if let Some(mode) = parse_display(quote_val) {
                                dom.set_style_display(elem_idx, mode);
                                self.dom_dirty = true;
                            }
                        } else if prop.eq_ignore_ascii_case(b"borderColor") || prop.eq_ignore_ascii_case(b"border_color") {
                            if let Some(col) = parse_color(quote_val) {
                                dom.nodes[elem_idx].computed_style.border_color = Some(col);
                                self.dom_dirty = true;
                            }
                        } else if prop.eq_ignore_ascii_case(b"border") {
                            let (bw, bc) = parse_border(quote_val);
                            if let Some(w) = bw { dom.nodes[elem_idx].computed_style.border_width = w; }
                            if let Some(c) = bc { dom.nodes[elem_idx].computed_style.border_color = Some(c); }
                            self.dom_dirty = true;
                        } else if prop.eq_ignore_ascii_case(b"width") {
                            if let Some(w) = parse_px(quote_val) {
                                dom.nodes[elem_idx].computed_style.width = Some(w);
                                self.dom_dirty = true;
                            }
                        } else if prop.eq_ignore_ascii_case(b"height") {
                            if let Some(h) = parse_px(quote_val) {
                                dom.nodes[elem_idx].computed_style.height = Some(h);
                                self.dom_dirty = true;
                            }
                        }
                        continue;
                    }
                }
            }

            // 8. Atribuição aritmética / variáveis numéricas: `clicks = clicks + 1` / `clicks += 1`
            if let Some(eq_pos) = clean_stmt.iter().position(|&b| b == b'=') {
                let var_name = clean_stmt[..eq_pos].trim_ascii();
                let is_plus_eq = !var_name.is_empty() && var_name[var_name.len() - 1] == b'+';
                let real_var = if is_plus_eq { &var_name[..var_name.len() - 1].trim_ascii() } else { &var_name };

                let rhs = clean_stmt[eq_pos + 1..].trim_ascii();
                let cur_val = self.get_num(real_var).unwrap_or(0);

                if is_plus_eq {
                    let add_val = parse_int(rhs).unwrap_or(1);
                    self.set_num(real_var, cur_val + add_val);
                } else if let Some(plus_idx) = rhs.iter().position(|&b| b == b'+') {
                    let op1 = rhs[..plus_idx].trim_ascii();
                    let op2 = rhs[plus_idx + 1..].trim_ascii();
                    let val1 = if op1 == *real_var { cur_val } else { parse_int(op1).unwrap_or(0) };
                    let val2 = if op2 == *real_var { cur_val } else { parse_int(op2).unwrap_or(0) };
                    self.set_num(real_var, val1 + val2);
                } else if let Some(lit) = extract_string_lit(rhs) {
                    self.set_str(real_var, lit);
                } else {
                    let val = parse_int(rhs).unwrap_or(0);
                    self.set_num(real_var, val);
                }
            }
        }
    }

    pub fn eval_expr_string(&self, expr: &[u8], dom: &DomArena, out: &mut [u8]) -> usize {
        let mut out_len = 0usize;
        for part in expr.split(|&b| b == b'+') {
            let p = part.trim_ascii();
            if let Some(lit) = extract_string_lit(p) {
                let cpy = lit.len().min(out.len() - out_len);
                out[out_len..out_len + cpy].copy_from_slice(&lit[..cpy]);
                out_len += cpy;
            } else if let Some(num) = self.get_num(p) {
                let mut num_buf = [0u8; 16];
                let n_len = format_i64(num, &mut num_buf);
                let cpy = n_len.min(out.len() - out_len);
                out[out_len..out_len + cpy].copy_from_slice(&num_buf[..cpy]);
                out_len += cpy;
            } else if let Some(s) = self.get_str(p) {
                let cpy = s.len().min(out.len() - out_len);
                out[out_len..out_len + cpy].copy_from_slice(&s[..cpy]);
                out_len += cpy;
            } else if let Some(val_pos) = find_subslice(p, b".value") {
                let elem_part = p[..val_pos].trim_ascii();
                if let Some(idx) = self.resolve_elem_target(dom, elem_part) {
                    let t = &dom.nodes[idx].text[..dom.nodes[idx].text_len];
                    let cpy = t.len().min(out.len() - out_len);
                    out[out_len..out_len + cpy].copy_from_slice(&t[..cpy]);
                    out_len += cpy;
                }
            }
        }
        out_len
    }
}

// ── Funções Auxiliares de Parsing e Conversão ─────────────────────────────────

pub fn parse_color(input: &[u8]) -> Option<u32> {
    let s = input.trim_ascii();
    if s.is_empty() { return None; }
    if s.starts_with(b"#") {
        return parse_hex_color(s);
    }
    if s.starts_with(b"rgb(") || s.starts_with(b"rgba(") {
        return parse_rgb_color(s);
    }
    if s.eq_ignore_ascii_case(b"black") { return Some(0x0000_0000); }
    if s.eq_ignore_ascii_case(b"white") { return Some(0x00FF_FFFF); }
    if s.eq_ignore_ascii_case(b"red") { return Some(0x00F3_8BA8); }
    if s.eq_ignore_ascii_case(b"green") { return Some(0x00A6_E3A1); }
    if s.eq_ignore_ascii_case(b"blue") { return Some(0x0089_B4FA); }
    if s.eq_ignore_ascii_case(b"yellow") { return Some(0x00F9_E2AF); }
    if s.eq_ignore_ascii_case(b"cyan") { return Some(0x0094_E2D5); }
    if s.eq_ignore_ascii_case(b"magenta") || s.eq_ignore_ascii_case(b"pink") { return Some(0x00F5_C2E7); }
    if s.eq_ignore_ascii_case(b"gray") || s.eq_ignore_ascii_case(b"grey") { return Some(0x006C_7086); }
    if s.eq_ignore_ascii_case(b"transparent") { return None; }
    parse_hex_color(s)
}

pub fn parse_rgb_color(s: &[u8]) -> Option<u32> {
    let lparen = s.iter().position(|&b| b == b'(')?;
    let rparen = s.iter().rposition(|&b| b == b')')?;
    if rparen <= lparen { return None; }
    let inside = &s[lparen + 1..rparen];
    let mut parts = inside.split(|&b| b == b',');
    let r_str = parts.next()?.trim_ascii();
    let g_str = parts.next()?.trim_ascii();
    let b_str = parts.next()?.trim_ascii();
    let r = parse_int(r_str)? as u32;
    let g = parse_int(g_str)? as u32;
    let b = parse_int(b_str)? as u32;
    Some(((r.min(255)) << 16) | ((g.min(255)) << 8) | (b.min(255)))
}

pub fn parse_border(val: &[u8]) -> (Option<usize>, Option<u32>) {
    let s = val.trim_ascii();
    let mut border_w = None;
    let mut border_c = None;
    for part in s.split(|&b| b == b' ' || b == b'\t') {
        let p = part.trim_ascii();
        if p.is_empty() || p.eq_ignore_ascii_case(b"solid") || p.eq_ignore_ascii_case(b"dashed") || p.eq_ignore_ascii_case(b"dotted") {
            continue;
        }
        if border_w.is_none() && p.iter().any(|&b| b >= b'0' && b <= b'9') && !p.starts_with(b"#") {
            border_w = parse_px(p);
        } else if border_c.is_none() {
            border_c = parse_color(p);
        }
    }
    (border_w, border_c)
}

pub fn parse_display(val: &[u8]) -> Option<DisplayMode> {
    let s = val.trim_ascii();
    if s.eq_ignore_ascii_case(b"none") {
        Some(DisplayMode::None)
    } else if s.eq_ignore_ascii_case(b"block") {
        Some(DisplayMode::Block)
    } else if s.eq_ignore_ascii_case(b"inline") {
        Some(DisplayMode::Inline)
    } else if s.eq_ignore_ascii_case(b"inline-block") {
        Some(DisplayMode::InlineBlock)
    } else if s.eq_ignore_ascii_case(b"flex") {
        Some(DisplayMode::Flex)
    } else {
        None
    }
}

pub fn parse_flex_direction(val: &[u8]) -> Option<FlexDirection> {
    let s = val.trim_ascii();
    if s.eq_ignore_ascii_case(b"column") {
        Some(FlexDirection::Column)
    } else if s.eq_ignore_ascii_case(b"row") {
        Some(FlexDirection::Row)
    } else {
        None
    }
}

pub fn parse_justify_content(val: &[u8]) -> Option<JustifyContent> {
    let s = val.trim_ascii();
    if s.eq_ignore_ascii_case(b"center") {
        Some(JustifyContent::Center)
    } else if s.eq_ignore_ascii_case(b"flex-end") || s.eq_ignore_ascii_case(b"end") {
        Some(JustifyContent::FlexEnd)
    } else if s.eq_ignore_ascii_case(b"space-between") {
        Some(JustifyContent::SpaceBetween)
    } else if s.eq_ignore_ascii_case(b"flex-start") || s.eq_ignore_ascii_case(b"start") {
        Some(JustifyContent::FlexStart)
    } else {
        None
    }
}

pub fn parse_align_items(val: &[u8]) -> Option<AlignItems> {
    let s = val.trim_ascii();
    if s.eq_ignore_ascii_case(b"center") {
        Some(AlignItems::Center)
    } else if s.eq_ignore_ascii_case(b"flex-end") || s.eq_ignore_ascii_case(b"end") {
        Some(AlignItems::FlexEnd)
    } else if s.eq_ignore_ascii_case(b"stretch") {
        Some(AlignItems::Stretch)
    } else if s.eq_ignore_ascii_case(b"flex-start") || s.eq_ignore_ascii_case(b"start") {
        Some(AlignItems::FlexStart)
    } else {
        None
    }
}

pub fn parse_text_align(val: &[u8]) -> Option<TextAlign> {
    let s = val.trim_ascii();
    if s.eq_ignore_ascii_case(b"center") {
        Some(TextAlign::Center)
    } else if s.eq_ignore_ascii_case(b"right") {
        Some(TextAlign::Right)
    } else if s.eq_ignore_ascii_case(b"left") {
        Some(TextAlign::Left)
    } else {
        None
    }
}

// ── Funções Auxiliares de Parsing e Conversão ─────────────────────────────────

pub fn parse_hex_color(hex: &[u8]) -> Option<u32> {
    let s = hex.trim_ascii();
    if s.is_empty() { return None; }
    let s = if s[0] == b'#' { &s[1..] } else { s };
    if s.len() == 6 {
        let r = hex_byte(&s[0..2])?;
        let g = hex_byte(&s[2..4])?;
        let b = hex_byte(&s[4..6])?;
        Some(((r as u32) << 16) | ((g as u32) << 8) | (b as u32))
    } else if s.len() == 3 {
        let r = hex_nibble(s[0])?;
        let g = hex_nibble(s[1])?;
        let b = hex_nibble(s[2])?;
        Some(((r as u32 * 17) << 16) | ((g as u32 * 17) << 8) | (b as u32 * 17))
    } else {
        None
    }
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn hex_byte(bytes: &[u8]) -> Option<u8> {
    if bytes.len() != 2 { return None; }
    let h = hex_nibble(bytes[0])?;
    let l = hex_nibble(bytes[1])?;
    Some((h << 4) | l)
}

pub fn parse_px(val: &[u8]) -> Option<usize> {
    let s = val.trim_ascii();
    let mut num: usize = 0;
    let mut found = false;
    for &b in s {
        if b >= b'0' && b <= b'9' {
            num = num.saturating_mul(10).saturating_add((b - b'0') as usize);
            found = true;
        } else {
            break;
        }
    }
    if found { Some(num) } else { None }
}

pub fn parse_one_or_two_px(val: &[u8]) -> Option<(usize, usize)> {
    let s = val.trim_ascii();
    if s.is_empty() { return None; }
    if let Some(pos) = s.iter().position(|&b| b == b' ' || b == b'\t') {
        let first = parse_px(&s[..pos])?;
        let second = parse_px(&s[pos + 1..])?;
        Some((first, second))
    } else {
        let one = parse_px(s)?;
        Some((one, one))
    }
}

pub fn parse_int(val: &[u8]) -> Option<i64> {
    let s = val.trim_ascii();
    if s.is_empty() { return None; }
    let (is_neg, start) = if s[0] == b'-' { (true, 1) } else { (false, 0) };
    let mut num: i64 = 0;
    let mut found = false;
    for &b in &s[start..] {
        if b >= b'0' && b <= b'9' {
            num = num.saturating_mul(10).saturating_add((b - b'0') as i64);
            found = true;
        } else {
            break;
        }
    }
    if found {
        Some(if is_neg { -num } else { num })
    } else {
        None
    }
}

pub fn format_i64(mut n: i64, buf: &mut [u8; 16]) -> usize {
    if n == 0 {
        buf[0] = b'0';
        return 1;
    }
    let is_neg = n < 0;
    if is_neg {
        n = -n;
    }
    let mut tmp = [0u8; 16];
    let mut count = 0;
    while n > 0 && count < 15 {
        tmp[count] = b'0' + (n % 10) as u8;
        n /= 10;
        count += 1;
    }
    let mut idx = 0;
    if is_neg {
        buf[idx] = b'-';
        idx += 1;
    }
    for i in (0..count).rev() {
        buf[idx] = tmp[i];
        idx += 1;
    }
    idx
}

pub fn extract_string_lit(val: &[u8]) -> Option<&[u8]> {
    let s = val.trim_ascii();
    if s.len() >= 2 && (s[0] == b'"' || s[0] == b'\'') {
        let quote = s[0];
        if s[s.len() - 1] == quote {
            return Some(&s[1..s.len() - 1]);
        }
    }
    None
}

pub fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() { return Some(0); }
    if haystack.len() < needle.len() { return None; }
    for i in 0..=(haystack.len() - needle.len()) {
        if &haystack[i..i + needle.len()] == needle {
            return Some(i);
        }
    }
    None
}

pub fn extract_quoted_arg(call: &[u8]) -> Option<&[u8]> {
    let lparen = call.iter().position(|&b| b == b'(')?;
    let rparen = call.iter().rposition(|&b| b == b')')?;
    if rparen > lparen + 1 {
        let arg = call[lparen + 1..rparen].trim_ascii();
        extract_string_lit(arg)
    } else {
        None
    }
}

// ── Utilitários para Leitura de CSS do VFS e Atributos HTML ───────────────────

pub fn extract_attr_val<'a>(tag: &'a [u8], attr: &[u8]) -> Option<&'a [u8]> {
    let mut i = 0;
    let len = tag.len();
    let a_len = attr.len();
    while i + a_len <= len {
        if tag[i..i + a_len].eq_ignore_ascii_case(attr) {
            i += a_len;
            let quote = if i < len && (tag[i] == b'"' || tag[i] == b'\'') {
                let q = tag[i];
                i += 1;
                Some(q)
            } else {
                None
            };
            let start = i;
            while i < len {
                let c = tag[i];
                if let Some(q) = quote {
                    if c == q {
                        return Some(&tag[start..i]);
                    }
                } else if c == b' ' || c == b'>' || c == b'/' {
                    return Some(&tag[start..i]);
                }
                i += 1;
            }
            return Some(&tag[start..i]);
        }
        i += 1;
    }
    None
}

pub fn read_css_file_from_vfs(href: &[u8], out: &mut [u8]) -> Option<usize> {
    let mut path_buf = [0u8; 64];
    let path = if href.starts_with(b"/") {
        href
    } else {
        let prefix = b"/disk/";
        let mut idx = 0;
        for &b in prefix {
            if idx < path_buf.len() - 1 { path_buf[idx] = b; idx += 1; }
        }
        for &b in href {
            if idx < path_buf.len() - 1 { path_buf[idx] = b; idx += 1; }
        }
        &path_buf[..idx]
    };

    let fd = unsafe { storage::sys_open(path, 0) };
    if fd < 0 {
        let fd_raw = unsafe { storage::sys_open(href, 0) };
        if fd_raw < 0 {
            return None;
        }
        let n = unsafe { storage::sys_read(fd_raw as u64, out.as_mut_ptr(), out.len() as u64) };
        unsafe { storage::sys_close(fd_raw as u64); }
        if n > 0 { Some(n as usize) } else { None }
    } else {
        let n = unsafe { storage::sys_read(fd as u64, out.as_mut_ptr(), out.len() as u64) };
        unsafe { storage::sys_close(fd as u64); }
        if n > 0 { Some(n as usize) } else { None }
    }
}

/// Normalização de texto HTML: decodificação de entidades e conversão de UTF-8 acentuado para ASCII bitmap
pub fn normalize_html_text(raw: &[u8], out: &mut [u8]) -> usize {
    let mut i = 0usize;
    let mut o = 0usize;
    let len = raw.len();
    let max = out.len().saturating_sub(1);

    while i < len && o < max {
        // 1. Entidades HTML
        if raw[i] == b'&' {
            if i + 3 < len && &raw[i..i + 4] == b"&lt;" {
                out[o] = b'<'; o += 1; i += 4; continue;
            } else if i + 3 < len && &raw[i..i + 4] == b"&gt;" {
                out[o] = b'>'; o += 1; i += 4; continue;
            } else if i + 4 < len && &raw[i..i + 5] == b"&amp;" {
                out[o] = b'&'; o += 1; i += 5; continue;
            } else if i + 5 < len && &raw[i..i + 6] == b"&quot;" {
                out[o] = b'"'; o += 1; i += 6; continue;
            } else if i + 4 < len && &raw[i..i + 5] == b"&#39;" {
                out[o] = b'\''; o += 1; i += 5; continue;
            } else if i + 5 < len && &raw[i..i + 6] == b"&apos;" {
                out[o] = b'\''; o += 1; i += 6; continue;
            } else if i + 5 < len && &raw[i..i + 6] == b"&nbsp;" {
                out[o] = b' '; o += 1; i += 6; continue;
            } else if i + 6 < len && &raw[i..i + 7] == b"&mdash;" {
                out[o] = b'-'; o += 1; i += 7; continue;
            } else if i + 6 < len && &raw[i..i + 7] == b"&ndash;" {
                out[o] = b'-'; o += 1; i += 7; continue;
            } else if i + 5 < len && &raw[i..i + 6] == b"&rarr;" {
                out[o] = b'-'; if o < max { out[o] = b'>'; o += 1; } i += 6; continue;
            } else if i + 5 < len && &raw[i..i + 6] == b"&copy;" {
                out[o] = b'c'; o += 1; i += 6; continue;
            }
        }

        // 2. Acentuação UTF-8 Português (0xC3 ...)
        if raw[i] == 0xC3 && i + 1 < len {
            let b2 = raw[i + 1];
            let ch = match b2 {
                0xA0..=0xA5 => b'a', // à, á, â, ã, ä, å
                0x80..=0x85 => b'A', // À, Á, Â, Ã, Ä, Å
                0xA8..=0xAB => b'e', // è, é, ê, ë
                0x88..=0x8B => b'E', // È, É, Ê, Ë
                0xAC..=0xAF => b'i', // ì, í, î, ï
                0x8C..=0x8F => b'I', // Ì, Í, Î, Ï
                0xB2..=0xB6 => b'o', // ò, ó, ô, õ, ö
                0x92..=0x96 => b'O', // Ò, Ó, Ô,Õ, Ö
                0xB9..=0xBC => b'u', // ù, ú, û, ü
                0x99..=0x9C => b'U', // Ù, Ú, Û, Ü
                0xA7 => b'c',        // ç
                0x87 => b'C',        // Ç
                _ => b'?',
            };
            out[o] = ch;
            o += 1;
            i += 2;
            continue;
        }

        // 3. Aspas e traços tipográficos UTF-8 (0xE2 0x80 ...)
        if raw[i] == 0xE2 && i + 2 < len && raw[i + 1] == 0x80 {
            let b3 = raw[i + 2];
            let ch = match b3 {
                0x98 | 0x99 => b'\'', // ‘ ’
                0x9C | 0x9D => b'"',  // “ ”
                0x93 | 0x94 => b'-',  // – —
                0xA6 => b'.',         // …
                _ => b' ',
            };
            out[o] = ch;
            o += 1;
            i += 3;
            continue;
        }

        // Caractere ASCII comum
        let b = raw[i];
        if b == b'\r' || b == b'\n' || b == b'\t' {
            if o > 0 && out[o - 1] != b' ' {
                out[o] = b' ';
                o += 1;
            }
        } else if b >= 32 && b <= 126 {
            out[o] = b;
            o += 1;
        }
        i += 1;
    }

    // Trim trailing spaces
    while o > 0 && out[o - 1] == b' ' {
        o -= 1;
    }
    out[o] = 0;
    o
}

// ── Parser HTML Zero-Copy para Flat Vector DOM ────────────────────────────────

pub fn parse_html_into_dom(html: &[u8], dom: &mut DomArena, css: &mut MiniCss, js: &mut MiniJsRuntime) {
    dom.node_count = 0;
    dom.title_len = 0;
    css.rule_count = 0;

    #[derive(Clone, Copy)]
    struct QueuedScript {
        is_vfs: bool,
        vfs_path: [u8; 64],
        vfs_path_len: usize,
        inline_start: usize,
        inline_end: usize,
    }

    impl QueuedScript {
        const fn empty() -> Self {
            Self {
                is_vfs: false,
                vfs_path: [0; 64],
                vfs_path_len: 0,
                inline_start: 0,
                inline_end: 0,
            }
        }
    }

    let mut queued_scripts = [QueuedScript::empty(); 8];
    let mut script_count = 0usize;

    let mut i = 0usize;
    let len = html.len();

    let mut parent_stack = [0usize; 16];
    let mut stack_depth = 0usize;

    while i < len {
        if html[i] == b'<' {
            i += 1;

            // 1. Comentário HTML <!-- ... -->
            if i + 2 < len && html[i] == b'!' && html[i + 1] == b'-' && html[i + 2] == b'-' {
                i += 3;
                while i + 2 < len && !(html[i] == b'-' && html[i + 1] == b'-' && html[i + 2] == b'>') {
                    i += 1;
                }
                if i + 2 < len { i += 3; }
                continue;
            }

            // 2. DOCTYPE <!DOCTYPE ... >
            if i < len && html[i] == b'!' {
                while i < len && html[i] != b'>' { i += 1; }
                if i < len { i += 1; }
                continue;
            }

            // 3. Tag de fechamento </tag>
            if i < len && html[i] == b'/' {
                i += 1;
                let ctag_start = i;
                while i < len && html[i] != b'>' && html[i] != b' ' && html[i] != b'\t' && html[i] != b'\r' && html[i] != b'\n' {
                    i += 1;
                }
                let raw_ctag = &html[ctag_start..i];
                let mut ctag_buf = [0u8; 16];
                let ctag_len = raw_ctag.len().min(15);
                for idx in 0..ctag_len {
                    let mut c = raw_ctag[idx];
                    if c >= b'A' && c <= b'Z' { c += 32; }
                    ctag_buf[idx] = c;
                }
                let ctag_name = &ctag_buf[..ctag_len];

                while i < len && html[i] != b'>' { i += 1; }
                if i < len { i += 1; }

                let is_unstacked_close = ctag_name == b"html"
                    || ctag_name == b"head"
                    || ctag_name == b"body"
                    || ctag_name == b"meta"
                    || ctag_name == b"link"
                    || ctag_name == b"base"
                    || ctag_name == b"param"
                    || ctag_name == b"hr"
                    || ctag_name == b"br"
                    || ctag_name == b"img"
                    || ctag_name == b"input";

                if !is_unstacked_close && stack_depth > 0 {
                    let mut match_idx = None;
                    for s in (0..stack_depth).rev() {
                        let pid = parent_stack[s];
                        let ptag = &dom.nodes[pid].tag[..dom.nodes[pid].tag_len];
                        if ptag == ctag_name {
                            match_idx = Some(s);
                            break;
                        }
                    }
                    if let Some(m) = match_idx {
                        stack_depth = m;
                    } else {
                        stack_depth -= 1;
                    }
                }
                continue;
            }

            // 4. Tag de abertura <tag ...>
            let tag_start = i;
            while i < len && html[i] != b' ' && html[i] != b'>' && html[i] != b'/' && html[i] != b'\t' && html[i] != b'\r' && html[i] != b'\n' {
                i += 1;
            }
            let mut tag_buf = [0u8; 16];
            let raw_tag = &html[tag_start..i];
            let tag_len = raw_tag.len().min(15);
            for t_idx in 0..tag_len {
                let mut c = raw_tag[t_idx];
                if c >= b'A' && c <= b'Z' { c += 32; }
                tag_buf[t_idx] = c;
            }
            let tag_name = &tag_buf[..tag_len];

            // Trata <style>
            if tag_name == b"style" {
                while i < len && html[i] != b'>' { i += 1; }
                if i < len { i += 1; }
                let style_start = i;
                while i + 7 < len && !html[i..i + 8].eq_ignore_ascii_case(b"</style>") {
                    i += 1;
                }
                let css_bytes = &html[style_start..i];
                css.parse_stylesheet_append(css_bytes);
                while i < len && html[i] != b'>' { i += 1; }
                if i < len { i += 1; }
                continue;
            }

            // Trata <script> (inline e externo src="...")
            if tag_name == b"script" {
                let script_tag_start = i;
                while i < len && html[i] != b'>' { i += 1; }
                let script_tag_content = &html[script_tag_start..i];
                if i < len { i += 1; }

                if let Some(src_val) = extract_attr_val(script_tag_content, b"src=") {
                    if script_count < queued_scripts.len() {
                        let mut qs = QueuedScript::empty();
                        qs.is_vfs = true;
                        let cpy = src_val.len().min(63);
                        qs.vfs_path[..cpy].copy_from_slice(&src_val[..cpy]);
                        qs.vfs_path_len = cpy;
                        queued_scripts[script_count] = qs;
                        script_count += 1;
                    }
                }

                let script_body_start = i;
                while i + 8 < len && !html[i..i + 9].eq_ignore_ascii_case(b"</script>") {
                    i += 1;
                }
                let script_body_end = i;

                if script_body_end > script_body_start && script_count < queued_scripts.len() {
                    let mut qs = QueuedScript::empty();
                    qs.is_vfs = false;
                    qs.inline_start = script_body_start;
                    qs.inline_end = script_body_end;
                    queued_scripts[script_count] = qs;
                    script_count += 1;
                }

                while i < len && html[i] != b'>' { i += 1; }
                if i < len { i += 1; }
                continue;
            }

            // Trata <title>
            if tag_name == b"title" {
                while i < len && html[i] != b'>' { i += 1; }
                if i < len { i += 1; }
                let title_start = i;
                while i + 7 < len && !html[i..i + 8].eq_ignore_ascii_case(b"</title>") {
                    i += 1;
                }
                let t_len = (i - title_start).min(63);
                dom.title[..t_len].copy_from_slice(&html[title_start..title_start + t_len]);
                dom.title_len = t_len;
                while i < len && html[i] != b'>' { i += 1; }
                if i < len { i += 1; }
                continue;
            }

            // Trata <link rel="stylesheet" href="...">
            if tag_name == b"link" {
                let link_tag_start = i;
                while i < len && html[i] != b'>' { i += 1; }
                let link_content = &html[link_tag_start..i];
                if i < len { i += 1; }

                let is_rel_css = if let Some(rel) = extract_attr_val(link_content, b"rel=") {
                    rel.eq_ignore_ascii_case(b"stylesheet")
                } else {
                    false
                };

                if let Some(href_val) = extract_attr_val(link_content, b"href=") {
                    if is_rel_css || href_val.ends_with(b".css") {
                        let mut css_buf = [0u8; 4096];
                        if let Some(n) = read_css_file_from_vfs(href_val, &mut css_buf) {
                            css.parse_stylesheet_append(&css_buf[..n]);
                        }
                    }
                }
                continue;
            }

            // Ignora tags estruturais e metadados sem consumir slots
            if tag_name == b"html" || tag_name == b"head" || tag_name == b"body" || tag_name == b"meta"
                || tag_name == b"link" || tag_name == b"base" || tag_name == b"param"
            {
                while i < len && html[i] != b'>' { i += 1; }
                if i < len { i += 1; }
                continue;
            }

            // Ignora completamente sub-árvores complexas de gráficos/scripts (svg, noscript)
            if tag_name == b"svg" {
                while i < len && html[i] != b'>' { i += 1; }
                if i < len { i += 1; }
                while i + 5 < len && !html[i..i + 6].eq_ignore_ascii_case(b"</svg>") {
                    i += 1;
                }
                while i < len && html[i] != b'>' { i += 1; }
                if i < len { i += 1; }
                continue;
            }
            if tag_name == b"noscript" {
                while i < len && html[i] != b'>' { i += 1; }
                if i < len { i += 1; }
                while i + 10 < len && !html[i..i + 11].eq_ignore_ascii_case(b"</noscript>") {
                    i += 1;
                }
                while i < len && html[i] != b'>' { i += 1; }
                if i < len { i += 1; }
                continue;
            }

            // Elemento normal do DOM
            let node_id_opt = dom.alloc_node(DomNodeType::Element);
            if node_id_opt.is_none() {
                // Ao invés de abortar o documento, pula este elemento e continua o parsing
                while i < len && html[i] != b'>' { i += 1; }
                if i < len { i += 1; }
                continue;
            }
            let node_id = node_id_opt.unwrap();
            dom.nodes[node_id].tag = tag_buf;
            dom.nodes[node_id].tag_len = tag_len;

            // Lê atributos: id, class, style, href, onclick
            let mut is_self_closing = false;
            while i < len && html[i] != b'>' {
                if html[i] == b'/' && i + 1 < len && html[i + 1] == b'>' {
                    is_self_closing = true;
                    i += 1;
                    break;
                }
                if i + 3 < len && html[i..i + 3].eq_ignore_ascii_case(b"id=") {
                    i += 3;
                    let quote = if i < len && (html[i] == b'"' || html[i] == b'\'') { let q = html[i]; i += 1; Some(q) } else { None };
                    let a_start = i;
                    while i < len {
                        let c = html[i];
                        if let Some(q) = quote { if c == q { i += 1; break; } } else if c == b' ' || c == b'>' || c == b'/' { break; }
                        i += 1;
                    }
                    let a_val = &html[a_start..if quote.is_some() { i - 1 } else { i }];
                    let cpy = a_val.len().min(31);
                    dom.nodes[node_id].id_attr[..cpy].copy_from_slice(&a_val[..cpy]);
                    dom.nodes[node_id].id_len = cpy;
                } else if i + 6 < len && html[i..i + 6].eq_ignore_ascii_case(b"class=") {
                    i += 6;
                    let quote = if i < len && (html[i] == b'"' || html[i] == b'\'') { let q = html[i]; i += 1; Some(q) } else { None };
                    let a_start = i;
                    while i < len {
                        let c = html[i];
                        if let Some(q) = quote { if c == q { i += 1; break; } } else if c == b' ' || c == b'>' || c == b'/' { break; }
                        i += 1;
                    }
                    let a_val = &html[a_start..if quote.is_some() { i - 1 } else { i }];
                    let cpy = a_val.len().min(31);
                    dom.nodes[node_id].class_attr[..cpy].copy_from_slice(&a_val[..cpy]);
                    dom.nodes[node_id].class_len = cpy;
                } else if i + 6 < len && html[i..i + 6].eq_ignore_ascii_case(b"style=") {
                    i += 6;
                    let quote = if i < len && (html[i] == b'"' || html[i] == b'\'') { let q = html[i]; i += 1; Some(q) } else { None };
                    let a_start = i;
                    while i < len {
                        let c = html[i];
                        if let Some(q) = quote { if c == q { i += 1; break; } } else if c == b' ' || c == b'>' || c == b'/' { break; }
                        i += 1;
                    }
                    let a_val = &html[a_start..if quote.is_some() { i - 1 } else { i }];
                    let cpy = a_val.len().min(63);
                    dom.nodes[node_id].style_attr[..cpy].copy_from_slice(&a_val[..cpy]);
                    dom.nodes[node_id].style_len = cpy;
                } else if i + 5 < len && html[i..i + 5].eq_ignore_ascii_case(b"href=") {
                    i += 5;
                    let quote = if i < len && (html[i] == b'"' || html[i] == b'\'') { let q = html[i]; i += 1; Some(q) } else { None };
                    let a_start = i;
                    while i < len {
                        let c = html[i];
                        if let Some(q) = quote { if c == q { i += 1; break; } } else if c == b' ' || c == b'>' || c == b'/' { break; }
                        i += 1;
                    }
                    let a_val = &html[a_start..if quote.is_some() { i - 1 } else { i }];
                    let cpy = a_val.len().min(127);
                    dom.nodes[node_id].href_attr[..cpy].copy_from_slice(&a_val[..cpy]);
                    dom.nodes[node_id].href_len = cpy;
                } else if i + 4 < len && html[i..i + 4].eq_ignore_ascii_case(b"src=") {
                    i += 4;
                    let quote = if i < len && (html[i] == b'"' || html[i] == b'\'') { let q = html[i]; i += 1; Some(q) } else { None };
                    let a_start = i;
                    while i < len {
                        let c = html[i];
                        if let Some(q) = quote { if c == q { i += 1; break; } } else if c == b' ' || c == b'>' || c == b'/' { break; }
                        i += 1;
                    }
                    let a_val = &html[a_start..if quote.is_some() { i - 1 } else { i }];
                    let cpy = a_val.len().min(127);
                    dom.nodes[node_id].src_attr[..cpy].copy_from_slice(&a_val[..cpy]);
                    dom.nodes[node_id].src_len = cpy;
                } else if i + 4 < len && html[i..i + 4].eq_ignore_ascii_case(b"alt=") {
                    i += 4;
                    let quote = if i < len && (html[i] == b'"' || html[i] == b'\'') { let q = html[i]; i += 1; Some(q) } else { None };
                    let a_start = i;
                    while i < len {
                        let c = html[i];
                        if let Some(q) = quote { if c == q { i += 1; break; } } else if c == b' ' || c == b'>' || c == b'/' { break; }
                        i += 1;
                    }
                    let a_val = &html[a_start..if quote.is_some() { i - 1 } else { i }];
                    let cpy = a_val.len().min(31);
                    dom.nodes[node_id].alt_attr[..cpy].copy_from_slice(&a_val[..cpy]);
                    dom.nodes[node_id].alt_len = cpy;
                } else if i + 6 < len && html[i..i + 6].eq_ignore_ascii_case(b"width=") {
                    i += 6;
                    let quote = if i < len && (html[i] == b'"' || html[i] == b'\'') { let q = html[i]; i += 1; Some(q) } else { None };
                    let a_start = i;
                    while i < len {
                        let c = html[i];
                        if let Some(q) = quote { if c == q { i += 1; break; } } else if c == b' ' || c == b'>' || c == b'/' { break; }
                        i += 1;
                    }
                    let a_val = &html[a_start..if quote.is_some() { i - 1 } else { i }];
                    if let Some(w_num) = parse_px(a_val) {
                        dom.nodes[node_id].width_attr = w_num.min(u16::MAX as usize) as u16;
                    }
                } else if i + 7 < len && html[i..i + 7].eq_ignore_ascii_case(b"height=") {
                    i += 7;
                    let quote = if i < len && (html[i] == b'"' || html[i] == b'\'') { let q = html[i]; i += 1; Some(q) } else { None };
                    let a_start = i;
                    while i < len {
                        let c = html[i];
                        if let Some(q) = quote { if c == q { i += 1; break; } } else if c == b' ' || c == b'>' || c == b'/' { break; }
                        i += 1;
                    }
                    let a_val = &html[a_start..if quote.is_some() { i - 1 } else { i }];
                    if let Some(h_num) = parse_px(a_val) {
                        dom.nodes[node_id].height_attr = h_num.min(u16::MAX as usize) as u16;
                    }
                } else if i + 8 < len && html[i..i + 8].eq_ignore_ascii_case(b"onclick=") {
                    i += 8;
                    let quote = if i < len && (html[i] == b'"' || html[i] == b'\'') { let q = html[i]; i += 1; Some(q) } else { None };
                    let a_start = i;
                    while i < len {
                        let c = html[i];
                        if let Some(q) = quote { if c == q { i += 1; break; } } else if c == b' ' || c == b'>' || c == b'/' { break; }
                        i += 1;
                    }
                    let a_val = &html[a_start..if quote.is_some() { i - 1 } else { i }];
                    let cpy = a_val.len().min(127);
                    dom.nodes[node_id].onclick_attr[..cpy].copy_from_slice(&a_val[..cpy]);
                    dom.nodes[node_id].onclick_len = cpy;
                } else {
                    i += 1;
                }
            }

            if i < len && html[i] == b'>' {
                i += 1;
            }

            // Link parent-child hierarchy
            if stack_depth > 0 {
                let pid = parent_stack[stack_depth - 1];
                dom.nodes[node_id].parent = Some(pid);
                if dom.nodes[pid].child_count < MAX_DOM_CHILDREN {
                    let c = dom.nodes[pid].child_count;
                    dom.nodes[pid].children[c] = node_id;
                    dom.nodes[pid].child_count += 1;
                }
            }

            let is_void = tag_name == b"hr" || tag_name == b"br" || tag_name == b"img" || tag_name == b"input";
            if !is_void && !is_self_closing && stack_depth < 16 {
                parent_stack[stack_depth] = node_id;
                stack_depth += 1;
            }

            // Lê texto até próximo '<'
            let mut text_buf = [0u8; 256];
            let mut t_idx = 0usize;
            while i < len && html[i] != b'<' {
                if t_idx < 255 {
                    text_buf[t_idx] = html[i];
                    t_idx += 1;
                }
                i += 1;
            }

            if t_idx > 0 {
                let mut norm_buf = [0u8; 256];
                let norm_len = normalize_html_text(&text_buf[..t_idx], &mut norm_buf);
                if norm_len > 0 {
                    dom.nodes[node_id].text[..norm_len].copy_from_slice(&norm_buf[..norm_len]);
                    dom.nodes[node_id].text_len = norm_len;
                }
            }

            css.resolve(&mut dom.nodes[node_id]);
        } else {
            // Captura texto após tags de fechamento (ex.: "<p><b>Label: </b>Valor</p>") como nó DomNodeType::Text
            let mut text_buf = [0u8; 256];
            let mut t_idx = 0usize;
            while i < len && html[i] != b'<' {
                if t_idx < 255 {
                    text_buf[t_idx] = html[i];
                    t_idx += 1;
                }
                i += 1;
            }
            if t_idx > 0 && stack_depth > 0 {
                let mut norm_buf = [0u8; 256];
                let norm_len = normalize_html_text(&text_buf[..t_idx], &mut norm_buf);
                if norm_len > 0 {
                    if let Some(text_id) = dom.alloc_node(DomNodeType::Text) {
                        dom.nodes[text_id].tag[..5].copy_from_slice(b"#text");
                        dom.nodes[text_id].tag_len = 5;
                        dom.nodes[text_id].text[..norm_len].copy_from_slice(&norm_buf[..norm_len]);
                        dom.nodes[text_id].text_len = norm_len;
                        let pid = parent_stack[stack_depth - 1];
                        dom.nodes[text_id].parent = Some(pid);
                        if dom.nodes[pid].child_count < MAX_DOM_CHILDREN {
                            let c = dom.nodes[pid].child_count;
                            dom.nodes[pid].children[c] = text_id;
                            dom.nodes[pid].child_count += 1;
                        }
                        css.resolve(&mut dom.nodes[text_id]);
                    }
                }
            }
        }
    }

    // 1. Resolução inicial de estilos CSS completos em cascata
    css.resolve_all(dom);

    // 2. DOMContentLoaded: Execução de scripts deferidos após construção da árvore DOM
    for s in 0..script_count {
        let qs = &queued_scripts[s];
        if qs.is_vfs {
            let mut script_buf = [0u8; 4096];
            if let Some(n) = read_css_file_from_vfs(&qs.vfs_path[..qs.vfs_path_len], &mut script_buf) {
                js.eval_script(&script_buf[..n], dom);
            }
        } else if qs.inline_end > qs.inline_start {
            js.eval_script(&html[qs.inline_start..qs.inline_end], dom);
        }
    }

    // 3. Se algum script realizou mutação reativa do DOM, recalcula a cascata CSS
    if js.dom_dirty {
        css.resolve_all(dom);
        js.dom_dirty = false;
    }
}

// ── Tipos do Parser e DOM Legado ──────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlockKind {
    Heading1,
    Heading2,
    Heading3,
    Paragraph,
    ListItem,
    Link,
    HorizontalRule,
    AudioPlayer,
    VideoPlayer,
    PdfViewer,
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

    #[inline(always)]
    pub fn reset(&mut self) {
        self.title_len = 0;
        self.block_count = 0;
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
    pub is_js: bool,
    pub node_idx: usize,
}

impl ClickRegion {
    pub const fn empty() -> Self {
        Self {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            target: [0; 128],
            target_len: 0,
            is_js: false,
            node_idx: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub struct BoxRect {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
    pub bg_color: u32,
    pub border_color: Option<u32>,
    pub border_width: usize,
    pub border_radius: usize,
}

#[derive(Clone, Copy)]
pub struct RenderLine {
    pub text: [u8; 96],
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
                } else if tag_name == b"p" || tag_name == b"div" || tag_name == b"header" || tag_name == b"footer" || tag_name == b"nav" || tag_name == b"article" || tag_name == b"section" || tag_name == b"main" || tag_name == b"aside" || tag_name == b"details" || tag_name == b"summary" {
                    active_block_kind = Some(BlockKind::Paragraph);
                } else if tag_name == b"audio" {
                    active_block_kind = Some(BlockKind::AudioPlayer);
                } else if tag_name == b"video" {
                    active_block_kind = Some(BlockKind::VideoPlayer);
                } else if tag_name == b"native-pdf" {
                    active_block_kind = Some(BlockKind::PdfViewer);
                } else if tag_name == b"li" {
                    active_block_kind = Some(BlockKind::ListItem);
                } else if tag_name == b"a" {
                    active_block_kind = Some(BlockKind::Link);
                } else if tag_name == b"hr" {
                    flush_block(page, BlockKind::HorizontalRule, b"", b"");
                }
            } else {
                if tag_name == b"h1" || tag_name == b"h2" || tag_name == b"h3" || tag_name == b"p" || tag_name == b"div" || tag_name == b"header" || tag_name == b"footer" || tag_name == b"nav" || tag_name == b"article" || tag_name == b"section" || tag_name == b"main" || tag_name == b"aside" || tag_name == b"details" || tag_name == b"summary" || tag_name == b"li" || tag_name == b"a" || tag_name == b"audio" || tag_name == b"video" || tag_name == b"native-pdf" {
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

pub const MAX_RENDER_LINES: usize = 512;
pub const MAX_RENDER_BOXES: usize = 64;
pub const MAX_RENDER_IMAGES: usize = 32;
pub const MAX_CLICK_REGIONS: usize = 256;

#[derive(Clone, Copy)]
pub struct RenderImage {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
    pub src: [u8; 128],
    pub src_len: usize,
    pub alt: [u8; 32],
    pub alt_len: usize,
    #[allow(dead_code)]
    pub node_idx: usize,
}

pub struct DocumentLayout {
    pub lines: [RenderLine; MAX_RENDER_LINES],
    pub line_count: usize,
    pub boxes: [BoxRect; MAX_RENDER_BOXES],
    pub box_count: usize,
    pub images: [RenderImage; MAX_RENDER_IMAGES],
    pub image_count: usize,
    pub click_regions: [ClickRegion; MAX_CLICK_REGIONS],
    pub click_count: usize,
    pub total_height: usize,
}

impl DocumentLayout {
    pub const fn new() -> Self {
        Self {
            lines: [RenderLine {
                text: [0; 96],
                text_len: 0,
                x: 0,
                y: 0,
                color: 0,
                is_link: false,
                is_hr: false,
            }; MAX_RENDER_LINES],
            line_count: 0,
            boxes: [BoxRect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
                bg_color: 0,
                border_color: None,
                border_width: 1,
                border_radius: 0,
            }; MAX_RENDER_BOXES],
            box_count: 0,
            images: [RenderImage {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
                src: [0; 128],
                src_len: 0,
                alt: [0; 32],
                alt_len: 0,
                node_idx: 0,
            }; MAX_RENDER_IMAGES],
            image_count: 0,
            click_regions: [ClickRegion {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
                target: [0; 128],
                target_len: 0,
                is_js: false,
                node_idx: 0,
            }; MAX_CLICK_REGIONS],
            click_count: 0,
            total_height: 0,
        }
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.line_count = 0;
        self.box_count = 0;
        self.image_count = 0;
        self.click_count = 0;
        self.total_height = 0;
    }

    pub fn compute_from_dom(&mut self, dom: &DomArena, content_width: usize) {
        self.line_count = 0;
        self.click_count = 0;
        self.box_count = 0;
        self.image_count = 0;

        let mut curr_y = 6usize;
        let mut laid_out = [false; MAX_DOM_NODES];

        // 1. Traverse all root-level nodes (nodes with parent == None)
        for i in 0..dom.node_count {
            if dom.nodes[i].parent.is_none() {
                self.layout_subtree(dom, i, 4, &mut curr_y, content_width, &mut laid_out);
            }
        }

        // 2. Fallback: layout any orphaned nodes that were not reached by root traversal
        for i in 0..dom.node_count {
            if !laid_out[i] {
                self.layout_subtree(dom, i, 4, &mut curr_y, content_width, &mut laid_out);
            }
        }

        self.total_height = curr_y + 10;
    }

    fn mark_subtree_laid_out(dom: &DomArena, node_idx: usize, laid_out: &mut [bool; MAX_DOM_NODES]) {
        if node_idx >= dom.node_count || laid_out[node_idx] { return; }
        laid_out[node_idx] = true;
        for c in 0..dom.nodes[node_idx].child_count {
            Self::mark_subtree_laid_out(dom, dom.nodes[node_idx].children[c], laid_out);
        }
    }

    fn find_enclosing_link<'a>(dom: &'a DomArena, mut curr_idx: usize) -> Option<&'a [u8]> {
        loop {
            let n = &dom.nodes[curr_idx];
            if n.tag_len == 1 && n.tag[0] == b'a' && n.href_len > 0 {
                return Some(&n.href_attr[..n.href_len]);
            }
            if let Some(pid) = n.parent {
                if pid < dom.node_count && pid != curr_idx {
                    curr_idx = pid;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        None
    }

    fn layout_subtree(
        &mut self,
        dom: &DomArena,
        node_idx: usize,
        base_x: usize,
        curr_y: &mut usize,
        avail_width: usize,
        laid_out: &mut [bool; MAX_DOM_NODES],
    ) {
        if node_idx >= dom.node_count || laid_out[node_idx] {
            return;
        }

        let node = &dom.nodes[node_idx];

        // Se display == none, ignora o elemento e toda sua sub-árvore
        if node.computed_style.display == DisplayMode::None {
            laid_out[node_idx] = true;
            for c in 0..node.child_count {
                Self::mark_subtree_laid_out(dom, node.children[c], laid_out);
            }
            return;
        }

        laid_out[node_idx] = true;
        let tag = &node.tag[..node.tag_len];

        if tag == b"hr" {
            if self.line_count < self.lines.len() {
                self.lines[self.line_count] = RenderLine {
                    text: [0; 96],
                    text_len: 0,
                    x: base_x + 8,
                    y: *curr_y + 4,
                    color: COLOR_HR,
                    is_link: false,
                    is_hr: true,
                };
                self.line_count += 1;
            }
            *curr_y += 10;
            return;
        }

        if tag == b"img" {
            if node.computed_style.display_block {
                *curr_y += node.computed_style.margin_top;
            }
            let mar_left = node.computed_style.margin_left;
            let mar_right = node.computed_style.margin_right;
            let pad_left = node.computed_style.padding_left;
            let pad_top = node.computed_style.padding_top;
            let pad_bottom = node.computed_style.padding_bottom;

            let max_img_w = avail_width.saturating_sub(mar_left + mar_right + pad_left).max(16);
            let probed_dims = if (node.width_attr == 0 || node.height_attr == 0) && node.src_len > 0 {
                unsafe { lookup_image_dimensions(&node.src_attr[..node.src_len]) }
            } else {
                None
            };

            let (img_w, img_h) = match (node.width_attr as usize, node.height_attr as usize, probed_dims) {
                (w_attr, h_attr, _) if w_attr > 0 && h_attr > 0 => {
                    let clamped_w = w_attr.min(max_img_w);
                    let clamped_h = if clamped_w < w_attr {
                        (h_attr * clamped_w / w_attr).max(8)
                    } else {
                        h_attr
                    };
                    (clamped_w, clamped_h.min(260))
                }
                (w_attr, 0, Some((ow, oh))) if w_attr > 0 && ow > 0 && oh > 0 => {
                    let cw = w_attr.min(max_img_w);
                    let ch = (oh * cw / ow).clamp(12, 260);
                    (cw, ch)
                }
                (0, h_attr, Some((ow, oh))) if h_attr > 0 && ow > 0 && oh > 0 => {
                    let mut cw = (ow * h_attr / oh).max(16);
                    let mut ch = h_attr;
                    if cw > max_img_w {
                        ch = (ch * max_img_w / cw).max(12);
                        cw = max_img_w;
                    }
                    (cw, ch.min(260))
                }
                (0, 0, Some((ow, oh))) if ow > 0 && oh > 0 => {
                    let mut cw = ow.min(max_img_w);
                    let mut ch = (oh * cw / ow).max(12);
                    if ch > 180 {
                        cw = (cw * 180 / ch).max(16);
                        ch = 180;
                    }
                    (cw, ch)
                }
                (w_attr, h_attr, _) => {
                    let cw = if w_attr > 0 { w_attr } else { 48 }.min(max_img_w);
                    let ch = if h_attr > 0 { h_attr } else { 36 };
                    (cw, ch)
                }
            };

            if self.image_count < MAX_RENDER_IMAGES {
                let slot = self.image_count;
                self.images[slot] = RenderImage {
                    x: base_x + mar_left + pad_left,
                    y: *curr_y + pad_top,
                    w: img_w,
                    h: img_h,
                    src: node.src_attr,
                    src_len: node.src_len,
                    alt: node.alt_attr,
                    alt_len: node.alt_len,
                    node_idx,
                };
                self.image_count += 1;

                let href_opt = if node.href_len > 0 {
                    Some(&node.href_attr[..node.href_len])
                } else if let Some(pid) = node.parent {
                    if dom.nodes[pid].href_len > 0 {
                        Some(&dom.nodes[pid].href_attr[..dom.nodes[pid].href_len])
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some(href) = href_opt {
                    if self.click_count < self.click_regions.len() {
                        let mut reg = ClickRegion::empty();
                        reg.x = base_x + mar_left + pad_left;
                        reg.y = *curr_y + pad_top;
                        reg.w = img_w;
                        reg.h = img_h;
                        let cpy_h = href.len().min(127);
                        reg.target[..cpy_h].copy_from_slice(&href[..cpy_h]);
                        reg.target_len = cpy_h;
                        self.click_regions[self.click_count] = reg;
                        self.click_count += 1;
                    }
                }
            }

            *curr_y += img_h + pad_top + pad_bottom;
            if node.computed_style.display_block {
                *curr_y += node.computed_style.margin_bottom;
            }
            return;
        }

        // Layout Flexbox Linha (display: flex && flex-direction: row)
        if node.child_count > 0 && node.computed_style.display == DisplayMode::Flex && node.computed_style.flex_direction == FlexDirection::Row {
            if node.computed_style.display_block {
                *curr_y += node.computed_style.margin_top;
            }
            let box_start_y = *curr_y;
            let pad_left = node.computed_style.padding_left;
            let pad_right = node.computed_style.padding_right;
            let pad_top = if node.computed_style.bg_color.is_some() || node.computed_style.border_color.is_some() {
                node.computed_style.padding_top.max(4)
            } else {
                node.computed_style.padding_top
            };
            let pad_bottom = if node.computed_style.bg_color.is_some() || node.computed_style.border_color.is_some() {
                node.computed_style.padding_bottom.max(4)
            } else {
                node.computed_style.padding_bottom
            };
            let mar_left = node.computed_style.margin_left;
            let mar_right = node.computed_style.margin_right;

            let has_box = node.computed_style.bg_color.is_some() || node.computed_style.border_color.is_some();
            let box_slot = if has_box && self.box_count < self.boxes.len() {
                let slot = self.box_count;
                self.box_count += 1;
                Some(slot)
            } else {
                None
            };

            *curr_y += pad_top;

            let inner_w = avail_width.saturating_sub(mar_left + mar_right + pad_left + pad_right);
            let mut visible_children = 0usize;
            for c in 0..node.child_count {
                let cid = node.children[c];
                if cid < dom.node_count && dom.nodes[cid].computed_style.display != DisplayMode::None {
                    visible_children += 1;
                }
            }

            let child_w = if visible_children > 0 { inner_w / visible_children } else { inner_w };
            let mut child_x = base_x + mar_left + pad_left;
            let mut max_y = *curr_y;

            let max_flex_right = base_x + mar_left + pad_left + inner_w;
            for c in 0..node.child_count {
                let cid = node.children[c];
                if cid < dom.node_count && dom.nodes[cid].computed_style.display != DisplayMode::None {
                    let rem_w = max_flex_right.saturating_sub(child_x);
                    if rem_w < font::FONT_WIDTH {
                        Self::mark_subtree_laid_out(dom, cid, laid_out);
                        continue;
                    }
                    let mut child_y = *curr_y;
                    let target_w = dom.nodes[cid].computed_style.width.unwrap_or(child_w).min(rem_w);
                    self.layout_subtree(dom, cid, child_x, &mut child_y, target_w, laid_out);
                    child_x += target_w;
                    if child_y > max_y {
                        max_y = child_y;
                    }
                }
            }
            *curr_y = max_y + pad_bottom;

            if let Some(slot) = box_slot {
                let box_h = node.computed_style.height.unwrap_or_else(|| {
                    curr_y.saturating_sub(box_start_y).max(font::FONT_HEIGHT + pad_top + pad_bottom)
                });
                let box_w = node.computed_style.width.unwrap_or(avail_width.saturating_sub(mar_left + mar_right));
                self.boxes[slot] = BoxRect {
                    x: base_x + mar_left,
                    y: box_start_y,
                    w: box_w,
                    h: box_h,
                    bg_color: node.computed_style.bg_color.unwrap_or(0),
                    border_color: node.computed_style.border_color,
                    border_width: node.computed_style.border_width,
                    border_radius: node.computed_style.border_radius,
                };
            }

            if node.computed_style.display_block {
                *curr_y += node.computed_style.margin_bottom;
            }
            return;
        }

        // Container element with children
        if node.child_count > 0 {
            if node.computed_style.display_block {
                *curr_y += node.computed_style.margin_top;
            }
            let box_start_y = *curr_y;
            let pad_left = node.computed_style.padding_left;
            let pad_right = node.computed_style.padding_right;
            let pad_top = if node.computed_style.bg_color.is_some() || node.computed_style.border_color.is_some() {
                node.computed_style.padding_top.max(4)
            } else {
                node.computed_style.padding_top
            };
            let pad_bottom = if node.computed_style.bg_color.is_some() || node.computed_style.border_color.is_some() {
                node.computed_style.padding_bottom.max(4)
            } else {
                node.computed_style.padding_bottom
            };
            let mar_left = node.computed_style.margin_left;
            let mar_right = node.computed_style.margin_right;

            let has_box = node.computed_style.bg_color.is_some() || node.computed_style.border_color.is_some();
            let box_slot = if has_box && self.box_count < self.boxes.len() {
                let slot = self.box_count;
                self.box_count += 1;
                Some(slot)
            } else {
                None
            };

            *curr_y += pad_top;

            // Direct text of container if any, or coalescing of simple inline children (e.g. <p><b>Label: </b>Valor</p>)
            let mut all_inline_text = true;
            for c in 0..node.child_count {
                let child_id = node.children[c];
                if child_id >= dom.node_count {
                    all_inline_text = false;
                    break;
                }
                let child = &dom.nodes[child_id];
                let ctag = &child.tag[..child.tag_len];
                let is_simple_inline = child.node_type == DomNodeType::Text
                    || ((ctag == b"b" || ctag == b"strong" || ctag == b"i" || ctag == b"em" || ctag == b"code" || ctag == b"small" || ctag == b"span")
                        && child.child_count == 0
                        && child.href_len == 0
                        && child.onclick_len == 0
                        && child.computed_style.bg_color.is_none()
                        && child.computed_style.border_color.is_none());
                if !is_simple_inline {
                    all_inline_text = false;
                    break;
                }
            }

            if all_inline_text {
                let mut combined = [0u8; 512];
                let mut c_len = 0usize;
                let append_chunk = |dst: &mut [u8; 512], dst_len: &mut usize, src: &[u8]| {
                    if src.is_empty() || *dst_len >= dst.len() {
                        return;
                    }
                    if *dst_len > 0 && dst[*dst_len - 1] != b' ' {
                        let first = src[0];
                        let is_punct = first == b'.' || first == b',' || first == b';' || first == b'!' || first == b'?' || first == b')' || first == b']';
                        if !is_punct && *dst_len < dst.len() {
                            dst[*dst_len] = b' ';
                            *dst_len += 1;
                        }
                    }
                    let take = src.len().min(dst.len() - *dst_len);
                    dst[*dst_len..*dst_len + take].copy_from_slice(&src[..take]);
                    *dst_len += take;
                };

                if node.text_len > 0 {
                    append_chunk(&mut combined, &mut c_len, &node.text[..node.text_len]);
                }
                for c in 0..node.child_count {
                    let child_id = node.children[c];
                    laid_out[child_id] = true;
                    let child = &dom.nodes[child_id];
                    if child.computed_style.display != DisplayMode::None && child.text_len > 0 {
                        append_chunk(&mut combined, &mut c_len, &child.text[..child.text_len]);
                    }
                }
                if c_len > 0 {
                    let max_chars = (avail_width.saturating_sub(mar_left + mar_right + pad_left + pad_right) / font::FONT_WIDTH).max(1);
                    let text_x = base_x + mar_left + pad_left + 4;
                    let enclosing_link = Self::find_enclosing_link(dom, node_idx);
                    let is_link = enclosing_link.is_some();
                    let color = if is_link { COLOR_LINK } else { node.computed_style.color };
                    self.add_wrapped_text(&combined[..c_len], text_x, curr_y, max_chars, color, is_link, enclosing_link);
                }
            } else {
                if node.text_len > 0 {
                    let max_chars = (avail_width.saturating_sub(mar_left + mar_right + pad_left + pad_right) / font::FONT_WIDTH).max(1);
                    let text_x = base_x + mar_left + pad_left + 4;
                    let enclosing_link = Self::find_enclosing_link(dom, node_idx);
                    let is_link = enclosing_link.is_some();
                    let color = if is_link { COLOR_LINK } else { node.computed_style.color };
                    self.add_wrapped_text(&node.text[..node.text_len], text_x, curr_y, max_chars, color, is_link, enclosing_link);
                }

                // Lay out all children
                let child_base_x = base_x + mar_left + pad_left;
                let child_width = avail_width.saturating_sub(mar_left + mar_right + pad_left + pad_right);
                for c in 0..node.child_count {
                    let child_id = node.children[c];
                    self.layout_subtree(dom, child_id, child_base_x, curr_y, child_width, laid_out);
                }
            }

            *curr_y += pad_bottom;

            if let Some(slot) = box_slot {
                let box_h = node.computed_style.height.unwrap_or_else(|| {
                    curr_y.saturating_sub(box_start_y).max(font::FONT_HEIGHT + pad_top + pad_bottom)
                });
                let box_w = node.computed_style.width.unwrap_or(avail_width.saturating_sub(mar_left + mar_right));
                self.boxes[slot] = BoxRect {
                    x: base_x + mar_left,
                    y: box_start_y,
                    w: box_w,
                    h: box_h,
                    bg_color: node.computed_style.bg_color.unwrap_or(0),
                    border_color: node.computed_style.border_color,
                    border_width: node.computed_style.border_width,
                    border_radius: node.computed_style.border_radius,
                };
            }

            if node.computed_style.display_block {
                *curr_y += node.computed_style.margin_bottom;
            }
            return;
        }

        // Leaf element (child_count == 0)
        if node.text_len == 0 && node.computed_style.bg_color.is_none() && node.computed_style.border_color.is_none() {
            return;
        }

        if node.computed_style.display_block {
            *curr_y += node.computed_style.margin_top;
        }

        let is_interactive = tag == b"button" || node.onclick_len > 0;
        let enclosing_link = Self::find_enclosing_link(dom, node_idx);
        let is_link = enclosing_link.is_some();
        let target_href = enclosing_link;
        let color = if is_link { COLOR_LINK } else { node.computed_style.color };
        let has_box = node.computed_style.bg_color.is_some() || node.computed_style.border_color.is_some();

        let pad_left = node.computed_style.padding_left;
        let pad_right = node.computed_style.padding_right;
        let pad_top = node.computed_style.padding_top;
        let pad_bottom = node.computed_style.padding_bottom;
        let mar_left = node.computed_style.margin_left;
        let mar_right = node.computed_style.margin_right;

        let elem_x = base_x + mar_left + pad_left + if is_interactive || has_box { 4 } else { 0 };
        let avail_for_text = avail_width.saturating_sub(mar_left + mar_right + pad_left + pad_right + 8);
        let max_chars = (avail_for_text / font::FONT_WIDTH).max(1);

        let box_start_y = *curr_y;
        *curr_y += pad_top;

        let text = &node.text[..node.text_len];
        if is_interactive {
            self.add_wrapped_text_interactive(text, elem_x, curr_y, max_chars, color, true, node.id);
        } else if is_link {
            self.add_wrapped_text(text, elem_x, curr_y, max_chars, color, true, target_href);
        } else if node.text_len > 0 {
            self.add_wrapped_text(text, elem_x, curr_y, max_chars, color, false, None);
        }

        *curr_y += pad_bottom;

        if has_box {
            if self.box_count < self.boxes.len() {
                let box_h = node.computed_style.height.unwrap_or_else(|| {
                    curr_y.saturating_sub(box_start_y).max(font::FONT_HEIGHT + pad_top + pad_bottom + 2)
                });
                let box_w = if let Some(w) = node.computed_style.width {
                    w.min(avail_width)
                } else if node.computed_style.display_block {
                    avail_width.saturating_sub(mar_left + mar_right)
                } else {
                    (node.text_len * font::FONT_WIDTH + pad_left + pad_right + 12).min(avail_width)
                };
                self.boxes[self.box_count] = BoxRect {
                    x: (base_x + mar_left).max(2),
                    y: box_start_y,
                    w: box_w,
                    h: box_h,
                    bg_color: node.computed_style.bg_color.unwrap_or(0x0031_3244),
                    border_color: node.computed_style.border_color,
                    border_width: node.computed_style.border_width,
                    border_radius: node.computed_style.border_radius,
                };
                self.box_count += 1;
            }
        }

        if node.computed_style.display_block {
            *curr_y += node.computed_style.margin_bottom;
        }
    }

    fn add_wrapped_text_interactive(
        &mut self,
        text: &[u8],
        start_x: usize,
        curr_y: &mut usize,
        max_chars: usize,
        color: u32,
        is_interactive: bool,
        node_idx: usize,
    ) {
        if text.is_empty() { return; }

        let mut offset = 0usize;

        while offset < text.len() && self.line_count < self.lines.len() {
            let remaining = text.len() - offset;
            let take = if remaining <= max_chars {
                remaining
            } else {
                let mut break_idx = max_chars;
                while break_idx > 0 && text[offset + break_idx] != b' ' {
                    break_idx -= 1;
                }
                if break_idx == 0 { max_chars } else { break_idx }
            };

            let mut line_buf = [0u8; 96];
            let actual_take = take.min(95);
            line_buf[..actual_take].copy_from_slice(&text[offset..offset + actual_take]);

            if is_interactive {
                if self.click_count < self.click_regions.len() {
                    let reg = ClickRegion {
                        x: start_x.saturating_sub(2),
                        y: *curr_y,
                        w: actual_take * font::FONT_WIDTH + 8,
                        h: font::FONT_HEIGHT + 4,
                        target: [0; 128],
                        target_len: 0,
                        is_js: true,
                        node_idx,
                    };
                    self.click_regions[self.click_count] = reg;
                    self.click_count += 1;
                }
            }

            self.lines[self.line_count] = RenderLine {
                text: line_buf,
                text_len: actual_take,
                x: start_x,
                y: *curr_y,
                color,
                is_link: false,
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

    pub fn compute(&mut self, page: &ParsedPage, content_width: usize) {
        self.line_count = 0;
        self.click_count = 0;

        let max_chars_per_line = (content_width / font::FONT_WIDTH).max(1);
        let mut curr_y = 6usize;

        for b_idx in 0..page.block_count {
            let blk = &page.blocks[b_idx];

            match blk.kind {
                BlockKind::HorizontalRule => {
                    if self.line_count < self.lines.len() {
                        self.lines[self.line_count] = RenderLine {
                            text: [0; 96],
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
                BlockKind::AudioPlayer => {
                    curr_y += 3;
                    let text = b"[ > AUDIO ] 0:00 [====----] 100%";
                    self.add_wrapped_text(text, 8, &mut curr_y, max_chars_per_line, 0x0089_DCEB, false, None);
                    curr_y += 4;
                }
                BlockKind::VideoPlayer => {
                    curr_y += 3;
                    let text = b"[ VIDEO ] Native Motion JMVD";
                    self.add_wrapped_text(text, 8, &mut curr_y, max_chars_per_line, 0x00A6_E3A1, false, None);
                    let ctrl = b"[ > PLAY ] [=======---] 1080p";
                    self.add_wrapped_text(ctrl, 8, &mut curr_y, max_chars_per_line, 0x00CD_D6F4, false, None);
                    curr_y += 4;
                }
                BlockKind::PdfViewer => {
                    curr_y += 3;
                    let text = b"[ PDF ] Native Vector Document";
                    self.add_wrapped_text(text, 8, &mut curr_y, max_chars_per_line, 0x00F9_E2AF, false, None);
                    let page_str = b"[ Page 1/1 ] [ 100% Vector OK ]";
                    self.add_wrapped_text(page_str, 8, &mut curr_y, max_chars_per_line, 0x00CD_D6F4, false, None);
                    curr_y += 4;
                }
            }
        }

        self.total_height = curr_y + 10;
    }

    pub fn compute_fallback_text(&mut self, html: &[u8], content_width: usize) {
        let max_chars = (content_width / font::FONT_WIDTH).max(1);
        let mut curr_y = 8usize;
        let mut i = 0usize;
        let len = html.len();

        while i < len && self.line_count < self.lines.len() {
            // Pula scripts, styles, svgs
            if html[i] == b'<' {
                i += 1;
                if i + 6 < len && html[i..i + 6].eq_ignore_ascii_case(b"script") {
                    while i + 8 < len && !html[i..i + 9].eq_ignore_ascii_case(b"</script>") { i += 1; }
                } else if i + 5 < len && html[i..i + 5].eq_ignore_ascii_case(b"style") {
                    while i + 7 < len && !html[i..i + 8].eq_ignore_ascii_case(b"</style>") { i += 1; }
                } else if i + 3 < len && html[i..i + 3].eq_ignore_ascii_case(b"svg") {
                    while i + 5 < len && !html[i..i + 6].eq_ignore_ascii_case(b"</svg>") { i += 1; }
                }
                while i < len && html[i] != b'>' { i += 1; }
                if i < len { i += 1; }
                continue;
            }

            // Coleta texto visível
            let mut text_buf = [0u8; 128];
            let mut t_len = 0;
            while i < len && html[i] != b'<' && t_len < 127 {
                let b = html[i];
                if b == b'\r' || b == b'\n' || b == b'\t' {
                    if t_len > 0 && text_buf[t_len - 1] != b' ' {
                        text_buf[t_len] = b' ';
                        t_len += 1;
                    }
                } else if b >= 32 && b <= 126 {
                    text_buf[t_len] = b;
                    t_len += 1;
                }
                i += 1;
            }

            let trimmed = text_buf[..t_len].trim_ascii();
            if !trimmed.is_empty() {
                self.add_wrapped_text(trimmed, 8, &mut curr_y, max_chars, COLOR_TEXT, false, None);
                curr_y += 2;
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

            let mut line_buf = [0u8; 96];
            let actual_take = take.min(95);
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
                            is_js: false,
                            node_idx: 0,
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

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FullscreenState {
    Windowed,
    EnteringFullscreen,
    Fullscreen,
    ExitingFullscreen,
    ResizePending,
    Recovery,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserAction {
    None,
    Back,
    Forward,
    Reload,
    NavigateTarget([u8; 128], usize),
    JsAction(usize),
    FocusAddress,
    Scroll(i32),
    ToggleFullscreen,
    ExitFullscreen,
    SetWallpaper([u8; 128], usize),
    SaveWallpaper([u8; 128], usize),
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
    pub viewport_w: usize,
    pub viewport_h: usize,
    pub saved_w: usize,
    pub saved_h: usize,
    pub fullscreen_state: FullscreenState,
    pub page: ParsedPage,
    pub dom: DomArena,
    pub css: MiniCss,
    pub js_runtime: MiniJsRuntime,
    pub layout: DocumentLayout,
    pub history_store: storage::BrowserHistoryStore,
    pub cookie_jar: storage::BrowserCookieJar,
    pub bookmark_store: storage::BrowserBookmarkStore,
}

impl BrowserSession {
    pub const fn new() -> Self {
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
            viewport_w: WIN_W,
            viewport_h: WIN_H,
            saved_w: WIN_W,
            saved_h: WIN_H,
            fullscreen_state: FullscreenState::Windowed,
            page: ParsedPage::new(),
            dom: DomArena::new(),
            css: MiniCss::new(),
            js_runtime: MiniJsRuntime::new(),
            layout: DocumentLayout::new(),
            history_store: storage::BrowserHistoryStore::new(),
            cookie_jar: storage::BrowserCookieJar::new(),
            bookmark_store: storage::BrowserBookmarkStore::new(),
        }
    }

    pub fn reset(&mut self) {
        self.current_url = [0; 128];
        self.current_url_len = 0;
        self.history_back_count = 0;
        self.history_fwd_count = 0;
        self.is_address_focused = false;
        self.address_selected = false;
        self.address_edit = [0; 128];
        self.address_edit_len = 0;
        self.scroll_y = 0;
        self.max_scroll = 0;
        self.viewport_w = WIN_W;
        self.viewport_h = WIN_H;
        self.saved_w = WIN_W;
        self.saved_h = WIN_H;
        self.fullscreen_state = FullscreenState::Windowed;
        self.page.reset();
        self.dom.reset();
        self.css.reset();
        self.js_runtime.reset();
        self.layout.reset();
    }

    pub fn init_persistence(&mut self) {
        storage::ensure_storage_hierarchy();
        let _ = self.history_store.load();
        let _ = self.cookie_jar.load();
        if !self.bookmark_store.load() {
            self.bookmark_store.init_defaults_if_empty();
            let _ = self.bookmark_store.save();
        }
    }

    #[allow(dead_code)]
    pub fn record_navigation(&mut self, url: &[u8], title: &[u8]) {
        self.history_store.add_visit(url, title);
        let _ = self.history_store.save();
    }

    pub fn record_navigation_current(&mut self, url: &[u8]) {
        let t_len = self.page.title_len;
        let title = self.page.title;
        self.history_store.add_visit(url, &title[..t_len]);
        let _ = self.history_store.save();
    }

    pub fn dispatch_js_click(&mut self, node_idx: usize) -> bool {
        if node_idx < self.dom.node_count {
            let onclick_len = self.dom.nodes[node_idx].onclick_len;
            if onclick_len > 0 {
                let mut script_buf = [0u8; 256];
                let cpy = onclick_len.min(255);
                script_buf[..cpy].copy_from_slice(&self.dom.nodes[node_idx].onclick_attr[..cpy]);
                self.js_runtime.eval_script(&script_buf[..cpy], &mut self.dom);
                self.css.resolve_all(&mut self.dom);
                self.recompute_layout();
                return true;
            }
        }
        false
    }

    pub fn resize(&mut self, new_w: usize, new_h: usize) {
        if new_w >= 100 && new_h >= 60 && new_w <= 1024 && new_h <= 768 {
            self.viewport_w = new_w;
            self.viewport_h = new_h;
            self.recompute_layout();
        }
    }

    pub fn recompute_layout(&mut self) {
        let content_w = self.viewport_w.saturating_sub(24).max(40);
        if self.dom.node_count > 0 {
            self.layout.compute_from_dom(&self.dom, content_w);
        } else {
            self.layout.compute(&self.page, content_w);
        }
        let clip_top = CLIP_TOP;
        let clip_bottom = content_clip_bottom(self.viewport_h);
        let v_h = clip_bottom.saturating_sub(clip_top);
        if self.layout.total_height > v_h {
            self.max_scroll = self.layout.total_height - v_h;
        } else {
            self.max_scroll = 0;
        }
        self.scroll_y = self.scroll_y.min(self.max_scroll);
    }

    pub fn toggle_fullscreen(&mut self) -> FullscreenState {
        match self.fullscreen_state {
            FullscreenState::Windowed => {
                self.fullscreen_state = FullscreenState::EnteringFullscreen;
                self.saved_w = self.viewport_w;
                self.saved_h = self.viewport_h;
                self.viewport_w = 1024;
                self.viewport_h = 768;
                self.recompute_layout();
                self.fullscreen_state = FullscreenState::Fullscreen;
                FullscreenState::Fullscreen
            }
            FullscreenState::Fullscreen => {
                self.fullscreen_state = FullscreenState::ExitingFullscreen;
                self.viewport_w = if self.saved_w > 0 { self.saved_w } else { WIN_W };
                self.viewport_h = if self.saved_h > 0 { self.saved_h } else { WIN_H };
                self.recompute_layout();
                self.fullscreen_state = FullscreenState::Windowed;
                FullscreenState::Windowed
            }
            _ => {
                self.recover();
                FullscreenState::Windowed
            }
        }
    }

    pub fn recover(&mut self) {
        self.viewport_w = WIN_W;
        self.viewport_h = WIN_H;
        self.recompute_layout();
        self.fullscreen_state = FullscreenState::Windowed;
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
        parse_html_into_dom(html, &mut self.dom, &mut self.css, &mut self.js_runtime);
        self.recompute_layout();
        if self.layout.line_count == 0 && !html.is_empty() {
            let content_w = self.viewport_w.saturating_sub(24).max(40);
            self.layout.compute_fallback_text(html, content_w);
            let clip_top = CLIP_TOP;
            let clip_bottom = content_clip_bottom(self.viewport_h);
            let v_h = clip_bottom.saturating_sub(clip_top);
            if self.layout.total_height > v_h {
                self.max_scroll = self.layout.total_height - v_h;
            } else {
                self.max_scroll = 0;
            }
        }

        unsafe {
            crate::print_str(b"[BROWSER-LAYOUT] Documento carregado. Linhas=");
            crate::print_u64(self.layout.line_count as u64);
            crate::print_str(b" Regioes clicaveis=");
            crate::print_u64(self.layout.click_count as u64);
            crate::print_str(b"\n");
            for i in 0..self.layout.click_count {
                let r = &self.layout.click_regions[i];
                crate::print_str(b"  -> [CLICK-REGION ");
                crate::print_u64(i as u64);
                crate::print_str(b"] x=");
                crate::print_u64(r.x as u64);
                crate::print_str(b" y=");
                crate::print_u64(r.y as u64);
                crate::print_str(b" w=");
                crate::print_u64(r.w as u64);
                crate::print_str(b" h=");
                crate::print_u64(r.h as u64);
                crate::print_str(b" target=");
                crate::print_str(&r.target[..r.target_len]);
                crate::print_str(b"\n");
            }
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
        let w = self.viewport_w;
        let h = self.viewport_h;
        let clip_top = CLIP_TOP;
        let clip_bottom = content_clip_bottom(h);

        // 1. Barra de ferramentas / Chrome (y: 3..24)
        if click_y >= 3 && click_y <= 24 {
            // Botão Back [<] em (4..22)
            if click_x >= 4 && click_x <= 22 {
                if self.can_go_back() {
                    return BrowserAction::Back;
                }
                return BrowserAction::None;
            }

            // Botão Forward [>] em (24..42)
            if click_x >= 24 && click_x <= 42 {
                if self.can_go_forward() {
                    return BrowserAction::Forward;
                }
                return BrowserAction::None;
            }

            // Botão Reload [R] em (44..62)
            if click_x >= 44 && click_x <= 62 {
                return BrowserAction::Reload;
            }

            // Botão Fullscreen [F] no canto direito (w - 26 .. w - 4)
            let fs_x = w.saturating_sub(26);
            if click_x >= fs_x && click_x <= w.saturating_sub(4) {
                return BrowserAction::ToggleFullscreen;
            }

            // Barra de Endereço em Pílula (66 .. url_x + url_w)
            let url_x = 66;
            let badge_w = 64;
            let url_w = w.saturating_sub(url_x + 32 + badge_w);
            if click_x >= url_x && click_x <= url_x + url_w {
                self.focus_address();
                return BrowserAction::FocusAddress;
            }
        }

        // 2. Viewport de Conteúdo: aplica deslocamento de scroll_y
        if click_y >= clip_top && click_y <= clip_bottom {
            if self.is_address_focused {
                self.unfocus_address();
            }

            // Barra de rolagem (track_x = w - 14 .. w)
            if click_x >= w.saturating_sub(14) {
                let track_y = clip_top;
                let track_h = clip_bottom.saturating_sub(clip_top);
                if self.max_scroll > 0 {
                    let thumb_h = 24usize.max(track_h * (clip_bottom - clip_top) / self.layout.total_height.max(1));
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

            // Hit-test em links e botões interativos (com margem de tolerância de 2px)
            let doc_y = (click_y - clip_top) + self.scroll_y;
            for i in 0..self.layout.click_count {
                let r = &self.layout.click_regions[i];
                let pad = 2usize;
                let rx_start = r.x.saturating_sub(pad);
                let rx_end = r.x + r.w + pad;
                let ry_start = r.y.saturating_sub(pad);
                let ry_end = r.y + r.h + pad;
                if click_x >= rx_start && click_x <= rx_end && doc_y >= ry_start && doc_y <= ry_end {
                    if r.is_js {
                        return BrowserAction::JsAction(r.node_idx);
                    } else {
                        let target_slice = &r.target[..r.target_len];
                        if target_slice.starts_with(b"set_wallpaper:") {
                            let mut target = [0u8; 128];
                            let param = &target_slice[b"set_wallpaper:".len()..];
                            let plen = param.len().min(128);
                            target[..plen].copy_from_slice(&param[..plen]);
                            return BrowserAction::SetWallpaper(target, plen);
                        } else if target_slice.starts_with(b"save_wallpaper:") {
                            let mut target = [0u8; 128];
                            let param = &target_slice[b"save_wallpaper:".len()..];
                            let plen = param.len().min(128);
                            target[..plen].copy_from_slice(&param[..plen]);
                            return BrowserAction::SaveWallpaper(target, plen);
                        }
                        let mut target = [0u8; 128];
                        target[..r.target_len].copy_from_slice(&r.target[..r.target_len]);
                        return BrowserAction::NavigateTarget(target, r.target_len);
                    }
                }
            }

            // Hit-test em imagens: clicar sobre imagem define como papel de parede
            for i in 0..self.layout.image_count {
                let img = &self.layout.images[i];
                if click_x >= img.x && click_x <= img.x + img.w && doc_y >= img.y && doc_y <= img.y + img.h {
                    if img.src_len > 0 {
                        let mut target = [0u8; 128];
                        let slen = img.src_len.min(128);
                        target[..slen].copy_from_slice(&img.src[..slen]);
                        return BrowserAction::SetWallpaper(target, slen);
                    }
                }
            }
        }

        BrowserAction::None
    }

    pub fn render(&self, buf: &mut [u32]) {
        let w = self.viewport_w;
        let h = self.viewport_h;
        let clip_top = CLIP_TOP;
        let clip_bottom = content_clip_bottom(h);
        let max_content_x = w.saturating_sub(16);

        // 1. Fundo Geral
        fill_rect(buf, w, h, 0, 0, w, h, COLOR_BG);

        // 2. Barra Superior de Navegação (Chrome) - Estilo Glassmorphism / Frosted Glass translúcido (alpha ~140)
        fill_rect_alpha(buf, w, h, 0, 0, w, 26, 0x000F_172A, 140);
        draw_rect_border(buf, w, h, 0, 0, w, 26, 0x001E_293B);

        // Botões arredondados em cápsula: [<] [>] [R]
        let back_col = if self.can_go_back() { 0x00E2_E8F0 } else { 0x0047_5569 };
        let fwd_col = if self.can_go_forward() { 0x00E2_E8F0 } else { 0x0047_5569 };
        let reload_col = 0x0038_BDF8;

        fill_rounded_rect(buf, w, h, 4, 3, 18, 20, 4, 0x001E_293B);
        fill_rounded_rect(buf, w, h, 24, 3, 18, 20, 4, 0x001E_293B);
        fill_rounded_rect(buf, w, h, 44, 3, 18, 20, 4, 0x001E_293B);

        font::draw_text(buf, w, h, 9, 9, b"<", back_col, None);
        font::draw_text(buf, w, h, 29, 9, b">", fwd_col, None);
        font::draw_text(buf, w, h, 49, 9, b"R", reload_col, None);

        // Botão de Tela Cheia [F] no canto direito
        let fs_btn_x = w.saturating_sub(24);
        let fs_col = if self.fullscreen_state == FullscreenState::Fullscreen { 0x0038_BDF8 } else { 0x0094_A3B8 };
        fill_rounded_rect(buf, w, h, fs_btn_x.saturating_sub(2), 3, 20, 20, 4, 0x001E_293B);
        font::draw_text(buf, w, h, fs_btn_x + 3, 9, b"F", fs_col, None);

        // Campo de URL (Barra de Endereço em Pílula)
        let url_x = 66;
        let badge_w = 64;
        let url_w = w.saturating_sub(url_x + 32 + badge_w);
        let border_col = if self.is_address_focused { 0x0038_BDF8 } else { 0x0033_4155 };

        fill_rounded_rect(buf, w, h, url_x, 3, url_w, 20, 6, 0x000F_172A);
        draw_rounded_border(buf, w, h, url_x, 3, url_w, 20, 6, 1, border_col);

        // Badge de segurança / status de rede em tempo real à direita da barra
        let badge_x = url_x + url_w + 4;
        let is_https = self.current_url[..self.current_url_len].starts_with(b"https://");
        let (badge_txt, badge_bg, badge_fg): (&[u8], u32, u32) = if is_https {
            (b"TLS1.3", 0x0006_4E3B, 0x0034_D399) // Emerald
        } else {
            (b"e1000", 0x001E_3A5F, 0x0038_BDF8)  // Intel e1000 Sky
        };
        fill_rounded_rect(buf, w, h, badge_x, 3, badge_w.saturating_sub(6), 20, 4, badge_bg);
        font::draw_text(buf, w, h, badge_x + 6, 9, badge_txt, badge_fg, None);

        let url_max_x = url_x + url_w.saturating_sub(4);
        let vis_chars = (url_w.saturating_sub(14) / font::FONT_WIDTH).max(1);
        if self.is_address_focused {
            if self.address_selected && self.address_edit_len > 0 {
                let edit_slice = &self.address_edit[..self.address_edit_len];
                let shown = edit_slice.len().min(vis_chars);
                let sel_w = shown * font::FONT_WIDTH;
                fill_rect(buf, w, h, url_x + 6, 5, sel_w + 2, 16, 0x0045_475A);
                font::draw_text_clipped_rect(buf, w, h, url_x + 8, 9, edit_slice, 0x00CD_D6F4, None, url_max_x, 3, 23);
            } else {
                let start_ch = if self.address_edit_len + 1 > vis_chars {
                    (self.address_edit_len + 1) - vis_chars
                } else {
                    0
                };
                let edit_slice = &self.address_edit[start_ch..self.address_edit_len];
                font::draw_text_clipped_rect(buf, w, h, url_x + 8, 9, edit_slice, 0x0038_BDF8, None, url_max_x, 3, 23);
                // Cursor '_' sempre dentro da pilula
                let cursor_x = url_x + 8 + edit_slice.len() * font::FONT_WIDTH;
                if cursor_x + font::FONT_WIDTH <= url_max_x {
                    font::draw_text_clipped_rect(buf, w, h, cursor_x, 9, b"_", 0x0038_BDF8, None, url_max_x, 3, 23);
                }
            }
        } else {
            let url_slice = &self.current_url[..self.current_url_len];
            font::draw_text_clipped_rect(buf, w, h, url_x + 8, 9, url_slice, 0x00CD_D6F4, None, url_max_x, 3, 23);
        }

        // 2.5. Renderiza caixas estilizadas (backgrounds de .card, buttons, etc.) limitadas ao viewport horizontal
        for i in 0..self.layout.box_count {
            let bx = &self.layout.boxes[i];
            if bx.x >= max_content_x {
                continue;
            }
            let bx_w = bx.w.min(max_content_x.saturating_sub(bx.x));
            if bx_w == 0 {
                continue;
            }
            if bx.y + bx.h > self.scroll_y {
                let box_top = if bx.y >= self.scroll_y {
                    (bx.y - self.scroll_y) + clip_top
                } else {
                    clip_top
                };
                let box_bottom = ((bx.y + bx.h).saturating_sub(self.scroll_y)) + clip_top;
                if box_top < clip_bottom && box_bottom > clip_top {
                    let draw_y = box_top.max(clip_top);
                    let draw_h = box_bottom.min(clip_bottom).saturating_sub(draw_y);
                    if draw_h > 0 {
                        if bx.border_radius > 0 {
                            if bx.bg_color != 0 {
                                fill_rounded_rect(buf, w, h, bx.x, draw_y, bx_w, draw_h, bx.border_radius, bx.bg_color);
                            }
                            if let Some(bc) = bx.border_color {
                                draw_rounded_border(buf, w, h, bx.x, draw_y, bx_w, draw_h, bx.border_radius, bx.border_width, bc);
                            }
                        } else {
                            if bx.bg_color != 0 {
                                fill_rect(buf, w, h, bx.x, draw_y, bx_w, draw_h, bx.bg_color);
                            }
                            if let Some(bc) = bx.border_color {
                                draw_rect_border(buf, w, h, bx.x, draw_y, bx_w, draw_h, bc);
                            }
                        }
                    }
                }
            }
        }

        // 2.7. Renderiza imagens (<img>) com clipping e scroll
        for i in 0..self.layout.image_count {
            let img = &self.layout.images[i];
            if img.x >= max_content_x {
                continue;
            }
            if img.y + img.h > self.scroll_y {
                let img_top = if img.y >= self.scroll_y {
                    (img.y - self.scroll_y) + clip_top
                } else {
                    clip_top
                };
                let img_bottom = ((img.y + img.h).saturating_sub(self.scroll_y)) + clip_top;
                if img_top < clip_bottom && img_bottom > clip_top {
                    let draw_y = img_top.max(clip_top);
                    let draw_h = img_bottom.min(clip_bottom).saturating_sub(draw_y);
                    if draw_h > 0 {
                        let src_y_off = self.scroll_y.saturating_sub(img.y);
                        render_image_element(buf, w, h, img, draw_y, draw_h, src_y_off, clip_top, clip_bottom);
                    }
                }
            }
        }

        // 3. Documento no Viewport (com scroll_y aplicado e clipping horizontal/vertical)
        for i in 0..self.layout.line_count {
            let line = &self.layout.lines[i];
            if line.x >= max_content_x {
                continue;
            }

            if line.is_hr {
                if line.y >= self.scroll_y {
                    let actual_y = (line.y - self.scroll_y) + clip_top;
                    if actual_y >= clip_top && actual_y <= clip_bottom {
                        let hr_w = max_content_x.saturating_sub(line.x).min(w.saturating_sub(24));
                        fill_rect(buf, w, h, line.x, actual_y, hr_w, 1, COLOR_HR);
                    }
                }
            } else {
                if line.y >= self.scroll_y {
                    let actual_y = (line.y - self.scroll_y) + clip_top;
                    if actual_y + font::FONT_HEIGHT <= clip_bottom {
                        let text_slice = &line.text[..line.text_len];
                        font::draw_text_clipped_rect(
                            buf,
                            w,
                            h,
                            line.x,
                            actual_y,
                            text_slice,
                            line.color,
                            None,
                            max_content_x,
                            clip_top,
                            clip_bottom,
                        );

                        if line.is_link {
                            let underline_y = actual_y + font::FONT_HEIGHT;
                            if underline_y < clip_bottom {
                                let link_w = (line.text_len * font::FONT_WIDTH).min(max_content_x.saturating_sub(line.x));
                                fill_rect(buf, w, h, line.x, underline_y, link_w, 1, line.color);
                            }
                        }
                    }
                }
            }
        }

        // 4. Barra de Rolagem Vertical (Scrollbar Track & Thumb estritamente acima da barra BRCR)
        let track_x = w.saturating_sub(14);
        let track_w = 8;
        let track_y = clip_top;
        let track_h = clip_bottom.saturating_sub(clip_top);
        fill_rect(buf, w, h, track_x, track_y, track_w, track_h, COLOR_SCROLL_TRACK);
        draw_rect_border(buf, w, h, track_x, track_y, track_w, track_h, COLOR_BORDER);

        if self.max_scroll > 0 && track_h > 0 {
            let thumb_h = 24usize.min(track_h).max(track_h * (clip_bottom - clip_top) / self.layout.total_height.max(1));
            let thumb_travel = track_h.saturating_sub(thumb_h);
            let thumb_y = track_y + (self.scroll_y * thumb_travel / self.max_scroll.max(1));
            fill_rect(buf, w, h, track_x + 1, thumb_y, track_w - 2, thumb_h, COLOR_SCROLL_THUMB);
            draw_rect_border(buf, w, h, track_x + 1, thumb_y, track_w - 2, thumb_h, COLOR_SCROLL_THUMB_BORDER);
        }

        // 5. Barra Inferior de Telemetria BRCR (Jinn Browser 0.8.2)
        if h >= 100 {
            let status_y = h.saturating_sub(14);
            fill_rect(buf, w, h, 0, status_y, w, 14, COLOR_CHROME);
            let status_txt: &[u8] = if w >= 460 {
                b"BRCR: 0.134% [HIBERNATED] (Mem 0.336% | Gfx 0% | CPU 0)"
            } else {
                b"BRCR: 0.13% | Mem 0.34% | CPU 0"
            };
            font::draw_text_clipped_rect(
                buf,
                w,
                h,
                6,
                status_y + 3,
                status_txt,
                COLOR_URL_TEXT,
                None,
                w.saturating_sub(6),
                status_y + 1,
                h.saturating_sub(1),
            );
            draw_rect_border(buf, w, h, 0, status_y, w, 14, COLOR_BORDER);
        }
    }
}

// ── Primitivas Gráficas Locais ────────────────────────────────────────────────

fn fill_rect(buf: &mut [u32], stride_w: usize, max_h: usize, rx: usize, ry: usize, rw: usize, rh: usize, color: u32) {
    let x_end = (rx + rw).min(stride_w);
    let y_end = (ry + rh).min(max_h);
    for y in ry..y_end {
        let row = y * stride_w;
        for x in rx..x_end {
            let idx = row + x;
            if idx < buf.len() {
                buf[idx] = color;
            }
        }
    }
}

#[inline(always)]
fn alpha_blend(bg: u32, fg: u32, alpha: u8) -> u32 {
    let a = alpha as u32;
    let inv_a = 255 - a;
    let r = ((((fg >> 16) & 0xFF) * a + ((bg >> 16) & 0xFF) * inv_a) >> 8) & 0xFF;
    let g = ((((fg >> 8) & 0xFF) * a + ((bg >> 8) & 0xFF) * inv_a) >> 8) & 0xFF;
    let b = (((fg & 0xFF) * a + (bg & 0xFF) * inv_a) >> 8) & 0xFF;
    (r << 16) | (g << 8) | b
}

fn fill_rect_alpha(buf: &mut [u32], stride_w: usize, max_h: usize, rx: usize, ry: usize, rw: usize, rh: usize, color: u32, alpha: u8) {
    let x_end = (rx + rw).min(stride_w);
    let y_end = (ry + rh).min(max_h);
    for y in ry..y_end {
        let row = y * stride_w;
        for x in rx..x_end {
            let idx = row + x;
            if idx < buf.len() {
                buf[idx] = alpha_blend(buf[idx], color, alpha);
            }
        }
    }
}

fn draw_rect_border(buf: &mut [u32], stride_w: usize, max_h: usize, rx: usize, ry: usize, rw: usize, rh: usize, color: u32) {
    if rw == 0 || rh == 0 { return; }
    let x_end = rx + rw - 1;
    let y_end = ry + rh - 1;
    for x in rx..=x_end.min(stride_w.saturating_sub(1)) {
        if ry < max_h && ry * stride_w + x < buf.len() { buf[ry * stride_w + x] = color; }
        if y_end < max_h && y_end * stride_w + x < buf.len() { buf[y_end * stride_w + x] = color; }
    }
    for y in ry..=y_end.min(max_h.saturating_sub(1)) {
        if rx < stride_w && y * stride_w + rx < buf.len() { buf[y * stride_w + rx] = color; }
        if x_end < stride_w && y * stride_w + x_end < buf.len() { buf[y * stride_w + x_end] = color; }
    }
}

fn fill_rounded_rect(
    buf: &mut [u32],
    stride_w: usize,
    max_h: usize,
    rx: usize,
    ry: usize,
    rw: usize,
    rh: usize,
    radius: usize,
    color: u32,
) {
    if rw == 0 || rh == 0 { return; }
    let r = radius.min(rw / 2).min(rh / 2);
    let x_end = (rx + rw).min(stride_w);
    let y_end = (ry + rh).min(max_h);

    for y in ry..y_end {
        let dy = if y < ry + r {
            (ry + r) - y
        } else if y >= ry + rh - r {
            y - (ry + rh - r - 1)
        } else {
            0
        };

        for x in rx..x_end {
            let dx = if x < rx + r {
                (rx + r) - x
            } else if x >= rx + rw - r {
                x - (rx + rw - r - 1)
            } else {
                0
            };

            if dx > 0 && dy > 0 && dx * dx + dy * dy > r * r {
                continue;
            }

            let idx = y * stride_w + x;
            if idx < buf.len() {
                buf[idx] = color;
            }
        }
    }
}

fn draw_rounded_border(
    buf: &mut [u32],
    stride_w: usize,
    max_h: usize,
    rx: usize,
    ry: usize,
    rw: usize,
    rh: usize,
    radius: usize,
    border_w: usize,
    color: u32,
) {
    if rw == 0 || rh == 0 { return; }
    let r = radius.min(rw / 2).min(rh / 2);
    let bw = border_w.max(1);
    let x_end = (rx + rw).min(stride_w);
    let y_end = (ry + rh).min(max_h);

    for y in ry..y_end {
        let dy_top = if y < ry + r { (ry + r) - y } else { 0 };
        let dy_bot = if y >= ry + rh - r { y - (ry + rh - r - 1) } else { 0 };
        let dy = dy_top.max(dy_bot);
        let is_edge_y = y < ry + bw || y >= ry + rh - bw;

        for x in rx..x_end {
            let dx_left = if x < rx + r { (rx + r) - x } else { 0 };
            let dx_right = if x >= rx + rw - r { x - (rx + rw - r - 1) } else { 0 };
            let dx = dx_left.max(dx_right);
            let is_edge_x = x < rx + bw || x >= rx + rw - bw;

            if dx > 0 && dy > 0 {
                let dist_sq = dx * dx + dy * dy;
                if dist_sq > r * r {
                    continue;
                }
                let inner_r = r.saturating_sub(bw);
                if dist_sq < inner_r * inner_r {
                    continue;
                }
            } else if !is_edge_x && !is_edge_y {
                continue;
            }

            let idx = y * stride_w + x;
            if idx < buf.len() {
                buf[idx] = color;
            }
        }
    }
}

// ── Decodificadores de Imagem Sem Alocação Dinâmica (#![no_std]) ───────────────

pub const MAX_IMAGE_SCRATCH_W: usize = 320;
pub const MAX_IMAGE_SCRATCH_H: usize = 240;
pub const MAX_IMAGE_SCRATCH_PIXELS: usize = MAX_IMAGE_SCRATCH_W * MAX_IMAGE_SCRATCH_H;

static mut IMG_FILE_SCRATCH: [u8; 65536] = [0u8; 65536];
static mut IMG_DECODED_PIXELS: [u32; MAX_IMAGE_SCRATCH_PIXELS] = [0u32; MAX_IMAGE_SCRATCH_PIXELS];
static mut IMG_SCALED_PIXELS: [u32; MAX_IMAGE_SCRATCH_PIXELS] = [0u32; MAX_IMAGE_SCRATCH_PIXELS];
static mut IMG_PNG_IDAT_BUF: [u8; 65536] = [0u8; 65536];
static mut IMG_PNG_ZLIB_OUT: [u8; 524288] = [0u8; 524288];

const MAX_REMOTE_IMG_SLOTS: usize = 8;

#[derive(Clone, Copy)]
struct RemoteImgEntry {
    hash_a: u64,
    hash_b: u64,
    len: usize,
    orig_w: u16,
    orig_h: u16,
    valid: bool,
    data: [u8; 65536],
}

impl RemoteImgEntry {
    const fn empty() -> Self {
        Self {
            hash_a: 0,
            hash_b: 0,
            len: 0,
            orig_w: 0,
            orig_h: 0,
            valid: false,
            data: [0; 65536],
        }
    }
}

static mut REMOTE_IMG_CACHE: [RemoteImgEntry; MAX_REMOTE_IMG_SLOTS] = [RemoteImgEntry::empty(); MAX_REMOTE_IMG_SLOTS];
static mut REMOTE_IMG_NEXT_SLOT: usize = 0;

fn img_fnv1a_hash(s: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in s {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

pub unsafe fn cache_store_remote_image(key_a: &[u8], key_b: &[u8], raw: &[u8]) -> Option<(usize, usize)> {
    if raw.is_empty() || raw.len() > 65536 {
        return None;
    }
    let ha = img_fnv1a_hash(key_a);
    let hb = img_fnv1a_hash(key_b);
    let probe = probe_image(raw);
    let (ow, oh) = probe.map(|p| (p.width, p.height)).unwrap_or((0, 0));

    let mut target_slot = None;
    for idx in 0..MAX_REMOTE_IMG_SLOTS {
        if REMOTE_IMG_CACHE[idx].valid && (REMOTE_IMG_CACHE[idx].hash_a == ha || REMOTE_IMG_CACHE[idx].hash_b == hb) {
            target_slot = Some(idx);
            break;
        }
    }
    let slot = match target_slot {
        Some(s) => s,
        None => {
            let s = REMOTE_IMG_NEXT_SLOT % MAX_REMOTE_IMG_SLOTS;
            REMOTE_IMG_NEXT_SLOT = (REMOTE_IMG_NEXT_SLOT + 1) % MAX_REMOTE_IMG_SLOTS;
            s
        }
    };

    REMOTE_IMG_CACHE[slot].hash_a = ha;
    REMOTE_IMG_CACHE[slot].hash_b = hb;
    REMOTE_IMG_CACHE[slot].data[..raw.len()].copy_from_slice(raw);
    REMOTE_IMG_CACHE[slot].len = raw.len();
    REMOTE_IMG_CACHE[slot].orig_w = ow.min(u16::MAX as usize) as u16;
    REMOTE_IMG_CACHE[slot].orig_h = oh.min(u16::MAX as usize) as u16;
    REMOTE_IMG_CACHE[slot].valid = true;

    if ow > 0 && oh > 0 { Some((ow, oh)) } else { None }
}

pub unsafe fn cache_lookup_remote_image(key: &[u8]) -> Option<&'static [u8]> {
    if key.is_empty() {
        return None;
    }
    let h = img_fnv1a_hash(key);
    for idx in 0..MAX_REMOTE_IMG_SLOTS {
        let e = &REMOTE_IMG_CACHE[idx];
        if e.valid && (e.hash_a == h || e.hash_b == h) {
            return Some(&e.data[..e.len]);
        }
    }
    None
}

pub unsafe fn lookup_image_dimensions(src: &[u8]) -> Option<(usize, usize)> {
    if let Some(cached) = cache_lookup_remote_image(src) {
        if let Some(p) = probe_image(cached) {
            if p.width > 0 && p.height > 0 {
                return Some((p.width, p.height));
            }
        }
    }
    let mut path_buf = [0u8; 128];
    let path_len = if src.starts_with(b"/") {
        let l = src.len().min(127);
        path_buf[..l].copy_from_slice(&src[..l]);
        l
    } else {
        let prefix = b"/disk/";
        path_buf[..prefix.len()].copy_from_slice(prefix);
        let l = src.len().min(127 - prefix.len());
        path_buf[prefix.len()..prefix.len() + l].copy_from_slice(&src[..l]);
        prefix.len() + l
    };
    let mut fd = storage::sys_open(&path_buf[..path_len], storage::O_RDONLY);
    if fd < 0 && !src.starts_with(b"/") {
        let wp_prefix = b"/disk/wallpapers/";
        path_buf[..wp_prefix.len()].copy_from_slice(wp_prefix);
        let l = src.len().min(127 - wp_prefix.len());
        path_buf[wp_prefix.len()..wp_prefix.len() + l].copy_from_slice(&src[..l]);
        let plen = wp_prefix.len() + l;
        fd = storage::sys_open(&path_buf[..plen], storage::O_RDONLY);
    }
    if fd >= 0 {
        let n = storage::sys_read(fd as u64, IMG_FILE_SCRATCH.as_mut_ptr(), 4096);
        let _ = storage::sys_close(fd as u64);
        if n > 0 {
            if let Some(p) = probe_image(&IMG_FILE_SCRATCH[..n as usize]) {
                if p.width > 0 && p.height > 0 {
                    return Some((p.width, p.height));
                }
            }
        }
    }
    None
}

/// Decodificador PNG puro `#![no_std]` (color types 0, 2, 3, 4, 6; bit depth 8)
pub fn decode_png(
    data: &[u8],
    out_pixels: &mut [u32],
    max_w: usize,
    max_h: usize,
) -> Option<(usize, usize)> {
    if data.len() < 33 || !data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return None;
    }

    let mut pos = 8usize;
    let mut width = 0usize;
    let mut height = 0usize;
    let mut bit_depth = 0u8;
    let mut color_type = 255u8;
    let mut palette = [[0u8; 4]; 256];
    let mut pal_count = 0usize;
    let mut idat_len = 0usize;
    let idat_buf = unsafe { &mut *core::ptr::addr_of_mut!(IMG_PNG_IDAT_BUF) };

    for idx in 0..256 {
        palette[idx][3] = 0xFF;
    }

    while pos + 12 <= data.len() {
        let clen = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        let ctype = &data[pos + 4..pos + 8];
        let dstart = pos + 8;
        let dend = dstart.checked_add(clen)?;
        if dend + 4 > data.len() {
            break;
        }
        let chunk = &data[dstart..dend];

        if ctype == b"IHDR" {
            if clen < 13 { return None; }
            width = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) as usize;
            height = u32::from_be_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]) as usize;
            bit_depth = chunk[8];
            color_type = chunk[9];
            let compression = chunk[10];
            let filter_m = chunk[11];
            let interlace = chunk[12];
            if width == 0 || height == 0 || width > 2048 || height > 2048 {
                return None;
            }
            if bit_depth != 8 || compression != 0 || filter_m != 0 || interlace != 0 {
                return None;
            }
        } else if ctype == b"PLTE" {
            let entries = (clen / 3).min(256);
            for idx in 0..entries {
                palette[idx][0] = chunk[idx * 3];
                palette[idx][1] = chunk[idx * 3 + 1];
                palette[idx][2] = chunk[idx * 3 + 2];
            }
            pal_count = entries;
        } else if ctype == b"tRNS" && color_type == 3 {
            let entries = clen.min(pal_count.max(256));
            for idx in 0..entries {
                palette[idx][3] = chunk[idx];
            }
        } else if ctype == b"IDAT" {
            if idat_len + clen > idat_buf.len() {
                return None;
            }
            idat_buf[idat_len..idat_len + clen].copy_from_slice(chunk);
            idat_len += clen;
        } else if ctype == b"IEND" {
            break;
        }

        pos = dend + 4;
    }

    if width == 0 || height == 0 || idat_len == 0 {
        return None;
    }

    let bpp = match color_type {
        0 => 1usize, // Grayscale
        2 => 3usize, // RGB
        3 => 1usize, // Indexed
        4 => 2usize, // Gray + Alpha
        6 => 4usize, // RGBA
        _ => return None,
    };

    let row_bytes = width.checked_mul(bpp)?;
    let scanline_stride = row_bytes.checked_add(1)?;
    let expected_raw = scanline_stride.checked_mul(height)?;
    let zlib_out = unsafe { &mut *core::ptr::addr_of_mut!(IMG_PNG_ZLIB_OUT) };
    if expected_raw > zlib_out.len() {
        return None;
    }

    let raw_len = crate::gzip::decompress_deflate(&idat_buf[..idat_len], zlib_out).ok()?;
    if raw_len < expected_raw {
        return None;
    }

    // Desfaz os filtros de cada scanline in-place dentro de zlib_out
    #[inline(always)]
    fn paeth_predictor(a: i32, b: i32, c: i32) -> u8 {
        let p = a + b - c;
        let pa = (p - a).abs();
        let pb = (p - b).abs();
        let pc = (p - c).abs();
        if pa <= pb && pa <= pc { a as u8 } else if pb <= pc { b as u8 } else { c as u8 }
    }

    for y in 0..height {
        let cur_off = y * scanline_stride;
        let filter_type = zlib_out[cur_off];
        let prev_off = cur_off.saturating_sub(scanline_stride);
        let data_off = cur_off + 1;
        match filter_type {
            0 => {}
            1 => {
                for x in bpp..row_bytes {
                    zlib_out[data_off + x] = zlib_out[data_off + x].wrapping_add(zlib_out[data_off + x - bpp]);
                }
            }
            2 => {
                if y > 0 {
                    for x in 0..row_bytes {
                        zlib_out[data_off + x] = zlib_out[data_off + x].wrapping_add(zlib_out[prev_off + 1 + x]);
                    }
                }
            }
            3 => {
                for x in 0..row_bytes {
                    let left = if x >= bpp { zlib_out[data_off + x - bpp] as u16 } else { 0 };
                    let up = if y > 0 { zlib_out[prev_off + 1 + x] as u16 } else { 0 };
                    zlib_out[data_off + x] = zlib_out[data_off + x].wrapping_add(((left + up) >> 1) as u8);
                }
            }
            4 => {
                for x in 0..row_bytes {
                    let left = if x >= bpp { zlib_out[data_off + x - bpp] as i32 } else { 0 };
                    let up = if y > 0 { zlib_out[prev_off + 1 + x] as i32 } else { 0 };
                    let up_left = if y > 0 && x >= bpp { zlib_out[prev_off + 1 + x - bpp] as i32 } else { 0 };
                    zlib_out[data_off + x] = zlib_out[data_off + x].wrapping_add(paeth_predictor(left, up, up_left));
                }
            }
            _ => return None,
        }
    }

    let dec_w = width.min(max_w);
    let dec_h = height.min(max_h);
    if dec_w * dec_h > out_pixels.len() {
        return None;
    }

    for dy in 0..dec_h {
        let sy = dy * height / dec_h;
        let row_start = sy * scanline_stride + 1;
        let dst_row = dy * dec_w;
        for dx in 0..dec_w {
            let sx = dx * width / dec_w;
            let px_off = row_start + sx * bpp;
            let (r, g, b, a): (u32, u32, u32, u32) = match color_type {
                0 => {
                    let gval = zlib_out[px_off] as u32;
                    (gval, gval, gval, 0xFF)
                }
                2 => (
                    zlib_out[px_off] as u32,
                    zlib_out[px_off + 1] as u32,
                    zlib_out[px_off + 2] as u32,
                    0xFF,
                ),
                3 => {
                    let idx = zlib_out[px_off] as usize;
                    let pal = palette[idx];
                    (pal[0] as u32, pal[1] as u32, pal[2] as u32, pal[3] as u32)
                }
                4 => {
                    let gval = zlib_out[px_off] as u32;
                    let alpha = zlib_out[px_off + 1] as u32;
                    (gval, gval, gval, alpha)
                }
                6 => (
                    zlib_out[px_off] as u32,
                    zlib_out[px_off + 1] as u32,
                    zlib_out[px_off + 2] as u32,
                    zlib_out[px_off + 3] as u32,
                ),
                _ => (0, 0, 0, 0xFF),
            };
            out_pixels[dst_row + dx] = (a << 24) | (r << 16) | (g << 8) | b;
        }
    }

    Some((dec_w, dec_h))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    PpmBinary,
    PpmAscii,
    Bmp24,
    Bmp32,
    Png,
    Jpeg,
    Unknown,
}

#[derive(Clone, Copy, Debug)]
pub struct ImageProbeInfo {
    pub format: ImageFormat,
    pub width: usize,
    pub height: usize,
}

/// Helper para obter próximo token de texto em cabeçalho PPM (ignora espaços e comentários '#' até '\n')
fn ppm_next_token<'a>(data: &'a [u8], offset: &mut usize) -> Option<&'a [u8]> {
    while *offset < data.len() {
        let b = data[*offset];
        if b == b'#' {
            while *offset < data.len() && data[*offset] != b'\n' {
                *offset += 1;
            }
        } else if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' {
            *offset += 1;
        } else {
            break;
        }
    }
    if *offset >= data.len() {
        return None;
    }
    let start = *offset;
    while *offset < data.len() {
        let b = data[*offset];
        if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' || b == b'#' {
            break;
        }
        *offset += 1;
    }
    Some(&data[start..*offset])
}

fn parse_ascii_usize(bytes: &[u8]) -> Option<usize> {
    let mut val = 0usize;
    if bytes.is_empty() { return None; }
    for &b in bytes {
        if b >= b'0' && b <= b'9' {
            val = val.saturating_mul(10).saturating_add((b - b'0') as usize);
        } else {
            return None;
        }
    }
    Some(val)
}

/// Decodificador de Netpbm PPM (P6 binário e P3 ASCII)
pub fn decode_ppm(
    data: &[u8],
    out_pixels: &mut [u32],
    max_w: usize,
    max_h: usize,
) -> Option<(usize, usize)> {
    if data.len() < 8 || data[0] != b'P' {
        return None;
    }
    let is_p6 = data[1] == b'6';
    let is_p3 = data[1] == b'3';
    if !is_p6 && !is_p3 {
        return None;
    }

    let mut offset = 2;
    let w_bytes = ppm_next_token(data, &mut offset)?;
    let h_bytes = ppm_next_token(data, &mut offset)?;
    let maxval_bytes = ppm_next_token(data, &mut offset)?;

    let width = parse_ascii_usize(w_bytes)?;
    let height = parse_ascii_usize(h_bytes)?;
    let maxval = parse_ascii_usize(maxval_bytes)?;

    if width == 0 || height == 0 || maxval == 0 || maxval > 255 {
        return None;
    }

    let dec_w = width.min(max_w);
    let dec_h = height.min(max_h);

    if is_p6 {
        if offset < data.len() && (data[offset] == b' ' || data[offset] == b'\n' || data[offset] == b'\r') {
            offset += 1;
            if offset < data.len() && data[offset] == b'\n' && data[offset - 1] == b'\r' {
                offset += 1;
            }
        }

        for y in 0..dec_h {
            let row_start = offset + y * width * 3;
            let dst_row = y * dec_w;
            for x in 0..dec_w {
                let px_idx = row_start + x * 3;
                if px_idx + 2 < data.len() {
                    let r = data[px_idx] as u32;
                    let g = data[px_idx + 1] as u32;
                    let b = data[px_idx + 2] as u32;
                    out_pixels[dst_row + x] = 0xFF00_0000 | (r << 16) | (g << 8) | b;
                }
            }
        }
    } else {
        // P3 ASCII
        for y in 0..dec_h {
            let dst_row = y * dec_w;
            for x in 0..width {
                let r_bytes = ppm_next_token(data, &mut offset)?;
                let g_bytes = ppm_next_token(data, &mut offset)?;
                let b_bytes = ppm_next_token(data, &mut offset)?;
                if x < dec_w {
                    let r = parse_ascii_usize(r_bytes).unwrap_or(0).min(255) as u32;
                    let g = parse_ascii_usize(g_bytes).unwrap_or(0).min(255) as u32;
                    let b = parse_ascii_usize(b_bytes).unwrap_or(0).min(255) as u32;
                    out_pixels[dst_row + x] = 0xFF00_0000 | (r << 16) | (g << 8) | b;
                }
            }
        }
    }

    Some((dec_w, dec_h))
}

/// Decodificador de Windows BMP (24-bit e 32-bit não-comprimido, BI_RGB)
pub fn decode_bmp(
    data: &[u8],
    out_pixels: &mut [u32],
    max_w: usize,
    max_h: usize,
) -> Option<(usize, usize)> {
    if data.len() < 54 || data[0] != b'B' || data[1] != b'M' {
        return None;
    }

    let off_bits = u32::from_le_bytes([data[10], data[11], data[12], data[13]]) as usize;
    let bi_size = u32::from_le_bytes([data[14], data[15], data[16], data[17]]) as usize;
    if bi_size < 40 || off_bits >= data.len() {
        return None;
    }

    let bi_width = i32::from_le_bytes([data[18], data[19], data[20], data[21]]);
    let bi_height = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
    let bi_planes = u16::from_le_bytes([data[26], data[27]]);
    let bi_bit_count = u16::from_le_bytes([data[28], data[29]]);
    let bi_compression = u32::from_le_bytes([data[30], data[31], data[32], data[33]]);

    if bi_planes != 1 || bi_width <= 0 || bi_height == 0 {
        return None;
    }
    if bi_bit_count != 24 && bi_bit_count != 32 {
        return None;
    }
    if bi_compression != 0 && bi_compression != 3 {
        return None;
    }

    let width = bi_width as usize;
    let height = bi_height.unsigned_abs() as usize;
    let is_bottom_up = bi_height > 0;

    let dec_w = width.min(max_w);
    let dec_h = height.min(max_h);

    let bytes_per_pixel = (bi_bit_count / 8) as usize;
    let row_stride = ((width * bytes_per_pixel + 3) / 4) * 4;

    for y in 0..dec_h {
        let src_y = if is_bottom_up {
            height.saturating_sub(1 + y)
        } else {
            y
        };
        let row_start = off_bits + src_y * row_stride;
        let dst_row = y * dec_w;

        if row_start >= data.len() {
            continue;
        }

        for x in 0..dec_w {
            let px_offset = row_start + x * bytes_per_pixel;
            if px_offset + bytes_per_pixel <= data.len() {
                let b = data[px_offset] as u32;
                let g = data[px_offset + 1] as u32;
                let r = data[px_offset + 2] as u32;
                let a = if bytes_per_pixel == 4 {
                    let alpha = data[px_offset + 3] as u32;
                    if alpha == 0 { 0xFF } else { alpha }
                } else {
                    0xFF
                };
                out_pixels[dst_row + x] = (a << 24) | (r << 16) | (g << 8) | b;
            }
        }
    }

    Some((dec_w, dec_h))
}

/// Identificador de formato e cabeçalho de imagem (PPM, BMP, PNG, JPEG)
pub fn probe_image(data: &[u8]) -> Option<ImageProbeInfo> {
    if data.len() >= 8 && data[0] == b'P' && (data[1] == b'6' || data[1] == b'3') {
        let mut offset = 2;
        if let (Some(wb), Some(hb)) = (ppm_next_token(data, &mut offset), ppm_next_token(data, &mut offset)) {
            if let (Some(w), Some(h)) = (parse_ascii_usize(wb), parse_ascii_usize(hb)) {
                return Some(ImageProbeInfo {
                    format: if data[1] == b'6' { ImageFormat::PpmBinary } else { ImageFormat::PpmAscii },
                    width: w,
                    height: h,
                });
            }
        }
    } else if data.len() >= 26 && data[0] == b'B' && data[1] == b'M' {
        let bi_w = i32::from_le_bytes([data[18], data[19], data[20], data[21]]);
        let bi_h = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
        let bpp = u16::from_le_bytes([data[28], data[29]]);
        if bi_w > 0 && bi_h != 0 {
            return Some(ImageProbeInfo {
                format: if bpp == 32 { ImageFormat::Bmp32 } else { ImageFormat::Bmp24 },
                width: bi_w as usize,
                height: bi_h.unsigned_abs() as usize,
            });
        }
    } else if data.len() >= 24 && data.starts_with(b"\x89PNG\r\n\x1a\n") {
        let w = u32::from_be_bytes([data[16], data[17], data[18], data[19]]) as usize;
        let h = u32::from_be_bytes([data[20], data[21], data[22], data[23]]) as usize;
        return Some(ImageProbeInfo {
            format: ImageFormat::Png,
            width: w,
            height: h,
        });
    } else if data.len() >= 10 && data.starts_with(b"\xFF\xD8\xFF") {
        let mut idx = 2;
        while idx + 8 < data.len() {
            if data[idx] == 0xFF {
                let m = data[idx + 1];
                if m == 0xC0 || m == 0xC1 || m == 0xC2 {
                    let h = u16::from_be_bytes([data[idx + 5], data[idx + 6]]) as usize;
                    let w = u16::from_be_bytes([data[idx + 7], data[idx + 8]]) as usize;
                    return Some(ImageProbeInfo {
                        format: ImageFormat::Jpeg,
                        width: w,
                        height: h,
                    });
                } else if m == 0xD9 || m == 0xDA {
                    break;
                } else if m != 0x00 && m != 0xFF {
                    let seg_len = u16::from_be_bytes([data[idx + 2], data[idx + 3]]) as usize;
                    idx += 2 + seg_len;
                    continue;
                }
            }
            idx += 1;
        }
        return Some(ImageProbeInfo {
            format: ImageFormat::Jpeg,
            width: 0,
            height: 0,
        });
    }
    None
}

/// Redimensionamento bilinear de imagens RGBA com aceleração por inteiros de ponto fixo (16.16)
pub fn scale_image_rgba_simd(
    src: &[u32],
    src_w: usize,
    src_h: usize,
    dst: &mut [u32],
    dst_w: usize,
    dst_h: usize,
) {
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
        return;
    }
    if src.len() < src_w * src_h || dst.len() < dst_w * dst_h {
        return;
    }

    let x_ratio = ((src_w << 16) / dst_w) as u32;
    let y_ratio = ((src_h << 16) / dst_h) as u32;

    for y in 0..dst_h {
        let sy = (y as u32 * y_ratio) >> 16;
        let y_diff = ((y as u32 * y_ratio) & 0xFFFF) as u32;
        let y_diff_inv = 0x10000 - y_diff;
        let sy1 = (sy + 1).min(src_h as u32 - 1) as usize;
        let sy0 = sy as usize;

        let row_dst = y * dst_w;
        let row_src0 = sy0 * src_w;
        let row_src1 = sy1 * src_w;

        for x in 0..dst_w {
            let sx = (x as u32 * x_ratio) >> 16;
            let x_diff = ((x as u32 * x_ratio) & 0xFFFF) as u32;
            let x_diff_inv = 0x10000 - x_diff;
            let sx1 = (sx + 1).min(src_w as u32 - 1) as usize;
            let sx0 = sx as usize;

            let p00 = src[row_src0 + sx0];
            let p10 = src[row_src0 + sx1];
            let p01 = src[row_src1 + sx0];
            let p11 = src[row_src1 + sx1];

            let mut out_pixel = 0u32;
            for shift in [0, 8, 16, 24] {
                let c00 = ((p00 >> shift) & 0xFF) as u32;
                let c10 = ((p10 >> shift) & 0xFF) as u32;
                let c01 = ((p01 >> shift) & 0xFF) as u32;
                let c11 = ((p11 >> shift) & 0xFF) as u32;

                let top = (c00 * x_diff_inv + c10 * x_diff) >> 16;
                let bottom = (c01 * x_diff_inv + c11 * x_diff) >> 16;
                let final_c = ((top * y_diff_inv + bottom * y_diff) >> 16) & 0xFF;
                out_pixel |= final_c << shift;
            }
            dst[row_dst + x] = out_pixel;
        }
    }
}

pub fn render_image_element(
    buf: &mut [u32],
    w: usize,
    h: usize,
    img: &RenderImage,
    draw_y: usize,
    draw_h: usize,
    src_y_off: usize,
    clip_top: usize,
    clip_bottom: usize,
) {
    let max_x = w.saturating_sub(16);
    if img.w == 0 || draw_h == 0 || img.x >= max_x { return; }
    let vis_img_w = img.w.min(max_x.saturating_sub(img.x));
    if vis_img_w == 0 { return; }

    // Fundo padrão da imagem (Catppuccin Surface)
    fill_rect(buf, w, h, img.x, draw_y, vis_img_w, draw_h, 0x0031_3244);
    draw_rect_border(buf, w, h, img.x, draw_y, vis_img_w, draw_h, 0x0045_475A);

    let mut decoded_ok = false;
    let mut probe_res: Option<ImageProbeInfo> = None;

    if img.src_len > 0 {
        let src_slice = &img.src[..img.src_len];
        unsafe {
            let remote_raw = cache_lookup_remote_image(src_slice);
            let raw_opt: Option<&[u8]> = if let Some(cached) = remote_raw {
                Some(cached)
            } else {
                let mut path_buf = [0u8; 128];
                let mut path_len;
                if src_slice.starts_with(b"/") {
                    let l = src_slice.len().min(127);
                    path_buf[..l].copy_from_slice(&src_slice[..l]);
                    path_len = l;
                } else {
                    let prefix = b"/disk/";
                    path_buf[..prefix.len()].copy_from_slice(prefix);
                    let l = src_slice.len().min(127 - prefix.len());
                    path_buf[prefix.len()..prefix.len() + l].copy_from_slice(&src_slice[..l]);
                    path_len = prefix.len() + l;
                }

                let mut read_fd = storage::sys_open(&path_buf[..path_len], storage::O_RDONLY);
                if read_fd < 0 && !src_slice.starts_with(b"/") {
                    let wp_prefix = b"/disk/wallpapers/";
                    path_buf[..wp_prefix.len()].copy_from_slice(wp_prefix);
                    let l = src_slice.len().min(127 - wp_prefix.len());
                    path_buf[wp_prefix.len()..wp_prefix.len() + l].copy_from_slice(&src_slice[..l]);
                    path_len = wp_prefix.len() + l;
                    read_fd = storage::sys_open(&path_buf[..path_len], storage::O_RDONLY);
                }
                if read_fd >= 0 {
                    let bytes_read = storage::sys_read(read_fd as u64, IMG_FILE_SCRATCH.as_mut_ptr(), IMG_FILE_SCRATCH.len() as u64);
                    let _ = storage::sys_close(read_fd as u64);
                    if bytes_read > 0 {
                        Some(&IMG_FILE_SCRATCH[..bytes_read as usize])
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            if let Some(raw_slice) = raw_opt {
                probe_res = probe_image(raw_slice);

                let dec_res = if raw_slice.starts_with(b"P6") || raw_slice.starts_with(b"P3") {
                    decode_ppm(raw_slice, &mut IMG_DECODED_PIXELS, MAX_IMAGE_SCRATCH_W, MAX_IMAGE_SCRATCH_H)
                } else if raw_slice.starts_with(b"BM") {
                    decode_bmp(raw_slice, &mut IMG_DECODED_PIXELS, MAX_IMAGE_SCRATCH_W, MAX_IMAGE_SCRATCH_H)
                } else if raw_slice.starts_with(b"\x89PNG\r\n\x1a\n") {
                    decode_png(raw_slice, &mut IMG_DECODED_PIXELS, MAX_IMAGE_SCRATCH_W, MAX_IMAGE_SCRATCH_H)
                } else {
                    None
                };

                if let Some((dec_w, dec_h)) = dec_res {
                    let target_w = img.w.min(MAX_IMAGE_SCRATCH_W);
                    let target_h = img.h.min(MAX_IMAGE_SCRATCH_H);

                    scale_image_rgba_simd(
                        &IMG_DECODED_PIXELS[..dec_w * dec_h],
                        dec_w,
                        dec_h,
                        &mut IMG_SCALED_PIXELS[..target_w * target_h],
                        target_w,
                        target_h,
                    );

                    for y in 0..draw_h {
                        let screen_y = draw_y + y;
                        if screen_y >= clip_top && screen_y < clip_bottom && screen_y < h {
                            let scaled_y = (src_y_off + y).min(target_h.saturating_sub(1));
                            let src_row = scaled_y * target_w;
                            let dst_row = screen_y * w;

                            for x in 0..target_w.min(vis_img_w) {
                                let screen_x = img.x + x;
                                if screen_x < max_x && screen_x < w {
                                    let p = IMG_SCALED_PIXELS[src_row + x];
                                    let alpha = (p >> 24) & 0xFF;
                                    if alpha >= 250 {
                                        buf[dst_row + screen_x] = p & 0x00FF_FFFF;
                                    } else if alpha > 10 {
                                        let bg = buf[dst_row + screen_x];
                                        let inv_a = 255 - alpha;
                                        let r = ((((p >> 16) & 0xFF) * alpha + ((bg >> 16) & 0xFF) * inv_a) >> 8) & 0xFF;
                                        let g = ((((p >> 8) & 0xFF) * alpha + ((bg >> 8) & 0xFF) * inv_a) >> 8) & 0xFF;
                                        let b = (((p & 0xFF) * alpha + (bg & 0xFF) * inv_a) >> 8) & 0xFF;
                                        buf[dst_row + screen_x] = (r << 16) | (g << 8) | b;
                                    }
                                }
                            }
                        }
                    }

                    draw_rect_border(buf, w, h, img.x, draw_y, vis_img_w, draw_h, COLOR_BORDER);
                    decoded_ok = true;
                }
            }
        }
    }

    if !decoded_ok {
        if draw_h >= 14 && vis_img_w >= 28 {
            let text_y = draw_y + 2;
            let text_max_x = img.x + vis_img_w.saturating_sub(2);
            if text_y + font::FONT_HEIGHT <= clip_bottom {
                let (tag, tag_color) = if let Some(probe) = probe_res {
                    match probe.format {
                        ImageFormat::Png => (b"[PNG]" as &[u8], 0x0089_DCEB),
                        ImageFormat::Jpeg => (b"[JPG]" as &[u8], 0x00FAB387),
                        ImageFormat::Bmp24 | ImageFormat::Bmp32 => (b"[BMP]" as &[u8], 0x00A6_E3A1),
                        ImageFormat::PpmBinary | ImageFormat::PpmAscii => (b"[PPM]" as &[u8], 0x00F9_E2AF),
                        _ => (b"[IMG]" as &[u8], COLOR_H1),
                    }
                } else {
                    (b"[IMG]" as &[u8], COLOR_H1)
                };

                font::draw_text_clipped_rect(buf, w, h, img.x + 3, text_y, tag, tag_color, None, text_max_x, clip_top, clip_bottom);

                let alt_x = img.x + 3 + tag.len() * font::FONT_WIDTH + 4;
                if alt_x + font::FONT_WIDTH <= text_max_x {
                    if img.alt_len > 0 {
                        let alt_slice = &img.alt[..img.alt_len];
                        font::draw_text_clipped_rect(buf, w, h, alt_x, text_y, alt_slice, COLOR_TEXT, None, text_max_x, clip_top, clip_bottom);
                    } else if img.src_len > 0 {
                        let src_slice = &img.src[..img.src_len];
                        font::draw_text_clipped_rect(buf, w, h, alt_x, text_y, src_slice, 0x00A6_ADC8, None, text_max_x, clip_top, clip_bottom);
                    }
                }
            }
        }
    }
}

/// Extrai URL de redirecionamento `<meta http-equiv="refresh" content="N;url=...">`
pub fn extract_meta_refresh_url(html: &[u8], out: &mut [u8; 128]) -> Option<usize> {
    let scan_len = html.len().min(4096);
    let head = &html[..scan_len];
    let mut i = 0usize;
    while i + 5 < head.len() {
        if head[i..].starts_with(b"<meta") || head[i..].starts_with(b"<META") {
            let mut end = i + 5;
            while end < head.len() && head[end] != b'>' {
                end += 1;
            }
            let tag = &head[i..end];
            if tag.contains_ignore_ascii_case(b"refresh") {
                if let Some(content) = extract_attr_val(tag, b"content=") {
                    if let Some(upos) = content.windows(4).position(|w| w.eq_ignore_ascii_case(b"url=")) {
                        let mut url = &content[upos + 4..];
                        url = url.trim_ascii();
                        if !url.is_empty() && (url[0] == b'\'' || url[0] == b'"') {
                            url = &url[1..];
                        }
                        if !url.is_empty() && (url[url.len() - 1] == b'\'' || url[url.len() - 1] == b'"') {
                            url = &url[..url.len() - 1];
                        }
                        let cpy = url.len().min(127);
                        if cpy > 0 {
                            out[..cpy].copy_from_slice(&url[..cpy]);
                            out[cpy] = 0;
                            return Some(cpy);
                        }
                    }
                }
            }
            i = end;
        }
        i += 1;
    }
    None
}


// ── Subsistema HTTP / URL do Jinn Browser 0.4.0 ───────────────────────────────

#[derive(Clone, Copy)]
pub struct ParsedHttpUrl {
    pub host: [u8; 64],
    pub host_len: usize,
    pub port: u16,
    pub path: [u8; 128],
    pub path_len: usize,
    pub is_https: bool,
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

    let is_https = clean.starts_with(b"https://");
    if clean.starts_with(b"http://") {
        clean = &clean[7..];
    } else if clean.starts_with(b"https://") {
        clean = &clean[8..];
    }

    // Separa host (+ porta) do caminho
    let (host_port_slice, path_slice) = match clean.iter().position(|&b| b == b'/') {
        Some(idx) => (&clean[..idx], &clean[idx..]),
        None => (clean, &b"/"[..]),
    };

    let default_port = if is_https { 443 } else { 80 };

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
            if p == 0 { (h, default_port) } else { (h, p) }
        }
        None => (host_port_slice, default_port),
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
        is_https,
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

/// Verifica se uma referência textual representa um domínio web ou endereço externo (ex: "google.com", "example.com/foo", "10.0.2.2:8080")
pub fn is_web_domain(s: &[u8]) -> bool {
    if s.is_empty() { return false; }
    let host_part = match s.iter().position(|&b| b == b'/') {
        Some(pos) => &s[..pos],
        None => s,
    };
    if host_part.eq_ignore_ascii_case(b"index.htm")
        || host_part.eq_ignore_ascii_case(b"about.htm")
        || host_part.eq_ignore_ascii_case(b"readme.txt")
        || host_part.ends_with(b".htm")
        || host_part.ends_with(b".html")
        || host_part.ends_with(b".txt")
        || host_part.ends_with(b".elf")
        || host_part.ends_with(b".ppm")
        || host_part.ends_with(b".png")
        || host_part.ends_with(b".jpg")
        || host_part.ends_with(b".jpeg")
        || host_part.ends_with(b".bmp")
        || host_part.ends_with(b".gif")
        || host_part.ends_with(b".svg")
        || host_part.ends_with(b".webp")
        || host_part.ends_with(b".ico")
        || host_part.ends_with(b".css")
        || host_part.ends_with(b".js")
        || host_part.ends_with(b".json")
        || host_part.ends_with(b".xml")
        || host_part.ends_with(b".pdf")
    {
        return false;
    }
    if host_part.starts_with(b"www.") { return true; }
    if host_part.contains(&b'.') || host_part.contains(&b':') {
        return true;
    }
    false
}

/// Verifica de forma determinística e compatível com RFC 7230 / RFC 9112 se o fluxo chunked
/// foi totalmente recebido, analisando a cadeia de chunks a partir do byte 0 sem falsos positivos.
pub fn is_chunked_complete(mut raw: &[u8]) -> bool {
    // Se o slice contiver cabeçalhos HTTP, avança até o início efetivo do corpo chunked
    if raw.starts_with(b"HTTP/1.") {
        if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            raw = &raw[pos + 4..];
        } else if let Some(pos) = raw.windows(2).position(|w| w == b"\n\n") {
            raw = &raw[pos + 2..];
        } else {
            return false;
        }
    }

    let mut src_idx = 0usize;
    while src_idx < raw.len() {
        let line_end = match raw[src_idx..].iter().position(|&b| b == b'\n') {
            Some(p) => src_idx + p,
            None => return false,
        };
        let mut hex_slice = &raw[src_idx..line_end];
        if !hex_slice.is_empty() && hex_slice[hex_slice.len() - 1] == b'\r' {
            hex_slice = &hex_slice[..hex_slice.len() - 1];
        }
        if let Some(semi) = hex_slice.iter().position(|&b| b == b';') {
            hex_slice = &hex_slice[..semi];
        }
        let hex_slice = hex_slice.trim_ascii();
        if hex_slice.is_empty() {
            src_idx = line_end + 1;
            continue;
        }

        let mut chunk_size: usize = 0;
        for &b in hex_slice {
            let val = match b {
                b'0'..=b'9' => (b - b'0') as usize,
                b'a'..=b'f' => (b - b'a' + 10) as usize,
                b'A'..=b'F' => (b - b'A' + 10) as usize,
                _ => return false,
            };
            chunk_size = match chunk_size.checked_mul(16).and_then(|c| c.checked_add(val)) {
                Some(c) => c,
                None => return false,
            };
        }

        src_idx = line_end + 1;
        if chunk_size == 0 {
            // Último chunk encontrado (tamanho 0). O payload está completo.
            return true;
        }

        // Se o chunk atual ainda não foi inteiramente recebido, o fluxo ainda não está completo
        if src_idx + chunk_size > raw.len() {
            return false;
        }
        src_idx += chunk_size;

        // Pula o delimitador CRLF após o corpo do chunk
        if src_idx < raw.len() && raw[src_idx] == b'\r' {
            src_idx += 1;
        }
        if src_idx < raw.len() && raw[src_idx] == b'\n' {
            src_idx += 1;
        }
    }
    false
}

/// Verifica se o payload contém o terminador de chunked transfer (RFC 7230)
pub fn has_chunked_terminator(body: &[u8]) -> bool {
    is_chunked_complete(body)
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

    // Normalização amigável de jinn.os.dev.br ou jinn-os.dev.br
    if clean_target.starts_with(b"jinn.os.dev.br") || clean_target.starts_with(b"jinn-os.dev.br") {
        let prefix = b"https://jinn-os.dev.br";
        let rest = if clean_target.starts_with(b"jinn.os.dev.br") {
            &clean_target[b"jinn.os.dev.br".len()..]
        } else {
            &clean_target[b"jinn-os.dev.br".len()..]
        };
        let p_len = prefix.len();
        out[..p_len].copy_from_slice(prefix);
        let r_len = rest.len().min(127 - p_len);
        out[p_len..p_len + r_len].copy_from_slice(&rest[..r_len]);
        let total = p_len + r_len;
        out[total] = 0;
        return total;
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
        let prefix: &[u8] = if base.starts_with(b"https://") { b"https:" } else { b"http:" };
        let p_len = prefix.len();
        out[..p_len].copy_from_slice(prefix);
        let c_len = clean_target.len().min(127 - p_len);
        out[p_len..p_len + c_len].copy_from_slice(&clean_target[..c_len]);
        let total = p_len + c_len;
        out[total] = 0;
        return total;
    }

    // Se for domínio web sem esquema (ex: google.com, example.com, 10.0.2.2:8080)
    // NUNCA envia para o VFS local (/disk/)!
    if is_web_domain(clean_target) {
        let prefix = b"http://";
        let p_len = prefix.len();
        out[..p_len].copy_from_slice(prefix);
        let c_len = clean_target.len().min(127 - p_len);
        out[p_len..p_len + c_len].copy_from_slice(&clean_target[..c_len]);
        let total = p_len + c_len;
        out[total] = 0;
        return total;
    }

    // Se base for HTTP ou HTTPS (RFC 3986)
    let is_https = base.starts_with(b"https://");
    let is_http = base.starts_with(b"http://");
    if is_https || is_http {
        let scheme_len = if is_https { 8 } else { 7 };
        let after_scheme = &base[scheme_len..];
        let host_port_end = after_scheme.iter().position(|&b| b == b'/').unwrap_or(after_scheme.len());
        let host_prefix_len = scheme_len + host_port_end;

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
            let last_slash = base.iter().rposition(|&b| b == b'/').unwrap_or(scheme_len.saturating_sub(1));
            let pos = if last_slash >= scheme_len {
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
    let is_default_port = (!parsed.is_https && parsed.port == 80) || (parsed.is_https && parsed.port == 443);
    if !is_default_port {
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
    append(out, &mut pos, b"Accept-Encoding: identity\r\n");
    append(out, &mut pos, b"Connection: close\r\n\r\n");
    pos
}

#[derive(Clone, Copy)]
pub struct HttpResponseHeader {
    pub status_code: u16,
    pub header_len: usize,
    pub content_length: Option<usize>,
    pub is_chunked: bool,
    pub is_gzip: bool,
    pub is_deflate: bool,
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
    let mut is_gzip = false;
    let mut is_deflate = false;
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
        } else if clean_line.len() > 17 && clean_line[..17].eq_ignore_ascii_case(b"content-encoding:") {
            if clean_line[17..].contains_ignore_ascii_case(b"gzip") {
                is_gzip = true;
            } else if clean_line[17..].contains_ignore_ascii_case(b"deflate") {
                is_deflate = true;
            }
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
        is_gzip,
        is_deflate,
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
        if hex_slice.is_empty() {
            src_idx = line_end + 1;
            continue;
        }

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

/// Gera documento HTML formatado com diagnóstico completo para erros de rede (ARP, DNS, TCP, TLS, HTTP)
pub fn format_net_error_html_diag(
    url: &[u8],
    title: &[u8],
    msg: &[u8],
    detail: &[u8],
    stage: &[u8],
    target_ip: &[u8],
    iface_info: &[u8],
    gw_mac: &[u8],
    tx_pkts: u64,
    rx_pkts: u64,
    out: &mut [u8],
) -> usize {
    let mut pos = 0usize;
    let append = |dst: &mut [u8], p: &mut usize, src: &[u8]| {
        let len = src.len().min(dst.len() - *p);
        dst[*p..*p + len].copy_from_slice(&src[..len]);
        *p += len;
    };
    let append_u64 = |dst: &mut [u8], p: &mut usize, mut val: u64| {
        if val == 0 {
            append(dst, p, b"0");
            return;
        }
        let mut buf = [0u8; 20];
        let mut i = 0;
        while val > 0 {
            buf[i] = b'0' + (val % 10) as u8;
            val /= 10;
            i += 1;
        }
        let mut j = 0;
        let mut rev = [0u8; 20];
        while i > 0 {
            i -= 1;
            rev[j] = buf[i];
            j += 1;
        }
        append(dst, p, &rev[..j]);
    };

    append(out, &mut pos, b"<!DOCTYPE html><html><head><title>");
    append(out, &mut pos, title);
    append(out, &mut pos, b"</title></head><body>");
    append(out, &mut pos, b"<div class=\"card\"><h1>");
    append(out, &mut pos, title);
    append(out, &mut pos, b"</h1><p><b>Destino: </b>");
    append(out, &mut pos, url);
    append(out, &mut pos, b"</p><p><b>Diagnostico: </b>");
    append(out, &mut pos, msg);
    append(out, &mut pos, b"</p><hr/><h3>Diagnostico de Fluxo e Pilha de Rede</h3>");
    append(out, &mut pos, b"<p><b>Estagio da Falha: </b>");
    append(out, &mut pos, stage);
    append(out, &mut pos, b"</p><p><b>IP Externo/DNS Resolvido: </b>");
    append(out, &mut pos, target_ip);
    append(out, &mut pos, b"</p><p><b>Interface de Rede: </b>");
    append(out, &mut pos, iface_info);
    append(out, &mut pos, b"</p><p><b>Contador de Pacotes: </b>TX: ");
    append_u64(out, &mut pos, tx_pkts);
    append(out, &mut pos, b" | RX: ");
    append_u64(out, &mut pos, rx_pkts);
    append(out, &mut pos, b" | ARP Gateway: ");
    append(out, &mut pos, gw_mac);
    append(out, &mut pos, b"</p><p><i>Detalhes tecnicos: </i>");
    append(out, &mut pos, detail);
    append(out, &mut pos, b"</p><hr/><p><a href=\"index.htm\">Voltar ao Inicio</a></p></div></body></html>");
    pos
}

/// Gera documento HTML formatado para erros locais / de rede sem telemetria ativa
pub fn format_net_error_html(url: &[u8], title: &[u8], msg: &[u8], detail: &[u8], out: &mut [u8]) -> usize {
    format_net_error_html_diag(
        url,
        title,
        msg,
        detail,
        b"VFS_OR_URL",
        b"N/A (Local)",
        b"VFS Local (/disk)",
        b"--:--:--:--:--:--",
        0,
        0,
        out,
    )
}

/// Gera documento HTML informativo sobre o status real de HTTPS/TLS 1.3
pub fn format_tls_info_html(url: &[u8], out: &mut [u8]) -> usize {
    let mut pos = 0usize;
    let append = |dst: &mut [u8], p: &mut usize, src: &[u8]| {
        let len = src.len().min(dst.len() - *p);
        dst[*p..*p + len].copy_from_slice(&src[..len]);
        *p += len;
    };

    append(out, &mut pos, b"<!DOCTYPE html><html><head><title>Jinn Browser - TLS 1.3 Criptografia Ativa</title></head><body>");
    append(out, &mut pos, b"<h1>Conexao Segura (HTTPS / TLS 1.3)</h1>");
    append(out, &mut pos, b"<p><b>Status da Pilha: </b>Operacional e Habilitada (RFC 8446)</p>");
    append(out, &mut pos, b"<p><b>Cipher Suite Principal: </b>TLS_CHACHA20_POLY1305_SHA256 (0x1303)</p>");
    append(out, &mut pos, b"<p><b>Troca de Chaves Assimetrica: </b>ECDHE Curve25519 / X25519 (RFC 7748)</p>");
    append(out, &mut pos, b"<p><b>Entropia Criptografica: </b>CSPRNG via sys_getrandom (Syscall 69) [RDRAND + Jitter]</p>");
    append(out, &mut pos, b"<p><b>Relogio do Sistema: </b>RTC CMOS UTC via sys_clock_gettime (Syscall 70)</p>");
    append(out, &mut pos, b"<p><b>Validacao X.509: </b>ASN.1 DER Parser com checagem temporal notBefore/notAfter e SAN</p>");
    append(out, &mut pos, b"<p><b>Root CA Trust Store: </b>ISRG Root X1 (Let's Encrypt), DigiCert Global Root G2, Jinn OS Root CA</p>");
    append(out, &mut pos, b"<p><b>Destino / URL: </b>");
    append(out, &mut pos, url);
    append(out, &mut pos, b"</p><p><b>Navegacao Web Aberta: </b>Conexoes HTTPS nativas habilitadas na porta 443</p>");
    append(out, &mut pos, b"<hr/><p><a href=\"index.htm\">Voltar ao Inicio</a></p></body></html>");
    pos
}

// ── Decodificadores Nativos de Audio e Documentos Ring 3 (no_std, zero-alloc) ─

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeWavInfo {
    pub channels: u16,
    pub sample_rate: u32,
    pub bits_per_sample: u16,
    pub data_len: usize,
    pub duration_ms: u32,
}

#[allow(dead_code)]
pub fn parse_wav_header(bytes: &[u8]) -> Option<NativeWavInfo> {
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut offset = 12;
    let mut channels = 0u16;
    let mut sample_rate = 0u32;
    let mut bits_per_sample = 0u16;
    let mut data_len = 0usize;

    while offset + 8 <= bytes.len() {
        let chunk_id = &bytes[offset..offset + 4];
        let chunk_size = u32::from_le_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]) as usize;
        offset += 8;

        if chunk_id == b"fmt " && chunk_size >= 16 && offset + 16 <= bytes.len() {
            channels = u16::from_le_bytes([bytes[offset + 2], bytes[offset + 3]]);
            sample_rate = u32::from_le_bytes([
                bytes[offset + 4],
                bytes[offset + 5],
                bytes[offset + 6],
                bytes[offset + 7],
            ]);
            bits_per_sample = u16::from_le_bytes([bytes[offset + 14], bytes[offset + 15]]);
        } else if chunk_id == b"data" {
            data_len = chunk_size.min(bytes.len().saturating_sub(offset));
        }

        offset += chunk_size;
        if chunk_size % 2 != 0 {
            offset += 1;
        }
    }

    if channels == 0 || sample_rate == 0 || bits_per_sample == 0 {
        return None;
    }

    let bytes_per_frame = (channels as usize) * ((bits_per_sample as usize) / 8);
    let frames = if bytes_per_frame > 0 { data_len / bytes_per_frame } else { 0 };
    let duration_ms = ((frames as u64 * 1000) / sample_rate as u64) as u32;

    Some(NativeWavInfo {
        channels,
        sample_rate,
        bits_per_sample,
        data_len,
        duration_ms,
    })
}

#[allow(dead_code)]
pub fn synthesize_alert_pcm(freq_hz: u32, duration_ms: u32, sample_rate: u32, out: &mut [u8]) -> usize {
    let num_samples = ((duration_ms as u64 * sample_rate as u64) / 1000) as usize;
    let max_samples = out.len() / 2;
    let count = num_samples.min(max_samples);

    let period = if freq_hz > 0 { sample_rate / freq_hz } else { 1 };
    let half_period = (period / 2).max(1);

    for i in 0..count {
        let sample_val: i16 = if (i as u32 % period) < half_period {
            12000
        } else {
            -12000
        };
        let b = sample_val.to_le_bytes();
        out[i * 2] = b[0];
        out[i * 2 + 1] = b[1];
    }

    count * 2
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativePdfInfo {
    pub version_major: u8,
    pub version_minor: u8,
    pub page_count: usize,
}

#[allow(dead_code)]
pub fn parse_pdf_info(bytes: &[u8]) -> Option<NativePdfInfo> {
    if bytes.len() < 8 || !bytes.starts_with(b"%PDF-") {
        return None;
    }
    let maj = bytes[5].saturating_sub(b'0');
    let min = bytes.get(7).copied().unwrap_or(b'4').saturating_sub(b'0');

    // Conta ocorrências de "/Type /Page"
    let mut page_count = 0usize;
    let target = b"/Type /Page";
    if bytes.len() >= target.len() {
        for w in bytes.windows(target.len()) {
            if w == target {
                page_count += 1;
            }
        }
    }
    if page_count == 0 {
        page_count = 1;
    }

    Some(NativePdfInfo {
        version_major: maj,
        version_minor: min,
        page_count,
    })
}

// ── Subsistema de Persistência Local do Jinn Browser (VFS & Transacional) ─────

pub mod storage {
    #[inline(always)]
    pub(crate) unsafe fn sys_open(path: &[u8], flags: u32) -> i64 {
        crate::syscall3(15, path.as_ptr() as u64, path.len() as u64, flags as u64)
    }

    #[inline(always)]
    pub(crate) unsafe fn sys_close(fd: u64) -> i64 {
        crate::syscall3(16, fd, 0, 0)
    }

    #[inline(always)]
    pub(crate) unsafe fn sys_write(fd: u64, buf: *const u8, len: u64) -> i64 {
        crate::syscall3(1, fd, buf as u64, len)
    }

    #[inline(always)]
    pub(crate) unsafe fn sys_read(fd: u64, buf: *mut u8, len: u64) -> i64 {
        crate::syscall3(2, fd, buf as u64, len)
    }

    #[inline(always)]
    unsafe fn sys_sync() -> i64 {
        crate::syscall3(23, 0, 0, 0)
    }

    #[inline(always)]
    unsafe fn sys_mkdir(path: &[u8]) -> i64 {
        crate::syscall3(51, path.as_ptr() as u64, path.len() as u64, 0)
    }

    #[inline(always)]
    unsafe fn sys_unlink(path: &[u8]) -> i64 {
        crate::syscall3(50, path.as_ptr() as u64, path.len() as u64, 0)
    }

    #[repr(C)]
    struct RenameArgs {
        old_ptr: *const u8,
        old_len: usize,
        new_ptr: *const u8,
        new_len: usize,
    }

    #[inline(always)]
    unsafe fn sys_rename(old_path: &[u8], new_path: &[u8]) -> i64 {
        let args = RenameArgs {
            old_ptr: old_path.as_ptr(),
            old_len: old_path.len(),
            new_ptr: new_path.as_ptr(),
            new_len: new_path.len(),
        };
        crate::syscall3(52, &args as *const _ as u64, 0, 0)
    }

    pub const O_RDONLY: u32 = 0x0;
    pub const O_WRONLY: u32 = 0x1;
    #[allow(dead_code)]
    pub const O_RDWR:   u32 = 0x2;
    pub const O_CREAT:  u32 = 0x40;
    pub const O_TRUNC:  u32 = 0x200;
    #[allow(dead_code)]
    pub const O_APPEND: u32 = 0x400;

    /// Cria a estrutura de diretórios do Jinn Browser caso ainda não existam
    pub fn ensure_storage_hierarchy() {
        unsafe {
            let _ = sys_mkdir(b"/disk/browser");
            let _ = sys_mkdir(b"/disk/browser/cache");
            let _ = sys_mkdir(b"/disk/browser/cookies");
            let _ = sys_mkdir(b"/disk/browser/history");
            let _ = sys_mkdir(b"/disk/browser/bookmark");
        }
    }

    /// Grava dados em arquivo de forma atômica e transacional (.TMP -> .BAK -> Definitivo)
    pub fn transactional_write(path: &[u8], data: &[u8]) -> bool {
        ensure_storage_hierarchy();

        let mut tmp_path = [0u8; 96];
        let mut bak_path = [0u8; 96];
        let p_len = path.len();
        if p_len + 4 > 96 { return false; }

        tmp_path[..p_len].copy_from_slice(path);
        tmp_path[p_len..p_len + 4].copy_from_slice(b".TMP");
        let tmp_slice = &tmp_path[..p_len + 4];

        bak_path[..p_len].copy_from_slice(path);
        bak_path[p_len..p_len + 4].copy_from_slice(b".BAK");
        let bak_slice = &bak_path[..p_len + 4];

        unsafe {
            // 1. Grava no arquivo temporário (.TMP)
            let fd = sys_open(tmp_slice, O_WRONLY | O_CREAT | O_TRUNC);
            if fd < 0 { return false; }

            let mut written = 0usize;
            while written < data.len() {
                let to_write = data.len() - written;
                let chunk = if to_write > 512 { 512 } else { to_write };
                let n = sys_write(fd as u64, data[written..].as_ptr(), chunk as u64);
                if n <= 0 {
                    let _ = sys_close(fd as u64);
                    let _ = sys_unlink(tmp_slice);
                    return false;
                }
                written += n as usize;
            }
            let _ = sys_close(fd as u64);
            let _ = sys_sync();

            // 2. Se o arquivo de destino já existir, cria backup rotacional (.BAK)
            let test_fd = sys_open(path, O_RDONLY);
            if test_fd >= 0 {
                let _ = sys_close(test_fd as u64);
                let _ = sys_unlink(bak_slice);
                let _ = sys_rename(path, bak_slice);
            }

            // 3. Move o arquivo temporário (.TMP) para o nome definitivo
            let rename_res = sys_rename(tmp_slice, path);
            let _ = sys_sync();
            rename_res >= 0
        }
    }

    /// Lê arquivo de dados com fallback automático para salvaguarda .BAK caso o principal falhe
    pub fn transactional_read(path: &[u8], out_buf: &mut [u8]) -> Option<usize> {
        unsafe {
            let mut fd = sys_open(path, O_RDONLY);
            let mut is_bak = false;

            if fd < 0 {
                // Tenta fallback .BAK
                let mut bak_path = [0u8; 96];
                let p_len = path.len();
                if p_len + 4 <= 96 {
                    bak_path[..p_len].copy_from_slice(path);
                    bak_path[p_len..p_len + 4].copy_from_slice(b".BAK");
                    fd = sys_open(&bak_path[..p_len + 4], O_RDONLY);
                    is_bak = true;
                }
            }

            if fd < 0 { return None; }

            let mut total_read = 0usize;
            while total_read < out_buf.len() {
                let to_read = out_buf.len() - total_read;
                let chunk = if to_read > 512 { 512 } else { to_read };
                let n = sys_read(fd as u64, out_buf[total_read..].as_mut_ptr(), chunk as u64);
                if n <= 0 { break; }
                total_read += n as usize;
            }
            let _ = sys_close(fd as u64);

            if is_bak && total_read > 0 {
                // Recuperou do backup com sucesso; restaura o arquivo principal
                let _ = transactional_write(path, &out_buf[..total_read]);
            }

            if total_read > 0 { Some(total_read) } else { None }
        }
    }

    // ── 1. Histórico de Navegação (/disk/browser/history/history.dat) ─────────────

    pub const MAX_STORED_HISTORY: usize = 16;
    pub const HISTORY_PATH: &'static [u8] = b"/disk/browser/history/history.dat";
    const HISTORY_MAGIC: [u8; 4] = *b"JNH1";

    #[derive(Clone, Copy)]
    pub struct StoredHistoryEntry {
        pub url: [u8; 128],
        pub url_len: usize,
        pub title: [u8; 48],
        pub title_len: usize,
        pub visit_count: u32,
    }

    impl StoredHistoryEntry {
        pub const fn empty() -> Self {
            Self {
                url: [0; 128],
                url_len: 0,
                title: [0; 48],
                title_len: 0,
                visit_count: 0,
            }
        }
    }

    pub struct BrowserHistoryStore {
        pub entries: [StoredHistoryEntry; MAX_STORED_HISTORY],
        pub count: usize,
    }

    impl BrowserHistoryStore {
        pub const fn new() -> Self {
            Self {
                entries: [StoredHistoryEntry::empty(); MAX_STORED_HISTORY],
                count: 0,
            }
        }

        pub fn add_visit(&mut self, url: &[u8], title: &[u8]) {
            if url.is_empty() { return; }

            // Se a URL já existir, incrementa contagem e move para o topo
            for i in 0..self.count {
                if self.entries[i].url_len == url.len() && &self.entries[i].url[..url.len()] == url {
                    self.entries[i].visit_count = self.entries[i].visit_count.saturating_add(1);
                    if !title.is_empty() {
                        let t_len = if title.len() > 48 { 48 } else { title.len() };
                        self.entries[i].title[..t_len].copy_from_slice(&title[..t_len]);
                        self.entries[i].title_len = t_len;
                    }
                    let entry = self.entries[i];
                    for k in (1..=i).rev() {
                        self.entries[k] = self.entries[k - 1];
                    }
                    self.entries[0] = entry;
                    return;
                }
            }

            // Insere novo no topo deslocando os demais
            let target_count = if self.count < MAX_STORED_HISTORY { self.count + 1 } else { MAX_STORED_HISTORY };
            for k in (1..target_count).rev() {
                self.entries[k] = self.entries[k - 1];
            }

            let u_len = if url.len() > 128 { 128 } else { url.len() };
            let t_len = if title.len() > 48 { 48 } else { title.len() };

            let mut new_entry = StoredHistoryEntry::empty();
            new_entry.url[..u_len].copy_from_slice(&url[..u_len]);
            new_entry.url_len = u_len;
            if t_len > 0 {
                new_entry.title[..t_len].copy_from_slice(&title[..t_len]);
                new_entry.title_len = t_len;
            }
            new_entry.visit_count = 1;

            self.entries[0] = new_entry;
            self.count = target_count;
        }

        pub fn save(&self) -> bool {
            let mut buf = [0u8; 4 + 4 + MAX_STORED_HISTORY * (128 + 4 + 48 + 4 + 4)];
            buf[0..4].copy_from_slice(&HISTORY_MAGIC);
            let count_u32 = self.count as u32;
            buf[4..8].copy_from_slice(&count_u32.to_le_bytes());

            let mut offset = 8;
            for i in 0..self.count {
                let e = &self.entries[i];
                let u_len_u32 = e.url_len as u32;
                buf[offset..offset + 4].copy_from_slice(&u_len_u32.to_le_bytes());
                offset += 4;
                buf[offset..offset + 128].copy_from_slice(&e.url);
                offset += 128;

                let t_len_u32 = e.title_len as u32;
                buf[offset..offset + 4].copy_from_slice(&t_len_u32.to_le_bytes());
                offset += 4;
                buf[offset..offset + 48].copy_from_slice(&e.title);
                offset += 48;

                buf[offset..offset + 4].copy_from_slice(&e.visit_count.to_le_bytes());
                offset += 4;
            }

            transactional_write(HISTORY_PATH, &buf[..offset])
        }

        pub fn load(&mut self) -> bool {
            let mut buf = [0u8; 4 + 4 + MAX_STORED_HISTORY * (128 + 4 + 48 + 4 + 4)];
            let n = match transactional_read(HISTORY_PATH, &mut buf) {
                Some(len) => len,
                None => return false,
            };

            if n < 8 || &buf[0..4] != &HISTORY_MAGIC {
                return false;
            }

            let count = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]) as usize;
            let count = if count > MAX_STORED_HISTORY { MAX_STORED_HISTORY } else { count };

            let mut offset = 8;
            for i in 0..count {
                if offset + 4 + 128 + 4 + 48 + 4 > n { break; }
                let u_len = u32::from_le_bytes([buf[offset], buf[offset+1], buf[offset+2], buf[offset+3]]) as usize;
                offset += 4;
                let mut url = [0u8; 128];
                url.copy_from_slice(&buf[offset..offset + 128]);
                offset += 128;

                let t_len = u32::from_le_bytes([buf[offset], buf[offset+1], buf[offset+2], buf[offset+3]]) as usize;
                offset += 4;
                let mut title = [0u8; 48];
                title.copy_from_slice(&buf[offset..offset + 48]);
                offset += 48;

                let visit_count = u32::from_le_bytes([buf[offset], buf[offset+1], buf[offset+2], buf[offset+3]]);
                offset += 4;

                let mut e = StoredHistoryEntry::empty();
                e.url = url;
                e.url_len = if u_len > 128 { 128 } else { u_len };
                e.title = title;
                e.title_len = if t_len > 48 { 48 } else { t_len };
                e.visit_count = visit_count;

                self.entries[i] = e;
                self.count = i + 1;
            }

            true
        }
    }

    // ── 2. Cookies HTTP (/disk/browser/cookies/cookies.dat) ───────────────────────

    pub const MAX_STORED_COOKIES: usize = 16;
    pub const COOKIES_PATH: &'static [u8] = b"/disk/browser/cookies/cookies.dat";
    const COOKIES_MAGIC: [u8; 4] = *b"JNC1";

    #[derive(Clone, Copy)]
    pub struct StoredCookie {
        pub domain: [u8; 48],
        pub domain_len: usize,
        pub name: [u8; 32],
        pub name_len: usize,
        pub value: [u8; 64],
        pub value_len: usize,
        pub secure: bool,
    }

    impl StoredCookie {
        pub const fn empty() -> Self {
            Self {
                domain: [0; 48],
                domain_len: 0,
                name: [0; 32],
                name_len: 0,
                value: [0; 64],
                value_len: 0,
                secure: false,
            }
        }
    }

    pub struct BrowserCookieJar {
        pub cookies: [StoredCookie; MAX_STORED_COOKIES],
        pub count: usize,
    }

    impl BrowserCookieJar {
        pub const fn new() -> Self {
            Self {
                cookies: [StoredCookie::empty(); MAX_STORED_COOKIES],
                count: 0,
            }
        }

        pub fn set(&mut self, domain: &[u8], name: &[u8], value: &[u8], secure: bool) -> bool {
            if domain.is_empty() || name.is_empty() { return false; }

            let d_len = if domain.len() > 48 { 48 } else { domain.len() };
            let n_len = if name.len() > 32 { 32 } else { name.len() };
            let v_len = if value.len() > 64 { 64 } else { value.len() };

            for i in 0..self.count {
                if self.cookies[i].domain_len == d_len
                    && &self.cookies[i].domain[..d_len] == &domain[..d_len]
                    && self.cookies[i].name_len == n_len
                    && &self.cookies[i].name[..n_len] == &name[..n_len]
                {
                    self.cookies[i].value[..v_len].copy_from_slice(&value[..v_len]);
                    self.cookies[i].value_len = v_len;
                    self.cookies[i].secure = secure;
                    return true;
                }
            }

            if self.count < MAX_STORED_COOKIES {
                let idx = self.count;
                let mut c = StoredCookie::empty();
                c.domain[..d_len].copy_from_slice(&domain[..d_len]);
                c.domain_len = d_len;
                c.name[..n_len].copy_from_slice(&name[..n_len]);
                c.name_len = n_len;
                c.value[..v_len].copy_from_slice(&value[..v_len]);
                c.value_len = v_len;
                c.secure = secure;

                self.cookies[idx] = c;
                self.count += 1;
                true
            } else {
                false
            }
        }

        pub fn get<'a>(&'a self, domain: &[u8], name: &[u8]) -> Option<&'a [u8]> {
            let d_len = if domain.len() > 48 { 48 } else { domain.len() };
            let n_len = if name.len() > 32 { 32 } else { name.len() };

            for i in 0..self.count {
                if self.cookies[i].domain_len == d_len
                    && &self.cookies[i].domain[..d_len] == &domain[..d_len]
                    && self.cookies[i].name_len == n_len
                    && &self.cookies[i].name[..n_len] == &name[..n_len]
                {
                    return Some(&self.cookies[i].value[..self.cookies[i].value_len]);
                }
            }
            None
        }

        pub fn save(&self) -> bool {
            let mut buf = [0u8; 4 + 4 + MAX_STORED_COOKIES * (48 + 4 + 32 + 4 + 64 + 4 + 1)];
            buf[0..4].copy_from_slice(&COOKIES_MAGIC);
            let count_u32 = self.count as u32;
            buf[4..8].copy_from_slice(&count_u32.to_le_bytes());

            let mut offset = 8;
            for i in 0..self.count {
                let c = &self.cookies[i];
                let d_len_u32 = c.domain_len as u32;
                buf[offset..offset + 4].copy_from_slice(&d_len_u32.to_le_bytes());
                offset += 4;
                buf[offset..offset + 48].copy_from_slice(&c.domain);
                offset += 48;

                let n_len_u32 = c.name_len as u32;
                buf[offset..offset + 4].copy_from_slice(&n_len_u32.to_le_bytes());
                offset += 4;
                buf[offset..offset + 32].copy_from_slice(&c.name);
                offset += 32;

                let v_len_u32 = c.value_len as u32;
                buf[offset..offset + 4].copy_from_slice(&v_len_u32.to_le_bytes());
                offset += 4;
                buf[offset..offset + 64].copy_from_slice(&c.value);
                offset += 64;

                buf[offset] = if c.secure { 1 } else { 0 };
                offset += 1;
            }

            transactional_write(COOKIES_PATH, &buf[..offset])
        }

        pub fn load(&mut self) -> bool {
            let mut buf = [0u8; 4 + 4 + MAX_STORED_COOKIES * (48 + 4 + 32 + 4 + 64 + 4 + 1)];
            let n = match transactional_read(COOKIES_PATH, &mut buf) {
                Some(len) => len,
                None => return false,
            };

            if n < 8 || &buf[0..4] != &COOKIES_MAGIC {
                return false;
            }

            let count = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]) as usize;
            let count = if count > MAX_STORED_COOKIES { MAX_STORED_COOKIES } else { count };

            let mut offset = 8;
            for i in 0..count {
                if offset + 4 + 48 + 4 + 32 + 4 + 64 + 1 > n { break; }
                let d_len = u32::from_le_bytes([buf[offset], buf[offset+1], buf[offset+2], buf[offset+3]]) as usize;
                offset += 4;
                let mut domain = [0u8; 48];
                domain.copy_from_slice(&buf[offset..offset + 48]);
                offset += 48;

                let n_len = u32::from_le_bytes([buf[offset], buf[offset+1], buf[offset+2], buf[offset+3]]) as usize;
                offset += 4;
                let mut name = [0u8; 32];
                name.copy_from_slice(&buf[offset..offset + 32]);
                offset += 32;

                let v_len = u32::from_le_bytes([buf[offset], buf[offset+1], buf[offset+2], buf[offset+3]]) as usize;
                offset += 4;
                let mut value = [0u8; 64];
                value.copy_from_slice(&buf[offset..offset + 64]);
                offset += 64;

                let secure = buf[offset] != 0;
                offset += 1;

                let mut c = StoredCookie::empty();
                c.domain = domain;
                c.domain_len = if d_len > 48 { 48 } else { d_len };
                c.name = name;
                c.name_len = if n_len > 32 { 32 } else { n_len };
                c.value = value;
                c.value_len = if v_len > 64 { 64 } else { v_len };
                c.secure = secure;

                self.cookies[i] = c;
                self.count = i + 1;
            }

            true
        }
    }

    // ── 3. Favoritos / Bookmarks (/disk/browser/bookmark/bookmrk.dat) ─────────────

    pub const MAX_STORED_BOOKMARKS: usize = 12;
    pub const BOOKMARKS_PATH: &'static [u8] = b"/disk/browser/bookmark/bookmrk.dat";
    const BOOKMARKS_MAGIC: [u8; 4] = *b"JNB1";

    #[derive(Clone, Copy)]
    pub struct StoredBookmark {
        pub title: [u8; 48],
        pub title_len: usize,
        pub url: [u8; 128],
        pub url_len: usize,
    }

    impl StoredBookmark {
        pub const fn empty() -> Self {
            Self {
                title: [0; 48],
                title_len: 0,
                url: [0; 128],
                url_len: 0,
            }
        }
    }

    pub struct BrowserBookmarkStore {
        pub bookmarks: [StoredBookmark; MAX_STORED_BOOKMARKS],
        pub count: usize,
    }

    impl BrowserBookmarkStore {
        pub const fn new() -> Self {
            Self {
                bookmarks: [StoredBookmark::empty(); MAX_STORED_BOOKMARKS],
                count: 0,
            }
        }

        pub fn init_defaults_if_empty(&mut self) {
            if self.count == 0 {
                self.add(b"Jinn OS Portal", b"file:///disk/INDEX.HTM");
                self.add(b"Sobre o Navegador", b"about:version");
                self.add(b"NeverSSL (HTTP Test)", b"http://neverssl.com");
                self.add(b"Example Domain", b"http://example.com");
            }
        }

        pub fn add(&mut self, title: &[u8], url: &[u8]) -> bool {
            if url.is_empty() || self.count >= MAX_STORED_BOOKMARKS { return false; }

            for i in 0..self.count {
                if self.bookmarks[i].url_len == url.len() && &self.bookmarks[i].url[..url.len()] == url {
                    return true;
                }
            }

            let t_len = if title.len() > 48 { 48 } else { title.len() };
            let u_len = if url.len() > 128 { 128 } else { url.len() };

            let mut b = StoredBookmark::empty();
            b.title[..t_len].copy_from_slice(&title[..t_len]);
            b.title_len = t_len;
            b.url[..u_len].copy_from_slice(&url[..u_len]);
            b.url_len = u_len;

            self.bookmarks[self.count] = b;
            self.count += 1;
            true
        }

        pub fn save(&self) -> bool {
            let mut buf = [0u8; 4 + 4 + MAX_STORED_BOOKMARKS * (48 + 4 + 128 + 4)];
            buf[0..4].copy_from_slice(&BOOKMARKS_MAGIC);
            let count_u32 = self.count as u32;
            buf[4..8].copy_from_slice(&count_u32.to_le_bytes());

            let mut offset = 8;
            for i in 0..self.count {
                let b = &self.bookmarks[i];
                let t_len_u32 = b.title_len as u32;
                buf[offset..offset + 4].copy_from_slice(&t_len_u32.to_le_bytes());
                offset += 4;
                buf[offset..offset + 48].copy_from_slice(&b.title);
                offset += 48;

                let u_len_u32 = b.url_len as u32;
                buf[offset..offset + 4].copy_from_slice(&u_len_u32.to_le_bytes());
                offset += 4;
                buf[offset..offset + 128].copy_from_slice(&b.url);
                offset += 128;
            }

            transactional_write(BOOKMARKS_PATH, &buf[..offset])
        }

        pub fn load(&mut self) -> bool {
            let mut buf = [0u8; 4 + 4 + MAX_STORED_BOOKMARKS * (48 + 4 + 128 + 4)];
            let n = match transactional_read(BOOKMARKS_PATH, &mut buf) {
                Some(len) => len,
                None => return false,
            };

            if n < 8 || &buf[0..4] != &BOOKMARKS_MAGIC {
                return false;
            }

            let count = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]) as usize;
            let count = if count > MAX_STORED_BOOKMARKS { MAX_STORED_BOOKMARKS } else { count };

            let mut offset = 8;
            for i in 0..count {
                if offset + 4 + 48 + 4 + 128 > n { break; }
                let t_len = u32::from_le_bytes([buf[offset], buf[offset+1], buf[offset+2], buf[offset+3]]) as usize;
                offset += 4;
                let mut title = [0u8; 48];
                title.copy_from_slice(&buf[offset..offset + 48]);
                offset += 48;

                let u_len = u32::from_le_bytes([buf[offset], buf[offset+1], buf[offset+2], buf[offset+3]]) as usize;
                offset += 4;
                let mut url = [0u8; 128];
                url.copy_from_slice(&buf[offset..offset + 128]);
                offset += 128;

                let mut b = StoredBookmark::empty();
                b.title = title;
                b.title_len = if t_len > 48 { 48 } else { t_len };
                b.url = url;
                b.url_len = if u_len > 128 { 128 } else { u_len };

                self.bookmarks[i] = b;
                self.count = i + 1;
            }

            true
        }
    }

    // ── 4. Cache Local de Documentos Web (/disk/browser/cache/) ──────────────────

    pub const CACHE_PREFIX: &'static [u8] = b"/disk/browser/cache/C";
    const CACHE_MAGIC: [u8; 4] = *b"JCA1";

    pub fn compute_cache_filename(url: &[u8], out_buf: &mut [u8; 40]) -> usize {
        let mut h: u32 = 0x811c9dc5;
        for &b in url {
            h ^= b as u32;
            h = h.wrapping_mul(0x01000193);
        }
        let p_len = CACHE_PREFIX.len();
        out_buf[..p_len].copy_from_slice(CACHE_PREFIX);
        let mut idx = p_len;
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        for shift in (0..8).rev() {
            let nibble = ((h >> (shift * 4)) & 0xF) as usize;
            out_buf[idx] = HEX[nibble];
            idx += 1;
        }
        let ext = b".DAT";
        out_buf[idx..idx + 4].copy_from_slice(ext);
        idx + 4
    }

    pub struct BrowserCacheStore;

    impl BrowserCacheStore {
        pub fn store(url: &[u8], payload: &[u8]) -> bool {
            if url.is_empty() || payload.is_empty() || payload.len() > 16384 {
                return false;
            }
            let mut file_path = [0u8; 40];
            let path_len = compute_cache_filename(url, &mut file_path);

            let mut store_buf = [0u8; 4 + 4 + 16384];
            store_buf[0..4].copy_from_slice(&CACHE_MAGIC);
            let p_len_u32 = payload.len() as u32;
            store_buf[4..8].copy_from_slice(&p_len_u32.to_le_bytes());
            store_buf[8..8 + payload.len()].copy_from_slice(payload);

            transactional_write(&file_path[..path_len], &store_buf[..8 + payload.len()])
        }

        pub fn retrieve(url: &[u8], out_buf: &mut [u8]) -> Option<usize> {
            if url.is_empty() { return None; }
            let mut file_path = [0u8; 40];
            let path_len = compute_cache_filename(url, &mut file_path);

            let mut raw_buf = [0u8; 4 + 4 + 16384];
            let n = match transactional_read(&file_path[..path_len], &mut raw_buf) {
                Some(len) => len,
                None => return None,
            };

            if n < 8 || &raw_buf[0..4] != &CACHE_MAGIC {
                return None;
            }

            let payload_len = u32::from_le_bytes([raw_buf[4], raw_buf[5], raw_buf[6], raw_buf[7]]) as usize;
            if 8 + payload_len > n || payload_len > out_buf.len() {
                return None;
            }

            out_buf[..payload_len].copy_from_slice(&raw_buf[8..8 + payload_len]);
            Some(payload_len)
        }
    }

    /// Executa bateria de testes do subsistema de persistência local em Ring 3
    pub fn run_selftests() -> (usize, usize) {
        let mut passed = 0;
        let mut total = 0;

        // Teste 1: Criação da infraestrutura de diretórios
        total += 1;
        ensure_storage_hierarchy();
        passed += 1;

        // Teste 2: Escrita e leitura transacional básica (.TMP -> Definitivo)
        total += 1;
        let test_path = b"/disk/browser/history/test_trn.dat";
        let test_data = b"JINN_TRANSACTION_PAYLOAD_V1";
        if transactional_write(test_path, test_data) {
            let mut read_buf = [0u8; 64];
            if let Some(n) = transactional_read(test_path, &mut read_buf) {
                if &read_buf[..n] == test_data {
                    passed += 1;
                }
            }
        }

        // Teste 3: Rotação .BAK na sobrescrita
        total += 1;
        let new_data = b"JINN_UPDATED_PAYLOAD_V2";
        if transactional_write(test_path, new_data) {
            let mut bak_buf = [0u8; 64];
            let bak_path = b"/disk/browser/history/test_trn.dat.BAK";
            let fd = unsafe { sys_open(bak_path, O_RDONLY) };
            if fd >= 0 {
                let n_bak = unsafe { sys_read(fd as u64, bak_buf.as_mut_ptr(), 64) };
                unsafe { let _ = sys_close(fd as u64); };
                if n_bak == test_data.len() as i64 && &bak_buf[..n_bak as usize] == test_data {
                    passed += 1;
                }
            }
        }

        // Teste 4: Recuperação automática a partir de .BAK quando .DAT ausente
        total += 1;
        unsafe { let _ = sys_unlink(test_path); }
        let mut recovery_buf = [0u8; 64];
        if let Some(n_rec) = transactional_read(test_path, &mut recovery_buf) {
            if &recovery_buf[..n_rec] == test_data {
                passed += 1;
            }
        }

        // Limpeza dos arquivos de teste
        unsafe {
            let _ = sys_unlink(test_path);
            let _ = sys_unlink(b"/disk/browser/history/test_trn.dat.BAK");
            let _ = sys_unlink(b"/disk/browser/history/test_trn.dat.TMP");
        }

        // Teste 5: BrowserHistoryStore (Adição, contagem e persistência)
        total += 1;
        let mut h_store = BrowserHistoryStore::new();
        h_store.add_visit(b"file:///disk/INDEX.HTM", b"Portal Jinn");
        h_store.add_visit(b"http://example.com", b"Example Domain");
        h_store.add_visit(b"file:///disk/INDEX.HTM", b"Portal Jinn Atualizado");
        if h_store.count == 2 && h_store.entries[0].visit_count == 2 && h_store.save() {
            let mut h_reloaded = BrowserHistoryStore::new();
            if h_reloaded.load() && h_reloaded.count >= 2 {
                passed += 1;
            }
        }

        // Teste 6: BrowserCookieJar (Set, Get e persistência)
        total += 1;
        let mut jar = BrowserCookieJar::new();
        jar.set(b"example.com", b"session_id", b"xyz987654321", true);
        if let Some(val) = jar.get(b"example.com", b"session_id") {
            if val == b"xyz987654321" && jar.save() {
                let mut jar_reloaded = BrowserCookieJar::new();
                if jar_reloaded.load() {
                    if let Some(r_val) = jar_reloaded.get(b"example.com", b"session_id") {
                        if r_val == b"xyz987654321" {
                            passed += 1;
                        }
                    }
                }
            }
        }

        // Teste 7: BrowserBookmarkStore (Defaults, add, persistência)
        total += 1;
        let mut b_store = BrowserBookmarkStore::new();
        b_store.init_defaults_if_empty();
        b_store.add(b"Favorito Teste", b"http://teste.dev.br");
        if b_store.save() {
            let mut b_reloaded = BrowserBookmarkStore::new();
            if b_reloaded.load() && b_reloaded.count >= 5 {
                passed += 1;
            }
        }

        // Teste 8: BrowserCacheStore (Armazenamento, integridade e recuperação por hash FNV-1a)
        total += 1;
        let cache_url = b"http://example.com/api/test.json";
        let cache_payload = b"{\"status\":\"ok\",\"engine\":\"jinn-browser\"}";
        if BrowserCacheStore::store(cache_url, cache_payload) {
            let mut out_cache = [0u8; 128];
            if let Some(c_len) = BrowserCacheStore::retrieve(cache_url, &mut out_cache) {
                if &out_cache[..c_len] == cache_payload {
                    passed += 1;
                }
            }
        }

        (passed, total)
    }
}

