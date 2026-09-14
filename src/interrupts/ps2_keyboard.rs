/// PS/2 Keyboard Driver
///
/// Inicialização correta do controlador PS/2 i8042.
/// Usa polling direto (sem IRQ) da porta 0x60/0x64.
/// Scancode Set 1 (XT) - compatível com VirtualBox.

#[allow(dead_code)]

use crate::keyboard_buffer;
use crate::interrupts::pic;

const KBD_DATA: u16 = 0x60;  // PS/2 data port
const KBD_CMD:  u16 = 0x64;  // PS/2 command/status port

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

/// Aguarda o buffer de ENTRADA do controlador esvaziar (bit 1 = 0).
/// Deve ser chamado ANTES de escrever em 0x60 ou 0x64.
#[inline]
unsafe fn ps2_wait_write() {
    let mut timeout = 100_000usize;
    while timeout > 0 {
        let s = inb(KBD_CMD);
        if (s & 2) == 0 { return; }
        core::hint::spin_loop();
        timeout -= 1;
    }
}

/// Aguarda o buffer de SAÍDA ter dado (bit 0 = 1).
/// Deve ser chamado ANTES de ler de 0x60.
#[inline]
unsafe fn ps2_wait_read() {
    let mut timeout = 100_000usize;
    while timeout > 0 {
        let s = inb(KBD_CMD);
        if (s & 1) != 0 { return; }
        core::hint::spin_loop();
        timeout -= 1;
    }
}

/// US-QWERTY Scancode Set 1 → ASCII.
static SCANCODE_MAP_NORMAL: [u8; 88] = [
//  0      1      2      3      4      5      6      7      8      9      A      B      C      D      E      F
    0,     0x1B,  b'1',  b'2',  b'3',  b'4',  b'5',  b'6',  b'7',  b'8',  b'9',  b'0',  b'-',  b'=',  0x08,  b'\t', // 0x
    b'q',  b'w',  b'e',  b'r',  b't',  b'y',  b'u',  b'i',  b'o',  b'p',  b'[',  b']',  b'\n', 0,     b'a',  b's',  // 1x
    b'd',  b'f',  b'g',  b'h',  b'j',  b'k',  b'l',  b';',  b'\'', b'`',  0,     b'\\', b'z',  b'x',  b'c',  b'v',  // 2x
    b'b',  b'n',  b'm',  b',',  b'.',  b'/',  0,     b'*',  0,     b' ',  0,     0,     0,     0,     0,     0,     // 3x
    0,     0,     0,     0,     0,     0,     0,     b'7',  b'8',  b'9',  b'-',  b'4',  b'5',  b'6',  b'+',  b'1',  // 4x
    b'2',  b'3',  b'0',  b'.',  0,     0,     0,     0,                                                             // 5x
];

/// Shifted US-QWERTY Scancode Set 1 → ASCII.
static SCANCODE_MAP_SHIFT: [u8; 88] = [
    0,     0x1B,  b'!',  b'@',  b'#',  b'$',  b'%',  b'^',  b'&',  b'*',  b'(',  b')',  b'_',  b'+',  0x08,  b'\t',
    b'Q',  b'W',  b'E',  b'R',  b'T',  b'Y',  b'U',  b'I',  b'O',  b'P',  b'{',  b'}',  b'\n', 0,     b'A',  b'S',
    b'D',  b'F',  b'G',  b'H',  b'J',  b'K',  b'L',  b':',  b'"',  b'~',  0,     b'|',  b'Z',  b'X',  b'C',  b'V',
    b'B',  b'N',  b'M',  b'<',  b'>',  b'?',  0,     b'*',  0,     b' ',  0,     0,     0,     0,     0,     0,
    0,     0,     0,     0,     0,     0,     0,     b'7',  b'8',  b'9',  b'-',  b'4',  b'5',  b'6',  b'+',  b'1',
    b'2',  b'3',  b'0',  b'.',  0,     0,     0,     0,
];

static mut SHIFT_DOWN: bool = false;
static mut CAPS_LOCK:  bool = false;

/// Inicializa o controlador PS/2 i8042 e o teclado.
///
/// Sequência completa compatível com VirtualBox/QEMU:
/// 1. Desabilita portas PS/2
/// 2. Limpa buffer de saída
/// 3. Lê e modifica byte de configuração
/// 4. Habilita porta 1
/// 5. Reseta o teclado (0xFF) e aguarda BAT
/// 6. Habilita scanning (0xF4)
pub fn init_and_flush() {
    unsafe {
        // Passo 1: Desabilita porta 1 (impede interferência durante init)
        ps2_wait_write();
        outb(KBD_CMD, 0xAD); // Disable port 1

        // Passo 2: Flush do buffer de saída (descarta dados velhos)
        for _ in 0..16 {
            let s = inb(KBD_CMD);
            if (s & 1) == 0 { break; }
            let _ = inb(KBD_DATA);
        }

        // Passo 3: Lê o byte de configuração do controlador
        ps2_wait_write();
        outb(KBD_CMD, 0x20); // "Read config byte"
        ps2_wait_read();
        let mut cfg = inb(KBD_DATA);

        // Modifica: habilita porta 1, desabilita tradução (já em Set 1)
        cfg &= !(1 << 4); // Bit 4 = Port 1 clock disable → clear para HABILITAR
        cfg &= !(1 << 6); // Bit 6 = Translation enable → clear
        cfg |=  (1 << 0); // Bit 0 = Port 1 IRQ enable (para IRQ futuro)

        // Passo 4: Escreve o byte de configuração de volta
        ps2_wait_write();
        outb(KBD_CMD, 0x60); // "Write config byte"
        ps2_wait_write();
        outb(KBD_DATA, cfg);

        // Passo 5: Habilita porta 1 explicitamente
        ps2_wait_write();
        outb(KBD_CMD, 0xAE); // Enable port 1

        // Passo 6: Reseta o teclado
        ps2_wait_write();
        outb(KBD_DATA, 0xFF); // Reset keyboard

        // Aguarda ACK (0xFA) do reset
        ps2_wait_read();
        let _ack = inb(KBD_DATA); // deve ser 0xFA

        // Aguarda resultado do BAT (0xAA = passou, 0xFC = falhou)
        ps2_wait_read();
        let _bat = inb(KBD_DATA); // deve ser 0xAA

        // Passo 7: Habilita scanning do teclado
        ps2_wait_write();
        outb(KBD_DATA, 0xF4); // Enable scanning

        // Aguarda ACK do 0xF4
        ps2_wait_read();
        let _ack2 = inb(KBD_DATA); // deve ser 0xFA
    }
}

/// Converte scancode PS/2 Set 1 para ASCII e empurra para o buffer.
pub fn process_scancode(sc: u8) {
    unsafe {
        let is_release = (sc & 0x80) != 0;
        let make_code  = sc & 0x7F;

        match make_code {
            0x2A | 0x36 => { SHIFT_DOWN = !is_release; } // Shift L/R
            0x3A if !is_release => { CAPS_LOCK = !CAPS_LOCK; }
            code if !is_release && (code as usize) < SCANCODE_MAP_NORMAL.len() => {
                let shifted = SHIFT_DOWN ^ (CAPS_LOCK && code >= 0x10 && code <= 0x32);
                let ch = if shifted {
                    SCANCODE_MAP_SHIFT[code as usize]
                } else {
                    SCANCODE_MAP_NORMAL[code as usize]
                };
                if ch != 0 {
                    keyboard_buffer::push(ch);
                }
            }
            _ => {}
        }
    }
}

/// IRQ1 handler (vetor 33) — usado se IRQ1 for habilitado no PIC.
#[no_mangle]
pub extern "C" fn kbd_irq_handler() {
    unsafe {
        let sc = inb(KBD_DATA);
        pic::send_eoi(1);
        process_scancode(sc);
    }
}
