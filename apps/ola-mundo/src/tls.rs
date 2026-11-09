//! Jinn OS Ring 3 — Pilha Criptográfica TLS 1.3 no_std (Zero Alloc)
//!
//! Implementa os padrões RFC 8446 (TLS 1.3), RFC 8439 (ChaCha20-Poly1305),
//! RFC 7748 (Curve25519 / X25519) e RFC 6234 (SHA-256) operando inteiramente
//! sobre buffers estáticos e pilhas de execução, sem dependência de alocador dinâmico.

pub const TLS_CHACHA20_POLY1305_SHA256: u16 = 0x1303;
pub const TLS13_VERSION: u16 = 0x0304;

pub const CONTENT_ALERT: u8 = 21;
pub const CONTENT_HANDSHAKE: u8 = 22;
pub const CONTENT_APPLICATION_DATA: u8 = 23;

// ── SHA-256 & HMAC & HKDF ────────────────────────────────────────────────────

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
    0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
    0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
    0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
    0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
    0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

#[derive(Clone, Copy, Debug)]
pub struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    buf_len: usize,
    total_len: u64,
}

impl Sha256 {
    pub const fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
                0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
            ],
            buffer: [0u8; 64],
            buf_len: 0,
            total_len: 0,
        }
    }

    pub fn update(&mut self, data: &[u8]) {
        self.total_len += data.len() as u64;
        let mut offset = 0;

        if self.buf_len > 0 {
            let to_copy = (64 - self.buf_len).min(data.len());
            self.buffer[self.buf_len..self.buf_len + to_copy].copy_from_slice(&data[..to_copy]);
            self.buf_len += to_copy;
            offset += to_copy;

            if self.buf_len == 64 {
                let block = self.buffer;
                self.process_block(&block);
                self.buf_len = 0;
            }
        }

        while offset + 64 <= data.len() {
            let mut block = [0u8; 64];
            block.copy_from_slice(&data[offset..offset + 64]);
            self.process_block(&block);
            offset += 64;
        }

        if offset < data.len() {
            let rem = data.len() - offset;
            self.buffer[..rem].copy_from_slice(&data[offset..]);
            self.buf_len = rem;
        }
    }

    pub fn finalize(mut self) -> [u8; 32] {
        let bit_len = self.total_len * 8;
        self.buffer[self.buf_len] = 0x80;
        self.buf_len += 1;

        if self.buf_len > 56 {
            for b in &mut self.buffer[self.buf_len..64] { *b = 0; }
            let block = self.buffer;
            self.process_block(&block);
            self.buf_len = 0;
        }

        for b in &mut self.buffer[self.buf_len..56] { *b = 0; }
        self.buffer[56..64].copy_from_slice(&bit_len.to_be_bytes());
        let block = self.buffer;
        self.process_block(&block);

        let mut digest = [0u8; 32];
        for (i, word) in self.state.iter().enumerate() {
            digest[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        digest
    }

    pub fn digest(data: &[u8]) -> [u8; 32] {
        let mut h = Self::new();
        h.update(data);
        h.finalize()
    }

    fn process_block(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([block[i * 4], block[i * 4 + 1], block[i * 4 + 2], block[i * 4 + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let mut a = self.state[0];
        let mut b = self.state[1];
        let mut c = self.state[2];
        let mut d = self.state[3];
        let mut e = self.state[4];
        let mut f = self.state[5];
        let mut g = self.state[6];
        let mut h = self.state[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }
}

pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut key_pad = [0u8; 64];
    if key.len() > 64 {
        let hash = Sha256::digest(key);
        key_pad[..32].copy_from_slice(&hash);
    } else {
        key_pad[..key.len()].copy_from_slice(key);
    }

    let mut inner_pad = [0u8; 64];
    let mut outer_pad = [0u8; 64];
    for i in 0..64 {
        inner_pad[i] = key_pad[i] ^ 0x36;
        outer_pad[i] = key_pad[i] ^ 0x5c;
    }

    let mut inner = Sha256::new();
    inner.update(&inner_pad);
    inner.update(data);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(&outer_pad);
    outer.update(&inner_hash);
    outer.finalize()
}

pub fn hkdf_extract(salt: &[u8], ikm: &[u8]) -> [u8; 32] {
    let default_salt = [0u8; 32];
    let actual_salt = if salt.is_empty() { &default_salt[..] } else { salt };
    hmac_sha256(actual_salt, ikm)
}

pub fn hkdf_expand_label(secret: &[u8; 32], label: &[u8], context: &[u8], out: &mut [u8]) {
    let prefix = b"tls13 ";
    let full_label_len = prefix.len() + label.len();
    let mut hkdf_label = [0u8; 128];
    let mut p = 0;

    let len_bytes = (out.len() as u16).to_be_bytes();
    hkdf_label[p..p + 2].copy_from_slice(&len_bytes); p += 2;
    hkdf_label[p] = full_label_len as u8; p += 1;
    hkdf_label[p..p + prefix.len()].copy_from_slice(prefix); p += prefix.len();
    hkdf_label[p..p + label.len()].copy_from_slice(label); p += label.len();
    hkdf_label[p] = context.len() as u8; p += 1;
    if !context.is_empty() {
        hkdf_label[p..p + context.len()].copy_from_slice(context); p += context.len();
    }

    // Expand T(1)
    let info = &hkdf_label[..p];
    let mut hasher_data = [0u8; 160];
    hasher_data[..info.len()].copy_from_slice(info);
    hasher_data[info.len()] = 1;

    let t = hmac_sha256(secret, &hasher_data[..info.len() + 1]);
    let to_copy = out.len().min(32);
    out[..to_copy].copy_from_slice(&t[..to_copy]);
}

pub fn derive_secret(secret: &[u8; 32], label: &[u8], transcript_hash: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    hkdf_expand_label(secret, label, transcript_hash, &mut out);
    out
}

// ── Curve25519 / X25519 ──────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct U256(pub [u64; 4]);

const P: U256 = U256([
    0xFFFF_FFFF_FFFF_FFED,
    0xFFFF_FFFF_FFFF_FFFF,
    0xFFFF_FFFF_FFFF_FFFF,
    0x7FFF_FFFF_FFFF_FFFF,
]);

impl U256 {
    pub const ZERO: Self = U256([0, 0, 0, 0]);
    pub const ONE: Self = U256([1, 0, 0, 0]);
    pub const A24: Self = U256([121665, 0, 0, 0]);

    pub fn from_bytes_le(b: &[u8; 32]) -> Self {
        let mut limbs = [0u64; 4];
        for i in 0..4 {
            let mut arr = [0u8; 8];
            arr.copy_from_slice(&b[i * 8..i * 8 + 8]);
            limbs[i] = u64::from_le_bytes(arr);
        }
        limbs[3] &= 0x7FFF_FFFF_FFFF_FFFF;
        let mut u = U256(limbs);
        if u.gte(&P) { u = u.sub_p(); }
        u
    }

    pub fn to_bytes_le(&self) -> [u8; 32] {
        let mut u = *self;
        if u.gte(&P) { u = u.sub_p(); }
        let mut out = [0u8; 32];
        for i in 0..4 {
            out[i * 8..i * 8 + 8].copy_from_slice(&u.0[i].to_le_bytes());
        }
        out
    }

    pub fn gte(&self, other: &Self) -> bool {
        for i in (0..4).rev() {
            if self.0[i] > other.0[i] { return true; }
            if self.0[i] < other.0[i] { return false; }
        }
        true
    }

    fn sub_p(&self) -> Self {
        let mut borrow = 0i128;
        let mut res = [0u64; 4];
        for i in 0..4 {
            let diff = (self.0[i] as i128) - (P.0[i] as i128) - borrow;
            if diff < 0 {
                res[i] = (diff + 0x1_0000_0000_0000_0000i128) as u64;
                borrow = 1;
            } else {
                res[i] = diff as u64;
                borrow = 0;
            }
        }
        U256(res)
    }

    pub fn add(&self, other: &Self) -> Self {
        let mut carry = 0u128;
        let mut sum = [0u64; 4];
        for i in 0..4 {
            let s = (self.0[i] as u128) + (other.0[i] as u128) + carry;
            sum[i] = s as u64;
            carry = s >> 64;
        }

        let high_carry = (carry << 1) | ((sum[3] >> 63) as u128);
        sum[3] &= 0x7FFF_FFFF_FFFF_FFFF;

        let mut c2 = high_carry * 19;
        for i in 0..4 {
            let s = (sum[i] as u128) + (c2 & 0xFFFF_FFFF_FFFF_FFFF);
            sum[i] = s as u64;
            c2 = (c2 >> 64) + (s >> 64);
        }

        let mut res = U256(sum);
        if res.gte(&P) { res = res.sub_p(); }
        res
    }

    pub fn sub(&self, other: &Self) -> Self {
        if self.gte(other) {
            let mut borrow = 0i128;
            let mut res = [0u64; 4];
            for i in 0..4 {
                let diff = (self.0[i] as i128) - (other.0[i] as i128) - borrow;
                if diff < 0 {
                    res[i] = (diff + 0x1_0000_0000_0000_0000i128) as u64;
                    borrow = 1;
                } else {
                    res[i] = diff as u64;
                    borrow = 0;
                }
            }
            U256(res)
        } else {
            let self_plus_p = {
                let mut carry = 0u128;
                let mut s = [0u64; 4];
                for i in 0..4 {
                    let term = (self.0[i] as u128) + (P.0[i] as u128) + carry;
                    s[i] = term as u64;
                    carry = term >> 64;
                }
                U256(s)
            };
            let mut borrow = 0i128;
            let mut res = [0u64; 4];
            for i in 0..4 {
                let diff = (self_plus_p.0[i] as i128) - (other.0[i] as i128) - borrow;
                if diff < 0 {
                    res[i] = (diff + 0x1_0000_0000_0000_0000i128) as u64;
                    borrow = 1;
                } else {
                    res[i] = diff as u64;
                    borrow = 0;
                }
            }
            U256(res)
        }
    }

    pub fn mul(&self, other: &Self) -> Self {
        let mut limbs = [0u64; 8];
        for i in 0..4 {
            let mut carry = 0u128;
            for j in 0..4 {
                let cur = (limbs[i + j] as u128) + (self.0[i] as u128) * (other.0[j] as u128) + carry;
                limbs[i + j] = cur as u64;
                carry = cur >> 64;
            }
            let mut k = i + 4;
            while carry > 0 && k < 8 {
                let cur = (limbs[k] as u128) + carry;
                limbs[k] = cur as u64;
                carry = cur >> 64;
                k += 1;
            }
        }

        let mut sum1 = [0u64; 5];
        let mut c = 0u128;
        for i in 0..4 {
            let term = (limbs[4 + i] as u128) * 38 + (limbs[i] as u128) + c;
            sum1[i] = term as u64;
            c = term >> 64;
        }
        sum1[4] = c as u64;

        let high1 = ((sum1[3] >> 63) as u128) | ((sum1[4] as u128) << 1);
        sum1[3] &= 0x7FFF_FFFF_FFFF_FFFF;

        let mut c2 = high1 * 19;
        let mut res = [0u64; 4];
        for i in 0..4 {
            let v = (sum1[i] as u128) + (c2 & 0xFFFF_FFFF_FFFF_FFFF);
            res[i] = v as u64;
            c2 = (c2 >> 64) + (v >> 64);
        }

        while c2 > 0 || (res[3] >> 63) > 0 {
            let high2 = (c2 << 1) | ((res[3] >> 63) as u128);
            res[3] &= 0x7FFF_FFFF_FFFF_FFFF;
            let mut c3 = high2 * 19;
            for i in 0..4 {
                let v = (res[i] as u128) + (c3 & 0xFFFF_FFFF_FFFF_FFFF);
                res[i] = v as u64;
                c3 = (c3 >> 64) + (v >> 64);
            }
            c2 = c3;
        }

        let mut final_u = U256(res);
        if final_u.gte(&P) { final_u = final_u.sub_p(); }
        final_u
    }

    pub fn square(&self) -> Self {
        self.mul(self)
    }

    pub fn invert(&self) -> Self {
        let mut res = U256::ONE;
        let mut base = *self;

        let p_minus_2: [u64; 4] = [
            0xFFFF_FFFF_FFFF_FFEBu64,
            0xFFFF_FFFF_FFFF_FFFFu64,
            0xFFFF_FFFF_FFFF_FFFFu64,
            0x7FFF_FFFF_FFFF_FFFFu64,
        ];

        for &limb in &p_minus_2 {
            for b in 0..64 {
                if limb == 0x7FFF_FFFF_FFFF_FFFFu64 && b >= 63 { break; }
                if (limb >> b) & 1 == 1 {
                    res = res.mul(&base);
                }
                base = base.square();
            }
        }
        res
    }

    pub fn cswap(a: &mut Self, b: &mut Self, swap: u64) {
        let mask = (swap as i64).wrapping_neg() as u64;
        for i in 0..4 {
            let x = mask & (a.0[i] ^ b.0[i]);
            a.0[i] ^= x;
            b.0[i] ^= x;
        }
    }
}

pub fn x25519(scalar: &[u8; 32], point_u: &[u8; 32]) -> [u8; 32] {
    let mut k = *scalar;
    k[0] &= 248;
    k[31] &= 127;
    k[31] |= 64;

    let x1 = U256::from_bytes_le(point_u);
    let mut x2 = U256::ONE;
    let mut z2 = U256::ZERO;
    let mut x3 = x1;
    let mut z3 = U256::ONE;
    let mut swap = 0u64;

    for i in (0..255).rev() {
        let bit = ((k[i / 8] >> (i % 8)) & 1) as u64;
        swap ^= bit;
        U256::cswap(&mut x2, &mut x3, swap);
        U256::cswap(&mut z2, &mut z3, swap);
        swap = bit;

        let a = x2.add(&z2);
        let aa = a.square();
        let b = x2.sub(&z2);
        let bb = b.square();
        let e = aa.sub(&bb);
        let c = x3.add(&z3);
        let d = x3.sub(&z3);
        let da = d.mul(&a);
        let cb = c.mul(&b);

        x3 = da.add(&cb).square();
        z3 = x1.mul(&da.sub(&cb).square());
        x2 = aa.mul(&bb);
        z2 = e.mul(&aa.add(&U256::A24.mul(&e)));
    }

    U256::cswap(&mut x2, &mut x3, swap);
    U256::cswap(&mut z2, &mut z3, swap);

    let z2_inv = z2.invert();
    let result = x2.mul(&z2_inv);
    result.to_bytes_le()
}

pub fn x25519_basepoint(scalar: &[u8; 32]) -> [u8; 32] {
    let mut base = [0u8; 32];
    base[0] = 9;
    x25519(scalar, &base)
}

// ── ChaCha20 & Poly1305 ──────────────────────────────────────────────────────

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
            state[4 + i] = u32::from_le_bytes([key[i * 4], key[i * 4 + 1], key[i * 4 + 2], key[i * 4 + 3]]);
        }
        state[12] = counter;
        for i in 0..3 {
            state[13 + i] = u32::from_le_bytes([nonce[i * 4], nonce[i * 4 + 1], nonce[i * 4 + 2], nonce[i * 4 + 3]]);
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
        if is_final { b[block.len()] = 1; } else { b[16] = 1; }

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

pub struct ChaCha20Poly1305;

impl ChaCha20Poly1305 {
    pub fn encrypt(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], plaintext: &mut [u8]) -> [u8; 16] {
        let mut cipher0 = ChaCha20::new(key, nonce, 0);
        let mut poly_key_block = [0u8; 64];
        cipher0.block(&mut poly_key_block);
        let mut poly_key = [0u8; 32];
        poly_key.copy_from_slice(&poly_key_block[..32]);

        let mut cipher1 = ChaCha20::new(key, nonce, 1);
        cipher1.apply_keystream(plaintext);

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

        poly.finalize()
    }

    pub fn decrypt(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], ciphertext: &mut [u8], expected_tag: &[u8; 16]) -> bool {
        let mut cipher0 = ChaCha20::new(key, nonce, 0);
        let mut poly_key_block = [0u8; 64];
        cipher0.block(&mut poly_key_block);
        let mut poly_key = [0u8; 32];
        poly_key.copy_from_slice(&poly_key_block[..32]);

        let mut poly = Poly1305::new(&poly_key);
        poly.update(aad);
        if aad.len() % 16 != 0 {
            poly.update(&[0u8; 16][..16 - (aad.len() % 16)]);
        }

        poly.update(ciphertext);
        if ciphertext.len() % 16 != 0 {
            poly.update(&[0u8; 16][..16 - (ciphertext.len() % 16)]);
        }

        poly.update(&(aad.len() as u64).to_le_bytes());
        poly.update(&(ciphertext.len() as u64).to_le_bytes());

        let computed_tag = poly.finalize();

        let mut diff = 0u8;
        for i in 0..16 {
            diff |= expected_tag[i] ^ computed_tag[i];
        }

        if diff != 0 {
            return false;
        }

        let mut cipher1 = ChaCha20::new(key, nonce, 1);
        cipher1.apply_keystream(ciphertext);
        true
    }
}

// ── TLS 1.3 Client Handshake Protocol (Buffer Estático) ─────────────────────

pub struct Tls13ClientSession {
    pub client_priv: [u8; 32],
    pub client_pub: [u8; 32],
    pub client_random: [u8; 32],
    pub server_random: [u8; 32],
    pub transcript: Sha256,
    pub client_hs_key: [u8; 32],
    pub client_hs_iv: [u8; 12],
    pub server_hs_key: [u8; 32],
    pub server_hs_iv: [u8; 12],
    pub client_app_key: [u8; 32],
    pub client_app_iv: [u8; 12],
    pub server_app_key: [u8; 32],
    pub server_app_iv: [u8; 12],
    pub client_seq: u64,
    pub server_seq: u64,
    pub is_established: bool,
}

impl Tls13ClientSession {
    pub fn new(random_seed: &[u8; 64]) -> Self {
        let mut client_priv = [0u8; 32];
        let mut client_random = [0u8; 32];
        client_priv.copy_from_slice(&random_seed[0..32]);
        client_random.copy_from_slice(&random_seed[32..64]);

        let client_pub = x25519_basepoint(&client_priv);

        Self {
            client_priv,
            client_pub,
            client_random,
            server_random: [0u8; 32],
            transcript: Sha256::new(),
            client_hs_key: [0u8; 32],
            client_hs_iv: [0u8; 12],
            server_hs_key: [0u8; 32],
            server_hs_iv: [0u8; 12],
            client_app_key: [0u8; 32],
            client_app_iv: [0u8; 12],
            server_app_key: [0u8; 32],
            server_app_iv: [0u8; 12],
            client_seq: 0,
            server_seq: 0,
            is_established: false,
        }
    }

    /// Gera o pacote ClientHello gravando diretamente no buffer de saída
    pub fn build_client_hello(&mut self, hostname: &[u8], out: &mut [u8]) -> usize {
        let mut p = 0;

        // TLS Record Header: [0x16, 0x03, 0x01, len_hi, len_lo]
        out[p] = CONTENT_HANDSHAKE; p += 1;
        out[p] = 0x03; out[p + 1] = 0x01; p += 2;
        let record_len_pos = p; p += 2;

        let hs_start = p;
        // Handshake Header: [Type=1 (ClientHello), len24]
        out[p] = 1; p += 1;
        let hs_len_pos = p; p += 3;

        let body_start = p;
        // Legacy version: 0x0303
        out[p] = 0x03; out[p + 1] = 0x03; p += 2;
        // Client random (32 bytes)
        out[p..p + 32].copy_from_slice(&self.client_random); p += 32;
        // Legacy session ID (32 bytes)
        out[p] = 32; p += 1;
        for i in 0..32 { out[p + i] = 0x42; } p += 32;

        // Cipher suites: TLS_CHACHA20_POLY1305_SHA256 (0x1303)
        out[p] = 0x00; out[p + 1] = 0x02; out[p + 2] = 0x13; out[p + 3] = 0x03; p += 4;
        // Compression
        out[p] = 0x01; out[p + 1] = 0x00; p += 2;

        // Extensions Length pos
        let ext_len_pos = p; p += 2;
        let ext_start = p;

        // 1. SNI
        out[p] = 0x00; out[p + 1] = 0x00; p += 2; // ext type 0
        let sni_len = 2 + 1 + 2 + hostname.len();
        out[p] = (sni_len >> 8) as u8; out[p + 1] = sni_len as u8; p += 2;
        let list_len = (hostname.len() + 3) as u16;
        out[p] = (list_len >> 8) as u8; out[p + 1] = list_len as u8; p += 2;
        out[p] = 0; p += 1; // HostName
        out[p] = (hostname.len() >> 8) as u8; out[p + 1] = hostname.len() as u8; p += 2;
        out[p..p + hostname.len()].copy_from_slice(hostname); p += hostname.len();

        // 2. Supported Groups: X25519 (0x001D)
        out[p] = 0x00; out[p + 1] = 0x0A; p += 2;
        out[p] = 0x00; out[p + 1] = 0x04; p += 2;
        out[p] = 0x00; out[p + 1] = 0x02; p += 2;
        out[p] = 0x00; out[p + 1] = 0x1D; p += 2;

        // 3. Supported Versions: TLS 1.3 (0x0304)
        out[p] = 0x00; out[p + 1] = 0x2B; p += 2;
        out[p] = 0x00; out[p + 1] = 0x03; p += 2;
        out[p] = 0x02; p += 1;
        out[p] = 0x03; out[p + 1] = 0x04; p += 2;

        // 4. Key Share: Client X25519 Public Key
        out[p] = 0x00; out[p + 1] = 0x33; p += 2;
        let ks_len = 2 + 2 + 2 + 32;
        out[p] = (ks_len >> 8) as u8; out[p + 1] = ks_len as u8; p += 2;
        out[p] = 0x00; out[p + 1] = 36; p += 2; // client shares len
        out[p] = 0x00; out[p + 1] = 0x1D; p += 2; // group X25519
        out[p] = 0x00; out[p + 1] = 32; p += 2; // key len
        out[p..p + 32].copy_from_slice(&self.client_pub); p += 32;

        // Patch extensions len
        let ext_bytes = p - ext_start;
        out[ext_len_pos] = (ext_bytes >> 8) as u8;
        out[ext_len_pos + 1] = ext_bytes as u8;

        // Patch handshake len
        let hs_body_len = p - body_start;
        out[hs_len_pos] = (hs_body_len >> 16) as u8;
        out[hs_len_pos + 1] = (hs_body_len >> 8) as u8;
        out[hs_len_pos + 2] = hs_body_len as u8;

        // Patch record len
        let rec_data_len = p - hs_start;
        out[record_len_pos] = (rec_data_len >> 8) as u8;
        out[record_len_pos + 1] = rec_data_len as u8;

        // Update transcript hash with the Handshake message
        self.transcript.update(&out[hs_start..p]);

        p
    }

    /// Processa o ServerHello do servidor e deriva os segredos de chave
    pub fn process_server_hello(&mut self, record: &[u8]) -> bool {
        if record.len() < 5 || record[0] != CONTENT_HANDSHAKE {
            return false;
        }
        let hs_data = &record[5..];
        if hs_data.len() < 40 || hs_data[0] != 2 {
            return false;
        }

        self.transcript.update(hs_data);

        // Server random
        self.server_random.copy_from_slice(&hs_data[6..38]);

        // Localiza key_share nas extensões do ServerHello
        let mut server_pub = [0u8; 32];
        let mut found_ks = false;

        let sess_len = hs_data[38] as usize;
        let mut pos = 39 + sess_len;
        if pos + 2 > hs_data.len() { return false; }
        pos += 2 + 1; // cipher suite + compression

        if pos + 2 <= hs_data.len() {
            let ext_len = ((hs_data[pos] as usize) << 8) | (hs_data[pos + 1] as usize);
            pos += 2;
            let end = pos + ext_len;
            while pos + 4 <= end && pos + 4 <= hs_data.len() {
                let ext_id = ((hs_data[pos] as u16) << 8) | (hs_data[pos + 1] as u16);
                let e_len = ((hs_data[pos + 2] as usize) << 8) | (hs_data[pos + 3] as usize);
                pos += 4;
                if ext_id == 51 && e_len >= 36 && pos + 36 <= hs_data.len() {
                    let group = ((hs_data[pos] as u16) << 8) | (hs_data[pos + 1] as u16);
                    if group == 0x001D {
                        server_pub.copy_from_slice(&hs_data[pos + 4..pos + 36]);
                        found_ks = true;
                        break;
                    }
                }
                pos += e_len;
            }
        }

        if !found_ks { return false; }

        // Deriva segredo compartilhado X25519
        let shared_secret = x25519(&self.client_priv, &server_pub);

        // Deriva Early Secret e Handshake Secret
        let early_secret = hkdf_extract(&[0u8; 32], &[0u8; 32]);
        let empty_hash = Sha256::digest(&[]);
        let derived_secret = derive_secret(&early_secret, b"derived", &empty_hash);
        let handshake_secret = hkdf_extract(&derived_secret, &shared_secret);

        let transcript_hash = self.transcript.finalize();
        let client_hs_secret = derive_secret(&handshake_secret, b"c hs traffic", &transcript_hash);
        let server_hs_secret = derive_secret(&handshake_secret, b"s hs traffic", &transcript_hash);

        hkdf_expand_label(&client_hs_secret, b"key", &[], &mut self.client_hs_key);
        hkdf_expand_label(&client_hs_secret, b"iv", &[], &mut self.client_hs_iv);
        hkdf_expand_label(&server_hs_secret, b"key", &[], &mut self.server_hs_key);
        hkdf_expand_label(&server_hs_secret, b"iv", &[], &mut self.server_hs_iv);

        // Deriva Master Secret e Application Traffic Secrets
        let derived_app = derive_secret(&handshake_secret, b"derived", &empty_hash);
        let master_secret = hkdf_extract(&derived_app, &[0u8; 32]);

        let client_app_secret = derive_secret(&master_secret, b"c ap traffic", &transcript_hash);
        let server_app_secret = derive_secret(&master_secret, b"s ap traffic", &transcript_hash);

        hkdf_expand_label(&client_app_secret, b"key", &[], &mut self.client_app_key);
        hkdf_expand_label(&client_app_secret, b"iv", &[], &mut self.client_app_iv);
        hkdf_expand_label(&server_app_secret, b"key", &[], &mut self.server_app_key);
        hkdf_expand_label(&server_app_secret, b"iv", &[], &mut self.server_app_iv);

        self.is_established = true;
        true
    }

    /// Criptografa dados em um registro TLSCiphertext
    pub fn encrypt_application_data(&mut self, payload: &[u8], out: &mut [u8]) -> usize {
        let mut nonce = self.client_app_iv;
        let seq_bytes = self.client_seq.to_be_bytes();
        for i in 0..8 { nonce[4 + i] ^= seq_bytes[i]; }
        self.client_seq += 1;

        let ct_len = payload.len() + 1 + 16;
        out[0] = CONTENT_APPLICATION_DATA;
        out[1] = 0x03; out[2] = 0x03;
        out[3] = (ct_len >> 8) as u8;
        out[4] = ct_len as u8;

        // Plaintext + inner type
        out[5..5 + payload.len()].copy_from_slice(payload);
        out[5 + payload.len()] = CONTENT_APPLICATION_DATA;

        let aad = [out[0], out[1], out[2], out[3], out[4]];
        let tag = ChaCha20Poly1305::encrypt(
            &self.client_app_key,
            &nonce,
            &aad,
            &mut out[5..5 + payload.len() + 1],
        );
        out[5 + payload.len() + 1..5 + ct_len].copy_from_slice(&tag);

        5 + ct_len
    }

    /// Descriptografa registro recebido de volta para o buffer
    pub fn decrypt_application_data(&mut self, record: &[u8], out_payload: &mut [u8]) -> Option<usize> {
        if record.len() < 5 + 16 + 1 { return None; }
        let ct_len = ((record[3] as usize) << 8) | (record[4] as usize);
        if 5 + ct_len > record.len() { return None; }

        let mut nonce = self.server_app_iv;
        let seq_bytes = self.server_seq.to_be_bytes();
        for i in 0..8 { nonce[4 + i] ^= seq_bytes[i]; }
        self.server_seq += 1;

        let pt_and_type_len = ct_len - 16;
        let aad = &record[0..5];
        let mut expected_tag = [0u8; 16];
        expected_tag.copy_from_slice(&record[5 + pt_and_type_len..5 + ct_len]);

        out_payload[..pt_and_type_len].copy_from_slice(&record[5..5 + pt_and_type_len]);

        let ok = ChaCha20Poly1305::decrypt(
            &self.server_app_key,
            &nonce,
            aad,
            &mut out_payload[..pt_and_type_len],
            &expected_tag,
        );

        if !ok { return None; }

        let mut end = pt_and_type_len;
        while end > 0 && out_payload[end - 1] == 0 { end -= 1; }
        if end == 0 { return None; }
        let inner_type = out_payload[end - 1];
        end -= 1;

        if inner_type == CONTENT_APPLICATION_DATA {
            Some(end)
        } else {
            None
        }
    }
}

