/// Driver de Mouse PS/2 (i8042 Aux Device)
///
/// Implementa a comunicação com o mouse PS/2 padrão conectado à segunda
/// porta do controlador i8042. Suporta tanto recepção assíncrona via IRQ12
/// quanto polling direto em laço de renderização, garantindo operação contínua
/// com telemetria rigorosa, proteção contra desfasamento de pacotes e sincronização atômica.

use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};
use crate::interrupts::pic;

const PS2_DATA: u16 = 0x60;
const PS2_CMD:  u16 = 0x64;

// ── Telemetria do Subsistema PS/2 Mouse ────────────────────────────────────────

pub static IRQ12_COUNT: AtomicU64 = AtomicU64::new(0);
pub static MOUSE_BYTES_RECEIVED: AtomicU64 = AtomicU64::new(0);
pub static MOUSE_PACKETS_COMPLETED: AtomicU64 = AtomicU64::new(0);
pub static MOUSE_INVALID_PACKETS: AtomicU64 = AtomicU64::new(0);
pub static DISCARDED_OVERFLOW: AtomicU64 = AtomicU64::new(0);
pub static MOUSE_EVENTS_DELIVERED: AtomicU64 = AtomicU64::new(0);

// ── Estado Atômico e Lock-Free do Mouse ───────────────────────────────────────

pub static MOUSE_X: AtomicUsize = AtomicUsize::new(512);
pub static MOUSE_Y: AtomicUsize = AtomicUsize::new(384);
pub static MOUSE_BUTTONS: AtomicU8 = AtomicU8::new(0); // bit 0=Left, bit 1=Right, bit 2=Middle
pub static MOUSE_MAX_X: AtomicUsize = AtomicUsize::new(1024);
pub static MOUSE_MAX_Y: AtomicUsize = AtomicUsize::new(768);
pub static MOUSE_CYCLE: AtomicU8 = AtomicU8::new(0);
pub static MOUSE_PACKET: [AtomicU8; 3] = [AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0)];
pub static LAST_BYTE_TICK: AtomicU64 = AtomicU64::new(0);
pub static HARDWARE_INITIALIZED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseTelemetry {
    pub irq12_count: u64,
    pub bytes_received: u64,
    pub packets_completed: u64,
    pub invalid_packets: u64,
    pub discarded_overflow: u64,
    pub events_delivered: u64,
}

pub fn get_telemetry() -> MouseTelemetry {
    MouseTelemetry {
        irq12_count: IRQ12_COUNT.load(Ordering::Relaxed),
        bytes_received: MOUSE_BYTES_RECEIVED.load(Ordering::Relaxed),
        packets_completed: MOUSE_PACKETS_COMPLETED.load(Ordering::Relaxed),
        invalid_packets: MOUSE_INVALID_PACKETS.load(Ordering::Relaxed),
        discarded_overflow: DISCARDED_OVERFLOW.load(Ordering::Relaxed),
        events_delivered: MOUSE_EVENTS_DELIVERED.load(Ordering::Relaxed),
    }
}

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    core::arch::asm!("in al, dx", out("al") val, in("dx") port, options(nomem, nostack));
    val
}

#[inline]
unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack));
}

/// Aguarda o buffer de entrada do i8042 esvaziar para poder escrever.
unsafe fn wait_write() {
    let mut timeout = 500_000usize;
    while timeout > 0 {
        if (inb(PS2_CMD) & 2) == 0 {
            return;
        }
        core::hint::spin_loop();
        timeout -= 1;
    }
}

/// Aguarda o buffer de saída ter dados para leitura.
unsafe fn wait_read() {
    let mut timeout = 500_000usize;
    while timeout > 0 {
        if (inb(PS2_CMD) & 1) != 0 {
            return;
        }
        core::hint::spin_loop();
        timeout -= 1;
    }
}

/// Envia um comando especificamente direcionado ao dispositivo da porta 2 (Mouse).
unsafe fn mouse_write(cmd: u8) {
    wait_write();
    outb(PS2_CMD, 0xD4); // Próximo byte vai para a porta auxiliar (mouse)
    wait_write();
    outb(PS2_DATA, cmd);
}

/// Lê uma resposta do mouse.
unsafe fn mouse_read() -> u8 {
    wait_read();
    if (inb(PS2_CMD) & 1) != 0 {
        inb(PS2_DATA)
    } else {
        0
    }
}

/// Define os limites da tela (resolução do framebuffer)
pub fn set_bounds(max_x: usize, max_y: usize) {
    if max_x == 0 || max_y == 0 {
        return;
    }
    MOUSE_MAX_X.store(max_x, Ordering::Relaxed);
    MOUSE_MAX_Y.store(max_y, Ordering::Relaxed);

    let cur_x = MOUSE_X.load(Ordering::Relaxed);
    let cur_y = MOUSE_Y.load(Ordering::Relaxed);
    if cur_x >= max_x {
        MOUSE_X.store(max_x.saturating_sub(1), Ordering::Release);
    }
    if cur_y >= max_y {
        MOUSE_Y.store(max_y.saturating_sub(1), Ordering::Release);
    }
}

/// Processa um byte individual de dados do mouse na máquina de estados de 3 bytes (RFC / PS2 Spec).
pub fn process_byte(b: u8) {
    MOUSE_BYTES_RECEIVED.fetch_add(1, Ordering::Relaxed);

    // Timeout de sincronização de pacote: se transcorreram > 30 ticks (300ms) entre bytes,
    // o pacote foi interrompido/perdido no canal serial. Reseta o ciclo para evitar desfasamento.
    let now = crate::timer::ticks();
    let last = LAST_BYTE_TICK.swap(now, Ordering::Relaxed);
    if now.saturating_sub(last) > 30 && MOUSE_CYCLE.load(Ordering::Relaxed) != 0 {
        MOUSE_CYCLE.store(0, Ordering::Relaxed);
        MOUSE_INVALID_PACKETS.fetch_add(1, Ordering::Relaxed);
    }

    let cycle = MOUSE_CYCLE.load(Ordering::Relaxed);
    match cycle {
        0 => {
            // Byte 0: Flags. Bit 3 deve ser obrigatoriamente 1 em qualquer pacote PS/2 padrão.
            if (b & 0x08) != 0 {
                MOUSE_PACKET[0].store(b, Ordering::Relaxed);
                MOUSE_CYCLE.store(1, Ordering::Relaxed);
            } else {
                // Descarta byte órfão que não possui o bit de sincronismo
                MOUSE_INVALID_PACKETS.fetch_add(1, Ordering::Relaxed);
            }
        }
        1 => {
            // Byte 1: Delta X
            MOUSE_PACKET[1].store(b, Ordering::Relaxed);
            MOUSE_CYCLE.store(2, Ordering::Relaxed);
        }
        2 => {
            // Byte 2: Delta Y
            MOUSE_PACKET[2].store(b, Ordering::Relaxed);
            MOUSE_CYCLE.store(0, Ordering::Relaxed);

            let flags = MOUSE_PACKET[0].load(Ordering::Relaxed);
            let raw_x = MOUSE_PACKET[1].load(Ordering::Relaxed);
            let raw_y = b;

            // 1. Descarta pacotes com overflow (bits 6 e 7)
            if (flags & 0xC0) != 0 {
                DISCARDED_OVERFLOW.fetch_add(1, Ordering::Relaxed);
                return;
            }

            // 2. Decodificação de deltas com sinal 9-bit conforme especificação PS/2
            // Bit 4 = sinal de X, Bit 5 = sinal de Y
            let delta_x: i32 = if (flags & 0x10) != 0 {
                (raw_x as i32) - 256
            } else {
                raw_x as i32
            };

            let delta_y: i32 = if (flags & 0x20) != 0 {
                (raw_y as i32) - 256
            } else {
                raw_y as i32
            };

            let max_x = MOUSE_MAX_X.load(Ordering::Relaxed);
            let max_y = MOUSE_MAX_Y.load(Ordering::Relaxed);

            let cur_x = MOUSE_X.load(Ordering::Relaxed) as i32;
            let cur_y = MOUSE_Y.load(Ordering::Relaxed) as i32;

            // Atualiza posição X (limitada a [0, max_x - 1])
            let new_x = (cur_x + delta_x).clamp(0, max_x.saturating_sub(1) as i32);
            MOUSE_X.store(new_x as usize, Ordering::Release);

            // Atualiza posição Y (No protocolo PS/2, delta_y positivo é para cima; na tela Y=0 é o topo)
            let new_y = (cur_y - delta_y).clamp(0, max_y.saturating_sub(1) as i32);
            MOUSE_Y.store(new_y as usize, Ordering::Release);

            // Botões: bit 0 = Left, bit 1 = Right, bit 2 = Middle
            let buttons = flags & 0x07;
            MOUSE_BUTTONS.store(buttons, Ordering::Release);

            MOUSE_PACKETS_COMPLETED.fetch_add(1, Ordering::Relaxed);
        }
        _ => {
            MOUSE_CYCLE.store(0, Ordering::Relaxed);
        }
    }
}

/// Reseta a máquina de estados de sincronização de pacotes do mouse (3 bytes) e libera botões presos
pub fn reset_sync() {
    MOUSE_CYCLE.store(0, Ordering::Relaxed);
    MOUSE_PACKET[0].store(0, Ordering::Relaxed);
    MOUSE_PACKET[1].store(0, Ordering::Relaxed);
    MOUSE_PACKET[2].store(0, Ordering::Relaxed);
    MOUSE_BUTTONS.store(0, Ordering::Relaxed);
}

pub fn get_cycle() -> u8 {
    MOUSE_CYCLE.load(Ordering::Relaxed)
}

pub fn set_cycle(cycle: u8) {
    MOUSE_CYCLE.store(cycle, Ordering::Relaxed);
}

/// Atualiza o estado do cursor e botões a partir de comandos normalizados do driver em Ring 3
pub fn update_from_driver(kind: u16, code: u16, x: usize, y: usize) {
    let max_x = MOUSE_MAX_X.load(Ordering::Relaxed);
    let max_y = MOUSE_MAX_Y.load(Ordering::Relaxed);

    MOUSE_X.store(x.min(max_x.saturating_sub(1)), Ordering::Release);
    MOUSE_Y.store(y.min(max_y.saturating_sub(1)), Ordering::Release);

    match kind {
        crate::drivers::device_manager::INPUT_KIND_MOUSE_BUTTON_DOWN => {
            let cur = MOUSE_BUTTONS.load(Ordering::Relaxed);
            MOUSE_BUTTONS.store(cur | (code as u8), Ordering::Release);
        }
        crate::drivers::device_manager::INPUT_KIND_MOUSE_BUTTON_UP => {
            let cur = MOUSE_BUTTONS.load(Ordering::Relaxed);
            MOUSE_BUTTONS.store(cur & !(code as u8), Ordering::Release);
        }
        _ => {}
    }
}

/// Verifica estado temporal do mouse e recupera máquina de estados se estagnada.
/// A ingestão da porta 0x60 é tratada de forma estrita e sem contenção pelo ISR (IRQ12).
pub fn poll() {
    let now = crate::timer::ticks();
    let last = LAST_BYTE_TICK.load(Ordering::Relaxed);
    if now.saturating_sub(last) > 30 && MOUSE_CYCLE.load(Ordering::Relaxed) != 0 {
        reset_sync();
    }
}

/// Inicializa a segunda porta do controlador PS/2 e configura o streaming do mouse.
/// É idempotente e temporariamente mascara IRQ12 durante comandos para prevenir que ACKs
/// desfasem a máquina de estados de 3 bytes.
pub fn init() {
    unsafe {
        // 1. Define resolução máxima a partir do framebuffer se disponível
        if let Some(fb) = crate::limine::framebuffer() {
            set_bounds(fb.width as usize, fb.height as usize);
            MOUSE_X.store((fb.width as usize) / 2, Ordering::Release);
            MOUSE_Y.store((fb.height as usize) / 2, Ordering::Release);
        }

        // Se o hardware já foi inicializado anteriormente pelo kernel boot, apenas reseta sincronização
        if HARDWARE_INITIALIZED.load(Ordering::Acquire) {
            reset_sync();
            return;
        }

        // 2. Mascara temporariamente IRQ12 no PIC para evitar que ACKs de comandos vazem para o IRQ handler
        crate::interrupts::pic::mask_irq(12);

        // 3. Drena quaisquer dados residuais no buffer de saída do controlador
        let mut drain = 1000;
        while (inb(PS2_CMD) & 1) != 0 && drain > 0 {
            let _ = inb(PS2_DATA);
            drain -= 1;
        }

        // 4. Habilita a segunda porta PS/2 (porta auxiliar)
        wait_write();
        outb(PS2_CMD, 0xA8);

        // 5. Lê e configura o byte de comando (CCB) do controlador
        wait_write();
        outb(PS2_CMD, 0x20);
        wait_read();
        let mut status = inb(PS2_DATA);

        // Habilita interrupção da porta 2 (bit 1) e garante clock ativo (bit 5 limpo)
        status |= 1 << 1;
        status &= !(1 << 5);

        wait_write();
        outb(PS2_CMD, 0x60);
        wait_write();
        outb(PS2_DATA, status);

        // 6. Envia comando para carregar parâmetros padrão do mouse (0xF6) e descarta ACK de hardware
        mouse_write(0xF6);
        let ack1 = mouse_read();
        crate::serial_write(b"[PS2-MOUSE] Init ACK 0xF6: ");
        crate::serial_write_byte(if ack1 == 0xFA { b'Y' } else { b'N' });
        crate::serial_write(b"\n");

        // 7. Habilita streaming de pacotes de dados (0xF4) e descarta ACK de hardware
        mouse_write(0xF4);
        let ack2 = mouse_read();
        crate::serial_write(b"[PS2-MOUSE] Init ACK 0xF4: ");
        crate::serial_write_byte(if ack2 == 0xFA { b'Y' } else { b'N' });
        crate::serial_write(b"\n");

        // 8. Drena quaisquer bytes espúrios residuais antes de abrir a máquina de estados
        let mut drain2 = 100;
        while (inb(PS2_CMD) & 1) != 0 && drain2 > 0 {
            let _ = inb(PS2_DATA);
            drain2 -= 1;
        }

        reset_sync();
        HARDWARE_INITIALIZED.store(true, Ordering::Release);

        // 9. Desmascara IRQ12 no PIC para recepção assíncrona (e cascata IRQ2)
        crate::interrupts::pic::unmask_irq(12);
    }
}

/// Retorna a posição atual do mouse e o estado dos botões `(x, y, buttons)`.
pub fn get_state() -> (usize, usize, u8) {
    (
        MOUSE_X.load(Ordering::Acquire),
        MOUSE_Y.load(Ordering::Acquire),
        MOUSE_BUTTONS.load(Ordering::Acquire),
    )
}

/// Handler de interrupção IRQ12 (Vetor 44 da IDT).
#[no_mangle]
pub extern "C" fn mouse_irq_handler() {
    IRQ12_COUNT.fetch_add(1, Ordering::Relaxed);

    unsafe {
        for _ in 0..16 {
            let status = inb(PS2_CMD);
            if (status & 1) == 0 {
                break;
            }
            let b = inb(PS2_DATA);
            // Verifica o bit AUX (bit 5): se 1, pertence ao mouse; se 0, pertence ao teclado.
            if (status & 0x20) != 0 {
                if !crate::drivers::device_manager::post_raw_hardware_byte(crate::drivers::device_manager::DEV_MOUSE0, 12, b) {
                    process_byte(b);
                }
            } else {
                if !crate::drivers::device_manager::post_raw_hardware_byte(crate::drivers::device_manager::DEV_KBD0, 1, b) {
                    crate::interrupts::ps2_keyboard::process_scancode(b);
                }
            }
        }
        pic::send_eoi(12);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ps2_mouse_packet_decoding() {
        MOUSE_X.store(500, Ordering::Relaxed);
        MOUSE_Y.store(400, Ordering::Relaxed);
        MOUSE_MAX_X.store(1024, Ordering::Relaxed);
        MOUSE_MAX_Y.store(768, Ordering::Relaxed);
        MOUSE_CYCLE.store(0, Ordering::Relaxed);
        MOUSE_BUTTONS.store(0, Ordering::Relaxed);

        // Envia pacote válido: Flags (0x09: bit 3 + bit 0 left button), DeltaX = 10, DeltaY = 5
        process_byte(0x09);
        assert_eq!(MOUSE_CYCLE.load(Ordering::Relaxed), 1);
        process_byte(10);
        assert_eq!(MOUSE_CYCLE.load(Ordering::Relaxed), 2);
        process_byte(5);
        assert_eq!(MOUSE_CYCLE.load(Ordering::Relaxed), 0);

        assert_eq!(MOUSE_X.load(Ordering::Relaxed), 510);
        assert_eq!(MOUSE_Y.load(Ordering::Relaxed), 395); // Delta Y positivo move para cima no PS/2
        assert_eq!(MOUSE_BUTTONS.load(Ordering::Relaxed), 1); // Left click ativo
    }
}
