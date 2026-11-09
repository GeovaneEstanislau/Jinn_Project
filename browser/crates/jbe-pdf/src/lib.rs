//! `jbe-pdf`: Pure native zero-dependency PDF parser and vector rasterizer for Jinn Browser Engine.
//!
//! Provides native PDF document analysis, cross-reference and object resolution,
//! content stream execution, and vector-to-pixel rasterization into 32-bit RGBA surfaces.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod document;
pub mod inflate;
pub mod interpreter;
pub mod lexer;
pub mod object;
pub mod rasterizer;

pub use document::{DocumentError, PdfDocument, PdfPage};
pub use interpreter::{DrawCommand, GraphicsState, Matrix2D, PdfInterpreter};
pub use lexer::{LexerError, PdfLexer, Token};
pub use object::PdfObject;
pub use rasterizer::{PdfRasterizer, PdfSurface};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pdf_flate_decompress() {
        // Raw deflate uncompressed block: BFINAL=1, BTYPE=00, LEN=5, NLEN=!5, data="Hello"
        let raw = [
            0x01, 0x05, 0x00, 0xFA, 0xFF, b'H', b'e', b'l', b'l', b'o',
        ];
        let decompressed = inflate::decompress_flate(&raw).expect("decompress uncompressed flate");
        assert_eq!(decompressed, b"Hello");
    }

    #[test]
    fn test_pdf_lexer_and_object_parsing() {
        let pdf_snippet = b"<< /Type /Catalog /Pages 2 0 R /Author (Jinn OS) /Count 42 >>";
        let mut lexer = PdfLexer::new(pdf_snippet);
        let obj = lexer.parse_object().expect("parse dict object");

        if let PdfObject::Dictionary(dict) = obj {
            assert_eq!(dict.get("Type").and_then(|o| o.as_name()), Some("Catalog"));
            assert_eq!(dict.get("Pages").and_then(|o| o.as_ref()), Some((2, 0)));
            assert_eq!(dict.get("Author").and_then(|o| o.as_str()), Some("Jinn OS"));
            assert_eq!(dict.get("Count").and_then(|o| o.as_i64()), Some(42));
        } else {
            panic!("Expected Dictionary object");
        }
    }

    #[test]
    fn test_pdf_document_parsing_and_rasterization() {
        // Minimal valid PDF 1.4 document with text and a rectangle
        let pdf_data = b"%PDF-1.4\n\
1 0 obj\n\
<< /Type /Catalog /Pages 2 0 R >>\n\
endobj\n\
2 0 obj\n\
<< /Type /Pages /Kids [3 0 R] /Count 1 >>\n\
endobj\n\
3 0 obj\n\
<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 300] /Contents 4 0 R >>\n\
endobj\n\
4 0 obj\n\
<< /Length 75 >>\n\
stream\n\
1 0 0 rg\n\
50 50 100 80 re\n\
f\n\
0 0 0 rg\n\
BT\n\
/F1 16 Tf\n\
60 60 Td\n\
(JINN OS) Tj\n\
ET\n\
endstream\n\
endobj\n\
xref\n\
0 5\n\
trailer\n\
<< /Size 5 /Root 1 0 R >>\n\
startxref\n\
350\n\
%%EOF";

        let doc = PdfDocument::parse(pdf_data).expect("parse valid pdf");
        assert_eq!(doc.page_count(), 1);

        let page = doc.get_page(0).expect("page 0 exists");
        assert_eq!(page.media_box.width, 400.0);
        assert_eq!(page.media_box.height, 300.0);
        assert_eq!(page.content_streams.len(), 1);

        // Rasterize page at 1.0x scale
        let surface = PdfRasterizer::render_page(page, 1.0);
        assert_eq!(surface.width, 400);
        assert_eq!(surface.height, 300);
        assert_eq!(surface.pixels.len(), 400 * 300);

        // Page background is white (0xFFFFFFFF)
        assert_eq!(surface.pixels[0], 0xFFFFFFFF);

        // Check that red rectangle was drawn (1 0 0 rg, 50 50 100 80 re f)
        // Red is 0xFFFF0000. In surface coords, y = 300 - (50 + 80) = 170.
        // Let's sample inside the rect: x = 80, y = 180
        let sample_idx = (180 * surface.stride + 80) as usize;
        assert_eq!(surface.pixels[sample_idx], 0xFFFF0000);
    }
}
