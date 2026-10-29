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
    session
        .navigate("file:///index.html")
        .expect("Navigation should succeed");

    // Verify DOM
    let dom = session.current_dom.as_ref().unwrap();
    assert!(dom.node_count() > 10);
    let h1_id = dom
        .find_first_by_tag("h1")
        .expect("H1 element should exist");
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
    let root_pixel = session
        .backend
        .get_pixel(0, 0)
        .expect("Pixel (0,0) should exist");
    assert_eq!(root_pixel, Color::rgb(18, 18, 26));

    // (25, 25) is body background #101018 = rgb(16, 16, 24)
    let body_pixel = session
        .backend
        .get_pixel(25, 25)
        .expect("Pixel (25,25) should exist");
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

    loader.insert(
        "file:///broken.html",
        broken_html.as_bytes().to_vec(),
        "text/html",
    );

    let mut session = BrowserSession::new(400, 300, Box::new(loader));
    let res = session.navigate("file:///broken.html");
    // The state-machine parser must safely recover without crashing or panic
    assert!(
        res.is_ok(),
        "Parser should safely recover from malformed HTML"
    );

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
    ppm_bytes.extend_from_slice(&[255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255]);

    loader.insert(
        "file:///page_with_img.html",
        b"<html><body><img src=\"test.ppm\"></body></html>".to_vec(),
        "text/html",
    );
    loader.insert("file:///test.ppm", ppm_bytes, "image/x-portable-pixmap");

    let mut session = BrowserSession::new(400, 300, Box::new(loader));
    let res = session.navigate("file:///page_with_img.html");
    assert!(res.is_ok(), "HTML with embedded image should load cleanly");
}

#[test]
fn test_navigation_history_back_forward_reload() {
    let mut loader = MockLoader::new();
    loader.insert(
        "file:///page1.html",
        b"<h1>Page 1</h1><a href=\"page2.html\">Next</a>".to_vec(),
        "text/html",
    );
    loader.insert(
        "file:///page2.html",
        b"<h1>Page 2</h1><a href=\"page3.html\">Next</a>".to_vec(),
        "text/html",
    );
    loader.insert(
        "file:///page3.html",
        b"<h1>Page 3</h1>".to_vec(),
        "text/html",
    );

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

#[test]
fn test_viewport_scrolling_and_clamping() {
    let mut loader = MockLoader::new();
    let long_html = r#"
        <html>
            <head>
                <style>
                    body { background-color: #12121a; padding: 10px; }
                    .card { height: 200px; margin-bottom: 20px; background-color: #1f1f2e; }
                </style>
            </head>
            <body>
                <div class="card">Card 1</div>
                <div class="card">Card 2</div>
                <div class="card">Card 3</div>
                <div class="card">Card 4</div>
            </body>
        </html>
    "#;
    loader.insert(
        "file:///long.html",
        long_html.as_bytes().to_vec(),
        "text/html",
    );

    // Viewport height is 300px, content will be > 800px
    let mut session = BrowserSession::new(400, 300, Box::new(loader));
    session.navigate("file:///long.html").unwrap();

    let content_h = session.content_height();
    assert!(
        content_h > 800.0,
        "Content height should exceed 800px, got {}",
        content_h
    );
    let max_scroll = session.max_scroll_y();
    assert_eq!(max_scroll, content_h - 300.0);

    // Initial scroll is 0.0
    assert_eq!(session.scroll_y, 0.0);

    // Scroll down by 50px
    assert!(session.scroll(50.0));
    assert_eq!(session.scroll_y, 50.0);

    // Scroll up by 20px
    assert!(session.scroll(-20.0));
    assert_eq!(session.scroll_y, 30.0);

    // Negative scroll clamps at 0.0
    session.scroll(-100.0);
    assert_eq!(session.scroll_y, 0.0);

    // Excessive scroll clamps at max_scroll_y
    session.scroll(9999.0);
    assert_eq!(session.scroll_y, max_scroll);
}

#[test]
fn test_link_click_interaction_after_scrolling() {
    let mut loader = MockLoader::new();
    let doc1 = r#"
        <html>
            <head>
                <style>
                    body { padding: 0px; margin: 0px; }
                    .spacer { height: 800px; background-color: #101010; }
                    .linkbox { height: 50px; background-color: #202020; }
                </style>
            </head>
            <body>
                <div class="spacer">Spacer 800px</div>
                <div class="linkbox">
                    <a href="page_target.html">Click Scrolled Target</a>
                </div>
            </body>
        </html>
    "#;
    let doc2 = r#"
        <html>
            <body>
                <h1>Target Page Loaded Successfully</h1>
            </body>
        </html>
    "#;

    loader.insert("file:///doc1.html", doc1.as_bytes().to_vec(), "text/html");
    loader.insert(
        "file:///page_target.html",
        doc2.as_bytes().to_vec(),
        "text/html",
    );

    // Viewport height: 300px
    let mut session = BrowserSession::new(400, 300, Box::new(loader));
    session.navigate("file:///doc1.html").unwrap();

    // Link is initially at Y >= 800px, which is outside the 300px viewport
    let hit_before = session.click_at(Point::new(20.0, 50.0)).unwrap();
    assert_eq!(hit_before, None);

    // Scroll down by 550px so linkbox (at Y=800) is now visible at client Y = 800 - 550 = 250px
    session.scroll(550.0);
    assert_eq!(session.scroll_y, 550.0);

    // Now click lands on the scrolled link region (Y=250..300)!
    let hit_after = session.click_at(Point::new(20.0, 260.0)).unwrap();
    assert_eq!(hit_after, Some("page_target.html".to_string()));

    // Verify navigation occurred to doc2
    assert_eq!(session.current_url().unwrap().path, "/page_target.html");
    let dom = session.current_dom.as_ref().unwrap();
    let h1_id = dom.find_first_by_tag("h1").unwrap();
    assert_eq!(dom.text_content(h1_id), "Target Page Loaded Successfully");
}
