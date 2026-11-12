//! Testes de Estresse e Validação de Pacotes do Driver de Mouse PS/2 (i8042 Aux)
//!
//! Valida a máquina de estados de 3 bytes, extensão de sinal 9-bit, descartes por overflow,
//! rejeição de bytes órfãos sem bit de sincronismo (bit 3), recuperação de desfasamento
//! temporal, limitação de coordenadas nas bordas do framebuffer e telemetria atômica.

#![allow(dead_code, unused_imports)]

use core::sync::atomic::Ordering;

#[path = "../src/interrupts/ps2_mouse.rs"]
mod ps2_mouse;

#[path = "../src/timer.rs"]
mod timer;

// Mock simplificado do subsistema PIC e Device Manager para execução em host
mod interrupts {
    pub mod pic {
        use core::sync::atomic::{AtomicU8, Ordering};
        pub static PIC1_MASK: AtomicU8 = AtomicU8::new(0xFF);
        pub static PIC2_MASK: AtomicU8 = AtomicU8::new(0xFF);

        pub fn mask_irq(irq: u8) {
            if irq < 8 {
                PIC1_MASK.fetch_or(1 << irq, Ordering::Relaxed);
            } else {
                PIC2_MASK.fetch_or(1 << (irq - 8), Ordering::Relaxed);
            }
        }

        pub fn unmask_irq(irq: u8) {
            if irq < 8 {
                PIC1_MASK.fetch_and(!(1 << irq), Ordering::Relaxed);
            } else {
                PIC2_MASK.fetch_and(!(1 << (irq - 8)), Ordering::Relaxed);
                PIC1_MASK.fetch_and(!(1 << 2), Ordering::Relaxed);
            }
        }

        pub fn send_eoi(_irq: u8) {}
    }

    pub mod ps2_keyboard {
        pub fn process_scancode(_sc: u8) {}
    }
}

mod limine {
    pub struct Framebuffer {
        pub width: u64,
        pub height: u64,
    }
    pub fn framebuffer() -> Option<Framebuffer> {
        Some(Framebuffer { width: 1024, height: 768 })
    }
}

mod drivers {
    pub mod device_manager {
        pub const DEV_KBD0: u32 = 0x1001;
        pub const DEV_MOUSE0: u32 = 0x1002;
        pub const INPUT_KIND_MOUSE_BUTTON_DOWN: u16 = 4;
        pub const INPUT_KIND_MOUSE_BUTTON_UP: u16 = 5;

        pub fn post_raw_hardware_byte(_dev: u32, _irq: u8, _byte: u8) -> bool {
            false // Retorna false para simular fallback padrão no driver nativo de kernel
        }
    }
}

fn setup_mouse() {
    ps2_mouse::set_bounds(1024, 768);
    ps2_mouse::reset_sync();
    ps2_mouse::MOUSE_X.store(512, Ordering::SeqCst);
    ps2_mouse::MOUSE_Y.store(384, Ordering::SeqCst);
    ps2_mouse::MOUSE_BUTTONS.store(0, Ordering::SeqCst);
    ps2_mouse::LAST_BYTE_TICK.store(0, Ordering::SeqCst);
}

#[test]
fn test_mouse_positive_deltas_and_clamping() {
    setup_mouse();

    // Pacote 1: Flags=0x08 (bit 3 ativo, sem botões, deltas positivos)
    // DeltaX = +20, DeltaY = +15 (PS/2 Delta Y positivo move para CIMA na tela, ou seja, reduz Y)
    ps2_mouse::process_byte(0x08);
    assert_eq!(ps2_mouse::get_cycle(), 1);
    ps2_mouse::process_byte(20);
    assert_eq!(ps2_mouse::get_cycle(), 2);
    ps2_mouse::process_byte(15);
    assert_eq!(ps2_mouse::get_cycle(), 0);

    let (x, y, btn) = ps2_mouse::get_state();
    assert_eq!(x, 512 + 20); // 532
    assert_eq!(y, 384 - 15); // 369
    assert_eq!(btn, 0);

    // Movimenta até o limite direito (max_x - 1 = 1023)
    // Flags=0x08, DeltaX = 250, DeltaY = 0 (duas vezes para estourar o limite de 1024)
    for _ in 0..3 {
        ps2_mouse::process_byte(0x08);
        ps2_mouse::process_byte(250);
        ps2_mouse::process_byte(0);
    }
    let (clamped_x, _, _) = ps2_mouse::get_state();
    assert_eq!(clamped_x, 1023);
}

#[test]
fn test_mouse_negative_deltas_sign_extension() {
    setup_mouse();

    // Pacote com movimento negativo:
    // Bit 4 = 1 (sinal negativo para X), Bit 5 = 1 (sinal negativo para Y)
    // Flags = 0x08 | 0x10 | 0x20 = 0x38
    // DeltaX = 0xFE (-2 em complemento de dois) -> (254 - 256) = -2
    // DeltaY = 0xFD (-3 em complemento de dois) -> (253 - 256) = -3
    // PS/2: DeltaY negativo move para BAIXO na tela, ou seja, aumenta Y (+3)
    ps2_mouse::process_byte(0x38);
    ps2_mouse::process_byte(0xFE);
    ps2_mouse::process_byte(0xFD);

    let (x, y, btn) = ps2_mouse::get_state();
    assert_eq!(x, 512 - 2); // 510
    assert_eq!(y, 384 + 3); // 387
    assert_eq!(btn, 0);

    // Testa limite superior esquerdo (0, 0)
    for _ in 0..4 {
        ps2_mouse::process_byte(0x18); // DeltaX negativo (-250), DeltaY 0
        ps2_mouse::process_byte(6);    // 6 - 256 = -250
        ps2_mouse::process_byte(0);
    }
    let (zero_x, _, _) = ps2_mouse::get_state();
    assert_eq!(zero_x, 0);
}

#[test]
fn test_mouse_button_states_and_transitions() {
    setup_mouse();

    // 1. Clique Esquerdo: Flags bit 0 = 1 -> Flags = 0x09
    ps2_mouse::process_byte(0x09);
    ps2_mouse::process_byte(0);
    ps2_mouse::process_byte(0);
    let (_, _, btn) = ps2_mouse::get_state();
    assert_eq!(btn, 1); // Left click

    // 2. Clique Direito junto com Esquerdo: Flags bit 0 e bit 1 -> Flags = 0x0B
    ps2_mouse::process_byte(0x0B);
    ps2_mouse::process_byte(0);
    ps2_mouse::process_byte(0);
    let (_, _, btn2) = ps2_mouse::get_state();
    assert_eq!(btn2, 3); // Left + Right click

    // 3. Liberação de todos os botões: Flags = 0x08
    ps2_mouse::process_byte(0x08);
    ps2_mouse::process_byte(0);
    ps2_mouse::process_byte(0);
    let (_, _, btn3) = ps2_mouse::get_state();
    assert_eq!(btn3, 0);
}

#[test]
fn test_mouse_overflow_packet_discarded() {
    setup_mouse();
    let initial_overflow = ps2_mouse::DISCARDED_OVERFLOW.load(Ordering::Relaxed);

    // Pacote com bit de overflow ativo:
    // Bit 6 = 1 (X overflow), Flags = 0x48
    ps2_mouse::process_byte(0x48);
    ps2_mouse::process_byte(100);
    ps2_mouse::process_byte(100);

    // Deltas devem ser sumariamente descartados
    let (x, y, _) = ps2_mouse::get_state();
    assert_eq!(x, 512);
    assert_eq!(y, 384);
    assert_eq!(ps2_mouse::DISCARDED_OVERFLOW.load(Ordering::Relaxed), initial_overflow + 1);
}

#[test]
fn test_mouse_sync_bit_rejection_at_cycle0() {
    setup_mouse();
    let initial_invalid = ps2_mouse::MOUSE_INVALID_PACKETS.load(Ordering::Relaxed);

    // Byte no ciclo 0 sem bit 3 ativo (0x08) deve ser rejeitado imediatamente
    ps2_mouse::process_byte(0x00);
    assert_eq!(ps2_mouse::get_cycle(), 0);

    ps2_mouse::process_byte(0x07); // bits 0,1,2 ativos, mas bit 3 zero
    assert_eq!(ps2_mouse::get_cycle(), 0);

    ps2_mouse::process_byte(0x50);
    assert_eq!(ps2_mouse::get_cycle(), 0);

    assert_eq!(
        ps2_mouse::MOUSE_INVALID_PACKETS.load(Ordering::Relaxed),
        initial_invalid + 3
    );
}

#[test]
fn test_mouse_packet_desynchronization_and_resync() {
    setup_mouse();

    // Injeta sequência de bytes espúrios/órfãos
    ps2_mouse::process_byte(0x01);
    ps2_mouse::process_byte(0x02);
    assert_eq!(ps2_mouse::get_cycle(), 0);

    // Agora envia um pacote perfeitamente válido de 3 bytes: Flags=0x08, dx=10, dy=10
    ps2_mouse::process_byte(0x08);
    assert_eq!(ps2_mouse::get_cycle(), 1);
    ps2_mouse::process_byte(10);
    assert_eq!(ps2_mouse::get_cycle(), 2);
    ps2_mouse::process_byte(10);
    assert_eq!(ps2_mouse::get_cycle(), 0);

    let (x, y, _) = ps2_mouse::get_state();
    assert_eq!(x, 522);
    assert_eq!(y, 374);
}

#[test]
fn test_mouse_stale_packet_timeout_resync() {
    setup_mouse();

    // Inicia um pacote (cycle vira 1)
    ps2_mouse::process_byte(0x08);
    assert_eq!(ps2_mouse::get_cycle(), 1);

    // Simula passagem de tempo no timer do sistema (> 30 ticks ~ 300ms)
    for _ in 0..35 {
        timer::tick();
    }

    // Chegada do próximo byte: deve detectar timeout, resetar cycle para 0 e processar
    // Como 0x08 tem bit 3 = 1, ele é aceito como novo Byte 0!
    ps2_mouse::process_byte(0x08);
    assert_eq!(ps2_mouse::get_cycle(), 1);

    // Conclui o novo pacote
    ps2_mouse::process_byte(5);
    ps2_mouse::process_byte(5);
    assert_eq!(ps2_mouse::get_cycle(), 0);
}

#[test]
fn test_mouse_bounds_reconfiguration() {
    setup_mouse();

    // Reconfigura para 800x600
    ps2_mouse::set_bounds(800, 600);
    assert_eq!(ps2_mouse::MOUSE_MAX_X.load(Ordering::Relaxed), 800);
    assert_eq!(ps2_mouse::MOUSE_MAX_Y.load(Ordering::Relaxed), 600);

    // Coloca coordenadas acima de 800 e 600
    ps2_mouse::MOUSE_X.store(950, Ordering::Relaxed);
    ps2_mouse::MOUSE_Y.store(750, Ordering::Relaxed);
    ps2_mouse::set_bounds(800, 600);

    let (x, y, _) = ps2_mouse::get_state();
    assert_eq!(x, 799);
    assert_eq!(y, 599);
}

#[test]
fn test_mouse_telemetry_accuracy() {
    setup_mouse();

    let t0 = ps2_mouse::get_telemetry();
    ps2_mouse::process_byte(0x08);
    ps2_mouse::process_byte(1);
    ps2_mouse::process_byte(1);

    let t1 = ps2_mouse::get_telemetry();
    assert_eq!(t1.bytes_received, t0.bytes_received + 3);
    assert_eq!(t1.packets_completed, t0.packets_completed + 1);
}

#[test]
fn test_mouse_reset_sync_clears_cycle_and_buttons() {
    setup_mouse();

    // Inicia pacote e aperta botão
    ps2_mouse::process_byte(0x09); // Left click
    assert_eq!(ps2_mouse::get_cycle(), 1);

    ps2_mouse::reset_sync();
    assert_eq!(ps2_mouse::get_cycle(), 0);
    assert_eq!(ps2_mouse::MOUSE_BUTTONS.load(Ordering::Relaxed), 0);
}
