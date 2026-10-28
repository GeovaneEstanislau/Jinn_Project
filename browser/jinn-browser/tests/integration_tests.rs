//! Comprehensive Integration and Security Test Suite for Jinn Browser 0.1.0

use jbe_core::{Color, EngineError, Point};
use jbe_net::MockLoader;
use jinn_browser::BrowserSession;

#[test]
fn test_full_pipeline_complex_document() {
    let mut loader = MockLoader::new();
    let html = r#"
        <html>
            <head>
                <style>
                    body { background-color: #101018; color: #ffffff; padding: 20px; }
                    .card { background-color: #1f1f2e; border: 1px; border-color: #3b82f6; padding: 15px; margin: 10px 0; }
                    h1 { color: #60a5fa; font-size: 22px; margin-bottom: 8px; }
                    p { color: #d1d5db; font-size: 13px; line-height: 18px; }
                    a { color: #34d399; font-weight: bold; }
                </style>
            </head>
            <body>
                <div class="card">
                    <h1>Jinn OS System Architecture</h1>
                    <p>Microkernel design with capability-based isolation, VFS, and IPC.</p>
                    <p><a href="details.html">Explore Details</a></p>
                </div>
            </body>
        </html>
    "#;

    loader.insert("file:///index.html", html.as_bytes().to_vec(), "text/html");

    let mut session = BrowserSession::new(640, 480, Box::new(loader));
    session.navigate("file:///index.html").expect("Navigation should succeed");

    // Verify DOM
    let dom = session.current_dom.as_ref().unwrap();
    assert!(dom.node_count() > 10);
    let h1_id = dom.find_first_by_tag("h1").expect("H1 element should exist");
    assert_eq!(dom.text_content(h1_id), "Jinn OS System Architecture");

    // Verify Layout Box Tree
    let layout = session.current_layout.as_ref().unwrap();
    assert_eq!(layout.dimensions.content.width, 640.0);

    // Verify Paint Commands
    let commands = &session.backend.commands;
    assert!(!commands.is_empty());

    // Verify Click Regions
    assert_eq!(session.backend.click_regions.len(), 1);
    assert_eq!(session.backend.click_regions[0].1, "details.html");

    // Verify Framebuffer Pixel values:
    // (0, 0) is html document background #12121a = rgb(18, 18, 26)
    let root_pixel = session.backend.get_pixel(0, 0).expect("Pixel (0,0) should exist");
    assert_eq!(root_pixel, Color::rgb(18, 18, 26));

    // (25, 25) is body background #101018 = rgb(16, 16, 24)
    let body_pixel = session.backend.get_pixel(25, 25).expect("Pixel (25,25) should exist");
    assert_eq!(body_pixel, Color::rgb(16, 16, 24));
}

#[test]
fn test_malformed_html_resilience() {
    let mut loader = MockLoader::new();
    // Broken markup: unclosed tags, overlapping tags, missing quotes, unclosed comments
    let broken_html = r#"
        <div>
            <h1>Broken Header
            <p>Paragraph with <b>bold <i>and italic</b> text</i></p>
            <img src=avatar.ppm alt=NoQuotes>
            <br>
            <!-- Unclosed comment at end of document
    "#;

    loader.insert("file:///broken.html", broken_html.as_bytes().to_vec(), "text/html");

    let mut session = BrowserSession::new(400, 300, Box::new(loader));
    let res = session.navigate("file:///broken.html");
    // The state-machine parser must safely recover without crashing or panic
    assert!(res.is_ok(), "Parser should safely recover from malformed HTML");

    let dom = session.current_dom.as_ref().unwrap();
    assert!(dom.find_first_by_tag("h1").is_some());
    assert!(dom.find_first_by_tag("img").is_some());
}

#[test]
fn test_security_path_traversal_rejection() {
    let mut loader = MockLoader::new();
    loader.insert(
        "file:///app/index.html",
        b"<a href=\"../../../../etc/shadow\">Leaked File</a>".to_vec(),
        "text/html",
    );

    let mut session = BrowserSession::new(400, 300, Box::new(loader));
    session.navigate("file:///app/index.html").unwrap();

    let click_pt = Point::new(
        session.backend.click_regions[0].0.x + 1.0,
        session.backend.click_regions[0].0.y + 1.0,
    );

    let res = session.click_at(click_pt);
    assert!(
        matches!(res, Err(EngineError::SecurityViolation(_))),
        "Path traversal escaping root must be rejected with SecurityViolation"
    );
}

#[test]
fn test_security_unsupported_schemes() {
    let loader = MockLoader::new();
    let mut session = BrowserSession::new(400, 300, Box::new(loader));

    // Reject javascript: execution
    let js_res = session.navigate("javascript:alert('malicious')");
    assert!(matches!(js_res, Err(EngineError::SecurityViolation(_))));

    // Reject unverified HTTPS (requires TLS)
    let https_res = session.navigate("https://secure.example.com");
    assert!(matches!(https_res, Err(EngineError::UnsupportedScheme(_))));

    // Reject data: scheme
    let data_res = session.navigate("data:text/html,<h1>evil</h1>");
    assert!(matches!(data_res, Err(EngineError::UnsupportedScheme(_))));
}

#[test]
fn test_resource_limits_enforced() {
    let mut loader = MockLoader::new();
    let huge_content = vec![b'A'; 600 * 1024]; // 600 KiB > 512 KiB limit
    loader.insert("file:///huge.html", huge_content, "text/html");

    let mut session = BrowserSession::new(400, 300, Box::new(loader));
    let res = session.navigate("file:///huge.html");
    assert!(
        matches!(res, Err(EngineError::ResourceTooLarge { .. })),
        "Document exceeding max_document_size must be rejected"
    );
}

#[test]
fn test_image_loading_and_bounded_decoding() {
    let mut loader = MockLoader::new();

    // 2x2 binary PPM image
    let mut ppm_bytes = Vec::new();
    ppm_bytes.extend_from_slice(b"P6\n2 2\n255\n");
    ppm_bytes.extend_from_slice(&[
        255, 0, 0,
        0, 255, 0,
        0, 0, 255,
        255, 255, 255,
    ]);

    loader.insert("file:///page_with_img.html", b"<html><body><img src=\"test.ppm\"></body></html>".to_vec(), "text/html");
    loader.insert("file:///test.ppm", ppm_bytes, "image/x-portable-pixmap");

    let mut session = BrowserSession::new(400, 300, Box::new(loader));
    let res = session.navigate("file:///page_with_img.html");
    assert!(res.is_ok(), "HTML with embedded image should load cleanly");
}

#[test]
fn test_navigation_history_back_forward_reload() {
    let mut loader = MockLoader::new();
    loader.insert("file:///page1.html", b"<h1>Page 1</h1><a href=\"page2.html\">Next</a>".to_vec(), "text/html");
    loader.insert("file:///page2.html", b"<h1>Page 2</h1><a href=\"page3.html\">Next</a>".to_vec(), "text/html");
    loader.insert("file:///page3.html", b"<h1>Page 3</h1>".to_vec(), "text/html");

    let mut session = BrowserSession::new(400, 300, Box::new(loader));
    session.navigate("file:///page1.html").unwrap();
    assert_eq!(session.history.len(), 1);

    session.navigate("file:///page2.html").unwrap();
    assert_eq!(session.history.len(), 2);
    assert_eq!(session.history_index, 1);

    session.navigate("file:///page3.html").unwrap();
    assert_eq!(session.history.len(), 3);
    assert_eq!(session.history_index, 2);

    // Back to page 2
    assert!(session.back().unwrap());
    assert_eq!(session.current_url().unwrap().path, "/page2.html");
    assert_eq!(session.history_index, 1);

    // Back to page 1
    assert!(session.back().unwrap());
    assert_eq!(session.current_url().unwrap().path, "/page1.html");
    assert_eq!(session.history_index, 0);

    // Forward to page 2
    assert!(session.forward().unwrap());
    assert_eq!(session.current_url().unwrap().path, "/page2.html");
    assert_eq!(session.history_index, 1);

    // Reload page 2
    assert!(session.reload().is_ok());
    assert_eq!(session.current_url().unwrap().path, "/page2.html");
}
