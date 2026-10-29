//! Jinn Browser Engine - Reference Implementation and Session Manager

use jbe_core::{EngineError, EngineLimits, Point};
use jbe_css::StyleSheet;
use jbe_dom::DomTree;
use jbe_html::parse_html;
use jbe_layout::{build_and_layout_tree, LayoutBox};
use jbe_net::ResourceLoader;
use jbe_render::{generate_paint_commands_with_scroll, RenderBackend, TestBackend};
use jbe_url::Url;

/// Browser navigation session state.
pub struct BrowserSession {
    pub history: Vec<Url>,
    pub history_index: usize,
    pub limits: EngineLimits,
    pub loader: Box<dyn ResourceLoader>,
    pub backend: TestBackend,
    pub current_dom: Option<DomTree>,
    pub current_layout: Option<LayoutBox>,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub scroll_y: f32,
}

impl BrowserSession {
    pub fn new(viewport_width: u32, viewport_height: u32, loader: Box<dyn ResourceLoader>) -> Self {
        Self {
            history: Vec::new(),
            history_index: 0,
            limits: EngineLimits::default(),
            loader,
            backend: TestBackend::new(viewport_width, viewport_height),
            current_dom: None,
            current_layout: None,
            viewport_width,
            viewport_height,
            scroll_y: 0.0,
        }
    }

    /// Navigates to a new URL, adding it to the history.
    pub fn navigate(&mut self, url_str: &str) -> Result<(), EngineError> {
        let url = if let Some(current_url) = self.current_url() {
            current_url.resolve_relative(url_str)?
        } else {
            Url::parse(url_str)?
        };

        self.load_url(url.clone())?;

        // Truncate forward history if navigating from middle
        if self.history_index + 1 < self.history.len() {
            self.history.truncate(self.history_index + 1);
        }
        self.history.push(url);
        self.history_index = self.history.len() - 1;

        Ok(())
    }

    /// Internal document load and render pipeline.
    pub fn load_url(&mut self, url: Url) -> Result<(), EngineError> {
        let resource = self.loader.load(&url)?;

        let html_content = String::from_utf8(resource.bytes).map_err(|_| {
            EngineError::ResourceNotFound("Resource content is not valid UTF-8".into())
        })?;

        // 1. HTML -> DOM
        let dom = parse_html(&html_content, &self.limits)?;

        // 2. Process and validate <img> elements
        for img_id in dom.find_all_by_tag("img") {
            if let Some(node) = dom.get(img_id) {
                if let Some(src) = node.get_attribute("src") {
                    if let Ok(img_url) = url.resolve_relative(src) {
                        if let Ok(img_resource) = self.loader.load(&img_url) {
                            let _ = jbe_image::decode_image(&img_resource.bytes, &self.limits);
                        }
                    }
                }
            }
        }

        // 3. CSS Stylesheets: default UA + extract <style> tags
        let mut stylesheet = StyleSheet::new();
        for style_node in dom.find_all_by_tag("style") {
            let css_text = dom.text_content(style_node);
            let parsed_sheet = StyleSheet::parse(&css_text);
            stylesheet.rules.extend(parsed_sheet.rules);
        }

        // 4. Computed Styles
        let computed_styles = jbe_css::resolve_styles(&dom, &stylesheet);

        // 5. Layout
        let layout_tree = build_and_layout_tree(&dom, &computed_styles, self.viewport_width as f32)
            .ok_or_else(|| EngineError::Internal("Failed to build layout tree".into()))?;

        self.scroll_y = 0.0;
        self.current_dom = Some(dom);
        self.current_layout = Some(layout_tree);

        // 6. Paint Commands & Backend Execution
        self.repaint();

        Ok(())
    }

    /// Returns total scrollable content height of current document.
    pub fn content_height(&self) -> f32 {
        self.current_layout
            .as_ref()
            .map(|l| l.content_height())
            .unwrap_or(0.0)
    }

    /// Returns max possible scroll_y given viewport height.
    pub fn max_scroll_y(&self) -> f32 {
        (self.content_height() - self.viewport_height as f32).max(0.0)
    }

    /// Scrolls vertically by delta_y pixels, clamping between 0 and max_scroll_y.
    pub fn scroll(&mut self, delta_y: f32) -> bool {
        let old = self.scroll_y;
        let max = self.max_scroll_y();
        self.scroll_y = (self.scroll_y + delta_y).clamp(0.0, max);
        if (self.scroll_y - old).abs() > 0.001 {
            self.repaint();
            true
        } else {
            false
        }
    }

    /// Repaints the current document using the active scroll_y offset.
    pub fn repaint(&mut self) {
        if let (Some(layout), Some(dom)) = (&self.current_layout, &self.current_dom) {
            let paint_commands =
                generate_paint_commands_with_scroll(layout, dom, 0.0, self.scroll_y);
            self.backend
                .begin_frame(self.viewport_width, self.viewport_height);
            for cmd in &paint_commands {
                self.backend.execute_command(cmd);
            }
            self.backend.end_frame();
        }
    }

    /// Returns current URL if any.
    pub fn current_url(&self) -> Option<&Url> {
        self.history.get(self.history_index)
    }

    /// Navigates back in history.
    pub fn back(&mut self) -> Result<bool, EngineError> {
        if self.history_index > 0 {
            self.history_index -= 1;
            let url = self.history[self.history_index].clone();
            self.load_url(url)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Navigates forward in history.
    pub fn forward(&mut self) -> Result<bool, EngineError> {
        if self.history_index + 1 < self.history.len() {
            self.history_index += 1;
            let url = self.history[self.history_index].clone();
            self.load_url(url)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Reloads current document.
    pub fn reload(&mut self) -> Result<(), EngineError> {
        if let Some(url) = self.current_url().cloned() {
            self.load_url(url)?;
        }
        Ok(())
    }

    /// Simulates clicking at (x, y). If an <a> region was hit, navigates to target.
    pub fn click_at(&mut self, point: Point) -> Result<Option<String>, EngineError> {
        if let Some(target) = self.backend.click_at(point).map(String::from) {
            self.navigate(&target)?;
            Ok(Some(target))
        } else {
            Ok(None)
        }
    }
}
