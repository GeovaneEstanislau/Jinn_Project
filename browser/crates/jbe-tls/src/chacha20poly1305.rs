//! Pure Rust implementation of ChaCha20, Poly1305, and ChaCha20-Poly1305 AEAD (RFC 8439)

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

#[inline(always)]
fn qr(st: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    st[a] = st[a].wrapping_add(st[b]); st[d] ^= st[a]; st[d] = st[d].rotate_left(16);
    st[c] = st[c].wrapping_add(st[d]); st[b] ^= st[c]; st[b] = st[b].rotate_left(12);
    st[a] = st[a].wrapping_add(st[b]); st[d] ^= st[a]; st[d] = st[d].rotate_left(8);
    st[c] = st[c].wrapping_add(st[d]); st[b] ^= st[c]; st[b] = st[b].rotate_left(7);
}

pub struct ChaCha20 {
    state: [u32; 16],
}

impl ChaCha20 {
    pub fn new(key: &[u8; 32], nonce: &[u8; 12], counter: u32) -> Self {
        let mut state = [0u32; 16];
        state[0] = 0x61707865;
        state[1] = 0x3320646e;
        state[2] = 0x79622d32;
        state[3] = 0x6b206574;

        for i in 0..8 {
            state[4 + i] = u32::from_le_bytes([
                key[i * 4],
                key[i * 4 + 1],
                key[i * 4 + 2],
                key[i * 4 + 3],
            ]);
        }

        state[12] = counter;
        for i in 0..3 {
            state[13 + i] = u32::from_le_bytes([
                nonce[i * 4],
                nonce[i * 4 + 1],
                nonce[i * 4 + 2],
                nonce[i * 4 + 3],
            ]);
        }

        Self { state }
    }

    pub fn block(&mut self, out: &mut [u8; 64]) {
        let mut working = self.state;

        for _ in 0..10 {
            qr(&mut working, 0, 4, 8, 12);
            qr(&mut working, 1, 5, 9, 13);
            qr(&mut working, 2, 6, 10, 14);
            qr(&mut working, 3, 7, 11, 15);
            qr(&mut working, 0, 5, 10, 15);
            qr(&mut working, 1, 6, 11, 12);
            qr(&mut working, 2, 7, 8, 13);
            qr(&mut working, 3, 4, 9, 14);
        }

        for i in 0..16 {
            let val = working[i].wrapping_add(self.state[i]);
            out[i * 4..i * 4 + 4].copy_from_slice(&val.to_le_bytes());
        }

        self.state[12] = self.state[12].wrapping_add(1);
    }

    pub fn apply_keystream(&mut self, buffer: &mut [u8]) {
        let mut block = [0u8; 64];
        let mut offset = 0;

        while offset < buffer.len() {
            self.block(&mut block);
            let chunk = (buffer.len() - offset).min(64);
            for i in 0..chunk {
                buffer[offset + i] ^= block[i];
            }
            offset += chunk;
        }
    }
}

pub struct Poly1305 {
    r0: u64, r1: u64, r2: u64,
    h0: u64, h1: u64, h2: u64,
    pad: [u8; 16],
    buffer: [u8; 16],
    buf_len: usize,
}

impl Poly1305 {
    pub fn new(key: &[u8; 32]) -> Self {
        let r0 = u64::from_le_bytes([key[0], key[1], key[2], key[3], key[4], key[5], 0, 0]) & 0x0ffffffc0fffffff;
        let r1 = (u64::from_le_bytes([key[3], key[4], key[5], key[6], key[7], key[8], key[9], key[10]]) >> 6) & 0x0ffffffc0ffffffc;
        let r2 = (u64::from_le_bytes([key[9], key[10], key[11], key[12], key[13], key[14], key[15], 0]) >> 12) & 0x000000000ffffffc;

        let mut pad = [0u8; 16];
        pad.copy_from_slice(&key[16..32]);

        Self {
            r0, r1, r2,
            h0: 0, h1: 0, h2: 0,
            pad,
            buffer: [0u8; 16],
            buf_len: 0,
        }
    }

    fn process_block(&mut self, block: &[u8], is_final: bool) {
        let mut b = [0u8; 17];
        b[..block.len()].copy_from_slice(block);
        if is_final {
            b[block.len()] = 1;
        } else {
            b[16] = 1;
        }

        let m0 = u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], 0, 0]) & 0x3ffffffffffff;
        let m1 = (u64::from_le_bytes([b[5], b[6], b[7], b[8], b[9], b[10], b[11], 0]) >> 2) & 0x3ffffffffffff;
        let m2 = (u64::from_le_bytes([b[11], b[12], b[13], b[14], b[15], b[16], 0, 0]) >> 4) & 0x3ffffffffffff;

        self.h0 += m0;
        self.h1 += m1;
        self.h2 += m2;

        let d0 = (self.h0 as u128) * (self.r0 as u128)
            + (self.h1 as u128) * (self.r2 as u128 * 20)
            + (self.h2 as u128) * (self.r1 as u128 * 20);

        let d1 = (self.h0 as u128) * (self.r1 as u128)
            + (self.h1 as u128) * (self.r0 as u128)
            + (self.h2 as u128) * (self.r2 as u128 * 20);

        let d2 = (self.h0 as u128) * (self.r2 as u128)
            + (self.h1 as u128) * (self.r1 as u128)
            + (self.h2 as u128) * (self.r0 as u128);

        let mask = (1u128 << 44) - 1;
        let mut c = d0 >> 44;
        self.h0 = (d0 & mask) as u64;

        let d1 = d1 + c;
        c = d1 >> 44;
        self.h1 = (d1 & mask) as u64;

        let d2 = d2 + c;
        c = d2 >> 42;
        self.h2 = (d2 & ((1u128 << 42) - 1)) as u64;

        self.h0 += (c * 5) as u64;
        c = (self.h0 >> 44) as u128;
        self.h0 &= (1u64 << 44) - 1;
        self.h1 += c as u64;
    }

    pub fn update(&mut self, data: &[u8]) {
        let mut offset = 0;
        if self.buf_len > 0 {
            let to_copy = (16 - self.buf_len).min(data.len());
            self.buffer[self.buf_len..self.buf_len + to_copy].copy_from_slice(&data[..to_copy]);
            self.buf_len += to_copy;
            offset += to_copy;

            if self.buf_len == 16 {
                let blk = self.buffer;
                self.process_block(&blk, false);
                self.buf_len = 0;
            }
        }

        while offset + 16 <= data.len() {
            self.process_block(&data[offset..offset + 16], false);
            offset += 16;
        }

        if offset < data.len() {
            let rem = data.len() - offset;
            self.buffer[..rem].copy_from_slice(&data[offset..]);
            self.buf_len = rem;
        }
    }

    pub fn finalize(mut self) -> [u8; 16] {
        if self.buf_len > 0 {
            let len = self.buf_len;
            let mut blk = [0u8; 16];
            blk[..len].copy_from_slice(&self.buffer[..len]);
            self.process_block(&blk[..len], true);
        }

        let mut c = self.h1 >> 44;
        self.h1 &= (1u64 << 44) - 1;
        self.h2 += c;
        c = self.h2 >> 42;
        self.h2 &= (1u64 << 42) - 1;
        self.h0 += c * 5;
        c = self.h0 >> 44;
        self.h0 &= (1u64 << 44) - 1;
        self.h1 += c;

        // Fully reduce h
        let mut g0 = self.h0.wrapping_add(5);
        c = g0 >> 44;
        g0 &= (1u64 << 44) - 1;
        let mut g1 = self.h1.wrapping_add(c);
        c = g1 >> 44;
        g1 &= (1u64 << 44) - 1;
        let mut g2 = self.h2.wrapping_add(c);

        let mask = ((g2 >> 42) as i64).wrapping_neg() as u64;
        g2 &= (1u64 << 42) - 1;

        self.h0 = (self.h0 & !mask) | (g0 & mask);
        self.h1 = (self.h1 & !mask) | (g1 & mask);
        self.h2 = (self.h2 & !mask) | (g2 & mask);

        let h_bits = (self.h0 as u128) | ((self.h1 as u128) << 44) | ((self.h2 as u128) << 88);
        let pad_val = u128::from_le_bytes(self.pad);
        let tag_val = h_bits.wrapping_add(pad_val);

        tag_val.to_le_bytes()
    }
}

/// ChaCha20-Poly1305 AEAD (RFC 8439)
pub struct ChaCha20Poly1305;

impl ChaCha20Poly1305 {
    /// Encrypts in place, appending 16-byte authentication tag
    pub fn encrypt(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], plaintext: &mut Vec<u8>) {
        // 1. One-time Poly1305 key from counter 0
        let mut cipher0 = ChaCha20::new(key, nonce, 0);
        let mut poly_key_block = [0u8; 64];
        cipher0.block(&mut poly_key_block);
        let mut poly_key = [0u8; 32];
        poly_key.copy_from_slice(&poly_key_block[..32]);

        // 2. Encrypt plaintext starting at counter 1
        let mut cipher1 = ChaCha20::new(key, nonce, 1);
        cipher1.apply_keystream(plaintext.as_mut_slice());

        // 3. Poly1305 over AAD || pad(AAD) || ciphertext || pad(ciphertext) || len(AAD) || len(CT)
        let mut poly = Poly1305::new(&poly_key);
        poly.update(aad);
        if aad.len() % 16 != 0 {
            poly.update(&[0u8; 16][..16 - (aad.len() % 16)]);
        }

        poly.update(plaintext);
        if plaintext.len() % 16 != 0 {
            poly.update(&[0u8; 16][..16 - (plaintext.len() % 16)]);
        }

        poly.update(&(aad.len() as u64).to_le_bytes());
        poly.update(&(plaintext.len() as u64).to_le_bytes());

        let tag = poly.finalize();
        plaintext.extend_from_slice(&tag);
    }

    /// Decrypts in place, removing and verifying 16-byte authentication tag
    pub fn decrypt(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], ciphertext_and_tag: &mut Vec<u8>) -> Result<(), &'static str> {
        if ciphertext_and_tag.len() < 16 {
            return Err("Ciphertext too short for tag");
        }

        let ct_len = ciphertext_and_tag.len() - 16;
        let mut expected_tag = [0u8; 16];
        expected_tag.copy_from_slice(&ciphertext_and_tag[ct_len..]);
        ciphertext_and_tag.truncate(ct_len);

        // 1. Generate Poly1305 key
        let mut cipher0 = ChaCha20::new(key, nonce, 0);
        let mut poly_key_block = [0u8; 64];
        cipher0.block(&mut poly_key_block);
        let mut poly_key = [0u8; 32];
        poly_key.copy_from_slice(&poly_key_block[..32]);

        // 2. Compute Poly1305 tag
        let mut poly = Poly1305::new(&poly_key);
        poly.update(aad);
        if aad.len() % 16 != 0 {
            poly.update(&[0u8; 16][..16 - (aad.len() % 16)]);
        }

        poly.update(ciphertext_and_tag);
        if ciphertext_and_tag.len() % 16 != 0 {
            poly.update(&[0u8; 16][..16 - (ciphertext_and_tag.len() % 16)]);
        }

        poly.update(&(aad.len() as u64).to_le_bytes());
        poly.update(&(ciphertext_and_tag.len() as u64).to_le_bytes());

        let computed_tag = poly.finalize();

        // Constant time comparison
        let mut diff = 0u8;
        for i in 0..16 {
            diff |= expected_tag[i] ^ computed_tag[i];
        }

        if diff != 0 {
            return Err("Bad Poly1305 authentication tag");
        }

        // 3. Decrypt
        let mut cipher1 = ChaCha20::new(key, nonce, 1);
        cipher1.apply_keystream(ciphertext_and_tag.as_mut_slice());

        Ok(())
    }
}

