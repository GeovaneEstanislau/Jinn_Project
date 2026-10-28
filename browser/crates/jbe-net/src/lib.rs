//! Resource loader abstraction and local transport for Jinn Browser Engine.
//!
//! Provides clean separation between resource requests and retrieval mechanisms,
//! enforcing strict sandboxing on local file paths and typed errors for remote schemes.

use std::fs;
use std::path::{Path, PathBuf};

use jbe_core::{EngineError, EngineLimits};
use jbe_url::{Url, UrlScheme};

/// A successfully loaded resource.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedResource {
    pub bytes: Vec<u8>,
    pub mime_type: String,
    pub url: Url,
}

/// Abstract interface for fetching resources.
pub trait ResourceLoader {
    /// Loads a resource by URL, validating permissions and limits.
    fn load(&self, url: &Url) -> Result<LoadedResource, EngineError>;
}

/// Local file system loader with directory containment security.
#[derive(Debug, Clone)]
pub struct FileSystemLoader {
    root_dir: Option<PathBuf>,
    limits: EngineLimits,
}

impl FileSystemLoader {
    /// Creates a loader with an optional sandbox root directory.
    pub fn new(root_dir: Option<PathBuf>, limits: EngineLimits) -> Self {
        Self { root_dir, limits }
    }

    /// Determines simple MIME type from file extension.
    pub fn guess_mime(path: &Path) -> String {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("html") | Some("htm") => "text/html".into(),
            Some("css") => "text/css".into(),
            Some("ppm") => "image/x-portable-pixmap".into(),
            Some("txt") => "text/plain".into(),
            _ => "application/octet-stream".into(),
        }
    }
}

impl ResourceLoader for FileSystemLoader {
    fn load(&self, url: &Url) -> Result<LoadedResource, EngineError> {
        match url.scheme {
            UrlScheme::File => {
                let rel_path = url.path.trim_start_matches('/');
                let target_path = if let Some(ref root) = self.root_dir {
                    let combined = root.join(rel_path);
                    // Verify canonicalization stays inside root
                    if let Ok(canon_root) = root.canonicalize() {
                        if let Ok(canon_target) = combined.canonicalize() {
                            if !canon_target.starts_with(&canon_root) {
                                return Err(EngineError::SecurityViolation(format!(
                                    "Path traversal outside root: {:?}",
                                    canon_target
                                )));
                            }
                        }
                    }
                    combined
                } else {
                    PathBuf::from(rel_path)
                };

                let metadata = fs::metadata(&target_path).map_err(|e| {
                    EngineError::ResourceNotFound(format!("{}: {}", target_path.display(), e))
                })?;

                if metadata.len() as usize > self.limits.max_document_size {
                    return Err(EngineError::ResourceTooLarge {
                        size: metadata.len() as usize,
                        max_size: self.limits.max_document_size,
                    });
                }

                let bytes = fs::read(&target_path).map_err(|e| {
                    EngineError::ResourceNotFound(format!("{}: {}", target_path.display(), e))
                })?;

                let mime_type = Self::guess_mime(&target_path);

                Ok(LoadedResource {
                    bytes,
                    mime_type,
                    url: url.clone(),
                })
            }
            UrlScheme::Http => Err(EngineError::NetworkError(
                "HTTP remote fetching is not configured on this host backend".into(),
            )),
            UrlScheme::Https => Err(EngineError::SecurityViolation(
                "HTTPS is disabled in v0.1.0 until TLS certificate validation is implemented".into(),
            )),
            UrlScheme::About => {
                let content = match url.path.as_str() {
                    "blank" => b"<html><body></body></html>".to_vec(),
                    "version" => b"<html><body><h1>Jinn Browser v0.1.0</h1><p>Experimental static document engine for Jinn OS.</p></body></html>".to_vec(),
                    _ => b"<html><body><h1>About Jinn Browser</h1></body></html>".to_vec(),
                };
                Ok(LoadedResource {
                    bytes: content,
                    mime_type: "text/html".into(),
                    url: url.clone(),
                })
            }
        }
    }
}

/// In-memory mock loader for unit tests.
#[derive(Debug, Clone, Default)]
pub struct MockLoader {
    resources: std::collections::HashMap<String, (Vec<u8>, String)>,
}

impl MockLoader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, url: &str, bytes: Vec<u8>, mime_type: &str) {
        self.resources.insert(url.to_string(), (bytes, mime_type.to_string()));
    }
}

impl ResourceLoader for MockLoader {
    fn load(&self, url: &Url) -> Result<LoadedResource, EngineError> {
        let key = url.to_string();
        if let Some((bytes, mime)) = self.resources.get(&key) {
            Ok(LoadedResource {
                bytes: bytes.clone(),
                mime_type: mime.clone(),
                url: url.clone(),
            })
        } else {
            Err(EngineError::ResourceNotFound(format!("Mock resource not found: {}", key)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_loader() {
        let mut loader = MockLoader::new();
        let url = Url::parse("file:///index.html").unwrap();
        loader.insert("file:///index.html", b"<h1>Hello</h1>".to_vec(), "text/html");

        let res = loader.load(&url).unwrap();
        assert_eq!(res.mime_type, "text/html");
        assert_eq!(res.bytes, b"<h1>Hello</h1>");
    }

    #[test]
    fn test_https_rejected() {
        let loader = FileSystemLoader::new(None, EngineLimits::default());
        let url = Url {
            scheme: UrlScheme::Https,
            host: Some("example.com".into()),
            port: None,
            path: "/".into(),
            query: None,
        };
        let res = loader.load(&url);
        assert!(matches!(res, Err(EngineError::SecurityViolation(_))));
    }
}
