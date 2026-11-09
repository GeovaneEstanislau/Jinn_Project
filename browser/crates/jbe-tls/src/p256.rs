//! Pure Rust no_std NIST P-256 (secp256r1 / prime256v1) ECC and ECDSA verification (FIPS 186-4 / SEC 1)

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use core::cmp::Ordering;
use crate::bignum::BigUint;

// Constantes NIST P-256
const P_HEX: &[u8] = b"FFFFFFFF00000001000000000000000000000000FFFFFFFFFFFFFFFFFFFFFFFF";
const B_HEX: &[u8] = b"5AC635D8AA3A93E7B3EBBD55769886BC651D06B0CC53B0F63BCE3C3E27D2604B";
const GX_HEX: &[u8] = b"6B17D1F2E12C4247F8BCE6E563A440F277037D812DEB33A0F4A13945D898C296";
const GY_HEX: &[u8] = b"4FE342E2FE1A7F9B8EE7EB4A7C0F9E162BCE33576B315ECECBB6406837BF51F5";
const N_HEX: &[u8] = b"FFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551";

fn hex_to_biguint(hex: &[u8]) -> BigUint {
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let mut i = 0;
    while i < hex.len() {
        let hi = hex_val(hex[i]);
        let lo = hex_val(hex[i + 1]);
        bytes.push((hi << 4) | lo);
        i += 2;
    }
    BigUint::from_be_bytes(&bytes)
}

fn hex_val(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => 0,
    }
}

/// Ponto afim na curva elíptica NIST P-256
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AffinePoint {
    pub x: BigUint,
    pub y: BigUint,
    pub is_infinity: bool,
}

impl AffinePoint {
    pub fn infinity() -> Self {
        Self {
            x: BigUint::zero(),
            y: BigUint::zero(),
            is_infinity: true,
        }
    }

    pub fn new(x: BigUint, y: BigUint) -> Self {
        Self {
            x,
            y,
            is_infinity: false,
        }
    }
}

/// Ponto em coordenadas Jacobianas (X : Y : Z) onde x = X / Z^2, y = Y / Z^3
#[derive(Clone, Debug)]
struct JacobianPoint {
    x: BigUint,
    y: BigUint,
    z: BigUint,
    is_infinity: bool,
}

impl JacobianPoint {
    fn infinity() -> Self {
        Self {
            x: BigUint::zero(),
            y: BigUint::zero(),
            z: BigUint::zero(),
            is_infinity: true,
        }
    }

    fn from_affine(p: &AffinePoint) -> Self {
        if p.is_infinity {
            Self::infinity()
        } else {
            Self {
                x: p.x.clone(),
                y: p.y.clone(),
                z: BigUint::one(),
                is_infinity: false,
            }
        }
    }

    fn to_affine(&self, p_mod: &BigUint) -> AffinePoint {
        if self.is_infinity || self.z.is_zero() {
            return AffinePoint::infinity();
        }
        // z_inv = z^(p - 2) mod p
        let p_minus_2 = p_mod.sub(&BigUint::from_u64(2));
        let z_inv = self.z.mod_exp(&p_minus_2, p_mod);
        let z_inv_2 = z_inv.mod_mul(&z_inv, p_mod);
        let z_inv_3 = z_inv_2.mod_mul(&z_inv, p_mod);

        let x = self.x.mod_mul(&z_inv_2, p_mod);
        let y = self.y.mod_mul(&z_inv_3, p_mod);

        AffinePoint::new(x, y)
    }

    fn double(&self, p_mod: &BigUint) -> Self {
        if self.is_infinity || self.y.is_zero() {
            return Self::infinity();
        }

        // Para a = -3:
        // delta = z^2 mod p
        // gamma = y^2 mod p
        // beta = x * gamma mod p
        // alpha = 3 * (x - delta) * (x + delta) mod p
        let delta = self.z.mod_mul(&self.z, p_mod);
        let gamma = self.y.mod_mul(&self.y, p_mod);
        let beta = self.x.mod_mul(&gamma, p_mod);

        let x_sub_delta = mod_sub(&self.x, &delta, p_mod);
        let x_add_delta = mod_add(&self.x, &delta, p_mod);
        let term = x_sub_delta.mod_mul(&x_add_delta, p_mod);
        let alpha = mod_mul_small(&term, 3, p_mod);

        // x' = alpha^2 - 8 * beta mod p
        let alpha_sq = alpha.mod_mul(&alpha, p_mod);
        let eight_beta = mod_mul_small(&beta, 8, p_mod);
        let x_prime = mod_sub(&alpha_sq, &eight_beta, p_mod);

        // z' = 2 * y * z mod p
        let two_y = mod_mul_small(&self.y, 2, p_mod);
        let z_prime = two_y.mod_mul(&self.z, p_mod);

        // y' = alpha * (4 * beta - x') - 8 * gamma^2 mod p
        let four_beta = mod_mul_small(&beta, 4, p_mod);
        let diff = mod_sub(&four_beta, &x_prime, p_mod);
        let term2 = alpha.mod_mul(&diff, p_mod);
        let gamma_sq = gamma.mod_mul(&gamma, p_mod);
        let eight_gamma_sq = mod_mul_small(&gamma_sq, 8, p_mod);
        let y_prime = mod_sub(&term2, &eight_gamma_sq, p_mod);

        Self {
            x: x_prime,
            y: y_prime,
            z: z_prime,
            is_infinity: false,
        }
    }

    fn add_affine(&self, q: &AffinePoint, p_mod: &BigUint) -> Self {
        if self.is_infinity {
            return Self::from_affine(q);
        }
        if q.is_infinity {
            return self.clone();
        }

        let z1_sq = self.z.mod_mul(&self.z, p_mod);
        let z1_cube = z1_sq.mod_mul(&self.z, p_mod);

        let u2 = q.x.mod_mul(&z1_sq, p_mod);
        let s2 = q.y.mod_mul(&z1_cube, p_mod);

        let h = mod_sub(&u2, &self.x, p_mod);
        let r = mod_sub(&s2, &self.y, p_mod);

        if h.is_zero() {
            if r.is_zero() {
                return self.double(p_mod);
            } else {
                return Self::infinity();
            }
        }

        let h_sq = h.mod_mul(&h, p_mod);
        let h_cube = h_sq.mod_mul(&h, p_mod);
        let u1_h_sq = self.x.mod_mul(&h_sq, p_mod);

        // x' = r^2 - h^3 - 2 * u1 * h^2 mod p
        let r_sq = r.mod_mul(&r, p_mod);
        let two_u1_h_sq = mod_mul_small(&u1_h_sq, 2, p_mod);
        let sub1 = mod_sub(&r_sq, &h_cube, p_mod);
        let x_prime = mod_sub(&sub1, &two_u1_h_sq, p_mod);

        // y' = r * (u1 * h^2 - x') - self.y * h^3 mod p
        let diff = mod_sub(&u1_h_sq, &x_prime, p_mod);
        let term1 = r.mod_mul(&diff, p_mod);
        let term2 = self.y.mod_mul(&h_cube, p_mod);
        let y_prime = mod_sub(&term1, &term2, p_mod);

        // z' = self.z * h mod p
        let z_prime = self.z.mod_mul(&h, p_mod);

        Self {
            x: x_prime,
            y: y_prime,
            z: z_prime,
            is_infinity: false,
        }
    }
}

fn mod_add(a: &BigUint, b: &BigUint, p: &BigUint) -> BigUint {
    let s = a.add(b);
    if s.cmp(p) != Ordering::Less {
        s.sub(p)
    } else {
        s
    }
}

fn mod_sub(a: &BigUint, b: &BigUint, p: &BigUint) -> BigUint {
    if a.cmp(b) == Ordering::Less {
        a.add(p).sub(b)
    } else {
        a.sub(b)
    }
}

fn mod_mul_small(a: &BigUint, k: u64, p: &BigUint) -> BigUint {
    a.mod_mul(&BigUint::from_u64(k), p)
}

/// Multiplicação escalar k * P na curva P-256
fn scalar_mul_p256(k: &BigUint, p: &AffinePoint, p_mod: &BigUint) -> AffinePoint {
    if k.is_zero() || p.is_infinity {
        return AffinePoint::infinity();
    }
    let mut res = JacobianPoint::infinity();
    let bit_len = k.bit_length();

    for i in (0..bit_len).rev() {
        res = res.double(p_mod);
        let limb_idx = i / 64;
        let bit_idx = i % 64;
        if (k.limbs[limb_idx] >> bit_idx) & 1 == 1 {
            res = res.add_affine(p, p_mod);
        }
    }

    res.to_affine(p_mod)
}

/// Chave pública ECDSA P-256
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P256PublicKey {
    pub point: AffinePoint,
}

impl P256PublicKey {
    /// Decodifica ponto P-256 a partir de chave pública uncompressed (65 bytes: 0x04 || X (32) || Y (32))
    pub fn from_sec1_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() != 65 || bytes[0] != 0x04 {
            return Err("Chave P-256 deve estar em formato uncompressed (0x04 || X || Y de 65 bytes)");
        }
        let x = BigUint::from_be_bytes(&bytes[1..33]);
        let y = BigUint::from_be_bytes(&bytes[33..65]);

        let p_mod = hex_to_biguint(P_HEX);
        let b_coeff = hex_to_biguint(B_HEX);

        // Verifica validade do ponto na curva: y^2 = x^3 - 3x + b mod p
        let y_sq = y.mod_mul(&y, &p_mod);
        let x_cube = x.mod_mul(&x, &p_mod).mod_mul(&x, &p_mod);
        let three_x = mod_mul_small(&x, 3, &p_mod);
        let rhs = mod_add(&mod_sub(&x_cube, &three_x, &p_mod), &b_coeff, &p_mod);

        if y_sq != rhs {
            return Err("Ponto fornecido nao pertence a curva NIST P-256");
        }

        Ok(Self {
            point: AffinePoint::new(x, y),
        })
    }

    /// Verifica assinatura ECDSA P-256 com resumo SHA-256 (RFC 5480 / FIPS 186-4)
    pub fn verify_signature(&self, message_hash: &[u8; 32], r_bytes: &[u8], s_bytes: &[u8]) -> Result<(), &'static str> {
        let p_mod = hex_to_biguint(P_HEX);
        let n_order = hex_to_biguint(N_HEX);
        let gx = hex_to_biguint(GX_HEX);
        let gy = hex_to_biguint(GY_HEX);
        let g = AffinePoint::new(gx, gy);

        let r = BigUint::from_be_bytes(r_bytes);
        let s = BigUint::from_be_bytes(s_bytes);

        if r.is_zero() || r.cmp(&n_order) != Ordering::Less {
            return Err("ECDSA: r fora da faixa [1, n-1]");
        }
        if s.is_zero() || s.cmp(&n_order) != Ordering::Less {
            return Err("ECDSA: s fora da faixa [1, n-1]");
        }

        // w = s^(n - 2) mod n
        let n_minus_2 = n_order.sub(&BigUint::from_u64(2));
        let w = s.mod_exp(&n_minus_2, &n_order);

        // e = H(m) mod n
        let e = BigUint::from_be_bytes(message_hash).rem(&n_order);

        // u1 = e * w mod n
        let u1 = e.mod_mul(&w, &n_order);
        // u2 = r * w mod n
        let u2 = r.mod_mul(&w, &n_order);

        // R = u1 * G + u2 * Q
        let p1 = scalar_mul_p256(&u1, &g, &p_mod);
        let p2 = scalar_mul_p256(&u2, &self.point, &p_mod);

        let r_jac = JacobianPoint::from_affine(&p1).add_affine(&p2, &p_mod);
        let r_aff = r_jac.to_affine(&p_mod);

        if r_aff.is_infinity {
            return Err("ECDSA: ponto R resultou no infinito");
        }

        let rx_mod_n = r_aff.x.rem(&n_order);
        if rx_mod_n != r {
            return Err("ECDSA: assinatura P-256 criptograficamente invalida (R.x != r)");
        }

        Ok(())
    }
}

/// Decodifica assinatura ECDSA no formato ASN.1 DER (SEQUENCE { r INTEGER, s INTEGER })
pub fn parse_der_ecdsa_signature(der: &[u8]) -> Result<(Vec<u8>, Vec<u8>), &'static str> {
    if der.is_empty() || der[0] != 0x30 {
        return Err("ECDSA Signature: SEQUENCE (0x30) esperada");
    }
    let mut offset = 1;
    if offset >= der.len() { return Err("EOF DER"); }
    let len_byte = der[offset];
    offset += 1;
    let seq_len = if len_byte & 0x80 == 0 {
        len_byte as usize
    } else {
        let count = (len_byte & 0x7F) as usize;
        if count > 2 || offset + count > der.len() { return Err("Tamanho DER invalido"); }
        let mut l = 0usize;
        for _ in 0..count {
            l = (l << 8) | (der[offset] as usize);
            offset += 1;
        }
        l
    };

    if offset + seq_len > der.len() {
        return Err("Payload de assinatura ECDSA ultrapassa o buffer");
    }

    let inner = &der[offset..offset + seq_len];
    let mut p = 0;

    // r INTEGER
    if p >= inner.len() || inner[p] != 0x02 { return Err("INTEGER r esperado"); }
    p += 1;
    let r_len = inner[p] as usize;
    p += 1;
    let r_bytes = &inner[p..p + r_len];
    p += r_len;

    // s INTEGER
    if p >= inner.len() || inner[p] != 0x02 { return Err("INTEGER s esperado"); }
    p += 1;
    let s_len = inner[p] as usize;
    p += 1;
    let s_bytes = &inner[p..p + s_len];

    Ok((r_bytes.to_vec(), s_bytes.to_vec()))
}
