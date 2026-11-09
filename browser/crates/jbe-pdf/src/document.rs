//! PDF Document parser, object resolver, and page tree extractor.

extern crate alloc;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use crate::inflate::decompress_flate;
use crate::lexer::PdfLexer;
use crate::object::PdfObject;
use jbe_core::Rect;

#[derive(Debug, Clone, PartialEq)]
pub enum DocumentError {
    InvalidHeader,
    CorruptedStructure,
    CatalogNotFound,
    PageNotFound,
    StreamDecompressionFailed,
}

/// Represents a single parsed PDF page ready for rendering.
#[derive(Debug, Clone)]
pub struct PdfPage {
    pub page_number: usize,
    pub media_box: Rect,
    pub content_streams: Vec<Vec<u8>>,
}

/// In-memory parsed PDF document.
pub struct PdfDocument {
    pub version: String,
    pub objects: BTreeMap<(u32, u16), PdfObject>,
    pub pages: Vec<PdfPage>,
}

impl PdfDocument {
    /// Parses a complete PDF document from raw file bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, DocumentError> {
        if bytes.len() < 8 || !bytes.starts_with(b"%PDF-") {
            return Err(DocumentError::InvalidHeader);
        }

        // Extract version string (e.g., "1.4")
        let ver_end = bytes[5..].iter().position(|&b| b == b'\r' || b == b'\n').unwrap_or(3);
        let version = String::from_utf8_lossy(&bytes[5..5 + ver_end]).into_owned();

        // Scan and extract all indirect objects
        let mut objects = BTreeMap::new();
        let mut lexer = PdfLexer::new(bytes);

        while lexer.pos < bytes.len() {
            lexer.skip_whitespace_and_comments();
            if lexer.pos >= bytes.len() {
                break;
            }

            // Look for pattern: <int> <int> obj
            let saved = lexer.pos;
            if let Ok(crate::lexer::Token::Number(num, true)) = lexer.next_token() {
                if let Ok(crate::lexer::Token::Number(gen, true)) = lexer.next_token() {
                    if let Ok(crate::lexer::Token::Keyword(kw)) = lexer.next_token() {
                        if kw == "obj" {
                            if let Ok(obj) = lexer.parse_object() {
                                objects.insert((num as u32, gen as u16), obj);
                                // Skip until "endobj"
                                while lexer.pos + 6 <= bytes.len() {
                                    if &bytes[lexer.pos..lexer.pos + 6] == b"endobj" {
                                        lexer.pos += 6;
                                        break;
                                    }
                                    lexer.pos += 1;
                                }
                                continue;
                            }
                        }
                    }
                }
            }

            // Move forward if no object pattern matched
            lexer.pos = saved + 1;
        }

        // Find Catalog object: search objects for Dictionary with /Type /Catalog
        let mut root_catalog: Option<&BTreeMap<String, PdfObject>> = None;
        for obj in objects.values() {
            if let Some(dict) = obj.as_dict() {
                if let Some(type_val) = dict.get("Type") {
                    if type_val.as_name() == Some("Catalog") {
                        root_catalog = Some(dict);
                        break;
                    }
                }
            }
        }

        let mut pages = Vec::new();

        if let Some(catalog) = root_catalog {
            if let Some(pages_ref) = catalog.get("Pages") {
                let pages_obj = Self::resolve_ref(pages_ref, &objects);
                if let Some(pages_dict) = pages_obj.and_then(|o| o.as_dict()) {
                    Self::collect_pages(pages_dict, &objects, &mut pages)?;
                }
            }
        }

        // If no catalog /Pages hierarchy was found, fall back to searching all Page objects directly
        if pages.is_empty() {
            for obj in objects.values() {
                if let Some(dict) = obj.as_dict() {
                    if let Some(type_val) = dict.get("Type") {
                        if type_val.as_name() == Some("Page") {
                            let page = Self::parse_single_page(dict, &objects, pages.len() + 1)?;
                            pages.push(page);
                        }
                    }
                }
            }
        }

        Ok(Self {
            version,
            objects,
            pages,
        })
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn get_page(&self, page_index: usize) -> Option<&PdfPage> {
        self.pages.get(page_index)
    }

    /// Resolves an indirect reference `(u32, u16)` in the object map.
    pub fn resolve_ref<'a>(
        obj: &'a PdfObject,
        objects: &'a BTreeMap<(u32, u16), PdfObject>,
    ) -> Option<&'a PdfObject> {
        match obj {
            PdfObject::Reference(num, gen) => objects.get(&(*num, *gen)),
            _ => Some(obj),
        }
    }

    fn collect_pages(
        pages_node: &BTreeMap<String, PdfObject>,
        objects: &BTreeMap<(u32, u16), PdfObject>,
        out_pages: &mut Vec<PdfPage>,
    ) -> Result<(), DocumentError> {
        if let Some(kids_obj) = pages_node.get("Kids") {
            let kids_resolved = Self::resolve_ref(kids_obj, objects);
            if let Some(kids_array) = kids_resolved.and_then(|o| o.as_array()) {
                for kid_ref in kids_array {
                    if let Some(kid) = Self::resolve_ref(kid_ref, objects) {
                        if let Some(dict) = kid.as_dict() {
                            let kid_type = dict.get("Type").and_then(|t| t.as_name());
                            if kid_type == Some("Pages") {
                                Self::collect_pages(dict, objects, out_pages)?;
                            } else if kid_type == Some("Page") {
                                let page = Self::parse_single_page(dict, objects, out_pages.len() + 1)?;
                                out_pages.push(page);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn parse_single_page(
        page_dict: &BTreeMap<String, PdfObject>,
        objects: &BTreeMap<(u32, u16), PdfObject>,
        page_number: usize,
    ) -> Result<PdfPage, DocumentError> {
        // MediaBox: [x0, y0, x1, y1] (default US Letter: 612 x 792)
        let mut media_box = Rect::new(0.0, 0.0, 612.0, 792.0);
        if let Some(mbox_obj) = page_dict.get("MediaBox") {
            if let Some(mbox) = Self::resolve_ref(mbox_obj, objects).and_then(|o| o.as_array()) {
                if mbox.len() >= 4 {
                    let x0 = mbox[0].as_f64().unwrap_or(0.0) as f32;
                    let y0 = mbox[1].as_f64().unwrap_or(0.0) as f32;
                    let x1 = mbox[2].as_f64().unwrap_or(612.0) as f32;
                    let y1 = mbox[3].as_f64().unwrap_or(792.0) as f32;
                    media_box = Rect::new(x0, y0, (x1 - x0).abs(), (y1 - y0).abs());
                }
            }
        }

        // Contents stream
        let mut content_streams = Vec::new();
        if let Some(contents_obj) = page_dict.get("Contents") {
            let resolved_contents = Self::resolve_ref(contents_obj, objects);
            match resolved_contents {
                Some(PdfObject::Stream(stream_dict, raw_data)) => {
                    let decompressed = Self::extract_stream_data(stream_dict, raw_data)?;
                    content_streams.push(decompressed);
                }
                Some(PdfObject::Array(arr)) => {
                    for stream_ref in arr {
                        if let Some(PdfObject::Stream(stream_dict, raw_data)) =
                            Self::resolve_ref(stream_ref, objects)
                        {
                            let decompressed = Self::extract_stream_data(stream_dict, raw_data)?;
                            content_streams.push(decompressed);
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(PdfPage {
            page_number,
            media_box,
            content_streams,
        })
    }

    fn extract_stream_data(
        dict: &BTreeMap<String, PdfObject>,
        raw_data: &[u8],
    ) -> Result<Vec<u8>, DocumentError> {
        if let Some(filter) = dict.get("Filter").and_then(|f| f.as_name()) {
            if filter == "FlateDecode" {
                return decompress_flate(raw_data)
                    .map_err(|_| DocumentError::StreamDecompressionFailed);
            }
        }
        // Uncompressed or unsupported filter passed as raw
        Ok(raw_data.to_vec())
    }
}
