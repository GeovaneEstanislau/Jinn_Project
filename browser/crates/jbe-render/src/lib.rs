//! Paint command generator and deterministic rendering backend for Jinn Browser Engine.
//!
//! Transforms the geometric LayoutBox tree into an ordered stream of paint commands
//! and provides both a test recording backend and an in-memory ARGB software rasterizer.

use jbe_core::{Color, Edges, Point, Rect, ResourceId};
use jbe_dom::DomTree;
use jbe_layout::LayoutBox;

/// Paint commands emitted by the rendering engine.
#[derive(Debug, Clone, PartialEq)]
pub enum PaintCommand {
    FillRect {
        rect: Rect,
        color: Color,
    },
    DrawBorder {
        rect: Rect,
        edges: Edges,
        color: Color,
    },
    DrawText {
        text: String,
        point: Point,
        font_size: f32,
        color: Color,
    },
    DrawImage {
        resource_id: ResourceId,
        rect: Rect,
    },
    RegisterClickRegion {
        rect: Rect,
        target_url: String,
    },
    ClipRect {
        rect: Rect,
    },
}

/// Abstract rendering backend interface.
pub trait RenderBackend {
    fn begin_frame(&mut self, width: u32, height: u32);
    fn execute_command(&mut self, cmd: &PaintCommand);
    fn end_frame(&mut self);
}

/// Basic 8x8 font glyph bitmap for ASCII characters 32..127.
mod embedded_font {
    // 8x8 simple bitmap font for ASCII 32..126
    pub fn get_glyph(c: char) -> &'static [u8; 8] {
        static DEFAULT_GLYPH: [u8; 8] = [0xFF, 0x81, 0x81, 0x81, 0x81, 0x81, 0x81, 0xFF]; // Box
        static SPACE: [u8; 8] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        static CHAR_A: [u8; 8] = [0x18, 0x24, 0x42, 0x7E, 0x42, 0x42, 0x42, 0x00];
        static CHAR_B: [u8; 8] = [0x7C, 0x42, 0x42, 0x7C, 0x42, 0x42, 0x7C, 0x00];
        static CHAR_C: [u8; 8] = [0x3C, 0x42, 0x40, 0x40, 0x40, 0x42, 0x3C, 0x00];
        static CHAR_D: [u8; 8] = [0x78, 0x44, 0x42, 0x42, 0x42, 0x44, 0x78, 0x00];
        static CHAR_E: [u8; 8] = [0x7E, 0x40, 0x40, 0x78, 0x40, 0x40, 0x7E, 0x00];
        static CHAR_H: [u8; 8] = [0x42, 0x42, 0x42, 0x7E, 0x42, 0x42, 0x42, 0x00];
        static CHAR_I: [u8; 8] = [0x3E, 0x1C, 0x08, 0x08, 0x08, 0x1C, 0x3E, 0x00];
        static CHAR_J: [u8; 8] = [0x0E, 0x04, 0x04, 0x04, 0x44, 0x44, 0x38, 0x00];
        static CHAR_N: [u8; 8] = [0x42, 0x62, 0x52, 0x4A, 0x46, 0x42, 0x42, 0x00];
        static CHAR_O: [u8; 8] = [0x3C, 0x42, 0x42, 0x42, 0x42, 0x42, 0x3C, 0x00];

        match c {
            ' ' => &SPACE,
            'A' | 'a' => &CHAR_A,
            'B' | 'b' => &CHAR_B,
            'C' | 'c' => &CHAR_C,
            'D' | 'd' => &CHAR_D,
            'E' | 'e' => &CHAR_E,
            'H' | 'h' => &CHAR_H,
            'I' | 'i' => &CHAR_I,
            'J' | 'j' => &CHAR_J,
            'N' | 'n' => &CHAR_N,
            'O' | 'o' => &CHAR_O,
            _ => &DEFAULT_GLYPH,
        }
    }
}

/// A deterministic in-memory test backend.
///
/// Records all emitted paint commands, renders them to an in-memory ARGB32 framebuffer,
/// and can generate an ASCII art canvas representation for tests and CLI output.
#[derive(Debug, Clone)]
pub struct TestBackend {
    pub commands: Vec<PaintCommand>,
    pub width: u32,
    pub height: u32,
    pub framebuffer: Vec<u32>,
    pub click_regions: Vec<(Rect, String)>,
}

impl TestBackend {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            commands: Vec::new(),
            width,
            height,
            framebuffer: vec![0xFF000000; (width * height) as usize],
            click_regions: Vec::new(),
        }
    }

    /// Returns the pixel color at (x, y).
    pub fn get_pixel(&self, x: u32, y: u32) -> Option<Color> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let val = self.framebuffer[(y * self.width + x) as usize];
        let a = ((val >> 24) & 0xFF) as u8;
        let r = ((val >> 16) & 0xFF) as u8;
        let g = ((val >> 8) & 0xFF) as u8;
        let b = (val & 0xFF) as u8;
        Some(Color::rgba(r, g, b, a))
    }

    /// Sets pixel at (x, y) with bounds checking.
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x < self.width && y < self.height {
            self.framebuffer[(y * self.width + x) as usize] = color.to_argb();
        }
    }

    /// Finds if a given click point lands on a registered click region.
    pub fn click_at(&self, point: Point) -> Option<&str> {
        for (rect, url) in &self.click_regions {
            if rect.contains(point) {
                return Some(url.as_str());
            }
        }
        None
    }

    /// Generates an ASCII preview of the rendered output.
    pub fn render_ascii_canvas(&self, cols: u32, rows: u32) -> String {
        let mut out = String::new();
        let step_x = (self.width as f32 / cols as f32).max(1.0);
        let step_y = (self.height as f32 / rows as f32).max(1.0);

        for r in 0..rows {
            let py = ((r as f32 + 0.5) * step_y) as u32;
            for c in 0..cols {
                let px = ((c as f32 + 0.5) * step_x) as u32;
                if let Some(col) = self.get_pixel(px, py) {
                    let luma = 0.299 * (col.r as f32) + 0.587 * (col.g as f32) + 0.114 * (col.b as f32);
                    let ch = if luma > 200.0 {
                        '#'
                    } else if luma > 150.0 {
                        '*'
                    } else if luma > 100.0 {
                        '+'
                    } else if luma > 50.0 {
                        '.'
                    } else {
                        ' '
                    };
                    out.push(ch);
                } else {
                    out.push(' ');
                }
            }
            out.push('\n');
        }
        out
    }
}

impl RenderBackend for TestBackend {
    fn begin_frame(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.commands.clear();
        self.click_regions.clear();
        self.framebuffer.resize((width * height) as usize, 0xFF000000);
        self.framebuffer.fill(0xFF000000);
    }

    fn execute_command(&mut self, cmd: &PaintCommand) {
        self.commands.push(cmd.clone());

        match cmd {
            PaintCommand::FillRect { rect, color } => {
                let x0 = (rect.x.max(0.0) as u32).min(self.width);
                let y0 = (rect.y.max(0.0) as u32).min(self.height);
                let x1 = ((rect.x + rect.width).max(0.0) as u32).min(self.width);
                let y1 = ((rect.y + rect.height).max(0.0) as u32).min(self.height);

                for y in y0..y1 {
                    for x in x0..x1 {
                        self.set_pixel(x, y, *color);
                    }
                }
            }
            PaintCommand::DrawBorder { rect, edges, color } => {
                let x0 = rect.x as u32;
                let y0 = rect.y as u32;
                let x1 = (rect.x + rect.width) as u32;
                let y1 = (rect.y + rect.height) as u32;

                // Top border
                for y in y0..y0 + edges.top as u32 {
                    for x in x0..x1 {
                        self.set_pixel(x, y, *color);
                    }
                }
                // Bottom border
                for y in y1.saturating_sub(edges.bottom as u32)..y1 {
                    for x in x0..x1 {
                        self.set_pixel(x, y, *color);
                    }
                }
                // Left border
                for y in y0..y1 {
                    for x in x0..x0 + edges.left as u32 {
                        self.set_pixel(x, y, *color);
                    }
                }
                // Right border
                for y in y0..y1 {
                    for x in x1.saturating_sub(edges.right as u32)..x1 {
                        self.set_pixel(x, y, *color);
                    }
                }
            }
            PaintCommand::DrawText { text, point, color, .. } => {
                let mut start_x = point.x as u32;
                let start_y = point.y as u32;

                for ch in text.chars() {
                    let glyph = embedded_font::get_glyph(ch);
                    for (row, &byte) in glyph.iter().enumerate() {
                        for col in 0..8 {
                            if (byte & (0x80 >> col)) != 0 {
                                self.set_pixel(start_x + col, start_y + row as u32, *color);
                            }
                        }
                    }
                    start_x += 8;
                }
            }
            PaintCommand::RegisterClickRegion { rect, target_url } => {
                self.click_regions.push((*rect, target_url.clone()));
            }
            _ => {}
        }
    }

    fn end_frame(&mut self) {}
}

/// Recursively generates paint commands from the layout box tree.
pub fn generate_paint_commands(root_box: &LayoutBox, dom: &DomTree) -> Vec<PaintCommand> {
    let mut commands = Vec::new();
    collect_paint_commands(root_box, dom, &mut commands);
    commands
}

fn collect_paint_commands(
    layout_box: &LayoutBox,
    dom: &DomTree,
    commands: &mut Vec<PaintCommand>,
) {
    let padding_box = layout_box.dimensions.padding_box();
    let border_box = layout_box.dimensions.border_box();

    // 1. Background fill
    if let Some(bg_color) = layout_box.style.background_color {
        commands.push(PaintCommand::FillRect {
            rect: padding_box,
            color: bg_color,
        });
    }

    // 2. Borders
    if layout_box.dimensions.border.top > 0.0
        || layout_box.dimensions.border.right > 0.0
        || layout_box.dimensions.border.bottom > 0.0
        || layout_box.dimensions.border.left > 0.0
    {
        commands.push(PaintCommand::DrawBorder {
            rect: border_box,
            edges: layout_box.dimensions.border,
            color: layout_box.style.border_color,
        });
    }

    // 3. Text lines
    for line in &layout_box.lines {
        commands.push(PaintCommand::DrawText {
            text: line.text.clone(),
            point: Point::new(line.rect.x, line.rect.y),
            font_size: layout_box.style.font_size,
            color: layout_box.style.color,
        });
    }

    // 4. Click regions for <a> elements
    if let Some(node_id) = layout_box.node_id {
        if let Some(node) = dom.get(node_id) {
            if let Some(tag) = node.tag_name() {
                if tag == "a" {
                    if let Some(href) = node.get_attribute("href") {
                        commands.push(PaintCommand::RegisterClickRegion {
                            rect: border_box,
                            target_url: href.to_string(),
                        });
                    }
                }
            }
        }
    }

    // 5. Recurse children
    for child in &layout_box.children {
        collect_paint_commands(child, dom, commands);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jbe_core::EngineLimits;
    use jbe_css::StyleSheet;
    use jbe_layout::build_and_layout_tree;

    #[test]
    fn test_paint_command_generation() {
        let limits = EngineLimits::default();
        let mut dom = DomTree::new(limits);
        let div = dom
            .create_element(
                "div",
                vec![("style".into(), "background-color: #ff0000; height: 40px;".into())],
            )
            .unwrap();
        let a = dom
            .create_element(
                "a",
                vec![
                    ("href".into(), "page2.html".into()),
                    ("style".into(), "color: #00ff00;".into()),
                ],
            )
            .unwrap();
        let txt = dom.create_text("Click here").unwrap();

        dom.append_child(dom.root(), div).unwrap();
        dom.append_child(div, a).unwrap();
        dom.append_child(a, txt).unwrap();

        let sheet = StyleSheet::new();
        let styles = jbe_css::resolve_styles(&dom, &sheet);
        let layout_tree = build_and_layout_tree(&dom, &styles, 400.0).unwrap();

        let commands = generate_paint_commands(&layout_tree, &dom);
        assert!(!commands.is_empty());

        let mut backend = TestBackend::new(400, 300);
        backend.begin_frame(400, 300);
        for cmd in &commands {
            backend.execute_command(cmd);
        }
        backend.end_frame();

        // Check that background was rendered in framebuffer
        let pixel = backend.get_pixel(10, 10).unwrap();
        assert_eq!(pixel.r, 255);
        assert_eq!(pixel.g, 0);
        assert_eq!(pixel.b, 0);

        // Check click region registered
        assert_eq!(backend.click_regions.len(), 1);
        assert_eq!(backend.click_regions[0].1, "page2.html");
        assert_eq!(
            backend.click_at(Point::new(10.0, 10.0)),
            Some("page2.html")
        );
    }
}
