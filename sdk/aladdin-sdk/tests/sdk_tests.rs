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
}

