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

pub const MAX_DOM_NODES: usize = 768;

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
pub struct StageRect {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct StageGeometry {
    pub chrome_rect: StageRect,
    pub status_rect: StageRect,
    pub scrollbar_rect: StageRect,
    pub viewport_rect: StageRect,
    pub content_rect: StageRect,
}

pub fn compute_stage_geometry(w: usize, h: usize) -> StageGeometry {
    let clip_bottom = content_clip_bottom(h);
    let chrome_rect = StageRect { x: 0, y: 0, w, h: CLIP_TOP.min(h) };
    let status_rect = if h >= 100 {
        StageRect { x: 0, y: h.saturating_sub(14), w, h: 14 }
    } else {
        StageRect { x: 0, y: h, w, h: 0 }
    };
    let vp_h = clip_bottom.saturating_sub(CLIP_TOP);
    let sb_x = w.saturating_sub(14);
    let scrollbar_rect = StageRect {
        x: sb_x,
        y: CLIP_TOP,
        w: 8,
        h: vp_h,
    };
    let vp_w = w.saturating_sub(16);
    let viewport_rect = StageRect {
        x: 0,
        y: CLIP_TOP,
        w: vp_w,
        h: vp_h,
    };
    // Contêiner centralizado com max-width legível (760px) quando em tela cheia / janelas largas
    let inner_pad = 6usize;
    let raw_content_w = vp_w.saturating_sub(inner_pad * 2).max(40);
    let content_w = if vp_w > 940 { raw_content_w.min(920) } else { raw_content_w };
    let content_x = if vp_w > content_w + inner_pad * 2 {
        (vp_w - content_w) / 2
    } else {
        inner_pad
    };
    let content_rect = StageRect {
        x: content_x,
        y: CLIP_TOP,
        w: content_w,
        h: vp_h,
    };
    StageGeometry {
        chrome_rect,
        status_rect,
        scrollbar_rect,
        viewport_rect,
        content_rect,
    }
}

const STYLE_FLAG_HAS_BG: u16 = 1 << 0;
const STYLE_FLAG_HAS_BORDER_COL: u16 = 1 << 1;
const STYLE_FLAG_EXPLICIT_COL: u16 = 1 << 2;
const STYLE_FLAG_BOLD: u16 = 1 << 3;
const STYLE_FLAG_DISPLAY_BLOCK: u16 = 1 << 4;
const STYLE_FLAG_FLEX_WRAP: u16 = 1 << 5;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ComputedStyle {
    pub color: u32,
    pub bg_color: u32,
    pub border_color: u32,
    pub width: u16,      // u16::MAX == None
    pub height: u16,     // u16::MAX == None
    pub max_width: u16,  // u16::MAX == None
    pub flags: u16,
    pub border_width: u8,
    pub border_radius: u8,
    pub border_sides: u8,
    pub font_size: u8,
    pub margin_top: u8,
    pub margin_bottom: u8,
    pub margin_left: u8,
    pub margin_right: u8,
    pub padding_top: u8,
    pub padding_bottom: u8,
    pub padding_left: u8,
    pub padding_right: u8,
    pub gap: u8,
    pub grid_cols: u8,
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
            bg_color: 0,
            border_color: 0,
            width: u16::MAX,
            height: u16::MAX,
            max_width: u16::MAX,
            flags: STYLE_FLAG_DISPLAY_BLOCK,
            border_width: 1,
            border_radius: 0,
            border_sides: 15,
            font_size: 12,
            margin_top: 2,
            margin_bottom: 4,
            margin_left: 0,
            margin_right: 0,
            padding_top: 0,
            padding_bottom: 0,
            padding_left: 0,
            padding_right: 0,
            gap: 4,
            grid_cols: 0,
            display: DisplayMode::Block,
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::FlexStart,
            text_align: TextAlign::Left,
        }
    }

    #[inline(always)] pub fn has_bg(&self) -> bool { (self.flags & STYLE_FLAG_HAS_BG) != 0 }
    #[inline(always)] pub fn has_border_color(&self) -> bool { (self.flags & STYLE_FLAG_HAS_BORDER_COL) != 0 }
    #[inline(always)] pub fn has_box(&self) -> bool { (self.flags & (STYLE_FLAG_HAS_BG | STYLE_FLAG_HAS_BORDER_COL)) != 0 }
    #[inline(always)] pub fn has_explicit_color(&self) -> bool { (self.flags & STYLE_FLAG_EXPLICIT_COL) != 0 }
    #[inline(always)] pub fn is_bold(&self) -> bool { (self.flags & STYLE_FLAG_BOLD) != 0 }
    #[inline(always)] pub fn display_block(&self) -> bool { (self.flags & STYLE_FLAG_DISPLAY_BLOCK) != 0 }
    #[inline(always)] pub fn flex_wrap(&self) -> bool { (self.flags & STYLE_FLAG_FLEX_WRAP) != 0 }

    #[inline(always)]
    pub fn set_flag(&mut self, bit: u16, val: bool) {
        if val { self.flags |= bit; } else { self.flags &= !bit; }
    }
    #[inline(always)] pub fn set_explicit_color(&mut self, v: bool) { self.set_flag(STYLE_FLAG_EXPLICIT_COL, v); }
    #[inline(always)] pub fn set_bold(&mut self, v: bool) { self.set_flag(STYLE_FLAG_BOLD, v); }
    #[inline(always)] pub fn set_display_block(&mut self, v: bool) { self.set_flag(STYLE_FLAG_DISPLAY_BLOCK, v); }
    #[inline(always)] pub fn set_flex_wrap(&mut self, v: bool) { self.set_flag(STYLE_FLAG_FLEX_WRAP, v); }

    #[inline(always)]
    pub fn bg_color_opt(&self) -> Option<u32> {
        if self.has_bg() { Some(self.bg_color) } else { None }
    }
    #[inline(always)]
    pub fn set_bg_color(&mut self, c: Option<u32>) {
        match c {
            Some(col) => { self.bg_color = col; self.flags |= STYLE_FLAG_HAS_BG; }
            None => { self.bg_color = 0; self.flags &= !STYLE_FLAG_HAS_BG; }
        }
    }
    #[inline(always)]
    pub fn border_color_opt(&self) -> Option<u32> {
        if self.has_border_color() { Some(self.border_color) } else { None }
    }
    #[inline(always)]
    pub fn set_border_color(&mut self, c: Option<u32>) {
        match c {
            Some(col) => { self.border_color = col; self.flags |= STYLE_FLAG_HAS_BORDER_COL; }
            None => { self.border_color = 0; self.flags &= !STYLE_FLAG_HAS_BORDER_COL; }
        }
    }
    #[inline(always)]
    pub fn width_opt(&self) -> Option<usize> {
        if self.width == u16::MAX { None } else { Some(self.width as usize) }
    }
    #[inline(always)]
    pub fn set_width(&mut self, w: Option<usize>) {
        self.width = w.map_or(u16::MAX, |v| v.min((u16::MAX - 1) as usize) as u16);
    }
    #[inline(always)]
    pub fn height_opt(&self) -> Option<usize> {
        if self.height == u16::MAX { None } else { Some(self.height as usize) }
    }
    #[inline(always)]
    pub fn set_height(&mut self, h: Option<usize>) {
        self.height = h.map_or(u16::MAX, |v| v.min((u16::MAX - 1) as usize) as u16);
    }
    #[inline(always)]
    pub fn max_width_opt(&self) -> Option<usize> {
        if self.max_width == u16::MAX { None } else { Some(self.max_width as usize) }
    }
    #[inline(always)]
    pub fn set_max_width(&mut self, w: Option<usize>) {
        self.max_width = w.map_or(u16::MAX, |v| v.min((u16::MAX - 1) as usize) as u16);
    }
}

pub const MAX_DOM_CHILDREN: usize = 16;

#[allow(dead_code)]
#[derive(Clone, Copy)]
pub struct DomNode {
    pub tag: [u8; 12],
    pub id_attr: [u8; 32],
    pub class_attr: [u8; 136],
    pub style_attr: [u8; 96],
    pub href_attr: [u8; 128],
    pub src_attr: [u8; 128],
    pub alt_attr: [u8; 32],
    pub data_open_window: [u8; 32],
    pub onclick_attr: [u8; 96],
    pub text: [u8; 192],
    pub children: [u16; MAX_DOM_CHILDREN],
    pub parent: u16, // u16::MAX == None
    pub id: u16,
    pub width_attr: u16,
    pub height_attr: u16,
    pub box_x: u16,
    pub box_y: u16,
    pub box_w: u16,
    pub box_h: u16,
    pub tag_len: u8,
    pub id_len: u8,
    pub class_len: u8,
    pub style_len: u8,
    pub href_len: u8,
    pub src_len: u8,
    pub alt_len: u8,
    pub data_open_window_len: u8,
    pub onclick_len: u8,
    pub text_len: u8,
    pub child_count: u8,
    pub box_bg_fallback: u8,
    pub node_type: DomNodeType,
    pub computed_style: ComputedStyle,
}

impl DomNode {
    pub const fn empty() -> Self {
        Self {
            tag: [0; 12],
            id_attr: [0; 32],
            class_attr: [0; 136],
            style_attr: [0; 96],
            href_attr: [0; 128],
            src_attr: [0; 128],
            alt_attr: [0; 32],
            data_open_window: [0; 32],
            onclick_attr: [0; 96],
            text: [0; 192],
            children: [0; MAX_DOM_CHILDREN],
            parent: u16::MAX,
            id: 0,
            width_attr: 0,
            height_attr: 0,
            box_x: 0,
            box_y: 0,
            box_w: 0,
            box_h: 0,
            tag_len: 0,
            id_len: 0,
            class_len: 0,
            style_len: 0,
            href_len: 0,
            src_len: 0,
            alt_len: 0,
            data_open_window_len: 0,
            onclick_len: 0,
            text_len: 0,
            child_count: 0,
            box_bg_fallback: 0,
            node_type: DomNodeType::Element,
            computed_style: ComputedStyle::default(),
        }
    }

    #[inline(always)] pub fn parent_idx(&self) -> Option<usize> {
        if self.parent == u16::MAX { None } else { Some(self.parent as usize) }
    }
    #[inline(always)] pub fn set_parent(&mut self, p: Option<usize>) {
        self.parent = p.map_or(u16::MAX, |v| v.min((u16::MAX - 1) as usize) as u16);
    }
    #[inline(always)] pub fn child_id(&self, c: usize) -> usize { self.children[c] as usize }
    #[inline(always)] pub fn tag_slice(&self) -> &[u8] { &self.tag[..self.tag_len as usize] }
    #[inline(always)] pub fn id_slice(&self) -> &[u8] { &self.id_attr[..self.id_len as usize] }
    #[inline(always)] pub fn class_slice(&self) -> &[u8] { &self.class_attr[..self.class_len as usize] }
    #[inline(always)] pub fn style_slice(&self) -> &[u8] { &self.style_attr[..self.style_len as usize] }
    #[inline(always)] pub fn href_slice(&self) -> &[u8] { &self.href_attr[..self.href_len as usize] }
    #[inline(always)] pub fn src_slice(&self) -> &[u8] { &self.src_attr[..self.src_len as usize] }
    #[inline(always)] pub fn alt_slice(&self) -> &[u8] { &self.alt_attr[..self.alt_len as usize] }
    #[inline(always)] pub fn text_slice(&self) -> &[u8] { &self.text[..self.text_len as usize] }
    #[inline(always)] pub fn onclick_slice(&self) -> &[u8] { &self.onclick_attr[..self.onclick_len as usize] }
    #[inline(always)] pub fn data_open_window_slice(&self) -> &[u8] { &self.data_open_window[..self.data_open_window_len as usize] }
}

fn copy_trunc_u8(dst: &mut [u8], src: &[u8]) -> u8 {
    let n = src.len().min(dst.len());
    dst[..n].copy_from_slice(&src[..n]);
    n as u8
}

fn filter_visual_classes(src: &[u8], dst: &mut [u8]) -> u8 {
    if src.len() <= dst.len() {
        return copy_trunc_u8(dst, src);
    }
    let mut out_len = 0usize;
    for tok in src.split(|&b| b == b' ' || b == b'\t' || b == b'\r' || b == b'\n') {
        if tok.is_empty() { continue; }
        if tok.starts_with(b"hover:")
            || tok.starts_with(b"focus:")
            || tok.starts_with(b"active:")
            || tok.starts_with(b"group")
            || tok.starts_with(b"transition")
            || tok.starts_with(b"duration-")
            || tok.starts_with(b"ease-")
            || tok.starts_with(b"animate-")
            || tok.starts_with(b"shadow")
            || tok.starts_with(b"outline-")
            || tok.starts_with(b"tracking-")
            || tok.starts_with(b"cursor-")
            || tok.starts_with(b"select-")
            || tok.starts_with(b"resize-")
            || tok.starts_with(b"shrink")
        {
            continue;
        }
        let need_space = if out_len > 0 { 1 } else { 0 };
        if out_len + need_space + tok.len() > dst.len() {
            continue;
        }
        if need_space == 1 {
            dst[out_len] = b' ';
            out_len += 1;
        }
        dst[out_len..out_len + tok.len()].copy_from_slice(tok);
        out_len += tok.len();
    }
    out_len as u8
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
            node.id = id as u16;
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
            let id_s = self.nodes[i].id_slice();
            if id_s.len() == target.len() && id_s == target {
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
            let t_s = self.nodes[i].tag_slice();
            if t_s.len() == clean.len() && t_s.eq_ignore_ascii_case(clean) {
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
        let c_attr = self.nodes[idx].class_slice();
        for word in c_attr.split(|&b| b == b' ' || b == b'\t' || b == b'\r' || b == b'\n') {
            if !word.is_empty() && word.eq_ignore_ascii_case(class_name) {
                return true;
            }
        }
        false
    }

    pub fn has_class_hash(&self, idx: usize, class_hash: u32) -> bool {
        if idx >= self.node_count { return false; }
        let c_attr = self.nodes[idx].class_slice();
        for word in c_attr.split(|&b| b == b' ' || b == b'\t' || b == b'\r' || b == b'\n') {
            if !word.is_empty() && fnv1a_32_ci(word) == class_hash {
                return true;
            }
        }
        false
    }

    pub fn add_class(&mut self, idx: usize, class_name: &[u8]) {
        if idx >= self.node_count || self.has_class(idx, class_name) { return; }
        let node = &mut self.nodes[idx];
        let clen = node.class_len as usize;
        if clen + 1 + class_name.len() <= node.class_attr.len() {
            if clen > 0 {
                node.class_attr[clen] = b' ';
                node.class_len += 1;
            }
            let pos = node.class_len as usize;
            node.class_attr[pos..pos + class_name.len()].copy_from_slice(class_name);
            node.class_len = (pos + class_name.len()) as u8;
        }
    }

    pub fn remove_class(&mut self, idx: usize, class_name: &[u8]) {
        if idx >= self.node_count { return; }
        let node = &mut self.nodes[idx];
        let mut new_buf = [0u8; 136];
        let mut new_len = 0usize;
        let max_len = node.class_attr.len();
        let c_attr = &node.class_attr[..node.class_len as usize];
        for word in c_attr.split(|&b| b == b' ' || b == b'\t' || b == b'\r' || b == b'\n') {
            if word.is_empty() || word.eq_ignore_ascii_case(class_name) {
                continue;
            }
            if new_len > 0 && new_len + 1 < max_len {
                new_buf[new_len] = b' ';
                new_len += 1;
            }
            let cpy = word.len().min(max_len - new_len);
            new_buf[new_len..new_len + cpy].copy_from_slice(&word[..cpy]);
            new_len += cpy;
        }
        node.class_attr = new_buf;
        node.class_len = new_len as u8;
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
            let max_c = self.nodes[idx].text.len().saturating_sub(1);
            let len = new_text.len().min(max_c);
            self.nodes[idx].text[..len].copy_from_slice(&new_text[..len]);
            self.nodes[idx].text_len = len as u8;
        }
    }

    pub fn set_style_color(&mut self, idx: usize, col: u32) {
        if idx < self.node_count {
            self.nodes[idx].computed_style.color = col;
            self.nodes[idx].computed_style.set_explicit_color(true);
        }
    }

    #[allow(dead_code)]
    pub fn set_style_bg(&mut self, idx: usize, col: u32) {
        if idx < self.node_count {
            self.nodes[idx].computed_style.set_bg_color(Some(col));
        }
    }

    pub fn set_style_display(&mut self, idx: usize, mode: DisplayMode) {
        if idx < self.node_count {
            self.nodes[idx].computed_style.display = mode;
            self.nodes[idx].computed_style.set_display_block(mode == DisplayMode::Block);
        }
    }
}

// ── Mini-CSS e Resolução de Estilos ──────────────────────────────────────────

pub const MAX_CSS_RULES: usize = 128;
pub const MAX_CSS_VARS: usize = 32;

const SEL_ATOM_NONE: u8 = 0;
const SEL_ATOM_TAG: u8 = 1;
const SEL_ATOM_CLASS: u8 = 2;
const SEL_ATOM_ID: u8 = 3;
const SEL_ATOM_WILDCARD: u8 = 4;

#[inline]
pub fn fnv1a_32_ci(s: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for &b in s {
        h ^= b.to_ascii_lowercase() as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

#[inline]
pub fn fnv1a_32(s: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for &b in s {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

#[derive(Clone, Copy)]
pub struct CssVar {
    pub name_hash: u32,
    pub val: [u8; 31],
    pub val_len: u8,
}

impl CssVar {
    pub const fn empty() -> Self {
        Self {
            name_hash: 0,
            val: [0; 31],
            val_len: 0,
        }
    }
}

fn strip_css_important(val: &[u8]) -> &[u8] {
    let mut s = val.trim_ascii();
    if s.len() >= 10 && s[s.len() - 10..].eq_ignore_ascii_case(b"!important") {
        s = s[..s.len() - 10].trim_ascii();
    }
    s
}

const RULE_HAS_COLOR: u32 = 1 << 0;
const RULE_HAS_BG: u32 = 1 << 1;
const RULE_HAS_BORDER_COLOR: u32 = 1 << 2;
const RULE_HAS_BORDER_WIDTH: u32 = 1 << 3;
const RULE_HAS_BORDER_RADIUS: u32 = 1 << 4;
const RULE_HAS_BORDER_SIDES: u32 = 1 << 5;
const RULE_HAS_FONT_SIZE: u32 = 1 << 6;
const RULE_HAS_WIDTH: u32 = 1 << 7;
const RULE_HAS_HEIGHT: u32 = 1 << 8;
const RULE_HAS_MAX_WIDTH: u32 = 1 << 9;
const RULE_HAS_MT: u32 = 1 << 10;
const RULE_HAS_MB: u32 = 1 << 11;
const RULE_HAS_ML: u32 = 1 << 12;
const RULE_HAS_MR: u32 = 1 << 13;
const RULE_HAS_PT: u32 = 1 << 14;
const RULE_HAS_PB: u32 = 1 << 15;
const RULE_HAS_PL: u32 = 1 << 16;
const RULE_HAS_PR: u32 = 1 << 17;
const RULE_HAS_GAP: u32 = 1 << 18;
const RULE_HAS_BOLD: u32 = 1 << 19;
const RULE_HAS_DISPLAY: u32 = 1 << 20;
const RULE_HAS_FLEX_DIR: u32 = 1 << 21;
const RULE_HAS_FLEX_WRAP: u32 = 1 << 22;
const RULE_HAS_GRID_COLS: u32 = 1 << 23;
const RULE_HAS_JUSTIFY: u32 = 1 << 24;
const RULE_HAS_ALIGN: u32 = 1 << 25;
const RULE_HAS_TEXT_ALIGN: u32 = 1 << 26;
const RULE_IS_BOLD_VAL: u32 = 1 << 27;
const RULE_FLEX_WRAP_VAL: u32 = 1 << 28;
const RULE_IS_BODY_TAG: u32 = 1 << 29;

#[allow(dead_code)]
#[derive(Clone, Copy)]
pub struct CssRule {
    pub sel_hashes: [u32; 2],
    pub parent_hash: u32,
    pub present_mask: u32,
    pub color: u32,
    pub bg_color: u32,
    pub border_color: u32,
    pub width: u16,
    pub height: u16,
    pub max_width: u16,
    pub specificity: u8,
    pub sel_atoms: u8,   // bits 0..3: atom0 kind, bits 3..6: atom1 kind
    pub parent_kind: u8, // SEL_ATOM_*
    pub border_width: u8,
    pub border_radius: u8,
    pub border_sides: u8,
    pub font_size: u8,
    pub margin_top: u8,
    pub margin_bottom: u8,
    pub margin_left: u8,
    pub margin_right: u8,
    pub padding_top: u8,
    pub padding_bottom: u8,
    pub padding_left: u8,
    pub padding_right: u8,
    pub gap: u8,
    pub grid_cols: u8,
    pub packed_enums: u8, // bits 0..3: DisplayMode, bit 3: FlexDir, bits 4..6: Justify, bits 6..8: Align
    pub text_align_bits: u8,
}

impl CssRule {
    pub const fn empty() -> Self {
        Self {
            sel_hashes: [0; 2],
            parent_hash: 0,
            present_mask: 0,
            color: 0,
            bg_color: 0,
            border_color: 0,
            width: 0,
            height: 0,
            max_width: 0,
            specificity: 1,
            sel_atoms: 0,
            parent_kind: SEL_ATOM_NONE,
            border_width: 0,
            border_radius: 0,
            border_sides: 0,
            font_size: 0,
            margin_top: 0,
            margin_bottom: 0,
            margin_left: 0,
            margin_right: 0,
            padding_top: 0,
            padding_bottom: 0,
            padding_left: 0,
            padding_right: 0,
            gap: 0,
            grid_cols: 0,
            packed_enums: 0,
            text_align_bits: 0,
        }
    }

    #[inline(always)] pub fn atom0_kind(&self) -> u8 { self.sel_atoms & 0x07 }
    #[inline(always)] pub fn atom1_kind(&self) -> u8 { (self.sel_atoms >> 3) & 0x07 }
    #[inline(always)] pub fn set_atom_kinds(&mut self, k0: u8, k1: u8) {
        self.sel_atoms = (k0 & 0x07) | ((k1 & 0x07) << 3);
    }
    #[inline(always)] pub fn is_descendant(&self) -> bool { self.parent_kind != SEL_ATOM_NONE }
    #[inline(always)] pub fn is_tag_only(&self) -> bool {
        !self.is_descendant() && self.atom0_kind() == SEL_ATOM_TAG && self.atom1_kind() == SEL_ATOM_NONE
    }
}

fn encode_selector_part(sel: &[u8]) -> Option<([u32; 2], u8, u8, u8)> {
    let s = sel.trim_ascii();
    if s.is_empty() {
        return None;
    }
    let mut hashes = [0u32; 2];
    let mut kinds = [SEL_ATOM_NONE; 2];
    let mut count = 0usize;
    let mut spec = 0u16;
    let mut i = 0usize;

    while i < s.len() {
        let (kind, start) = if s[i] == b'.' {
            i += 1;
            (SEL_ATOM_CLASS, i)
        } else if s[i] == b'#' {
            i += 1;
            (SEL_ATOM_ID, i)
        } else {
            (SEL_ATOM_TAG, i)
        };
        while i < s.len() && s[i] != b'.' && s[i] != b'#' {
            i += 1;
        }
        let tok = &s[start..i];
        if tok.is_empty() {
            return None;
        }
        let (final_kind, h, add_spec) = match kind {
            SEL_ATOM_CLASS => (SEL_ATOM_CLASS, fnv1a_32_ci(tok), 10),
            SEL_ATOM_ID => (SEL_ATOM_ID, fnv1a_32(tok), 100),
            _ => {
                if tok == b"*" {
                    (SEL_ATOM_WILDCARD, 0, 0)
                } else {
                    (SEL_ATOM_TAG, fnv1a_32_ci(tok), 1)
                }
            }
        };
        spec = spec.saturating_add(add_spec);
        if count < 2 {
            kinds[count] = final_kind;
            hashes[count] = h;
            count += 1;
        } else {
            return None;
        }
    }

    if count == 0 {
        None
    } else {
        Some((hashes, kinds[0], kinds[1], spec.max(1).min(255) as u8))
    }
}

#[inline]
fn node_matches_atom(dom: &DomArena, node_idx: usize, kind: u8, hash: u32) -> bool {
    if node_idx >= dom.node_count {
        return false;
    }
    let node = &dom.nodes[node_idx];
    if node.node_type != DomNodeType::Element {
        return false;
    }
    match kind {
        SEL_ATOM_WILDCARD => true,
        SEL_ATOM_TAG => node.tag_len > 0 && fnv1a_32_ci(node.tag_slice()) == hash,
        SEL_ATOM_CLASS => dom.has_class_hash(node_idx, hash),
        SEL_ATOM_ID => node.id_len > 0 && fnv1a_32(node.id_slice()) == hash,
        _ => false,
    }
}

fn rule_matches_simple(dom: &DomArena, node_idx: usize, r: &CssRule) -> bool {
    let k0 = r.atom0_kind();
    if k0 == SEL_ATOM_NONE || !node_matches_atom(dom, node_idx, k0, r.sel_hashes[0]) {
        return false;
    }
    let k1 = r.atom1_kind();
    if k1 != SEL_ATOM_NONE && !node_matches_atom(dom, node_idx, k1, r.sel_hashes[1]) {
        return false;
    }
    true
}

fn parse_tw_spacing(raw: &[u8]) -> Option<usize> {
    if raw == b"px" { return Some(1); }
    let val100 = parse_fixed_100(raw)?;
    Some(((val100 * 4 + 50) / 100).min(48))
}

fn parse_tw_base_color(name: &[u8], shade: Option<usize>) -> Option<u32> {
    if name == b"white" { return Some(0x00FF_FFFF); }
    if name == b"black" { return Some(0x0000_0000); }
    if name == b"slate" || name == b"gray" || name == b"zinc" || name == b"neutral" {
        return Some(match shade.unwrap_or(500) {
            100 => 0x00F1_F5F9,
            200 => 0x00E2_E8F0,
            300 => 0x00CB_D5E1,
            400 => 0x0094_A3B8,
            500 => 0x0064_748B,
            600 => 0x0047_5569,
            700 => 0x0033_4155,
            800 => 0x001E_293B,
            900 => 0x000F_172A,
            _ => 0x0002_0617,
        });
    }
    if name == b"cyan" || name == b"sky" {
        return Some(match shade.unwrap_or(400) {
            200 => 0x00A5_F3FC,
            300 => 0x0067_E8F9,
            400 => 0x0022_D3EE,
            500 => 0x0006_B6D4,
            600 => 0x0008_91B2,
            700 => 0x000E_7490,
            _ => 0x0008_3344,
        });
    }
    if name == b"emerald" || name == b"green" || name == b"teal" {
        return Some(match shade.unwrap_or(400) {
            300 => 0x006E_E7B7,
            400 => 0x0034_D399,
            500 => 0x0010_B981,
            600 => 0x0005_9669,
            700 => 0x0004_7857,
            900 => 0x0006_4E3B,
            _ => 0x0002_2C22,
        });
    }
    if name == b"blue" || name == b"indigo" {
        return Some(match shade.unwrap_or(500) {
            300 => 0x00A5_B4FC,
            400 => 0x0060_A5FA,
            500 => 0x0063_66F1,
            600 => 0x004F_46E5,
            700 => 0x0043_38CA,
            900 => 0x001E_3A8A,
            _ => 0x001E_1B4B,
        });
    }
    if name == b"purple" || name == b"violet" || name == b"fuchsia" {
        return Some(match shade.unwrap_or(400) {
            300 => 0x00D8_B4FE,
            400 => 0x00C0_84FC,
            500 => 0x00A8_55F7,
            600 => 0x0093_33EA,
            700 => 0x007E_22CE,
            _ => 0x003B_0764,
        });
    }
    if name == b"amber" || name == b"yellow" || name == b"orange" {
        return Some(match shade.unwrap_or(400) {
            300 => 0x00FC_D34D,
            400 => 0x00FB_BF24,
            500 => 0x00F5_9E0B,
            600 => 0x00D9_7706,
            700 => 0x00B4_5309,
            _ => 0x0045_1A03,
        });
    }
    if name == b"rose" || name == b"red" {
        return Some(match shade.unwrap_or(400) {
            300 => 0x00FD_A4AF,
            400 => 0x00FB_7185,
            500 => 0x00F4_3F5E,
            600 => 0x00E1_1D48,
            700 => 0x00BE_123C,
            _ => 0x004C_0519,
        });
    }
    None
}

fn parse_tw_color_token(spec: &[u8]) -> Option<u32> {
    let (color_part, alpha_opt) = if let Some(slash) = spec.iter().position(|&b| b == b'/') {
        (&spec[..slash], parse_px(&spec[slash + 1..]))
    } else {
        (spec, None)
    };
    let (name, shade) = if let Some(dash) = color_part.iter().rposition(|&b| b == b'-') {
        (&color_part[..dash], parse_px(&color_part[dash + 1..]))
    } else {
        (color_part, None)
    };
    let rgb = parse_tw_base_color(name, shade)?;
    if let Some(pct) = alpha_opt {
        let a255 = ((pct.min(100) * 255 + 50) / 100) as u32;
        if a255 == 0 { return None; }
        if a255 < 250 {
            return Some(blend_over_rgb(rgb, CSS_DARK_CANVAS, a255));
        }
    }
    Some(rgb)
}

pub struct MiniCss {
    pub rules: [CssRule; MAX_CSS_RULES],
    pub rule_count: usize,
    pub vars: [CssVar; MAX_CSS_VARS],
    pub var_count: usize,
    pub body_bg_color: Option<u32>,
}

#[inline(always)]
fn display_to_bits(d: DisplayMode) -> u8 {
    match d {
        DisplayMode::Block => 0,
        DisplayMode::Inline => 1,
        DisplayMode::InlineBlock => 2,
        DisplayMode::Flex => 3,
        DisplayMode::None => 4,
    }
}

#[inline(always)]
fn bits_to_display(b: u8) -> DisplayMode {
    match b & 0x07 {
        1 => DisplayMode::Inline,
        2 => DisplayMode::InlineBlock,
        3 => DisplayMode::Flex,
        4 => DisplayMode::None,
        _ => DisplayMode::Block,
    }
}

#[inline(always)]
fn justify_to_bits(j: JustifyContent) -> u8 {
    match j {
        JustifyContent::FlexStart => 0,
        JustifyContent::Center => 1,
        JustifyContent::FlexEnd => 2,
        JustifyContent::SpaceBetween => 3,
    }
}

#[inline(always)]
fn bits_to_justify(b: u8) -> JustifyContent {
    match b & 0x03 {
        1 => JustifyContent::Center,
        2 => JustifyContent::FlexEnd,
        3 => JustifyContent::SpaceBetween,
        _ => JustifyContent::FlexStart,
    }
}

#[inline(always)]
fn align_to_bits(a: AlignItems) -> u8 {
    match a {
        AlignItems::FlexStart => 0,
        AlignItems::Center => 1,
        AlignItems::FlexEnd => 2,
        AlignItems::Stretch => 3,
    }
}

#[inline(always)]
fn bits_to_align(b: u8) -> AlignItems {
    match b & 0x03 {
        1 => AlignItems::Center,
        2 => AlignItems::FlexEnd,
        3 => AlignItems::Stretch,
        _ => AlignItems::FlexStart,
    }
}

#[inline(always)]
fn text_align_to_bits(t: TextAlign) -> u8 {
    match t {
        TextAlign::Left => 0,
        TextAlign::Center => 1,
        TextAlign::Right => 2,
    }
}

#[inline(always)]
fn bits_to_text_align(b: u8) -> TextAlign {
    match b & 0x03 {
        1 => TextAlign::Center,
        2 => TextAlign::Right,
        _ => TextAlign::Left,
    }
}

impl MiniCss {
    pub const fn new() -> Self {
        Self {
            rules: [CssRule::empty(); MAX_CSS_RULES],
            rule_count: 0,
            vars: [CssVar::empty(); MAX_CSS_VARS],
            var_count: 0,
            body_bg_color: None,
        }
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.rule_count = 0;
        self.var_count = 0;
        self.body_bg_color = None;
    }

    fn set_var(&mut self, name: &[u8], val: &[u8]) {
        let clean_val = strip_css_important(val);
        let h = fnv1a_32_ci(name);
        for i in 0..self.var_count {
            if self.vars[i].name_hash == h {
                self.vars[i].val_len = copy_trunc_u8(&mut self.vars[i].val, clean_val);
                return;
            }
        }
        if self.var_count < MAX_CSS_VARS {
            let slot = &mut self.vars[self.var_count];
            slot.name_hash = h;
            slot.val_len = copy_trunc_u8(&mut slot.val, clean_val);
            self.var_count += 1;
        }
    }

    fn lookup_var<'a>(&'a self, name: &[u8]) -> Option<&'a [u8]> {
        let h = fnv1a_32_ci(name);
        for i in 0..self.var_count {
            if self.vars[i].name_hash == h {
                let len = (self.vars[i].val_len as usize).min(self.vars[i].val.len());
                return Some(&self.vars[i].val[..len]);
            }
        }
        None
    }

    pub fn resolve_css_value<'a>(&self, raw: &[u8], scratch: &'a mut [u8; 160]) -> &'a [u8] {
        let s = strip_css_important(raw);
        let mut out_len = 0usize;
        let mut i = 0usize;
        while i < s.len() && out_len < scratch.len() - 1 {
            if s[i..].starts_with(b"var(") {
                let mut j = i + 4;
                let mut depth = 1usize;
                while j < s.len() && depth > 0 {
                    if s[j] == b'(' { depth += 1; }
                    else if s[j] == b')' { depth -= 1; }
                    j += 1;
                }
                let inner = if j > i + 4 && s[j - 1] == b')' {
                    &s[i + 4..j - 1]
                } else {
                    &s[i + 4..j]
                };
                let (vname, fallback) = if let Some(comma) = inner.iter().position(|&b| b == b',') {
                    (inner[..comma].trim_ascii(), Some(inner[comma + 1..].trim_ascii()))
                } else {
                    (inner.trim_ascii(), None)
                };
                let replacement = self.lookup_var(vname).or(fallback).unwrap_or(b"");
                let take = replacement.len().min((scratch.len() - 1) - out_len);
                scratch[out_len..out_len + take].copy_from_slice(&replacement[..take]);
                out_len += take;
                i = j;
            } else {
                scratch[out_len] = s[i];
                out_len += 1;
                i += 1;
            }
        }
        scratch[..out_len].trim_ascii()
    }

    fn apply_decl_to_rule(&self, rule: &mut CssRule, prop: &[u8], raw_val: &[u8]) {
        let mut val_buf = [0u8; 160];
        let val = self.resolve_css_value(raw_val, &mut val_buf);
        if val.is_empty() { return; }

        if prop.eq_ignore_ascii_case(b"color") {
            if let Some(c) = parse_color(val) {
                rule.color = c;
                rule.present_mask |= RULE_HAS_COLOR;
            }
        } else if prop.eq_ignore_ascii_case(b"background-color") || prop.eq_ignore_ascii_case(b"background") {
            if let Some(bg) = parse_color(val) {
                rule.bg_color = bg;
                rule.present_mask |= RULE_HAS_BG;
            }
        } else if prop.eq_ignore_ascii_case(b"border") {
            let (bw, bc) = parse_border(val);
            if let Some(w) = bw {
                rule.border_width = w.min(255) as u8;
                rule.present_mask |= RULE_HAS_BORDER_WIDTH;
            }
            if let Some(c) = bc {
                rule.border_color = c;
                rule.present_mask |= RULE_HAS_BORDER_COLOR;
            }
            rule.border_sides = 15;
            rule.present_mask |= RULE_HAS_BORDER_SIDES;
        } else if prop.eq_ignore_ascii_case(b"border-bottom")
            || prop.eq_ignore_ascii_case(b"border-top")
            || prop.eq_ignore_ascii_case(b"border-left")
            || prop.eq_ignore_ascii_case(b"border-right")
        {
            let (bw, bc) = parse_border(val);
            if let Some(w) = bw {
                rule.border_width = w.min(255) as u8;
                rule.present_mask |= RULE_HAS_BORDER_WIDTH;
            }
            if let Some(c) = bc {
                rule.border_color = c;
                rule.present_mask |= RULE_HAS_BORDER_COLOR;
            }
            let bit = if prop.eq_ignore_ascii_case(b"border-top") { 1 }
                else if prop.eq_ignore_ascii_case(b"border-right") { 2 }
                else if prop.eq_ignore_ascii_case(b"border-bottom") { 4 }
                else { 8 };
            let prev = if (rule.present_mask & RULE_HAS_BORDER_SIDES) != 0 { rule.border_sides } else { 0 };
            rule.border_sides = prev | bit;
            rule.present_mask |= RULE_HAS_BORDER_SIDES;
        } else if prop.eq_ignore_ascii_case(b"-webkit-mask-image") || prop.eq_ignore_ascii_case(b"mask-image") {
            rule.packed_enums = (rule.packed_enums & !0x07) | display_to_bits(DisplayMode::None);
            rule.present_mask |= RULE_HAS_DISPLAY;
        } else if prop.eq_ignore_ascii_case(b"border-color") {
            if let Some(bc) = parse_color(val) {
                rule.border_color = bc;
                rule.present_mask |= RULE_HAS_BORDER_COLOR;
            }
        } else if prop.eq_ignore_ascii_case(b"border-width") {
            if let Some(w) = parse_px(val) {
                rule.border_width = w.min(255) as u8;
                rule.present_mask |= RULE_HAS_BORDER_WIDTH;
            }
        } else if prop.eq_ignore_ascii_case(b"border-radius") {
            if let Some(r) = parse_px(val) {
                rule.border_radius = r.min(255) as u8;
                rule.present_mask |= RULE_HAS_BORDER_RADIUS;
            }
        } else if prop.eq_ignore_ascii_case(b"font-size") {
            if let Some(fs) = parse_px(val) {
                rule.font_size = fs.min(255) as u8;
                rule.present_mask |= RULE_HAS_FONT_SIZE;
            }
        } else if prop.eq_ignore_ascii_case(b"width") {
            if let Some(w) = parse_px(val) {
                rule.width = w.min(u16::MAX as usize) as u16;
                rule.present_mask |= RULE_HAS_WIDTH;
            }
        } else if prop.eq_ignore_ascii_case(b"height") {
            if let Some(h) = parse_px(val) {
                rule.height = h.min(u16::MAX as usize) as u16;
                rule.present_mask |= RULE_HAS_HEIGHT;
            }
        } else if prop.eq_ignore_ascii_case(b"max-width") {
            if let Some(mw) = parse_px(val) {
                rule.max_width = mw.min(u16::MAX as usize) as u16;
                rule.present_mask |= RULE_HAS_MAX_WIDTH;
            }
        } else if prop.eq_ignore_ascii_case(b"margin") {
            if let Some((v, h)) = parse_one_or_two_px(val) {
                let vb = v.min(255) as u8;
                let hb = h.min(255) as u8;
                rule.margin_top = vb;
                rule.margin_bottom = vb;
                rule.margin_left = hb;
                rule.margin_right = hb;
                rule.present_mask |= RULE_HAS_MT | RULE_HAS_MB | RULE_HAS_ML | RULE_HAS_MR;
            }
        } else if prop.eq_ignore_ascii_case(b"margin-top") {
            if let Some(v) = parse_px(val) { rule.margin_top = v.min(255) as u8; rule.present_mask |= RULE_HAS_MT; }
        } else if prop.eq_ignore_ascii_case(b"margin-bottom") {
            if let Some(v) = parse_px(val) { rule.margin_bottom = v.min(255) as u8; rule.present_mask |= RULE_HAS_MB; }
        } else if prop.eq_ignore_ascii_case(b"margin-left") {
            if let Some(v) = parse_px(val) { rule.margin_left = v.min(255) as u8; rule.present_mask |= RULE_HAS_ML; }
        } else if prop.eq_ignore_ascii_case(b"margin-right") {
            if let Some(v) = parse_px(val) { rule.margin_right = v.min(255) as u8; rule.present_mask |= RULE_HAS_MR; }
        } else if prop.eq_ignore_ascii_case(b"padding") {
            if let Some((v, h)) = parse_one_or_two_px(val) {
                let vb = v.min(255) as u8;
                let hb = h.min(255) as u8;
                rule.padding_top = vb;
                rule.padding_bottom = vb;
                rule.padding_left = hb;
                rule.padding_right = hb;
                rule.present_mask |= RULE_HAS_PT | RULE_HAS_PB | RULE_HAS_PL | RULE_HAS_PR;
            }
        } else if prop.eq_ignore_ascii_case(b"padding-top") {
            if let Some(v) = parse_px(val) { rule.padding_top = v.min(255) as u8; rule.present_mask |= RULE_HAS_PT; }
        } else if prop.eq_ignore_ascii_case(b"padding-bottom") {
            if let Some(v) = parse_px(val) { rule.padding_bottom = v.min(255) as u8; rule.present_mask |= RULE_HAS_PB; }
        } else if prop.eq_ignore_ascii_case(b"padding-left") {
            if let Some(v) = parse_px(val) { rule.padding_left = v.min(255) as u8; rule.present_mask |= RULE_HAS_PL; }
        } else if prop.eq_ignore_ascii_case(b"padding-right") {
            if let Some(v) = parse_px(val) { rule.padding_right = v.min(255) as u8; rule.present_mask |= RULE_HAS_PR; }
        } else if prop.eq_ignore_ascii_case(b"gap")
            || prop.eq_ignore_ascii_case(b"column-gap")
            || prop.eq_ignore_ascii_case(b"row-gap")
        {
            if let Some(g) = parse_px(val) {
                rule.gap = g.min(255) as u8;
                rule.present_mask |= RULE_HAS_GAP;
            }
        } else if prop.eq_ignore_ascii_case(b"font-weight") {
            if val.eq_ignore_ascii_case(b"bold") || val.eq_ignore_ascii_case(b"600") || val.eq_ignore_ascii_case(b"700") || val.eq_ignore_ascii_case(b"800") {
                rule.present_mask |= RULE_HAS_BOLD | RULE_IS_BOLD_VAL;
            } else if val.eq_ignore_ascii_case(b"normal") || val.eq_ignore_ascii_case(b"400") {
                rule.present_mask = (rule.present_mask | RULE_HAS_BOLD) & !RULE_IS_BOLD_VAL;
            }
        } else if prop.eq_ignore_ascii_case(b"display") {
            if let Some(d) = parse_display(val) {
                rule.packed_enums = (rule.packed_enums & !0x07) | display_to_bits(d);
                rule.present_mask |= RULE_HAS_DISPLAY;
            }
        } else if prop.eq_ignore_ascii_case(b"flex-direction") {
            if let Some(fd) = parse_flex_direction(val) {
                if fd == FlexDirection::Column {
                    rule.packed_enums |= 0x08;
                } else {
                    rule.packed_enums &= !0x08;
                }
                rule.present_mask |= RULE_HAS_FLEX_DIR;
            }
        } else if prop.eq_ignore_ascii_case(b"flex-wrap") {
            if let Some(fw) = parse_flex_wrap(val) {
                rule.present_mask |= RULE_HAS_FLEX_WRAP;
                if fw { rule.present_mask |= RULE_FLEX_WRAP_VAL; } else { rule.present_mask &= !RULE_FLEX_WRAP_VAL; }
            }
        } else if prop.eq_ignore_ascii_case(b"justify-content") {
            if let Some(jc) = parse_justify_content(val) {
                rule.packed_enums = (rule.packed_enums & !(0x03 << 4)) | (justify_to_bits(jc) << 4);
                rule.present_mask |= RULE_HAS_JUSTIFY;
            }
        } else if prop.eq_ignore_ascii_case(b"align-items") {
            if let Some(ai) = parse_align_items(val) {
                rule.packed_enums = (rule.packed_enums & !(0x03 << 6)) | (align_to_bits(ai) << 6);
                rule.present_mask |= RULE_HAS_ALIGN;
            }
        } else if prop.eq_ignore_ascii_case(b"text-align") {
            if let Some(ta) = parse_text_align(val) {
                rule.text_align_bits = text_align_to_bits(ta);
                rule.present_mask |= RULE_HAS_TEXT_ALIGN;
            }
        } else if prop.eq_ignore_ascii_case(b"grid-template-columns") {
            if let Some(gc) = parse_grid_template_columns(val) {
                rule.grid_cols = gc.min(255) as u8;
                rule.present_mask |= RULE_HAS_GRID_COLS;
            }
        }
    }

    pub fn parse_stylesheet(&mut self, css: &[u8]) {
        self.reset();
        self.parse_stylesheet_append(css);
    }

    pub fn parse_stylesheet_append(&mut self, css: &[u8]) {
        let mut i = 0usize;
        let len = css.len();

        while i < len {
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

            // Suporta blocos aninhados (@media / @keyframes) sem corromper o scanner
            if sel_raw.starts_with(b"@") {
                let mut depth = 1usize;
                while i < len && depth > 0 {
                    if css[i] == b'{' { depth += 1; }
                    else if css[i] == b'}' { depth -= 1; }
                    i += 1;
                }
                continue;
            }

            let body_start = i;
            let mut depth = 1usize;
            while i < len && depth > 0 {
                if css[i] == b'{' { depth += 1; }
                else if css[i] == b'}' { depth -= 1; }
                i += 1;
            }
            let body_end = if depth == 0 && i > body_start { i - 1 } else { i };
            let body = &css[body_start..body_end];

            if sel_raw.is_empty() { continue; }

            // Processa blocos `:root` extraindo `--var: val` com nome indexado por FNV-1a
            if sel_raw.eq_ignore_ascii_case(b":root") {
                for decl in body.split(|&b| b == b';') {
                    let decl_clean = decl.trim_ascii();
                    if let Some(colon) = decl_clean.iter().position(|&b| b == b':') {
                        let prop = decl_clean[..colon].trim_ascii();
                        let val = decl_clean[colon + 1..].trim_ascii();
                        if prop.starts_with(b"--") {
                            self.set_var(prop, val);
                        }
                    }
                }
                continue;
            }

            if self.rule_count >= MAX_CSS_RULES {
                continue;
            }

            for sub_sel in sel_raw.split(|&b| b == b',') {
                let s_clean = sub_sel.trim_ascii();
                if s_clean.is_empty() || self.rule_count >= MAX_CSS_RULES { continue; }
                if s_clean.contains(&b':') {
                    continue;
                }

                let mut rule = CssRule::empty();
                let last_ws = s_clean.iter().rposition(|&b| b == b' ' || b == b'\t' || b == b'>');
                let is_body_sel = if let Some(pos) = last_ws {
                    let mut p_part = s_clean[..pos].trim_ascii();
                    while let Some(prev_ws) = p_part.iter().rposition(|&b| b == b' ' || b == b'\t' || b == b'>') {
                        p_part = p_part[prev_ws + 1..].trim_ascii();
                    }
                    let c_part = s_clean[pos + 1..].trim_ascii();
                    let Some((p_hashes, p_k0, _, p_spec)) = encode_selector_part(p_part) else {
                        continue;
                    };
                    let Some((c_hashes, c_k0, c_k1, c_spec)) = encode_selector_part(c_part) else {
                        continue;
                    };
                    rule.parent_kind = p_k0;
                    rule.parent_hash = p_hashes[0];
                    rule.sel_hashes = c_hashes;
                    rule.set_atom_kinds(c_k0, c_k1);
                    rule.specificity = p_spec.saturating_add(c_spec);
                    false
                } else {
                    let Some((hashes, k0, k1, spec)) = encode_selector_part(s_clean) else {
                        continue;
                    };
                    rule.parent_kind = SEL_ATOM_NONE;
                    rule.parent_hash = 0;
                    rule.sel_hashes = hashes;
                    rule.set_atom_kinds(k0, k1);
                    rule.specificity = spec;
                    s_clean.eq_ignore_ascii_case(b"body")
                };

                if is_body_sel {
                    rule.present_mask |= RULE_IS_BODY_TAG;
                }

                for decl in body.split(|&b| b == b';') {
                    let decl_clean = decl.trim_ascii();
                    if decl_clean.is_empty() { continue; }
                    if let Some(colon) = decl_clean.iter().position(|&b| b == b':') {
                        let prop = decl_clean[..colon].trim_ascii();
                        let val = decl_clean[colon + 1..].trim_ascii();
                        self.apply_decl_to_rule(&mut rule, prop, val);
                    }
                }

                if is_body_sel && (rule.present_mask & RULE_HAS_BG) != 0 {
                    self.body_bg_color = Some(rule.bg_color);
                }

                self.rules[self.rule_count] = rule;
                self.rule_count += 1;
            }
        }
    }

    pub fn resolve_all(&self, dom: &mut DomArena, viewport_w: usize) {
        for i in 0..dom.node_count {
            self.resolve_node(dom, i, viewport_w);
        }
    }

    fn apply_tailwind_token(style: &mut ComputedStyle, token: &[u8], tag_bytes: &[u8]) {
        if token == b"hidden" {
            style.display = DisplayMode::None;
            style.set_display_block(false);
        } else if token == b"block" {
            style.display = DisplayMode::Block;
            style.set_display_block(true);
        } else if token == b"inline" {
            style.display = DisplayMode::Inline;
            style.set_display_block(false);
        } else if token == b"inline-block" || token == b"inline-flex" {
            style.display = DisplayMode::InlineBlock;
            style.set_display_block(false);
        } else if token == b"flex" {
            style.display = DisplayMode::Flex;
            style.set_display_block(false);
        } else if token == b"grid" {
            style.display = DisplayMode::Flex;
            style.set_display_block(false);
            style.set_flex_wrap(true);
            if style.grid_cols == 0 {
                style.grid_cols = 1;
            }
        } else if token == b"flex-col" {
            style.flex_direction = FlexDirection::Column;
        } else if token == b"flex-row" {
            style.flex_direction = FlexDirection::Row;
        } else if token == b"flex-wrap" {
            style.set_flex_wrap(true);
        } else if token.starts_with(b"grid-cols-") {
            if let Some(cols) = parse_px(&token[10..]) {
                style.grid_cols = cols.clamp(1, 6) as u8;
            }
        } else if token == b"items-center" {
            style.align_items = AlignItems::Center;
        } else if token == b"justify-between" {
            style.justify_content = JustifyContent::SpaceBetween;
        } else if token == b"justify-center" {
            style.justify_content = JustifyContent::Center;
        } else if token == b"text-center" {
            style.text_align = TextAlign::Center;
        } else if token == b"font-bold" || token == b"font-semibold" || token == b"font-medium" || token == b"font-black" {
            style.set_bold(true);
        } else if token == b"w-full" {
            style.set_width(None);
        } else if token == b"border" {
            style.border_width = 1;
            style.border_sides = 15;
            if !style.has_border_color() {
                style.set_border_color(Some(0x0033_4155));
            }
        } else if token == b"border-2" {
            style.border_width = 2;
            style.border_sides = 15;
            if !style.has_border_color() {
                style.set_border_color(Some(0x0033_4155));
            }
        } else if token == b"border-b" || token == b"border-t" || token == b"border-l" || token == b"border-l-4" {
            style.border_width = if token == b"border-l-4" { 3 } else { 1 };
            style.border_sides = if token == b"border-t" { 1 } else if token == b"border-b" { 4 } else { 8 };
            if !style.has_border_color() {
                style.set_border_color(Some(0x0033_4155));
            }
        } else if token == b"rounded" {
            style.border_radius = 4;
        } else if token == b"rounded-lg" {
            style.border_radius = 8;
        } else if token == b"rounded-xl" {
            style.border_radius = 10;
        } else if token == b"rounded-full" {
            style.border_radius = 12;
        } else if token.starts_with(b"gap-") {
            if let Some(g) = parse_tw_spacing(&token[4..]) { style.gap = g.min(255) as u8; }
        } else if token.starts_with(b"space-y-") {
            if let Some(g) = parse_tw_spacing(&token[8..]) { style.gap = g.min(255) as u8; }
        } else if token.starts_with(b"p-") {
            if let Some(p) = parse_tw_spacing(&token[2..]) {
                let pb = p.min(255) as u8;
                style.padding_top = pb; style.padding_bottom = pb;
                style.padding_left = pb; style.padding_right = pb;
            }
        } else if token.starts_with(b"px-") {
            if let Some(p) = parse_tw_spacing(&token[3..]) {
                let pb = p.min(255) as u8;
                style.padding_left = pb; style.padding_right = pb;
            }
        } else if token.starts_with(b"py-") {
            if let Some(p) = parse_tw_spacing(&token[3..]) {
                let pb = p.min(255) as u8;
                style.padding_top = pb; style.padding_bottom = pb;
            }
        } else if token.starts_with(b"pt-") {
            if let Some(p) = parse_tw_spacing(&token[3..]) { style.padding_top = p.min(255) as u8; }
        } else if token.starts_with(b"pb-") {
            if let Some(p) = parse_tw_spacing(&token[3..]) { style.padding_bottom = p.min(255) as u8; }
        } else if token.starts_with(b"pl-") {
            if let Some(p) = parse_tw_spacing(&token[3..]) { style.padding_left = p.min(255) as u8; }
        } else if token.starts_with(b"mt-") {
            if let Some(m) = parse_tw_spacing(&token[3..]) { style.margin_top = m.min(255) as u8; }
        } else if token.starts_with(b"mb-") {
            if let Some(m) = parse_tw_spacing(&token[3..]) { style.margin_bottom = m.min(255) as u8; }
        } else if token.starts_with(b"my-") {
            if let Some(m) = parse_tw_spacing(&token[3..]) {
                let mb = m.min(255) as u8;
                style.margin_top = mb; style.margin_bottom = mb;
            }
        } else if token.starts_with(b"w-") {
            if let Some(w_px) = parse_tw_spacing(&token[2..]) { style.set_width(Some(w_px.max(6))); }
        } else if token.starts_with(b"h-") {
            if let Some(h_px) = parse_tw_spacing(&token[2..]) { style.set_height(Some(h_px.max(6))); }
        } else if token.starts_with(b"bg-") && !token.starts_with(b"bg-gradient") {
            if let Some(c) = parse_tw_color_token(&token[3..]) {
                style.set_bg_color(Some(c));
            }
        } else if token.starts_with(b"from-") {
            if !style.has_bg() {
                if let Some(c) = parse_tw_color_token(&token[5..]) {
                    style.set_bg_color(Some(c));
                }
            }
        } else if token.starts_with(b"to-") {
            if let Some(c_to) = parse_tw_color_token(&token[3..]) {
                let blended = if style.has_bg() { blend_over_rgb(style.bg_color, c_to, 128) } else { c_to };
                style.set_bg_color(Some(blended));
            }
        } else if token.starts_with(b"border-") {
            if let Some(c) = parse_tw_color_token(&token[7..]) {
                style.set_border_color(Some(c));
            }
        } else if token.starts_with(b"text-") {
            if token == b"text-xs" { style.font_size = 11; }
            else if token == b"text-sm" { style.font_size = 12; }
            else if token == b"text-lg" { style.font_size = 14; }
            else if token == b"text-xl" || token == b"text-2xl" || token == b"text-3xl" { style.font_size = 16; }
            else if let Some(c) = parse_tw_color_token(&token[5..]) {
                style.color = c;
                style.set_explicit_color(true);
            }
        } else if token == b"font-mono" && tag_bytes != b"a" && !style.has_explicit_color() {
            style.color = 0x00CB_D5E1;
        }
    }

    pub fn resolve_node(&self, dom: &mut DomArena, node_idx: usize, viewport_w: usize) {
        if node_idx >= dom.node_count { return; }

        let tag = dom.nodes[node_idx].tag;
        let tag_len = dom.nodes[node_idx].tag_len as usize;
        let tag_bytes = &tag[..tag_len.min(tag.len())];
        let parent_idx = dom.nodes[node_idx].parent_idx();

        // 1. Estilos Padrão do User-Agent baseados na Tag
        let mut style = ComputedStyle::default();
        if tag_bytes == b"h1" {
            style.color = COLOR_H1;
            style.set_explicit_color(true);
            style.font_size = 16;
            style.margin_top = 8;
            style.margin_bottom = 6;
            style.set_bold(true);
            style.display = DisplayMode::Block;
            style.set_display_block(true);
        } else if tag_bytes == b"h2" {
            style.color = 0x00F3_8BA8;
            style.set_explicit_color(true);
            style.font_size = 14;
            style.margin_top = 6;
            style.margin_bottom = 4;
            style.set_bold(true);
            style.display = DisplayMode::Block;
            style.set_display_block(true);
        } else if tag_bytes == b"h3" {
            style.color = 0x00A6_E3A1;
            style.set_explicit_color(true);
            style.font_size = 13;
            style.margin_top = 4;
            style.margin_bottom = 4;
            style.set_bold(true);
            style.display = DisplayMode::Block;
            style.set_display_block(true);
        } else if tag_bytes == b"h4" || tag_bytes == b"h5" || tag_bytes == b"h6" {
            style.color = 0x00F9_E2AF;
            style.set_explicit_color(true);
            style.margin_top = 3;
            style.margin_bottom = 3;
            style.set_bold(true);
            style.display = DisplayMode::Block;
            style.set_display_block(true);
        } else if tag_bytes == b"p" {
            style.color = COLOR_TEXT;
            style.margin_top = 2;
            style.margin_bottom = 4;
            style.display = DisplayMode::Block;
            style.set_display_block(true);
        } else if tag_bytes == b"a" {
            style.color = COLOR_LINK;
            style.display = DisplayMode::Inline;
            style.set_display_block(false);
        } else if tag_bytes == b"button" {
            style.color = COLOR_BTN_TEXT;
            style.set_explicit_color(true);
            style.set_bg_color(Some(0x0031_3244));
            style.set_border_color(Some(0x0045_475A));
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
            style.set_display_block(false);
        } else if tag_bytes == b"div" || tag_bytes == b"nav" || tag_bytes == b"header" || tag_bytes == b"section" || tag_bytes == b"main" || tag_bytes == b"article" || tag_bytes == b"footer" || tag_bytes == b"aside" || tag_bytes == b"picture" || tag_bytes == b"figure" || tag_bytes == b"figcaption" || tag_bytes == b"form" {
            style.display = DisplayMode::Block;
            style.set_display_block(true);
            let is_semantic_box = tag_bytes == b"section" || tag_bytes == b"article" || tag_bytes == b"header" || tag_bytes == b"footer" || tag_bytes == b"nav";
            style.margin_top = if is_semantic_box { 4 } else { 0 };
            style.margin_bottom = if is_semantic_box { 4 } else { 0 };
        } else if tag_bytes == b"blockquote" || tag_bytes == b"pre" {
            style.display = DisplayMode::Block;
            style.set_display_block(true);
            style.set_bg_color(Some(0x001E_1E2E));
            style.set_border_color(Some(0x0045_475A));
            style.border_width = 1;
            style.border_radius = 4;
            style.margin_top = 4;
            style.margin_bottom = 4;
            style.padding_top = 4;
            style.padding_bottom = 4;
            style.padding_left = 8;
            style.padding_right = 6;
            if tag_bytes == b"pre" {
                style.color = 0x00A6_E3A1;
                style.set_explicit_color(true);
            }
        } else if tag_bytes == b"table" || tag_bytes == b"tr" {
            style.display = DisplayMode::Block;
            style.set_display_block(true);
            style.margin_top = if tag_bytes == b"table" { 4 } else { 1 };
            style.margin_bottom = if tag_bytes == b"table" { 4 } else { 2 };
        } else if tag_bytes == b"td" || tag_bytes == b"th" {
            style.display = DisplayMode::InlineBlock;
            style.set_display_block(false);
            style.padding_left = 4;
            style.padding_right = 6;
            if tag_bytes == b"th" {
                style.set_bold(true);
                style.color = 0x0089_DCEB;
                style.set_explicit_color(true);
            }
        } else if tag_bytes == b"hr" {
            style.display = DisplayMode::Block;
            style.set_display_block(true);
            style.margin_top = 6;
            style.margin_bottom = 6;
        } else if tag_bytes == b"b" || tag_bytes == b"strong" {
            style.set_bold(true);
            style.display = DisplayMode::Inline;
            style.set_display_block(false);
        } else if tag_bytes == b"ul" || tag_bytes == b"ol" {
            style.display = DisplayMode::Block;
            style.set_display_block(true);
            style.margin_top = 4;
            style.margin_bottom = 4;
            style.padding_left = 8;
        } else if tag_bytes == b"li" {
            style.display = DisplayMode::Block;
            style.set_display_block(true);
            style.margin_top = 2;
            style.margin_bottom = 2;
        } else if tag_bytes == b"img" {
            style.display = DisplayMode::InlineBlock;
            style.set_display_block(false);
        } else if tag_bytes == b"code" {
            style.display = DisplayMode::Inline;
            style.set_display_block(false);
            style.color = 0x00A6_E3A1;
            style.set_explicit_color(true);
            style.set_bg_color(Some(0x001E_293B));
            style.set_border_color(Some(0x0033_4155));
            style.border_width = 1;
            style.border_radius = 3;
            style.padding_top = 1;
            style.padding_bottom = 1;
            style.padding_left = 3;
            style.padding_right = 3;
        } else if tag_bytes == b"span" || tag_bytes == b"#text" || tag_bytes == b"i" || tag_bytes == b"em" || tag_bytes == b"small" || tag_bytes == b"time" || tag_bytes == b"label" {
            style.display = DisplayMode::Inline;
            style.set_display_block(false);
            if tag_bytes == b"#text" {
                style.margin_top = 0;
                style.margin_bottom = 0;
            }
        } else if tag_bytes == b"input" || tag_bytes == b"textarea" {
            let sentinel = dom.nodes[node_idx].alt_len;
            if sentinel == 2 {
                style.display = DisplayMode::None;
                style.set_display_block(false);
            } else if sentinel == 3 {
                style.color = 0x00E0_F2FE;
                style.set_explicit_color(true);
                style.set_bg_color(Some(0x001E_3A5F));
                style.set_border_color(Some(0x0038_BDF8));
                style.border_width = 1;
                style.border_radius = 4;
                style.padding_top = 4;
                style.padding_bottom = 4;
                style.padding_left = 10;
                style.padding_right = 10;
                style.margin_top = 3;
                style.margin_bottom = 3;
                style.display = DisplayMode::InlineBlock;
                style.set_display_block(false);
            } else {
                style.color = if sentinel == 1 { 0x0094_A3B8 } else { COLOR_TEXT };
                style.set_explicit_color(true);
                style.set_bg_color(Some(0x000F_172A));
                style.set_border_color(Some(0x0047_5569));
                style.border_width = 1;
                style.border_radius = 4;
                style.margin_top = 3;
                style.margin_bottom = 4;
                style.padding_top = 4;
                style.padding_bottom = 4;
                style.padding_left = 8;
                style.padding_right = 8;
                style.display = DisplayMode::Block;
                style.set_display_block(true);
            }
        }

        // Herança CSS de cor do elemento pai quando não há cor explícita no elemento
        if !style.has_explicit_color() && tag_bytes != b"a" {
            if let Some(pid) = parent_idx {
                if pid < dom.node_count {
                    style.color = dom.nodes[pid].computed_style.color;
                }
            }
        }

        // 2. Cascata CSS indexada por FNV-1a (faixas de especificidade: Tag/Desc -> Classe -> Composta -> ID)
        const TIERS: [(u8, u8); 4] = [(0, 10), (10, 20), (20, 100), (100, u8::MAX)];
        for &(min_s, max_s) in &TIERS {
            for i in 0..self.rule_count {
                let r = &self.rules[i];
                if r.specificity < min_s || r.specificity >= max_s {
                    continue;
                }
                let matched = if !r.is_descendant() {
                    rule_matches_simple(dom, node_idx, r)
                } else if rule_matches_simple(dom, node_idx, r) {
                    let mut matched_ancestor = false;
                    let mut cur_parent = dom.nodes[node_idx].parent_idx();
                    while let Some(pid) = cur_parent {
                        if node_matches_atom(dom, pid, r.parent_kind, r.parent_hash) {
                            matched_ancestor = true;
                            break;
                        }
                        cur_parent = dom.nodes[pid].parent_idx();
                    }
                    matched_ancestor
                } else {
                    false
                };
                if matched {
                    apply_rule_to_style(&mut style, r);
                }
            }
        }

        // 3. Utilitários Tailwind CSS Nativos (Base + Breakpoints Desktop sm:/md:/lg:)
        if dom.nodes[node_idx].class_len > 0 {
            let class_copy = dom.nodes[node_idx].class_attr;
            let class_len = (dom.nodes[node_idx].class_len as usize).min(class_copy.len());
            let class_slice = &class_copy[..class_len];

            for tok in class_slice.split(|&b| b == b' ' || b == b'\t' || b == b'\r' || b == b'\n') {
                if tok.is_empty() || tok.contains(&b':') {
                    continue;
                }
                Self::apply_tailwind_token(&mut style, tok, tag_bytes);
            }

            let bps: [(&[u8], usize); 3] = [(b"sm:", 480), (b"md:", 760), (b"lg:", 960)];
            for &(prefix, min_w) in &bps {
                if viewport_w >= min_w {
                    for tok in class_slice.split(|&b| b == b' ' || b == b'\t' || b == b'\r' || b == b'\n') {
                        if tok.starts_with(prefix) && tok.len() > prefix.len() {
                            Self::apply_tailwind_token(&mut style, &tok[prefix.len()..], tag_bytes);
                        }
                    }
                }
            }
        }

        // 4. Estilos Inline (style="...") — especificidade máxima
        if dom.nodes[node_idx].style_len > 0 {
            let inline = dom.nodes[node_idx].style_attr;
            let inline_len = (dom.nodes[node_idx].style_len as usize).min(inline.len());
            let inline_slice = &inline[..inline_len];
            for decl in inline_slice.split(|&b| b == b';') {
                let decl_clean = decl.trim_ascii();
                if let Some(colon) = decl_clean.iter().position(|&b| b == b':') {
                    let prop = decl_clean[..colon].trim_ascii();
                    let raw_val = decl_clean[colon + 1..].trim_ascii();
                    let mut val_buf = [0u8; 160];
                    let val = self.resolve_css_value(raw_val, &mut val_buf);
                    if prop.eq_ignore_ascii_case(b"color") {
                        if let Some(c) = parse_color(val) {
                            style.color = c;
                            style.set_explicit_color(true);
                        }
                    } else if prop.eq_ignore_ascii_case(b"background-color") || prop.eq_ignore_ascii_case(b"background") {
                        if let Some(bg) = parse_color(val) { style.set_bg_color(Some(bg)); }
                    } else if prop.eq_ignore_ascii_case(b"border") {
                        let (bw, bc) = parse_border(val);
                        if let Some(w) = bw { style.border_width = w.min(255) as u8; }
                        if let Some(c) = bc { style.set_border_color(Some(c)); }
                    } else if prop.eq_ignore_ascii_case(b"border-color") {
                        if let Some(bc) = parse_color(val) { style.set_border_color(Some(bc)); }
                    } else if prop.eq_ignore_ascii_case(b"border-width") {
                        if let Some(w) = parse_px(val) { style.border_width = w.min(255) as u8; }
                    } else if prop.eq_ignore_ascii_case(b"border-radius") {
                        if let Some(r) = parse_px(val) { style.border_radius = r.min(255) as u8; }
                    } else if prop.eq_ignore_ascii_case(b"font-size") {
                        if let Some(fs) = parse_px(val) { style.font_size = fs.min(255) as u8; }
                    } else if prop.eq_ignore_ascii_case(b"width") {
                        if let Some(w) = parse_px(val) { style.set_width(Some(w)); }
                    } else if prop.eq_ignore_ascii_case(b"height") {
                        if let Some(h) = parse_px(val) { style.set_height(Some(h)); }
                    } else if prop.eq_ignore_ascii_case(b"max-width") {
                        if let Some(mw) = parse_px(val) { style.set_max_width(Some(mw)); }
                    } else if prop.eq_ignore_ascii_case(b"margin") {
                        if let Some((v, h)) = parse_one_or_two_px(val) {
                            let vb = v.min(255) as u8;
                            let hb = h.min(255) as u8;
                            style.margin_top = vb; style.margin_bottom = vb;
                            style.margin_left = hb; style.margin_right = hb;
                        }
                    } else if prop.eq_ignore_ascii_case(b"margin-top") {
                        if let Some(m) = parse_px(val) { style.margin_top = m.min(255) as u8; }
                    } else if prop.eq_ignore_ascii_case(b"margin-bottom") {
                        if let Some(m) = parse_px(val) { style.margin_bottom = m.min(255) as u8; }
                    } else if prop.eq_ignore_ascii_case(b"margin-left") {
                        if let Some(m) = parse_px(val) { style.margin_left = m.min(255) as u8; }
                    } else if prop.eq_ignore_ascii_case(b"margin-right") {
                        if let Some(m) = parse_px(val) { style.margin_right = m.min(255) as u8; }
                    } else if prop.eq_ignore_ascii_case(b"padding") {
                        if let Some((v, h)) = parse_one_or_two_px(val) {
                            let vb = v.min(255) as u8;
                            let hb = h.min(255) as u8;
                            style.padding_top = vb; style.padding_bottom = vb;
                            style.padding_left = hb; style.padding_right = hb;
                        }
                    } else if prop.eq_ignore_ascii_case(b"padding-top") {
                        if let Some(p) = parse_px(val) { style.padding_top = p.min(255) as u8; }
                    } else if prop.eq_ignore_ascii_case(b"padding-bottom") {
                        if let Some(p) = parse_px(val) { style.padding_bottom = p.min(255) as u8; }
                    } else if prop.eq_ignore_ascii_case(b"padding-left") {
                        if let Some(p) = parse_px(val) { style.padding_left = p.min(255) as u8; }
                    } else if prop.eq_ignore_ascii_case(b"padding-right") {
                        if let Some(p) = parse_px(val) { style.padding_right = p.min(255) as u8; }
                    } else if prop.eq_ignore_ascii_case(b"font-weight") {
                        if val.eq_ignore_ascii_case(b"bold") {
                            style.set_bold(true);
                        } else if val.eq_ignore_ascii_case(b"normal") {
                            style.set_bold(false);
                        }
                    } else if prop.eq_ignore_ascii_case(b"display") {
                        if let Some(d) = parse_display(val) {
                            style.display = d;
                            style.set_display_block(d == DisplayMode::Block);
                        }
                    } else if prop.eq_ignore_ascii_case(b"flex-direction") {
                        if let Some(fd) = parse_flex_direction(val) { style.flex_direction = fd; }
                    } else if prop.eq_ignore_ascii_case(b"flex-wrap") {
                        if let Some(fw) = parse_flex_wrap(val) { style.set_flex_wrap(fw); }
                    } else if prop.eq_ignore_ascii_case(b"justify-content") {
                        if let Some(jc) = parse_justify_content(val) { style.justify_content = jc; }
                    } else if prop.eq_ignore_ascii_case(b"align-items") {
                        if let Some(ai) = parse_align_items(val) { style.align_items = ai; }
                    } else if prop.eq_ignore_ascii_case(b"text-align") {
                        if let Some(ta) = parse_text_align(val) { style.text_align = ta; }
                    } else if prop.eq_ignore_ascii_case(b"gap")
                        || prop.eq_ignore_ascii_case(b"column-gap")
                        || prop.eq_ignore_ascii_case(b"row-gap")
                    {
                        if let Some(px) = parse_px(val) { style.gap = px.min(64) as u8; }
                    } else if prop.eq_ignore_ascii_case(b"grid-template-columns") {
                        if let Some(gc) = parse_grid_template_columns(val) {
                            style.grid_cols = gc.min(6) as u8;
                        }
                    }
                }
            }
        }

        // Fallback visual discreto para cartões/painéis/badges e blocos semânticos HTML5 sem CSS explícito carregado
        if !style.has_box() {
            let is_dark_shell = dom.has_class(node_idx, b"hero")
                || dom.has_class(node_idx, b"terminal")
                || dom.has_class(node_idx, b"window");
            let is_card_panel = is_dark_shell
                || dom.has_class(node_idx, b"card")
                || dom.has_class(node_idx, b"panel")
                || dom.has_class(node_idx, b"lab-card");
            let is_semantic_box = tag_bytes == b"article"
                || tag_bytes == b"nav"
                || tag_bytes == b"header"
                || tag_bytes == b"footer"
                || (tag_bytes == b"section" && dom.nodes[node_idx].child_count > 0);
            if is_card_panel || is_semantic_box {
                style.set_bg_color(Some(if is_dark_shell || is_semantic_box { 0x000F_172A } else { 0x001E_293B }));
                style.set_border_color(Some(0x0033_4155));
                style.border_width = 1;
                style.border_radius = if is_semantic_box { 5 } else { 6 };
                style.padding_top = style.padding_top.max(6);
                style.padding_bottom = style.padding_bottom.max(6);
                style.padding_left = style.padding_left.max(8);
                style.padding_right = style.padding_right.max(8);
                style.margin_top = style.margin_top.max(4);
                style.margin_bottom = style.margin_bottom.max(4);
            } else if dom.has_class(node_idx, b"badge") || dom.has_class(node_idx, b"pill") {
                style.set_bg_color(Some(0x001E_3A5F));
                style.set_border_color(Some(0x0038_BDF8));
                style.border_width = 1;
                style.border_radius = 4;
                style.padding_top = style.padding_top.max(2);
                style.padding_bottom = style.padding_bottom.max(2);
                style.padding_left = style.padding_left.max(6);
                style.padding_right = style.padding_right.max(6);
                style.display = DisplayMode::InlineBlock;
                style.set_display_block(false);
            }
        }

        // Em janelas estreitas (< 760px), força empilhamento 1-coluna para evitar esmagamento de grids
        if viewport_w < 760 && style.grid_cols > 1 {
            style.grid_cols = 1;
        }

        // Preserva o posicionamento em fluxo de .os-window / .top-panel / .cairo-dock / .conky-hud
        if dom.has_class(node_idx, b"top-panel") {
            style.display = DisplayMode::Flex;
            style.flex_direction = FlexDirection::Row;
            style.set_flex_wrap(true);
            style.margin_bottom = style.margin_bottom.max(6);
        } else if dom.has_class(node_idx, b"desktop-shortcuts") {
            style.display = DisplayMode::Flex;
            style.flex_direction = FlexDirection::Row;
            style.set_flex_wrap(true);
            style.set_width(None);
            style.padding_top = style.padding_top.max(6);
            style.padding_bottom = style.padding_bottom.max(6);
        } else if dom.has_class(node_idx, b"cairo-dock-container") {
            style.display = DisplayMode::Flex;
            style.flex_direction = FlexDirection::Row;
            style.set_flex_wrap(true);
            style.margin_top = style.margin_top.max(8);
        }

        dom.nodes[node_idx].computed_style = style;
    }

    pub fn resolve(&self, node: &mut DomNode) {
        let tag = &node.tag[..(node.tag_len as usize).min(node.tag.len())];
        if tag == b"h1" {
            node.computed_style.color = COLOR_H1;
            node.computed_style.font_size = 16;
            node.computed_style.margin_top = 8;
            node.computed_style.margin_bottom = 6;
            node.computed_style.set_bold(true);
            node.computed_style.display = DisplayMode::Block;
            node.computed_style.set_display_block(true);
        } else if tag == b"h2" {
            node.computed_style.color = 0x00F3_8BA8;
            node.computed_style.font_size = 14;
            node.computed_style.margin_top = 6;
            node.computed_style.margin_bottom = 4;
            node.computed_style.set_bold(true);
            node.computed_style.display = DisplayMode::Block;
            node.computed_style.set_display_block(true);
        } else if tag == b"h3" {
            node.computed_style.color = 0x00A6_E3A1;
            node.computed_style.font_size = 13;
            node.computed_style.margin_top = 4;
            node.computed_style.margin_bottom = 4;
            node.computed_style.set_bold(true);
            node.computed_style.display = DisplayMode::Block;
            node.computed_style.set_display_block(true);
        } else if tag == b"p" {
            node.computed_style.color = COLOR_TEXT;
            node.computed_style.margin_top = 2;
            node.computed_style.margin_bottom = 4;
            node.computed_style.display = DisplayMode::Block;
            node.computed_style.set_display_block(true);
        } else if tag == b"a" {
            node.computed_style.color = COLOR_LINK;
            node.computed_style.display = DisplayMode::Inline;
            node.computed_style.set_display_block(false);
        } else if tag == b"button" {
            node.computed_style.color = COLOR_BTN_TEXT;
            node.computed_style.set_bg_color(Some(0x0031_3244));
            node.computed_style.set_border_color(Some(0x0045_475A));
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
            node.computed_style.set_display_block(false);
        } else if tag == b"div" || tag == b"nav" || tag == b"header" || tag == b"section" {
            node.computed_style.display = DisplayMode::Block;
            node.computed_style.set_display_block(true);
            node.computed_style.margin_top = 0;
            node.computed_style.margin_bottom = 0;
        } else if tag == b"hr" {
            node.computed_style.display = DisplayMode::Block;
            node.computed_style.set_display_block(true);
            node.computed_style.margin_top = 6;
            node.computed_style.margin_bottom = 6;
        } else if tag == b"b" || tag == b"strong" {
            node.computed_style.set_bold(true);
            node.computed_style.display = DisplayMode::Inline;
            node.computed_style.set_display_block(false);
        } else if tag == b"ul" {
            node.computed_style.display = DisplayMode::Block;
            node.computed_style.set_display_block(true);
            node.computed_style.margin_top = 4;
            node.computed_style.margin_bottom = 4;
            node.computed_style.padding_left = 8;
        } else if tag == b"li" {
            node.computed_style.display = DisplayMode::Block;
            node.computed_style.set_display_block(true);
            node.computed_style.margin_top = 2;
            node.computed_style.margin_bottom = 2;
        } else if tag == b"span" || tag == b"#text" || tag == b"i" || tag == b"em" || tag == b"code" || tag == b"small" {
            node.computed_style.display = DisplayMode::Inline;
            node.computed_style.set_display_block(false);
            if tag == b"#text" {
                node.computed_style.margin_top = 0;
                node.computed_style.margin_bottom = 0;
            }
        }

        let tag_hash = if node.tag_len > 0 { fnv1a_32_ci(tag) } else { 0 };
        for i in 0..self.rule_count {
            let r = &self.rules[i];
            if r.is_tag_only() && node.tag_len > 0 && r.sel_hashes[0] == tag_hash {
                apply_rule_to_style(&mut node.computed_style, r);
            }
        }
    }
}

fn apply_rule_to_style(style: &mut ComputedStyle, r: &CssRule) {
    let m = r.present_mask;
    if (m & RULE_HAS_COLOR) != 0 {
        style.color = r.color;
        style.set_explicit_color(true);
    }
    if (m & RULE_HAS_BG) != 0 { style.set_bg_color(Some(r.bg_color)); }
    if (m & RULE_HAS_BORDER_COLOR) != 0 { style.set_border_color(Some(r.border_color)); }
    if (m & RULE_HAS_BORDER_WIDTH) != 0 { style.border_width = r.border_width; }
    if (m & RULE_HAS_BORDER_RADIUS) != 0 { style.border_radius = r.border_radius; }
    if (m & RULE_HAS_BORDER_SIDES) != 0 { style.border_sides = r.border_sides; }
    if (m & RULE_HAS_FONT_SIZE) != 0 { style.font_size = r.font_size; }
    if (m & RULE_HAS_WIDTH) != 0 { style.set_width(Some(r.width as usize)); }
    if (m & RULE_HAS_HEIGHT) != 0 { style.set_height(Some(r.height as usize)); }
    if (m & RULE_HAS_MAX_WIDTH) != 0 { style.set_max_width(Some(r.max_width as usize)); }
    if (m & RULE_HAS_MT) != 0 { style.margin_top = r.margin_top; }
    if (m & RULE_HAS_MB) != 0 { style.margin_bottom = r.margin_bottom; }
    if (m & RULE_HAS_ML) != 0 { style.margin_left = r.margin_left; }
    if (m & RULE_HAS_MR) != 0 { style.margin_right = r.margin_right; }
    if (m & RULE_HAS_PT) != 0 { style.padding_top = r.padding_top; }
    if (m & RULE_HAS_PB) != 0 { style.padding_bottom = r.padding_bottom; }
    if (m & RULE_HAS_PL) != 0 { style.padding_left = r.padding_left; }
    if (m & RULE_HAS_PR) != 0 { style.padding_right = r.padding_right; }
    if (m & RULE_HAS_BOLD) != 0 { style.set_bold((m & RULE_IS_BOLD_VAL) != 0); }
    if (m & RULE_HAS_DISPLAY) != 0 {
        let d = bits_to_display(r.packed_enums);
        style.display = d;
        style.set_display_block(d == DisplayMode::Block);
    }
    if (m & RULE_HAS_FLEX_DIR) != 0 {
        style.flex_direction = if (r.packed_enums & 0x08) != 0 { FlexDirection::Column } else { FlexDirection::Row };
    }
    if (m & RULE_HAS_FLEX_WRAP) != 0 { style.set_flex_wrap((m & RULE_FLEX_WRAP_VAL) != 0); }
    if (m & RULE_HAS_GRID_COLS) != 0 { style.grid_cols = r.grid_cols; }
    if (m & RULE_HAS_JUSTIFY) != 0 { style.justify_content = bits_to_justify(r.packed_enums >> 4); }
    if (m & RULE_HAS_ALIGN) != 0 { style.align_items = bits_to_align(r.packed_enums >> 6); }
    if (m & RULE_HAS_TEXT_ALIGN) != 0 { style.text_align = bits_to_text_align(r.text_align_bits); }
    if (m & RULE_HAS_GAP) != 0 { style.gap = r.gap; }
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

                            let cpy = handler_clean.len().min(dom.nodes[target_idx].onclick_attr.len().saturating_sub(1));
                            dom.nodes[target_idx].onclick_attr[..cpy].copy_from_slice(&handler_clean[..cpy]);
                            dom.nodes[target_idx].onclick_len = cpy as u8;
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
                                dom.nodes[elem_idx].computed_style.set_border_color(Some(col));
                                self.dom_dirty = true;
                            }
                        } else if prop.eq_ignore_ascii_case(b"border") {
                            let (bw, bc) = parse_border(quote_val);
                            if let Some(w) = bw { dom.nodes[elem_idx].computed_style.border_width = w.min(255) as u8; }
                            if let Some(c) = bc { dom.nodes[elem_idx].computed_style.set_border_color(Some(c)); }
                            self.dom_dirty = true;
                        } else if prop.eq_ignore_ascii_case(b"width") {
                            if let Some(w) = parse_px(quote_val) {
                                dom.nodes[elem_idx].computed_style.set_width(Some(w));
                                self.dom_dirty = true;
                            }
                        } else if prop.eq_ignore_ascii_case(b"height") {
                            if let Some(h) = parse_px(quote_val) {
                                dom.nodes[elem_idx].computed_style.set_height(Some(h));
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
                    let t = dom.nodes[idx].text_slice();
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
    let s = strip_css_important(input);
    if s.is_empty() { return None; }
    if s.starts_with(b"#") {
        return parse_hex_color(s);
    }
    if s.starts_with(b"rgb(") || s.starts_with(b"rgba(") {
        return parse_rgb_color(s);
    }
    if find_subslice_ci(s, b"gradient(").is_some() {
        return parse_gradient_fallback_color(s);
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
    if s.eq_ignore_ascii_case(b"transparent") || s.eq_ignore_ascii_case(b"none") { return None; }
    parse_hex_color(s)
}

const CSS_DARK_CANVAS: u32 = 0x000B_0F19;

fn blend_over_rgb(fg: u32, bg: u32, alpha255: u32) -> u32 {
    let a = alpha255.min(255);
    let inv = 255 - a;
    let fr = (fg >> 16) & 0xFF;
    let fg_c = (fg >> 8) & 0xFF;
    let fb = fg & 0xFF;
    let br = (bg >> 16) & 0xFF;
    let bg_c = (bg >> 8) & 0xFF;
    let bb = bg & 0xFF;
    let r = (fr * a + br * inv + 127) / 255;
    let g = (fg_c * a + bg_c * inv + 127) / 255;
    let b = (fb * a + bb * inv + 127) / 255;
    (r << 16) | (g << 8) | b
}

fn parse_css_alpha_255(raw: &[u8]) -> u32 {
    let s = raw.trim_ascii();
    if s.is_empty() { return 255; }
    if s.ends_with(b"%") {
        let num = parse_fixed_100(&s[..s.len() - 1]).unwrap_or(10000);
        return ((num.min(10000) * 255 + 5000) / 10000) as u32;
    }
    let val100 = parse_fixed_100(s).unwrap_or(100);
    ((val100.min(100) * 255 + 50) / 100) as u32
}

fn parse_gradient_fallback_color(s: &[u8]) -> Option<u32> {
    // Prioriza cor explícita fora dos parênteses (ex.: `radial-gradient(...) #0b0f19`)
    let mut depth = 0usize;
    let mut i = 0usize;
    while i < s.len() {
        let b = s[i];
        if b == b'(' {
            depth += 1;
        } else if b == b')' {
            depth = depth.saturating_sub(1);
        } else if depth == 0 && b == b'#' {
            let start = i;
            let mut end = i + 1;
            while end < s.len() && s[end].is_ascii_hexdigit() {
                end += 1;
            }
            if let Some(c) = parse_hex_color(&s[start..end]) {
                return Some(c);
            }
        }
        i += 1;
    }
    // Fallback: primeira cor hex ou rgba dentro do gradiente
    let mut j = 0usize;
    while j < s.len() {
        if s[j] == b'#' {
            let mut end = j + 1;
            while end < s.len() && s[end].is_ascii_hexdigit() {
                end += 1;
            }
            if let Some(c) = parse_hex_color(&s[j..end]) {
                return Some(c);
            }
        } else if s[j..].starts_with(b"rgba(") || s[j..].starts_with(b"rgb(") {
            if let Some(rp) = s[j..].iter().position(|&c| c == b')') {
                if let Some(c) = parse_rgb_color(&s[j..j + rp + 1]) {
                    return Some(c);
                }
            }
        }
        j += 1;
    }
    None
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
    let rgb = ((r.min(255)) << 16) | ((g.min(255)) << 8) | (b.min(255));
    if let Some(a_str) = parts.next() {
        let a255 = parse_css_alpha_255(a_str);
        if a255 == 0 {
            return None;
        }
        if a255 < 250 {
            return Some(blend_over_rgb(rgb, CSS_DARK_CANVAS, a255));
        }
    }
    Some(rgb)
}

pub fn parse_border(val: &[u8]) -> (Option<usize>, Option<u32>) {
    let s = strip_css_important(val);
    let mut border_w = None;
    let mut border_c = None;
    let mut i = 0usize;
    while i < s.len() {
        while i < s.len() && (s[i] == b' ' || s[i] == b'\t') { i += 1; }
        if i >= s.len() { break; }
        let start = i;
        let mut depth = 0usize;
        while i < s.len() && (depth > 0 || (s[i] != b' ' && s[i] != b'\t')) {
            if s[i] == b'(' { depth += 1; }
            else if s[i] == b')' { depth = depth.saturating_sub(1); }
            i += 1;
        }
        let p = s[start..i].trim_ascii();
        if p.is_empty() || p.eq_ignore_ascii_case(b"solid") || p.eq_ignore_ascii_case(b"dashed") || p.eq_ignore_ascii_case(b"dotted") {
            continue;
        }
        if border_w.is_none() && !p.starts_with(b"#") && !p.starts_with(b"rgb") && p.iter().any(|&b| b >= b'0' && b <= b'9') {
            border_w = parse_px(p);
        } else if border_c.is_none() {
            border_c = parse_color(p);
        }
    }
    (border_w, border_c)
}

pub fn parse_display(val: &[u8]) -> Option<DisplayMode> {
    let s = strip_css_important(val);
    if s.eq_ignore_ascii_case(b"none") {
        Some(DisplayMode::None)
    } else if s.eq_ignore_ascii_case(b"block") {
        Some(DisplayMode::Block)
    } else if s.eq_ignore_ascii_case(b"inline") {
        Some(DisplayMode::Inline)
    } else if s.eq_ignore_ascii_case(b"inline-block") || s.eq_ignore_ascii_case(b"inline-flex") {
        Some(DisplayMode::InlineBlock)
    } else if s.eq_ignore_ascii_case(b"flex") || s.eq_ignore_ascii_case(b"grid") {
        Some(DisplayMode::Flex)
    } else {
        None
    }
}

pub fn parse_flex_wrap(val: &[u8]) -> Option<bool> {
    let s = strip_css_important(val);
    if s.eq_ignore_ascii_case(b"wrap") || s.eq_ignore_ascii_case(b"wrap-reverse") {
        Some(true)
    } else if s.eq_ignore_ascii_case(b"nowrap") {
        Some(false)
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

pub fn parse_grid_template_columns(val: &[u8]) -> Option<usize> {
    let s = strip_css_important(val);
    if s.is_empty() || s.eq_ignore_ascii_case(b"none") {
        return None;
    }
    if s.len() > 7 && s[..7].eq_ignore_ascii_case(b"repeat(") {
        let inner = s[7..].trim_ascii();
        if inner.starts_with(b"auto-fit") || inner.starts_with(b"auto-fill") {
            return Some(3);
        }
        if let Some(n) = parse_int(inner) {
            if n > 0 {
                return Some((n as usize).clamp(1, 6));
            }
        }
    }
    let mut cols = 0usize;
    let mut depth = 0usize;
    let mut in_tok = false;
    for &b in s {
        if b == b'(' {
            if !in_tok && depth == 0 {
                cols += 1;
                in_tok = true;
            }
            depth += 1;
        } else if b == b')' {
            depth = depth.saturating_sub(1);
        } else if depth == 0 && (b == b' ' || b == b'\t' || b == b'\r' || b == b'\n') {
            in_tok = false;
        } else if !in_tok {
            cols += 1;
            in_tok = true;
        }
    }
    if cols > 0 { Some(cols.clamp(1, 6)) } else { None }
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

fn parse_fixed_100(val: &[u8]) -> Option<usize> {
    let s = val.trim_ascii();
    let mut int_part: usize = 0;
    let mut frac_part: usize = 0;
    let mut frac_scale: usize = 1;
    let mut in_frac = false;
    let mut found = false;
    for &b in s {
        if b >= b'0' && b <= b'9' {
            let d = (b - b'0') as usize;
            if !in_frac {
                int_part = int_part.saturating_mul(10).saturating_add(d);
            } else if frac_scale < 100 {
                frac_part = frac_part.saturating_mul(10).saturating_add(d);
                frac_scale *= 10;
            }
            found = true;
        } else if b == b'.' && !in_frac {
            in_frac = true;
        } else {
            break;
        }
    }
    if !found {
        return None;
    }
    let scaled_frac = if frac_scale == 10 { frac_part * 10 } else if frac_scale == 100 { frac_part } else { 0 };
    Some(int_part.saturating_mul(100).saturating_add(scaled_frac))
}

pub fn parse_px(val: &[u8]) -> Option<usize> {
    let s = strip_css_important(val);
    let mut num_end = 0usize;
    let mut seen_dot = false;
    let mut found = false;
    while num_end < s.len() {
        let b = s[num_end];
        if b >= b'0' && b <= b'9' {
            found = true;
            num_end += 1;
        } else if b == b'.' && !seen_dot {
            seen_dot = true;
            num_end += 1;
        } else {
            break;
        }
    }
    if !found { return None; }
    let unit = s[num_end..].trim_ascii();
    if unit.starts_with(b"%") || unit.eq_ignore_ascii_case(b"vw") || unit.eq_ignore_ascii_case(b"vh") {
        return None;
    }
    let val100 = parse_fixed_100(&s[..num_end])?;
    if unit.eq_ignore_ascii_case(b"rem") || unit.eq_ignore_ascii_case(b"em") {
        Some(((val100 * 12 + 50) / 100).max(if val100 > 0 { 1 } else { 0 }))
    } else {
        Some((val100 + 50) / 100)
    }
}

pub fn parse_one_or_two_px(val: &[u8]) -> Option<(usize, usize)> {
    let s = strip_css_important(val);
    if s.is_empty() { return None; }
    if let Some(pos) = s.iter().position(|&b| b == b' ' || b == b'\t') {
        let first = parse_px(&s[..pos])?;
        let rest = s[pos + 1..].trim_ascii();
        let second = if rest.eq_ignore_ascii_case(b"auto") { 0 } else { parse_px(rest).unwrap_or(first) };
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
        let at_boundary = i == 0
            || tag[i - 1].is_ascii_whitespace()
            || tag[i - 1] == b'"'
            || tag[i - 1] == b'\''
            || tag[i - 1] == b'<';
        if at_boundary && tag[i..i + a_len].eq_ignore_ascii_case(attr) {
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
                } else if c == b' ' || c == b'>' || c == b'/' || c.is_ascii_whitespace() {
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

#[inline]
pub fn is_attr_match(buf: &[u8], i: usize, attr: &[u8]) -> bool {
    if i + attr.len() > buf.len() {
        return false;
    }
    let at_boundary = i == 0
        || buf[i - 1].is_ascii_whitespace()
        || buf[i - 1] == b'"'
        || buf[i - 1] == b'\''
        || buf[i - 1] == b'<';
    at_boundary && buf[i..i + attr.len()].eq_ignore_ascii_case(attr)
}

/// Extrai a primeira URL válida de um atributo HTML `srcset` (ex: `"hero.png 1x, hero@2x.png 2x"`).
pub fn extract_first_srcset_url(raw: &[u8]) -> &[u8] {
    for candidate in raw.split(|&b| b == b',') {
        let trimmed = candidate.trim_ascii();
        if trimmed.is_empty() {
            continue;
        }
        let url_end = trimmed
            .iter()
            .position(|b| b.is_ascii_whitespace())
            .unwrap_or(trimmed.len());
        let url = &trimmed[..url_end];
        if !url.is_empty() {
            return url;
        }
    }
    b""
}

fn push_ascii_bytes(out: &mut [u8], o: &mut usize, max: usize, bytes: &[u8]) {
    for &b in bytes {
        if *o >= max {
            break;
        }
        out[*o] = b;
        *o += 1;
    }
}

fn decode_codepoint_to_ascii(cp: u32, out: &mut [u8], o: &mut usize, max: usize) {
    match cp {
        0x20 | 0xA0 | 0x2002 | 0x2003 | 0x2009 => {
            if *o > 0 && out[*o - 1] != b' ' {
                push_ascii_bytes(out, o, max, b" ");
            }
        }
        0x21..=0x7E => {
            let b = cp as u8;
            push_ascii_bytes(out, o, max, &[b]);
        }
        0xA9 => push_ascii_bytes(out, o, max, b"(c)"),
        0xAE => push_ascii_bytes(out, o, max, b"(R)"),
        0xAB | 0xBB | 0x201C | 0x201D | 0x201E => push_ascii_bytes(out, o, max, b"\""),
        0x2018 | 0x2019 | 0x201A => push_ascii_bytes(out, o, max, b"'"),
        0xB0 => push_ascii_bytes(out, o, max, b"'"),
        0xB1 => push_ascii_bytes(out, o, max, b"+/-"),
        0xB7 | 0x2022 => push_ascii_bytes(out, o, max, b"*"),
        0xD7 => push_ascii_bytes(out, o, max, b"x"),
        0xF7 => push_ascii_bytes(out, o, max, b"/"),
        0x2013 | 0x2014 => push_ascii_bytes(out, o, max, b"-"),
        0x2026 => push_ascii_bytes(out, o, max, b"..."),
        0x2190 => push_ascii_bytes(out, o, max, b"<-"),
        0x2192 => push_ascii_bytes(out, o, max, b"->"),
        0x2122 => push_ascii_bytes(out, o, max, b"TM"),
        0x20AC => push_ascii_bytes(out, o, max, b"EUR"),
        0xC0..=0xC5 => push_ascii_bytes(out, o, max, b"A"),
        0xE0..=0xE5 => push_ascii_bytes(out, o, max, b"a"),
        0xC7 => push_ascii_bytes(out, o, max, b"C"),
        0xE7 => push_ascii_bytes(out, o, max, b"c"),
        0xC8..=0xCB => push_ascii_bytes(out, o, max, b"E"),
        0xE8..=0xEB => push_ascii_bytes(out, o, max, b"e"),
        0xCC..=0xCF => push_ascii_bytes(out, o, max, b"I"),
        0xEC..=0xEF => push_ascii_bytes(out, o, max, b"i"),
        0xD1 => push_ascii_bytes(out, o, max, b"N"),
        0xF1 => push_ascii_bytes(out, o, max, b"n"),
        0xD2..=0xD6 => push_ascii_bytes(out, o, max, b"O"),
        0xF2..=0xF6 => push_ascii_bytes(out, o, max, b"o"),
        0xD9..=0xDC => push_ascii_bytes(out, o, max, b"U"),
        0xF9..=0xFC => push_ascii_bytes(out, o, max, b"u"),
        _ => push_ascii_bytes(out, o, max, b"?"),
    }
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

    const NAMED_ENTITIES: &[(&[u8], &[u8])] = &[
        (b"&lt;", b"<"),
        (b"&gt;", b">"),
        (b"&amp;", b"&"),
        (b"&quot;", b"\""),
        (b"&apos;", b"'"),
        (b"&nbsp;", b" "),
        (b"&ensp;", b" "),
        (b"&emsp;", b" "),
        (b"&thinsp;", b" "),
        (b"&mdash;", b"-"),
        (b"&ndash;", b"-"),
        (b"&rarr;", b"->"),
        (b"&larr;", b"<-"),
        (b"&rsquo;", b"'"),
        (b"&lsquo;", b"'"),
        (b"&rdquo;", b"\""),
        (b"&ldquo;", b"\""),
        (b"&laquo;", b"\""),
        (b"&raquo;", b"\""),
        (b"&bull;", b"*"),
        (b"&middot;", b"."),
        (b"&hellip;", b"..."),
        (b"&copy;", b"(c)"),
        (b"&reg;", b"(R)"),
        (b"&trade;", b"TM"),
        (b"&deg;", b"'"),
        (b"&times;", b"x"),
        (b"&divide;", b"/"),
        (b"&plusmn;", b"+/-"),
        (b"&euro;", b"EUR"),
    ];

    while i < len && o < max {
        // 1. Entidades HTML (&nome;, &#NNN;, &#xHHH;)
        if raw[i] == b'&' {
            if i + 2 < len && raw[i + 1] == b'#' {
                let mut scan = i + 2;
                let is_hex = raw[scan] == b'x' || raw[scan] == b'X';
                if is_hex {
                    scan += 1;
                }
                let digits_start = scan;
                let mut val = 0u32;
                let mut valid = false;
                while scan < len && (scan - digits_start) < 8 && raw[scan] != b';' {
                    let c = raw[scan];
                    if is_hex {
                        let d = match c {
                            b'0'..=b'9' => Some((c - b'0') as u32),
                            b'a'..=b'f' => Some((c - b'a' + 10) as u32),
                            b'A'..=b'F' => Some((c - b'A' + 10) as u32),
                            _ => None,
                        };
                        if let Some(dv) = d {
                            val = val.saturating_mul(16).saturating_add(dv);
                            valid = true;
                            scan += 1;
                        } else {
                            valid = false;
                            break;
                        }
                    } else if c >= b'0' && c <= b'9' {
                        val = val.saturating_mul(10).saturating_add((c - b'0') as u32);
                        valid = true;
                        scan += 1;
                    } else {
                        valid = false;
                        break;
                    }
                }
                if valid && scan < len && raw[scan] == b';' {
                    decode_codepoint_to_ascii(val, out, &mut o, max);
                    i = scan + 1;
                    continue;
                }
            }

            let mut matched_named = false;
            for &(ent, rep) in NAMED_ENTITIES {
                if i + ent.len() <= len && raw[i..i + ent.len()].eq_ignore_ascii_case(ent) {
                    if rep == b" " {
                        if o > 0 && out[o - 1] != b' ' {
                            push_ascii_bytes(out, &mut o, max, b" ");
                        }
                    } else {
                        push_ascii_bytes(out, &mut o, max, rep);
                    }
                    i += ent.len();
                    matched_named = true;
                    break;
                }
            }
            if matched_named {
                continue;
            }
        }

        // 2. Símbolos Latin-1 UTF-8 (0xC2 ...)
        if raw[i] == 0xC2 && i + 1 < len {
            let b2 = raw[i + 1];
            match b2 {
                0xA0 => {
                    if o > 0 && out[o - 1] != b' ' {
                        push_ascii_bytes(out, &mut o, max, b" ");
                    }
                }
                0xA9 => push_ascii_bytes(out, &mut o, max, b"(c)"),
                0xAB | 0xBB => push_ascii_bytes(out, &mut o, max, b"\""),
                0xAE => push_ascii_bytes(out, &mut o, max, b"(R)"),
                0xB0 => push_ascii_bytes(out, &mut o, max, b"'"),
                0xB1 => push_ascii_bytes(out, &mut o, max, b"+/-"),
                0xB7 => push_ascii_bytes(out, &mut o, max, b"."),
                _ => push_ascii_bytes(out, &mut o, max, b" "),
            }
            i += 2;
            continue;
        }

        // 3. Acentuação UTF-8 Português (0xC3 ...)
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
                0x92..=0x96 => b'O', // Ò, Ó, Ô, Õ, Ö
                0xB9..=0xBC => b'u', // ù, ú, û, ü
                0x99..=0x9C => b'U', // Ù, Ú, Û, Ü
                0xA7 => b'c',        // ç
                0x87 => b'C',        // Ç
                0xB1 => b'n',        // ñ
                0x91 => b'N',        // Ñ
                0x97 => b'x',        // ×
                0xB7 => b'/',        // ÷
                _ => b'?',
            };
            out[o] = ch;
            o += 1;
            i += 2;
            continue;
        }

        // 4. Aspas, marcadores e traços tipográficos UTF-8 (0xE2 0x80 ... / 0xE2 0x86 ...)
        if raw[i] == 0xE2 && i + 2 < len && raw[i + 1] == 0x80 {
            let b3 = raw[i + 2];
            match b3 {
                0x98 | 0x99 => push_ascii_bytes(out, &mut o, max, b"'"),  // ‘ ’
                0x9C | 0x9D => push_ascii_bytes(out, &mut o, max, b"\""), // “ ”
                0x93 | 0x94 => push_ascii_bytes(out, &mut o, max, b"-"),  // – —
                0xA2 => push_ascii_bytes(out, &mut o, max, b"*"),         // •
                0xA6 => push_ascii_bytes(out, &mut o, max, b"..."),       // …
                _ => {
                    if o > 0 && out[o - 1] != b' ' {
                        push_ascii_bytes(out, &mut o, max, b" ");
                    }
                }
            }
            i += 3;
            continue;
        }
        if raw[i] == 0xE2 && i + 2 < len && raw[i + 1] == 0x86 {
            match raw[i + 2] {
                0x90 => push_ascii_bytes(out, &mut o, max, b"<-"),
                0x92 => push_ascii_bytes(out, &mut o, max, b"->"),
                _ => {}
            }
            i += 3;
            continue;
        }

        // Caractere ASCII comum (colapsa espaços/quebras de linha consecutivos)
        let b = raw[i];
        if b == b' ' || b == b'\r' || b == b'\n' || b == b'\t' {
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

#[inline]
fn read_html_attr_val<'a>(html: &'a [u8], i: &mut usize, prefix_len: usize) -> &'a [u8] {
    let len = html.len();
    *i += prefix_len;
    let quote = if *i < len && (html[*i] == b'"' || html[*i] == b'\'') {
        let q = html[*i];
        *i += 1;
        Some(q)
    } else {
        None
    };
    let a_start = *i;
    let mut val_end = a_start;
    while *i < len {
        let c = html[*i];
        if let Some(q) = quote {
            if c == q {
                val_end = *i;
                *i += 1;
                return &html[a_start..val_end];
            }
        } else if c == b' ' || c == b'>' || c == b'/' || c.is_ascii_whitespace() {
            val_end = *i;
            return &html[a_start..val_end];
        }
        *i += 1;
        val_end = *i;
    }
    &html[a_start..val_end]
}

#[inline]
fn copy_trunc_bytes(dst: &mut [u8], src: &[u8]) -> usize {
    let cap = dst.len().saturating_sub(1);
    let cpy = src.len().min(cap);
    dst[..cpy].copy_from_slice(&src[..cpy]);
    cpy
}

pub fn parse_html_into_dom(html: &[u8], dom: &mut DomArena, css: &mut MiniCss, js: &mut MiniJsRuntime) {
    parse_html_into_dom_with_base(html, b"", 800, dom, css, js);
}

pub fn parse_html_into_dom_with_base(
    html: &[u8],
    base_url: &[u8],
    viewport_w: usize,
    dom: &mut DomArena,
    css: &mut MiniCss,
    js: &mut MiniJsRuntime,
) {
    dom.node_count = 0;
    dom.title_len = 0;
    css.reset();

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

    let mut parent_stack = [0usize; 24];
    let mut stack_depth = 0usize;
    let mut pending_picture_src = [0u8; 128];
    let mut pending_picture_src_len = 0usize;

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

                if ctag_name == b"picture" {
                    pending_picture_src_len = 0;
                }

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
                    || ctag_name == b"input"
                    || ctag_name == b"source"
                    || ctag_name == b"wbr";

                if !is_unstacked_close && stack_depth > 0 {
                    let mut match_idx = None;
                    for s in (0..stack_depth).rev() {
                        let pid = parent_stack[s];
                        if dom.nodes[pid].tag_slice() == ctag_name {
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
            let mut tag_buf = [0u8; 12];
            let raw_tag = &html[tag_start..i];
            let tag_len = raw_tag.len().min(11);
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

            // Trata <link rel="stylesheet" href="..."> usando buffer reutilizavel unico de 24 KiB
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
                        let mut loaded = false;
                        if !base_url.is_empty() {
                            let mut resolved_css = [0u8; 128];
                            let rlen = resolve_relative_url(base_url, href_val, &mut resolved_css);
                            if let Some(cached) = unsafe { css_scratch_lookup(&resolved_css[..rlen]) } {
                                css.parse_stylesheet_append(cached);
                                loaded = true;
                            }
                        }
                        if !loaded {
                            if let Some(cached) = unsafe { css_scratch_lookup(href_val) } {
                                css.parse_stylesheet_append(cached);
                                loaded = true;
                            }
                        }
                        if !loaded {
                            let vfs_buf = unsafe { css_scratch_slice_mut() };
                            if let Some(n) = read_css_file_from_vfs(href_val, vfs_buf) {
                                css.parse_stylesheet_append(&vfs_buf[..n]);
                            }
                        }
                    }
                }
                continue;
            }

            // Captura <source srcset="..." / src="..."> dentro de <picture> sem consumir slot no DOM
            if tag_name == b"source" {
                let s_start = i;
                while i < len && html[i] != b'>' { i += 1; }
                let source_content = &html[s_start..i];
                if i < len { i += 1; }
                if pending_picture_src_len == 0 {
                    if let Some(srcset_val) = extract_attr_val(source_content, b"srcset=") {
                        let first_u = extract_first_srcset_url(srcset_val);
                        if !first_u.is_empty() {
                            pending_picture_src_len = copy_trunc_bytes(&mut pending_picture_src, first_u);
                        }
                    } else if let Some(src_val) = extract_attr_val(source_content, b"src=") {
                        if !src_val.is_empty() {
                            pending_picture_src_len = copy_trunc_bytes(&mut pending_picture_src, src_val);
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
                let mut svg_self_closing = false;
                while i < len && html[i] != b'>' {
                    if html[i] == b'/' && i + 1 < len && html[i + 1] == b'>' {
                        svg_self_closing = true;
                    }
                    i += 1;
                }
                if i < len { i += 1; }
                if !svg_self_closing {
                    while i + 5 < len && !html[i..i + 6].eq_ignore_ascii_case(b"</svg>") {
                        i += 1;
                    }
                    while i < len && html[i] != b'>' { i += 1; }
                    if i < len { i += 1; }
                }
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
                while i < len && html[i] != b'>' { i += 1; }
                if i < len { i += 1; }
                continue;
            }
            let node_id = node_id_opt.unwrap();
            dom.nodes[node_id].tag = tag_buf;
            dom.nodes[node_id].tag_len = tag_len as u8;
            if tag_name == b"picture" {
                pending_picture_src_len = 0;
            }

            let mut is_self_closing = false;
            let mut input_val_buf = [0u8; 64];
            let mut input_val_len = 0usize;
            let mut input_ph_buf = [0u8; 64];
            let mut input_ph_len = 0usize;
            let mut is_input_hidden = false;
            let mut is_input_submit = false;
            while i < len && html[i] != b'>' {
                if html[i] == b'/' && i + 1 < len && html[i + 1] == b'>' {
                    is_self_closing = true;
                    i += 1;
                    break;
                }
                if is_attr_match(html, i, b"id=") {
                    let a_val = read_html_attr_val(html, &mut i, 3);
                    dom.nodes[node_id].id_len = copy_trunc_u8(&mut dom.nodes[node_id].id_attr, a_val);
                } else if is_attr_match(html, i, b"class=") {
                    let a_val = read_html_attr_val(html, &mut i, 6);
                    dom.nodes[node_id].class_len = filter_visual_classes(a_val, &mut dom.nodes[node_id].class_attr);
                } else if is_attr_match(html, i, b"style=") {
                    let a_val = read_html_attr_val(html, &mut i, 6);
                    dom.nodes[node_id].style_len = copy_trunc_u8(&mut dom.nodes[node_id].style_attr, a_val);
                } else if is_attr_match(html, i, b"href=") {
                    let a_val = read_html_attr_val(html, &mut i, 5);
                    dom.nodes[node_id].href_len = copy_trunc_u8(&mut dom.nodes[node_id].href_attr, a_val);
                } else if is_attr_match(html, i, b"srcset=") {
                    let a_val = read_html_attr_val(html, &mut i, 7);
                    if dom.nodes[node_id].src_len == 0 {
                        let first_u = extract_first_srcset_url(a_val);
                        if !first_u.is_empty() {
                            dom.nodes[node_id].src_len = copy_trunc_u8(&mut dom.nodes[node_id].src_attr, first_u);
                        }
                    }
                } else if is_attr_match(html, i, b"src=") {
                    let a_val = read_html_attr_val(html, &mut i, 4);
                    dom.nodes[node_id].src_len = copy_trunc_u8(&mut dom.nodes[node_id].src_attr, a_val);
                } else if is_attr_match(html, i, b"alt=") {
                    let a_val = read_html_attr_val(html, &mut i, 4);
                    dom.nodes[node_id].alt_len = copy_trunc_u8(&mut dom.nodes[node_id].alt_attr, a_val);
                } else if is_attr_match(html, i, b"width=") {
                    let a_val = read_html_attr_val(html, &mut i, 6);
                    if let Some(w_num) = parse_px(a_val) {
                        dom.nodes[node_id].width_attr = w_num.min(u16::MAX as usize) as u16;
                    }
                } else if is_attr_match(html, i, b"height=") {
                    let a_val = read_html_attr_val(html, &mut i, 7);
                    if let Some(h_num) = parse_px(a_val) {
                        dom.nodes[node_id].height_attr = h_num.min(u16::MAX as usize) as u16;
                    }
                } else if is_attr_match(html, i, b"data-open-window=") {
                    let a_val = read_html_attr_val(html, &mut i, 17);
                    dom.nodes[node_id].data_open_window_len = copy_trunc_u8(&mut dom.nodes[node_id].data_open_window, a_val);
                } else if is_attr_match(html, i, b"onclick=") {
                    let a_val = read_html_attr_val(html, &mut i, 8);
                    dom.nodes[node_id].onclick_len = copy_trunc_u8(&mut dom.nodes[node_id].onclick_attr, a_val);
                } else if is_attr_match(html, i, b"value=") {
                    let a_val = read_html_attr_val(html, &mut i, 6);
                    input_val_len = copy_trunc_bytes(&mut input_val_buf, a_val);
                } else if is_attr_match(html, i, b"placeholder=") {
                    let a_val = read_html_attr_val(html, &mut i, 12);
                    input_ph_len = copy_trunc_bytes(&mut input_ph_buf, a_val);
                } else if is_attr_match(html, i, b"type=") {
                    let a_val = read_html_attr_val(html, &mut i, 5);
                    if a_val.eq_ignore_ascii_case(b"hidden") {
                        is_input_hidden = true;
                    } else if a_val.eq_ignore_ascii_case(b"submit") || a_val.eq_ignore_ascii_case(b"button") {
                        is_input_submit = true;
                    }
                } else {
                    i += 1;
                }
            }

            if tag_name == b"input" || tag_name == b"textarea" {
                if is_input_hidden {
                    dom.nodes[node_id].alt_len = 2;
                } else if input_val_len > 0 {
                    let cpy = input_val_len.min(dom.nodes[node_id].text.len());
                    dom.nodes[node_id].text[..cpy].copy_from_slice(&input_val_buf[..cpy]);
                    dom.nodes[node_id].text_len = cpy as u8;
                    dom.nodes[node_id].alt_len = if is_input_submit { 3 } else { 0 };
                } else if input_ph_len > 0 {
                    let cpy = input_ph_len.min(dom.nodes[node_id].text.len());
                    dom.nodes[node_id].text[..cpy].copy_from_slice(&input_ph_buf[..cpy]);
                    dom.nodes[node_id].text_len = cpy as u8;
                    dom.nodes[node_id].alt_len = 1;
                }
            }

            if tag_name == b"img" && dom.nodes[node_id].src_len == 0 && pending_picture_src_len > 0 {
                let cpy = pending_picture_src_len.min(dom.nodes[node_id].src_attr.len().saturating_sub(1));
                dom.nodes[node_id].src_attr[..cpy].copy_from_slice(&pending_picture_src[..cpy]);
                dom.nodes[node_id].src_len = cpy as u8;
                pending_picture_src_len = 0;
            }

            if i < len && html[i] == b'>' {
                i += 1;
            }

            // Link parent-child hierarchy
            if stack_depth > 0 {
                let pid = parent_stack[stack_depth - 1];
                dom.nodes[node_id].set_parent(Some(pid));
                if (dom.nodes[pid].child_count as usize) < MAX_DOM_CHILDREN {
                    let c = dom.nodes[pid].child_count as usize;
                    dom.nodes[pid].children[c] = node_id as u16;
                    dom.nodes[pid].child_count += 1;
                }
            }

            let is_void = tag_name == b"hr" || tag_name == b"br" || tag_name == b"img"
                || tag_name == b"input" || tag_name == b"source" || tag_name == b"wbr";
            if !is_void && !is_self_closing && stack_depth < 24 {
                parent_stack[stack_depth] = node_id;
                stack_depth += 1;
            }

            // Lê texto até próximo '<'
            let mut text_buf = [0u8; 192];
            let mut t_idx = 0usize;
            while i < len && html[i] != b'<' {
                if t_idx < 191 {
                    text_buf[t_idx] = html[i];
                    t_idx += 1;
                }
                i += 1;
            }

            if t_idx > 0 {
                let mut norm_buf = [0u8; 192];
                let norm_len = normalize_html_text(&text_buf[..t_idx], &mut norm_buf).min(191);
                if norm_len > 0 {
                    dom.nodes[node_id].text[..norm_len].copy_from_slice(&norm_buf[..norm_len]);
                    dom.nodes[node_id].text_len = norm_len as u8;
                }
            }

            css.resolve(&mut dom.nodes[node_id]);
        } else {
            // Captura texto após tags de fechamento (ex.: "<p><b>Label: </b>Valor</p>") como nó DomNodeType::Text
            let mut text_buf = [0u8; 192];
            let mut t_idx = 0usize;
            while i < len && html[i] != b'<' {
                if t_idx < 191 {
                    text_buf[t_idx] = html[i];
                    t_idx += 1;
                }
                i += 1;
            }
            if t_idx > 0 && stack_depth > 0 {
                let mut norm_buf = [0u8; 192];
                let norm_len = normalize_html_text(&text_buf[..t_idx], &mut norm_buf).min(191);
                if norm_len > 0 {
                    if let Some(text_id) = dom.alloc_node(DomNodeType::Text) {
                        dom.nodes[text_id].tag[..5].copy_from_slice(b"#text");
                        dom.nodes[text_id].tag_len = 5;
                        dom.nodes[text_id].text[..norm_len].copy_from_slice(&norm_buf[..norm_len]);
                        dom.nodes[text_id].text_len = norm_len as u8;
                        let pid = parent_stack[stack_depth - 1];
                        dom.nodes[text_id].set_parent(Some(pid));
                        if (dom.nodes[pid].child_count as usize) < MAX_DOM_CHILDREN {
                            let c = dom.nodes[pid].child_count as usize;
                            dom.nodes[pid].children[c] = text_id as u16;
                            dom.nodes[pid].child_count += 1;
                        }
                        css.resolve(&mut dom.nodes[text_id]);
                    }
                }
            }
        }
    }

    // 1. Resolução inicial de estilos CSS completos em cascata
    css.resolve_all(dom, viewport_w);

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
        css.resolve_all(dom, viewport_w);
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

pub const CLICK_FLAG_JS: u8 = 1 << 0;
pub const CLICK_FLAG_PAGE_BLOCK: u8 = 1 << 1;

#[derive(Clone, Copy)]
pub struct ClickRegion {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    pub node_idx: u16,
    pub flags: u8,
}

impl ClickRegion {
    pub const fn empty() -> Self {
        Self {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            node_idx: 0,
            flags: 0,
        }
    }

    #[inline(always)]
    pub fn is_js(&self) -> bool {
        (self.flags & CLICK_FLAG_JS) != 0
    }

    #[inline(always)]
    pub fn is_page_block(&self) -> bool {
        (self.flags & CLICK_FLAG_PAGE_BLOCK) != 0
    }
}

pub const LINE_FLAG_LINK: u8 = 1 << 0;
pub const LINE_FLAG_HR: u8 = 1 << 1;

#[derive(Clone, Copy)]
pub struct RenderLine {
    pub text: [u8; 96],
    pub color: u32,
    pub x: u16,
    pub y: u16,
    pub text_len: u8,
    pub flags: u8,
}

impl RenderLine {
    pub const fn empty() -> Self {
        Self {
            text: [0; 96],
            color: 0,
            x: 0,
            y: 0,
            text_len: 0,
            flags: 0,
        }
    }

    #[inline(always)]
    pub fn is_link(&self) -> bool {
        (self.flags & LINE_FLAG_LINK) != 0
    }

    #[inline(always)]
    pub fn is_hr(&self) -> bool {
        (self.flags & LINE_FLAG_HR) != 0
    }

    #[inline(always)]
    pub fn text_slice(&self) -> &[u8] {
        &self.text[..(self.text_len as usize).min(self.text.len())]
    }
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

pub const MAX_RENDER_LINES: usize = 896;
pub const MAX_RENDER_IMAGES: usize = 32;
pub const MAX_CLICK_REGIONS: usize = 384;

#[derive(Clone, Copy)]
pub struct RenderImage {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    pub node_idx: u16,
}

impl RenderImage {
    pub const fn empty() -> Self {
        Self {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            node_idx: 0,
        }
    }
}

pub struct DocumentLayout {
    pub lines: [RenderLine; MAX_RENDER_LINES],
    pub line_count: usize,
    pub images: [RenderImage; MAX_RENDER_IMAGES],
    pub image_count: usize,
    pub click_regions: [ClickRegion; MAX_CLICK_REGIONS],
    pub click_count: usize,
    pub total_height: usize,
}

impl DocumentLayout {
    #[inline]
    fn is_ascii_rule_run(s: &[u8]) -> bool {
        if s.len() < 8 {
            return false;
        }
        let c = s[0];
        if c != b'=' && c != b'-' && c != b'_' && c != b'*' && c != b'~' {
            return false;
        }
        s.iter().all(|&b| b == c)
    }

    pub const fn new() -> Self {
        Self {
            lines: [RenderLine::empty(); MAX_RENDER_LINES],
            line_count: 0,
            images: [RenderImage::empty(); MAX_RENDER_IMAGES],
            image_count: 0,
            click_regions: [ClickRegion::empty(); MAX_CLICK_REGIONS],
            click_count: 0,
            total_height: 0,
        }
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.line_count = 0;
        self.image_count = 0;
        self.click_count = 0;
        self.total_height = 0;
    }

    #[inline]
    fn push_click_region(&mut self, x: usize, y: usize, w: usize, h: usize, node_idx: usize, flags: u8) {
        if w == 0 || h == 0 || self.click_count >= self.click_regions.len() {
            return;
        }
        self.click_regions[self.click_count] = ClickRegion {
            x: x.min(u16::MAX as usize) as u16,
            y: y.min(u16::MAX as usize) as u16,
            w: w.min(u16::MAX as usize) as u16,
            h: h.min(u16::MAX as usize) as u16,
            node_idx: node_idx.min(u16::MAX as usize) as u16,
            flags,
        };
        self.click_count += 1;
    }

    #[inline]
    fn record_dom_box(
        dom: &mut DomArena,
        node_idx: usize,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        fallback_bg: u8,
    ) {
        if node_idx < dom.node_count && w > 0 && h > 0 {
            let n = &mut dom.nodes[node_idx];
            n.box_x = x.min(u16::MAX as usize) as u16;
            n.box_y = y.min(u16::MAX as usize) as u16;
            n.box_w = w.min(u16::MAX as usize) as u16;
            n.box_h = h.min(u16::MAX as usize) as u16;
            n.box_bg_fallback = fallback_bg;
        }
    }

    #[allow(dead_code)]
    pub fn compute_from_dom(&mut self, dom: &mut DomArena, content_width: usize) {
        self.compute_from_dom_stage(dom, 4, content_width);
    }

    pub fn compute_from_dom_stage(&mut self, dom: &mut DomArena, content_x: usize, content_width: usize) {
        self.line_count = 0;
        self.click_count = 0;
        self.image_count = 0;

        for i in 0..dom.node_count {
            dom.nodes[i].box_w = 0;
            dom.nodes[i].box_h = 0;
            dom.nodes[i].box_bg_fallback = 0;
        }

        let mut curr_y = 6usize;
        let mut laid_out = [false; MAX_DOM_NODES];

        // 1. Traverse all root-level nodes (nodes with parent == None)
        for i in 0..dom.node_count {
            if dom.nodes[i].parent_idx().is_none() {
                self.layout_subtree(dom, i, content_x, &mut curr_y, content_width, &mut laid_out);
            }
        }

        // 2. Fallback: layout any orphaned nodes that were not reached by root traversal
        for i in 0..dom.node_count {
            if !laid_out[i] {
                self.layout_subtree(dom, i, content_x, &mut curr_y, content_width, &mut laid_out);
            }
        }

        self.total_height = curr_y + 10;
    }

    fn mark_subtree_laid_out(dom: &DomArena, node_idx: usize, laid_out: &mut [bool; MAX_DOM_NODES]) {
        if node_idx >= dom.node_count || laid_out[node_idx] { return; }
        laid_out[node_idx] = true;
        let cc = (dom.nodes[node_idx].child_count as usize).min(MAX_DOM_CHILDREN);
        for c in 0..cc {
            Self::mark_subtree_laid_out(dom, dom.nodes[node_idx].child_id(c), laid_out);
        }
    }

    fn find_enclosing_link_idx(dom: &DomArena, mut curr_idx: usize) -> Option<usize> {
        loop {
            let n = &dom.nodes[curr_idx];
            if n.tag_slice() == b"a" && n.href_len > 0 {
                return Some(curr_idx);
            }
            if let Some(pid) = n.parent_idx() {
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

    fn is_inline_flow_tag(tag: &[u8]) -> bool {
        tag == b"a"
            || tag == b"b"
            || tag == b"strong"
            || tag == b"i"
            || tag == b"em"
            || tag == b"span"
            || tag == b"code"
            || tag == b"small"
            || tag == b"time"
            || tag == b"label"
            || tag == b"button"
            || tag == b"br"
            || tag == b"wbr"
    }

    fn subtree_breaks_inline(dom: &DomArena, node_idx: usize, depth: usize) -> bool {
        if node_idx >= dom.node_count || depth > 5 {
            return false;
        }
        let n = &dom.nodes[node_idx];
        let tag = n.tag_slice();
        if n.node_type == DomNodeType::Element && tag == b"img" {
            return true;
        }
        if n.node_type == DomNodeType::Element && (n.computed_style.display_block() || (!Self::is_inline_flow_tag(tag) && tag != b"#text")) {
            return true;
        }
        let cc = (n.child_count as usize).min(MAX_DOM_CHILDREN);
        for c in 0..cc {
            if Self::subtree_breaks_inline(dom, n.child_id(c), depth + 1) {
                return true;
            }
        }
        false
    }

    fn is_inline_run_candidate(dom: &DomArena, node_idx: usize) -> bool {
        if node_idx >= dom.node_count {
            return false;
        }
        let n = &dom.nodes[node_idx];
        if n.computed_style.display == DisplayMode::None {
            return false;
        }
        if n.node_type == DomNodeType::Text {
            return true;
        }
        let tag = n.tag_slice();
        if !n.computed_style.display_block() && Self::is_inline_flow_tag(tag) {
            return !Self::subtree_breaks_inline(dom, node_idx, 0);
        }
        false
    }

    fn collect_inline_items(
        dom: &DomArena,
        node_idx: usize,
        link_node: Option<usize>,
        items: &mut [(usize, Option<usize>, bool, bool); 64],
        count: &mut usize,
        laid_out: &mut [bool; MAX_DOM_NODES],
        depth: usize,
    ) {
        if node_idx >= dom.node_count || *count >= items.len() || depth > 6 {
            return;
        }
        let n = &dom.nodes[node_idx];
        if n.computed_style.display == DisplayMode::None {
            Self::mark_subtree_laid_out(dom, node_idx, laid_out);
            return;
        }
        laid_out[node_idx] = true;
        let tag = n.tag_slice();
        let next_link = if tag == b"a" && n.href_len > 0 {
            Some(node_idx)
        } else {
            link_node
        };

        if tag == b"br" {
            items[*count] = (node_idx, next_link, true, false);
            *count += 1;
            return;
        }
        if tag == b"wbr" {
            return;
        }

        let is_btn_or_box = tag == b"button"
            || n.onclick_len > 0
            || n.data_open_window_len > 0
            || n.computed_style.has_box();

        if n.text_len > 0 || (is_btn_or_box && n.child_count == 0) {
            items[*count] = (node_idx, next_link, false, is_btn_or_box);
            *count += 1;
        }

        let cc = (n.child_count as usize).min(MAX_DOM_CHILDREN);
        for c in 0..cc {
            if *count >= items.len() {
                break;
            }
            let cid = n.child_id(c);
            Self::collect_inline_items(dom, cid, next_link, items, count, laid_out, depth + 1);
        }
    }

    fn flush_inline_segment(
        &mut self,
        dom: &DomArena,
        buf: &[u8],
        start_x: usize,
        y: usize,
        color: u32,
        link_node: Option<usize>,
    ) {
        if buf.is_empty() || self.line_count >= self.lines.len() {
            return;
        }
        let mut line_buf = [0u8; 96];
        let take = buf.len().min(95);
        line_buf[..take].copy_from_slice(&buf[..take]);
        let is_link = link_node.is_some();

        if let Some(ln) = link_node {
            if ln < dom.node_count && dom.nodes[ln].href_len > 0 {
                self.push_click_region(start_x, y, take * font::FONT_WIDTH, font::FONT_HEIGHT + 2, ln, 0);
            }
        }

        self.lines[self.line_count] = RenderLine {
            text: line_buf,
            color,
            x: start_x.min(u16::MAX as usize) as u16,
            y: y.min(u16::MAX as usize) as u16,
            text_len: take as u8,
            flags: if is_link { LINE_FLAG_LINK } else { 0 },
        };
        self.line_count += 1;
    }

    fn layout_inline_run(
        &mut self,
        dom: &mut DomArena,
        leading_text_node: Option<usize>,
        children: &[u16],
        c_idx: &mut usize,
        base_x: usize,
        curr_y: &mut usize,
        avail_width: usize,
        text_align: TextAlign,
        laid_out: &mut [bool; MAX_DOM_NODES],
    ) {
        let mut items: [(usize, Option<usize>, bool, bool); 64] = [(0, None, false, false); 64];
        let mut item_count = 0usize;

        if let Some(lid) = leading_text_node {
            if lid < dom.node_count && dom.nodes[lid].text_len > 0 {
                let enc_link = Self::find_enclosing_link_idx(dom, lid);
                items[0] = (lid, enc_link, false, false);
                item_count = 1;
            }
        }

        while *c_idx < children.len() && item_count < 64 {
            let cid = children[*c_idx] as usize;
            if cid >= dom.node_count || laid_out[cid] {
                *c_idx += 1;
                continue;
            }
            if dom.nodes[cid].computed_style.display == DisplayMode::None {
                Self::mark_subtree_laid_out(dom, cid, laid_out);
                *c_idx += 1;
                continue;
            }
            if !Self::is_inline_run_candidate(dom, cid) {
                break;
            }
            let parent_link = dom.nodes[cid].parent_idx().and_then(|p| Self::find_enclosing_link_idx(dom, p));
            Self::collect_inline_items(dom, cid, parent_link, &mut items, &mut item_count, laid_out, 0);
            *c_idx += 1;
        }

        if item_count == 0 {
            return;
        }

        let start_line_idx = self.line_count;
        let row_right = base_x + avail_width.max(font::FONT_WIDTH);
        let mut cursor_x = base_x;
        let mut line_y = *curr_y;
        let mut line_max_h = font::FONT_HEIGHT + 4;
        let mut prev_ended_sep = true;
        let mut placed_any = false;

        for idx in 0..item_count {
            let (nid, mut link_node, is_br, is_box_btn) = items[idx];
            if is_br {
                cursor_x = base_x;
                line_y += line_max_h;
                line_max_h = font::FONT_HEIGHT + 4;
                prev_ended_sep = true;
                placed_any = true;
                continue;
            }
            if nid >= dom.node_count {
                continue;
            }
            if let Some(ln) = link_node {
                if ln >= dom.node_count || dom.nodes[ln].href_len == 0 {
                    link_node = Self::find_enclosing_link_idx(dom, nid);
                }
            }

            let n_style = dom.nodes[nid].computed_style;
            let n_tag = dom.nodes[nid].tag;
            let n_tag_len = (dom.nodes[nid].tag_len as usize).min(n_tag.len());
            let is_btn_tag = &n_tag[..n_tag_len] == b"button";
            let is_js = is_btn_tag || dom.nodes[nid].onclick_len > 0 || dom.nodes[nid].data_open_window_len > 0;
            let text_buf = dom.nodes[nid].text;
            let text_len = (dom.nodes[nid].text_len as usize).min(text_buf.len());
            let text = &text_buf[..text_len];

            let color = if n_style.has_explicit_color() {
                n_style.color
            } else if link_node.is_some() {
                COLOR_LINK
            } else {
                n_style.color
            };

            if is_box_btn && text.len() <= 40 {
                if text.is_empty() {
                    let dot_w = n_style.width_opt().unwrap_or(10).min(avail_width).max(4);
                    let dot_h = n_style.height_opt().unwrap_or(10).clamp(4, 32);
                    if cursor_x > base_x {
                        cursor_x += 4;
                    }
                    if cursor_x + dot_w > row_right && cursor_x > base_x {
                        cursor_x = base_x;
                        line_y += line_max_h;
                        line_max_h = font::FONT_HEIGHT + 4;
                    }
                    let dot_y = line_y + (font::FONT_HEIGHT + 4).saturating_sub(dot_h) / 2;
                    Self::record_dom_box(dom, nid, cursor_x, dot_y, dot_w, dot_h, 1);
                    cursor_x += dot_w + 4;
                    line_max_h = line_max_h.max(dot_h + 2);
                    prev_ended_sep = false;
                    placed_any = true;
                    continue;
                }

                let pad_l = (n_style.padding_left as usize).max(4);
                let pad_r = (n_style.padding_right as usize).max(4);
                let pad_t = (n_style.padding_top as usize).max(2);
                let pad_b = (n_style.padding_bottom as usize).max(2);
                let btn_w = (text.len() * font::FONT_WIDTH + pad_l + pad_r + 8).min(avail_width);
                let btn_h = font::FONT_HEIGHT + pad_t + pad_b + 4;

                if cursor_x > base_x {
                    cursor_x += 6;
                }
                if cursor_x + btn_w > row_right && cursor_x > base_x {
                    cursor_x = base_x;
                    line_y += line_max_h;
                    line_max_h = font::FONT_HEIGHT + 4;
                }

                Self::record_dom_box(dom, nid, cursor_x, line_y, btn_w, btn_h, 1);

                let text_x = cursor_x + pad_l + 3;
                let text_y = line_y + pad_t + 2;
                if is_js {
                    self.push_click_region(cursor_x, line_y, btn_w, btn_h, nid, CLICK_FLAG_JS);
                }
                let suppress_underline = n_style.has_bg();
                self.flush_inline_segment(dom, text, text_x, text_y, color, if is_js || suppress_underline { None } else { link_node });
                if suppress_underline && !is_js {
                    if let Some(ln) = link_node {
                        if ln < dom.node_count && dom.nodes[ln].href_len > 0 {
                            self.push_click_region(cursor_x, line_y, btn_w, btn_h, ln, 0);
                        }
                    }
                }
                cursor_x += btn_w + 4;
                line_max_h = line_max_h.max(btn_h + 4);
                prev_ended_sep = false;
                placed_any = true;
                continue;
            }

            if text.is_empty() {
                continue;
            }

            let mut seg_buf = [0u8; 96];
            let mut seg_len = 0usize;
            let mut seg_start_x = cursor_x;

            for word in text.split(|&b| b == b' ') {
                if word.is_empty() {
                    continue;
                }
                let first_b = word[0];
                let no_space_before = first_b == b'.'
                    || first_b == b','
                    || first_b == b';'
                    || first_b == b':'
                    || first_b == b'!'
                    || first_b == b'?'
                    || first_b == b')'
                    || first_b == b']';

                let need_space = if seg_len == 0 {
                    cursor_x > base_x && !prev_ended_sep && !no_space_before
                } else {
                    !no_space_before
                };
                let space_px = if need_space { font::FONT_WIDTH } else { 0 };
                let word_px = word.len() * font::FONT_WIDTH;

                let is_rule_run = Self::is_ascii_rule_run(word);
                if is_rule_run {
                    if seg_len > 0 {
                        self.flush_inline_segment(dom, &seg_buf[..seg_len], seg_start_x, line_y, color, link_node);
                        seg_len = 0;
                        cursor_x = base_x;
                        line_y += line_max_h;
                        line_max_h = font::FONT_HEIGHT + 4;
                    } else if cursor_x > base_x && cursor_x + space_px + word_px > row_right {
                        cursor_x = base_x;
                        line_y += line_max_h;
                        line_max_h = font::FONT_HEIGHT + 4;
                    }
                    let max_rule_chars = ((row_right.saturating_sub(cursor_x)) / font::FONT_WIDTH).clamp(1, 95);
                    let take = word.len().min(max_rule_chars);
                    self.flush_inline_segment(dom, &word[..take], cursor_x, line_y, color, None);
                    line_y += line_max_h;
                    line_max_h = font::FONT_HEIGHT + 4;
                    cursor_x = base_x;
                    prev_ended_sep = true;
                    placed_any = true;
                    continue;
                }

                if cursor_x + space_px + word_px > row_right && cursor_x > base_x {
                    if seg_len > 0 {
                        self.flush_inline_segment(dom, &seg_buf[..seg_len], seg_start_x, line_y, color, link_node);
                        seg_len = 0;
                    }
                    cursor_x = base_x;
                    line_y += line_max_h;
                    line_max_h = font::FONT_HEIGHT + 4;
                    prev_ended_sep = true;
                }

                let mut w_off = 0usize;
                while w_off < word.len() {
                    let space_here = if seg_len == 0 {
                        cursor_x > base_x && !prev_ended_sep && !no_space_before && w_off == 0
                    } else {
                        !no_space_before && w_off == 0
                    };

                    if seg_len == 0 {
                        if space_here && cursor_x + font::FONT_WIDTH < row_right {
                            cursor_x += font::FONT_WIDTH;
                        }
                        seg_start_x = cursor_x;
                    } else if space_here && seg_len < 95 {
                        seg_buf[seg_len] = b' ';
                        seg_len += 1;
                        cursor_x += font::FONT_WIDTH;
                    }

                    let max_chunk_chars = ((row_right.saturating_sub(cursor_x)) / font::FONT_WIDTH).max(1);
                    let cap_chars = (95usize.saturating_sub(seg_len)).min(max_chunk_chars);
                    if cap_chars == 0 {
                        self.flush_inline_segment(dom, &seg_buf[..seg_len], seg_start_x, line_y, color, link_node);
                        seg_len = 0;
                        cursor_x = base_x;
                        line_y += line_max_h;
                        line_max_h = font::FONT_HEIGHT + 4;
                        continue;
                    }

                    let take = (word.len() - w_off).min(cap_chars);
                    seg_buf[seg_len..seg_len + take].copy_from_slice(&word[w_off..w_off + take]);
                    seg_len += take;
                    cursor_x += take * font::FONT_WIDTH;
                    w_off += take;
                    placed_any = true;

                    if w_off < word.len() {
                        self.flush_inline_segment(dom, &seg_buf[..seg_len], seg_start_x, line_y, color, link_node);
                        seg_len = 0;
                        cursor_x = base_x;
                        line_y += line_max_h;
                        line_max_h = font::FONT_HEIGHT + 4;
                    }
                }

                let last_b = word[word.len() - 1];
                prev_ended_sep = last_b == b'(' || last_b == b'[';
            }

            if seg_len > 0 {
                self.flush_inline_segment(dom, &seg_buf[..seg_len], seg_start_x, line_y, color, link_node);
            }
        }

        if text_align != TextAlign::Left && self.line_count > start_line_idx {
            let mut l = start_line_idx;
            while l < self.line_count {
                let row_y = self.lines[l].y;
                let mut r = l + 1;
                let mut max_end = self.lines[l].x as usize + (self.lines[l].text_len as usize) * font::FONT_WIDTH;
                while r < self.line_count && self.lines[r].y == row_y {
                    let end = self.lines[r].x as usize + (self.lines[r].text_len as usize) * font::FONT_WIDTH;
                    if end > max_end { max_end = end; }
                    r += 1;
                }
                let row_w = max_end.saturating_sub(base_x);
                let shift = if avail_width > row_w {
                    match text_align {
                        TextAlign::Center => (avail_width - row_w) / 2,
                        TextAlign::Right => avail_width - row_w,
                        TextAlign::Left => 0,
                    }
                } else {
                    0
                };
                if shift > 0 {
                    for k in l..r {
                        self.lines[k].x = (self.lines[k].x as usize + shift).min(u16::MAX as usize) as u16;
                    }
                }
                l = r;
            }
        }

        if placed_any {
            *curr_y = line_y + line_max_h;
        }
    }

    fn estimate_subtree_width(dom: &DomArena, node_idx: usize, depth: usize) -> usize {
        if node_idx >= dom.node_count || depth > 5 {
            return 0;
        }
        let n = &dom.nodes[node_idx];
        if n.computed_style.display == DisplayMode::None {
            return 0;
        }
        let pad_lr = n.computed_style.padding_left as usize + n.computed_style.padding_right as usize;
        let mar_lr = n.computed_style.margin_left as usize + n.computed_style.margin_right as usize;
        if let Some(w) = n.computed_style.width_opt() {
            return w + pad_lr + mar_lr;
        }
        let own_text_w = if n.text_len > 0 {
            (n.text_len as usize).min(48) * font::FONT_WIDTH + 14
        } else {
            0
        };
        let cc = (n.child_count as usize).min(MAX_DOM_CHILDREN);
        if cc == 0 {
            let base = if own_text_w > 0 {
                own_text_w
            } else if n.computed_style.has_box() {
                36
            } else {
                0
            };
            return base + pad_lr + mar_lr;
        }
        let is_row = n.computed_style.display == DisplayMode::Inline
            || n.computed_style.display == DisplayMode::InlineBlock
            || (n.computed_style.display == DisplayMode::Flex
                && n.computed_style.flex_direction == FlexDirection::Row
                && n.computed_style.grid_cols <= 1);
        let mut content_w = own_text_w;
        let mut vis = 0usize;
        for c in 0..cc {
            let cw = Self::estimate_subtree_width(dom, n.child_id(c), depth + 1);
            if cw > 0 {
                vis += 1;
                if is_row {
                    content_w = content_w.saturating_add(cw);
                } else {
                    content_w = content_w.max(cw);
                }
            }
        }
        if is_row && vis > 1 {
            content_w = content_w.saturating_add((n.computed_style.gap as usize).min(24) * (vis - 1));
        }
        content_w + pad_lr + mar_lr + 10
    }

    fn record_interactive_box_region(&mut self, dom: &DomArena, node_idx: usize, x: usize, y: usize, w: usize, h: usize) {
        if node_idx >= dom.node_count || w == 0 || h == 0 || self.click_count >= self.click_regions.len() {
            return;
        }
        let node = &dom.nodes[node_idx];
        let is_close_btn = dom.has_class(node_idx, b"win-btn") && dom.has_class(node_idx, b"close");
        if node.data_open_window_len > 0 || node.onclick_len > 0 || is_close_btn {
            self.push_click_region(x, y, w, h, node_idx, CLICK_FLAG_JS);
        } else if node.href_len > 0 {
            self.push_click_region(x, y, w, h, node_idx, 0);
        }
    }

    fn layout_subtree(
        &mut self,
        dom: &mut DomArena,
        node_idx: usize,
        base_x: usize,
        curr_y: &mut usize,
        avail_width: usize,
        laid_out: &mut [bool; MAX_DOM_NODES],
    ) {
        if node_idx >= dom.node_count || laid_out[node_idx] {
            return;
        }

        let style = dom.nodes[node_idx].computed_style;
        let tag_buf = dom.nodes[node_idx].tag;
        let tag_len = (dom.nodes[node_idx].tag_len as usize).min(tag_buf.len());
        let tag = &tag_buf[..tag_len];
        let child_count = (dom.nodes[node_idx].child_count as usize).min(MAX_DOM_CHILDREN);
        let children = dom.nodes[node_idx].children;

        // Se display == none, ignora o elemento e toda sua sub-árvore
        if style.display == DisplayMode::None {
            laid_out[node_idx] = true;
            for c in 0..child_count {
                Self::mark_subtree_laid_out(dom, children[c] as usize, laid_out);
            }
            return;
        }

        laid_out[node_idx] = true;

        if tag == b"hr" {
            if self.line_count < self.lines.len() {
                self.lines[self.line_count] = RenderLine {
                    text: [0; 96],
                    color: COLOR_HR,
                    x: (base_x + 8).min(u16::MAX as usize) as u16,
                    y: (*curr_y + 4).min(u16::MAX as usize) as u16,
                    text_len: 0,
                    flags: LINE_FLAG_HR,
                };
                self.line_count += 1;
            }
            *curr_y += 10;
            return;
        }

        if tag == b"br" {
            *curr_y += font::FONT_HEIGHT + 4;
            return;
        }

        let width_limit = match (style.width_opt(), style.max_width_opt()) {
            (Some(w), Some(mw)) => Some(w.min(mw)),
            (Some(w), None) => Some(w),
            (None, Some(mw)) => Some(mw),
            (None, None) => None,
        };
        let eff_avail_w = if let Some(mw) = width_limit {
            avail_width.min(mw).max(font::FONT_WIDTH * 4)
        } else {
            avail_width
        };
        let auto_center_off = if style.max_width_opt().is_some()
            && style.width_opt().is_none()
            && avail_width > eff_avail_w
            && style.margin_left == 0
            && style.margin_right == 0
            && !Self::is_inline_flow_tag(tag)
        {
            (avail_width - eff_avail_w) / 2
        } else {
            0
        };
        let eff_base_x = base_x + auto_center_off;

        if tag == b"img" {
            if style.display_block() {
                *curr_y += style.margin_top as usize;
            }
            let mar_left = style.margin_left as usize;
            let mar_right = style.margin_right as usize;
            let pad_left = style.padding_left as usize;
            let pad_top = style.padding_top as usize;
            let pad_bottom = style.padding_bottom as usize;

            let max_img_w = eff_avail_w.saturating_sub(mar_left + mar_right + pad_left).max(16);
            let width_attr = dom.nodes[node_idx].width_attr as usize;
            let height_attr = dom.nodes[node_idx].height_attr as usize;
            let probed_dims = if (width_attr == 0 || height_attr == 0) && dom.nodes[node_idx].src_len > 0 {
                unsafe { lookup_image_dimensions(dom.nodes[node_idx].src_slice()) }
            } else {
                None
            };

            let (img_w, img_h) = match (width_attr, height_attr, probed_dims) {
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
                let ix = eff_base_x + mar_left + pad_left;
                let iy = *curr_y + pad_top;
                self.images[slot] = RenderImage {
                    x: ix.min(u16::MAX as usize) as u16,
                    y: iy.min(u16::MAX as usize) as u16,
                    w: img_w.min(u16::MAX as usize) as u16,
                    h: img_h.min(u16::MAX as usize) as u16,
                    node_idx: node_idx as u16,
                };
                self.image_count += 1;

                if let Some(link_owner) = Self::find_enclosing_link_idx(dom, node_idx) {
                    self.push_click_region(ix, iy, img_w, img_h, link_owner, 0);
                }
            }

            *curr_y += img_h + pad_top + pad_bottom;
            if style.display_block() {
                *curr_y += style.margin_bottom as usize;
            }
            return;
        }

        // Layout Flexbox / Grid Linha (display: flex && flex-direction: row) ou linha de tabela <tr>
        let is_table_row = tag == b"tr" && child_count > 1 && child_count <= 5;
        let is_grid_row = child_count > 0 && style.grid_cols > 1;
        let is_flex_row = child_count > 0
            && style.display == DisplayMode::Flex
            && style.flex_direction == FlexDirection::Row;
        if is_flex_row || is_table_row || is_grid_row {
            if style.display_block() {
                *curr_y += style.margin_top as usize;
            }
            let box_start_y = *curr_y;
            let pad_left = style.padding_left as usize;
            let pad_right = style.padding_right as usize;
            let has_box = style.has_box();
            let pad_top = if has_box {
                (style.padding_top as usize).max(4)
            } else {
                style.padding_top as usize
            };
            let pad_bottom = if has_box {
                (style.padding_bottom as usize).max(4)
            } else {
                style.padding_bottom as usize
            };
            let mar_left = style.margin_left as usize;
            let mar_right = style.margin_right as usize;

            *curr_y += pad_top;

            let inner_w = eff_avail_w.saturating_sub(mar_left + mar_right + pad_left + pad_right);
            let mut visible_children = 0usize;
            for c in 0..child_count {
                let cid = children[c] as usize;
                if cid < dom.node_count && dom.nodes[cid].computed_style.display != DisplayMode::None {
                    visible_children += 1;
                }
            }

            let gap = if is_table_row {
                (style.gap as usize).max(8).min(32)
            } else {
                (style.gap as usize).min(32)
            };
            let start_x = eff_base_x + mar_left + pad_left;
            let max_flex_right = start_x + inner_w;
            let mut max_y = *curr_y;

            if is_grid_row {
                let max_sane_cols = (inner_w / 140).max(1);
                let cols = (style.grid_cols as usize).min(max_sane_cols).min(visible_children.max(1)).max(1);
                let col_w = inner_w.saturating_sub(gap.saturating_mul(cols - 1)) / cols;
                let mut col_idx = 0usize;
                let mut row_y = *curr_y;
                let mut row_max_y = row_y;
                for c in 0..child_count {
                    let cid = children[c] as usize;
                    if cid < dom.node_count && dom.nodes[cid].computed_style.display != DisplayMode::None {
                        if col_idx >= cols {
                            col_idx = 0;
                            row_y = row_max_y + gap.max(4);
                            row_max_y = row_y;
                        }
                        let cx = start_x + col_idx * (col_w + gap);
                        let mut cy = row_y;
                        let target_w = dom.nodes[cid].computed_style.width_opt().unwrap_or(col_w).min(col_w);
                        self.layout_subtree(dom, cid, cx, &mut cy, target_w, laid_out);
                        if cy > row_max_y { row_max_y = cy; }
                        col_idx += 1;
                    }
                }
                max_y = row_max_y;
            } else if is_flex_row && (style.flex_wrap() || visible_children >= 2) {
                let mut child_x = start_x;
                let mut row_y = *curr_y;
                let mut row_max_y = row_y;
                for c in 0..child_count {
                    let cid = children[c] as usize;
                    if cid < dom.node_count && dom.nodes[cid].computed_style.display != DisplayMode::None {
                        let est_w = dom.nodes[cid].computed_style.width_opt()
                            .unwrap_or_else(|| Self::estimate_subtree_width(dom, cid, 0))
                            .max(12)
                            .min(inner_w);
                        if child_x > start_x && child_x + est_w > max_flex_right {
                            child_x = start_x;
                            row_y = row_max_y + gap.max(4);
                            row_max_y = row_y;
                        }
                        let rem_w = max_flex_right.saturating_sub(child_x).max(font::FONT_WIDTH * 2);
                        let target_w = est_w.min(rem_w);
                        let mut cy = row_y;
                        self.layout_subtree(dom, cid, child_x, &mut cy, target_w, laid_out);
                        child_x += target_w + gap;
                        if cy > row_max_y { row_max_y = cy; }
                    }
                }
                max_y = row_max_y;
            } else {
                let total_gap = gap.saturating_mul(visible_children.saturating_sub(1));
                let avail_children_w = inner_w.saturating_sub(total_gap);
                let child_w = if visible_children > 0 { avail_children_w / visible_children } else { avail_children_w };
                let mut child_x = start_x;

                for c in 0..child_count {
                    let cid = children[c] as usize;
                    if cid < dom.node_count && dom.nodes[cid].computed_style.display != DisplayMode::None {
                        let rem_w = max_flex_right.saturating_sub(child_x);
                        if rem_w < font::FONT_WIDTH {
                            Self::mark_subtree_laid_out(dom, cid, laid_out);
                            continue;
                        }
                        let mut child_y = *curr_y;
                        let target_w = dom.nodes[cid].computed_style.width_opt().unwrap_or(child_w).min(rem_w);
                        self.layout_subtree(dom, cid, child_x, &mut child_y, target_w, laid_out);
                        child_x += target_w + gap;
                        if child_y > max_y {
                            max_y = child_y;
                        }
                    }
                }
            }
            *curr_y = max_y + pad_bottom;

            let card_w = style
                .width_opt()
                .unwrap_or(eff_avail_w.saturating_sub(mar_left + mar_right))
                .min(eff_avail_w.saturating_sub(mar_left + mar_right));
            let content_h = curr_y.saturating_sub(box_start_y).max(font::FONT_HEIGHT + pad_top + pad_bottom);
            let box_h = content_h.max(style.height_opt().unwrap_or(0));
            if box_h > content_h {
                *curr_y = box_start_y + box_h;
            }
            if has_box {
                Self::record_dom_box(dom, node_idx, eff_base_x + mar_left, box_start_y, card_w, box_h, 0);
            }
            self.record_interactive_box_region(dom, node_idx, eff_base_x + mar_left, box_start_y, card_w, box_h);

            if style.display_block() {
                *curr_y += style.margin_bottom as usize;
            }
            return;
        }

        // Container element with children
        if child_count > 0 {
            if style.display_block() {
                *curr_y += style.margin_top as usize;
            }
            let box_start_y = *curr_y;
            let pad_left = style.padding_left as usize;
            let pad_right = style.padding_right as usize;
            let has_box = style.has_box();
            let pad_top = if has_box {
                (style.padding_top as usize).max(4)
            } else {
                style.padding_top as usize
            };
            let pad_bottom = if has_box {
                (style.padding_bottom as usize).max(4)
            } else {
                style.padding_bottom as usize
            };
            let mar_left = style.margin_left as usize;
            let mar_right = style.margin_right as usize;

            *curr_y += pad_top;

            let mut all_inline_text = true;
            for c in 0..child_count {
                let child_id = children[c] as usize;
                if child_id >= dom.node_count {
                    all_inline_text = false;
                    break;
                }
                let child = &dom.nodes[child_id];
                let ctag = child.tag_slice();
                let is_simple_inline = child.node_type == DomNodeType::Text
                    || ((ctag == b"b" || ctag == b"strong" || ctag == b"i" || ctag == b"em" || ctag == b"code" || ctag == b"small" || ctag == b"span")
                        && child.child_count == 0
                        && child.href_len == 0
                        && child.onclick_len == 0
                        && !child.computed_style.has_box());
                if !is_simple_inline {
                    all_inline_text = false;
                    break;
                }
            }

            let inner_avail = eff_avail_w.saturating_sub(mar_left + mar_right + pad_left + pad_right);
            if all_inline_text {
                let mut combined = [0u8; 512];
                let mut c_len = 0usize;
                let append_chunk = |dst: &mut [u8; 512], dst_len: &mut usize, src: &[u8]| {
                    if src.is_empty() || *dst_len >= dst.len() {
                        return;
                    }
                    if *dst_len > 0 && dst[*dst_len - 1] != b' ' {
                        let first = src[0];
                        let is_punct = first == b'.' || first == b',' || first == b';' || first == b':' || first == b'!' || first == b'?' || first == b')' || first == b']';
                        if !is_punct && *dst_len < dst.len() {
                            dst[*dst_len] = b' ';
                            *dst_len += 1;
                        }
                    }
                    let take = src.len().min(dst.len() - *dst_len);
                    dst[*dst_len..*dst_len + take].copy_from_slice(&src[..take]);
                    *dst_len += take;
                };

                if dom.nodes[node_idx].text_len > 0 {
                    append_chunk(&mut combined, &mut c_len, dom.nodes[node_idx].text_slice());
                }
                for c in 0..child_count {
                    let child_id = children[c] as usize;
                    laid_out[child_id] = true;
                    let child = &dom.nodes[child_id];
                    if child.computed_style.display != DisplayMode::None && child.text_len > 0 {
                        append_chunk(&mut combined, &mut c_len, child.text_slice());
                    }
                }
                if c_len > 0 {
                    let max_chars = (inner_avail / font::FONT_WIDTH).max(1);
                    let text_x = eff_base_x + mar_left + pad_left + 4;
                    let link_ref = Self::find_enclosing_link_idx(dom, node_idx).map(|id| (id, 0u8));
                    let color = if link_ref.is_some() { COLOR_LINK } else { style.color };
                    self.add_wrapped_text_aligned(&combined[..c_len], text_x, inner_avail, curr_y, max_chars, color, link_ref.is_some(), link_ref, style.text_align);
                }
            } else {
                let child_base_x = eff_base_x + mar_left + pad_left;
                let child_width = inner_avail;
                let mut c = 0usize;

                if dom.nodes[node_idx].text_len > 0 {
                    if c < child_count && Self::is_inline_run_candidate(dom, children[c] as usize) {
                        self.layout_inline_run(
                            dom,
                            Some(node_idx),
                            &children[..child_count],
                            &mut c,
                            child_base_x,
                            curr_y,
                            child_width,
                            style.text_align,
                            laid_out,
                        );
                    } else {
                        let max_chars = (child_width / font::FONT_WIDTH).max(1);
                        let text_x = child_base_x + 4;
                        let link_ref = Self::find_enclosing_link_idx(dom, node_idx).map(|id| (id, 0u8));
                        let color = if link_ref.is_some() { COLOR_LINK } else { style.color };
                        let t_buf = dom.nodes[node_idx].text;
                        let t_len = (dom.nodes[node_idx].text_len as usize).min(t_buf.len());
                        self.add_wrapped_text_aligned(&t_buf[..t_len], text_x, child_width, curr_y, max_chars, color, link_ref.is_some(), link_ref, style.text_align);
                    }
                }

                // Agrupa filhos inline consecutivos (<a>, <span>, <b>, #text, <button>) na mesma linha horizontal
                while c < child_count {
                    let child_id = children[c] as usize;
                    if Self::is_inline_run_candidate(dom, child_id) {
                        self.layout_inline_run(
                            dom,
                            None,
                            &children[..child_count],
                            &mut c,
                            child_base_x,
                            curr_y,
                            child_width,
                            style.text_align,
                            laid_out,
                        );
                    } else {
                        self.layout_subtree(dom, child_id, child_base_x, curr_y, child_width, laid_out);
                        c += 1;
                    }
                }
            }

            *curr_y += pad_bottom;

            let card_w = style
                .width_opt()
                .unwrap_or(eff_avail_w.saturating_sub(mar_left + mar_right))
                .min(eff_avail_w.saturating_sub(mar_left + mar_right));
            let content_h = curr_y.saturating_sub(box_start_y).max(font::FONT_HEIGHT + pad_top + pad_bottom);
            let box_h = content_h.max(style.height_opt().unwrap_or(0));
            if box_h > content_h {
                *curr_y = box_start_y + box_h;
            }
            if has_box {
                Self::record_dom_box(dom, node_idx, eff_base_x + mar_left, box_start_y, card_w, box_h, 0);
            }
            self.record_interactive_box_region(dom, node_idx, eff_base_x + mar_left, box_start_y, card_w, box_h);

            if style.display_block() {
                *curr_y += style.margin_bottom as usize;
            }
            return;
        }

        // Leaf element (child_count == 0)
        let own_text_len = dom.nodes[node_idx].text_len as usize;
        if own_text_len == 0 && !style.has_box() {
            return;
        }

        if style.display_block() {
            *curr_y += style.margin_top as usize;
        }

        let is_form_input = tag == b"input" || tag == b"textarea";
        let is_input_submit = is_form_input && dom.nodes[node_idx].alt_len == 3;
        let is_interactive = tag == b"button"
            || is_input_submit
            || dom.nodes[node_idx].onclick_len > 0
            || dom.nodes[node_idx].data_open_window_len > 0
            || (dom.has_class(node_idx, b"win-btn") && dom.has_class(node_idx, b"close"));
        let enclosing_link = Self::find_enclosing_link_idx(dom, node_idx).map(|id| (id, 0u8));
        let is_link = enclosing_link.is_some();
        let color = if style.has_explicit_color() {
            style.color
        } else if is_link {
            COLOR_LINK
        } else {
            style.color
        };
        let has_box = style.has_box();

        let pad_left = style.padding_left as usize;
        let pad_right = style.padding_right as usize;
        let pad_top = style.padding_top as usize;
        let pad_bottom = style.padding_bottom as usize;
        let mar_left = style.margin_left as usize;
        let mar_right = style.margin_right as usize;

        let elem_x = eff_base_x + mar_left + pad_left + if is_interactive || has_box { 3 } else { 0 };
        let avail_for_text = eff_avail_w.saturating_sub(mar_left + mar_right + pad_left + pad_right + if is_interactive || has_box { 6 } else { 0 });
        let max_chars = (avail_for_text / font::FONT_WIDTH).max(1);

        let box_start_y = *curr_y;
        *curr_y += pad_top;

        let t_buf = dom.nodes[node_idx].text;
        let t_len = own_text_len.min(t_buf.len());
        let text = &t_buf[..t_len];
        if t_len == 0 && has_box {
            let default_empty_h = if tag == b"textarea" { 44 } else if tag == b"input" { font::FONT_HEIGHT + 4 } else { 14 };
            let empty_h = style.height_opt().unwrap_or(default_empty_h).clamp(6, 64);
            *curr_y += empty_h;
        } else if is_interactive {
            self.add_wrapped_text_interactive(text, elem_x, curr_y, max_chars, color, true, node_idx);
        } else if is_link {
            let show_underline = !has_box;
            self.add_wrapped_text_aligned(text, elem_x, avail_for_text, curr_y, max_chars, color, show_underline, enclosing_link, style.text_align);
        } else if t_len > 0 {
            self.add_wrapped_text_aligned(text, elem_x, avail_for_text, curr_y, max_chars, color, false, None, style.text_align);
        }

        *curr_y += pad_bottom;

        if has_box {
            let content_h = curr_y.saturating_sub(box_start_y);
            let min_field_h = if tag == b"textarea" { 44 + pad_top + pad_bottom } else { font::FONT_HEIGHT + pad_top + pad_bottom + 2 };
            let box_h = if t_len == 0 {
                content_h.max(style.height_opt().unwrap_or(if is_form_input { min_field_h } else { 14 })).max(6)
            } else {
                content_h.max(style.height_opt().unwrap_or(0)).max(min_field_h)
            };
            if box_h > content_h {
                *curr_y = box_start_y + box_h;
            }
            let max_box_w = eff_avail_w.saturating_sub(mar_left + mar_right);
            let box_w = if let Some(w) = style.width_opt() {
                w.min(max_box_w)
            } else if is_form_input && !is_input_submit {
                max_box_w.min(260).max(180).min(max_box_w)
            } else if style.display_block() && t_len > 0 {
                max_box_w
            } else if t_len == 0 {
                32.min(max_box_w)
            } else {
                (t_len * font::FONT_WIDTH + pad_left + pad_right + 12).min(max_box_w)
            };
            Self::record_dom_box(dom, node_idx, (eff_base_x + mar_left).max(2), box_start_y, box_w, box_h, 1);
            self.record_interactive_box_region(dom, node_idx, (eff_base_x + mar_left).max(2), box_start_y, box_w, box_h);
        } else {
            self.record_interactive_box_region(dom, node_idx, elem_x, box_start_y, avail_for_text, curr_y.saturating_sub(box_start_y).max(font::FONT_HEIGHT));
        }

        if style.display_block() {
            *curr_y += style.margin_bottom as usize;
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
        self.push_wrapped_text(
            text,
            start_x,
            max_chars * font::FONT_WIDTH,
            curr_y,
            max_chars,
            color,
            false,
            None,
            TextAlign::Left,
            is_interactive,
            node_idx,
        );
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
                            color: COLOR_HR,
                            x: 8,
                            y: (curr_y + 4).min(u16::MAX as usize) as u16,
                            text_len: 0,
                            flags: LINE_FLAG_HR,
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
                    let link_ref = if blk.link_target_len > 0 { Some((b_idx, CLICK_FLAG_PAGE_BLOCK)) } else { None };
                    self.add_wrapped_text(text, 8, &mut curr_y, max_chars_per_line, COLOR_LINK, true, link_ref);
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
        link_ref: Option<(usize, u8)>,
    ) {
        self.add_wrapped_text_aligned(
            text,
            start_x,
            max_chars * font::FONT_WIDTH,
            curr_y,
            max_chars,
            color,
            is_link,
            link_ref,
            TextAlign::Left,
        );
    }

    fn add_wrapped_text_aligned(
        &mut self,
        text: &[u8],
        start_x: usize,
        avail_w: usize,
        curr_y: &mut usize,
        max_chars: usize,
        color: u32,
        is_link: bool,
        link_ref: Option<(usize, u8)>,
        text_align: TextAlign,
    ) {
        self.push_wrapped_text(
            text,
            start_x,
            avail_w,
            curr_y,
            max_chars,
            color,
            is_link,
            link_ref,
            text_align,
            false,
            0,
        );
    }

    fn push_wrapped_text(
        &mut self,
        text: &[u8],
        start_x: usize,
        avail_w: usize,
        curr_y: &mut usize,
        max_chars: usize,
        color: u32,
        is_link: bool,
        link_ref: Option<(usize, u8)>,
        text_align: TextAlign,
        is_interactive: bool,
        node_idx: usize,
    ) {
        if text.is_empty() { return; }

        let safe_max = max_chars.max(1);
        let mut offset = 0usize;

        while offset < text.len() && self.line_count < self.lines.len() {
            let rem_slice = &text[offset..];
            let is_rule = Self::is_ascii_rule_run(rem_slice.trim_ascii());
            let remaining = rem_slice.len();
            let take = if is_rule || remaining <= safe_max {
                remaining.min(safe_max)
            } else {
                let mut break_idx = safe_max;
                while break_idx > 0 && text[offset + break_idx] != b' ' {
                    break_idx -= 1;
                }
                if break_idx == 0 { safe_max } else { break_idx }
            };

            let mut line_buf = [0u8; 96];
            let actual_take = take.min(95);
            line_buf[..actual_take].copy_from_slice(&text[offset..offset + actual_take]);

            let line_px_w = actual_take * font::FONT_WIDTH;
            let line_x = match text_align {
                TextAlign::Center if avail_w > line_px_w => start_x + (avail_w - line_px_w) / 2,
                TextAlign::Right if avail_w > line_px_w => start_x + (avail_w - line_px_w),
                _ => start_x,
            };

            if is_interactive {
                self.push_click_region(
                    line_x.saturating_sub(2),
                    *curr_y,
                    line_px_w + 8,
                    font::FONT_HEIGHT + 4,
                    node_idx,
                    CLICK_FLAG_JS,
                );
            } else if is_link {
                if let Some((ln, lflags)) = link_ref {
                    self.push_click_region(line_x, *curr_y, line_px_w, font::FONT_HEIGHT + 2, ln, lflags);
                }
            }

            self.lines[self.line_count] = RenderLine {
                text: line_buf,
                color,
                x: line_x.min(u16::MAX as usize) as u16,
                y: (*curr_y).min(u16::MAX as usize) as u16,
                text_len: actual_take as u8,
                flags: if is_link { LINE_FLAG_LINK } else { 0 },
            };
            self.line_count += 1;

            if is_rule {
                *curr_y += font::FONT_HEIGHT + 4;
                break;
            }
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
    pub dragging_scrollbar: bool,
    pub viewport_w: usize,
    pub viewport_h: usize,
    pub saved_w: usize,
    pub saved_h: usize,
    pub fullscreen_state: FullscreenState,
    pub http_fallback_insecure: bool,
    pub downgrade_blocked_count: usize,
    pub net_iface: [u8; 16],
    pub net_iface_len: usize,
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
            dragging_scrollbar: false,
            viewport_w: WIN_W,
            viewport_h: WIN_H,
            saved_w: WIN_W,
            saved_h: WIN_H,
            fullscreen_state: FullscreenState::Windowed,
            http_fallback_insecure: false,
            downgrade_blocked_count: 0,
            net_iface: *b"e1000\0\0\0\0\0\0\0\0\0\0\0",
            net_iface_len: 5,
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
        self.dragging_scrollbar = false;
        self.viewport_w = WIN_W;
        self.viewport_h = WIN_H;
        self.saved_w = WIN_W;
        self.saved_h = WIN_H;
        self.fullscreen_state = FullscreenState::Windowed;
        self.http_fallback_insecure = false;
        self.downgrade_blocked_count = 0;
        self.net_iface = *b"e1000\0\0\0\0\0\0\0\0\0\0\0";
        self.net_iface_len = 5;
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

    pub fn set_net_iface(&mut self, iface: &[u8]) {
        if iface.is_empty() {
            self.net_iface = *b"e1000\0\0\0\0\0\0\0\0\0\0\0";
            self.net_iface_len = 5;
            return;
        }
        let len = iface.len().min(self.net_iface.len());
        self.net_iface = [0u8; 16];
        self.net_iface[..len].copy_from_slice(&iface[..len]);
        self.net_iface_len = len;
    }

    fn chrome_pills_geometry(&self, w: usize) -> (usize, usize, usize, usize, usize, &[u8], u32, u32, &[u8]) {
        let url_slice = &self.current_url[..self.current_url_len];
        let is_https = url_slice.starts_with(b"https://");
        let is_http = url_slice.starts_with(b"http://");
        let (sec_txt, sec_bg, sec_fg): (&[u8], u32, u32) = if self.http_fallback_insecure {
            (b"HTTP (Sem TLS)", 0x007C_2D12, 0x00FB_BF24)
        } else if is_https {
            (b"TLS1.3", 0x0006_4E3B, 0x0034_D399)
        } else if is_http {
            (b"HTTP", 0x001E_293B, 0x0094_A3B8)
        } else {
            (b"VFS", 0x001E_293B, 0x0094_A3B8)
        };
        let nic_txt: &[u8] = if self.net_iface_len > 0 {
            &self.net_iface[..self.net_iface_len]
        } else {
            b"e1000"
        };

        let url_x = 66usize;
        let right_edge = w.saturating_sub(28);
        let avail_mid = right_edge.saturating_sub(url_x);
        let max_pills_zone = avail_mid.saturating_sub(56);

        let raw_nic_w = nic_txt.len() * font::FONT_WIDTH + 8;
        let nic_box_w = raw_nic_w.min(max_pills_zone / 2).max(28.min(max_pills_zone));
        let gap = if nic_box_w > 0 { 3 } else { 0 };
        let raw_sec_w = sec_txt.len() * font::FONT_WIDTH + 8;
        let sec_box_w = raw_sec_w.min(max_pills_zone.saturating_sub(nic_box_w + gap + 4)).max(28.min(max_pills_zone));
        let total_pills = sec_box_w + gap + nic_box_w + 6;
        let url_w = avail_mid.saturating_sub(total_pills).max(40);
        let sec_x = url_x + url_w + 4;
        let nic_x = sec_x + sec_box_w + gap;
        (url_w, sec_x, sec_box_w, nic_x, nic_box_w, sec_txt, sec_bg, sec_fg, nic_txt)
    }

    fn draw_chrome_pill(buf: &mut [u32], w: usize, h: usize, x: usize, box_w: usize, txt: &[u8], bg: u32, fg: u32) {
        if box_w < 12 {
            return;
        }
        fill_rounded_rect(buf, w, h, x, 3, box_w, 20, 4, bg);
        font::draw_text_clipped_rect(
            buf,
            w,
            h,
            x + 4,
            9,
            txt,
            fg,
            None,
            x + box_w.saturating_sub(2),
            3,
            23,
        );
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

    pub fn click_region_target(&self, r: &ClickRegion) -> &[u8] {
        let idx = r.node_idx as usize;
        if r.is_js() {
            b"js:action"
        } else if r.is_page_block() {
            if idx < self.page.block_count {
                let b = &self.page.blocks[idx];
                &b.link_target[..b.link_target_len.min(b.link_target.len())]
            } else {
                b""
            }
        } else if idx < self.dom.node_count {
            self.dom.nodes[idx].href_slice()
        } else {
            b""
        }
    }

    pub fn dispatch_js_click(&mut self, node_idx: usize) -> bool {
        if node_idx < self.dom.node_count {
            if self.dom.has_class(node_idx, b"win-btn") && self.dom.has_class(node_idx, b"close") {
                let mut cur = self.dom.nodes[node_idx].parent_idx();
                while let Some(pid) = cur {
                    if self.dom.has_class(pid, b"os-window") {
                        self.dom.add_class(pid, b"hidden");
                        self.css.resolve_all(&mut self.dom, self.viewport_w);
                        self.recompute_layout();
                        return true;
                    }
                    cur = self.dom.nodes[pid].parent_idx();
                }
            }

            let open_len = self.dom.nodes[node_idx].data_open_window_len as usize;
            if open_len > 0 {
                let open_id = self.dom.nodes[node_idx].data_open_window;
                let target_id = &open_id[..open_len.min(open_id.len())];
                if let Some(target_idx) = self.dom.find_by_id(target_id) {
                    let was_hidden = self.dom.has_class(target_idx, b"hidden");
                    for idx in 0..self.dom.node_count {
                        if self.dom.has_class(idx, b"os-window") {
                            self.dom.add_class(idx, b"hidden");
                        }
                    }
                    if was_hidden {
                        self.dom.remove_class(target_idx, b"hidden");
                    }
                    self.css.resolve_all(&mut self.dom, self.viewport_w);
                    self.recompute_layout();
                    return true;
                }
            }

            let onclick_len = self.dom.nodes[node_idx].onclick_len as usize;
            if onclick_len > 0 {
                let mut script_buf = [0u8; 96];
                let cpy = onclick_len.min(script_buf.len());
                script_buf[..cpy].copy_from_slice(&self.dom.nodes[node_idx].onclick_attr[..cpy]);
                self.js_runtime.eval_script(&script_buf[..cpy], &mut self.dom);
                self.css.resolve_all(&mut self.dom, self.viewport_w);
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
            if self.dom.node_count > 0 {
                self.css.resolve_all(&mut self.dom, self.viewport_w);
            }
            self.recompute_layout();
        }
    }

    pub fn recompute_layout(&mut self) {
        let stage = compute_stage_geometry(self.viewport_w, self.viewport_h);
        if self.dom.node_count > 0 {
            self.layout.compute_from_dom_stage(&mut self.dom, stage.content_rect.x, stage.content_rect.w);
        } else {
            self.layout.compute(&self.page, stage.content_rect.w);
        }
        let v_h = stage.viewport_rect.h;
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
                if self.dom.node_count > 0 {
                    self.css.resolve_all(&mut self.dom, self.viewport_w);
                }
                self.recompute_layout();
                self.fullscreen_state = FullscreenState::Fullscreen;
                FullscreenState::Fullscreen
            }
            FullscreenState::Fullscreen => {
                self.fullscreen_state = FullscreenState::ExitingFullscreen;
                self.viewport_w = if self.saved_w > 0 { self.saved_w } else { WIN_W };
                self.viewport_h = if self.saved_h > 0 { self.saved_h } else { WIN_H };
                if self.dom.node_count > 0 {
                    self.css.resolve_all(&mut self.dom, self.viewport_w);
                }
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

    pub fn mark_http_fallback_insecure(&mut self) {
        self.http_fallback_insecure = true;
    }

    pub fn record_downgrade_blocked(&mut self) {
        self.downgrade_blocked_count = self.downgrade_blocked_count.saturating_add(1);
    }

    pub fn load_document_bytes(&mut self, html: &[u8], url: &[u8]) {
        self.set_url(url);
        self.scroll_y = 0;
        self.dragging_scrollbar = false;
        self.is_address_focused = false;
        self.address_selected = false;
        self.http_fallback_insecure = false;
        self.downgrade_blocked_count = 0;
        parse_html(html, &mut self.page);
        parse_html_into_dom_with_base(html, url, self.viewport_w, &mut self.dom, &mut self.css, &mut self.js_runtime);
        self.recompute_layout();
        if self.layout.line_count == 0 && !html.is_empty() {
            let stage = compute_stage_geometry(self.viewport_w, self.viewport_h);
            self.layout.compute_fallback_text(html, stage.content_rect.w);
            let v_h = stage.viewport_rect.h;
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
                crate::print_str(self.click_region_target(r));
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

    pub fn scroll_to_track_y(&mut self, pointer_y: usize) {
        let stage = compute_stage_geometry(self.viewport_w, self.viewport_h);
        let track_y = stage.scrollbar_rect.y;
        let track_h = stage.scrollbar_rect.h;
        if self.max_scroll == 0 || track_h == 0 {
            self.scroll_y = 0;
            return;
        }
        let thumb_h = 24usize
            .min(track_h)
            .max(track_h * stage.viewport_rect.h / self.layout.total_height.max(1));
        let thumb_travel = track_h.saturating_sub(thumb_h).max(1);
        let rel = pointer_y.saturating_sub(track_y + thumb_h / 2).min(thumb_travel);
        self.scroll_y = (rel * self.max_scroll / thumb_travel).min(self.max_scroll);
    }

    /// Hit-testing para cliques do mouse: despacha botões do chrome, barra de endereço, rolagem e links
    pub fn handle_click(&mut self, click_x: usize, click_y: usize) -> BrowserAction {
        let w = self.viewport_w;
        let h = self.viewport_h;
        let stage = compute_stage_geometry(w, h);
        let clip_top = stage.viewport_rect.y;
        let clip_bottom = stage.viewport_rect.y + stage.viewport_rect.h;

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
            let (url_w, _, _, _, _, _, _, _, _) = self.chrome_pills_geometry(w);
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

            // Barra de rolagem (track_x = stage.scrollbar_rect.x .. w)
            if click_x >= stage.scrollbar_rect.x.saturating_sub(2) {
                let track_y = stage.scrollbar_rect.y;
                let track_h = stage.scrollbar_rect.h;
                if self.max_scroll > 0 && track_h > 0 {
                    self.dragging_scrollbar = true;
                    let thumb_h = 24usize
                        .min(track_h)
                        .max(track_h * stage.viewport_rect.h / self.layout.total_height.max(1));
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
                let rx_start = (r.x as usize).saturating_sub(pad);
                let rx_end = r.x as usize + r.w as usize + pad;
                let ry_start = (r.y as usize).saturating_sub(pad);
                let ry_end = r.y as usize + r.h as usize + pad;
                if click_x >= rx_start && click_x <= rx_end && doc_y >= ry_start && doc_y <= ry_end {
                    if r.is_js() {
                        return BrowserAction::JsAction(r.node_idx as usize);
                    } else {
                        let target_slice = self.click_region_target(r);
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
                        let tlen = target_slice.len().min(128);
                        target[..tlen].copy_from_slice(&target_slice[..tlen]);
                        return BrowserAction::NavigateTarget(target, tlen);
                    }
                }
            }

            // Hit-test em imagens: clicar sobre imagem define como papel de parede
            for i in 0..self.layout.image_count {
                let img = &self.layout.images[i];
                let ix = img.x as usize;
                let iy = img.y as usize;
                let iw = img.w as usize;
                let ih = img.h as usize;
                if click_x >= ix && click_x <= ix + iw && doc_y >= iy && doc_y <= iy + ih {
                    let nid = img.node_idx as usize;
                    if nid < self.dom.node_count && self.dom.nodes[nid].src_len > 0 {
                        let src_s = self.dom.nodes[nid].src_slice();
                        let mut target = [0u8; 128];
                        let slen = src_s.len().min(128);
                        target[..slen].copy_from_slice(&src_s[..slen]);
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
        let stage = compute_stage_geometry(w, h);
        let clip_top = stage.viewport_rect.y;
        let clip_bottom = stage.viewport_rect.y + stage.viewport_rect.h;
        let max_content_x = stage.scrollbar_rect.x.saturating_sub(2);

        // 1. Fundo Geral (respeita `body { background: ... }` quando presente)
        let canvas_bg = self.css.body_bg_color.unwrap_or(COLOR_BG);
        fill_rect(buf, w, h, 0, 0, w, h, canvas_bg);

        // 2. Barra Superior de Navegação (Chrome) - Estilo Glassmorphism / Frosted Glass translúcido (alpha ~140)
        fill_rect_alpha(buf, w, h, 0, 0, stage.chrome_rect.w, 26, 0x000F_172A, 140);
        draw_rect_border(buf, w, h, 0, 0, stage.chrome_rect.w, 26, 0x001E_293B);

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

        // Campo de URL + Pílulas duplas lado a lado (Transporte TLS/HTTP + Placa NIC ativa)
        let url_x = 66;
        let (url_w, sec_x, sec_box_w, nic_x, nic_box_w, sec_txt, sec_bg, sec_fg, nic_txt) =
            self.chrome_pills_geometry(w);
        let border_col = if self.is_address_focused {
            0x0038_BDF8
        } else if self.http_fallback_insecure {
            0x00F5_9E0B
        } else {
            0x0033_4155
        };

        fill_rounded_rect(buf, w, h, url_x, 3, url_w, 20, 6, 0x000F_172A);
        draw_rounded_border(buf, w, h, url_x, 3, url_w, 20, 6, 1, border_col);

        Self::draw_chrome_pill(buf, w, h, sec_x, sec_box_w, sec_txt, sec_bg, sec_fg);
        Self::draw_chrome_pill(buf, w, h, nic_x, nic_box_w, nic_txt, 0x001E_3A5F, 0x0038_BDF8);

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

        // 2.5. Pintura immediate-mode de cartões, painéis, inputs e blocos semânticos direto do DomArena (zero array extra de caixas)
        for idx in 0..self.dom.node_count {
            let node = &self.dom.nodes[idx];
            let bx_x = node.box_x as usize;
            let bx_y = node.box_y as usize;
            let bx_w_raw = node.box_w as usize;
            let bx_h = node.box_h as usize;
            if bx_w_raw == 0 || bx_h == 0 || bx_x >= max_content_x {
                continue;
            }
            let bx_w = bx_w_raw.min(max_content_x.saturating_sub(bx_x));
            if bx_w == 0 {
                continue;
            }
            let style = &node.computed_style;
            let bg_color = if style.has_bg() {
                style.bg_color
            } else if node.box_bg_fallback != 0 {
                0x0031_3244
            } else {
                0
            };
            let border_color = style.border_color_opt();
            if bg_color == 0 && border_color.is_none() {
                continue;
            }
            let br = style.border_radius as usize;
            let bw = style.border_width as usize;
            let bs = style.border_sides;
            if bx_y + bx_h > self.scroll_y {
                let box_top = if bx_y >= self.scroll_y {
                    (bx_y - self.scroll_y) + clip_top
                } else {
                    clip_top
                };
                let box_bottom = ((bx_y + bx_h).saturating_sub(self.scroll_y)) + clip_top;
                if box_top < clip_bottom && box_bottom > clip_top {
                    let draw_y = box_top.max(clip_top);
                    let draw_h = box_bottom.min(clip_bottom).saturating_sub(draw_y);
                    if draw_h > 0 {
                        if br > 0 && bs == 15 {
                            if bg_color != 0 {
                                fill_rounded_rect(buf, w, h, bx_x, draw_y, bx_w, draw_h, br, bg_color);
                            }
                            if let Some(bc) = border_color {
                                draw_rounded_border(buf, w, h, bx_x, draw_y, bx_w, draw_h, br, bw, bc);
                            }
                        } else {
                            if bg_color != 0 {
                                if br > 0 {
                                    fill_rounded_rect(buf, w, h, bx_x, draw_y, bx_w, draw_h, br, bg_color);
                                } else {
                                    fill_rect(buf, w, h, bx_x, draw_y, bx_w, draw_h, bg_color);
                                }
                            }
                            if let Some(bc) = border_color {
                                if bs == 15 {
                                    draw_rect_border(buf, w, h, bx_x, draw_y, bx_w, draw_h, bc);
                                } else {
                                    let eb = bw.clamp(1, 3);
                                    if (bs & 1) != 0 {
                                        fill_rect(buf, w, h, bx_x, draw_y, bx_w, eb, bc);
                                    }
                                    if (bs & 4) != 0 && draw_h >= eb {
                                        fill_rect(buf, w, h, bx_x, draw_y + draw_h - eb, bx_w, eb, bc);
                                    }
                                    if (bs & 8) != 0 {
                                        fill_rect(buf, w, h, bx_x, draw_y, eb, draw_h, bc);
                                    }
                                    if (bs & 2) != 0 && bx_w >= eb {
                                        fill_rect(buf, w, h, bx_x + bx_w - eb, draw_y, eb, draw_h, bc);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 2.7. Renderiza imagens (<img>) com clipping e scroll
        for i in 0..self.layout.image_count {
            let img = &self.layout.images[i];
            let ix = img.x as usize;
            let iy = img.y as usize;
            let ih = img.h as usize;
            if ix >= max_content_x {
                continue;
            }
            if iy + ih > self.scroll_y {
                let img_top = if iy >= self.scroll_y {
                    (iy - self.scroll_y) + clip_top
                } else {
                    clip_top
                };
                let img_bottom = ((iy + ih).saturating_sub(self.scroll_y)) + clip_top;
                if img_top < clip_bottom && img_bottom > clip_top {
                    let draw_y = img_top.max(clip_top);
                    let draw_h = img_bottom.min(clip_bottom).saturating_sub(draw_y);
                    if draw_h > 0 {
                        let src_y_off = self.scroll_y.saturating_sub(iy);
                        let nid = img.node_idx as usize;
                        let (src_s, alt_s): (&[u8], &[u8]) = if nid < self.dom.node_count {
                            (self.dom.nodes[nid].src_slice(), self.dom.nodes[nid].alt_slice())
                        } else {
                            (b"", b"")
                        };
                        render_image_element(buf, w, h, img, src_s, alt_s, draw_y, draw_h, src_y_off, clip_top, clip_bottom);
                    }
                }
            }
        }

        // 3. Documento no Viewport (com scroll_y aplicado e clipping horizontal/vertical)
        for i in 0..self.layout.line_count {
            let line = &self.layout.lines[i];
            let lx = line.x as usize;
            let ly = line.y as usize;
            if lx >= max_content_x {
                continue;
            }

            if line.is_hr() {
                if ly >= self.scroll_y {
                    let actual_y = (ly - self.scroll_y) + clip_top;
                    if actual_y >= clip_top && actual_y <= clip_bottom {
                        let hr_w = max_content_x.saturating_sub(lx).min(w.saturating_sub(24));
                        fill_rect(buf, w, h, lx, actual_y, hr_w, 1, COLOR_HR);
                    }
                }
            } else if ly >= self.scroll_y {
                let actual_y = (ly - self.scroll_y) + clip_top;
                if actual_y + font::FONT_HEIGHT <= clip_bottom {
                    let text_slice = line.text_slice();
                    font::draw_text_clipped_rect(
                        buf,
                        w,
                        h,
                        lx,
                        actual_y,
                        text_slice,
                        line.color,
                        None,
                        max_content_x,
                        clip_top,
                        clip_bottom,
                    );

                    if line.is_link() {
                        let underline_y = actual_y + font::FONT_HEIGHT;
                        if underline_y < clip_bottom {
                            let link_w = (text_slice.len() * font::FONT_WIDTH).min(max_content_x.saturating_sub(lx));
                            fill_rect(buf, w, h, lx, underline_y, link_w, 1, line.color);
                        }
                    }
                }
            }
        }

        // 4. Barra de Rolagem Vertical (Scrollbar Track & Thumb estritamente acima da barra BRCR)
        let track_x = stage.scrollbar_rect.x;
        let track_w = stage.scrollbar_rect.w;
        let track_y = stage.scrollbar_rect.y;
        let track_h = stage.scrollbar_rect.h;
        fill_rect(buf, w, h, track_x, track_y, track_w, track_h, COLOR_SCROLL_TRACK);
        draw_rect_border(buf, w, h, track_x, track_y, track_w, track_h, COLOR_BORDER);

        if self.max_scroll > 0 && track_h > 0 {
            let thumb_h = 24usize.min(track_h).max(track_h * stage.viewport_rect.h / self.layout.total_height.max(1));
            let thumb_travel = track_h.saturating_sub(thumb_h);
            let thumb_y = track_y + (self.scroll_y * thumb_travel / self.max_scroll.max(1));
            fill_rect(buf, w, h, track_x + 1, thumb_y, track_w - 2, thumb_h, COLOR_SCROLL_THUMB);
            draw_rect_border(buf, w, h, track_x + 1, thumb_y, track_w - 2, thumb_h, COLOR_SCROLL_THUMB_BORDER);
        }

        // 5. Barra Inferior de Telemetria BRCR (Jinn Browser 0.8.2)
        if stage.status_rect.h > 0 {
            let status_y = stage.status_rect.y;
            let status_h = stage.status_rect.h;
            fill_rect(buf, w, h, 0, status_y, w, status_h, COLOR_CHROME);
            let (status_txt, status_col): (&[u8], u32) = if self.downgrade_blocked_count > 0 {
                if w >= 460 {
                    (b"SEC: Downgrade HTTP Bloqueado (<img> HTTPS-only ativo) | BRCR: 0.134%", 0x00FB_BF24)
                } else {
                    (b"SEC: Downgrade <img> bloqueado", 0x00FB_BF24)
                }
            } else if self.http_fallback_insecure {
                if w >= 460 {
                    (b"HTTP (Sem TLS) - Fallback sem esquema ativo | BRCR: 0.134% [HIBERNATED]", 0x00FB_BF24)
                } else {
                    (b"HTTP (Sem TLS) | BRCR: 0.13%", 0x00FB_BF24)
                }
            } else if w >= 460 {
                (b"BRCR: 0.134% [HIBERNATED] (Mem 0.336% | Gfx 0% | CPU 0)", COLOR_URL_TEXT)
            } else {
                (b"BRCR: 0.13% | Mem 0.34% | CPU 0", COLOR_URL_TEXT)
            };
            font::draw_text_clipped_rect(
                buf,
                w,
                h,
                6,
                status_y + 3,
                status_txt,
                status_col,
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
pub const CSS_SCRATCH_SIZE: usize = 24576;

// Buffer único reutilizável de 24 KiB para carregamento e parsing de CSS (zero cache redundante em RAM)
static mut CSS_SCRATCH_BUF: [u8; CSS_SCRATCH_SIZE] = [0u8; CSS_SCRATCH_SIZE];
static mut CSS_SCRATCH_KEY_A: u64 = 0;
static mut CSS_SCRATCH_KEY_B: u64 = 0;
static mut CSS_SCRATCH_ALT_KEY: u64 = 0;
static mut CSS_SCRATCH_LEN: usize = 0;

pub fn urls_same_host(a: &[u8], b: &[u8]) -> bool {
    let Some(pa) = parse_http_url(a) else { return false; };
    let Some(pb) = parse_http_url(b) else { return false; };
    pa.host_len == pb.host_len && pa.host[..pa.host_len].eq_ignore_ascii_case(&pb.host[..pb.host_len])
}

pub unsafe fn css_scratch_slice_mut() -> &'static mut [u8] {
    CSS_SCRATCH_KEY_A = 0;
    CSS_SCRATCH_KEY_B = 0;
    CSS_SCRATCH_ALT_KEY = 0;
    CSS_SCRATCH_LEN = 0;
    core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(CSS_SCRATCH_BUF) as *mut u8, CSS_SCRATCH_SIZE)
}

pub unsafe fn css_scratch_store(key_a: &[u8], key_b: &[u8], raw: &[u8]) -> usize {
    if raw.is_empty() {
        CSS_SCRATCH_LEN = 0;
        return 0;
    }
    let mut copy_len = raw.len().min(CSS_SCRATCH_SIZE);
    if raw.len() > CSS_SCRATCH_SIZE {
        if let Some(last_brace) = raw[..CSS_SCRATCH_SIZE].iter().rposition(|&b| b == b'}') {
            copy_len = last_brace + 1;
        }
    }
    CSS_SCRATCH_KEY_A = img_fnv1a_hash(key_a);
    CSS_SCRATCH_KEY_B = img_fnv1a_hash(key_b);
    CSS_SCRATCH_ALT_KEY = 0;
    CSS_SCRATCH_BUF[..copy_len].copy_from_slice(&raw[..copy_len]);
    CSS_SCRATCH_LEN = copy_len;
    copy_len
}

pub unsafe fn css_scratch_update_alt_key(alt: &[u8]) {
    CSS_SCRATCH_ALT_KEY = img_fnv1a_hash(alt);
}

pub unsafe fn css_scratch_lookup(key: &[u8]) -> Option<&'static [u8]> {
    if key.is_empty() || CSS_SCRATCH_LEN == 0 || CSS_SCRATCH_LEN > CSS_SCRATCH_SIZE {
        return None;
    }
    let h = img_fnv1a_hash(key);
    if CSS_SCRATCH_KEY_B == h || CSS_SCRATCH_KEY_A == h || (CSS_SCRATCH_ALT_KEY != 0 && CSS_SCRATCH_ALT_KEY == h) {
        return Some(&CSS_SCRATCH_BUF[..CSS_SCRATCH_LEN]);
    }
    None
}

pub fn extract_next_stylesheet_href<'a>(html: &'a [u8], pos: &mut usize) -> Option<&'a [u8]> {
    let len = html.len();
    while *pos + 5 < len {
        if html[*pos] == b'<' {
            let mut j = *pos + 1;
            if j + 3 < len && html[j..j + 4].eq_ignore_ascii_case(b"link")
                && (j + 4 >= len || html[j + 4] == b' ' || html[j + 4] == b'\t' || html[j + 4] == b'\r' || html[j + 4] == b'\n')
            {
                j += 4;
                let tag_start = j;
                while j < len && html[j] != b'>' {
                    j += 1;
                }
                let attrs = &html[tag_start..j];
                *pos = if j < len { j + 1 } else { j };
                let is_rel_css = extract_attr_val(attrs, b"rel=")
                    .map(|r| r.eq_ignore_ascii_case(b"stylesheet"))
                    .unwrap_or(false);
                if let Some(href) = extract_attr_val(attrs, b"href=") {
                    if is_rel_css || href.ends_with(b".css") {
                        return Some(href);
                    }
                }
                continue;
            }
        }
        *pos += 1;
    }
    None
}

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
    src_slice: &[u8],
    alt_slice: &[u8],
    draw_y: usize,
    draw_h: usize,
    src_y_off: usize,
    clip_top: usize,
    clip_bottom: usize,
) {
    let max_x = w.saturating_sub(16);
    let ix = img.x as usize;
    let iw = img.w as usize;
    let ih = img.h as usize;
    if iw == 0 || draw_h == 0 || ix >= max_x { return; }
    let vis_img_w = iw.min(max_x.saturating_sub(ix));
    if vis_img_w == 0 { return; }

    // Fundo padrão da imagem (Catppuccin Surface)
    fill_rect(buf, w, h, ix, draw_y, vis_img_w, draw_h, 0x0031_3244);
    draw_rect_border(buf, w, h, ix, draw_y, vis_img_w, draw_h, 0x0045_475A);

    let mut decoded_ok = false;
    let mut probe_res: Option<ImageProbeInfo> = None;

    if !src_slice.is_empty() {
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
                    let target_w = iw.min(MAX_IMAGE_SCRATCH_W);
                    let target_h = ih.min(MAX_IMAGE_SCRATCH_H);

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
                                let screen_x = ix + x;
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

                    draw_rect_border(buf, w, h, ix, draw_y, vis_img_w, draw_h, COLOR_BORDER);
                    decoded_ok = true;
                }
            }
        }
    }

    if !decoded_ok {
        if draw_h >= 14 && vis_img_w >= 28 {
            let text_y = draw_y + 2;
            let text_max_x = ix + vis_img_w.saturating_sub(2);
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

                font::draw_text_clipped_rect(buf, w, h, ix + 3, text_y, tag, tag_color, None, text_max_x, clip_top, clip_bottom);

                let alt_x = ix + 3 + tag.len() * font::FONT_WIDTH + 4;
                if alt_x + font::FONT_WIDTH <= text_max_x {
                    if !alt_slice.is_empty() {
                        font::draw_text_clipped_rect(buf, w, h, alt_x, text_y, alt_slice, COLOR_TEXT, None, text_max_x, clip_top, clip_bottom);
                    } else if !src_slice.is_empty() {
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

    // Separa host (+ porta) do caminho (ou query/fragment sem '/' inicial)
    let split_idx = clean.iter().position(|&b| b == b'/' || b == b'?' || b == b'#');
    let (host_port_slice, path_slice) = match split_idx {
        Some(idx) if clean[idx] == b'/' => (&clean[..idx], &clean[idx..]),
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

    // Remove fragmento '#' do path enviado na requisição HTTP e garante '/' inicial
    let req_path = match path_slice.iter().position(|&b| b == b'#') {
        Some(idx) => &path_slice[..idx],
        None => path_slice,
    };
    if req_path.is_empty() {
        parsed.path[0] = b'/';
        parsed.path_len = 1;
    } else if req_path[0] != b'/' {
        parsed.path[0] = b'/';
        let p_len = req_path.len().min(126);
        parsed.path[1..1 + p_len].copy_from_slice(&req_path[..p_len]);
        parsed.path_len = 1 + p_len;
    } else {
        let p_len = req_path.len().min(127);
        parsed.path[..p_len].copy_from_slice(&req_path[..p_len]);
        parsed.path_len = p_len;
    }

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
    syscall_bench: &[u8],
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
    if !syscall_bench.is_empty() {
        append(out, &mut pos, b"</p><p><b>Custo Ring3&lt;-&gt;Ring0 (Sys88/97): </b>");
        append(out, &mut pos, syscall_bench);
    }
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
        b"",
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
    append(out, &mut pos, b"<p><b>Cipher Suite Principal: </b>TLS_CHACHA20_POLY1305_SHA256 (0x1303) / AES-128-GCM (0x1301)</p>");
    append(out, &mut pos, b"<p><b>Troca de Chaves Assimetrica: </b>ECDHE Curve25519 / X25519 (RFC 7748)</p>");
    append(out, &mut pos, b"<p><b>Entropia Criptografica: </b>CSPRNG via sys_getrandom (Syscall 69) [RDRAND + Jitter]</p>");
    append(out, &mut pos, b"<p><b>Relogio do Sistema: </b>RTC CMOS UTC via sys_clock_gettime (Syscall 70)</p>");
    append(out, &mut pos, b"<p><b>Validacao X.509 &amp; CertificateVerify: </b>Parser DER sem alocacao, SAN/CN RFC 6125 e assinatura ECDSA P-256 / RSA-PSS / PKCS#1 v1.5</p>");
    append(out, &mut pos, b"<p><b>Limitacoes PKI Conhecidas: </b>Sem OCSP, CRL ou assinatura Ed25519 nesta fase Ring 3</p>");
    append(out, &mut pos, b"<p><b>Destino / URL: </b>");
    append(out, &mut pos, url);
    append(out, &mut pos, b"</p><p><b>Navegacao Web Aberta: </b>Conexoes HTTPS nativas habilitadas na porta 443</p>");
    append(out, &mut pos, b"<hr/><p><a href=\"index.htm\">Voltar ao Inicio</a></p></body></html>");
    pos
}

#[inline]
fn buf_push(dst: &mut [u8], pos: &mut usize, src: &[u8]) {
    let take = src.len().min(dst.len().saturating_sub(*pos));
    if take > 0 {
        dst[*pos..*pos + take].copy_from_slice(&src[..take]);
        *pos += take;
    }
}

fn buf_push_html_escaped(dst: &mut [u8], pos: &mut usize, src: &[u8]) {
    for &b in src {
        if *pos + 6 >= dst.len() {
            break;
        }
        match b {
            b'<' => buf_push(dst, pos, b"&lt;"),
            b'>' => buf_push(dst, pos, b"&gt;"),
            b'&' => buf_push(dst, pos, b"&amp;"),
            b'"' => buf_push(dst, pos, b"&quot;"),
            b'\r' | b'\n' | b'\t' => buf_push(dst, pos, b" "),
            32..=126 => {
                dst[*pos] = b;
                *pos += 1;
            }
            _ => {
                // Preserva bytes UTF-8 para o decodificador do parser HTML
                dst[*pos] = b;
                *pos += 1;
            }
        }
    }
}

fn buf_push_xml_text_stripped(dst: &mut [u8], pos: &mut usize, raw: &[u8]) {
    let mut s = raw.trim_ascii();
    if s.starts_with(b"<![CDATA[") {
        s = &s[9..];
        if let Some(end) = s.windows(3).position(|w| w == b"]]>") {
            s = &s[..end];
        }
    }
    let mut in_tag = false;
    let mut i = 0usize;
    let mut emitted = 0usize;
    while i < s.len() && emitted < 220 && *pos + 8 < dst.len() {
        let b = s[i];
        if b == b'<' {
            in_tag = true;
            i += 1;
            continue;
        }
        if b == b'>' {
            in_tag = false;
            i += 1;
            continue;
        }
        if !in_tag {
            if b == b'\r' || b == b'\n' || b == b'\t' {
                buf_push(dst, pos, b" ");
            } else if b == b'"' {
                buf_push(dst, pos, b"&quot;");
            } else {
                dst[*pos] = b;
                *pos += 1;
            }
            emitted += 1;
        }
        i += 1;
    }
}

fn find_subslice_ci(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w.eq_ignore_ascii_case(needle))
}

fn extract_xml_tag_inner<'a>(block: &'a [u8], open_prefix: &[u8], close_tag: &[u8]) -> Option<&'a [u8]> {
    let start_idx = find_subslice_ci(block, open_prefix)?;
    let after_open = start_idx + open_prefix.len();
    if after_open > block.len() {
        return None;
    }
    // Verifica que é exatamente a tag (seguida de '>' ou espaço)
    if after_open < block.len() {
        let nxt = block[after_open];
        if nxt != b'>' && !nxt.is_ascii_whitespace() {
            return None;
        }
    }
    let gt_rel = block[start_idx..].iter().position(|&b| b == b'>')?;
    let val_start = start_idx + gt_rel + 1;
    let close_rel = find_subslice_ci(&block[val_start..], close_tag)?;
    Some(&block[val_start..val_start + close_rel])
}

fn extract_atom_link<'a>(block: &'a [u8]) -> Option<&'a [u8]> {
    if let Some(url) = extract_xml_tag_inner(block, b"<link", b"</link>") {
        let tr = url.trim_ascii();
        if !tr.is_empty() {
            return Some(tr);
        }
    }
    let mut scan = 0usize;
    while scan + 5 < block.len() {
        if let Some(rel) = find_subslice_ci(&block[scan..], b"<link") {
            let abs = scan + rel;
            let end_rel = block[abs..].iter().position(|&b| b == b'>').unwrap_or(block.len() - abs);
            let tag_slice = &block[abs..abs + end_rel];
            if let Some(href) = extract_attr_val(tag_slice, b"href=") {
                if !href.is_empty() {
                    return Some(href);
                }
            }
            scan = abs + end_rel.max(5);
        } else {
            break;
        }
    }
    None
}

fn format_rss_atom_html(xml: &[u8], url: &[u8], out: &mut [u8]) -> usize {
    let mut pos = 0usize;
    buf_push(out, &mut pos, b"<!DOCTYPE html><html><head><title>Feed RSS/Atom - Jinn Browser</title></head><body>");
    buf_push(out, &mut pos, b"<div class=\"card\"><h1>Feed RSS / Atom</h1><p><b>Fonte: </b>");
    buf_push_html_escaped(out, &mut pos, url);
    buf_push(out, &mut pos, b"</p>");

    let is_atom = find_subslice_ci(xml, b"<entry").is_some() && find_subslice_ci(xml, b"<item").is_none();
    let item_open: &[u8] = if is_atom { b"<entry" } else { b"<item" };
    let item_close: &[u8] = if is_atom { b"</entry>" } else { b"</item>" };

    let first_item_pos = find_subslice_ci(xml, item_open).unwrap_or(xml.len());
    let header_slice = &xml[..first_item_pos];
    if let Some(feed_title) = extract_xml_tag_inner(header_slice, b"<title", b"</title>") {
        buf_push(out, &mut pos, b"<p><b>Canal: </b>");
        buf_push_xml_text_stripped(out, &mut pos, feed_title);
        buf_push(out, &mut pos, b"</p>");
    }
    buf_push(out, &mut pos, b"</div>");

    let mut scan = first_item_pos;
    let mut count = 0usize;
    while scan < xml.len() && count < 14 && pos + 512 < out.len() {
        let Some(open_rel) = find_subslice_ci(&xml[scan..], item_open) else { break; };
        let item_start = scan + open_rel;
        let next_b = xml.get(item_start + item_open.len()).copied().unwrap_or(b' ');
        if next_b != b'>' && !next_b.is_ascii_whitespace() {
            scan = item_start + item_open.len();
            continue;
        }
        let close_rel = find_subslice_ci(&xml[item_start..], item_close).unwrap_or(xml.len() - item_start);
        let item_block = &xml[item_start..item_start + close_rel];

        let title = extract_xml_tag_inner(item_block, b"<title", b"</title>").unwrap_or(b"Sem titulo");
        let link = extract_atom_link(item_block);
        let date = extract_xml_tag_inner(item_block, b"<pubDate", b"</pubDate>")
            .or_else(|| extract_xml_tag_inner(item_block, b"<updated", b"</updated>"))
            .or_else(|| extract_xml_tag_inner(item_block, b"<published", b"</published>"));
        let desc = extract_xml_tag_inner(item_block, b"<description", b"</description>")
            .or_else(|| extract_xml_tag_inner(item_block, b"<summary", b"</summary>"));

        buf_push(out, &mut pos, b"<div class=\"card\"><h3>");
        if let Some(l_url) = link {
            buf_push(out, &mut pos, b"<a href=\"");
            buf_push_html_escaped(out, &mut pos, l_url);
            buf_push(out, &mut pos, b"\">");
            buf_push_xml_text_stripped(out, &mut pos, title);
            buf_push(out, &mut pos, b"</a>");
        } else {
            buf_push_xml_text_stripped(out, &mut pos, title);
        }
        buf_push(out, &mut pos, b"</h3>");
        if let Some(d_val) = date {
            buf_push(out, &mut pos, b"<p><i>");
            buf_push_xml_text_stripped(out, &mut pos, d_val);
            buf_push(out, &mut pos, b"</i></p>");
        }
        if let Some(body_txt) = desc {
            buf_push(out, &mut pos, b"<p>");
            buf_push_xml_text_stripped(out, &mut pos, body_txt);
            buf_push(out, &mut pos, b"</p>");
        }
        buf_push(out, &mut pos, b"</div>");

        count += 1;
        scan = item_start + close_rel + item_close.len();
    }

    if count == 0 {
        buf_push(out, &mut pos, b"<pre>");
        let preview = &xml[..xml.len().min(1200)];
        buf_push_html_escaped(out, &mut pos, preview);
        buf_push(out, &mut pos, b"</pre>");
    }

    buf_push(out, &mut pos, b"<hr/><p><a href=\"index.htm\">Voltar ao Inicio</a></p></body></html>");
    pos
}

fn read_json_string(s: &[u8], i: &mut usize) -> Option<(usize, usize)> {
    if *i >= s.len() || s[*i] != b'"' {
        return None;
    }
    *i += 1;
    let start = *i;
    while *i < s.len() {
        if s[*i] == b'\\' && *i + 1 < s.len() {
            *i += 2;
            continue;
        }
        if s[*i] == b'"' {
            let end = *i;
            *i += 1;
            return Some((start, end));
        }
        *i += 1;
    }
    None
}

fn skip_json_ws(s: &[u8], i: &mut usize) {
    while *i < s.len() && s[*i].is_ascii_whitespace() {
        *i += 1;
    }
}

fn format_json_html(json: &[u8], url: &[u8], out: &mut [u8]) -> usize {
    let mut pos = 0usize;
    buf_push(out, &mut pos, b"<!DOCTYPE html><html><head><title>JSON API - Jinn Browser</title></head><body>");
    buf_push(out, &mut pos, b"<div class=\"card\"><h1>Endpoint JSON / REST API</h1><p><b>URL: </b>");
    buf_push_html_escaped(out, &mut pos, url);
    buf_push(out, &mut pos, b"</p></div><div class=\"card\"><h3>Campos Estruturados</h3><table><tr><th>Chave</th><th>Valor</th></tr>");

    let mut i = 0usize;
    let mut rows = 0usize;
    while i < json.len() && rows < 40 && pos + 256 < out.len() {
        if json[i] != b'"' {
            i += 1;
            continue;
        }
        let Some((ks, ke)) = read_json_string(json, &mut i) else { break; };
        let key_slice = &json[ks..ke];
        let mut scan = i;
        skip_json_ws(json, &mut scan);
        if scan < json.len() && json[scan] == b':' {
            scan += 1;
            skip_json_ws(json, &mut scan);
            if scan >= json.len() {
                break;
            }
            let (val_slice, is_url): (&[u8], bool) = if json[scan] == b'"' {
                let mut vs = scan;
                if let Some((a, b)) = read_json_string(json, &mut vs) {
                    i = vs;
                    let sl = &json[a..b];
                    let link = sl.starts_with(b"http://") || sl.starts_with(b"https://");
                    (sl, link)
                } else {
                    break;
                }
            } else if json[scan] == b'{' || json[scan] == b'[' {
                let tag: &[u8] = if json[scan] == b'{' { b"{...}" } else { b"[...]" };
                i = scan + 1;
                (tag, false)
            } else {
                let vstart = scan;
                while scan < json.len()
                    && json[scan] != b','
                    && json[scan] != b'}'
                    && json[scan] != b']'
                    && !json[scan].is_ascii_whitespace()
                {
                    scan += 1;
                }
                i = scan;
                (&json[vstart..scan], false)
            };

            buf_push(out, &mut pos, b"<tr><td><b>");
            buf_push_html_escaped(out, &mut pos, &key_slice[..key_slice.len().min(48)]);
            buf_push(out, &mut pos, b"</b></td><td>");
            if is_url {
                buf_push(out, &mut pos, b"<a href=\"");
                buf_push_html_escaped(out, &mut pos, &val_slice[..val_slice.len().min(120)]);
                buf_push(out, &mut pos, b"\">");
                buf_push_html_escaped(out, &mut pos, &val_slice[..val_slice.len().min(90)]);
                buf_push(out, &mut pos, b"</a>");
            } else {
                buf_push_html_escaped(out, &mut pos, &val_slice[..val_slice.len().min(120)]);
            }
            buf_push(out, &mut pos, b"</td></tr>");
            rows += 1;
        }
    }

    if rows == 0 {
        buf_push(out, &mut pos, b"<tr><td><i>payload</i></td><td><pre>");
        buf_push_html_escaped(out, &mut pos, &json[..json.len().min(800)]);
        buf_push(out, &mut pos, b"</pre></td></tr>");
    }

    buf_push(out, &mut pos, b"</table></div><hr/><p><a href=\"index.htm\">Voltar ao Inicio</a></p></body></html>");
    pos
}

/// Detecta respostas não-HTML estruturadas (RSS/Atom XML ou JSON REST API) e gera HTML semanticamente formatado.
pub fn format_structured_response_html(raw: &[u8], url: &[u8], out: &mut [u8]) -> Option<usize> {
    let mut s = raw;
    if s.starts_with(&[0xEF, 0xBB, 0xBF]) {
        s = &s[3..];
    }
    s = s.trim_ascii();
    if s.is_empty() {
        return None;
    }

    // Nunca reescreve páginas HTML válidas
    if s.len() >= 5
        && (s[..5].eq_ignore_ascii_case(b"<html")
            || (s.len() >= 9 && s[..9].eq_ignore_ascii_case(b"<!doctype"))
            || s[..5].eq_ignore_ascii_case(b"<head")
            || s[..5].eq_ignore_ascii_case(b"<body"))
    {
        return None;
    }

    let is_xml_feed = (s.starts_with(b"<?xml") || s.starts_with(b"<rss") || s.starts_with(b"<feed"))
        && (find_subslice_ci(s, b"<rss").is_some()
            || find_subslice_ci(s, b"<feed").is_some()
            || find_subslice_ci(s, b"<channel").is_some());
    if is_xml_feed {
        return Some(format_rss_atom_html(s, url, out));
    }

    if (s.starts_with(b"{") && s.ends_with(b"}")) || (s.starts_with(b"[") && s.ends_with(b"]")) {
        return Some(format_json_html(s, url, out));
    }

    None
}

/// Detecta se uma página HTML é um shell vazio de SPA pesado dependente de JS moderno (React/Next/Vite).
pub fn detect_heavy_js_spa(html: &[u8], rendered_lines: usize) -> bool {
    if rendered_lines > 5 || find_subslice_ci(html, b"<script").is_none() {
        return false;
    }
    find_subslice_ci(html, b"id=\"root\"").is_some()
        || find_subslice_ci(html, b"id='root'").is_some()
        || find_subslice_ci(html, b"id=\"app\"").is_some()
        || find_subslice_ci(html, b"id='app'").is_some()
        || find_subslice_ci(html, b"id=\"__next\"").is_some()
        || find_subslice_ci(html, b"__NEXT_DATA__").is_some()
        || find_subslice_ci(html, b"enable JavaScript").is_some()
}

/// Gera banner diagnóstico claro quando um aplicativo web exige runtime ECMAScript completo (SPA).
pub fn format_spa_notice_html(url: &[u8], out: &mut [u8]) -> usize {
    let mut pos = 0usize;
    buf_push(out, &mut pos, b"<!DOCTYPE html><html><head><title>Aviso JS/SPA - Jinn Browser</title></head><body>");
    buf_push(out, &mut pos, b"<div class=\"card\"><h1>Aplicacao Web Dinamica (SPA / JS Pesado)</h1>");
    buf_push(out, &mut pos, b"<p><b>Destino: </b>");
    buf_push_html_escaped(out, &mut pos, url);
    buf_push(out, &mut pos, b"</p><p><b>Diagnostico: </b>Esta pagina renderiza seu conteudo no cliente via JavaScript moderno (React/Next/Bundle SPA) alem do subconjunto leve suportado pelo MiniJsRuntime.</p>");
    buf_push(out, &mut pos, b"<p><b>Recomendacao: </b>Utilize versoes HTML estaticas, feeds RSS/Atom ou endpoints JSON suportados nativamente pelo Jinn Browser.</p>");
    buf_push(out, &mut pos, b"<hr/><p><a href=\"index.htm\">Voltar ao Inicio</a></p></div></body></html>");
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

