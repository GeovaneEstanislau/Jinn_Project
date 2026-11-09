//! PDF Content Stream interpreter and vector command accumulator.

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;
use crate::lexer::PdfLexer;
use crate::object::PdfObject;
use jbe_core::Color;

/// 2D Affine Transformation Matrix [a, b, c, d, e, f].
/// [ x' ] = [ a c e ] [ x ]
/// [ y' ] = [ b d f ] [ y ]
/// [ 1  ] = [ 0 0 1 ] [ 1 ]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix2D {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Matrix2D {
    pub fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    pub fn transform_point(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    pub fn multiply(&self, rhs: &Matrix2D) -> Matrix2D {
        Matrix2D {
            a: self.a * rhs.a + self.b * rhs.c,
            b: self.a * rhs.b + self.b * rhs.d,
            c: self.c * rhs.a + self.d * rhs.c,
            d: self.c * rhs.b + self.d * rhs.d,
            e: self.e * rhs.a + self.f * rhs.c + rhs.e,
            f: self.e * rhs.b + self.f * rhs.d + rhs.f,
        }
    }
}

/// Graphics state maintained during stream execution.
#[derive(Debug, Clone)]
pub struct GraphicsState {
    pub ctm: Matrix2D,
    pub fill_color: Color,
    pub stroke_color: Color,
    pub line_width: f32,
    pub font_size: f32,
    pub text_matrix: Matrix2D,
    pub text_line_matrix: Matrix2D,
}

impl Default for GraphicsState {
    fn default() -> Self {
        Self {
            ctm: Matrix2D::identity(),
            fill_color: Color::rgba(0, 0, 0, 255),   // Default black
            stroke_color: Color::rgba(0, 0, 0, 255), // Default black
            line_width: 1.0,
            font_size: 12.0,
            text_matrix: Matrix2D::identity(),
            text_line_matrix: Matrix2D::identity(),
        }
    }
}

/// High-level vector drawing commands extracted from PDF content stream.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    FillRect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Color,
    },
    StrokeRect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Color,
        line_width: f32,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        color: Color,
        line_width: f32,
    },
    Text {
        x: f32,
        y: f32,
        text: String,
        font_size: f32,
        color: Color,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum SubpathOp {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    Rect(f32, f32, f32, f32),
    Close,
}

pub struct PdfInterpreter {
    pub state: GraphicsState,
    pub state_stack: Vec<GraphicsState>,
    pub commands: Vec<DrawCommand>,
    current_path: Vec<SubpathOp>,
}

impl PdfInterpreter {
    pub fn new() -> Self {
        Self {
            state: GraphicsState::default(),
            state_stack: Vec::new(),
            commands: Vec::new(),
            current_path: Vec::new(),
        }
    }

    pub fn interpret_stream(&mut self, stream_bytes: &[u8]) {
        let mut lexer = PdfLexer::new(stream_bytes);
        let mut operand_stack = Vec::new();

        while lexer.pos < stream_bytes.len() {
            lexer.skip_whitespace_and_comments();
            if lexer.pos >= stream_bytes.len() {
                break;
            }

            match lexer.parse_object() {
                Ok(obj) => {
                    if let PdfObject::Name(op) = &obj {
                        self.execute_operator(op.as_str(), &mut operand_stack);
                    } else {
                        operand_stack.push(obj);
                    }
                }
                Err(_) => {
                    // Skip invalid character
                    lexer.pos += 1;
                }
            }
        }
    }

    fn pop_f32(stack: &mut Vec<PdfObject>) -> Option<f32> {
        stack.pop().and_then(|o| o.as_f64()).map(|f| f as f32)
    }

    fn execute_operator(&mut self, op: &str, stack: &mut Vec<PdfObject>) {
        match op {
            // Graphics state save / restore
            "q" => {
                self.state_stack.push(self.state.clone());
            }
            "Q" => {
                if let Some(prev) = self.state_stack.pop() {
                    self.state = prev;
                }
            }
            // Current transformation matrix: a b c d e f cm
            "cm" => {
                if stack.len() >= 6 {
                    let f = Self::pop_f32(stack).unwrap_or(0.0);
                    let e = Self::pop_f32(stack).unwrap_or(0.0);
                    let d = Self::pop_f32(stack).unwrap_or(1.0);
                    let c = Self::pop_f32(stack).unwrap_or(0.0);
                    let b = Self::pop_f32(stack).unwrap_or(0.0);
                    let a = Self::pop_f32(stack).unwrap_or(1.0);
                    let new_m = Matrix2D { a, b, c, d, e, f };
                    self.state.ctm = self.state.ctm.multiply(&new_m);
                }
            }
            // Line width: w
            "w" => {
                if let Some(w) = Self::pop_f32(stack) {
                    self.state.line_width = w.max(0.1);
                }
            }
            // Color operators
            "g" => {
                // Grayscale fill
                if let Some(gray) = Self::pop_f32(stack) {
                    let byte = (gray.clamp(0.0, 1.0) * 255.0) as u8;
                    self.state.fill_color = Color::rgba(byte, byte, byte, 255);
                }
            }
            "G" => {
                // Grayscale stroke
                if let Some(gray) = Self::pop_f32(stack) {
                    let byte = (gray.clamp(0.0, 1.0) * 255.0) as u8;
                    self.state.stroke_color = Color::rgba(byte, byte, byte, 255);
                }
            }
            "rg" => {
                // RGB fill
                if stack.len() >= 3 {
                    let b = Self::pop_f32(stack).unwrap_or(0.0);
                    let g = Self::pop_f32(stack).unwrap_or(0.0);
                    let r = Self::pop_f32(stack).unwrap_or(0.0);
                    self.state.fill_color = Color::rgba(
                        (r.clamp(0.0, 1.0) * 255.0) as u8,
                        (g.clamp(0.0, 1.0) * 255.0) as u8,
                        (b.clamp(0.0, 1.0) * 255.0) as u8,
                        255,
                    );
                }
            }
            "RG" => {
                // RGB stroke
                if stack.len() >= 3 {
                    let b = Self::pop_f32(stack).unwrap_or(0.0);
                    let g = Self::pop_f32(stack).unwrap_or(0.0);
                    let r = Self::pop_f32(stack).unwrap_or(0.0);
                    self.state.stroke_color = Color::rgba(
                        (r.clamp(0.0, 1.0) * 255.0) as u8,
                        (g.clamp(0.0, 1.0) * 255.0) as u8,
                        (b.clamp(0.0, 1.0) * 255.0) as u8,
                        255,
                    );
                }
            }
            // Path construction
            "m" => {
                if stack.len() >= 2 {
                    let y = Self::pop_f32(stack).unwrap_or(0.0);
                    let x = Self::pop_f32(stack).unwrap_or(0.0);
                    let (tx, ty) = self.state.ctm.transform_point(x, y);
                    self.current_path.push(SubpathOp::MoveTo(tx, ty));
                }
            }
            "l" => {
                if stack.len() >= 2 {
                    let y = Self::pop_f32(stack).unwrap_or(0.0);
                    let x = Self::pop_f32(stack).unwrap_or(0.0);
                    let (tx, ty) = self.state.ctm.transform_point(x, y);
                    self.current_path.push(SubpathOp::LineTo(tx, ty));
                }
            }
            "re" => {
                if stack.len() >= 4 {
                    let h = Self::pop_f32(stack).unwrap_or(0.0);
                    let w = Self::pop_f32(stack).unwrap_or(0.0);
                    let y = Self::pop_f32(stack).unwrap_or(0.0);
                    let x = Self::pop_f32(stack).unwrap_or(0.0);
                    let (tx, ty) = self.state.ctm.transform_point(x, y);
                    self.current_path.push(SubpathOp::Rect(tx, ty, w, h));
                }
            }
            "h" => {
                self.current_path.push(SubpathOp::Close);
            }
            // Path painting
            "f" | "F" | "f*" => {
                self.paint_path(true, false);
            }
            "S" | "s" => {
                if op == "s" {
                    self.current_path.push(SubpathOp::Close);
                }
                self.paint_path(false, true);
            }
            "B" | "B*" | "b" | "b*" => {
                if op == "b" || op == "b*" {
                    self.current_path.push(SubpathOp::Close);
                }
                self.paint_path(true, true);
            }
            "n" => {
                self.current_path.clear();
            }
            // Text object operators
            "BT" => {
                self.state.text_matrix = Matrix2D::identity();
                self.state.text_line_matrix = Matrix2D::identity();
            }
            "ET" => {}
            "Tf" => {
                if stack.len() >= 2 {
                    let size = Self::pop_f32(stack).unwrap_or(12.0);
                    let _font_name = stack.pop();
                    self.state.font_size = size;
                }
            }
            "Td" | "TD" => {
                if stack.len() >= 2 {
                    let ty = Self::pop_f32(stack).unwrap_or(0.0);
                    let tx = Self::pop_f32(stack).unwrap_or(0.0);
                    let trans = Matrix2D {
                        a: 1.0,
                        b: 0.0,
                        c: 0.0,
                        d: 1.0,
                        e: tx,
                        f: ty,
                    };
                    self.state.text_line_matrix = trans.multiply(&self.state.text_line_matrix);
                    self.state.text_matrix = self.state.text_line_matrix;
                }
            }
            "T*" => {
                let trans = Matrix2D {
                    a: 1.0,
                    b: 0.0,
                    c: 0.0,
                    d: 1.0,
                    e: 0.0,
                    f: -self.state.font_size * 1.2,
                };
                self.state.text_line_matrix = trans.multiply(&self.state.text_line_matrix);
                self.state.text_matrix = self.state.text_line_matrix;
            }
            "Tj" | "'" => {
                if let Some(obj) = stack.pop() {
                    let text = obj.as_str().unwrap_or("").to_string();
                    let (x, y) = self.state.ctm.transform_point(
                        self.state.text_matrix.e,
                        self.state.text_matrix.f,
                    );
                    self.commands.push(DrawCommand::Text {
                        x,
                        y,
                        text: text.clone(),
                        font_size: self.state.font_size,
                        color: self.state.fill_color,
                    });

                    // Advance text position horizontally
                    let approx_width = text.len() as f32 * self.state.font_size * 0.6;
                    self.state.text_matrix.e += approx_width;
                }
            }
            "TJ" => {
                if let Some(PdfObject::Array(items)) = stack.pop() {
                    let mut text_buf = String::new();
                    for item in items {
                        if let Some(s) = item.as_str() {
                            text_buf.push_str(s);
                        } else if let Some(kern) = item.as_f64() {
                            // Negative kern advances pos
                            let kern_shift = (-kern as f32 / 1000.0) * self.state.font_size;
                            self.state.text_matrix.e += kern_shift;
                        }
                    }
                    let (x, y) = self.state.ctm.transform_point(
                        self.state.text_matrix.e,
                        self.state.text_matrix.f,
                    );
                    self.commands.push(DrawCommand::Text {
                        x,
                        y,
                        text: text_buf.clone(),
                        font_size: self.state.font_size,
                        color: self.state.fill_color,
                    });
                    let approx_width = text_buf.len() as f32 * self.state.font_size * 0.6;
                    self.state.text_matrix.e += approx_width;
                }
            }
            _ => {
                // Unhandled operator: clear stack to prevent leaks
                stack.clear();
            }
        }
    }

    fn paint_path(&mut self, fill: bool, stroke: bool) {
        let mut cur_pt = (0.0f32, 0.0f32);
        let mut start_pt = (0.0f32, 0.0f32);

        for op in self.current_path.drain(..) {
            match op {
                SubpathOp::MoveTo(x, y) => {
                    cur_pt = (x, y);
                    start_pt = (x, y);
                }
                SubpathOp::LineTo(x, y) => {
                    if stroke {
                        self.commands.push(DrawCommand::Line {
                            x1: cur_pt.0,
                            y1: cur_pt.1,
                            x2: x,
                            y2: y,
                            color: self.state.stroke_color,
                            line_width: self.state.line_width,
                        });
                    }
                    cur_pt = (x, y);
                }
                SubpathOp::Rect(x, y, w, h) => {
                    if fill {
                        self.commands.push(DrawCommand::FillRect {
                            x,
                            y,
                            w,
                            h,
                            color: self.state.fill_color,
                        });
                    }
                    if stroke {
                        self.commands.push(DrawCommand::StrokeRect {
                            x,
                            y,
                            w,
                            h,
                            color: self.state.stroke_color,
                            line_width: self.state.line_width,
                        });
                    }
                }
                SubpathOp::Close => {
                    if stroke && cur_pt != start_pt {
                        self.commands.push(DrawCommand::Line {
                            x1: cur_pt.0,
                            y1: cur_pt.1,
                            x2: start_pt.0,
                            y2: start_pt.1,
                            color: self.state.stroke_color,
                            line_width: self.state.line_width,
                        });
                    }
                    cur_pt = start_pt;
                }
            }
        }
    }
}
