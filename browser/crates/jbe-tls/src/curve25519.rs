//! Pure Rust Curve25519 / X25519 scalar multiplication (RFC 7748)
//!
//! Uses 256-bit exact integer field arithmetic modulo 2^255 - 19.

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
    pub const A24: Self = U256([121665, 0, 0, 0]); // (486662 - 2) / 4

    pub fn from_bytes_le(b: &[u8; 32]) -> Self {
        let mut limbs = [0u64; 4];
        for i in 0..4 {
            let mut arr = [0u8; 8];
            arr.copy_from_slice(&b[i * 8..i * 8 + 8]);
            limbs[i] = u64::from_le_bytes(arr);
        }
        // Mask out bit 255
        limbs[3] &= 0x7FFF_FFFF_FFFF_FFFF;
        let mut u = U256(limbs);
        if u.gte(&P) {
            u = u.sub_p();
        }
        u
    }

    pub fn to_bytes_le(&self) -> [u8; 32] {
        let mut u = *self;
        if u.gte(&P) {
            u = u.sub_p();
        }
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
        if res.gte(&P) {
            res = res.sub_p();
        }
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
            // self + P - other
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

        // sum1 = limbs[0..4] + limbs[4..8] * 38
        let mut sum1 = [0u64; 5];
        let mut c = 0u128;
        for i in 0..4 {
            let term = (limbs[4 + i] as u128) * 38 + (limbs[i] as u128) + c;
            sum1[i] = term as u64;
            c = term >> 64;
        }
        sum1[4] = c as u64;

        // Fold bits >= 255: high1 = (sum1[3] >> 63) | (sum1[4] << 1)
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
        if final_u.gte(&P) {
            final_u = final_u.sub_p();
        }
        final_u
    }

    pub fn square(&self) -> Self {
        self.mul(self)
    }

    pub fn invert(&self) -> Self {
        // Exponentiation by P - 2 = 2^255 - 21
        let mut res = U256::ONE;
        let mut base = *self;

        // P - 2 has limbs:
        // [0xFFFF_FFFF_FFFF_FFEBu64, 0xFFFF_FFFF_FFFF_FFFF, 0xFFFF_FFFF_FFFF_FFFF, 0x7FFF_FFFF_FFFF_FFFF]
        let p_minus_2: [u64; 4] = [
            0xFFFF_FFFF_FFFF_FFEBu64,
            0xFFFF_FFFF_FFFF_FFFFu64,
            0xFFFF_FFFF_FFFF_FFFFu64,
            0x7FFF_FFFF_FFFF_FFFFu64,
        ];

        for &limb in &p_minus_2 {
            for b in 0..64 {
                if limb == 0x7FFF_FFFF_FFFF_FFFFu64 && b >= 63 {
                    break;
                }
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

/// Computes X25519 scalar multiplication (RFC 7748)
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
