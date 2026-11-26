//! Jinn OS Ring 3 — Pilha Criptográfica TLS 1.3 no_std (Zero Alloc)
//!
//! Implementa os padrões RFC 8446 (TLS 1.3), RFC 8439 (ChaCha20-Poly1305),
//! RFC 7748 (Curve25519 / X25519) e RFC 6234 (SHA-256) operando inteiramente
//! sobre buffers estáticos e pilhas de execução, sem dependência de alocador dinâmico.

pub const TLS_AES_128_GCM_SHA256: u16 = 0x1301;
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

    pub fn current_digest(&self) -> [u8; 32] {
        let copy = *self;
        copy.finalize()
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
        let mut clamped = [0u8; 16];
        clamped.copy_from_slice(&key[..16]);
        clamped[3] &= 15;
        clamped[7] &= 15;
        clamped[11] &= 15;
        clamped[15] &= 15;
        clamped[4] &= 252;
        clamped[8] &= 252;
        clamped[12] &= 252;

        let r_full = u128::from_le_bytes(clamped);
        let r0 = (r_full & 0x0fff_ffff_ffff) as u64;
        let r1 = ((r_full >> 44) & 0x0fff_ffff_ffff) as u64;
        let r2 = ((r_full >> 88) & 0x03ff_ffff_ffff) as u64;

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
        let mut lo = [0u8; 16];
        lo[..block.len()].copy_from_slice(block);
        let hi_bit: u128 = if is_final && block.len() < 16 {
            lo[block.len()] = 1;
            0
        } else {
            1
        };

        let m_lo = u128::from_le_bytes(lo);
        let m0 = (m_lo & 0x0fff_ffff_ffff) as u64;
        let m1 = ((m_lo >> 44) & 0x0fff_ffff_ffff) as u64;
        let m2 = (((m_lo >> 88) & 0x00ff_ffff_ffff) | (hi_bit << 40)) as u64;

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
        c = self.h1 >> 44;
        self.h1 &= (1u64 << 44) - 1;
        self.h2 += c;

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

        let h_bits = (self.h0 as u128)
            .wrapping_add((self.h1 as u128) << 44)
            .wrapping_add((self.h2 as u128) << 88);
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

// ── AES-128-GCM AEAD (NIST FIPS 197 / SP 800-38D, Zero Alloc) ────────────────

const AES_SBOX: [u8; 256] = [
    0x63,0x7c,0x77,0x7b,0xf2,0x6b,0x6f,0xc5,0x30,0x01,0x67,0x2b,0xfe,0xd7,0xab,0x76,
    0xca,0x82,0xc9,0x7d,0xfa,0x59,0x47,0xf0,0xad,0xd4,0xa2,0xaf,0x9c,0xa4,0x72,0xc0,
    0xb7,0xfd,0x93,0x26,0x36,0x3f,0xf7,0xcc,0x34,0xa5,0xe5,0xf1,0x71,0xd8,0x31,0x15,
    0x04,0xc7,0x23,0xc3,0x18,0x96,0x05,0x9a,0x07,0x12,0x80,0xe2,0xeb,0x27,0xb2,0x75,
    0x09,0x83,0x2c,0x1a,0x1b,0x6e,0x5a,0xa0,0x52,0x3b,0xd6,0xb3,0x29,0xe3,0x2f,0x84,
    0x53,0xd1,0x00,0xed,0x20,0xfc,0xb1,0x5b,0x6a,0xcb,0xbe,0x39,0x4a,0x4c,0x58,0xcf,
    0xd0,0xef,0xaa,0xfb,0x43,0x4d,0x33,0x85,0x45,0xf9,0x02,0x7f,0x50,0x3c,0x9f,0xa8,
    0x51,0xa3,0x40,0x8f,0x92,0x9d,0x38,0xf5,0xbc,0xb6,0xda,0x21,0x10,0xff,0xf3,0xd2,
    0xcd,0x0c,0x13,0xec,0x5f,0x97,0x44,0x17,0xc4,0xa7,0x7e,0x3d,0x64,0x5d,0x19,0x73,
    0x60,0x81,0x4f,0xdc,0x22,0x2a,0x90,0x88,0x46,0xee,0xb8,0x14,0xde,0x5e,0x0b,0xdb,
    0xe0,0x32,0x3a,0x0a,0x49,0x06,0x24,0x5c,0xc2,0xd3,0xac,0x62,0x91,0x95,0xe4,0x79,
    0xe7,0xc8,0x37,0x6d,0x8d,0xd5,0x4e,0xa9,0x6c,0x56,0xf4,0xea,0x65,0x7a,0xae,0x08,
    0xba,0x78,0x25,0x2e,0x1c,0xa6,0xb4,0xc6,0xe8,0xdd,0x74,0x1f,0x4b,0xbd,0x8b,0x8a,
    0x70,0x3e,0xb5,0x66,0x48,0x03,0xf6,0x0e,0x61,0x35,0x57,0xb9,0x86,0xc1,0x1d,0x9e,
    0xe1,0xf8,0x98,0x11,0x69,0xd9,0x8e,0x94,0x9b,0x1e,0x87,0xe9,0xce,0x55,0x28,0xdf,
    0x8c,0xa1,0x89,0x0d,0xbf,0xe6,0x42,0x68,0x41,0x99,0x2d,0x0f,0xb0,0x54,0xbb,0x16,
];

const AES_RCON: [u8; 10] = [0x01,0x02,0x04,0x08,0x10,0x20,0x40,0x80,0x1b,0x36];

#[inline(always)]
fn xtime(b: u8) -> u8 {
    (b << 1) ^ (((b >> 7) & 1).wrapping_mul(0x1b))
}

fn aes128_key_schedule(key: &[u8; 16]) -> [[u8; 16]; 11] {
    let mut w = [0u8; 176];
    w[..16].copy_from_slice(key);
    for r in 0..10 {
        let off = (r + 1) * 16;
        let t0 = AES_SBOX[w[off - 3] as usize] ^ AES_RCON[r];
        let t1 = AES_SBOX[w[off - 2] as usize];
        let t2 = AES_SBOX[w[off - 1] as usize];
        let t3 = AES_SBOX[w[off - 4] as usize];
        w[off] = w[off - 16] ^ t0;
        w[off + 1] = w[off - 15] ^ t1;
        w[off + 2] = w[off - 14] ^ t2;
        w[off + 3] = w[off - 13] ^ t3;
        for i in 4..16 {
            w[off + i] = w[off + i - 16] ^ w[off + i - 4];
        }
    }
    let mut rk = [[0u8; 16]; 11];
    for r in 0..11 {
        rk[r].copy_from_slice(&w[r * 16..(r + 1) * 16]);
    }
    rk
}

fn aes128_encrypt_block(rk: &[[u8; 16]; 11], inp: &[u8; 16]) -> [u8; 16] {
    let mut s = [0u8; 16];
    for i in 0..16 {
        s[i] = inp[i] ^ rk[0][i];
    }
    for rnd in 1..=10 {
        for b in s.iter_mut() {
            *b = AES_SBOX[*b as usize];
        }
        s = [
            s[0], s[5], s[10], s[15],
            s[4], s[9], s[14], s[3],
            s[8], s[13], s[2], s[7],
            s[12], s[1], s[6], s[11],
        ];
        if rnd < 10 {
            let mut ns = [0u8; 16];
            for c in 0..4 {
                let a = s[4 * c];
                let b = s[4 * c + 1];
                let c0 = s[4 * c + 2];
                let d = s[4 * c + 3];
                ns[4 * c]     = xtime(a) ^ xtime(b) ^ b ^ c0 ^ d;
                ns[4 * c + 1] = a ^ xtime(b) ^ xtime(c0) ^ c0 ^ d;
                ns[4 * c + 2] = a ^ b ^ xtime(c0) ^ xtime(d) ^ d;
                ns[4 * c + 3] = xtime(a) ^ a ^ b ^ c0 ^ xtime(d);
            }
            s = ns;
        }
        for i in 0..16 {
            s[i] ^= rk[rnd][i];
        }
    }
    s
}

fn gf_mul_128(x: &[u8; 16], y: &[u8; 16]) -> [u8; 16] {
    let vx = u128::from_be_bytes(*x);
    let mut vy = u128::from_be_bytes(*y);
    let mut z: u128 = 0;
    for i in 0..128 {
        if ((vx >> (127 - i)) & 1) != 0 {
            z ^= vy;
        }
        let lsb = vy & 1;
        vy >>= 1;
        if lsb != 0 {
            vy ^= 0xE1u128 << 120;
        }
    }
    z.to_be_bytes()
}

fn gcm_ghash(h: &[u8; 16], aad: &[u8], ct: &[u8]) -> [u8; 16] {
    let mut y = [0u8; 16];
    let mut update = |data: &[u8]| {
        let mut off = 0;
        while off < data.len() {
            let take = (data.len() - off).min(16);
            let mut blk = y;
            for j in 0..take {
                blk[j] ^= data[off + j];
            }
            y = gf_mul_128(&blk, h);
            off += 16;
        }
    };
    update(aad);
    update(ct);
    let mut lens = [0u8; 16];
    lens[..8].copy_from_slice(&((aad.len() as u64) * 8).to_be_bytes());
    lens[8..].copy_from_slice(&((ct.len() as u64) * 8).to_be_bytes());
    for j in 0..16 {
        y[j] ^= lens[j];
    }
    gf_mul_128(&y, h)
}

pub struct Aes128Gcm;

impl Aes128Gcm {
    fn keystream_xor(rk: &[[u8; 16]; 11], nonce: &[u8; 12], data: &mut [u8]) {
        let mut ctr_blk = [0u8; 16];
        ctr_blk[..12].copy_from_slice(nonce);
        let mut off = 0usize;
        let mut ctr: u32 = 2;
        while off < data.len() {
            ctr_blk[12..16].copy_from_slice(&ctr.to_be_bytes());
            let ks = aes128_encrypt_block(rk, &ctr_blk);
            let take = (data.len() - off).min(16);
            for j in 0..take {
                data[off + j] ^= ks[j];
            }
            off += 16;
            ctr = ctr.wrapping_add(1);
        }
    }

    pub fn encrypt(key: &[u8; 16], nonce: &[u8; 12], aad: &[u8], plaintext: &mut [u8]) -> [u8; 16] {
        let rk = aes128_key_schedule(key);
        let h = aes128_encrypt_block(&rk, &[0u8; 16]);
        Self::keystream_xor(&rk, nonce, plaintext);
        let y = gcm_ghash(&h, aad, plaintext);
        let mut j0 = [0u8; 16];
        j0[..12].copy_from_slice(nonce);
        j0[15] = 1;
        let s0 = aes128_encrypt_block(&rk, &j0);
        let mut tag = [0u8; 16];
        for i in 0..16 {
            tag[i] = y[i] ^ s0[i];
        }
        tag
    }

    pub fn decrypt(key: &[u8; 16], nonce: &[u8; 12], aad: &[u8], ciphertext: &mut [u8], expected_tag: &[u8; 16]) -> bool {
        let rk = aes128_key_schedule(key);
        let h = aes128_encrypt_block(&rk, &[0u8; 16]);
        let y = gcm_ghash(&h, aad, ciphertext);
        let mut j0 = [0u8; 16];
        j0[..12].copy_from_slice(nonce);
        j0[15] = 1;
        let s0 = aes128_encrypt_block(&rk, &j0);
        let mut diff = 0u8;
        for i in 0..16 {
            diff |= (y[i] ^ s0[i]) ^ expected_tag[i];
        }
        if diff != 0 {
            return false;
        }
        Self::keystream_xor(&rk, nonce, ciphertext);
        true
    }
}

fn is_ipv4_literal_bytes(host: &[u8]) -> bool {
    if host.is_empty() { return false; }
    let mut dots = 0usize;
    for &b in host {
        if b == b'.' { dots += 1; }
        else if !b.is_ascii_digit() { return false; }
    }
    dots == 3
}

static mut HS_DECRYPT_BUF: [u8; 18432] = [0u8; 18432];
static mut HS_REASM_BUF: [u8; 32768] = [0u8; 32768];

// ── TLS 1.3 Client Handshake Protocol (Buffer Estático RFC 8446) ─────────────

pub struct Tls13ClientSession {
    pub client_priv: [u8; 32],
    pub client_pub: [u8; 32],
    pub client_random: [u8; 32],
    pub server_random: [u8; 32],
    pub transcript: Sha256,
    pub handshake_secret: [u8; 32],
    pub master_secret: [u8; 32],
    pub client_hs_traffic_secret: [u8; 32],
    pub server_hs_traffic_secret: [u8; 32],
    pub client_hs_key: [u8; 32],
    pub client_hs_iv: [u8; 12],
    pub server_hs_key: [u8; 32],
    pub server_hs_iv: [u8; 12],
    pub client_app_traffic_secret: [u8; 32],
    pub server_app_traffic_secret: [u8; 32],
    pub client_app_key: [u8; 32],
    pub client_app_iv: [u8; 12],
    pub server_app_key: [u8; 32],
    pub server_app_iv: [u8; 12],
    pub client_hs_seq: u64,
    pub server_hs_seq: u64,
    pub client_seq: u64,
    pub server_seq: u64,
    pub cipher_suite: u16,
    pub hs_reasm_len: usize,
    pub has_server_hello: bool,
    pub is_established: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppRecord {
    Data(usize),
    Handshake,
    Alert(u8, u8),
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
            handshake_secret: [0u8; 32],
            master_secret: [0u8; 32],
            client_hs_traffic_secret: [0u8; 32],
            server_hs_traffic_secret: [0u8; 32],
            client_hs_key: [0u8; 32],
            client_hs_iv: [0u8; 12],
            server_hs_key: [0u8; 32],
            server_hs_iv: [0u8; 12],
            client_app_traffic_secret: [0u8; 32],
            server_app_traffic_secret: [0u8; 32],
            client_app_key: [0u8; 32],
            client_app_iv: [0u8; 12],
            server_app_key: [0u8; 32],
            server_app_iv: [0u8; 12],
            client_hs_seq: 0,
            server_hs_seq: 0,
            client_seq: 0,
            server_seq: 0,
            cipher_suite: TLS_CHACHA20_POLY1305_SHA256,
            hs_reasm_len: 0,
            has_server_hello: false,
            is_established: false,
        }
    }

    /// Gera o pacote ClientHello com extensões completas RFC 8446 (SNI, supported_groups, signature_algorithms, supported_versions, key_share)
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
        // Legacy session ID (32 bytes para Middlebox Compatibility)
        out[p] = 32; p += 1;
        for i in 0..32 { out[p + i] = 0x42; } p += 32;

        // Cipher suites: TLS_CHACHA20_POLY1305_SHA256 (0x1303) e TLS_AES_128_GCM_SHA256 (0x1301)
        out[p] = 0x00; out[p + 1] = 0x04; p += 2;
        out[p] = 0x13; out[p + 1] = 0x03; p += 2;
        out[p] = 0x13; out[p + 1] = 0x01; p += 2;
        // Compression (null)
        out[p] = 0x01; out[p + 1] = 0x00; p += 2;

        // Extensions Length pos
        let ext_len_pos = p; p += 2;
        let ext_start = p;

        // 1. server_name (SNI - 0x0000): omitido para IPv4 literal conforme RFC 6066
        if !hostname.is_empty() && !is_ipv4_literal_bytes(hostname) {
            out[p] = 0x00; out[p + 1] = 0x00; p += 2;
            let sni_len = 2 + 1 + 2 + hostname.len();
            out[p] = (sni_len >> 8) as u8; out[p + 1] = sni_len as u8; p += 2;
            let list_len = (hostname.len() + 3) as u16;
            out[p] = (list_len >> 8) as u8; out[p + 1] = list_len as u8; p += 2;
            out[p] = 0; p += 1; // HostName
            out[p] = (hostname.len() >> 8) as u8; out[p + 1] = hostname.len() as u8; p += 2;
            out[p..p + hostname.len()].copy_from_slice(hostname); p += hostname.len();
        }

        // 2. supported_groups (0x000A): X25519 (0x001D) e secp256r1 (0x0017)
        out[p] = 0x00; out[p + 1] = 0x0A; p += 2;
        out[p] = 0x00; out[p + 1] = 0x06; p += 2; // ext_len = 6
        out[p] = 0x00; out[p + 1] = 0x04; p += 2; // list_len = 4
        out[p] = 0x00; out[p + 1] = 0x1D; p += 2; // X25519
        out[p] = 0x00; out[p + 1] = 0x17; p += 2; // secp256r1

        // 3. signature_algorithms (0x000D): ECDSA P-256/P-384, Ed25519, RSA-PSS e RSA PKCS#1
        out[p] = 0x00; out[p + 1] = 0x0D; p += 2;
        out[p] = 0x00; out[p + 1] = 0x10; p += 2; // ext_len = 16
        out[p] = 0x00; out[p + 1] = 0x0E; p += 2; // list_len = 14 (7 algs)
        out[p] = 0x04; out[p + 1] = 0x03; p += 2; // ecdsa_secp256r1_sha256 (0x0403)
        out[p] = 0x08; out[p + 1] = 0x04; p += 2; // rsa_pss_rsae_sha256 (0x0804)
        out[p] = 0x04; out[p + 1] = 0x01; p += 2; // rsa_pkcs1_sha256 (0x0401)
        out[p] = 0x05; out[p + 1] = 0x03; p += 2; // ecdsa_secp384r1_sha384 (0x0503)
        out[p] = 0x08; out[p + 1] = 0x05; p += 2; // rsa_pss_rsae_sha384 (0x0805)
        out[p] = 0x08; out[p + 1] = 0x06; p += 2; // rsa_pss_rsae_sha512 (0x0806)
        out[p] = 0x08; out[p + 1] = 0x07; p += 2; // ed25519 (0x0807)

        // 4. supported_versions (0x002B): TLS 1.3 (0x0304)
        out[p] = 0x00; out[p + 1] = 0x2B; p += 2;
        out[p] = 0x00; out[p + 1] = 0x03; p += 2; // ext_len = 3
        out[p] = 0x02; p += 1;                   // list_len = 2
        out[p] = 0x03; out[p + 1] = 0x04; p += 2; // TLS 1.3

        // 5. key_share (0x0033): Chave Pública Efêmera X25519 (32 bytes)
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

        // Atualiza transcript hash com o ClientHello
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
        let msg_len = ((hs_data[1] as usize) << 16) | ((hs_data[2] as usize) << 8) | (hs_data[3] as usize);
        let hs_end = (4 + msg_len).min(hs_data.len());
        let sh_msg = &hs_data[..hs_end];

        // Server random (detecta HelloRetryRequest magico da RFC 8446)
        self.server_random.copy_from_slice(&sh_msg[6..38]);
        const HRR_RANDOM: [u8; 32] = [
            0xcf, 0x21, 0xad, 0x74, 0xe5, 0x9a, 0x61, 0x11,
            0xbe, 0x1d, 0x8c, 0x02, 0x1e, 0x65, 0xb8, 0x91,
            0xc2, 0xa2, 0x11, 0x16, 0x7a, 0xbb, 0x8c, 0x5e,
            0x07, 0x9e, 0x09, 0xe2, 0xc8, 0xa8, 0x33, 0x9c,
        ];
        if self.server_random == HRR_RANDOM {
            return false;
        }

        let sess_len = sh_msg[38] as usize;
        let mut pos = 39 + sess_len;
        if pos + 3 > sh_msg.len() { return false; }
        let selected_cs = ((sh_msg[pos] as u16) << 8) | (sh_msg[pos + 1] as u16);
        if selected_cs != TLS_CHACHA20_POLY1305_SHA256 && selected_cs != TLS_AES_128_GCM_SHA256 {
            return false;
        }
        self.cipher_suite = selected_cs;
        pos += 3; // cipher suite (2) + compression (1)

        let mut server_pub = [0u8; 32];
        let mut found_ks = false;
        let mut tls13_confirmed = false;

        if pos + 2 <= sh_msg.len() {
            let ext_len = ((sh_msg[pos] as usize) << 8) | (sh_msg[pos + 1] as usize);
            pos += 2;
            let end = pos + ext_len;
            while pos + 4 <= end && pos + 4 <= sh_msg.len() {
                let ext_id = ((sh_msg[pos] as u16) << 8) | (sh_msg[pos + 1] as u16);
                let e_len = ((sh_msg[pos + 2] as usize) << 8) | (sh_msg[pos + 3] as usize);
                pos += 4;
                if pos + e_len > sh_msg.len() { break; }
                if ext_id == 0x002B && e_len >= 2 {
                    let ver = ((sh_msg[pos] as u16) << 8) | (sh_msg[pos + 1] as u16);
                    if ver == TLS13_VERSION {
                        tls13_confirmed = true;
                    }
                } else if ext_id == 0x0033 && e_len >= 36 {
                    let group = ((sh_msg[pos] as u16) << 8) | (sh_msg[pos + 1] as u16);
                    if group == 0x001D {
                        server_pub.copy_from_slice(&sh_msg[pos + 4..pos + 36]);
                        found_ks = true;
                    }
                }
                pos += e_len;
            }
        }

        if !found_ks || !tls13_confirmed { return false; }

        self.transcript.update(sh_msg);

        // Deriva segredo compartilhado X25519
        let shared_secret = x25519(&self.client_priv, &server_pub);

        // Deriva Early Secret e Handshake Secret (RFC 8446 Seção 7.1)
        let early_secret = hkdf_extract(&[0u8; 32], &[0u8; 32]);
        let empty_hash = Sha256::digest(&[]);
        let derived_secret = derive_secret(&early_secret, b"derived", &empty_hash);
        self.handshake_secret = hkdf_extract(&derived_secret, &shared_secret);

        let transcript_hash = self.transcript.current_digest();
        self.client_hs_traffic_secret = derive_secret(&self.handshake_secret, b"c hs traffic", &transcript_hash);
        self.server_hs_traffic_secret = derive_secret(&self.handshake_secret, b"s hs traffic", &transcript_hash);

        let key_len = if self.cipher_suite == TLS_AES_128_GCM_SHA256 { 16 } else { 32 };
        hkdf_expand_label(&self.client_hs_traffic_secret, b"key", &[], &mut self.client_hs_key[..key_len]);
        hkdf_expand_label(&self.client_hs_traffic_secret, b"iv", &[], &mut self.client_hs_iv);
        hkdf_expand_label(&self.server_hs_traffic_secret, b"key", &[], &mut self.server_hs_key[..key_len]);
        hkdf_expand_label(&self.server_hs_traffic_secret, b"iv", &[], &mut self.server_hs_iv);

        self.server_hs_seq = 0;
        self.client_hs_seq = 0;
        self.hs_reasm_len = 0;
        self.has_server_hello = true;
        true
    }

    fn aead_decrypt(&self, key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], buf: &mut [u8], tag: &[u8; 16]) -> bool {
        if self.cipher_suite == TLS_AES_128_GCM_SHA256 {
            let mut k16 = [0u8; 16];
            k16.copy_from_slice(&key[..16]);
            Aes128Gcm::decrypt(&k16, nonce, aad, buf, tag)
        } else {
            ChaCha20Poly1305::decrypt(key, nonce, aad, buf, tag)
        }
    }

    fn aead_encrypt(&self, key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], buf: &mut [u8]) -> [u8; 16] {
        if self.cipher_suite == TLS_AES_128_GCM_SHA256 {
            let mut k16 = [0u8; 16];
            k16.copy_from_slice(&key[..16]);
            Aes128Gcm::encrypt(&k16, nonce, aad, buf)
        } else {
            ChaCha20Poly1305::encrypt(key, nonce, aad, buf)
        }
    }

    /// Processa registros cifrados de Handshake recebidos com reassembly multi-registro
    /// (`EncryptedExtensions`, `Certificate` fragmentado em vários records, `CertificateVerify`, `Finished`).
    pub fn process_handshake_record(&mut self, record: &[u8], client_finished_out: &mut [u8]) -> Result<Option<usize>, &'static str> {
        if record.len() < 5 + 16 + 1 {
            return Err("Tamanho de registro insuficiente");
        }
        let ct_len = ((record[3] as usize) << 8) | (record[4] as usize);
        if 5 + ct_len > record.len() {
            return Err("Registro TLS truncado");
        }

        let mut nonce = self.server_hs_iv;
        let seq_bytes = self.server_hs_seq.to_be_bytes();
        for i in 0..8 { nonce[4 + i] ^= seq_bytes[i]; }
        self.server_hs_seq += 1;

        let pt_and_type_len = ct_len - 16;
        let aad = &record[0..5];
        let mut expected_tag = [0u8; 16];
        expected_tag.copy_from_slice(&record[5 + pt_and_type_len..5 + ct_len]);

        let hs_dec_cap = unsafe { HS_DECRYPT_BUF.len() };
        if pt_and_type_len > hs_dec_cap {
            return Err("Mensagem de handshake excede limite de 18KB");
        }
        let pt_buf = unsafe { &mut HS_DECRYPT_BUF[..pt_and_type_len] };
        pt_buf.copy_from_slice(&record[5..5 + pt_and_type_len]);

        if !self.aead_decrypt(&self.server_hs_key, &nonce, aad, pt_buf, &expected_tag) {
            return Err("Falha de autenticacao AEAD (Poly1305/GCM) no handshake");
        }

        let mut end = pt_and_type_len;
        while end > 0 && pt_buf[end - 1] == 0 { end -= 1; }
        if end == 0 { return Err("Payload do handshake vazio"); }
        let inner_type = pt_buf[end - 1];
        end -= 1;

        if inner_type == CONTENT_ALERT {
            return Err("Alerta TLS cifrado recebido durante handshake");
        }
        if inner_type != CONTENT_HANDSHAKE {
            return Err("Tipo interno nao eh Handshake");
        }

        // Anexa no buffer contínuo de reassembly handshake (suporta mensagens cruzando múltiplos records TLS)
        let reasm = unsafe { &mut HS_REASM_BUF };
        if self.hs_reasm_len + end > reasm.len() {
            return Err("Fluxo acumulado de handshake excede 32KB");
        }
        reasm[self.hs_reasm_len..self.hs_reasm_len + end].copy_from_slice(&pt_buf[..end]);
        self.hs_reasm_len += end;

        let mut pos = 0;
        let mut generated_client_finished = None;

        while pos + 4 <= self.hs_reasm_len {
            let msg_type = reasm[pos];
            let msg_len = ((reasm[pos + 1] as usize) << 16) | ((reasm[pos + 2] as usize) << 8) | (reasm[pos + 3] as usize);
            let full_msg_len = 4 + msg_len;
            if pos + full_msg_len > self.hs_reasm_len {
                // Mensagem continua no(s) próximo(s) registro(s) TLS: aguarda próximo record
                break;
            }

            if msg_type == 20 { // Finished
                if msg_len < 32 {
                    return Err("Finished do servidor invalido");
                }
                let mut server_finished_key = [0u8; 32];
                hkdf_expand_label(&self.server_hs_traffic_secret, b"finished", &[], &mut server_finished_key);
                let transcript_before_sf = self.transcript.current_digest();
                let expected_server_verify_data = hmac_sha256(&server_finished_key, &transcript_before_sf);

                let body = &reasm[pos + 4..pos + full_msg_len];
                let mut diff = 0u8;
                for i in 0..32 {
                    diff |= body[i] ^ expected_server_verify_data[i];
                }
                if diff != 0 {
                    return Err("Server Finished verify_data HMAC rejeitado!");
                }

                // Finished autenticado! Atualiza transcript com a mensagem do servidor
                self.transcript.update(&reasm[pos..pos + full_msg_len]);

                // Transição para Master Secret e chaves de tráfego de aplicação (RFC 8446 Seção 7.1)
                let empty_hash = Sha256::digest(&[]);
                let derived_app = derive_secret(&self.handshake_secret, b"derived", &empty_hash);
                self.master_secret = hkdf_extract(&derived_app, &[0u8; 32]);

                let transcript_after_sf = self.transcript.current_digest();
                self.client_app_traffic_secret = derive_secret(&self.master_secret, b"c ap traffic", &transcript_after_sf);
                self.server_app_traffic_secret = derive_secret(&self.master_secret, b"s ap traffic", &transcript_after_sf);

                let key_len = if self.cipher_suite == TLS_AES_128_GCM_SHA256 { 16 } else { 32 };
                hkdf_expand_label(&self.client_app_traffic_secret, b"key", &[], &mut self.client_app_key[..key_len]);
                hkdf_expand_label(&self.client_app_traffic_secret, b"iv", &[], &mut self.client_app_iv);
                hkdf_expand_label(&self.server_app_traffic_secret, b"key", &[], &mut self.server_app_key[..key_len]);
                hkdf_expand_label(&self.server_app_traffic_secret, b"iv", &[], &mut self.server_app_iv);

                self.client_seq = 0;
                self.server_seq = 0;

                // Constrói Finished do cliente
                let mut client_finished_key = [0u8; 32];
                hkdf_expand_label(&self.client_hs_traffic_secret, b"finished", &[], &mut client_finished_key);
                let client_verify_data = hmac_sha256(&client_finished_key, &transcript_after_sf);

                let mut client_fin_msg = [0u8; 36];
                client_fin_msg[0] = 20; // HandshakeType::Finished
                client_fin_msg[1] = 0; client_fin_msg[2] = 0; client_fin_msg[3] = 32;
                client_fin_msg[4..36].copy_from_slice(&client_verify_data);

                // Atualiza transcript com o Finished do cliente
                self.transcript.update(&client_fin_msg);

                // Criptografa o Finished do cliente com as chaves de handshake
                let mut fin_nonce = self.client_hs_iv;
                let seq_bytes = self.client_hs_seq.to_be_bytes();
                for i in 0..8 { fin_nonce[4 + i] ^= seq_bytes[i]; }
                self.client_hs_seq += 1;

                let out_ct_len = 36 + 1 + 16;
                client_finished_out[0] = CONTENT_APPLICATION_DATA;
                client_finished_out[1] = 0x03; client_finished_out[2] = 0x03;
                client_finished_out[3] = (out_ct_len >> 8) as u8;
                client_finished_out[4] = out_ct_len as u8;

                client_finished_out[5..41].copy_from_slice(&client_fin_msg);
                client_finished_out[41] = CONTENT_HANDSHAKE;

                let fin_aad = [client_finished_out[0], client_finished_out[1], client_finished_out[2], client_finished_out[3], client_finished_out[4]];
                let key_copy = self.client_hs_key;
                let tag = self.aead_encrypt(&key_copy, &fin_nonce, &fin_aad, &mut client_finished_out[5..42]);
                client_finished_out[42..58].copy_from_slice(&tag);

                self.is_established = true;
                generated_client_finished = Some(58);
            } else {
                // EncryptedExtensions (8), Certificate (11), CertificateVerify (15)
                self.transcript.update(&reasm[pos..pos + full_msg_len]);
            }
            pos += full_msg_len;
        }

        if pos > 0 {
            let rem = self.hs_reasm_len - pos;
            if rem > 0 {
                reasm.copy_within(pos..self.hs_reasm_len, 0);
            }
            self.hs_reasm_len = rem;
        }

        Ok(generated_client_finished)
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
        let key_copy = self.client_app_key;
        let tag = self.aead_encrypt(&key_copy, &nonce, &aad, &mut out[5..5 + payload.len() + 1]);
        out[5 + payload.len() + 1..5 + ct_len].copy_from_slice(&tag);

        5 + ct_len
    }

    /// Descriptografa registro TLSCiphertext de aplicação e retorna o inner ContentType decifrado
    pub fn decrypt_application_record(&mut self, record: &[u8], out_payload: &mut [u8]) -> Result<AppRecord, ()> {
        if record.len() < 5 + 16 + 1 { return Err(()); }
        let ct_len = ((record[3] as usize) << 8) | (record[4] as usize);
        if 5 + ct_len > record.len() { return Err(()); }

        let mut nonce = self.server_app_iv;
        let seq_bytes = self.server_seq.to_be_bytes();
        for i in 0..8 { nonce[4 + i] ^= seq_bytes[i]; }
        self.server_seq += 1;

        let pt_and_type_len = ct_len - 16;
        if pt_and_type_len > out_payload.len() { return Err(()); }
        let aad = &record[0..5];
        let mut expected_tag = [0u8; 16];
        expected_tag.copy_from_slice(&record[5 + pt_and_type_len..5 + ct_len]);

        out_payload[..pt_and_type_len].copy_from_slice(&record[5..5 + pt_and_type_len]);

        let key_copy = self.server_app_key;
        let ok = self.aead_decrypt(&key_copy, &nonce, aad, &mut out_payload[..pt_and_type_len], &expected_tag);

        if !ok { return Err(()); }

        let mut end = pt_and_type_len;
        while end > 0 && out_payload[end - 1] == 0 { end -= 1; }
        if end == 0 { return Err(()); }
        let inner_type = out_payload[end - 1];
        end -= 1;

        if inner_type == CONTENT_APPLICATION_DATA {
            Ok(AppRecord::Data(end))
        } else if inner_type == CONTENT_HANDSHAKE {
            // NewSessionTicket post-handshake emitido pelo servidor
            Ok(AppRecord::Handshake)
        } else if inner_type == CONTENT_ALERT {
            let level = if end >= 1 { out_payload[0] } else { 0 };
            let desc = if end >= 2 { out_payload[1] } else { 0 };
            Ok(AppRecord::Alert(level, desc))
        } else {
            Err(())
        }
    }

    /// Descriptografa registro recebido de volta para o buffer
    pub fn decrypt_application_data(&mut self, record: &[u8], out_payload: &mut [u8]) -> Option<usize> {
        match self.decrypt_application_record(record, out_payload) {
            Ok(AppRecord::Data(n)) => Some(n),
            Ok(AppRecord::Handshake) => Some(0),
            Ok(AppRecord::Alert(_, _)) | Err(()) => None,
        }
    }
}

