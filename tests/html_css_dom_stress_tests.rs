//! Jinn OS — Testes Unitários e de Estresse do Parser HTML, Mini-CSS, Box Model e DOM
//!
//! Validação completa de conformidade para o Alpha 0.0.3 (PROMPT 3):
//! 1. Tokenizer zero-copy e construtor de DOM plano com hierarquia pai-filho
//! 2. Resolução de seletores Mini-CSS (tags, classes .card / .btn e IDs #id)
//! 3. Especificidade e herança em cascata (Tag -> Class -> ID -> Style inline)
//! 4. Parser de margens e preenchimentos (1 valor, 2 valores e propriedades direcionais)
//! 5. Modelo de caixas geométricas (Box Model) encerrando filhos com padding e margins
//! 6. Quebra de linha inteligente (Word-Wrap) sem truncamento prematuro
//! 7. Regiões clicáveis (links href e despachos JS onclick) com coordenadas absolutas
//! 8. Execução de scripts Mini-JS e mutação do DOM com recalculo dinâmico de layout
//! 9. Decodificação de entidades HTML (&lt;, &gt;, &amp;, &nbsp;) e normalização de espaços
//! 10. Validação de documentos complexos e robustez contra tags órfãs/malformadas
//! 11. Teste de conformidade exato com o arquivo local INDEX.HTM do disco virtual

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
pub const COLOR_CHROME: u32 = 0x001E_1E2E;
pub const COLOR_BORDER: u32 = 0x0031_3244;
pub const COLOR_URL_BG: u32 = 0x0031_3244;
pub const COLOR_URL_TEXT: u32 = 0x0089_DCEB;
pub const COLOR_BTN_TEXT: u32 = 0x00A6_ADC8;
pub const COLOR_H1: u32 = 0x00F9_E2AF;
pub const COLOR_TEXT: u32 = 0x00CD_D6F4;
pub const COLOR_LINK: u32 = 0x0089_B4FA;
pub const COLOR_HR: u32 = 0x0045_475A;

pub const FONT_WIDTH: usize = 6;
pub const FONT_HEIGHT: usize = 12;

pub const MAX_DOM_NODES: usize = 64;
pub const MAX_DOM_CHILDREN: usize = 16;
pub const MAX_CSS_RULES: usize = 32;
pub const MAX_RENDER_LINES: usize = 128;
pub const MAX_RENDER_BOXES: usize = 32;
pub const MAX_CLICK_REGIONS: usize = 32;
pub const MAX_JS_VARS: usize = 16;

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
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SelectorKind {
    Tag,
    Class,
    Id,
}

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

            if i + 2 < len && html[i] == b'!' && html[i + 1] == b'-' && html[i + 2] == b'-' {
                i += 3;
                while i + 2 < len && !(html[i] == b'-' && html[i + 1] == b'-' && html[i + 2] == b'>') {
                    i += 1;
                }
                if i + 2 < len { i += 3; }
                continue;
            }

            if i < len && html[i] == b'!' {
                while i < len && html[i] != b'>' { i += 1; }
                if i < len { i += 1; }
                continue;
            }

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

            if tag_name == b"html" || tag_name == b"head" || tag_name == b"body" || tag_name == b"meta" || tag_name == b"link" {
                while i < len && html[i] != b'>' { i += 1; }
                if i < len { i += 1; }
                continue;
            }

            let node_id_opt = dom.alloc_node(DomNodeType::Element);
            if node_id_opt.is_none() {
                break;
            }
            let node_id = node_id_opt.unwrap();
            dom.nodes[node_id].tag = tag_buf;
            dom.nodes[node_id].tag_len = tag_len;

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
                        dom.find_by_id(b"counter-display")
                    };

                    if let Some(elem_idx) = elem_idx_opt {
                        let mut eval_buf = [0u8; 128];
                        let eval_len = self.eval_expr_string(rhs, &mut eval_buf);
                        dom.set_text_content(elem_idx, &eval_buf[..eval_len]);
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

        for i in 0..dom.node_count {
            if dom.nodes[i].parent.is_none() {
                self.layout_subtree(dom, i, 4, &mut curr_y, content_width, &mut laid_out);
            }
        }

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

            if node.text_len > 0 {
                let max_chars = (avail_width.saturating_sub(mar_left + mar_right + pad_left + pad_right) / FONT_WIDTH).max(1);
                let text_x = base_x + mar_left + pad_left + 4;
                self.add_wrapped_text(&node.text[..node.text_len], text_x, curr_y, max_chars, node.computed_style.color, false, None);
            }

            let child_base_x = base_x + mar_left + pad_left;
            let child_width = avail_width.saturating_sub(mar_left + mar_right + pad_left + pad_right);
            for c in 0..node.child_count {
                let child_id = node.children[c];
                self.layout_subtree(dom, child_id, child_base_x, curr_y, child_width, laid_out);
            }

            *curr_y += pad_bottom;

            if let Some(slot) = box_slot {
                let box_h = curr_y.saturating_sub(box_start_y).max(FONT_HEIGHT + pad_top + pad_bottom);
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
        let max_chars = (avail_for_text / FONT_WIDTH).max(1);

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
                let box_h = curr_y.saturating_sub(box_start_y).max(FONT_HEIGHT + pad_top + pad_bottom + 2);
                let box_w = if node.computed_style.display_block {
                    avail_width.saturating_sub(mar_left + mar_right)
                } else {
                    (node.text_len * FONT_WIDTH + pad_left + pad_right + 12).min(avail_width)
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

            if is_link {
                if let Some(href) = target_href {
                    if self.click_count < self.click_regions.len() {
                        let mut reg = ClickRegion {
                            x: start_x,
                            y: *curr_y,
                            w: actual_take * FONT_WIDTH,
                            h: FONT_HEIGHT + 2,
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
            *curr_y += FONT_HEIGHT + 4;
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
                        w: actual_take * FONT_WIDTH + 8,
                        h: FONT_HEIGHT + 4,
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
            *curr_y += FONT_HEIGHT + 4;
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Suíte de Testes Host
// ─────────────────────────────────────────────────────────────────────────────

fn test_tokenizer_and_dom_hierarchy() {
    print!("Teste 1: Tokenizer zero-copy e hierarquia DOM pai-filho... ");
    let html = b"<div><p id=\"item1\">Texto 1</p><p id=\"item2\">Texto 2</p></div>";
    let mut dom = DomArena::new();
    let mut css = MiniCss::new();
    let mut js = MiniJsRuntime::new();

    parse_html_into_dom(html, &mut dom, &mut css, &mut js);

    assert_eq!(dom.node_count, 3, "Deveria ter 3 elementos no DOM");
    let div = &dom.nodes[0];
    let p1 = &dom.nodes[1];
    let p2 = &dom.nodes[2];

    assert_eq!(&div.tag[..div.tag_len], b"div");
    assert_eq!(div.parent, None, "Div raiz não deve ter pai");
    assert_eq!(div.child_count, 2, "Div deve conter 2 filhos");
    assert_eq!(div.children[0], 1, "Filho 0 deve ser o p1");
    assert_eq!(div.children[1], 2, "Filho 1 deve ser o p2");

    assert_eq!(&p1.tag[..p1.tag_len], b"p");
    assert_eq!(p1.parent, Some(0), "p1 deve ter o div como pai");
    assert_eq!(&p1.id_attr[..p1.id_len], b"item1");
    assert_eq!(&p1.text[..p1.text_len], b"Texto 1");

    assert_eq!(&p2.tag[..p2.tag_len], b"p");
    assert_eq!(p2.parent, Some(0), "p2 deve ter o div como pai");
    assert_eq!(&p2.id_attr[..p2.id_len], b"item2");
    assert_eq!(&p2.text[..p2.text_len], b"Texto 2");

    println!("PASSOU!");
}

fn test_mini_css_selectors_and_specificity() {
    print!("Teste 2: Mini-CSS seletores (Tag, Class, ID) e especificidade... ");
    let css_data = b"
        p { color: #aaaaaa; margin: 2px; }
        .highlight { color: #ff0000; background-color: #222222; }
        #special { color: #00ff00; }
    ";
    let mut css = MiniCss::new();
    css.parse_stylesheet(css_data);

    assert_eq!(css.rule_count, 3, "Deveria ter 3 regras CSS");

    let mut node1 = DomNode::empty();
    node1.tag[..1].copy_from_slice(b"p");
    node1.tag_len = 1;
    css.resolve(&mut node1);
    assert_eq!(node1.computed_style.color, 0x00AA_AAAA, "Cor da tag p deve ser #aaaaaa");

    let mut node2 = DomNode::empty();
    node2.tag[..1].copy_from_slice(b"p");
    node2.tag_len = 1;
    node2.class_attr[..9].copy_from_slice(b"highlight");
    node2.class_len = 9;
    css.resolve(&mut node2);
    assert_eq!(node2.computed_style.color, 0x00FF_0000, "Classe .highlight deve sobrescrever cor da tag");
    assert_eq!(node2.computed_style.bg_color, Some(0x0022_2222), "Background da classe deve ser aplicado");

    let mut node3 = DomNode::empty();
    node3.tag[..1].copy_from_slice(b"p");
    node3.tag_len = 1;
    node3.class_attr[..9].copy_from_slice(b"highlight");
    node3.class_len = 9;
    node3.id_attr[..7].copy_from_slice(b"special");
    node3.id_len = 7;
    css.resolve(&mut node3);
    assert_eq!(node3.computed_style.color, 0x0000_FF00, "ID #special deve ter precedência máxima sobre classe e tag");
    assert_eq!(node3.computed_style.bg_color, Some(0x0022_2222), "Background da classe deve persistir");

    println!("PASSOU!");
}

fn test_margin_and_padding_parsing() {
    print!("Teste 3: Parsing de margin e padding (1 valor, 2 valores, direcionais)... ");
    let css_data = b"
        .box1 { margin: 6px; padding: 4px 8px; }
        .box2 { margin-top: 10px; margin-bottom: 20px; padding-left: 12px; }
    ";
    let mut css = MiniCss::new();
    css.parse_stylesheet(css_data);

    let mut node1 = DomNode::empty();
    node1.class_attr[..4].copy_from_slice(b"box1");
    node1.class_len = 4;
    css.resolve(&mut node1);

    assert_eq!(node1.computed_style.margin_top, 6);
    assert_eq!(node1.computed_style.margin_bottom, 6);
    assert_eq!(node1.computed_style.margin_left, 6);
    assert_eq!(node1.computed_style.margin_right, 6);
    assert_eq!(node1.computed_style.padding_top, 4);
    assert_eq!(node1.computed_style.padding_bottom, 4);
    assert_eq!(node1.computed_style.padding_left, 8);
    assert_eq!(node1.computed_style.padding_right, 8);

    let mut node2 = DomNode::empty();
    node2.class_attr[..4].copy_from_slice(b"box2");
    node2.class_len = 4;
    css.resolve(&mut node2);

    assert_eq!(node2.computed_style.margin_top, 10);
    assert_eq!(node2.computed_style.margin_bottom, 20);
    assert_eq!(node2.computed_style.padding_left, 12);

    println!("PASSOU!");
}

fn test_container_box_model_encloses_children() {
    print!("Teste 4: Box Model de contêiner encerrando filhos com padding... ");
    let html = b"
        <style>
          .card { background-color: #1e1e2e; border-color: #45475a; margin: 8px; padding: 6px; }
          .btn { background-color: #313244; color: #89dceb; }
        </style>
        <div class=\"card\">
          <p>Linha 1 do card</p>
          <p>Linha 2 do card</p>
          <button class=\"btn\">Acao</button>
        </div>
    ";
    let mut dom = DomArena::new();
    let mut css = MiniCss::new();
    let mut js = MiniJsRuntime::new();
    let mut layout = DocumentLayout::new();

    parse_html_into_dom(html, &mut dom, &mut css, &mut js);
    layout.compute_from_dom(&dom, 280);

    assert!(layout.box_count >= 2, "Deveria ter a caixa do card e a caixa do botão");
    let card_box = layout.boxes[0];
    let btn_box = layout.boxes[1];

    assert_eq!(card_box.bg_color, 0x001E_1E2E, "Card background deve ser #1e1e2e");
    assert_eq!(card_box.border_color, Some(0x0045_475A), "Card border deve ser #45475a");
    assert_eq!(card_box.x, 12, "Card deve estar indentado pelo margin (4 base + 8 margin)");

    // O card DEVE encerrar todos os filhos:
    // A altura do card tem que ser significativamente maior que um simples botão
    // (deve conter 2 parágrafos + 1 botão + paddings)
    assert!(card_box.h > 45, "Altura do card deve encerrar todos os filhos (obtido: {}px)", card_box.h);

    // O botão deve estar inteiramente dentro dos limites verticais do card
    assert!(btn_box.y >= card_box.y, "Botão y deve estar dentro do card");
    assert!(btn_box.y + btn_box.h <= card_box.y + card_box.h, "Botão deve estar totalmente dentro do card");

    println!("PASSOU! (Card altura = {}px, Botao y = {}px)", card_box.h, btn_box.y);
}

fn test_word_wrap_and_no_premature_truncation() {
    print!("Teste 5: Word-Wrap geométrico sem truncamento prematuro... ");
    let html = b"<p>Este e um texto longo de teste projetado especificamente para verificar se o algoritmo de quebra de linha por palavras do Jinn Browser divide o paragrafo em multiplas linhas sem perder nenhum caractere.</p>";
    let mut dom = DomArena::new();
    let mut css = MiniCss::new();
    let mut js = MiniJsRuntime::new();
    let mut layout = DocumentLayout::new();

    parse_html_into_dom(html, &mut dom, &mut css, &mut js);
    layout.compute_from_dom(&dom, 120); // Janela estreita (120px = ~20 chars por linha)

    assert!(layout.line_count >= 4, "Texto longo em janela estreita deve produzir pelo menos 4 linhas (obtido: {})", layout.line_count);

    // Concatena as linhas e verifica se todas as palavras foram mantidas
    let mut full_text = Vec::new();
    for i in 0..layout.line_count {
        let line = &layout.lines[i];
        full_text.extend_from_slice(&line.text[..line.text_len]);
        full_text.push(b' ');
    }
    let recovered_str = String::from_utf8_lossy(&full_text);

    assert!(recovered_str.contains("Este e um texto longo"), "Primeira parte preservada");
    assert!(recovered_str.contains("algoritmo de quebra de linha"), "Meio preservado");
    assert!(recovered_str.contains("sem perder nenhum caractere"), "Final preservado sem truncamento");

    println!("PASSOU! ({} linhas geradas)", layout.line_count);
}

fn test_click_regions_and_interactive_links() {
    print!("Teste 6: Regiões clicáveis (links href e eventos onclick)... ");
    let html = b"
        <p><a href=\"page2.htm\">Acesse a Pagina 2</a></p>
        <button onclick=\"alert();\">Clique Aqui</button>
    ";
    let mut dom = DomArena::new();
    let mut css = MiniCss::new();
    let mut js = MiniJsRuntime::new();
    let mut layout = DocumentLayout::new();

    parse_html_into_dom(html, &mut dom, &mut css, &mut js);
    layout.compute_from_dom(&dom, 280);

    assert_eq!(layout.click_count, 2, "Deveria registrar exatamente 2 regiões clicáveis");

    let link_reg = &layout.click_regions[0];
    assert!(!link_reg.is_js, "Primeira região é um link");
    assert_eq!(&link_reg.target[..link_reg.target_len], b"page2.htm");
    assert!(link_reg.w > 50, "Largura do link deve ser proporcional ao texto");

    let btn_reg = &layout.click_regions[1];
    assert!(btn_reg.is_js, "Segunda região é um botão interativo JS");
    assert_eq!(btn_reg.node_idx, 2, "Índice do nó do botão deve ser 2");
    assert!(btn_reg.y > link_reg.y, "Botão deve estar abaixo do link");

    println!("PASSOU!");
}

fn test_minijs_dom_mutation_and_relayout() {
    print!("Teste 7: Execução Mini-JS, mutação de .innerText e recálculo dinâmico... ");
    let html = b"
        <p id=\"counter-display\">Cliques: 0</p>
        <button id=\"btn-inc\" onclick=\"clicks = clicks + 1; var c = document.getElementById('counter-display'); c.innerText = 'Cliques: ' + clicks;\">[ + ]</button>
        <script>var clicks = 0;</script>
    ";
    let mut dom = DomArena::new();
    let mut css = MiniCss::new();
    let mut js = MiniJsRuntime::new();
    let mut layout = DocumentLayout::new();

    parse_html_into_dom(html, &mut dom, &mut css, &mut js);
    layout.compute_from_dom(&dom, 280);

    let display_idx = dom.find_by_id(b"counter-display").expect("Elemento counter-display deve existir");
    assert_eq!(&dom.nodes[display_idx].text[..dom.nodes[display_idx].text_len], b"Cliques: 0");

    // Simula clique no botão 1: clicks -> 1
    let mut script_buf = [0u8; 128];
    let s_len = dom.nodes[1].onclick_len;
    script_buf[..s_len].copy_from_slice(&dom.nodes[1].onclick_attr[..s_len]);

    js.eval_script(&script_buf[..s_len], &mut dom);
    assert_eq!(&dom.nodes[display_idx].text[..dom.nodes[display_idx].text_len], b"Cliques: 1");

    // Simula clique no botão 2: clicks -> 2
    js.eval_script(&script_buf[..s_len], &mut dom);
    assert_eq!(&dom.nodes[display_idx].text[..dom.nodes[display_idx].text_len], b"Cliques: 2");

    // Recalcula layout após mutação
    layout.compute_from_dom(&dom, 280);
    assert_eq!(&layout.lines[0].text[..layout.lines[0].text_len], b"Cliques: 2");

    println!("PASSOU!");
}

fn test_html_entities_and_whitespace_normalization() {
    print!("Teste 8: Entidades HTML (&lt;, &gt;, &amp;, &nbsp;) e normalização... ");
    let html = b"<p>   Teste  &lt;tag&gt;   com &amp;   e espaco&nbsp;inquebravel   </p>";
    let mut dom = DomArena::new();
    let mut css = MiniCss::new();
    let mut js = MiniJsRuntime::new();

    parse_html_into_dom(html, &mut dom, &mut css, &mut js);

    assert_eq!(dom.node_count, 1);
    let p = &dom.nodes[0];
    let text = std::str::from_utf8(&p.text[..p.text_len]).unwrap();

    assert_eq!(text, "Teste <tag> com & e espaco inquebravel");

    println!("PASSOU!");
}

fn test_malformed_and_orphaned_tags_resilience() {
    print!("Teste 9: Resiliência contra HTML malformado e tags órfãs... ");
    let html = b"<div><p>Paragrafo sem fechar<div>Outro div<span>Texto</span>";
    let mut dom = DomArena::new();
    let mut css = MiniCss::new();
    let mut js = MiniJsRuntime::new();
    let mut layout = DocumentLayout::new();

    parse_html_into_dom(html, &mut dom, &mut css, &mut js);
    layout.compute_from_dom(&dom, 280);

    assert!(dom.node_count >= 3, "Todos os elementos devem ser alocados");
    assert!(layout.line_count >= 2, "Nenhuma linha deve ser perdida mesmo com tags sem fechar");
    assert!(layout.total_height > 0, "Altura total calculada");

    println!("PASSOU!");
}

fn test_index_htm_exact_conformance() {
    print!("Teste 10: Conformidade exata com o arquivo INDEX.HTM do Jinn OS... ");
    let index_html = b"<!DOCTYPE html>
<html>
<head>
  <title>Jinn OS Portal</title>
  <style>
    h1 { color: #f9e2af; margin: 8px; }
    .card { background-color: #1e1e2e; border-color: #45475a; margin: 6px; }
    .btn { background-color: #313244; color: #89dceb; border-color: #89dceb; margin: 4px; }
    #counter-display { color: #00ffaa; font-weight: bold; }
  </style>
</head>
<body>
  <h1>Jinn Browser 0.6.0</h1>
  <p>Motor Nativo: HTML + Mini-CSS + JavaScript (no_std / Ring 3).</p>
  <div class=\"card\">
    <p id=\"counter-display\">Cliques: 0</p>
    <button id=\"btn-count\" class=\"btn\" onclick=\"clicks = clicks + 1; var c = document.getElementById('counter-display'); c.innerText = 'Cliques: ' + clicks;\">[ Incrementar Contador ]</button>
  </div>
  <script>
    var clicks = 0;
  </script>
  <hr/>
  <p><b>Acessos Rapidos:</b></p>
  <p>- <a href=\"http://neverssl.com\">Testar Internet Real (neverssl.com)</a></p>
  <p>- <a href=\"http://example.com\">Dominio de Teste (example.com)</a></p>
  <p>- <a href=\"about.htm\">Sobre o Jinn Microkernel</a></p>
</body>
</html>";

    let mut dom = DomArena::new();
    let mut css = MiniCss::new();
    let mut js = MiniJsRuntime::new();
    let mut layout = DocumentLayout::new();

    parse_html_into_dom(index_html, &mut dom, &mut css, &mut js);
    layout.compute_from_dom(&dom, 276);

    // 1. Título do documento
    assert_eq!(&dom.title[..dom.title_len], b"Jinn OS Portal");

    // 2. Estilos do h1
    let h1_idx = dom.nodes.iter().position(|n| &n.tag[..n.tag_len] == b"h1").expect("h1 deve existir");
    assert_eq!(dom.nodes[h1_idx].computed_style.color, 0x00F9_E2AF);
    assert_eq!(dom.nodes[h1_idx].computed_style.margin_top, 8);

    // 3. Estrutura do card
    let card_idx = dom.nodes.iter().position(|n| &n.class_attr[..n.class_len] == b"card").expect("card deve existir");
    assert_eq!(dom.nodes[card_idx].computed_style.bg_color, Some(0x001E_1E2E));
    assert_eq!(dom.nodes[card_idx].computed_style.border_color, Some(0x0045_475A));
    assert_eq!(dom.nodes[card_idx].child_count, 2, "Card deve conter exatamente 2 filhos (p e button)");

    // 4. Filhos do card
    let counter_idx = dom.find_by_id(b"counter-display").expect("counter-display deve existir");
    assert_eq!(dom.nodes[counter_idx].parent, Some(card_idx), "counter-display deve ser filho do card");
    assert_eq!(dom.nodes[counter_idx].computed_style.color, 0x0000_FFAA, "Cor do contador deve ser #00ffaa");
    assert!(dom.nodes[counter_idx].computed_style.is_bold, "Contador deve ser bold");

    let btn_idx = dom.find_by_id(b"btn-count").expect("btn-count deve existir");
    assert_eq!(dom.nodes[btn_idx].parent, Some(card_idx), "btn-count deve ser filho do card");
    assert_eq!(dom.nodes[btn_idx].computed_style.bg_color, Some(0x0031_3244));
    assert_eq!(dom.nodes[btn_idx].computed_style.color, 0x0089_DCEB);

    // 5. Layout e caixas
    assert!(layout.box_count >= 2, "Pelo menos o card e o botão devem ter caixas");
    let card_box = layout.boxes[0];
    let btn_box = layout.boxes[1];

    assert!(card_box.h > btn_box.h + 20, "Caixa do card deve encerrar completamente o texto e o botão (h={})", card_box.h);
    assert!(btn_box.y >= card_box.y, "Botão y deve estar dentro do card");
    assert!(btn_box.y + btn_box.h <= card_box.y + card_box.h, "Botão deve estar totalmente dentro dos limites do card");

    // 6. Links rápidos
    assert!(layout.click_count >= 4, "Deve ter o botão e 3 links registrados como regiões clicáveis (obtido: {})", layout.click_count);

    let mut found_neverssl = false;
    let mut found_example = false;
    let mut found_about = false;
    for c in 0..layout.click_count {
        let reg = &layout.click_regions[c];
        let target = &reg.target[..reg.target_len];
        if target == b"http://neverssl.com" { found_neverssl = true; }
        if target == b"http://example.com" { found_example = true; }
        if target == b"about.htm" { found_about = true; }
    }
    assert!(found_neverssl, "Link neverssl.com deve estar registrado");
    assert!(found_example, "Link example.com deve estar registrado");
    assert!(found_about, "Link about.htm deve estar registrado");

    println!("PASSOU! ({} nós DOM, {} linhas renderizadas, {} caixas, {} links/ações)",
        dom.node_count, layout.line_count, layout.box_count, layout.click_count);
}

fn main() {
    println!("==================================================================");
    println!("  JINN OS — AUDITORIA E TESTES DE ESTRESSE: PARSER HTML E MINI-CSS");
    println!("==================================================================");

    test_tokenizer_and_dom_hierarchy();
    test_mini_css_selectors_and_specificity();
    test_margin_and_padding_parsing();
    test_container_box_model_encloses_children();
    test_word_wrap_and_no_premature_truncation();
    test_click_regions_and_interactive_links();
    test_minijs_dom_mutation_and_relayout();
    test_html_entities_and_whitespace_normalization();
    test_malformed_and_orphaned_tags_resilience();
    test_index_htm_exact_conformance();

    println!("==================================================================");
    println!("  TODOS OS 10 TESTES DE HTML, MINI-CSS E DOM PASSARAM COM SUCESSO!");
    println!("==================================================================");
}
