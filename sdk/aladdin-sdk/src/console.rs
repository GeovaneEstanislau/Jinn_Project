//! Aladdin Desktop SDK — Impressão e Depuração no Terminal

use crate::syscall::{syscall3, SYS_WRITE};

/// Imprime uma sequência de caracteres UTF-8 no terminal via sys_write (fd 1).
pub fn print(s: &str) {
    let bytes = s.as_bytes();
    unsafe {
        syscall3(SYS_WRITE, 1, bytes.as_ptr() as u64, bytes.len() as u64);
    }
}

/// Imprime uma sequência de caracteres UTF-8 seguida por quebra de linha `\n`.
pub fn println(s: &str) {
    print(s);
    print("\n");
}

/// Imprime um número inteiro sem sinal em formato decimal.
pub fn print_num(mut n: u64) {
    if n == 0 {
        print("0");
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 0;
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    buf[..i].reverse();
    if let Ok(s) = core::str::from_utf8(&buf[..i]) {
        print(s);
    }
}

/// Imprime um número inteiro em formato hexadecimal (ex: 0x000000001234ABCD).
pub fn print_hex(n: u64) {
    print("0x");
    let hex_chars = b"0123456789ABCDEF";
    let mut buf = [0u8; 16];
    for i in 0..16 {
        let shift = (15 - i) * 4;
        buf[i] = hex_chars[((n >> shift) & 0xF) as usize];
    }
    if let Ok(s) = core::str::from_utf8(&buf) {
        print(s);
    }
}
