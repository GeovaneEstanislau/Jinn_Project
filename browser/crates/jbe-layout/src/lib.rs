//! Block flow layout engine for Jinn Browser Engine.
//!
//! Converts DOM nodes and computed styles into a geometric box tree,
//! calculating dimensions, margins, padding, line wrapping, and vertical stacking.

use std::collections::HashMap;

use jbe_core::{Edges, NodeId, Rect};
use jbe_css::{ComputedStyle, Display, TextAlign};
use jbe_dom::{DomTree, NodeType};

/// Box type classification.
#[derive(Debug, Clone, PartialEq)]
pub enum BoxType {
    Block,
    Inline,
    AnonymousBlock,
    Text(String),
}

/// Geometric dimensions and box model edges.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BoxDimensions {
    pub content: Rect,
    pub padding: Edges,
    pub border: Edges,
    pub margin: Edges,
}

impl BoxDimensions {
    /// Returns the padding box (content expanded by padding).
    pub fn padding_box(&self) -> Rect {
        self.content.expanded_by(&self.padding)
    }

    /// Returns the border box (padding box expanded by border).
    pub fn border_box(&self) -> Rect {
        self.padding_box().expanded_by(&self.border)
    }

    /// Returns the margin box (border box expanded by margin).
    pub fn margin_box(&self) -> Rect {
        self.border_box().expanded_by(&self.margin)
    }
}

/// A line of wrapped text.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLine {
    pub text: String,
    pub rect: Rect,
}

/// A layout box in the renderable box tree.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutBox {
    pub node_id: Option<NodeId>,
    pub box_type: BoxType,
    pub dimensions: BoxDimensions,
    pub style: ComputedStyle,
    pub children: Vec<LayoutBox>,
    pub lines: Vec<TextLine>,
}

impl LayoutBox {
    pub fn new(box_type: BoxType, style: ComputedStyle, node_id: Option<NodeId>) -> Self {
        Self {
            node_id,
            box_type,
            dimensions: BoxDimensions::default(),
            style,
            children: Vec::new(),
            lines: Vec::new(),
        }
    }

    /// Approximate width of a text string given font size.
    pub fn measure_text_width(text: &str, font_size: f32) -> f32 {
        // Monospace/standard glyph approximation: 0.6 * font_size per character
        let char_count = text.chars().count();
        char_count as f32 * (font_size * 0.6)
    }

    /// Line height based on font size.
    pub fn line_height(font_size: f32) -> f32 {
        (font_size * 1.3).ceil()
    }

    /// Computes the maximum bottom extent (Y + height) of this box and all its children.
    pub fn content_height(&self) -> f32 {
        let margin_b = self.dimensions.margin_box();
        let mut max_y = margin_b.y + margin_b.height;
        for line in &self.lines {
            max_y = max_y.max(line.rect.y + line.rect.height);
        }
        for child in &self.children {
            max_y = max_y.max(child.content_height());
        }
        max_y
    }
}

/// Builds the layout box tree from DOM and computed styles, then executes layout.
pub fn build_and_layout_tree(
    dom: &DomTree,
    styles: &HashMap<NodeId, ComputedStyle>,
    viewport_width: f32,
) -> Option<LayoutBox> {
    let mut root_box = build_box_tree(dom.root(), dom, styles)?;
    let containing_block = Rect::new(0.0, 0.0, viewport_width, 0.0);
    layout_box(&mut root_box, &containing_block);
    Some(root_box)
}

/// Recursively builds the layout box tree from the DOM.
fn build_box_tree(
    node_id: NodeId,
    dom: &DomTree,
    styles: &HashMap<NodeId, ComputedStyle>,
) -> Option<LayoutBox> {
    let node = dom.get(node_id)?;
    let style = styles.get(&node_id).cloned().unwrap_or_default();

    if style.display == Display::None {
        return None;
    }

    match &node.node_type {
        NodeType::Document => {
            let mut doc_box = LayoutBox::new(BoxType::Block, style, Some(node_id));
            for &child_id in &node.children {
                if let Some(child_box) = build_box_tree(child_id, dom, styles) {
                    doc_box.children.push(child_box);
                }
            }
            Some(doc_box)
        }
        NodeType::Element { .. } => {
            let box_type = match style.display {
                Display::Block => BoxType::Block,
                Display::Inline => BoxType::Inline,
                Display::None => return None,
            };

            let mut el_box = LayoutBox::new(box_type, style, Some(node_id));
            for &child_id in &node.children {
                if let Some(child_box) = build_box_tree(child_id, dom, styles) {
                    el_box.children.push(child_box);
                }
            }
            Some(el_box)
        }
        NodeType::Text(text) => {
            // Trim or collapse whitespace
            let text_trimmed = text.trim();
            if text_trimmed.is_empty() {
                return None;
            }
            Some(LayoutBox::new(
                BoxType::Text(text.clone()),
                style,
                Some(node_id),
            ))
        }
        NodeType::Comment(_) => None,
    }
}

/// Executes layout on a box given its containing block geometry.
pub fn layout_box(box_node: &mut LayoutBox, containing_block: &Rect) {
    box_node.dimensions.margin = box_node.style.margin;
    box_node.dimensions.padding = box_node.style.padding;
    box_node.dimensions.border = box_node.style.border;

    // Calculate content width
    let total_h_extras = box_node.dimensions.margin.left
        + box_node.dimensions.margin.right
        + box_node.dimensions.border.left
        + box_node.dimensions.border.right
        + box_node.dimensions.padding.left
        + box_node.dimensions.padding.right;

    let content_width = if let Some(w) = box_node.style.width {
        w
    } else {
        (containing_block.width - total_h_extras).max(0.0)
    };

    box_node.dimensions.content.width = content_width;
    box_node.dimensions.content.x = containing_block.x
        + box_node.dimensions.margin.left
        + box_node.dimensions.border.left
        + box_node.dimensions.padding.left;

    box_node.dimensions.content.y = containing_block.y
        + containing_block.height
        + box_node.dimensions.margin.top
        + box_node.dimensions.border.top
        + box_node.dimensions.padding.top;

    // Layout children or text
    let maybe_text = match &box_node.box_type {
        BoxType::Text(text) => Some(text.clone()),
        _ => None,
    };

    if let Some(text) = maybe_text {
        layout_text_lines(box_node, &text, content_width);
    } else {
        let mut cursor_y = 0.0;
        // Collect inline text from children or layout block children
        let mut child_containing_block = Rect::new(
            box_node.dimensions.content.x,
            box_node.dimensions.content.y,
            content_width,
            0.0,
        );

        for child in &mut box_node.children {
            child_containing_block.height = cursor_y;
            layout_box(child, &child_containing_block);
            let child_mb = child.dimensions.margin_box();
            cursor_y += child_mb.height;
        }

        // Content height is either explicitly specified or calculated from children
        if let Some(h) = box_node.style.height {
            box_node.dimensions.content.height = h;
        } else {
            box_node.dimensions.content.height = cursor_y;
        }
    }
}

/// Breaks text into wrapped lines and calculates total height.
fn layout_text_lines(layout_box: &mut LayoutBox, text: &str, available_width: f32) {
    let font_size = layout_box.style.font_size;
    let line_height = LayoutBox::line_height(font_size);
    let space_width = LayoutBox::measure_text_width(" ", font_size);

    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        layout_box.dimensions.content.height = 0.0;
        return;
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0.0;

    for word in words {
        let word_width = LayoutBox::measure_text_width(word, font_size);

        if current_line.is_empty() {
            current_line.push_str(word);
            current_width = word_width;
        } else if current_width + space_width + word_width <= available_width {
            current_line.push(' ');
            current_line.push_str(word);
            current_width += space_width + word_width;
        } else {
            // Line break
            lines.push((current_line, current_width));
            current_line = word.to_string();
            current_width = word_width;
        }
    }

    if !current_line.is_empty() {
        lines.push((current_line, current_width));
    }

    // Convert into TextLine items with coordinates
    let origin_x = layout_box.dimensions.content.x;
    let origin_y = layout_box.dimensions.content.y;
    let mut total_height = 0.0;

    layout_box.lines.clear();
    for (idx, (line_str, line_w)) in lines.into_iter().enumerate() {
        let y = origin_y + (idx as f32 * line_height);

        // Alignment offset
        let x_offset = match layout_box.style.text_align {
            TextAlign::Left => 0.0,
            TextAlign::Center => ((available_width - line_w) / 2.0).max(0.0),
            TextAlign::Right => (available_width - line_w).max(0.0),
        };

        layout_box.lines.push(TextLine {
            text: line_str,
            rect: Rect::new(origin_x + x_offset, y, line_w, line_height),
        });
        total_height += line_height;
    }

    layout_box.dimensions.content.height = total_height;
}

#[cfg(test)]
mod tests {
    use super::*;
    use jbe_core::EngineLimits;
    use jbe_css::StyleSheet;
    use jbe_dom::DomTree;

    #[test]
    fn test_block_flow_stacking() {
        let limits = EngineLimits::default();
        let mut dom = DomTree::new(limits);
        let div1 = dom
            .create_element(
                "div",
                vec![("style".into(), "height: 50px; margin: 10px;".into())],
            )
            .unwrap();
        let div2 = dom
            .create_element(
                "div",
                vec![("style".into(), "height: 30px; margin: 5px;".into())],
            )
            .unwrap();

        dom.append_child(dom.root(), div1).unwrap();
        dom.append_child(dom.root(), div2).unwrap();

        let sheet = StyleSheet::new();
        let styles = jbe_css::resolve_styles(&dom, &sheet);
        let root_box = build_and_layout_tree(&dom, &styles, 800.0).unwrap();

        // First child margin box height: 10 (top) + 50 (height) + 10 (bottom) = 70
        let b1 = &root_box.children[0];
        assert_eq!(b1.dimensions.content.height, 50.0);
        assert_eq!(b1.dimensions.margin_box().height, 70.0);

        // Second child starts below b1's margin box
        let b2 = &root_box.children[1];
        assert_eq!(b2.dimensions.content.height, 30.0);
        assert_eq!(b2.dimensions.content.y, 70.0 + 5.0); // 75.0
    }

    #[test]
    fn test_text_wrapping() {
        let limits = EngineLimits::default();
        let mut dom = DomTree::new(limits);
        let p = dom
            .create_element(
                "p",
                vec![("style".into(), "font-size: 10px; width: 60px;".into())],
            )
            .unwrap();
        let txt = dom.create_text("one two three four five").unwrap();
        dom.append_child(dom.root(), p).unwrap();
        dom.append_child(p, txt).unwrap();

        let sheet = StyleSheet::new();
        let styles = jbe_css::resolve_styles(&dom, &sheet);
        let root_box = build_and_layout_tree(&dom, &styles, 500.0).unwrap();

        let p_box = &root_box.children[0];
        let txt_box = &p_box.children[0];

        // Font size 10 means ~6px per char. "one" is 18px. "one two" is ~42px.
        // With width 60px, it should wrap across multiple lines!
        assert!(txt_box.lines.len() > 1);
        assert!(txt_box.dimensions.content.height > LayoutBox::line_height(10.0));
    }
}
