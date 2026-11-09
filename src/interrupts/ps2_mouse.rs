/// Driver de Mouse PS/2 (i8042 Aux Device)
///
/// Implementa a comunicação com o mouse PS/2 padrão conectado à segunda
/// porta do controlador i8042. Suporta tanto recepção assíncrona via IRQ12
/// quanto polling direto em laço de renderização, garantindo operação contínua
/// mesmo quando o I/O APIC está ativado ou interrupções legadas do PIC estão mascaradas.

use crate::interrupts::pic;

const PS2_DATA: u16 = 0x60;
const PS2_CMD:  u16 = 0x64;

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

/// Estado do mouse mantido pelo driver
pub struct MouseState {
    pub x: usize,
    pub y: usize,
    pub buttons: u8, // bit 0 = Left, bit 1 = Right, bit 2 = Middle
    pub max_x: usize,
    pub max_y: usize,
    pub cycle: u8,
    pub packet: [u8; 3],
}

static mut MOUSE: MouseState = MouseState {
    x: 1024 / 2,
    y: 768 / 2,
    buttons: 0,
    max_x: 1024,
    max_y: 768,
    cycle: 0,
    packet: [0; 3],
};

/// Processa um byte individual de dados do mouse na máquina de estados de 3 bytes.
pub fn process_byte(b: u8) {
    unsafe {
        let mouse = &mut *core::ptr::addr_of_mut!(MOUSE);
        match mouse.cycle {
            0 => {
                // Byte 0: Flags. Bit 3 deve ser sempre 1 para um pacote válido.
                if (b & 0x08) != 0 {
                    mouse.packet[0] = b;
                    mouse.cycle = 1;
                }
            }
            1 => {
                // Byte 1: Delta X
                mouse.packet[1] = b;
                mouse.cycle = 2;
            }
            2 => {
                // Byte 2: Delta Y
                mouse.packet[2] = b;
                mouse.cycle = 0;

                let flags = mouse.packet[0];
                let raw_x = mouse.packet[1];
                let raw_y = mouse.packet[2];

                // Descarta pacotes com overflow
                if (flags & 0xC0) != 0 {
                    return;
                }

                // Decodifica deltas com seus bits de sinal (bit 4 = sinal X, bit 5 = sinal Y)
                let delta_x = if (flags & 0x10) != 0 {
                    (raw_x as i16) - 256
                } else {
                    raw_x as i16
                };

                let delta_y = if (flags & 0x20) != 0 {
                    (raw_y as i16) - 256
                } else {
                    raw_y as i16
                };

                // Atualiza posição X
                let new_x = (mouse.x as i32) + (delta_x as i32);
                mouse.x = if new_x < 0 {
                    0
                } else if (new_x as usize) >= mouse.max_x {
                    mouse.max_x.saturating_sub(1)
                } else {
                    new_x as usize
                };

                // Atualiza posição Y (No protocolo PS/2, delta_y positivo é para cima)
                let new_y = (mouse.y as i32) - (delta_y as i32);
                mouse.y = if new_y < 0 {
                    0
                } else if (new_y as usize) >= mouse.max_y {
                    mouse.max_y.saturating_sub(1)
                } else {
                    new_y as usize
                };

                // Botões: bit 0 = Left, bit 1 = Right, bit 2 = Middle
                mouse.buttons = flags & 0x07;
            }
            _ => {
                mouse.cycle = 0;
            }
        }
    }
}

/// Reseta a máquina de estados de sincronização de pacotes do mouse (3 bytes) e libera botões presos
pub fn reset_sync() {
    unsafe {
        let mouse = &mut *core::ptr::addr_of_mut!(MOUSE);
        mouse.cycle = 0;
        mouse.packet = [0; 3];
        mouse.buttons = 0;
    }
}

pub fn get_cycle() -> u8 {
    unsafe { MOUSE.cycle }
}

pub fn set_cycle(cycle: u8) {
    unsafe { MOUSE.cycle = cycle; }
}

/// Atualiza o estado do cursor e botões a partir de comandos normalizados do driver em Ring 3
pub fn update_from_driver(kind: u16, code: u16, x: usize, y: usize) {
    unsafe {
        let mouse = &mut *core::ptr::addr_of_mut!(MOUSE);
        mouse.x = x.min(mouse.max_x.saturating_sub(1));
        mouse.y = y.min(mouse.max_y.saturating_sub(1));
        match kind {
            crate::drivers::device_manager::INPUT_KIND_MOUSE_BUTTON_DOWN => {
                mouse.buttons |= code as u8;
            }
            crate::drivers::device_manager::INPUT_KIND_MOUSE_BUTTON_UP => {
                mouse.buttons &= !(code as u8);
            }
            _ => {}
        }
    }
}

/// Faz polling direto no controlador i8042 processando bytes da porta auxiliar (mouse).
/// Garante funcionamento perfeito mesmo quando o I/O APIC está ativo ou IRQ12 não é entregue.
pub fn poll() {
    unsafe {
        for _ in 0..16 {
            let status = inb(PS2_CMD);
            // Buffer de saída vazio
            if (status & 1) == 0 {
                break;
            }
            // Se o bit 5 for 0, o byte pertence ao teclado! Não consome para não roubar tecla.
            if (status & 0x20) == 0 {
                break;
            }
            // Bit 5 = 1: O byte pertence ao mouse
            let b = inb(PS2_DATA);
            if !crate::drivers::device_manager::post_raw_hardware_byte(crate::drivers::device_manager::DEV_MOUSE0, 12, b) {
                process_byte(b);
            }
        }
    }
}

/// Inicializa a segunda porta do controlador PS/2 e configura o mouse.
pub fn init() {
    unsafe {
        // 1. Define resolução máxima a partir do framebuffer do Limine, se disponível
        if let Some(fb) = crate::limine::framebuffer() {
            MOUSE.max_x = fb.width as usize;
            MOUSE.max_y = fb.height as usize;
            MOUSE.x = MOUSE.max_x / 2;
            MOUSE.y = MOUSE.max_y / 2;
        }

        MOUSE.cycle = 0;

        // 2. Drena quaisquer dados residuais no buffer de saída do controlador
        let mut drain = 1000;
        while (inb(PS2_CMD) & 1) != 0 && drain > 0 {
            let _ = inb(PS2_DATA);
            drain -= 1;
        }

        // 3. Habilita a segunda porta PS/2 (porta auxiliar)
        wait_write();
        outb(PS2_CMD, 0xA8);

        // 4. Lê o byte de configuração atual do controlador
        wait_write();
        outb(PS2_CMD, 0x20);
        wait_read();
        let mut status = inb(PS2_DATA);

        // Habilita interrupção da porta 2 (bit 1) e garante clock ativo (bit 5 limpo)
        status |= 1 << 1;
        status &= !(1 << 5);

        // 5. Grava de volta o byte de configuração
        wait_write();
        outb(PS2_CMD, 0x60);
        wait_write();
        outb(PS2_DATA, status);

        // 6. Envia comando para carregar parâmetros padrão do mouse (0xF6)
        mouse_write(0xF6);
        let _ = mouse_read(); // Descarta ACK (0xFA)

        // 7. Habilita streaming de pacotes de dados (0xF4)
        mouse_write(0xF4);
        let _ = mouse_read(); // Descarta ACK (0xFA)
        // 8. Desmascara IRQ12 no PIC para recepcao assincrona dual (interrupcao + polling)
        crate::interrupts::pic::unmask_irq(12);
    }
}

/// Retorna a posição atual do mouse e o estado dos botões `(x, y, buttons)`.
pub fn get_state() -> (usize, usize, u8) {
    unsafe {
        let mouse = &*core::ptr::addr_of!(MOUSE);
        (mouse.x, mouse.y, mouse.buttons)
    }
}

/// Handler de interrupção IRQ12 (Vetor 44 da IDT).
#[no_mangle]
pub extern "C" fn mouse_irq_handler() {
    unsafe {
        let status = inb(PS2_CMD);
        if (status & 1) == 0 {
            pic::send_eoi(12);
            return;
        }

        // Processa apenas se o dado veio da porta auxiliar (mouse)
        if (status & 0x20) != 0 {
            let b = inb(PS2_DATA);
            if !crate::drivers::device_manager::post_raw_hardware_byte(crate::drivers::device_manager::DEV_MOUSE0, 12, b) {
                process_byte(b);
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
        unsafe {
            MOUSE.x = 500;
            MOUSE.y = 400;
            MOUSE.max_x = 1024;
            MOUSE.max_y = 768;
            MOUSE.cycle = 0;
            MOUSE.buttons = 0;

            // Envia pacote válido: Flags (0x09: bit 3 + bit 0 left button), DeltaX = 10, DeltaY = 5
            process_byte(0x09);
            assert_eq!(MOUSE.cycle, 1);
            process_byte(10);
            assert_eq!(MOUSE.cycle, 2);
            process_byte(5);
            assert_eq!(MOUSE.cycle, 0);

            assert_eq!(MOUSE.x, 510);
            assert_eq!(MOUSE.y, 395); // Delta Y positivo move para cima no PS/2
            assert_eq!(MOUSE.buttons, 1); // Left click ativo
        }
    }
}
