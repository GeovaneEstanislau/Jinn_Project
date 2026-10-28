//! # jbe-url — URL Representation, Validation, and Resolution
//!
//! Provides deterministic URL parsing, scheme classification, path normalization,
//! and secure relative reference resolution adhering to the Jinn OS security model.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use jbe_core::EngineError;

/// Supported URL Schemes in Jinn Browser Engine v0.1.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UrlScheme {
    /// Local filesystem file path (e.g. `file:///disk/page.html` or `file:///C:/page.html`).
    File,
    /// Unencrypted HTTP (e.g. `http://10.0.2.2:8080/index.html`).
    Http,
    /// Encrypted HTTPS (disabled in v0.1.0 until TLS verification is implemented).
    Https,
    /// Browser internal metadata pages (e.g. `about:blank`, `about:version`).
    About,
}

impl fmt::Display for UrlScheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File => write!(f, "file"),
            Self::Http => write!(f, "http"),
            Self::Https => write!(f, "https"),
            Self::About => write!(f, "about"),
        }
    }
}

/// Structured URL representing an authorized resource target.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Url {
    pub scheme: UrlScheme,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub path: String,
    pub query: Option<String>,
}

impl Url {
    /// Parses an absolute URL string according to v0.1.0 rules.
    pub fn parse(input: &str) -> Result<Self, EngineError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(EngineError::InvalidUrl("Empty URL input".into()));
        }

        // 1. Detect and parse "about:" scheme
        if let Some(rest) = input.strip_prefix("about:") {
            let path = rest.split('?').next().unwrap_or("").trim().to_string();
            if path != "blank" && path != "version" && path != "help" {
                return Err(EngineError::InvalidUrl(format!("Unknown about: target '{}'", path)));
            }
            return Ok(Self {
                scheme: UrlScheme::About,
                host: None,
                port: None,
                path,
                query: None,
            });
        }

        // 2. Reject explicitly unsupported schemes with helpful errors
        if input.starts_with("https://") {
            return Err(EngineError::UnsupportedScheme(
                "https:// is unsupported in v0.1.0 (requires TLS / cryptographic verification)".into(),
            ));
        }
        if input.starts_with("javascript:") {
            return Err(EngineError::SecurityViolation(
                "javascript: URLs are rejected by security policy (no script execution in v0.1.0)".into(),
            ));
        }
        if input.starts_with("data:") {
            return Err(EngineError::UnsupportedScheme("data: scheme is unsupported in v0.1.0".into()));
        }
        if input.starts_with("ftp://") {
            return Err(EngineError::UnsupportedScheme("ftp:// scheme is unsupported".into()));
        }

        // 3. Parse "file://" scheme
        if let Some(rest) = input.strip_prefix("file://") {
            let (path_part, query) = split_query(rest);
            let normalized_path = normalize_path(path_part)?;
            if normalized_path.is_empty() {
                return Err(EngineError::InvalidUrl("file:// URL with empty path".into()));
            }
            return Ok(Self {
                scheme: UrlScheme::File,
                host: None,
                port: None,
                path: normalized_path,
                query,
            });
        }

        // 4. Parse "http://" scheme
        if let Some(rest) = input.strip_prefix("http://") {
            let (authority_and_path, query) = split_query(rest);
            let (authority, path) = match authority_and_path.find('/') {
                Some(idx) => (&authority_and_path[..idx], &authority_and_path[idx..]),
                None => (authority_and_path, "/"),
            };

            let (host, port) = parse_authority(authority)?;
            let normalized_path = normalize_path(path)?;

            return Ok(Self {
                scheme: UrlScheme::Http,
                host: Some(host),
                port,
                path: normalized_path,
                query,
            });
        }

        // 5. Fallback: treat absolute local paths (e.g. `/disk/test.html` or `C:/...`) as file URLs
        if input.starts_with('/') || (input.len() >= 3 && input.as_bytes()[1] == b':' && input.as_bytes()[2] == b'/') {
            let (path_part, query) = split_query(input);
            return Ok(Self {
                scheme: UrlScheme::File,
                host: None,
                port: None,
                path: normalize_path(path_part)?,
                query,
            });
        }

        Err(EngineError::InvalidUrl(format!("Unrecognized URL format or missing scheme: '{}'", input)))
    }

    /// Resolves a relative reference string against `self` as a base URL.
    pub fn resolve_relative(&self, relative: &str) -> Result<Self, EngineError> {
        let relative = relative.trim();
        if relative.is_empty() {
            return Ok(self.clone());
        }

        // If it starts with a scheme, parse it as an absolute URL
        if relative.contains("://") || relative.starts_with("about:") {
            return Self::parse(relative);
        }

        match self.scheme {
            UrlScheme::About => {
                Err(EngineError::InvalidUrl("Cannot resolve relative URL against about: page".into()))
            }
            UrlScheme::File => {
                let (rel_path, query) = split_query(relative);
                let merged_path = if rel_path.starts_with('/') {
                    rel_path.to_string()
                } else {
                    let base_dir = match self.path.rfind('/') {
                        Some(idx) => &self.path[..=idx],
                        None => "/",
                    };
                    format!("{}{}", base_dir, rel_path)
                };

                let normalized = normalize_path(&merged_path)?;

                // Security Check: Path Traversal Protection
                // Ensure the relative resolution did not escape below root
                if !normalized.starts_with('/') && !normalized.contains(':') {
                    return Err(EngineError::SecurityViolation(
                        "Relative path escaped root directory boundary".into(),
                    ));
                }

                Ok(Self {
                    scheme: UrlScheme::File,
                    host: None,
                    port: None,
                    path: normalized,
                    query: query.or_else(|| self.query.clone()),
                })
            }
            UrlScheme::Http | UrlScheme::Https => {
                let (rel_path, query) = split_query(relative);
                let merged_path = if rel_path.starts_with('/') {
                    rel_path.to_string()
                } else {
                    let base_dir = match self.path.rfind('/') {
                        Some(idx) => &self.path[..=idx],
                        None => "/",
                    };
                    format!("{}{}", base_dir, rel_path)
                };

                Ok(Self {
                    scheme: self.scheme,
                    host: self.host.clone(),
                    port: self.port,
                    path: normalize_path(&merged_path)?,
                    query,
                })
            }
        }
    }

    /// Returns the normalized string representation of this URL.
    pub fn as_str(&self) -> String {
        match self.scheme {
            UrlScheme::About => format!("about:{}", self.path),
            UrlScheme::File => format!("file://{}", self.path),
            UrlScheme::Http | UrlScheme::Https => {
                let scheme_str = if self.scheme == UrlScheme::Https { "https" } else { "http" };
                let host = self.host.as_deref().unwrap_or("localhost");
                let port_str = match self.port {
                    Some(p) if (self.scheme == UrlScheme::Http && p != 80) || (self.scheme == UrlScheme::Https && p != 443) => {
                        format!(":{}", p)
                    }
                    _ => String::new(),
                };
                let query_str = match &self.query {
                    Some(q) => format!("?{}", q),
                    None => String::new(),
                };
                format!("{}://{}{}{}{}", scheme_str, host, port_str, self.path, query_str)
            }
        }
    }
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ── Parsing Helpers ──────────────────────────────────────────────────────────

fn split_query(input: &str) -> (&str, Option<String>) {
    match input.find('?') {
        Some(idx) => (&input[..idx], Some(input[idx + 1..].to_string())),
        None => (input, None),
    }
}

fn parse_authority(authority: &str) -> Result<(String, Option<u16>), EngineError> {
    if authority.is_empty() {
        return Err(EngineError::InvalidUrl("Empty HTTP authority".into()));
    }
    match authority.rfind(':') {
        Some(idx) => {
            let host = authority[..idx].trim().to_string();
            let port_str = &authority[idx + 1..];
            let port = port_str
                .parse::<u16>()
                .map_err(|_| EngineError::InvalidUrl(format!("Invalid port '{}'", port_str)))?;
            Ok((host, Some(port)))
        }
        None => Ok((authority.trim().to_string(), None)),
    }
}

/// Normalizes path by resolving `.` and `..` components cleanly without regex.
/// Enforces path traversal security: returns an error if `..` attempts to traverse above root.
pub fn normalize_path(path: &str) -> Result<String, EngineError> {
    let has_leading_slash = path.starts_with('/');
    let has_drive_letter = path.len() >= 2 && path.as_bytes()[1] == b':';
    let min_parts = if has_drive_letter { 1 } else { 0 };
    let mut parts: Vec<&str> = Vec::new();

    for seg in path.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        } else if seg == ".." {
            if parts.len() > min_parts {
                parts.pop();
            } else {
                return Err(EngineError::SecurityViolation(
                    "Path traversal attempt escaped root directory boundary".into(),
                ));
            }
        } else {
            parts.push(seg);
        }
    }

    let mut result = String::new();
    if has_leading_slash {
        result.push('/');
    }
    for (i, p) in parts.iter().enumerate() {
        if i > 0 || (has_leading_slash && !result.ends_with('/')) {
            result.push('/');
        }
        result.push_str(p);
    }
    if result.is_empty() && has_leading_slash {
        result.push('/');
    }
    Ok(result)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_file_url() {
        let url = Url::parse("file:///disk/index.html").unwrap();
        assert_eq!(url.scheme, UrlScheme::File);
        assert_eq!(url.path, "/disk/index.html");
        assert_eq!(url.host, None);
        assert_eq!(url.as_str(), "file:///disk/index.html");
    }

    #[test]
    fn test_parse_http_url() {
        let url = Url::parse("http://10.0.2.2:8080/docs/readme.html?v=1").unwrap();
        assert_eq!(url.scheme, UrlScheme::Http);
        assert_eq!(url.host.as_deref(), Some("10.0.2.2"));
        assert_eq!(url.port, Some(8080));
        assert_eq!(url.path, "/docs/readme.html");
        assert_eq!(url.query.as_deref(), Some("v=1"));
    }

    #[test]
    fn test_parse_about_url() {
        let url = Url::parse("about:blank").unwrap();
        assert_eq!(url.scheme, UrlScheme::About);
        assert_eq!(url.path, "blank");

        let url2 = Url::parse("about:version").unwrap();
        assert_eq!(url2.path, "version");

        assert!(Url::parse("about:unsupported").is_err());
    }

    #[test]
    fn test_relative_resolution_same_dir() {
        let base = Url::parse("file:///disk/docs/index.html").unwrap();
        let resolved = base.resolve_relative("styles.css").unwrap();
        assert_eq!(resolved.path, "/disk/docs/styles.css");
    }

    #[test]
    fn test_relative_resolution_parent_dir() {
        let base = Url::parse("file:///disk/docs/sub/page.html").unwrap();
        let resolved = base.resolve_relative("../images/logo.png").unwrap();
        assert_eq!(resolved.path, "/disk/docs/images/logo.png");
    }

    #[test]
    fn test_rejection_of_unsupported_and_dangerous_schemes() {
        // HTTPS rejection (TLS requirement)
        let err_https = Url::parse("https://example.com").unwrap_err();
        match err_https {
            EngineError::UnsupportedScheme(_) => {}
            other => panic!("Expected UnsupportedScheme, got {:?}", other),
        }

        // JavaScript injection rejection
        let err_js = Url::parse("javascript:alert(1)").unwrap_err();
        match err_js {
            EngineError::SecurityViolation(_) => {}
            other => panic!("Expected SecurityViolation, got {:?}", other),
        }

        // Data URI rejection
        assert!(matches!(
            Url::parse("data:text/html,<b>hi</b>").unwrap_err(),
            EngineError::UnsupportedScheme(_)
        ));
    }
}
