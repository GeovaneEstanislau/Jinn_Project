//! Jinn Browser - Reference Application and CLI Runner
//!
//! Demonstrates the complete in-house static document rendering pipeline:
//! URL -> ResourceLoader -> HTML Parser -> DOM -> CSS Resolver -> Layout -> Paint -> RenderBackend.

use std::env;
use std::fs;
use std::path::PathBuf;

use jbe_core::{EngineLimits, Point};
use jbe_net::{FileSystemLoader, MockLoader};
use jinn_browser::BrowserSession;

fn main() {
    println!("============================================================");
    println!("   JINN BROWSER 0.1.0 - Native In-House Rendering Engine   ");
    println!("============================================================");

    let args: Vec<String> = env::args().collect();
    let mut loader = MockLoader::new();

    // Prepare demo pages in mock loader
    let index_html = r#"
        <html>
            <head>
                <style>
                    body { background-color: #12121a; color: #f0f0f5; padding: 12px; }
                    .header-box { background-color: #1a1a2e; border: 2px; border-color: #00d2ff; padding: 10px; margin: 8px 0; }
                    h1 { color: #00d2ff; font-size: 20px; }
                    p { color: #b0b0c0; font-size: 12px; }
                    .card { background-color: #202035; padding: 8px; margin: 6px 0; }
                    a { color: #00ffaa; font-weight: bold; }
                </style>
            </head>
            <body>
                <div class="header-box">
                    <h1>Jinn OS Browser v0.1.0</h1>
                    <p>Native rendering engine built in pure Rust. No Blink, Gecko, or Servo.</p>
                </div>
                <div class="card">
                    <p>Modular Architecture: Core, URL, DOM, HTML, CSS, Layout, Render, Net.</p>
                    <p><a href="about.html">Read About Jinn Engine &gt;</a></p>
                </div>
            </body>
        </html>
    "#;

    let about_html = r#"
        <html>
            <body style="background-color: #181824; color: #ffffff; padding: 16px;">
                <h1 style="color: #ff9900;">About Jinn Browser</h1>
                <p>Features implemented:</p>
                <p>- Deterministic State Machine HTML Parser</p>
                <p>- DOM Tree Arena Storage with Node and Depth Limits</p>
                <p>- CSS Cascade, Inheritance, and Default UA Stylesheet</p>
                <p>- Block Flow Layout with Text Line Wrapping</p>
                <p>- Paint Command Generator & In-Memory ARGB Test Backend</p>
                <p><a href="index.html">&lt; Back to Home</a></p>
            </body>
        </html>
    "#;

    loader.insert("file:///index.html", index_html.as_bytes().to_vec(), "text/html");
    loader.insert("file:///about.html", about_html.as_bytes().to_vec(), "text/html");

    let mut session = BrowserSession::new(640, 480, Box::new(loader));

    let start_url = if args.len() > 1 {
        // If file exists on disk, load real file
        let path = PathBuf::from(&args[1]);
        if path.exists() {
            let canon = fs::canonicalize(&path).unwrap_or(path.clone());
            let path_str = canon.to_str().unwrap_or("").replace('\\', "/");
            let file_url = format!("file:///{}", path_str.trim_start_matches('/'));
            let fs_loader = FileSystemLoader::new(None, EngineLimits::default());
            session = BrowserSession::new(640, 480, Box::new(fs_loader));
            file_url
        } else {
            args[1].clone()
        }
    } else {
        "file:///index.html".to_string()
    };

    println!("[*] Navigating to: {}", start_url);
    if let Err(e) = session.navigate(&start_url) {
        eprintln!("[!] Navigation failed: {}", e);
        return;
    }

    println!("\n[*] DOM Nodes count: {}", session.current_dom.as_ref().map(|d| d.node_count()).unwrap_or(0));
    println!("[*] Paint Commands count: {}", session.backend.commands.len());
    println!("[*] Click regions registered: {}", session.backend.click_regions.len());

    println!("\n[*] Paint Commands Log:");
    for (i, cmd) in session.backend.commands.iter().take(10).enumerate() {
        println!("    [{:02}] {:?}", i, cmd);
    }
    if session.backend.commands.len() > 10 {
        println!("    ... and {} more commands.", session.backend.commands.len() - 10);
    }

    println!("\n[*] Rendered Visual Preview (ASCII Canvas {}x{}):", 60, 20);
    let ascii = session.backend.render_ascii_canvas(60, 20);
    println!("{}", ascii);

    // Demonstrate link interaction
    if !session.backend.click_regions.is_empty() {
        let (rect, ref target) = session.backend.click_regions[0];
        println!("[*] Simulating click on registered link '{}' at ({}, {})...", target, rect.x + 2.0, rect.y + 2.0);
        match session.click_at(Point::new(rect.x + 2.0, rect.y + 2.0)) {
            Ok(Some(nav_target)) => {
                println!("[*] Successfully navigated to: {}", nav_target);
                println!("[*] New DOM Node Count: {}", session.current_dom.as_ref().map(|d| d.node_count()).unwrap_or(0));
                println!("[*] History length: {}, current index: {}", session.history.len(), session.history_index);
            }
            Ok(None) => println!("[*] Click did not hit any active link."),
            Err(e) => eprintln!("[!] Click navigation error: {}", e),
        }
    }

    println!("\n[+] Jinn Browser execution completed cleanly.");
}
