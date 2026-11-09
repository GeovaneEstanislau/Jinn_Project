//! Fast tokenizer and parser for PDF lexical tokens and objects.

extern crate alloc;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use crate::object::PdfObject;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Keyword(String),
    Name(String),
    Number(f64, bool), // val, is_integer
    String(Vec<u8>),
    OpenBracket,  // [
    CloseBracket, // ]
    OpenDict,     // <<
    CloseDict,    // >>
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LexerError {
    UnexpectedEof,
    InvalidToken,
    InvalidHex,
    InvalidObject,
}

pub struct PdfLexer<'a> {
    pub bytes: &'a [u8],
    pub pos: usize,
}

impl<'a> PdfLexer<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    pub fn skip_whitespace_and_comments(&mut self) {
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' || b == 0 || b == 0x0C {
                self.pos += 1;
            } else if b == b'%' {
                // Comment until end of line
                self.pos += 1;
                while self.pos < self.bytes.len() && self.bytes[self.pos] != b'\r' && self.bytes[self.pos] != b'\n' {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    pub fn next_token(&mut self) -> Result<Token, LexerError> {
        self.skip_whitespace_and_comments();
        if self.pos >= self.bytes.len() {
            return Ok(Token::Eof);
        }

        let b = self.bytes[self.pos];

        if b == b'[' {
            self.pos += 1;
            return Ok(Token::OpenBracket);
        }
        if b == b']' {
            self.pos += 1;
            return Ok(Token::CloseBracket);
        }
        if b == b'<' {
            if self.pos + 1 < self.bytes.len() && self.bytes[self.pos + 1] == b'<' {
                self.pos += 2;
                return Ok(Token::OpenDict);
            } else {
                // Hex string: <48656c6c6f>
                self.pos += 1;
                let start = self.pos;
                while self.pos < self.bytes.len() && self.bytes[self.pos] != b'>' {
                    self.pos += 1;
                }
                if self.pos >= self.bytes.len() {
                    return Err(LexerError::UnexpectedEof);
                }
                let hex_slice = &self.bytes[start..self.pos];
                self.pos += 1; // skip '>'

                let hex_str: String = hex_slice.iter().filter(|b| !b.is_ascii_whitespace()).map(|&b| b as char).collect();
                let mut data = Vec::new();
                let mut chars = hex_str.chars();
                while let Some(c1) = chars.next() {
                    let c2 = chars.next().unwrap_or('0');
                    let byte = u8::from_str_radix(&alloc::format!("{}{}", c1, c2), 16).map_err(|_| LexerError::InvalidHex)?;
                    data.push(byte);
                }
                return Ok(Token::String(data));
            }
        }
        if b == b'>' {
            if self.pos + 1 < self.bytes.len() && self.bytes[self.pos + 1] == b'>' {
                self.pos += 2;
                return Ok(Token::CloseDict);
            }
            return Err(LexerError::InvalidToken);
        }
        if b == b'(' {
            // Literal string
            self.pos += 1;
            let mut depth = 1;
            let mut result = Vec::new();

            while self.pos < self.bytes.len() {
                let cur = self.bytes[self.pos];
                self.pos += 1;
                if cur == b'\\' {
                    if self.pos >= self.bytes.len() {
                        return Err(LexerError::UnexpectedEof);
                    }
                    let esc = self.bytes[self.pos];
                    self.pos += 1;
                    match esc {
                        b'n' => result.push(b'\n'),
                        b'r' => result.push(b'\r'),
                        b't' => result.push(b'\t'),
                        b'b' => result.push(0x08),
                        b'f' => result.push(0x0C),
                        b'(' => result.push(b'('),
                        b')' => result.push(b')'),
                        b'\\' => result.push(b'\\'),
                        _ => result.push(esc),
                    }
                } else if cur == b'(' {
                    depth += 1;
                    result.push(cur);
                } else if cur == b')' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    result.push(cur);
                } else {
                    result.push(cur);
                }
            }

            return Ok(Token::String(result));
        }
        if b == b'/' {
            // Name: /Catalog
            self.pos += 1;
            let start = self.pos;
            while self.pos < self.bytes.len() {
                let c = self.bytes[self.pos];
                if c.is_ascii_whitespace() || c == b'/' || c == b'[' || c == b']' || c == b'<' || c == b'>' || c == b'(' || c == b')' || c == b'%' {
                    break;
                }
                self.pos += 1;
            }
            let name = String::from_utf8_lossy(&self.bytes[start..self.pos]).into_owned();
            return Ok(Token::Name(name));
        }

        // Numbers or keywords
        let start = self.pos;
        while self.pos < self.bytes.len() {
            let c = self.bytes[self.pos];
            if c.is_ascii_whitespace() || c == b'/' || c == b'[' || c == b']' || c == b'<' || c == b'>' || c == b'(' || c == b')' || c == b'%' {
                break;
            }
            self.pos += 1;
        }

        let slice = &self.bytes[start..self.pos];
        let word = String::from_utf8_lossy(slice);

        // Try parse as integer
        if let Ok(i) = word.parse::<i64>() {
            return Ok(Token::Number(i as f64, true));
        }
        // Try parse as float
        if let Ok(f) = word.parse::<f64>() {
            return Ok(Token::Number(f, false));
        }

        Ok(Token::Keyword(word.into_owned()))
    }

    /// Parses the next PDF object from stream.
    pub fn parse_object(&mut self) -> Result<PdfObject, LexerError> {
        let token = self.next_token()?;
        match token {
            Token::Eof => Ok(PdfObject::Null),
            Token::OpenBracket => {
                let mut list = Vec::new();
                loop {
                    self.skip_whitespace_and_comments();
                    if self.pos < self.bytes.len() && self.bytes[self.pos] == b']' {
                        self.pos += 1;
                        break;
                    }
                    let item = self.parse_object()?;
                    list.push(item);
                }
                Ok(PdfObject::Array(list))
            }
            Token::OpenDict => {
                let mut dict = BTreeMap::new();
                loop {
                    self.skip_whitespace_and_comments();
                    if self.pos + 1 < self.bytes.len() && &self.bytes[self.pos..self.pos + 2] == b">>" {
                        self.pos += 2;
                        break;
                    }
                    let key_tok = self.next_token()?;
                    let key = match key_tok {
                        Token::Name(n) => n,
                        _ => return Err(LexerError::InvalidObject),
                    };
                    let val = self.parse_object()?;
                    dict.insert(key, val);
                }

                // Check if followed by "stream"
                let saved_pos = self.pos;
                self.skip_whitespace_and_comments();
                if self.pos + 6 <= self.bytes.len() && &self.bytes[self.pos..self.pos + 6] == b"stream" {
                    self.pos += 6;
                    // Skip newline following stream keyword
                    if self.pos < self.bytes.len() && self.bytes[self.pos] == b'\r' {
                        self.pos += 1;
                    }
                    if self.pos < self.bytes.len() && self.bytes[self.pos] == b'\n' {
                        self.pos += 1;
                    }

                    let stream_start = self.pos;
                    // Find "endstream"
                    let mut stream_end = stream_start;
                    while self.pos + 9 <= self.bytes.len() {
                        if &self.bytes[self.pos..self.pos + 9] == b"endstream" {
                            stream_end = self.pos;
                            // Trim trailing \r\n before endstream
                            if stream_end > stream_start && self.bytes[stream_end - 1] == b'\n' {
                                stream_end -= 1;
                            }
                            if stream_end > stream_start && self.bytes[stream_end - 1] == b'\r' {
                                stream_end -= 1;
                            }
                            self.pos += 9;
                            break;
                        }
                        self.pos += 1;
                    }

                    let stream_data = self.bytes[stream_start..stream_end].to_vec();
                    return Ok(PdfObject::Stream(dict, stream_data));
                } else {
                    self.pos = saved_pos;
                }

                Ok(PdfObject::Dictionary(dict))
            }
            Token::Name(n) => Ok(PdfObject::Name(n)),
            Token::String(s) => Ok(PdfObject::String(s)),
            Token::Number(n, is_int) => {
                // Look ahead for reference: "12 0 R"
                if is_int {
                    let saved_pos = self.pos;
                    if let Ok(Token::Number(gen, true)) = self.next_token() {
                        if let Ok(Token::Keyword(kw)) = self.next_token() {
                            if kw == "R" {
                                return Ok(PdfObject::Reference(n as u32, gen as u16));
                            }
                        }
                    }
                    self.pos = saved_pos;
                    Ok(PdfObject::Integer(n as i64))
                } else {
                    Ok(PdfObject::Real(n))
                }
            }
            Token::Keyword(kw) => {
                match kw.as_str() {
                    "true" => Ok(PdfObject::Boolean(true)),
                    "false" => Ok(PdfObject::Boolean(false)),
                    "null" => Ok(PdfObject::Null),
                    _ => Ok(PdfObject::Name(kw)),
                }
            }
            _ => Err(LexerError::InvalidToken),
        }
    }
}
