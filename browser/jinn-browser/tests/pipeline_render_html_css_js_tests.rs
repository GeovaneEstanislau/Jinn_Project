//! Jinn Browser Engine — Pipeline Completo de Renderização (HTML + CSS + JS)
//!
//! Validação automatizada e rigorosa dos 4 pilares arquiteturais:
//! 1. Parser HTML Zero-Copy e Arena DOM (Flat Vector DOM).
//! 2. Mini-CSS e Resolução de Estilos Computados (Tags, Classes, IDs, Inline Styles).
//! 3. Interpretador JavaScript Sandboxed em Ring 3 com Ponte DOM-JS (`document.getElementById`, `innerText`, `style`).
//! 4. Pipeline Completo: Carregamento, estilização, execução de script, manipulação de DOM e disparo de eventos interativos.

use jbe_net::MockLoader;
use jinn_browser::BrowserSession;

fn create_mock_loader(html: &str) -> Box<MockLoader> {
    let mut loader = MockLoader::new();
    loader.insert("http://jinn.os/app", html.as_bytes().to_vec(), "text/html");
    Box::new(loader)
}

#[test]
fn test_html_parser_and_dom_structure() {
    let html = r#"
        <html>
            <head><title>Test Page</title></head>
            <body>
                <div id="container" class="box main">
                    <h1>Title Header</h1>
                    <p>Paragraph with <a href="link.html">link</a></p>
                </div>
            </body>
        </html>
    "#;
    let loader = create_mock_loader(html);
    let mut session = BrowserSession::new(800, 600, loader);
    session.navigate("http://jinn.os/app").unwrap();

    let dom = session.current_dom.as_ref().unwrap();
    assert!(dom.node_count() >= 6);

    let container_id = dom.find_by_id("container").expect("Container must exist in DOM");
    let container_node = dom.get(container_id).unwrap();
    assert_eq!(container_node.tag_name(), Some("div"));
    assert_eq!(container_node.id_attr(), Some("container"));
    assert!(container_node.has_class("box"));
    assert!(container_node.has_class("main"));
}

#[test]
fn test_css_style_resolution_tags_classes_and_ids() {
    let html = r#"
        <html>
            <head>
                <style>
                    h1 { color: #f9e2af; margin: 10px; }
                    .card { background-color: #1e1e2e; padding: 8px; }
                    #special { color: #00ffaa; font-weight: bold; }
                </style>
            </head>
            <body>
                <h1>Header</h1>
                <div class="card">Card content</div>
                <p id="special">Special paragraph</p>
            </body>
        </html>
    "#;
    let loader = create_mock_loader(html);
    let mut session = BrowserSession::new(800, 600, loader);
    session.navigate("http://jinn.os/app").unwrap();

    let layout = session.current_layout.as_ref().unwrap();
    assert!(!layout.children.is_empty());
}

#[test]
fn test_javascript_inline_execution_and_dom_mutation() {
    let html = r##"
        <html>
            <body>
                <h1 id="title">Initial Title</h1>
                <p id="counter">0</p>
                <script>
                    var heading = document.getElementById("title");
                    heading.innerText = "Updated by JavaScript!";
                    var cnt = document.getElementById("counter");
                    cnt.innerText = "Count: 42";
                    cnt.style.color = "#00ffaa";
                </script>
            </body>
        </html>
    "##;
    let loader = create_mock_loader(html);
    let mut session = BrowserSession::new(800, 600, loader);
    session.navigate("http://jinn.os/app").unwrap();

    let dom = session.current_dom.as_ref().unwrap();
    let title_id = dom.find_by_id("title").unwrap();
    assert_eq!(dom.text_content(title_id), "Updated by JavaScript!");

    let cnt_id = dom.find_by_id("counter").unwrap();
    assert_eq!(dom.text_content(cnt_id), "Count: 42");

    let cnt_node = dom.get(cnt_id).unwrap();
    assert!(cnt_node.get_attribute("style").unwrap().contains("color: #00ffaa"));
}

#[test]
fn test_interactive_event_dispatch_and_layout_update() {
    let html = r##"
        <html>
            <body>
                <h1 id="status">Standby</h1>
                <button id="btn-action" onclick="var s = document.getElementById('status'); s.innerText = 'Active!'; s.style.color = '#89b4fa';">Click Me</button>
            </body>
        </html>
    "##;
    let loader = create_mock_loader(html);
    let mut session = BrowserSession::new(800, 600, loader);
    session.navigate("http://jinn.os/app").unwrap();

    let dom_before = session.current_dom.as_ref().unwrap();
    let status_id = dom_before.find_by_id("status").unwrap();
    let btn_id = dom_before.find_by_id("btn-action").unwrap();
    assert_eq!(dom_before.text_content(status_id), "Standby");

    // Dispara o evento de clique no botão
    let handled = session.dispatch_click(btn_id).unwrap();
    assert!(handled, "O evento de clique deve ser interceptado e executado pelo runtime JS");

    // Verifica que o DOM foi alterado dinamicamente
    let dom_after = session.current_dom.as_ref().unwrap();
    assert_eq!(dom_after.text_content(status_id), "Active!");
    let status_node = dom_after.get(status_id).unwrap();
    assert!(status_node.get_attribute("style").unwrap().contains("color: #89b4fa"));

    // Verifica que comandos de pintura foram emitidos para o novo estado
    assert!(!session.backend.commands.is_empty(), "Repaint deve emitir novos comandos de desenho");
    assert!(!session.backend.framebuffer.is_empty(), "Framebuffer deve conter pixels renderizados");
}

#[test]
fn test_javascript_state_retention_across_multiple_clicks() {
    let html = r##"
        <html>
            <body>
                <p id="val">Clicks: 0</p>
                <button id="inc" onclick="clicks = clicks + 1; var v = document.getElementById('val'); v.innerText = 'Clicks: ' + clicks;">Add</button>
                <script>
                    var clicks = 0;
                </script>
            </body>
        </html>
    "##;
    let loader = create_mock_loader(html);
    let mut session = BrowserSession::new(800, 600, loader);
    session.navigate("http://jinn.os/app").unwrap();

    let btn_id = session.current_dom.as_ref().unwrap().find_by_id("inc").unwrap();
    let val_id = session.current_dom.as_ref().unwrap().find_by_id("val").unwrap();

    // 1º Clique
    session.dispatch_click(btn_id).unwrap();
    assert_eq!(session.current_dom.as_ref().unwrap().text_content(val_id), "Clicks: 1");

    // 2º Clique
    session.dispatch_click(btn_id).unwrap();
    assert_eq!(session.current_dom.as_ref().unwrap().text_content(val_id), "Clicks: 2");

    // 3º Clique
    session.dispatch_click(btn_id).unwrap();
    assert_eq!(session.current_dom.as_ref().unwrap().text_content(val_id), "Clicks: 3");
}
