/// Driver de Mouse PS/2 (i8042 Aux Device)
///
/// Implementa a comunicação com o mouse PS/2 padrão conectado à segunda
/// porta do controlador i8042. Trata pacotes de 3 bytes (Flags, Delta X, Delta Y)
/// via interrupção IRQ12 (vetor 44).

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
    let mut timeout = 100_000usize;
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
    let mut timeout = 100_000usize;
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
    inb(PS2_DATA)
}

/// Estado do mouse mantido pelo driver
pub struct MouseState {
    pub x: usize,
    pub y: usize,
    pub buttons: u8, // bit 0 = Left, bit 1 = Right, bit 2 = Middle
    pub max_x: usize,
    pub max_y: usize,
    cycle: u8,
    packet: [u8; 3],
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

        // 2. Habilita a segunda porta PS/2 (porta auxiliar)
        wait_write();
        outb(PS2_CMD, 0xA8);

        // 3. Lê o byte de configuração atual do controlador
        wait_write();
        outb(PS2_CMD, 0x20);
        wait_read();
        let mut status = inb(PS2_DATA);

        // Habilita interrupção da porta 2 (bit 1) e garante clock ativo (bit 5 limpo)
        status |= 1 << 1;
        status &= !(1 << 5);

        // 4. Grava de volta o byte de configuração
        wait_write();
        outb(PS2_CMD, 0x60);
        wait_write();
        outb(PS2_DATA, status);

        // 5. Envia comando para carregar parâmetros padrão do mouse (0xF6)
        mouse_write(0xF6);
        let _ = mouse_read(); // Descarta ACK (0xFA)

        // 6. Habilita streaming de pacotes de dados (0xF4)
        mouse_write(0xF4);
        let _ = mouse_read(); // Descarta ACK (0xFA)
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

        let mouse = &mut *core::ptr::addr_of_mut!(MOUSE);
        let b = inb(PS2_DATA);
        pic::send_eoi(12);

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

                // Atualiza a renderização do cursor na tela
                crate::gui::update_cursor(mouse.x, mouse.y);
            }
            _ => {
                mouse.cycle = 0;
            }
        }
    }
}
