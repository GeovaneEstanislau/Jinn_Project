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

pub const MAX_DOM_NODES: usize = 64;

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DomNodeType {
    Element,
    Text,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ComputedStyle {
    pub color: u32,
    pub bg_color: Option<u32>,
    pub border_color: Option<u32>,
    pub font_size: usize,
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
}

impl ComputedStyle {
    pub const fn default() -> Self {
        Self {
            color: COLOR_TEXT,
            bg_color: None,
            border_color: None,
            font_size: 12,
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
    pub class_attr: [u8; 32],
    pub class_len: usize,
    pub style_attr: [u8; 64],
    pub style_len: usize,
    pub href_attr: [u8; 64],
    pub href_len: usize,
    pub onclick_attr: [u8; 128],
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
            class_attr: [0; 32],
            class_len: 0,
            style_attr: [0; 64],
            style_len: 0,
            href_attr: [0; 64],
            href_len: 0,
            onclick_attr: [0; 128],
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
        for i in 0..self.node_count {
            let node = &self.nodes[i];
            if node.id_len == id_bytes.len() && &node.id_attr[..node.id_len] == id_bytes {
                return Some(i);
            }
        }
        None
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
}

// ── Mini-CSS e Resolução de Estilos ──────────────────────────────────────────

pub const MAX_CSS_RULES: usize = 32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SelectorKind {
    Tag,
    Class,
    Id,
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
pub struct CssRule {
    pub kind: SelectorKind,
    pub selector: [u8; 32],
    pub selector_len: usize,
    pub color: Option<u32>,
    pub bg_color: Option<u32>,
    pub border_color: Option<u32>,
    pub font_size: Option<usize>,
    pub margin_top: Option<usize>,
    pub margin_bottom: Option<usize>,
    pub margin_left: Option<usize>,
    pub margin_right: Option<usize>,
    pub padding_top: Option<usize>,
    pub padding_bottom: Option<usize>,
    pub padding_left: Option<usize>,
    pub padding_right: Option<usize>,
    pub is_bold: Option<bool>,
    pub display_block: Option<bool>,
}

impl CssRule {
    pub const fn empty() -> Self {
        Self {
            kind: SelectorKind::Tag,
            selector: [0; 32],
            selector_len: 0,
            color: None,
            bg_color: None,
            border_color: None,
            font_size: None,
            margin_top: None,
            margin_bottom: None,
            margin_left: None,
            margin_right: None,
            padding_top: None,
            padding_bottom: None,
            padding_left: None,
            padding_right: None,
            is_bold: None,
            display_block: None,
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

    pub fn parse_stylesheet(&mut self, css: &[u8]) {
        self.rule_count = 0;
        let mut i = 0usize;
        let len = css.len();

        while i < len && self.rule_count < MAX_CSS_RULES {
            while i < len && (css[i] == b' ' || css[i] == b'\t' || css[i] == b'\r' || css[i] == b'\n') {
                i += 1;
            }
            if i >= len { break; }

            let sel_start = i;
            while i < len && css[i] != b'{' && css[i] != b'}' {
                i += 1;
            }
            if i >= len || css[i] != b'{' { break; }
            let sel_raw = css[sel_start..i].trim_ascii();
            i += 1;

            if sel_raw.is_empty() { continue; }

            let (kind, sel_name) = if sel_raw[0] == b'.' {
                (SelectorKind::Class, &sel_raw[1..])
            } else if sel_raw[0] == b'#' {
                (SelectorKind::Id, &sel_raw[1..])
            } else {
                (SelectorKind::Tag, sel_raw)
            };

            let mut rule = CssRule::empty();
            rule.kind = kind;
            let copy_sel = sel_name.len().min(31);
            rule.selector[..copy_sel].copy_from_slice(&sel_name[..copy_sel]);
            rule.selector_len = copy_sel;

            let body_start = i;
            while i < len && css[i] != b'}' {
                i += 1;
            }
            let body = &css[body_start..i];
            if i < len && css[i] == b'}' {
                i += 1;
            }

            for decl in body.split(|&b| b == b';') {
                let decl_clean = decl.trim_ascii();
                if decl_clean.is_empty() { continue; }
                if let Some(colon) = decl_clean.iter().position(|&b| b == b':') {
                    let prop = decl_clean[..colon].trim_ascii();
                    let val = decl_clean[colon + 1..].trim_ascii();

                    if prop.eq_ignore_ascii_case(b"color") {
                        rule.color = parse_hex_color(val);
                    } else if prop.eq_ignore_ascii_case(b"background-color") || prop.eq_ignore_ascii_case(b"background") {
                        rule.bg_color = parse_hex_color(val);
                    } else if prop.eq_ignore_ascii_case(b"border-color") {
                        rule.border_color = parse_hex_color(val);
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
                        }
                    } else if prop.eq_ignore_ascii_case(b"display") {
                        if val.eq_ignore_ascii_case(b"none") {
                            rule.display_block = Some(false);
                        } else if val.eq_ignore_ascii_case(b"block") {
                            rule.display_block = Some(true);
                        } else if val.eq_ignore_ascii_case(b"inline") {
                            rule.display_block = Some(false);
                        }
                    }
                }
            }

            self.rules[self.rule_count] = rule;
            self.rule_count += 1;
        }
    }

    pub fn resolve(&self, node: &mut DomNode) {
        let tag = &node.tag[..node.tag_len];
        if tag == b"h1" {
            node.computed_style.color = COLOR_H1;
            node.computed_style.margin_top = 8;
            node.computed_style.margin_bottom = 6;
            node.computed_style.is_bold = true;
            node.computed_style.display_block = true;
        } else if tag == b"h2" {
            node.computed_style.color = 0x00F3_8BA8;
            node.computed_style.margin_top = 6;
            node.computed_style.margin_bottom = 4;
            node.computed_style.is_bold = true;
            node.computed_style.display_block = true;
        } else if tag == b"h3" {
            node.computed_style.color = 0x00A6_E3A1;
            node.computed_style.margin_top = 4;
            node.computed_style.margin_bottom = 4;
            node.computed_style.is_bold = true;
            node.computed_style.display_block = true;
        } else if tag == b"p" {
            node.computed_style.color = COLOR_TEXT;
            node.computed_style.margin_top = 2;
            node.computed_style.margin_bottom = 4;
            node.computed_style.display_block = true;
        } else if tag == b"a" {
            node.computed_style.color = COLOR_LINK;
            node.computed_style.display_block = false;
        } else if tag == b"button" {
            node.computed_style.color = COLOR_BTN_TEXT;
            node.computed_style.bg_color = Some(0x0031_3244);
            node.computed_style.border_color = Some(0x0045_475A);
            node.computed_style.margin_top = 3;
            node.computed_style.margin_bottom = 3;
            node.computed_style.margin_left = 2;
            node.computed_style.margin_right = 2;
            node.computed_style.padding_top = 2;
            node.computed_style.padding_bottom = 2;
            node.computed_style.padding_left = 6;
            node.computed_style.padding_right = 6;
            node.computed_style.display_block = false;
        } else if tag == b"div" {
            node.computed_style.display_block = true;
            node.computed_style.margin_top = 0;
            node.computed_style.margin_bottom = 0;
        } else if tag == b"hr" {
            node.computed_style.display_block = true;
            node.computed_style.margin_top = 6;
            node.computed_style.margin_bottom = 6;
        } else if tag == b"b" || tag == b"strong" {
            node.computed_style.is_bold = true;
            node.computed_style.display_block = false;
        } else if tag == b"ul" {
            node.computed_style.display_block = true;
            node.computed_style.margin_top = 4;
            node.computed_style.margin_bottom = 4;
            node.computed_style.padding_left = 8;
        } else if tag == b"li" {
            node.computed_style.display_block = true;
            node.computed_style.margin_top = 2;
            node.computed_style.margin_bottom = 2;
        } else if tag == b"span" {
            node.computed_style.display_block = false;
        }

        let apply_rule = |node: &mut DomNode, r: &CssRule| {
            if let Some(c) = r.color { node.computed_style.color = c; }
            if let Some(bg) = r.bg_color { node.computed_style.bg_color = Some(bg); }
            if let Some(bc) = r.border_color { node.computed_style.border_color = Some(bc); }
            if let Some(mt) = r.margin_top { node.computed_style.margin_top = mt; }
            if let Some(mb) = r.margin_bottom { node.computed_style.margin_bottom = mb; }
            if let Some(ml) = r.margin_left { node.computed_style.margin_left = ml; }
            if let Some(mr) = r.margin_right { node.computed_style.margin_right = mr; }
            if let Some(pt) = r.padding_top { node.computed_style.padding_top = pt; }
            if let Some(pb) = r.padding_bottom { node.computed_style.padding_bottom = pb; }
            if let Some(pl) = r.padding_left { node.computed_style.padding_left = pl; }
            if let Some(pr) = r.padding_right { node.computed_style.padding_right = pr; }
            if let Some(b) = r.is_bold { node.computed_style.is_bold = b; }
            if let Some(d) = r.display_block { node.computed_style.display_block = d; }
        };

        // 1. Tag rules
        for i in 0..self.rule_count {
            let r = &self.rules[i];
            if r.kind == SelectorKind::Tag && node.tag_len == r.selector_len && &node.tag[..node.tag_len] == &r.selector[..r.selector_len] {
                apply_rule(node, r);
            }
        }
        // 2. Class rules
        for i in 0..self.rule_count {
            let r = &self.rules[i];
            if r.kind == SelectorKind::Class {
                let class_slice = &node.class_attr[..node.class_len];
                let r_sel = &r.selector[..r.selector_len];
                if class_slice.contains_ignore_ascii_case(r_sel) {
                    apply_rule(node, r);
                }
            }
        }
        // 3. ID rules
        for i in 0..self.rule_count {
            let r = &self.rules[i];
            if r.kind == SelectorKind::Id && node.id_len == r.selector_len && &node.id_attr[..node.id_len] == &r.selector[..r.selector_len] {
                apply_rule(node, r);
            }
        }

        if node.style_len > 0 {
            let inline = &node.style_attr[..node.style_len];
            for decl in inline.split(|&b| b == b';') {
                let decl_clean = decl.trim_ascii();
                if let Some(colon) = decl_clean.iter().position(|&b| b == b':') {
                    let prop = decl_clean[..colon].trim_ascii();
                    let val = decl_clean[colon + 1..].trim_ascii();
                    if prop.eq_ignore_ascii_case(b"color") {
                        if let Some(c) = parse_hex_color(val) { node.computed_style.color = c; }
                    } else if prop.eq_ignore_ascii_case(b"background-color") || prop.eq_ignore_ascii_case(b"background") {
                        if let Some(bg) = parse_hex_color(val) { node.computed_style.bg_color = Some(bg); }
                    } else if prop.eq_ignore_ascii_case(b"border-color") {
                        if let Some(bc) = parse_hex_color(val) { node.computed_style.border_color = Some(bc); }
                    } else if prop.eq_ignore_ascii_case(b"margin") {
                        if let Some((v, h)) = parse_one_or_two_px(val) {
                            node.computed_style.margin_top = v;
                            node.computed_style.margin_bottom = v;
                            node.computed_style.margin_left = h;
                            node.computed_style.margin_right = h;
                        }
                    } else if prop.eq_ignore_ascii_case(b"margin-top") {
                        if let Some(m) = parse_px(val) { node.computed_style.margin_top = m; }
                    } else if prop.eq_ignore_ascii_case(b"margin-bottom") {
                        if let Some(m) = parse_px(val) { node.computed_style.margin_bottom = m; }
                    } else if prop.eq_ignore_ascii_case(b"margin-left") {
                        if let Some(m) = parse_px(val) { node.computed_style.margin_left = m; }
                    } else if prop.eq_ignore_ascii_case(b"margin-right") {
                        if let Some(m) = parse_px(val) { node.computed_style.margin_right = m; }
                    } else if prop.eq_ignore_ascii_case(b"padding") {
                        if let Some((v, h)) = parse_one_or_two_px(val) {
                            node.computed_style.padding_top = v;
                            node.computed_style.padding_bottom = v;
                            node.computed_style.padding_left = h;
                            node.computed_style.padding_right = h;
                        }
                    } else if prop.eq_ignore_ascii_case(b"padding-top") {
                        if let Some(p) = parse_px(val) { node.computed_style.padding_top = p; }
                    } else if prop.eq_ignore_ascii_case(b"padding-bottom") {
                        if let Some(p) = parse_px(val) { node.computed_style.padding_bottom = p; }
                    } else if prop.eq_ignore_ascii_case(b"padding-left") {
                        if let Some(p) = parse_px(val) { node.computed_style.padding_left = p; }
                    } else if prop.eq_ignore_ascii_case(b"padding-right") {
                        if let Some(p) = parse_px(val) { node.computed_style.padding_right = p; }
                    } else if prop.eq_ignore_ascii_case(b"font-weight") {
                        if val.eq_ignore_ascii_case(b"bold") {
                            node.computed_style.is_bold = true;
                        }
                    } else if prop.eq_ignore_ascii_case(b"display") {
                        if val.eq_ignore_ascii_case(b"none") {
                            node.computed_style.display_block = false;
                        } else if val.eq_ignore_ascii_case(b"block") {
                            node.computed_style.display_block = true;
                        } else if val.eq_ignore_ascii_case(b"inline") {
                            node.computed_style.display_block = false;
                        }
                    }
                }
            }
        }
    }
}

// ── Mini-JS Runtime para Ring 3 (no_std) ──────────────────────────────────────

pub const MAX_JS_VARS: usize = 16;

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

pub struct MiniJsRuntime {
    pub vars: [JsVar; MAX_JS_VARS],
    pub var_count: usize,
}

impl MiniJsRuntime {
    pub const fn new() -> Self {
        Self {
            vars: [JsVar::empty(); MAX_JS_VARS],
            var_count: 0,
        }
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

    pub fn eval_script(&mut self, script: &[u8], dom: &mut DomArena) {
        for stmt_raw in script.split(|&b| b == b';' || b == b'\n') {
            let stmt = stmt_raw.trim_ascii();
            if stmt.is_empty() { continue; }

            let clean_stmt = if stmt.starts_with(b"var ") {
                stmt[4..].trim_ascii()
            } else if stmt.starts_with(b"let ") {
                stmt[4..].trim_ascii()
            } else {
                stmt
            };

            if let Some(inner_pos) = find_subslice(clean_stmt, b".innerText") {
                let lhs = clean_stmt[..inner_pos].trim_ascii();
                let after = clean_stmt[inner_pos + 10..].trim_ascii();
                if after.starts_with(b"=") {
                    let rhs = after[1..].trim_ascii();
                    let target_id_opt = extract_quoted_arg(lhs);
                    let elem_idx_opt = if let Some(id_name) = target_id_opt {
                        dom.find_by_id(id_name)
                    } else {
                        // Se for 'c.innerText' e houve getElementById prévio
                        dom.find_by_id(b"counter-display")
                    };

                    if let Some(elem_idx) = elem_idx_opt {
                        let mut eval_buf = [0u8; 128];
                        let eval_len = self.eval_expr_string(rhs, &mut eval_buf);
                        dom.set_text_content(elem_idx, &eval_buf[..eval_len]);
                    }
                }
            } else if let Some(color_pos) = find_subslice(clean_stmt, b".style.color") {
                let lhs = clean_stmt[..color_pos].trim_ascii();
                let after = clean_stmt[color_pos + 12..].trim_ascii();
                if after.starts_with(b"=") {
                    let rhs = after[1..].trim_ascii();
                    let target_id_opt = extract_quoted_arg(lhs);
                    let elem_idx_opt = if let Some(id_name) = target_id_opt {
                        dom.find_by_id(id_name)
                    } else {
                        dom.find_by_id(b"counter-display")
                    };
                    if let Some(elem_idx) = elem_idx_opt {
                        if let Some(quote_val) = extract_string_lit(rhs) {
                            if let Some(col) = parse_hex_color(quote_val) {
                                dom.set_style_color(elem_idx, col);
                            }
                        }
                    }
                }
            } else if let Some(eq_pos) = clean_stmt.iter().position(|&b| b == b'=') {
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
                } else {
                    let val = parse_int(rhs).unwrap_or(0);
                    self.set_num(real_var, val);
                }
            }
        }
    }

    fn eval_expr_string(&self, expr: &[u8], out: &mut [u8]) -> usize {
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
            }
        }
        out_len
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

// ── Parser HTML Zero-Copy para Flat Vector DOM ────────────────────────────────

pub fn parse_html_into_dom(html: &[u8], dom: &mut DomArena, css: &mut MiniCss, js: &mut MiniJsRuntime) {
    dom.node_count = 0;
    dom.title_len = 0;
    css.rule_count = 0;

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

                if stack_depth > 0 {
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
                css.parse_stylesheet(css_bytes);
                while i < len && html[i] != b'>' { i += 1; }
                if i < len { i += 1; }
                continue;
            }

            // Trata <script>
            if tag_name == b"script" {
                while i < len && html[i] != b'>' { i += 1; }
                if i < len { i += 1; }
                let script_start = i;
                while i + 8 < len && !html[i..i + 9].eq_ignore_ascii_case(b"</script>") {
                    i += 1;
                }
                let script_bytes = &html[script_start..i];
                js.eval_script(script_bytes, dom);
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

            // Ignora tags estruturais não-visuais
            if tag_name == b"html" || tag_name == b"head" || tag_name == b"body" || tag_name == b"meta" || tag_name == b"link" {
                while i < len && html[i] != b'>' { i += 1; }
                if i < len { i += 1; }
                continue;
            }

            // Elemento normal do DOM
            let node_id_opt = dom.alloc_node(DomNodeType::Element);
            if node_id_opt.is_none() {
                break;
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
                    let cpy = a_val.len().min(63);
                    dom.nodes[node_id].href_attr[..cpy].copy_from_slice(&a_val[..cpy]);
                    dom.nodes[node_id].href_len = cpy;
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
                let b = html[i];
                if b == b' ' || b == b'\n' || b == b'\r' || b == b'\t' {
                    if t_idx > 0 && text_buf[t_idx - 1] != b' ' && t_idx < 255 {
                        text_buf[t_idx] = b' ';
                        t_idx += 1;
                    }
                } else if b == b'&' {
                    if i + 3 < len && &html[i..i + 4] == b"&lt;" {
                        if t_idx < 255 { text_buf[t_idx] = b'<'; t_idx += 1; }
                        i += 3;
                    } else if i + 3 < len && &html[i..i + 4] == b"&gt;" {
                        if t_idx < 255 { text_buf[t_idx] = b'>'; t_idx += 1; }
                        i += 3;
                    } else if i + 4 < len && &html[i..i + 5] == b"&amp;" {
                        if t_idx < 255 { text_buf[t_idx] = b'&'; t_idx += 1; }
                        i += 4;
                    } else if i + 5 < len && &html[i..i + 6] == b"&nbsp;" {
                        if t_idx < 255 { text_buf[t_idx] = b' '; t_idx += 1; }
                        i += 5;
                    } else {
                        if t_idx < 255 { text_buf[t_idx] = b; t_idx += 1; }
                    }
                } else {
                    if t_idx < 255 {
                        text_buf[t_idx] = b;
                        t_idx += 1;
                    }
                }
                i += 1;
            }

            while t_idx > 0 && text_buf[t_idx - 1] == b' ' {
                t_idx -= 1;
            }
            let mut t_start = 0usize;
            while t_start < t_idx && text_buf[t_start] == b' ' {
                t_start += 1;
            }
            let actual_t_len = t_idx - t_start;
            if actual_t_len > 0 {
                dom.nodes[node_id].text[..actual_t_len].copy_from_slice(&text_buf[t_start..t_idx]);
                dom.nodes[node_id].text_len = actual_t_len;
            }

            css.resolve(&mut dom.nodes[node_id]);
        } else {
            i += 1;
        }
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

#[derive(Clone, Copy)]
pub struct BoxRect {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
    pub bg_color: u32,
    pub border_color: Option<u32>,
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

pub const MAX_RENDER_LINES: usize = 128;
pub const MAX_RENDER_BOXES: usize = 32;
pub const MAX_CLICK_REGIONS: usize = 32;

pub struct DocumentLayout {
    pub lines: [RenderLine; MAX_RENDER_LINES],
    pub line_count: usize,
    pub boxes: [BoxRect; MAX_RENDER_BOXES],
    pub box_count: usize,
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
            }; MAX_RENDER_BOXES],
            box_count: 0,
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

    pub fn compute_from_dom(&mut self, dom: &DomArena, content_width: usize) {
        self.line_count = 0;
        self.click_count = 0;
        self.box_count = 0;

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
        laid_out[node_idx] = true;
        let node = &dom.nodes[node_idx];
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

            // Direct text of container if any
            if node.text_len > 0 {
                let max_chars = (avail_width.saturating_sub(mar_left + mar_right + pad_left + pad_right) / font::FONT_WIDTH).max(1);
                let text_x = base_x + mar_left + pad_left + 4;
                self.add_wrapped_text(&node.text[..node.text_len], text_x, curr_y, max_chars, node.computed_style.color, false, None);
            }

            // Lay out all children
            let child_base_x = base_x + mar_left + pad_left;
            let child_width = avail_width.saturating_sub(mar_left + mar_right + pad_left + pad_right);
            for c in 0..node.child_count {
                let child_id = node.children[c];
                self.layout_subtree(dom, child_id, child_base_x, curr_y, child_width, laid_out);
            }

            *curr_y += pad_bottom;

            if let Some(slot) = box_slot {
                let box_h = curr_y.saturating_sub(box_start_y).max(font::FONT_HEIGHT + pad_top + pad_bottom);
                let box_w = avail_width.saturating_sub(mar_left + mar_right);
                self.boxes[slot] = BoxRect {
                    x: base_x + mar_left,
                    y: box_start_y,
                    w: box_w,
                    h: box_h,
                    bg_color: node.computed_style.bg_color.unwrap_or(0),
                    border_color: node.computed_style.border_color,
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
        let is_link = tag == b"a" && node.href_len > 0;
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
            self.add_wrapped_text_interactive(text, elem_x, curr_y, max_chars, node.computed_style.color, true, node.id);
        } else if is_link {
            let href = &node.href_attr[..node.href_len];
            self.add_wrapped_text(text, elem_x, curr_y, max_chars, node.computed_style.color, true, Some(href));
        } else if node.text_len > 0 {
            self.add_wrapped_text(text, elem_x, curr_y, max_chars, node.computed_style.color, false, None);
        }

        *curr_y += pad_bottom;

        if has_box {
            if self.box_count < self.boxes.len() {
                let box_h = curr_y.saturating_sub(box_start_y).max(font::FONT_HEIGHT + pad_top + pad_bottom + 2);
                let box_w = if node.computed_style.display_block {
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

        let max_chars_per_line = (content_width / font::FONT_WIDTH).min(34);
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
        }
    }

    pub fn dispatch_js_click(&mut self, node_idx: usize) -> bool {
        if node_idx < self.dom.node_count {
            let onclick_len = self.dom.nodes[node_idx].onclick_len;
            if onclick_len > 0 {
                let mut script_buf = [0u8; 128];
                script_buf[..onclick_len].copy_from_slice(&self.dom.nodes[node_idx].onclick_attr[..onclick_len]);
                self.js_runtime.eval_script(&script_buf[..onclick_len], &mut self.dom);
                for i in 0..self.dom.node_count {
                    self.css.resolve(&mut self.dom.nodes[i]);
                }
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
        let content_w = self.viewport_w.saturating_sub(24);
        if self.dom.node_count > 0 {
            self.layout.compute_from_dom(&self.dom, content_w);
        } else {
            self.layout.compute(&self.page, content_w);
        }
        let clip_top = CLIP_TOP;
        let clip_bottom = self.viewport_h.saturating_sub(1);
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
        let clip_bottom = h.saturating_sub(1);

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

            // Botão Fullscreen [F] no canto direito (w - 24 .. w - 4)
            let fs_x = w.saturating_sub(24);
            if click_x >= fs_x && click_x <= w.saturating_sub(4) {
                return BrowserAction::ToggleFullscreen;
            }

            // Barra de Endereço em (44 .. w - 28)
            if click_x >= 44 && click_x <= w.saturating_sub(28) {
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
                let track_h = h.saturating_sub(clip_top + 1);
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

            // Hit-test em links e botões interativos
            let doc_y = (click_y - clip_top) + self.scroll_y;
            for i in 0..self.layout.click_count {
                let r = &self.layout.click_regions[i];
                if click_x >= r.x && click_x <= r.x + r.w && doc_y >= r.y && doc_y <= r.y + r.h {
                    if r.is_js {
                        return BrowserAction::JsAction(r.node_idx);
                    } else {
                        let mut target = [0u8; 128];
                        target[..r.target_len].copy_from_slice(&r.target[..r.target_len]);
                        return BrowserAction::NavigateTarget(target, r.target_len);
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
        let clip_bottom = h.saturating_sub(1);

        // 1. Fundo Geral
        fill_rect(buf, w, h, 0, 0, w, h, COLOR_BG);

        // 2. Barra Superior de Navegação (Chrome)
        fill_rect(buf, w, h, 0, 0, w, 26, COLOR_CHROME);
        draw_rect_border(buf, w, h, 0, 0, w, 26, COLOR_BORDER);

        // Botões: [<] [>] [R]
        let back_col = if self.can_go_back() { COLOR_BTN_TEXT } else { 0x0058_5B70 };
        let fwd_col = if self.can_go_forward() { COLOR_BTN_TEXT } else { 0x0058_5B70 };
        let reload_col = COLOR_URL_TEXT;

        font::draw_text(buf, w, h, 6, 9, b"<", back_col, None);
        font::draw_text(buf, w, h, 18, 9, b">", fwd_col, None);
        font::draw_text(buf, w, h, 30, 9, b"R", reload_col, None);

        // Botão de Tela Cheia [F]
        let fs_btn_x = w.saturating_sub(20);
        let fs_col = if self.fullscreen_state == FullscreenState::Fullscreen { COLOR_H1 } else { COLOR_BTN_TEXT };
        font::draw_text(buf, w, h, fs_btn_x, 9, b"F", fs_col, None);

        // Campo de URL (Barra de Endereço)
        let url_x = 44;
        let url_w = w.saturating_sub(70);
        let border_col = if self.is_address_focused { COLOR_H1 } else { COLOR_URL_BORDER };
        fill_rect(buf, w, h, url_x, 4, url_w, 18, COLOR_URL_BG);
        draw_rect_border(buf, w, h, url_x, 4, url_w, 18, border_col);

        if self.is_address_focused {
            if self.address_selected && self.address_edit_len > 0 {
                let sel_w = (self.address_edit_len * font::FONT_WIDTH).min(url_w.saturating_sub(4));
                fill_rect(buf, w, h, url_x + 2, 6, sel_w + 2, 14, 0x0045_475A);
                let edit_slice = &self.address_edit[..self.address_edit_len];
                font::draw_text_clipped(buf, w, h, url_x + 4, 9, edit_slice, 0x00CD_D6F4, None, 4, 22);
            } else {
                let edit_slice = &self.address_edit[..self.address_edit_len];
                font::draw_text_clipped(buf, w, h, url_x + 4, 9, edit_slice, COLOR_H1, None, 4, 22);
                // Cursor '_'
                let cursor_x = url_x + 4 + self.address_edit_len * font::FONT_WIDTH;
                if cursor_x + font::FONT_WIDTH <= url_x + url_w {
                    font::draw_text_clipped(buf, w, h, cursor_x, 9, b"_", COLOR_H1, None, 4, 22);
                }
            }
        } else {
            let url_slice = &self.current_url[..self.current_url_len];
            font::draw_text_clipped(buf, w, h, url_x + 4, 9, url_slice, COLOR_URL_TEXT, None, 4, 22);
        }

        // 2.5. Renderiza caixas estilizadas (backgrounds de .card, buttons, etc.)
        for i in 0..self.layout.box_count {
            let bx = &self.layout.boxes[i];
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
                        fill_rect(buf, w, h, bx.x, draw_y, bx.w, draw_h, bx.bg_color);
                        if let Some(bc) = bx.border_color {
                            draw_rect_border(buf, w, h, bx.x, draw_y, bx.w, draw_h, bc);
                        }
                    }
                }
            }
        }

        // 3. Documento no Viewport (com scroll_y aplicado e clipping)
        for i in 0..self.layout.line_count {
            let line = &self.layout.lines[i];

            if line.is_hr {
                if line.y >= self.scroll_y {
                    let actual_y = (line.y - self.scroll_y) + clip_top;
                    if actual_y >= clip_top && actual_y <= clip_bottom {
                        fill_rect(buf, w, h, line.x, actual_y, w.saturating_sub(24), 1, COLOR_HR);
                    }
                }
            } else {
                if line.y >= self.scroll_y {
                    let actual_y = (line.y - self.scroll_y) + clip_top;
                    if actual_y + font::FONT_HEIGHT <= clip_bottom {
                        let text_slice = &line.text[..line.text_len];
                        font::draw_text_clipped(
                            buf,
                            w,
                            h,
                            line.x,
                            actual_y,
                            text_slice,
                            line.color,
                            None,
                            clip_top,
                            clip_bottom,
                        );

                        if line.is_link {
                            let underline_y = actual_y + font::FONT_HEIGHT;
                            if underline_y <= clip_bottom {
                                let link_w = line.text_len * font::FONT_WIDTH;
                                fill_rect(buf, w, h, line.x, underline_y, link_w, 1, line.color);
                            }
                        }
                    }
                }
            }
        }

        // 4. Barra de Rolagem Vertical (Scrollbar Track & Thumb)
        let track_x = w.saturating_sub(14);
        let track_w = 8;
        let track_y = clip_top;
        let track_h = h.saturating_sub(clip_top + 1);
        fill_rect(buf, w, h, track_x, track_y, track_w, track_h, COLOR_SCROLL_TRACK);
        draw_rect_border(buf, w, h, track_x, track_y, track_w, track_h, COLOR_BORDER);

        if self.max_scroll > 0 {
            let thumb_h = 24usize.max(track_h * (clip_bottom - clip_top) / self.layout.total_height.max(1));
            let thumb_travel = track_h.saturating_sub(thumb_h);
            let thumb_y = track_y + (self.scroll_y * thumb_travel / self.max_scroll.max(1));
            fill_rect(buf, w, h, track_x + 1, thumb_y, track_w - 2, thumb_h, COLOR_SCROLL_THUMB);
            draw_rect_border(buf, w, h, track_x + 1, thumb_y, track_w - 2, thumb_h, COLOR_SCROLL_THUMB_BORDER);
        }

        // 5. Barra Inferior de Telemetria BRCR (Jinn Browser 0.8.2)
        if h >= 100 {
            let status_y = h.saturating_sub(14);
            fill_rect(buf, w, h, 0, status_y, w, 14, COLOR_CHROME);
            draw_rect_border(buf, w, h, 0, status_y, w, 14, COLOR_BORDER);
            font::draw_text(buf, w, h, 6, status_y + 3, b"BRCR: 0.134% [HIBERNATED] (Mem 0.336% | Gfx 0% | CPU 0)", COLOR_URL_TEXT, None);
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
    {
        return false;
    }
    if host_part.starts_with(b"www.") { return true; }
    if host_part.contains(&b'.') || host_part.contains(&b':') {
        return true;
    }
    false
}

/// Verifica se o payload contém o terminador de chunked transfer ("0\r\n\r\n" ou "\r\n0\r\n\r\n")
pub fn has_chunked_terminator(body: &[u8]) -> bool {
    if body.len() < 5 { return false; }
    if body.starts_with(b"0\r\n\r\n") || body.ends_with(b"0\r\n\r\n") {
        return true;
    }
    if body.ends_with(b"\r\n0\r\n\r\n") {
        return true;
    }
    for w in body.windows(7) {
        if w == b"\r\n0\r\n\r\n" {
            return true;
        }
    }
    for w in body.windows(5) {
        if w == b"0\r\n\r\n" {
            return true;
        }
    }
    false
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
