//! Deterministic state-machine HTML parser and DOM builder for Jinn Browser Engine.
//!
//! Follows an explicit state-machine (no regex) conforming to the subproject specification.
//! Safely recovers from malformed markup, handles void elements, and enforces EngineLimits.

use jbe_core::{EngineError, EngineLimits, NodeId};
use jbe_dom::DomTree;

/// Recognized void elements that never have closing tags or children.
pub const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Checks if an element name is a void element.
pub fn is_void_element(tag: &str) -> bool {
    let lower = tag.to_ascii_lowercase();
    VOID_ELEMENTS.iter().any(|&v| v == lower)
}

/// Decode basic HTML entities.
pub fn decode_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '&' {
            let mut entity = String::new();
            let mut closed = false;
            while let Some(&next_ch) = chars.peek() {
                if next_ch == ';' {
                    chars.next();
                    closed = true;
                    break;
                } else if next_ch.is_alphanumeric() || next_ch == '#' {
                    entity.push(chars.next().unwrap());
                    if entity.len() > 10 {
                        break;
                    }
                } else {
                    break;
                }
            }
            if closed {
                match entity.as_str() {
                    "amp" => out.push('&'),
                    "lt" => out.push('<'),
                    "gt" => out.push('>'),
                    "quot" => out.push('"'),
                    "apos" | "#39" => out.push('\''),
                    "nbsp" => out.push(' '),
                    _ => {
                        out.push('&');
                        out.push_str(&entity);
                        out.push(';');
                    }
                }
            } else {
                out.push('&');
                out.push_str(&entity);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[derive(Debug, PartialEq, Eq)]
enum Token {
    StartTag {
        name: String,
        attributes: Vec<(String, String)>,
        self_closing: bool,
    },
    EndTag {
        name: String,
    },
    Text(String),
    Comment(String),
}

/// Tokenizer states for the state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenizerState {
    Data,
    TagOpen,
    EndTagOpen,
    TagName,
    EndTagName,
    BeforeAttributeName,
    AttributeName,
    AfterAttributeName,
    BeforeAttributeValue,
    AttributeValueDoubleQuoted,
    AttributeValueSingleQuoted,
    AttributeValueUnquoted,
    AfterAttributeValueQuoted,
    SelfClosingStartTag,
    MarkupDeclarationOpen,
    Comment,
}

/// Tokenizes an HTML string into discrete tokens using an explicit state machine.
struct HtmlTokenizer<'a> {
    input: &'a str,
    pos: usize,
    state: TokenizerState,
    limits: &'a EngineLimits,
}

impl<'a> HtmlTokenizer<'a> {
    fn new(input: &'a str, limits: &'a EngineLimits) -> Self {
        Self {
            input,
            pos: 0,
            state: TokenizerState::Data,
            limits,
        }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn next_char(&mut self) -> Option<char> {
        if let Some(ch) = self.peek() {
            self.pos += ch.len_utf8();
            Some(ch)
        } else {
            None
        }
    }

    fn next_token(&mut self) -> Result<Option<Token>, EngineError> {
        let mut text_buf = String::new();
        let mut current_tag = String::new();
        let mut current_attrs: Vec<(String, String)> = Vec::new();
        let mut current_attr_name = String::new();
        let mut current_attr_val = String::new();
        let mut comment_buf = String::new();
        let mut self_closing = false;

        while let Some(ch) = self.peek() {
            match self.state {
                TokenizerState::Data => {
                    if ch == '<' {
                        self.next_char();
                        if !text_buf.is_empty() {
                            self.state = TokenizerState::TagOpen;
                            let decoded = decode_entities(&text_buf);
                            return Ok(Some(Token::Text(decoded)));
                        } else {
                            self.state = TokenizerState::TagOpen;
                        }
                    } else {
                        self.next_char();
                        text_buf.push(ch);
                    }
                }
                TokenizerState::TagOpen => {
                    if ch == '/' {
                        self.next_char();
                        self.state = TokenizerState::EndTagOpen;
                    } else if ch == '!' {
                        self.next_char();
                        self.state = TokenizerState::MarkupDeclarationOpen;
                    } else if ch.is_ascii_alphabetic() {
                        current_tag.clear();
                        current_attrs.clear();
                        self_closing = false;
                        self.state = TokenizerState::TagName;
                    } else {
                        // Stray '<', revert to data
                        text_buf.push('<');
                        self.state = TokenizerState::Data;
                    }
                }
                TokenizerState::EndTagOpen => {
                    if ch.is_ascii_alphabetic() {
                        current_tag.clear();
                        current_tag.push(ch);
                        self.next_char();
                        self.state = TokenizerState::EndTagName;
                    } else if ch == '>' {
                        self.next_char();
                        self.state = TokenizerState::Data;
                    } else {
                        self.state = TokenizerState::Data;
                    }
                }
                TokenizerState::EndTagName => {
                    if ch == '>' {
                        self.next_char();
                        self.state = TokenizerState::Data;
                        let token = Token::EndTag {
                            name: current_tag.to_ascii_lowercase(),
                        };
                        return Ok(Some(token));
                    } else if ch.is_ascii_whitespace() {
                        self.next_char();
                    } else {
                        self.next_char();
                        current_tag.push(ch);
                    }
                }
                TokenizerState::TagName => {
                    if ch.is_ascii_whitespace() {
                        self.next_char();
                        self.state = TokenizerState::BeforeAttributeName;
                    } else if ch == '/' {
                        self.next_char();
                        self.state = TokenizerState::SelfClosingStartTag;
                    } else if ch == '>' {
                        self.next_char();
                        self.state = TokenizerState::Data;
                        let token = Token::StartTag {
                            name: current_tag.to_ascii_lowercase(),
                            attributes: current_attrs,
                            self_closing,
                        };
                        return Ok(Some(token));
                    } else {
                        self.next_char();
                        current_tag.push(ch);
                    }
                }
                TokenizerState::BeforeAttributeName => {
                    if ch.is_ascii_whitespace() {
                        self.next_char();
                    } else if ch == '/' {
                        self.next_char();
                        self.state = TokenizerState::SelfClosingStartTag;
                    } else if ch == '>' {
                        self.next_char();
                        self.state = TokenizerState::Data;
                        let token = Token::StartTag {
                            name: current_tag.to_ascii_lowercase(),
                            attributes: current_attrs,
                            self_closing,
                        };
                        return Ok(Some(token));
                    } else {
                        current_attr_name.clear();
                        current_attr_val.clear();
                        self.state = TokenizerState::AttributeName;
                    }
                }
                TokenizerState::AttributeName => {
                    if ch.is_ascii_whitespace() {
                        self.next_char();
                        self.state = TokenizerState::AfterAttributeName;
                    } else if ch == '=' {
                        self.next_char();
                        self.state = TokenizerState::BeforeAttributeValue;
                    } else if ch == '/' {
                        self.save_attr(&mut current_attrs, &current_attr_name, &current_attr_val)?;
                        self.next_char();
                        self.state = TokenizerState::SelfClosingStartTag;
                    } else if ch == '>' {
                        self.save_attr(&mut current_attrs, &current_attr_name, &current_attr_val)?;
                        self.next_char();
                        self.state = TokenizerState::Data;
                        let token = Token::StartTag {
                            name: current_tag.to_ascii_lowercase(),
                            attributes: current_attrs,
                            self_closing,
                        };
                        return Ok(Some(token));
                    } else {
                        self.next_char();
                        current_attr_name.push(ch);
                    }
                }
                TokenizerState::AfterAttributeName => {
                    if ch.is_ascii_whitespace() {
                        self.next_char();
                    } else if ch == '=' {
                        self.next_char();
                        self.state = TokenizerState::BeforeAttributeValue;
                    } else if ch == '/' {
                        self.save_attr(&mut current_attrs, &current_attr_name, &current_attr_val)?;
                        self.next_char();
                        self.state = TokenizerState::SelfClosingStartTag;
                    } else if ch == '>' {
                        self.save_attr(&mut current_attrs, &current_attr_name, &current_attr_val)?;
                        self.next_char();
                        self.state = TokenizerState::Data;
                        let token = Token::StartTag {
                            name: current_tag.to_ascii_lowercase(),
                            attributes: current_attrs,
                            self_closing,
                        };
                        return Ok(Some(token));
                    } else {
                        self.save_attr(&mut current_attrs, &current_attr_name, &current_attr_val)?;
                        current_attr_name.clear();
                        current_attr_val.clear();
                        self.state = TokenizerState::AttributeName;
                    }
                }
                TokenizerState::BeforeAttributeValue => {
                    if ch.is_ascii_whitespace() {
                        self.next_char();
                    } else if ch == '"' {
                        self.next_char();
                        self.state = TokenizerState::AttributeValueDoubleQuoted;
                    } else if ch == '\'' {
                        self.next_char();
                        self.state = TokenizerState::AttributeValueSingleQuoted;
                    } else if ch == '>' {
                        self.save_attr(&mut current_attrs, &current_attr_name, &current_attr_val)?;
                        self.next_char();
                        self.state = TokenizerState::Data;
                        let token = Token::StartTag {
                            name: current_tag.to_ascii_lowercase(),
                            attributes: current_attrs,
                            self_closing,
                        };
                        return Ok(Some(token));
                    } else {
                        self.state = TokenizerState::AttributeValueUnquoted;
                    }
                }
                TokenizerState::AttributeValueDoubleQuoted => {
                    self.next_char();
                    if ch == '"' {
                        self.save_attr(&mut current_attrs, &current_attr_name, &current_attr_val)?;
                        self.state = TokenizerState::AfterAttributeValueQuoted;
                    } else {
                        current_attr_val.push(ch);
                    }
                }
                TokenizerState::AttributeValueSingleQuoted => {
                    self.next_char();
                    if ch == '\'' {
                        self.save_attr(&mut current_attrs, &current_attr_name, &current_attr_val)?;
                        self.state = TokenizerState::AfterAttributeValueQuoted;
                    } else {
                        current_attr_val.push(ch);
                    }
                }
                TokenizerState::AttributeValueUnquoted => {
                    if ch.is_ascii_whitespace() {
                        self.save_attr(&mut current_attrs, &current_attr_name, &current_attr_val)?;
                        self.next_char();
                        self.state = TokenizerState::BeforeAttributeName;
                    } else if ch == '>' {
                        self.save_attr(&mut current_attrs, &current_attr_name, &current_attr_val)?;
                        self.next_char();
                        self.state = TokenizerState::Data;
                        let token = Token::StartTag {
                            name: current_tag.to_ascii_lowercase(),
                            attributes: current_attrs,
                            self_closing,
                        };
                        return Ok(Some(token));
                    } else {
                        self.next_char();
                        current_attr_val.push(ch);
                    }
                }
                TokenizerState::AfterAttributeValueQuoted => {
                    if ch.is_ascii_whitespace() {
                        self.next_char();
                        self.state = TokenizerState::BeforeAttributeName;
                    } else if ch == '/' {
                        self.next_char();
                        self.state = TokenizerState::SelfClosingStartTag;
                    } else if ch == '>' {
                        self.next_char();
                        self.state = TokenizerState::Data;
                        let token = Token::StartTag {
                            name: current_tag.to_ascii_lowercase(),
                            attributes: current_attrs,
                            self_closing,
                        };
                        return Ok(Some(token));
                    } else {
                        self.state = TokenizerState::BeforeAttributeName;
                    }
                }
                TokenizerState::SelfClosingStartTag => {
                    if ch == '>' {
                        self.next_char();
                        self.state = TokenizerState::Data;
                        let token = Token::StartTag {
                            name: current_tag.to_ascii_lowercase(),
                            attributes: current_attrs,
                            self_closing: true,
                        };
                        return Ok(Some(token));
                    } else {
                        self.state = TokenizerState::BeforeAttributeName;
                    }
                }
                TokenizerState::MarkupDeclarationOpen => {
                    // Check if it's comment "!--"
                    if self.input[self.pos..].starts_with("--") {
                        self.pos += 2;
                        comment_buf.clear();
                        self.state = TokenizerState::Comment;
                    } else {
                        // e.g. DOCTYPE or unknown declaration, skip until '>'
                        while let Some(c) = self.next_char() {
                            if c == '>' {
                                break;
                            }
                        }
                        self.state = TokenizerState::Data;
                    }
                }
                TokenizerState::Comment => {
                    if self.input[self.pos..].starts_with("-->") {
                        self.pos += 3;
                        self.state = TokenizerState::Data;
                        return Ok(Some(Token::Comment(comment_buf)));
                    } else if let Some(c) = self.next_char() {
                        comment_buf.push(c);
                    } else {
                        // EOF inside comment
                        self.state = TokenizerState::Data;
                        return Ok(Some(Token::Comment(comment_buf)));
                    }
                }
            }
        }

        // EOF handling
        if !text_buf.is_empty() {
            let decoded = decode_entities(&text_buf);
            Ok(Some(Token::Text(decoded)))
        } else {
            Ok(None)
        }
    }

    fn save_attr(
        &self,
        attrs: &mut Vec<(String, String)>,
        name: &str,
        val: &str,
    ) -> Result<(), EngineError> {
        let trimmed_name = name.trim();
        if trimmed_name.is_empty() {
            return Ok(());
        }
        if attrs.len() >= self.limits.max_attributes_per_element {
            return Err(EngineError::DomLimitExceeded(format!(
                "Element exceeded maximum attributes limit of {}",
                self.limits.max_attributes_per_element
            )));
        }
        let total_len = trimmed_name.len() + val.len();
        if total_len > self.limits.max_attribute_length {
            return Err(EngineError::DomLimitExceeded(format!(
                "Attribute length {} exceeded limit of {}",
                total_len, self.limits.max_attribute_length
            )));
        }
        attrs.push((trimmed_name.to_ascii_lowercase(), decode_entities(val)));
        Ok(())
    }
}

/// Parses an HTML string into a DOM tree.
pub fn parse_html(html: &str, limits: &EngineLimits) -> Result<DomTree, EngineError> {
    if html.len() > limits.max_document_size {
        return Err(EngineError::ResourceTooLarge {
            size: html.len(),
            max_size: limits.max_document_size,
        });
    }

    let mut dom = DomTree::new(*limits);
    let mut open_stack: Vec<NodeId> = vec![dom.root()];
    let mut tokenizer = HtmlTokenizer::new(html, limits);

    while let Some(token) = tokenizer.next_token()? {
        match token {
            Token::StartTag {
                name,
                attributes,
                self_closing,
            } => {
                // Auto-close preceding tags if semantic rules apply
                auto_close_tags(&name, &mut open_stack, &dom);

                let current_parent = *open_stack.last().unwrap_or(&dom.root());
                let node_id = dom.create_element(&name, attributes)?;
                dom.append_child(current_parent, node_id)?;

                // If not void and not self-closing, push to open stack
                if !is_void_element(&name) && !self_closing {
                    open_stack.push(node_id);
                }
            }
            Token::EndTag { name } => {
                let name_lower = name.to_ascii_lowercase();
                // Find matching open tag in reverse
                let mut match_idx = None;
                for (idx, &node_id) in open_stack.iter().enumerate().rev() {
                    if let Some(node) = dom.get(node_id) {
                        if let Some(tag) = node.tag_name() {
                            if tag == name_lower {
                                match_idx = Some(idx);
                                break;
                            }
                        }
                    }
                }

                if let Some(idx) = match_idx {
                    // Close down to and including matched tag
                    open_stack.truncate(idx);
                }
                // If not found, ignore stray end tag safely
            }
            Token::Text(text) => {
                // Only create text node if not purely whitespace between top-level elements,
                // or if it has meaningful content
                if !text.is_empty() {
                    let current_parent = *open_stack.last().unwrap_or(&dom.root());
                    let text_id = dom.create_text(&text)?;
                    dom.append_child(current_parent, text_id)?;
                }
            }
            Token::Comment(comment) => {
                let current_parent = *open_stack.last().unwrap_or(&dom.root());
                let comment_id = dom.create_comment(&comment)?;
                let _ = dom.append_child(current_parent, comment_id);
            }
        }
    }

    Ok(dom)
}

/// Auto-closes specific open elements according to HTML rules (e.g. `<p>` closes open `<p>`, `<li>` closes `<li>`).
fn auto_close_tags(next_tag: &str, open_stack: &mut Vec<NodeId>, dom: &DomTree) {
    if open_stack.len() <= 1 {
        return;
    }
    let last_id = *open_stack.last().unwrap();
    if let Some(node) = dom.get(last_id) {
        if let Some(last_tag) = node.tag_name() {
            if (next_tag == "p" && last_tag == "p")
                || (next_tag == "li" && last_tag == "li")
                || ((next_tag == "h1"
                    || next_tag == "h2"
                    || next_tag == "h3"
                    || next_tag == "h4"
                    || next_tag == "h5"
                    || next_tag == "h6")
                    && (last_tag == "p"))
            {
                open_stack.pop();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_and_minimal() {
        let limits = EngineLimits::default();
        let dom = parse_html("", &limits).unwrap();
        assert_eq!(dom.node_count(), 1); // Document root only

        let html = "<html><head><title>Title</title></head><body><h1>Heading</h1></body></html>";
        let dom2 = parse_html(html, &limits).unwrap();
        let h1_id = dom2.find_first_by_tag("h1").unwrap();
        assert_eq!(dom2.text_content(h1_id), "Heading");
    }

    #[test]
    fn test_parse_attributes_and_entities() {
        let limits = EngineLimits::default();
        let html = r#"<div id="container" class="primary bold" data-value="123 &amp; 456">Hello &lt;World&gt;</div>"#;
        let dom = parse_html(html, &limits).unwrap();
        let div = dom.find_first_by_tag("div").unwrap();
        let node = dom.get(div).unwrap();

        assert_eq!(node.id_attr(), Some("container"));
        assert!(node.has_class("primary"));
        assert!(node.has_class("bold"));
        assert_eq!(node.get_attribute("data-value"), Some("123 & 456"));
        assert_eq!(dom.text_content(div), "Hello <World>");
    }

    #[test]
    fn test_void_elements() {
        let limits = EngineLimits::default();
        let html = r#"<div>Line 1<br>Line 2<hr><img src="test.ppm" alt="Image"></div>"#;
        let dom = parse_html(html, &limits).unwrap();
        let div = dom.find_first_by_tag("div").unwrap();
        let div_node = dom.get(div).unwrap();

        // br, hr, img are children of div, not nesting into each other!
        assert_eq!(div_node.children.len(), 5); // "Line 1", br, "Line 2", hr, img
        let img = dom.find_first_by_tag("img").unwrap();
        let img_node = dom.get(img).unwrap();
        assert_eq!(img_node.get_attribute("src"), Some("test.ppm"));
    }

    #[test]
    fn test_unclosed_tags_and_auto_close() {
        let limits = EngineLimits::default();
        let html = "<p>First paragraph<p>Second paragraph";
        let dom = parse_html(html, &limits).unwrap();
        let ps = dom.find_all_by_tag("p");
        assert_eq!(ps.len(), 2);
        assert_eq!(dom.text_content(ps[0]), "First paragraph");
        assert_eq!(dom.text_content(ps[1]), "Second paragraph");
    }

    #[test]
    fn test_comments() {
        let limits = EngineLimits::default();
        let html = "<div>Before <!-- This is a comment --> After</div>";
        let dom = parse_html(html, &limits).unwrap();
        let div = dom.find_first_by_tag("div").unwrap();
        assert_eq!(dom.text_content(div), "Before  After");
    }

    #[test]
    fn test_document_size_limit() {
        let limits = EngineLimits {
            max_document_size: 20,
            ..Default::default()
        };
        let res = parse_html("<div>This document exceeds 20 bytes limit</div>", &limits);
        assert!(matches!(res, Err(EngineError::ResourceTooLarge { .. })));
    }
}
