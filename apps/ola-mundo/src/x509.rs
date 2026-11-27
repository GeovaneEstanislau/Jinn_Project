// SPDX-License-Identifier: GPL-2.0-only OR Jinn-Commercial-1.0
//! Verificador X.509 ASN.1 DER, SAN/CN e CertificateVerify TLS 1.3 em #![no_std] (Zero Alloc)
//!
//! Todos os buffers grandes (modulo RSA ate 4096 bits, lista de SANs e temporarios de exponenciacao)
//! residem em .bss estatico para preservar stack enxuta em Ring 3.

use core::cmp::Ordering;
use crate::tls::Sha256;

const MAX_SAN_ENTRIES: usize = 16;
const MAX_HOST_LEN: usize = 96;
const MAX_RSA_BYTES: usize = 512; // ate RSA-4096
const RSA_LIMBS: usize = 66;      // 64 limbs (4096 bits) + headroom
const RSA_PROD_LIMBS: usize = 132;

// Constantes NIST P-256 em big-endian (32 bytes)
const P256_P_BE: [u8; 32] = [
    0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x01,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF,
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
];
const P256_B_BE: [u8; 32] = [
    0x5A, 0xC6, 0x35, 0xD8, 0xAA, 0x3A, 0x93, 0xE7,
    0xB3, 0xEB, 0xBD, 0x55, 0x76, 0x98, 0x86, 0xBC,
    0x65, 0x1D, 0x06, 0xB0, 0xCC, 0x53, 0xB0, 0xF6,
    0x3B, 0xCE, 0x3C, 0x3E, 0x27, 0xD2, 0x60, 0x4B,
];
const P256_GX_BE: [u8; 32] = [
    0x6B, 0x17, 0xD1, 0xF2, 0xE1, 0x2C, 0x42, 0x47,
    0xF8, 0xBC, 0xE6, 0xE5, 0x63, 0xA4, 0x40, 0xF2,
    0x77, 0x03, 0x7D, 0x81, 0x2D, 0xEB, 0x33, 0xA0,
    0xF4, 0xA1, 0x39, 0x45, 0xD8, 0x98, 0xC2, 0x96,
];
const P256_GY_BE: [u8; 32] = [
    0x4F, 0xE3, 0x42, 0xE2, 0xFE, 0x1A, 0x7F, 0x9B,
    0x8E, 0xE7, 0xEB, 0x4A, 0x7C, 0x0F, 0x9E, 0x16,
    0x2B, 0xCE, 0x33, 0x57, 0x6B, 0x31, 0x5E, 0xCE,
    0xCB, 0xB6, 0x40, 0x68, 0x37, 0xBF, 0x51, 0xF5,
];
const P256_N_BE: [u8; 32] = [
    0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00,
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    0xBC, 0xE6, 0xFA, 0xAD, 0xA7, 0x17, 0x9E, 0x84,
    0xF3, 0xB9, 0xCA, 0xC2, 0xFC, 0x63, 0x25, 0x51,
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CertVerifyError {
    MalformedCertificate,
    UnsupportedPublicKey,
    HostnameMismatch,
    MalformedCertificateVerify,
    UnsupportedSignatureScheme,
    InvalidSignature,
}

impl CertVerifyError {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::MalformedCertificate => "Certificado X.509 DER malformado",
            Self::UnsupportedPublicKey => "Chave publica X.509 nao suportada (ex: Ed25519)",
            Self::HostnameMismatch => "Certificado X.509 rejeitado: SAN/CN divergente do hostname",
            Self::MalformedCertificateVerify => "CertificateVerify TLS 1.3 malformado",
            Self::UnsupportedSignatureScheme => "Esquema de assinatura CertificateVerify nao suportado",
            Self::InvalidSignature => "CertificateVerify: assinatura TLS 1.3 criptograficamente invalida",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LeafKeyKind {
    None,
    Rsa,
    EcdsaP256,
    Unsupported,
}

#[derive(Clone, Copy)]
struct SanEntry {
    is_ip: bool,
    len: usize,
    bytes: [u8; MAX_HOST_LEN],
}

impl SanEntry {
    const fn empty() -> Self {
        Self {
            is_ip: false,
            len: 0,
            bytes: [0u8; MAX_HOST_LEN],
        }
    }
}

struct LeafCertState {
    valid: bool,
    key_kind: LeafKeyKind,
    rsa_mod_limbs: [u64; RSA_LIMBS],
    rsa_mod_bytes: usize,
    rsa_mod_bits: usize,
    rsa_exp_limbs: [u64; 4],
    rsa_exp_len: usize,
    p256_qx: [u64; 4],
    p256_qy: [u64; 4],
    has_san_ext: bool,
    san_count: usize,
    sans: [SanEntry; MAX_SAN_ENTRIES],
    cn_len: usize,
    cn: [u8; MAX_HOST_LEN],
    diag_len: usize,
    diag: [u8; 96],
}

impl LeafCertState {
    const fn empty() -> Self {
        Self {
            valid: false,
            key_kind: LeafKeyKind::None,
            rsa_mod_limbs: [0u64; RSA_LIMBS],
            rsa_mod_bytes: 0,
            rsa_mod_bits: 0,
            rsa_exp_limbs: [0u64; 4],
            rsa_exp_len: 0,
            p256_qx: [0u64; 4],
            p256_qy: [0u64; 4],
            has_san_ext: false,
            san_count: 0,
            sans: [SanEntry::empty(); MAX_SAN_ENTRIES],
            cn_len: 0,
            cn: [0u8; MAX_HOST_LEN],
            diag_len: 0,
            diag: [0u8; 96],
        }
    }

    fn clear(&mut self) {
        self.valid = false;
        self.key_kind = LeafKeyKind::None;
        self.rsa_mod_limbs = [0u64; RSA_LIMBS];
        self.rsa_mod_bytes = 0;
        self.rsa_mod_bits = 0;
        self.rsa_exp_limbs = [0u64; 4];
        self.rsa_exp_len = 0;
        self.p256_qx = [0u64; 4];
        self.p256_qy = [0u64; 4];
        self.has_san_ext = false;
        self.san_count = 0;
        self.cn_len = 0;
        self.diag_len = 0;
    }

    fn set_diag(&mut self, msg: &[u8]) {
        let take = msg.len().min(self.diag.len());
        self.diag[..take].copy_from_slice(&msg[..take]);
        self.diag_len = take;
    }
}

struct RsaWorkspace {
    s_limbs: [u64; RSA_LIMBS],
    base: [u64; RSA_LIMBS],
    res: [u64; RSA_LIMBS],
    next: [u64; RSA_LIMBS],
    prod: [u64; RSA_PROD_LIMBS],
    rem: [u64; RSA_LIMBS],
    em: [u8; MAX_RSA_BYTES],
    db: [u8; MAX_RSA_BYTES],
    mask: [u8; MAX_RSA_BYTES],
}

impl RsaWorkspace {
    const fn new() -> Self {
        Self {
            s_limbs: [0u64; RSA_LIMBS],
            base: [0u64; RSA_LIMBS],
            res: [0u64; RSA_LIMBS],
            next: [0u64; RSA_LIMBS],
            prod: [0u64; RSA_PROD_LIMBS],
            rem: [0u64; RSA_LIMBS],
            em: [0u8; MAX_RSA_BYTES],
            db: [0u8; MAX_RSA_BYTES],
            mask: [0u8; MAX_RSA_BYTES],
        }
    }
}

static mut LEAF_CERT_STATE: LeafCertState = LeafCertState::empty();
static mut RSA_WORK: RsaWorkspace = RsaWorkspace::new();

pub fn last_cert_diag(out: &mut [u8; 96]) -> usize {
    let st = unsafe { &*core::ptr::addr_of!(LEAF_CERT_STATE) };
    let n = st.diag_len.min(96);
    out[..n].copy_from_slice(&st.diag[..n]);
    n
}

// ── ASN.1 DER Reader ──────────────────────────────────────────────────────────

struct DerReader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> DerReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn is_empty(&self) -> bool {
        self.offset >= self.data.len()
    }

    fn peek_tag(&self) -> Option<u8> {
        self.data.get(self.offset).copied()
    }

    fn read_element(&mut self) -> Result<(u8, &'a [u8]), ()> {
        if self.offset >= self.data.len() {
            return Err(());
        }
        let tag = self.data[self.offset];
        self.offset += 1;
        if self.offset >= self.data.len() {
            return Err(());
        }
        let first_len = self.data[self.offset];
        self.offset += 1;
        let len = if first_len & 0x80 == 0 {
            first_len as usize
        } else {
            let count = (first_len & 0x7F) as usize;
            if count == 0 || count > 4 || self.offset + count > self.data.len() {
                return Err(());
            }
            let mut l = 0usize;
            for _ in 0..count {
                l = (l << 8) | (self.data[self.offset] as usize);
                self.offset += 1;
            }
            l
        };
        if self.offset + len > self.data.len() {
            return Err(());
        }
        let slice = &self.data[self.offset..self.offset + len];
        self.offset += len;
        Ok((tag, slice))
    }

    fn read_sequence(&mut self) -> Result<&'a [u8], ()> {
        let (tag, data) = self.read_element()?;
        if tag == 0x30 {
            Ok(data)
        } else {
            Err(())
        }
    }

    fn read_integer(&mut self) -> Result<&'a [u8], ()> {
        let (tag, data) = self.read_element()?;
        if tag == 0x02 && !data.is_empty() {
            Ok(data)
        } else {
            Err(())
        }
    }

    fn read_bit_string(&mut self) -> Result<&'a [u8], ()> {
        let (tag, data) = self.read_element()?;
        if tag == 0x03 && !data.is_empty() {
            Ok(&data[1..])
        } else {
            Err(())
        }
    }
}

// ── Aritmetica Modular Sem Alocacao ────────────────────────────────────────────

fn u256_from_be_slice(bytes: &[u8]) -> Option<[u64; 4]> {
    let mut s = bytes;
    while s.len() > 32 && s[0] == 0 {
        s = &s[1..];
    }
    if s.len() > 32 {
        return None;
    }
    let mut padded = [0u8; 32];
    padded[32 - s.len()..].copy_from_slice(s);
    let mut limbs = [0u64; 4];
    for i in 0..4 {
        let off = 24 - i * 8;
        limbs[i] = u64::from_be_bytes([
            padded[off], padded[off + 1], padded[off + 2], padded[off + 3],
            padded[off + 4], padded[off + 5], padded[off + 6], padded[off + 7],
        ]);
    }
    Some(limbs)
}

#[inline]
fn u256_is_zero(a: &[u64; 4]) -> bool {
    a[0] == 0 && a[1] == 0 && a[2] == 0 && a[3] == 0
}

fn u256_cmp(a: &[u64; 4], b: &[u64; 4]) -> Ordering {
    for i in (0..4).rev() {
        if a[i] != b[i] {
            return a[i].cmp(&b[i]);
        }
    }
    Ordering::Equal
}

fn u5_cmp(a: &[u64; 5], b: &[u64; 4]) -> Ordering {
    if a[4] != 0 {
        return Ordering::Greater;
    }
    for i in (0..4).rev() {
        if a[i] != b[i] {
            return a[i].cmp(&b[i]);
        }
    }
    Ordering::Equal
}

fn u5_sub4(a: &mut [u64; 5], b: &[u64; 4]) {
    let mut borrow = 0u128;
    for i in 0..4 {
        let ai = a[i] as u128;
        let sub = (b[i] as u128) + borrow;
        if ai < sub {
            a[i] = ((ai + (1u128 << 64)) - sub) as u64;
            borrow = 1;
        } else {
            a[i] = (ai - sub) as u64;
            borrow = 0;
        }
    }
    a[4] = a[4].wrapping_sub(borrow as u64);
}

fn u256_mod_add(a: &[u64; 4], b: &[u64; 4], m: &[u64; 4]) -> [u64; 4] {
    let mut sum = [0u64; 5];
    let mut carry = 0u128;
    for i in 0..4 {
        let s = (a[i] as u128) + (b[i] as u128) + carry;
        sum[i] = s as u64;
        carry = s >> 64;
    }
    sum[4] = carry as u64;
    while u5_cmp(&sum, m) != Ordering::Less {
        u5_sub4(&mut sum, m);
    }
    [sum[0], sum[1], sum[2], sum[3]]
}

fn u256_mod_sub(a: &[u64; 4], b: &[u64; 4], m: &[u64; 4]) -> [u64; 4] {
    if u256_cmp(a, b) != Ordering::Less {
        let mut out = [0u64; 4];
        let mut borrow = 0u128;
        for i in 0..4 {
            let ai = a[i] as u128;
            let sub = (b[i] as u128) + borrow;
            if ai < sub {
                out[i] = ((ai + (1u128 << 64)) - sub) as u64;
                borrow = 1;
            } else {
                out[i] = (ai - sub) as u64;
                borrow = 0;
            }
        }
        out
    } else {
        // (a + m) - b
        let mut tmp = [0u64; 5];
        let mut carry = 0u128;
        for i in 0..4 {
            let s = (a[i] as u128) + (m[i] as u128) + carry;
            tmp[i] = s as u64;
            carry = s >> 64;
        }
        tmp[4] = carry as u64;
        u5_sub4(&mut tmp, b);
        [tmp[0], tmp[1], tmp[2], tmp[3]]
    }
}

fn u256_rem_u512(prod: &[u64; 8], m: &[u64; 4]) -> [u64; 4] {
    let mut top = 511isize;
    while top >= 0 {
        let w = (top as usize) / 64;
        let b = (top as usize) % 64;
        if ((prod[w] >> b) & 1) != 0 {
            break;
        }
        top -= 1;
    }
    let mut rem = [0u64; 5];
    if top < 0 {
        return [0u64; 4];
    }
    let mut bit = top as usize;
    loop {
        let carry_bit = (prod[bit / 64] >> (bit % 64)) & 1;
        let mut c = carry_bit;
        for j in 0..5 {
            let next_c = rem[j] >> 63;
            rem[j] = (rem[j] << 1) | c;
            c = next_c;
        }
        if u5_cmp(&rem, m) != Ordering::Less {
            u5_sub4(&mut rem, m);
        }
        if bit == 0 {
            break;
        }
        bit -= 1;
    }
    [rem[0], rem[1], rem[2], rem[3]]
}

fn u256_mod_mul(a: &[u64; 4], b: &[u64; 4], m: &[u64; 4]) -> [u64; 4] {
    let mut prod = [0u64; 8];
    for i in 0..4 {
        let ai = a[i] as u128;
        if ai == 0 {
            continue;
        }
        let mut carry = 0u128;
        for j in 0..4 {
            let cur = (prod[i + j] as u128) + ai * (b[j] as u128) + carry;
            prod[i + j] = cur as u64;
            carry = cur >> 64;
        }
        prod[i + 4] = (prod[i + 4] as u128 + carry) as u64;
    }
    u256_rem_u512(&prod, m)
}

fn u256_mod_mul_small(a: &[u64; 4], k: u64, m: &[u64; 4]) -> [u64; 4] {
    let b = [k, 0, 0, 0];
    u256_mod_mul(a, &b, m)
}

fn u256_sub_u64(a: &[u64; 4], k: u64) -> [u64; 4] {
    let mut out = *a;
    let mut borrow = k as u128;
    for i in 0..4 {
        let ai = out[i] as u128;
        if ai < borrow {
            out[i] = ((ai + (1u128 << 64)) - borrow) as u64;
            borrow = 1;
        } else {
            out[i] = (ai - borrow) as u64;
            borrow = 0;
        }
    }
    out
}

fn u256_bit_len(a: &[u64; 4]) -> usize {
    for i in (0..4).rev() {
        if a[i] != 0 {
            return i * 64 + (64 - a[i].leading_zeros() as usize);
        }
    }
    0
}

fn u256_mod_exp(base: &[u64; 4], exp: &[u64; 4], m: &[u64; 4]) -> [u64; 4] {
    let mut b = u256_rem_u512(&[base[0], base[1], base[2], base[3], 0, 0, 0, 0], m);
    let mut r = [1u64, 0, 0, 0];
    let blen = u256_bit_len(exp);
    for i in 0..blen {
        if ((exp[i / 64] >> (i % 64)) & 1) != 0 {
            r = u256_mod_mul(&r, &b, m);
        }
        if i + 1 < blen {
            b = u256_mod_mul(&b, &b, m);
        }
    }
    r
}

// ── Curva Eliptica NIST P-256 (Coordenadas Jacobianas sem Alocacao) ───────────

#[derive(Clone, Copy)]
struct P256Jac {
    x: [u64; 4],
    y: [u64; 4],
    z: [u64; 4],
    inf: bool,
}

impl P256Jac {
    const fn inf() -> Self {
        Self {
            x: [0; 4],
            y: [0; 4],
            z: [0; 4],
            inf: true,
        }
    }

    fn from_aff(ax: &[u64; 4], ay: &[u64; 4], a_inf: bool) -> Self {
        if a_inf {
            Self::inf()
        } else {
            Self {
                x: *ax,
                y: *ay,
                z: [1, 0, 0, 0],
                inf: false,
            }
        }
    }

    fn double(&self, p: &[u64; 4]) -> Self {
        if self.inf || u256_is_zero(&self.y) {
            return Self::inf();
        }
        let delta = u256_mod_mul(&self.z, &self.z, p);
        let gamma = u256_mod_mul(&self.y, &self.y, p);
        let beta = u256_mod_mul(&self.x, &gamma, p);
        let x_sub_d = u256_mod_sub(&self.x, &delta, p);
        let x_add_d = u256_mod_add(&self.x, &delta, p);
        let term = u256_mod_mul(&x_sub_d, &x_add_d, p);
        let alpha = u256_mod_mul_small(&term, 3, p);

        let alpha_sq = u256_mod_mul(&alpha, &alpha, p);
        let eight_beta = u256_mod_mul_small(&beta, 8, p);
        let xp = u256_mod_sub(&alpha_sq, &eight_beta, p);

        let two_y = u256_mod_mul_small(&self.y, 2, p);
        let zp = u256_mod_mul(&two_y, &self.z, p);

        let four_beta = u256_mod_mul_small(&beta, 4, p);
        let diff = u256_mod_sub(&four_beta, &xp, p);
        let term2 = u256_mod_mul(&alpha, &diff, p);
        let gamma_sq = u256_mod_mul(&gamma, &gamma, p);
        let eight_gamma_sq = u256_mod_mul_small(&gamma_sq, 8, p);
        let yp = u256_mod_sub(&term2, &eight_gamma_sq, p);

        Self { x: xp, y: yp, z: zp, inf: false }
    }

    fn add_affine(&self, qx: &[u64; 4], qy: &[u64; 4], p: &[u64; 4]) -> Self {
        if self.inf {
            return Self::from_aff(qx, qy, false);
        }
        let z1_sq = u256_mod_mul(&self.z, &self.z, p);
        let z1_cube = u256_mod_mul(&z1_sq, &self.z, p);
        let u2 = u256_mod_mul(qx, &z1_sq, p);
        let s2 = u256_mod_mul(qy, &z1_cube, p);

        let h = u256_mod_sub(&u2, &self.x, p);
        let r = u256_mod_sub(&s2, &self.y, p);
        if u256_is_zero(&h) {
            if u256_is_zero(&r) {
                return self.double(p);
            }
            return Self::inf();
        }
        let h_sq = u256_mod_mul(&h, &h, p);
        let h_cube = u256_mod_mul(&h_sq, &h, p);
        let u1_h_sq = u256_mod_mul(&self.x, &h_sq, p);

        let r_sq = u256_mod_mul(&r, &r, p);
        let two_u1_h_sq = u256_mod_mul_small(&u1_h_sq, 2, p);
        let sub1 = u256_mod_sub(&r_sq, &h_cube, p);
        let xp = u256_mod_sub(&sub1, &two_u1_h_sq, p);

        let diff = u256_mod_sub(&u1_h_sq, &xp, p);
        let term1 = u256_mod_mul(&r, &diff, p);
        let term2 = u256_mod_mul(&self.y, &h_cube, p);
        let yp = u256_mod_sub(&term1, &term2, p);
        let zp = u256_mod_mul(&self.z, &h, p);

        Self { x: xp, y: yp, z: zp, inf: false }
    }

    fn to_affine(&self, p: &[u64; 4]) -> Option<([u64; 4], [u64; 4])> {
        if self.inf || u256_is_zero(&self.z) {
            return None;
        }
        let p_minus_2 = u256_sub_u64(p, 2);
        let z_inv = u256_mod_exp(&self.z, &p_minus_2, p);
        let z_inv_2 = u256_mod_mul(&z_inv, &z_inv, p);
        let z_inv_3 = u256_mod_mul(&z_inv_2, &z_inv, p);
        Some((u256_mod_mul(&self.x, &z_inv_2, p), u256_mod_mul(&self.y, &z_inv_3, p)))
    }
}

fn p256_scalar_mul(k: &[u64; 4], ax: &[u64; 4], ay: &[u64; 4], p: &[u64; 4]) -> Option<([u64; 4], [u64; 4])> {
    if u256_is_zero(k) {
        return None;
    }
    let mut res = P256Jac::inf();
    let bit_len = u256_bit_len(k);
    for i in (0..bit_len).rev() {
        res = res.double(p);
        if ((k[i / 64] >> (i % 64)) & 1) != 0 {
            res = res.add_affine(ax, ay, p);
        }
    }
    res.to_affine(p)
}

fn p256_validate_point(bytes: &[u8]) -> Option<([u64; 4], [u64; 4])> {
    if bytes.len() != 65 || bytes[0] != 0x04 {
        return None;
    }
    let p = u256_from_be_slice(&P256_P_BE)?;
    let b = u256_from_be_slice(&P256_B_BE)?;
    let x = u256_from_be_slice(&bytes[1..33])?;
    let y = u256_from_be_slice(&bytes[33..65])?;
    if u256_cmp(&x, &p) != Ordering::Less || u256_cmp(&y, &p) != Ordering::Less {
        return None;
    }
    let y_sq = u256_mod_mul(&y, &y, &p);
    let x_cube = u256_mod_mul(&u256_mod_mul(&x, &x, &p), &x, &p);
    let three_x = u256_mod_mul_small(&x, 3, &p);
    let rhs = u256_mod_add(&u256_mod_sub(&x_cube, &three_x, &p), &b, &p);
    if y_sq == rhs {
        Some((x, y))
    } else {
        None
    }
}

fn verify_p256_ecdsa_der(qx: &[u64; 4], qy: &[u64; 4], hash: &[u8; 32], der_sig: &[u8]) -> Result<(), CertVerifyError> {
    let mut rd = DerReader::new(der_sig);
    let seq = rd.read_sequence().map_err(|_| CertVerifyError::MalformedCertificateVerify)?;
    let mut sr = DerReader::new(seq);
    let r_raw = sr.read_integer().map_err(|_| CertVerifyError::MalformedCertificateVerify)?;
    let s_raw = sr.read_integer().map_err(|_| CertVerifyError::MalformedCertificateVerify)?;

    let r = u256_from_be_slice(r_raw).ok_or(CertVerifyError::MalformedCertificateVerify)?;
    let s = u256_from_be_slice(s_raw).ok_or(CertVerifyError::MalformedCertificateVerify)?;

    let p = u256_from_be_slice(&P256_P_BE).unwrap();
    let n = u256_from_be_slice(&P256_N_BE).unwrap();
    let gx = u256_from_be_slice(&P256_GX_BE).unwrap();
    let gy = u256_from_be_slice(&P256_GY_BE).unwrap();

    if u256_is_zero(&r) || u256_cmp(&r, &n) != Ordering::Less {
        return Err(CertVerifyError::InvalidSignature);
    }
    if u256_is_zero(&s) || u256_cmp(&s, &n) != Ordering::Less {
        return Err(CertVerifyError::InvalidSignature);
    }

    let n_minus_2 = u256_sub_u64(&n, 2);
    let w = u256_mod_exp(&s, &n_minus_2, &n);
    let e_raw = u256_from_be_slice(hash).unwrap();
    let e = u256_rem_u512(&[e_raw[0], e_raw[1], e_raw[2], e_raw[3], 0, 0, 0, 0], &n);
    let u1 = u256_mod_mul(&e, &w, &n);
    let u2 = u256_mod_mul(&r, &w, &n);

    let p1 = p256_scalar_mul(&u1, &gx, &gy, &p);
    let p2 = p256_scalar_mul(&u2, qx, qy, &p);
    let sum_aff = match (p1, p2) {
        (None, None) => return Err(CertVerifyError::InvalidSignature),
        (Some(a), None) | (None, Some(a)) => a,
        (Some((x1, y1)), Some((x2, y2))) => {
            if x1 == x2 && u256_mod_add(&y1, &y2, &p) == [0; 4] {
                return Err(CertVerifyError::InvalidSignature);
            }
            let jac = P256Jac::from_aff(&x1, &y1, false).add_affine(&x2, &y2, &p);
            jac.to_affine(&p).ok_or(CertVerifyError::InvalidSignature)?
        }
    };

    let rx_mod_n = u256_rem_u512(&[sum_aff.0[0], sum_aff.0[1], sum_aff.0[2], sum_aff.0[3], 0, 0, 0, 0], &n);
    if rx_mod_n == r {
        Ok(())
    } else {
        Err(CertVerifyError::InvalidSignature)
    }
}

// ── Aritmetica RSA (Ate 4096 bits em .bss) ────────────────────────────────────

fn limbs_from_be(bytes: &[u8], out: &mut [u64]) -> Result<(usize, usize), ()> {
    let mut s = bytes;
    while s.len() > 1 && s[0] == 0 {
        s = &s[1..];
    }
    if s.len() > out.len() * 8 {
        return Err(());
    }
    for l in out.iter_mut() {
        *l = 0;
    }
    for (idx, &b) in s.iter().rev().enumerate() {
        out[idx / 8] |= (b as u64) << ((idx % 8) * 8);
    }
    let bit_len = limbs_bit_length(out);
    Ok((s.len(), bit_len))
}

fn limbs_bit_length(a: &[u64]) -> usize {
    for i in (0..a.len()).rev() {
        if a[i] != 0 {
            return i * 64 + (64 - a[i].leading_zeros() as usize);
        }
    }
    0
}

fn limbs_cmp(a: &[u64], b: &[u64]) -> Ordering {
    let len = a.len().max(b.len());
    for i in (0..len).rev() {
        let av = if i < a.len() { a[i] } else { 0 };
        let bv = if i < b.len() { b[i] } else { 0 };
        if av != bv {
            return av.cmp(&bv);
        }
    }
    Ordering::Equal
}

fn limbs_sub_in_place(a: &mut [u64], b: &[u64]) {
    let mut borrow = 0u128;
    for i in 0..a.len() {
        let ai = a[i] as u128;
        let bi = if i < b.len() { b[i] as u128 } else { 0 };
        let sub = bi + borrow;
        if ai < sub {
            a[i] = ((ai + (1u128 << 64)) - sub) as u64;
            borrow = 1;
        } else {
            a[i] = (ai - sub) as u64;
            borrow = 0;
        }
    }
}

unsafe fn rsa_mul_mod_into_next(a: &[u64; RSA_LIMBS], b: &[u64; RSA_LIMBS], m: &[u64; RSA_LIMBS]) {
    let work = &mut *core::ptr::addr_of_mut!(RSA_WORK);
    work.prod = [0u64; RSA_PROD_LIMBS];
    let a_words = (limbs_bit_length(a) + 63) / 64;
    let b_words = (limbs_bit_length(b) + 63) / 64;

    for i in 0..a_words {
        let ai = a[i] as u128;
        if ai == 0 {
            continue;
        }
        let mut carry = 0u128;
        for j in 0..b_words {
            let cur = (work.prod[i + j] as u128) + ai * (b[j] as u128) + carry;
            work.prod[i + j] = cur as u64;
            carry = cur >> 64;
        }
        work.prod[i + b_words] = (work.prod[i + b_words] as u128 + carry) as u64;
    }

    work.rem = [0u64; RSA_LIMBS];
    let top_bit = limbs_bit_length(&work.prod);
    if top_bit > 0 {
        let mut bit = top_bit - 1;
        loop {
            let mut c = (work.prod[bit / 64] >> (bit % 64)) & 1;
            for j in 0..RSA_LIMBS {
                let nc = work.rem[j] >> 63;
                work.rem[j] = (work.rem[j] << 1) | c;
                c = nc;
            }
            if limbs_cmp(&work.rem, m) != Ordering::Less {
                limbs_sub_in_place(&mut work.rem, m);
            }
            if bit == 0 {
                break;
            }
            bit -= 1;
        }
    }
    work.next = work.rem;
}

unsafe fn rsa_mod_exp_to_em(sig: &[u8], mod_n: &[u64; RSA_LIMBS], exp_e: &[u64; 4], k_bytes: usize) -> Result<(), CertVerifyError> {
    if k_bytes > MAX_RSA_BYTES || sig.len() != k_bytes {
        return Err(CertVerifyError::InvalidSignature);
    }
    let work = &mut *core::ptr::addr_of_mut!(RSA_WORK);
    if limbs_from_be(sig, &mut work.s_limbs).is_err() {
        return Err(CertVerifyError::MalformedCertificateVerify);
    }
    if limbs_cmp(&work.s_limbs, mod_n) != Ordering::Less {
        return Err(CertVerifyError::InvalidSignature);
    }
    work.base = work.s_limbs;
    work.res = [0u64; RSA_LIMBS];
    work.res[0] = 1;

    let exp_bits = limbs_bit_length(exp_e);
    if exp_bits == 0 {
        return Err(CertVerifyError::UnsupportedPublicKey);
    }

    for i in 0..exp_bits {
        if ((exp_e[i / 64] >> (i % 64)) & 1) != 0 {
            let cur_res = work.res;
            let cur_base = work.base;
            rsa_mul_mod_into_next(&cur_res, &cur_base, mod_n);
            work.res = work.next;
        }
        if i + 1 < exp_bits {
            let cur_base = work.base;
            rsa_mul_mod_into_next(&cur_base, &cur_base, mod_n);
            work.base = work.next;
        }
    }

    // Serializa work.res em k_bytes big-endian
    work.em = [0u8; MAX_RSA_BYTES];
    for idx in 0..k_bytes {
        let limb = work.res[idx / 8];
        let b = ((limb >> ((idx % 8) * 8)) & 0xFF) as u8;
        work.em[k_bytes - 1 - idx] = b;
    }
    Ok(())
}

fn verify_rsa_pkcs1v15_sha256(hash: &[u8; 32], sig: &[u8]) -> Result<(), CertVerifyError> {
    let st = unsafe { &*core::ptr::addr_of!(LEAF_CERT_STATE) };
    let k = st.rsa_mod_bytes;
    let mod_n = st.rsa_mod_limbs;
    let exp_e = st.rsa_exp_limbs;
    unsafe { rsa_mod_exp_to_em(sig, &mod_n, &exp_e, k)? };
    let em = unsafe { &(&(*core::ptr::addr_of!(RSA_WORK)).em)[..k] };

    const DIGEST_INFO_PREFIX: [u8; 19] = [
        0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01,
        0x65, 0x03, 0x04, 0x02, 0x01, 0x05, 0x00, 0x04, 0x20,
    ];
    let t_len = 19 + 32;
    if k < t_len + 11 || em[0] != 0x00 || em[1] != 0x01 {
        return Err(CertVerifyError::InvalidSignature);
    }
    let ps_end = k - t_len - 1;
    for &b in &em[2..ps_end] {
        if b != 0xFF {
            return Err(CertVerifyError::InvalidSignature);
        }
    }
    if em[ps_end] != 0x00 {
        return Err(CertVerifyError::InvalidSignature);
    }
    let dinfo = &em[ps_end + 1..];
    if dinfo.len() != t_len || dinfo[..19] != DIGEST_INFO_PREFIX || dinfo[19..] != hash[..] {
        return Err(CertVerifyError::InvalidSignature);
    }
    Ok(())
}

fn verify_rsa_pss_sha256(hash: &[u8; 32], sig: &[u8]) -> Result<(), CertVerifyError> {
    let st = unsafe { &*core::ptr::addr_of!(LEAF_CERT_STATE) };
    let k = st.rsa_mod_bytes;
    let mod_bits = st.rsa_mod_bits;
    let mod_n = st.rsa_mod_limbs;
    let exp_e = st.rsa_exp_limbs;
    unsafe { rsa_mod_exp_to_em(sig, &mod_n, &exp_e, k)? };

    let work = unsafe { &mut *core::ptr::addr_of_mut!(RSA_WORK) };
    let em = &work.em[..k];
    let h_len = 32usize;
    let s_len = 32usize;
    if k < h_len + s_len + 2 || em[k - 1] != 0xBC {
        return Err(CertVerifyError::InvalidSignature);
    }
    let db_len = k - h_len - 1;
    let masked_db = &em[..db_len];
    let h = &em[db_len..k - 1];

    // MGF1 com SHA-256 sobre H para work.mask[..db_len]
    let mut counter: u32 = 0;
    let mut off = 0usize;
    while off < db_len {
        let mut s_ctx = Sha256::new();
        s_ctx.update(h);
        s_ctx.update(&counter.to_be_bytes());
        let blk = s_ctx.finalize();
        let take = blk.len().min(db_len - off);
        work.mask[off..off + take].copy_from_slice(&blk[..take]);
        off += take;
        counter += 1;
    }

    for i in 0..db_len {
        work.db[i] = masked_db[i] ^ work.mask[i];
    }

    let unused_bits = (8 * k).saturating_sub(mod_bits.saturating_sub(1));
    if unused_bits > 0 && unused_bits < 8 {
        let mask_top = 0xFFu8 >> unused_bits;
        if (work.db[0] & !mask_top) != 0 {
            return Err(CertVerifyError::InvalidSignature);
        }
    }

    let mut split_idx = None;
    for i in 0..db_len {
        if work.db[i] == 0x01 {
            split_idx = Some(i);
            break;
        } else if work.db[i] != 0x00 {
            return Err(CertVerifyError::InvalidSignature);
        }
    }
    let one_idx = split_idx.ok_or(CertVerifyError::InvalidSignature)?;
    let salt = &work.db[one_idx + 1..db_len];

    let mut m_ctx = Sha256::new();
    m_ctx.update(&[0u8; 8]);
    m_ctx.update(hash);
    m_ctx.update(salt);
    let h_prime = m_ctx.finalize();
    if &h_prime != h {
        return Err(CertVerifyError::InvalidSignature);
    }
    Ok(())
}

// ── Parser de Certificado X.509 & Correspondencia SAN / CN ────────────────────

fn extract_cn_from_dn(dn_bytes: &[u8], out: &mut [u8; MAX_HOST_LEN]) -> usize {
    let mut rd = DerReader::new(dn_bytes);
    while !rd.is_empty() {
        if let Ok((tag, set_data)) = rd.read_element() {
            if tag == 0x31 {
                let mut set_rd = DerReader::new(set_data);
                if let Ok(seq) = set_rd.read_sequence() {
                    let mut atv = DerReader::new(seq);
                    if let Ok((oid_tag, oid)) = atv.read_element() {
                        if let Ok((_, val)) = atv.read_element() {
                            if oid_tag == 0x06 && oid == [0x55, 0x04, 0x03] {
                                let n = val.len().min(MAX_HOST_LEN);
                                out[..n].copy_from_slice(&val[..n]);
                                return n;
                            }
                        }
                    }
                }
            }
        } else {
            break;
        }
    }
    0
}

fn format_ipv4_into(ip: &[u8; 4], out: &mut [u8; MAX_HOST_LEN]) -> usize {
    let mut pos = 0usize;
    for idx in 0..4 {
        if idx > 0 {
            out[pos] = b'.';
            pos += 1;
        }
        let mut v = ip[idx];
        if v >= 100 {
            out[pos] = b'0' + (v / 100);
            pos += 1;
            v %= 100;
            out[pos] = b'0' + (v / 10);
            pos += 1;
            out[pos] = b'0' + (v % 10);
            pos += 1;
        } else if v >= 10 {
            out[pos] = b'0' + (v / 10);
            pos += 1;
            out[pos] = b'0' + (v % 10);
            pos += 1;
        } else {
            out[pos] = b'0' + v;
            pos += 1;
        }
    }
    pos
}

fn parse_spki_into_state(spki_bytes: &[u8], st: &mut LeafCertState) {
    let mut rd = DerReader::new(spki_bytes);
    let alg_bytes = match rd.read_sequence() {
        Ok(b) => b,
        Err(_) => {
            st.key_kind = LeafKeyKind::Unsupported;
            return;
        }
    };
    let key_bits = match rd.read_bit_string() {
        Ok(b) => b,
        Err(_) => {
            st.key_kind = LeafKeyKind::Unsupported;
            return;
        }
    };

    let mut alg_rd = DerReader::new(alg_bytes);
    let (oid_tag, oid) = match alg_rd.read_element() {
        Ok(v) => v,
        Err(_) => {
            st.key_kind = LeafKeyKind::Unsupported;
            return;
        }
    };

    // rsaEncryption: 1.2.840.113549.1.1.1
    if oid_tag == 0x06 && oid == [0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01] {
        let mut krd = DerReader::new(key_bits);
        if let Ok(seq) = krd.read_sequence() {
            let mut srd = DerReader::new(seq);
            if let (Ok(m_bytes), Ok(e_bytes)) = (srd.read_integer(), srd.read_integer()) {
                let mut m_clean = m_bytes;
                while m_clean.len() > 1 && m_clean[0] == 0 {
                    m_clean = &m_clean[1..];
                }
                if !m_clean.is_empty() && m_clean.len() <= MAX_RSA_BYTES {
                    if let Ok((mod_bytes_n, mod_bits_n)) = limbs_from_be(m_clean, &mut st.rsa_mod_limbs) {
                        if let Ok((exp_len_n, exp_bits)) = limbs_from_be(e_bytes, &mut st.rsa_exp_limbs) {
                            if mod_bits_n >= 1024 && exp_bits > 0 {
                                st.rsa_mod_bytes = mod_bytes_n;
                                st.rsa_mod_bits = mod_bits_n;
                                st.rsa_exp_len = exp_len_n;
                                st.key_kind = LeafKeyKind::Rsa;
                                return;
                            }
                        }
                    }
                }
            }
        }
        st.key_kind = LeafKeyKind::Unsupported;
        return;
    }

    // id-ecPublicKey: 1.2.840.10045.2.1
    if oid_tag == 0x06 && oid == [0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x02, 0x01] {
        // Verifica curva nomeada se informada: secp256r1 (1.2.840.10045.3.1.7)
        if let Ok((param_tag, param_oid)) = alg_rd.read_element() {
            if param_tag == 0x06 && param_oid != [0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x03, 0x01, 0x07] {
                st.key_kind = LeafKeyKind::Unsupported;
                return;
            }
        }
        if let Some((qx, qy)) = p256_validate_point(key_bits) {
            st.p256_qx = qx;
            st.p256_qy = qy;
            st.key_kind = LeafKeyKind::EcdsaP256;
            return;
        }
        st.key_kind = LeafKeyKind::Unsupported;
        return;
    }

    st.key_kind = LeafKeyKind::Unsupported;
}

fn parse_leaf_x509(der: &[u8], st: &mut LeafCertState) -> Result<(), CertVerifyError> {
    st.clear();
    let mut outer = DerReader::new(der);
    let cert_seq = outer.read_sequence().map_err(|_| CertVerifyError::MalformedCertificate)?;
    let mut cert_rd = DerReader::new(cert_seq);

    let tbs_seq = cert_rd.read_sequence().map_err(|_| CertVerifyError::MalformedCertificate)?;
    let mut tbs = DerReader::new(tbs_seq);

    // version [0] EXPLICIT optional
    if tbs.peek_tag() == Some(0xA0) {
        let _ = tbs.read_element().map_err(|_| CertVerifyError::MalformedCertificate)?;
    }
    // serialNumber
    let _ = tbs.read_element().map_err(|_| CertVerifyError::MalformedCertificate)?;
    // signature AlgorithmIdentifier
    let _ = tbs.read_element().map_err(|_| CertVerifyError::MalformedCertificate)?;
    // issuer
    let _ = tbs.read_sequence().map_err(|_| CertVerifyError::MalformedCertificate)?;
    // validity
    let _ = tbs.read_sequence().map_err(|_| CertVerifyError::MalformedCertificate)?;
    // subject DN -> CN fallback
    let subject_bytes = tbs.read_sequence().map_err(|_| CertVerifyError::MalformedCertificate)?;
    st.cn_len = extract_cn_from_dn(subject_bytes, &mut st.cn);
    // subjectPublicKeyInfo
    let spki_bytes = tbs.read_sequence().map_err(|_| CertVerifyError::MalformedCertificate)?;
    parse_spki_into_state(spki_bytes, st);

    // optional issuerUniqueID / subjectUniqueID / extensions [3]
    while !tbs.is_empty() {
        let (tag, elem_data) = match tbs.read_element() {
            Ok(v) => v,
            Err(_) => break,
        };
        if tag == 0xA3 {
            if let Ok(ext_seq) = DerReader::new(elem_data).read_sequence() {
                let mut ext_rd = DerReader::new(ext_seq);
                while !ext_rd.is_empty() {
                    let ext_item = match ext_rd.read_sequence() {
                        Ok(s) => s,
                        Err(_) => break,
                    };
                    let mut erd = DerReader::new(ext_item);
                    if let Ok((otag, oid)) = erd.read_element() {
                        if otag == 0x06 && oid == [0x55, 0x1D, 0x11] {
                            // SubjectAlternativeName (2.5.29.17)
                            st.has_san_ext = true;
                            while !erd.is_empty() {
                                if let Ok((vtag, vbytes)) = erd.read_element() {
                                    if vtag == 0x04 {
                                        if let Ok(san_seq) = DerReader::new(vbytes).read_sequence() {
                                            let mut san_rd = DerReader::new(san_seq);
                                            while !san_rd.is_empty() && st.san_count < MAX_SAN_ENTRIES {
                                                if let Ok((gtag, gdata)) = san_rd.read_element() {
                                                    if gtag == 0x82 && !gdata.is_empty() && gdata.len() <= MAX_HOST_LEN {
                                                        let slot = &mut st.sans[st.san_count];
                                                        slot.is_ip = false;
                                                        slot.len = gdata.len();
                                                        slot.bytes[..gdata.len()].copy_from_slice(gdata);
                                                        st.san_count += 1;
                                                    } else if gtag == 0x87 && gdata.len() == 4 {
                                                        let slot = &mut st.sans[st.san_count];
                                                        slot.is_ip = true;
                                                        let ip4 = [gdata[0], gdata[1], gdata[2], gdata[3]];
                                                        slot.len = format_ipv4_into(&ip4, &mut slot.bytes);
                                                        st.san_count += 1;
                                                    }
                                                } else {
                                                    break;
                                                }
                                            }
                                        }
                                        break;
                                    }
                                } else {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    st.valid = true;
    Ok(())
}

fn trim_trailing_dot(s: &[u8]) -> &[u8] {
    if s.len() > 1 && s[s.len() - 1] == b'.' {
        &s[..s.len() - 1]
    } else {
        s
    }
}

fn is_ipv4_literal(host: &[u8]) -> bool {
    if host.is_empty() {
        return false;
    }
    let mut dots = 0usize;
    for &b in host {
        if b == b'.' {
            dots += 1;
        } else if !b.is_ascii_digit() {
            return false;
        }
    }
    dots == 3
}

/// Correspondencia de hostname RFC 6125 com curinga de rotulo unico a esquerda (`*.exemplo.com`)
pub fn hostname_matches_pattern(host: &[u8], pattern: &[u8]) -> bool {
    let h = trim_trailing_dot(host);
    let p = trim_trailing_dot(pattern);
    if h.is_empty() || p.is_empty() {
        return false;
    }
    if h.eq_ignore_ascii_case(p) {
        return true;
    }
    if is_ipv4_literal(h) {
        return false;
    }
    // Wildcard apenas para rotulo inicial exatamente "*."
    if p.len() > 3 && p[0] == b'*' && p[1] == b'.' {
        let suffix = &p[1..]; // ".exemplo.com"
        // Rejeita curinga em TLD amplo (ex: "*.com")
        if !suffix[1..].contains(&b'.') {
            return false;
        }
        if h.len() > suffix.len() {
            let prefix_len = h.len() - suffix.len();
            let first_label = &h[..prefix_len];
            let host_tail = &h[prefix_len..];
            if !first_label.is_empty()
                && !first_label.contains(&b'.')
                && host_tail.eq_ignore_ascii_case(suffix)
            {
                return true;
            }
        }
    }
    false
}

fn check_hostname_against_state(st: &mut LeafCertState, expected_host: &[u8]) -> bool {
    if expected_host.is_empty() {
        return true;
    }
    if st.has_san_ext {
        for i in 0..st.san_count {
            let entry = &st.sans[i];
            if hostname_matches_pattern(expected_host, &entry.bytes[..entry.len]) {
                return true;
            }
        }
        if st.san_count > 0 {
            let mut tmp = [0u8; MAX_HOST_LEN];
            let n = st.sans[0].len;
            tmp[..n].copy_from_slice(&st.sans[0].bytes[..n]);
            st.set_diag(&tmp[..n]);
        } else {
            st.set_diag(b"(SAN vazio)");
        }
        false
    } else if st.cn_len > 0 {
        if hostname_matches_pattern(expected_host, &st.cn[..st.cn_len]) {
            true
        } else {
            let mut tmp = [0u8; MAX_HOST_LEN];
            let n = st.cn_len;
            tmp[..n].copy_from_slice(&st.cn[..n]);
            st.set_diag(&tmp[..n]);
            false
        }
    } else {
        st.set_diag(b"(sem SAN e sem CN)");
        false
    }
}

/// Decodifica mensagem TLS 1.3 `Certificate` (tipo 11), extrai SPKI do folha e valida SAN/CN
pub fn process_tls13_certificate_msg(cert_msg: &[u8], expected_host: &[u8]) -> Result<(), CertVerifyError> {
    if cert_msg.len() < 8 || cert_msg[0] != 11 {
        return Err(CertVerifyError::MalformedCertificate);
    }
    let body = &cert_msg[4..];
    if body.len() < 4 {
        return Err(CertVerifyError::MalformedCertificate);
    }
    let req_ctx_len = body[0] as usize;
    let mut off = 1 + req_ctx_len;
    if off + 3 > body.len() {
        return Err(CertVerifyError::MalformedCertificate);
    }
    let list_len = ((body[off] as usize) << 16) | ((body[off + 1] as usize) << 8) | (body[off + 2] as usize);
    off += 3;
    if list_len < 5 || off + list_len > body.len() {
        return Err(CertVerifyError::MalformedCertificate);
    }

    let cert0_len = ((body[off] as usize) << 16) | ((body[off + 1] as usize) << 8) | (body[off + 2] as usize);
    off += 3;
    if cert0_len == 0 || off + cert0_len > body.len() {
        return Err(CertVerifyError::MalformedCertificate);
    }
    let cert0_der = &body[off..off + cert0_len];

    let st = unsafe { &mut *core::ptr::addr_of_mut!(LEAF_CERT_STATE) };
    parse_leaf_x509(cert0_der, st)?;

    if st.key_kind == LeafKeyKind::Unsupported || st.key_kind == LeafKeyKind::None {
        return Err(CertVerifyError::UnsupportedPublicKey);
    }
    if !check_hostname_against_state(st, expected_host) {
        return Err(CertVerifyError::HostnameMismatch);
    }
    Ok(())
}

/// Verifica assinatura TLS 1.3 `CertificateVerify` (RFC 8446 Secao 4.4.3) usando SPKI do certificado folha
pub fn verify_tls13_certificate_verify(cv_msg: &[u8], transcript_before_cv: &[u8; 32]) -> Result<u16, CertVerifyError> {
    if cv_msg.len() < 8 || cv_msg[0] != 15 {
        return Err(CertVerifyError::MalformedCertificateVerify);
    }
    let body = &cv_msg[4..];
    if body.len() < 4 {
        return Err(CertVerifyError::MalformedCertificateVerify);
    }
    let sig_scheme = u16::from_be_bytes([body[0], body[1]]);
    let sig_len = ((body[2] as usize) << 8) | (body[3] as usize);
    if sig_len == 0 || body.len() < 4 + sig_len {
        return Err(CertVerifyError::MalformedCertificateVerify);
    }
    let sig_bytes = &body[4..4 + sig_len];

    let st = unsafe { &*core::ptr::addr_of!(LEAF_CERT_STATE) };
    if !st.valid {
        return Err(CertVerifyError::MalformedCertificate);
    }

    // RFC 8446 Secao 4.4.3: 64 * 0x20 || "TLS 1.3, server CertificateVerify" || 0x00 || TranscriptHash
    let mut signing_input = [0u8; 64 + 33 + 1 + 32];
    for i in 0..64 {
        signing_input[i] = 0x20;
    }
    signing_input[64..97].copy_from_slice(b"TLS 1.3, server CertificateVerify");
    signing_input[97] = 0x00;
    signing_input[98..130].copy_from_slice(transcript_before_cv);

    let msg_hash = Sha256::digest(&signing_input);

    match (sig_scheme, st.key_kind) {
        (0x0403, LeafKeyKind::EcdsaP256) => {
            verify_p256_ecdsa_der(&st.p256_qx, &st.p256_qy, &msg_hash, sig_bytes)?;
            Ok(sig_scheme)
        }
        (0x0804, LeafKeyKind::Rsa) => {
            verify_rsa_pss_sha256(&msg_hash, sig_bytes)?;
            Ok(sig_scheme)
        }
        (0x0401, LeafKeyKind::Rsa) => {
            verify_rsa_pkcs1v15_sha256(&msg_hash, sig_bytes)?;
            Ok(sig_scheme)
        }
        _ => Err(CertVerifyError::UnsupportedSignatureScheme),
    }
}
