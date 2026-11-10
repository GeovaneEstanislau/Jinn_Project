//! Testes Unitários de Conformidade da ABI do Aladdin SDK

use aladdin_sdk::events::*;
use aladdin_sdk::graphics::*;
use aladdin_sdk::syscall::*;

#[test]
fn test_client_window_event_abi_layout() {
    assert_eq!(core::mem::size_of::<ClientWindowEvent>(), 32);
    assert_eq!(core::mem::align_of::<ClientWindowEvent>(), 8);

    let mut ev = ClientWindowEvent::empty();
    assert_eq!(ev.event_type, EVENT_NONE);
    assert!(!ev.is_close());

    ev.event_type = EVENT_WINDOW_CLOSE;
    assert!(ev.is_close());

    ev.event_type = EVENT_KEY_DOWN;
    ev.param1 = 0x1B; // ESC
    assert_eq!(ev.key_code(), Some(0x1B));

    ev.event_type = EVENT_MOUSE_DOWN;
    ev.param1 = 120;
    ev.param2 = 80;
    ev.param3 = 1; // Left button
    assert_eq!(ev.mouse_coords(), Some((120, 80)));
    assert_eq!(ev.mouse_button(), Some(1));
}

#[test]
fn test_syscall_number_mapping() {
    assert_eq!(SYS_EXIT, 0);
    assert_eq!(SYS_WRITE, 1);
    assert_eq!(SYS_READ, 2);
    assert_eq!(SYS_YIELD, 3);
    assert_eq!(SYS_GETPID, 4);
    assert_eq!(SYS_SLEEP, 11);
    assert_eq!(SYS_WINDOW_CREATE, 20);
    assert_eq!(SYS_WINDOW_BLIT, 21);
    assert_eq!(SYS_WINDOW_CLOSE, 22);
    assert_eq!(SYS_WINDOW_POLL_EVENT, 24);
}

#[test]
fn test_graphics_color_encoding() {
    let c = Color::rgb(0xFF, 0x00, 0x7F);
    assert_eq!(c.0, 0x00FF_007F);

    let ca = Color::rgba(0x12, 0x34, 0x56, 0xAA);
    assert_eq!(ca.0, 0xAA12_3456);
}

#[test]
fn test_canvas_primitives() {
    let mut buf = [0u32; 100]; // 10x10
    let mut canvas = Canvas::new(&mut buf, 10, 10).expect("Canvas should fit");

    canvas.clear(Color::BLACK);
    assert_eq!(canvas.buffer[0], 0);

    canvas.set_pixel(2, 3, Color::WHITE);
    assert_eq!(canvas.buffer[3 * 10 + 2], 0x00FF_FFFF);

    canvas.fill_rect(0, 0, 4, 4, Color::RED);
    assert_eq!(canvas.buffer[0], Color::RED.0);
    assert_eq!(canvas.buffer[3 * 10 + 3], Color::RED.0);
    assert_eq!(canvas.buffer[4 * 10 + 4], Color::BLACK.0);

    canvas.draw_rect(5, 5, 4, 4, Color::BLUE);
    assert_eq!(canvas.buffer[5 * 10 + 5], Color::BLUE.0);
    assert_eq!(canvas.buffer[8 * 10 + 8], Color::BLUE.0);

    canvas.draw_gradient(Color::BLACK, Color::WHITE);
    assert_eq!(canvas.buffer[0], 0); // top row black
}

#[test]
fn test_window_error_and_config() {
    use aladdin_sdk::window::{WindowConfig, WindowError};

    let cfg = WindowConfig::new("Teste Janela", 640, 480);
    assert_eq!(cfg.title, "Teste Janela");
    assert_eq!(cfg.width, 640);
    assert_eq!(cfg.height, 480);

    let err_dim = WindowError::InvalidDimensions;
    assert_eq!(err_dim.as_str(), "Dimensoes invalidas (maximo suportado 1024x768)");

    let err_title = WindowError::InvalidTitle;
    assert_eq!(err_title.as_str(), "Titulo de janela invalido ou muito longo (maximo 48 bytes)");

    let err_perm = WindowError::PermissionDenied;
    assert_eq!(err_perm.as_str(), "Permissao negada para a janela");
}

#[test]
fn test_extended_events_abi() {
    let mut ev = ClientWindowEvent::empty();
    ev.event_type = EVENT_WINDOW_RESIZE;
    ev.param1 = 800;
    ev.param2 = 600;

    assert!(ev.is_resize());
    assert_eq!(ev.resize_dims(), Some((800, 600)));

    ev.event_type = EVENT_FOCUS_GAINED;
    assert!(ev.is_focus_gained());
    assert!(!ev.is_focus_lost());

    ev.event_type = EVENT_FOCUS_LOST;
    assert!(ev.is_focus_lost());
    assert!(!ev.is_focus_gained());
}

#[test]
fn test_graphics_server_syscall_constants() {
    assert_eq!(SYS_GRAPHICS_REGISTER, 72);
    assert_eq!(SYS_GRAPHICS_GET_FB_INFO, 73);
    assert_eq!(SYS_GRAPHICS_PRESENT, 74);
    assert_eq!(SYS_GRAPHICS_POLL_INPUT, 75);
    assert_eq!(SYS_GRAPHICS_UNREGISTER, 76);
    assert_eq!(SYS_GRAPHICS_READ_CLIENT_SURFACE, 77);
    assert_eq!(SYS_GRAPHICS_DISPATCH_CLIENT_EVENT, 78);
    assert_eq!(SYS_GRAPHICS_LIST_CLIENT_WINDOWS, 79);
}

