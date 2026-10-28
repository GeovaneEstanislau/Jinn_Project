//! # jbe-core — Jinn Browser Engine Core Definitions
//!
//! Provides fundamental types, geometry, identifiers, configurable limits,
//! and typed errors shared across all browser engine crates.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::string::String;
use core::fmt;

// ── Identifiers ──────────────────────────────────────────────────────────────

/// Unique identifier for an HTML document session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentId(pub u32);

/// Index-based identifier for a node in a DOM tree arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub usize);

impl NodeId {
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    pub const fn index(&self) -> usize {
        self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Node#{}", self.0)
    }
}

/// Unique identifier for an external resource (image, stylesheet, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourceId(pub u32);

impl ResourceId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }
}

// ── Geometry & Spatial Types ──────────────────────────────────────────────────

/// 2D Point with floating point coordinates (pixels).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// 2D Dimensions with floating point numbers (pixels).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Self = Self { width: 0.0, height: 0.0 };

    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// 2D Rectangle defined by top-left position and dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0, width: 0.0, height: 0.0 };

    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x < self.right() && p.y >= self.y && p.y < self.bottom()
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }

    pub fn expanded_by(&self, edges: &Edges) -> Rect {
        let x = self.x - edges.left;
        let y = self.y - edges.top;
        let w = (self.width + edges.left + edges.right).max(0.0);
        let h = (self.height + edges.top + edges.bottom).max(0.0);
        Rect::new(x, y, w, h)
    }
}

/// Four-sided box model edges (margins, paddings, borders).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Edges {
    pub const ZERO: Self = Self { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 };

    pub const fn all(val: f32) -> Self {
        Self { top: val, right: val, bottom: val, left: val }
    }

    pub const fn symmetric(vertical: f32, horizontal: f32) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }

    pub const fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self { top, right, bottom, left }
    }

    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

// ── Color System ─────────────────────────────────────────────────────────────

/// 32-bit RGBA color representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Self = Self { r: 0, g: 0, b: 0, a: 0 };
    pub const BLACK:       Self = Self { r: 0, g: 0, b: 0, a: 255 };
    pub const WHITE:       Self = Self { r: 255, g: 255, b: 255, a: 255 };
    pub const RED:         Self = Self { r: 255, g: 0, b: 0, a: 255 };
    pub const GREEN:       Self = Self { r: 0, g: 128, b: 0, a: 255 };
    pub const BLUE:        Self = Self { r: 0, g: 0, b: 255, a: 255 };
    pub const GRAY:        Self = Self { r: 128, g: 128, b: 128, a: 255 };
    pub const LIGHT_GRAY:  Self = Self { r: 211, g: 211, b: 211, a: 255 };
    pub const DARK_GRAY:   Self = Self { r: 64, g: 64, b: 64, a: 255 };
    pub const NAVY:        Self = Self { r: 0, g: 0, b: 128, a: 255 };
    pub const TEAL:        Self = Self { r: 0, g: 128, b: 128, a: 255 };

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Converts into ARGB32 format (0xAARRGGBB) used by Aladdin & Jinn framebuffers.
    pub fn to_argb(&self) -> u32 {
        ((self.a as u32) << 24) | ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }

    pub fn to_u32(&self) -> u32 {
        self.to_argb()
    }

    pub fn from_u32(val: u32) -> Self {
        Self {
            a: ((val >> 24) & 0xFF) as u8,
            r: ((val >> 16) & 0xFF) as u8,
            g: ((val >> 8) & 0xFF) as u8,
            b: (val & 0xFF) as u8,
        }
    }

    /// Parses `#RGB` or `#RRGGBB` hex strings into a Color.
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim();
        let bytes = hex.strip_prefix('#')?;
        match bytes.len() {
            3 => {
                let r = u8::from_str_radix(&bytes[0..1], 16).ok()?;
                let g = u8::from_str_radix(&bytes[1..2], 16).ok()?;
                let b = u8::from_str_radix(&bytes[2..3], 16).ok()?;
                Some(Self::rgb(r * 17, g * 17, b * 17))
            }
            6 => {
                let r = u8::from_str_radix(&bytes[0..2], 16).ok()?;
                let g = u8::from_str_radix(&bytes[2..4], 16).ok()?;
                let b = u8::from_str_radix(&bytes[4..6], 16).ok()?;
                Some(Self::rgb(r, g, b))
            }
            8 => {
                let r = u8::from_str_radix(&bytes[0..2], 16).ok()?;
                let g = u8::from_str_radix(&bytes[2..4], 16).ok()?;
                let b = u8::from_str_radix(&bytes[4..6], 16).ok()?;
                let a = u8::from_str_radix(&bytes[6..8], 16).ok()?;
                Some(Self::rgba(r, g, b, a))
            }
            _ => None,
        }
    }
}

// ── Configurable Security & Resource Limits ───────────────────────────────────

/// Engine resource limits protecting against memory exhaustion and adversarial inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineLimits {
    pub max_document_size: usize,
    pub max_dom_nodes: usize,
    pub max_dom_depth: usize,
    pub max_attributes_per_element: usize,
    pub max_attribute_length: usize,
    pub max_paint_commands: usize,
    pub max_image_dimension: u32,
    pub max_image_pixels: usize,
    pub max_decompressed_bytes: usize,
}

impl Default for EngineLimits {
    fn default() -> Self {
        Self {
            max_document_size: 512 * 1024,      // 512 KiB document limit
            max_dom_nodes: 1000,                 // 1000 DOM nodes max
            max_dom_depth: 32,                   // 32 levels maximum nesting
            max_attributes_per_element: 32,      // 32 attributes max
            max_attribute_length: 512,           // 512 bytes per attribute string
            max_paint_commands: 4000,            // 4000 paint commands per page
            max_image_dimension: 2048,           // 2048x2048 max width/height
            max_image_pixels: 1024 * 768,        // 786k pixels max
            max_decompressed_bytes: 8 * 1024 * 1024, // 8 MiB decompressed image limit
        }
    }
}

// ── Document Loading States ───────────────────────────────────────────────────

/// Lifecycle states of a document during navigation and rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadingState {
    Idle,
    Loading { url: String, bytes_loaded: usize },
    Loaded { bytes_total: usize },
    Failed { error: String },
}

// ── Engine Errors ─────────────────────────────────────────────────────────────

/// Strongly typed engine errors across parser, DOM, CSS, layout, and rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    ResourceTooLarge { size: usize, max_size: usize },
    DomLimitExceeded(String),
    TreeTooDeep { depth: usize, limit: usize },
    InvalidNodeId(usize),
    InvalidUrl(String),
    UnsupportedScheme(String),
    ResourceNotFound(String),
    AccessDenied(String),
    IoError(String),
    MalformedHtml(String),
    CssParseError(String),
    LayoutError(String),
    RenderError(String),
    ImageDecodeFailed(String),
    NetworkError(String),
    SecurityViolation(String),
    Internal(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ResourceTooLarge { size, max_size } => {
                write!(f, "Resource size ({} B) exceeds limit ({} B)", size, max_size)
            }
            Self::DomLimitExceeded(msg) => write!(f, "DOM limit exceeded: {}", msg),
            Self::TreeTooDeep { depth, limit } => {
                write!(f, "DOM tree depth ({}) exceeds limit ({})", depth, limit)
            }
            Self::InvalidNodeId(id) => write!(f, "Invalid node ID: {}", id),
            Self::InvalidUrl(msg) => write!(f, "Invalid URL: {}", msg),
            Self::UnsupportedScheme(msg) => write!(f, "Unsupported scheme: {}", msg),
            Self::ResourceNotFound(msg) => write!(f, "Resource not found: {}", msg),
            Self::AccessDenied(msg) => write!(f, "Access denied: {}", msg),
            Self::IoError(msg) => write!(f, "I/O error: {}", msg),
            Self::MalformedHtml(msg) => write!(f, "Malformed HTML: {}", msg),
            Self::CssParseError(msg) => write!(f, "CSS parse error: {}", msg),
            Self::LayoutError(msg) => write!(f, "Layout error: {}", msg),
            Self::RenderError(msg) => write!(f, "Render error: {}", msg),
            Self::ImageDecodeFailed(msg) => write!(f, "Image decode error: {}", msg),
            Self::NetworkError(msg) => write!(f, "Network error: {}", msg),
            Self::SecurityViolation(msg) => write!(f, "Security violation: {}", msg),
            Self::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

pub type Result<T> = core::result::Result<T, EngineError>;

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_containment_and_intersection() {
        let r1 = Rect::new(10.0, 20.0, 100.0, 50.0);
        assert!(r1.contains(Point::new(10.0, 20.0)));
        assert!(r1.contains(Point::new(109.0, 69.0)));
        assert!(!r1.contains(Point::new(110.0, 70.0)));
        assert!(!r1.contains(Point::new(9.0, 20.0)));

        let r2 = Rect::new(50.0, 40.0, 100.0, 100.0);
        assert!(r1.intersects(&r2));

        let r3 = Rect::new(200.0, 200.0, 50.0, 50.0);
        assert!(!r1.intersects(&r3));
    }

    #[test]
    fn test_color_hex_parsing() {
        assert_eq!(Color::from_hex("#ff0000"), Some(Color::rgb(255, 0, 0)));
        assert_eq!(Color::from_hex("#00ff00"), Some(Color::rgb(0, 255, 0)));
        assert_eq!(Color::from_hex("#0000ff"), Some(Color::rgb(0, 0, 255)));
        assert_eq!(Color::from_hex("#fff"), Some(Color::rgb(255, 255, 255)));
        assert_eq!(Color::from_hex("#000"), Some(Color::rgb(0, 0, 0)));
        assert_eq!(Color::from_hex("#12345678"), Some(Color::rgba(0x12, 0x34, 0x56, 0x78)));
        assert_eq!(Color::from_hex("invalid"), None);
    }

    #[test]
    fn test_color_to_and_from_u32() {
        let c = Color::rgb(0xAA, 0xBB, 0xCC);
        let val = c.to_argb();
        assert_eq!(val, 0xFFAABBCC);
        assert_eq!(Color::from_u32(val), c);
    }

    #[test]
    fn test_engine_limits_default() {
        let limits = EngineLimits::default();
        assert_eq!(limits.max_document_size, 512 * 1024);
        assert_eq!(limits.max_dom_depth, 32);
        assert_eq!(limits.max_dom_nodes, 1000);
    }
}
