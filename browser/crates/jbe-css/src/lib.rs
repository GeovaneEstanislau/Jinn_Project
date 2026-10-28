//! CSS parser, selector matching, and computed style computation for Jinn Browser Engine.
//!
//! Handles a verifiable subset of CSS properties, cascade resolution, inheritance,
//! and default user-agent styles.

use std::collections::HashMap;

use jbe_core::{Color, Edges, NodeId};
use jbe_dom::{DomTree, Node, NodeType};

/// CSS display property value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    Block,
    Inline,
    None,
}

impl Default for Display {
    fn default() -> Self {
        Display::Inline
    }
}

/// CSS font weight property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontWeight {
    Normal,
    Bold,
}

impl Default for FontWeight {
    fn default() -> Self {
        FontWeight::Normal
    }
}

/// CSS text alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

impl Default for TextAlign {
    fn default() -> Self {
        TextAlign::Left
    }
}

/// Fully computed CSS style for a DOM node.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    pub display: Display,
    pub color: Color,
    pub background_color: Option<Color>,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub margin: Edges,
    pub padding: Edges,
    pub border: Edges,
    pub border_color: Color,
    pub font_size: f32,
    pub font_weight: FontWeight,
    pub text_align: TextAlign,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self {
            display: Display::Inline,
            color: Color::WHITE,
            background_color: None,
            width: None,
            height: None,
            margin: Edges::ZERO,
            padding: Edges::ZERO,
            border: Edges::ZERO,
            border_color: Color::TRANSPARENT,
            font_size: 14.0,
            font_weight: FontWeight::Normal,
            text_align: TextAlign::Left,
        }
    }
}

/// A parsed CSS property declaration.
#[derive(Debug, Clone, PartialEq)]
pub enum Property {
    Display(Display),
    Color(Color),
    BackgroundColor(Option<Color>),
    Width(Option<f32>),
    Height(Option<f32>),
    Margin(Edges),
    MarginTop(f32),
    MarginRight(f32),
    MarginBottom(f32),
    MarginLeft(f32),
    Padding(Edges),
    PaddingTop(f32),
    PaddingRight(f32),
    PaddingBottom(f32),
    PaddingLeft(f32),
    Border(Edges),
    BorderWidth(f32),
    BorderColor(Color),
    FontSize(f32),
    FontWeight(FontWeight),
    TextAlign(TextAlign),
}

/// A simple CSS selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    pub tag: Option<String>,
    pub class: Option<String>,
    pub id: Option<String>,
}

impl Selector {
    /// Computes selector specificity (ID: 100, Class: 10, Tag: 1).
    pub fn specificity(&self) -> u32 {
        let mut score = 0;
        if self.id.is_some() {
            score += 100;
        }
        if self.class.is_some() {
            score += 10;
        }
        if self.tag.is_some() {
            score += 1;
        }
        score
    }

    /// Checks if this selector matches a DOM node.
    pub fn matches(&self, node: &Node) -> bool {
        if let NodeType::Element { tag_name, .. } = &node.node_type {
            if let Some(ref req_tag) = self.tag {
                if req_tag != "*" && !req_tag.eq_ignore_ascii_case(tag_name) {
                    return false;
                }
            }
            if let Some(ref req_id) = self.id {
                match node.id_attr() {
                    Some(id) if id == req_id => {}
                    _ => return false,
                }
            }
            if let Some(ref req_class) = self.class {
                if !node.has_class(req_class) {
                    return false;
                }
            }
            true
        } else {
            false
        }
    }
}

/// A CSS rule combining a selector and a list of property declarations.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub selector: Selector,
    pub properties: Vec<Property>,
}

/// A parsed stylesheet containing rules.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StyleSheet {
    pub rules: Vec<Rule>,
}

impl StyleSheet {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Parses a CSS string into a StyleSheet.
    pub fn parse(css: &str) -> Self {
        let mut sheet = StyleSheet::new();
        let mut rest = css;

        while let Some(brace_pos) = rest.find('{') {
            let selector_str = rest[..brace_pos].trim();
            rest = &rest[brace_pos + 1..];

            if let Some(close_brace) = rest.find('}') {
                let decls_str = &rest[..close_brace];
                rest = &rest[close_brace + 1..];

                for sel_part in selector_str.split(',') {
                    let sel = parse_selector(sel_part.trim());
                    let props = parse_declarations(decls_str);
                    if !props.is_empty() {
                        sheet.rules.push(Rule {
                            selector: sel,
                            properties: props,
                        });
                    }
                }
            } else {
                break;
            }
        }

        sheet
    }

    /// Default User-Agent stylesheet for Jinn OS dark-themed aesthetic.
    pub fn default_user_agent() -> Self {
        let css = r#"
            head, style, title, meta, link, script {
                display: none;
            }
            html, body {
                display: block;
                color: #e0e0e8;
                background-color: #12121a;
                font-size: 14;
                font-weight: normal;
                margin: 0;
                padding: 8;
            }
            h1 {
                display: block;
                font-size: 24;
                font-weight: bold;
                margin: 12 0;
                color: #ffffff;
            }
            h2 {
                display: block;
                font-size: 20;
                font-weight: bold;
                margin: 10 0;
                color: #ffffff;
            }
            h3 {
                display: block;
                font-size: 16;
                font-weight: bold;
                margin: 8 0;
                color: #ffffff;
            }
            h4, h5, h6 {
                display: block;
                font-size: 14;
                font-weight: bold;
                margin: 6 0;
            }
            p {
                display: block;
                margin: 8 0;
            }
            div {
                display: block;
            }
            span, strong, em, a {
                display: inline;
            }
            strong {
                font-weight: bold;
            }
            a {
                color: #4da6ff;
            }
            hr {
                display: block;
                height: 1;
                margin: 8 0;
                background-color: #333344;
            }
            ul, ol {
                display: block;
                margin: 6 0;
                padding: 0 0 0 16;
            }
            li {
                display: block;
                margin: 2 0;
            }
        "#;
        Self::parse(css)
    }
}

/// Parses a simple CSS selector string (e.g. "h1", ".box", "#main", "div.card").
pub fn parse_selector(s: &str) -> Selector {
    let s = s.trim();
    if s.is_empty() {
        return Selector {
            tag: None,
            class: None,
            id: None,
        };
    }

    let mut tag = None;
    let mut class = None;
    let mut id = None;

    if s.starts_with('.') {
        class = Some(s[1..].to_string());
    } else if s.starts_with('#') {
        id = Some(s[1..].to_string());
    } else if let Some(dot_idx) = s.find('.') {
        tag = Some(s[..dot_idx].to_ascii_lowercase());
        class = Some(s[dot_idx + 1..].to_string());
    } else if let Some(hash_idx) = s.find('#') {
        tag = Some(s[..hash_idx].to_ascii_lowercase());
        id = Some(s[hash_idx + 1..].to_string());
    } else {
        tag = Some(s.to_ascii_lowercase());
    }

    Selector { tag, class, id }
}

/// Parses a declarations block (e.g. `color: red; margin: 10px;`).
pub fn parse_declarations(decls: &str) -> Vec<Property> {
    let mut properties = Vec::new();
    for part in decls.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(colon) = part.find(':') {
            let key = part[..colon].trim().to_ascii_lowercase();
            let val = part[colon + 1..].trim();
            if let Some(prop) = parse_property(&key, val) {
                properties.push(prop);
            }
        }
    }
    properties
}

fn parse_property(key: &str, val: &str) -> Option<Property> {
    match key {
        "display" => match val.to_ascii_lowercase().as_str() {
            "block" => Some(Property::Display(Display::Block)),
            "inline" => Some(Property::Display(Display::Inline)),
            "none" => Some(Property::Display(Display::None)),
            _ => None,
        },
        "color" => parse_color(val).map(Property::Color),
        "background-color" | "background" => {
            if val.eq_ignore_ascii_case("none") || val.eq_ignore_ascii_case("transparent") {
                Some(Property::BackgroundColor(None))
            } else {
                parse_color(val).map(|c| Property::BackgroundColor(Some(c)))
            }
        }
        "width" => {
            if val.eq_ignore_ascii_case("auto") {
                Some(Property::Width(None))
            } else {
                parse_length(val).map(|v| Property::Width(Some(v)))
            }
        }
        "height" => {
            if val.eq_ignore_ascii_case("auto") {
                Some(Property::Height(None))
            } else {
                parse_length(val).map(|v| Property::Height(Some(v)))
            }
        }
        "margin" => parse_edges(val).map(Property::Margin),
        "margin-top" => parse_length(val).map(Property::MarginTop),
        "margin-right" => parse_length(val).map(Property::MarginRight),
        "margin-bottom" => parse_length(val).map(Property::MarginBottom),
        "margin-left" => parse_length(val).map(Property::MarginLeft),
        "padding" => parse_edges(val).map(Property::Padding),
        "padding-top" => parse_length(val).map(Property::PaddingTop),
        "padding-right" => parse_length(val).map(Property::PaddingRight),
        "padding-bottom" => parse_length(val).map(Property::PaddingBottom),
        "padding-left" => parse_length(val).map(Property::PaddingLeft),
        "border" => {
            // Simplified border: e.g. "1px solid #444" or just "1"
            let parts: Vec<&str> = val.split_whitespace().collect();
            if let Some(&first) = parts.first() {
                if let Some(w) = parse_length(first) {
                    return Some(Property::Border(Edges::all(w)));
                }
            }
            None
        }
        "border-width" => parse_length(val).map(Property::BorderWidth),
        "border-color" => parse_color(val).map(Property::BorderColor),
        "font-size" => parse_length(val).map(Property::FontSize),
        "font-weight" => match val.to_ascii_lowercase().as_str() {
            "bold" | "700" | "800" | "900" => Some(Property::FontWeight(FontWeight::Bold)),
            _ => Some(Property::FontWeight(FontWeight::Normal)),
        },
        "text-align" => match val.to_ascii_lowercase().as_str() {
            "center" => Some(Property::TextAlign(TextAlign::Center)),
            "right" => Some(Property::TextAlign(TextAlign::Right)),
            _ => Some(Property::TextAlign(TextAlign::Left)),
        },
        _ => None,
    }
}

/// Parses a length in pixels (e.g. "12px", "12", "1.5").
pub fn parse_length(val: &str) -> Option<f32> {
    let s = val.trim().trim_end_matches("px");
    s.parse::<f32>().ok().filter(|&v| v >= 0.0)
}

/// Parses 1, 2, or 4 values into Edges (top, right, bottom, left).
pub fn parse_edges(val: &str) -> Option<Edges> {
    let parts: Vec<f32> = val
        .split_whitespace()
        .filter_map(parse_length)
        .collect();

    match parts.len() {
        1 => Some(Edges::all(parts[0])),
        2 => Some(Edges::symmetric(parts[0], parts[1])),
        4 => Some(Edges::new(parts[0], parts[1], parts[2], parts[3])),
        _ => None,
    }
}

/// Parses hex (#rgb, #rrggbb), rgb(r, g, b), or named colors.
pub fn parse_color(val: &str) -> Option<Color> {
    let s = val.trim();
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 3 {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
            return Some(Color::rgb(r * 17, g * 17, b * 17));
        } else if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Color::rgb(r, g, b));
        }
    } else if s.starts_with("rgb(") && s.ends_with(')') {
        let inside = &s[4..s.len() - 1];
        let parts: Vec<u8> = inside
            .split(',')
            .filter_map(|p| p.trim().parse::<u8>().ok())
            .collect();
        if parts.len() == 3 {
            return Some(Color::rgb(parts[0], parts[1], parts[2]));
        }
    } else {
        match s.to_ascii_lowercase().as_str() {
            "black" => return Some(Color::BLACK),
            "white" => return Some(Color::WHITE),
            "red" => return Some(Color::rgb(255, 0, 0)),
            "green" => return Some(Color::rgb(0, 255, 0)),
            "blue" => return Some(Color::rgb(0, 0, 255)),
            "gray" | "grey" => return Some(Color::rgb(128, 128, 128)),
            "yellow" => return Some(Color::rgb(255, 255, 0)),
            "cyan" => return Some(Color::rgb(0, 255, 255)),
            "magenta" => return Some(Color::rgb(255, 0, 255)),
            "transparent" => return Some(Color::TRANSPARENT),
            _ => {}
        }
    }
    None
}

/// Applies a property to a ComputedStyle.
pub fn apply_property(style: &mut ComputedStyle, prop: &Property) {
    match prop {
        Property::Display(d) => style.display = *d,
        Property::Color(c) => style.color = *c,
        Property::BackgroundColor(bg) => style.background_color = *bg,
        Property::Width(w) => style.width = *w,
        Property::Height(h) => style.height = *h,
        Property::Margin(m) => style.margin = *m,
        Property::MarginTop(v) => style.margin.top = *v,
        Property::MarginRight(v) => style.margin.right = *v,
        Property::MarginBottom(v) => style.margin.bottom = *v,
        Property::MarginLeft(v) => style.margin.left = *v,
        Property::Padding(p) => style.padding = *p,
        Property::PaddingTop(v) => style.padding.top = *v,
        Property::PaddingRight(v) => style.padding.right = *v,
        Property::PaddingBottom(v) => style.padding.bottom = *v,
        Property::PaddingLeft(v) => style.padding.left = *v,
        Property::Border(b) => style.border = *b,
        Property::BorderWidth(w) => style.border = Edges::all(*w),
        Property::BorderColor(c) => style.border_color = *c,
        Property::FontSize(fs) => style.font_size = *fs,
        Property::FontWeight(fw) => style.font_weight = *fw,
        Property::TextAlign(ta) => style.text_align = *ta,
    }
}

/// Resolves computed styles for every node in a DOM tree.
pub fn resolve_styles(
    dom: &DomTree,
    author_stylesheet: &StyleSheet,
) -> HashMap<NodeId, ComputedStyle> {
    let ua_sheet = StyleSheet::default_user_agent();
    let mut computed_map: HashMap<NodeId, ComputedStyle> = HashMap::new();

    // Traverse preorder to ensure parents are computed before children for inheritance
    for node_id in dom.preorder() {
        let node = match dom.get(node_id) {
            Some(n) => n,
            None => continue,
        };

        // Determine base style: inherit inherited properties from parent if present
        let mut style = if let Some(parent_id) = node.parent {
            if let Some(parent_style) = computed_map.get(&parent_id) {
                let mut inherited = ComputedStyle::default();
                inherited.color = parent_style.color;
                inherited.font_size = parent_style.font_size;
                inherited.font_weight = parent_style.font_weight;
                inherited.text_align = parent_style.text_align;
                inherited
            } else {
                ComputedStyle::default()
            }
        } else {
            ComputedStyle::default()
        };

        // Collect matching rules and sort by specificity
        // UA sheet has base specificity 0
        // Author sheet has normal specificity
        let mut matched_props: Vec<(u32, Property)> = Vec::new();

        for rule in &ua_sheet.rules {
            if rule.selector.matches(node) {
                let spec = rule.selector.specificity();
                for prop in &rule.properties {
                    matched_props.push((spec, prop.clone()));
                }
            }
        }

        for rule in &author_stylesheet.rules {
            if rule.selector.matches(node) {
                let spec = 1000 + rule.selector.specificity();
                for prop in &rule.properties {
                    matched_props.push((spec, prop.clone()));
                }
            }
        }

        // Inline styles have highest specificity (e.g. 100_000)
        if let Some(inline_style_str) = node.get_attribute("style") {
            let inline_props = parse_declarations(inline_style_str);
            for prop in inline_props {
                matched_props.push((100_000, prop));
            }
        }

        // Sort stable by specificity so higher specificity overrides
        matched_props.sort_by_key(|&(spec, _)| spec);

        for (_, prop) in matched_props {
            apply_property(&mut style, &prop);
        }

        computed_map.insert(node_id, style);
    }

    computed_map
}

#[cfg(test)]
mod tests {
    use super::*;
    use jbe_core::EngineLimits;

    #[test]
    fn test_parse_color_and_length() {
        assert_eq!(parse_color("#fff"), Some(Color::WHITE));
        assert_eq!(parse_color("#000000"), Some(Color::BLACK));
        assert_eq!(parse_color("rgb(10, 20, 30)"), Some(Color::rgb(10, 20, 30)));
        assert_eq!(parse_color("red"), Some(Color::rgb(255, 0, 0)));

        assert_eq!(parse_length("16px"), Some(16.0));
        assert_eq!(parse_length("24"), Some(24.0));
        assert_eq!(parse_length("invalid"), None);
    }

    #[test]
    fn test_selector_matching_and_specificity() {
        let sel_tag = parse_selector("h1");
        let sel_class = parse_selector(".card");
        let sel_id = parse_selector("#header");

        assert_eq!(sel_tag.specificity(), 1);
        assert_eq!(sel_class.specificity(), 10);
        assert_eq!(sel_id.specificity(), 100);
    }

    #[test]
    fn test_style_resolution_and_cascade() {
        let limits = EngineLimits::default();
        let mut dom = DomTree::new(limits);
        let div = dom
            .create_element(
                "div",
                vec![
                    ("class".into(), "box".into()),
                    ("style".into(), "background-color: red;".into()),
                ],
            )
            .unwrap();
        dom.append_child(dom.root(), div).unwrap();

        let css = r#"
            div { background-color: blue; padding: 10; }
            .box { color: green; }
        "#;
        let sheet = StyleSheet::parse(css);
        let styles = resolve_styles(&dom, &sheet);
        let div_style = styles.get(&div).unwrap();

        // .box rule applied color green
        assert_eq!(div_style.color, Color::rgb(0, 255, 0));
        // inline style overrides div { background-color: blue }
        assert_eq!(div_style.background_color, Some(Color::rgb(255, 0, 0)));
        // div rule applied padding 10
        assert_eq!(div_style.padding, Edges::all(10.0));
        assert_eq!(div_style.display, Display::Block);
    }

    #[test]
    fn test_inheritance() {
        let limits = EngineLimits::default();
        let mut dom = DomTree::new(limits);
        let div = dom
            .create_element("div", vec![("style".into(), "color: #123456;".into())])
            .unwrap();
        let p = dom.create_element("p", vec![]).unwrap();
        let span = dom.create_element("span", vec![]).unwrap();

        dom.append_child(dom.root(), div).unwrap();
        dom.append_child(div, p).unwrap();
        dom.append_child(p, span).unwrap();

        let sheet = StyleSheet::new();
        let styles = resolve_styles(&dom, &sheet);

        let span_style = styles.get(&span).unwrap();
        assert_eq!(span_style.color, Color::rgb(0x12, 0x34, 0x56));
    }
}
