//! Pure Rust no_std arbitrary-precision unsigned integer arithmetic (BigUint)
//! Optimized for cryptographic operations: RSA and Elliptic Curve arithmetic.

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use core::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BigUint {
    // Little-endian limbs: limbs[0] is the least significant 64 bits.
    pub limbs: Vec<u64>,
}

impl BigUint {
    #[inline]
    pub fn zero() -> Self {
        Self { limbs: Vec::new() }
    }

    #[inline]
    pub fn one() -> Self {
        #[cfg(not(feature = "std"))]
        { Self { limbs: alloc::vec![1] } }
        #[cfg(feature = "std")]
        { Self { limbs: vec![1] } }
    }

    #[inline]
    pub fn from_u64(val: u64) -> Self {
        if val == 0 {
            Self::zero()
        } else {
            #[cfg(not(feature = "std"))]
            { Self { limbs: alloc::vec![val] } }
            #[cfg(feature = "std")]
            { Self { limbs: vec![val] } }
        }
    }

    pub fn trim(&mut self) {
        while let Some(&0) = self.limbs.last() {
            self.limbs.pop();
        }
    }

    #[inline]
    pub fn is_zero(&self) -> bool {
        self.limbs.iter().all(|&l| l == 0)
    }

    #[inline]
    pub fn is_odd(&self) -> bool {
        self.limbs.first().map_or(false, |&l| l & 1 == 1)
    }

    pub fn bit_length(&self) -> usize {
        let mut len = self.limbs.len();
        while len > 0 && self.limbs[len - 1] == 0 {
            len -= 1;
        }
        if len == 0 {
            return 0;
        }
        let top = self.limbs[len - 1];
        (len - 1) * 64 + (64 - top.leading_zeros() as usize)
    }

    pub fn from_be_bytes(bytes: &[u8]) -> Self {
        let mut start = 0;
        while start < bytes.len() && bytes[start] == 0 {
            start += 1;
        }
        let slice = &bytes[start..];
        if slice.is_empty() {
            return Self::zero();
        }

        let num_limbs = (slice.len() + 7) / 8;
        #[cfg(not(feature = "std"))]
        let mut limbs = alloc::vec![0u64; num_limbs];
        #[cfg(feature = "std")]
        let mut limbs = vec![0u64; num_limbs];

        for (i, &byte) in slice.iter().rev().enumerate() {
            let limb_idx = i / 8;
            let byte_idx = i % 8;
            limbs[limb_idx] |= (byte as u64) << (byte_idx * 8);
        }

        let mut res = Self { limbs };
        res.trim();
        res
    }

    pub fn to_be_bytes(&self) -> Vec<u8> {
        if self.is_zero() {
            #[cfg(not(feature = "std"))]
            return alloc::vec![0];
            #[cfg(feature = "std")]
            return vec![0];
        }
        let mut bytes = Vec::with_capacity(self.limbs.len() * 8);
        for &limb in self.limbs.iter().rev() {
            bytes.extend_from_slice(&limb.to_be_bytes());
        }
        let mut start = 0;
        while start < bytes.len() - 1 && bytes[start] == 0 {
            start += 1;
        }
        bytes[start..].to_vec()
    }

    pub fn to_be_bytes_padded(&self, len: usize) -> Option<Vec<u8>> {
        let raw = self.to_be_bytes();
        if raw.len() > len {
            if raw.len() == len + 1 && raw[0] == 0 {
                return Some(raw[1..].to_vec());
            }
            return None;
        }
        #[cfg(not(feature = "std"))]
        let mut out = alloc::vec![0u8; len];
        #[cfg(feature = "std")]
        let mut out = vec![0u8; len];

        let offset = len - raw.len();
        out[offset..].copy_from_slice(&raw);
        Some(out)
    }

    pub fn cmp(&self, other: &Self) -> Ordering {
        let l1 = self.effective_len();
        let l2 = other.effective_len();
        if l1 != l2 {
            return l1.cmp(&l2);
        }
        for i in (0..l1).rev() {
            let a = self.limbs.get(i).copied().unwrap_or(0);
            let b = other.limbs.get(i).copied().unwrap_or(0);
            if a != b {
                return a.cmp(&b);
            }
        }
        Ordering::Equal
    }

    #[inline]
    fn effective_len(&self) -> usize {
        let mut len = self.limbs.len();
        while len > 0 && self.limbs[len - 1] == 0 {
            len -= 1;
        }
        len
    }

    pub fn add(&self, other: &Self) -> Self {
        let max_len = core::cmp::max(self.limbs.len(), other.limbs.len());
        let mut res = Vec::with_capacity(max_len + 1);
        let mut carry = 0u128;

        for i in 0..max_len {
            let a = self.limbs.get(i).copied().unwrap_or(0) as u128;
            let b = other.limbs.get(i).copied().unwrap_or(0) as u128;
            let sum = a + b + carry;
            res.push(sum as u64);
            carry = sum >> 64;
        }
        if carry > 0 {
            res.push(carry as u64);
        }
        let mut out = Self { limbs: res };
        out.trim();
        out
    }

    pub fn sub(&self, other: &Self) -> Self {
        if self.cmp(other) == Ordering::Less {
            return Self::zero();
        }
        let mut res = Vec::with_capacity(self.limbs.len());
        let mut borrow = 0i128;

        for i in 0..self.limbs.len() {
            let a = self.limbs[i] as i128;
            let b = other.limbs.get(i).copied().unwrap_or(0) as i128;
            let diff = a - b - borrow;
            if diff < 0 {
                res.push((diff + 0x1_0000_0000_0000_0000i128) as u64);
                borrow = 1;
            } else {
                res.push(diff as u64);
                borrow = 0;
            }
        }
        let mut out = Self { limbs: res };
        out.trim();
        out
    }

    pub fn mul(&self, other: &Self) -> Self {
        if self.is_zero() || other.is_zero() {
            return Self::zero();
        }
        let n = self.limbs.len();
        let m = other.limbs.len();
        #[cfg(not(feature = "std"))]
        let mut res = alloc::vec![0u64; n + m];
        #[cfg(feature = "std")]
        let mut res = vec![0u64; n + m];

        for i in 0..n {
            let a = self.limbs[i] as u128;
            if a == 0 { continue; }
            let mut carry = 0u128;
            for j in 0..m {
                let b = other.limbs[j] as u128;
                let cur = res[i + j] as u128 + a * b + carry;
                res[i + j] = cur as u64;
                carry = cur >> 64;
            }
            if carry > 0 {
                res[i + m] += carry as u64;
            }
        }
        let mut out = Self { limbs: res };
        out.trim();
        out
    }

    pub fn shl_bits(&self, shift: usize) -> Self {
        if self.is_zero() || shift == 0 {
            return self.clone();
        }
        let limb_shift = shift / 64;
        let bit_shift = shift % 64;

        #[cfg(not(feature = "std"))]
        let mut res = alloc::vec![0u64; limb_shift];
        #[cfg(feature = "std")]
        let mut res = vec![0u64; limb_shift];

        if bit_shift == 0 {
            res.extend_from_slice(&self.limbs);
        } else {
            let mut carry = 0u64;
            for &limb in &self.limbs {
                res.push((limb << bit_shift) | carry);
                carry = limb >> (64 - bit_shift);
            }
            if carry > 0 {
                res.push(carry);
            }
        }
        let mut out = Self { limbs: res };
        out.trim();
        out
    }

    pub fn shr_bits(&self, shift: usize) -> Self {
        if self.is_zero() || shift == 0 {
            return self.clone();
        }
        let limb_shift = shift / 64;
        let bit_shift = shift % 64;

        if limb_shift >= self.limbs.len() {
            return Self::zero();
        }

        let slice = &self.limbs[limb_shift..];
        let mut res = Vec::with_capacity(slice.len());

        if bit_shift == 0 {
            res.extend_from_slice(slice);
        } else {
            let mut carry = 0u64;
            for &limb in slice.iter().rev() {
                let new_limb = (limb >> bit_shift) | carry;
                carry = limb << (64 - bit_shift);
                res.push(new_limb);
            }
            res.reverse();
        }
        let mut out = Self { limbs: res };
        out.trim();
        out
    }

    /// Divisão e Resto com Knuth's Algorithm D (TAOCP Vol 2)
    pub fn div_rem(&self, divisor: &Self) -> (Self, Self) {
        if divisor.is_zero() {
            panic!("Division by zero in BigUint");
        }
        let cmp = self.cmp(divisor);
        if cmp == Ordering::Less {
            return (Self::zero(), self.clone());
        }
        if cmp == Ordering::Equal {
            return (Self::one(), Self::zero());
        }

        // Caso base: divisor tem apenas 1 limb
        if divisor.limbs.len() == 1 {
            let d = divisor.limbs[0] as u128;
            let mut rem = 0u128;
            #[cfg(not(feature = "std"))]
            let mut quot = alloc::vec![0u64; self.limbs.len()];
            #[cfg(feature = "std")]
            let mut quot = vec![0u64; self.limbs.len()];

            for i in (0..self.limbs.len()).rev() {
                let cur = (rem << 64) | (self.limbs[i] as u128);
                quot[i] = (cur / d) as u64;
                rem = cur % d;
            }
            let mut q = Self { limbs: quot };
            q.trim();
            let mut r = Self::from_u64(rem as u64);
            r.trim();
            return (q, r);
        }

        // Knuth Algorithm D
        let n = divisor.effective_len(); // divisor length >= 2

        // Normalização: shift para que o bit mais significativo de divisor seja 1 (bit 63)
        let shift = divisor.limbs[n - 1].leading_zeros() as usize;
        let v = divisor.shl_bits(shift);
        let mut u = self.shl_bits(shift);

        let u_len = u.effective_len();
        while u.limbs.len() <= u_len {
            u.limbs.push(0);
        }

        let m = if u_len >= n { u_len - n } else { 0 };

        #[cfg(not(feature = "std"))]
        let mut q = alloc::vec![0u64; m + 1];
        #[cfg(feature = "std")]
        let mut q = vec![0u64; m + 1];

        let v1 = v.limbs[n - 1] as u128;
        let v2 = v.limbs[n - 2] as u128;

        #[cfg(not(feature = "std"))]
        let mut t = alloc::vec![0u64; n + 1];
        #[cfg(feature = "std")]
        let mut t = vec![0u64; n + 1];

        for j in (0..=m).rev() {
            let u_top = u.limbs[j + n] as u128;
            let u_next = u.limbs[j + n - 1] as u128;
            let u_third = if n >= 2 { u.limbs[j + n - 2] as u128 } else { 0 };

            let numerator = (u_top << 64) | u_next;
            let mut qhat = numerator / v1;
            let mut rhat = numerator % v1;

            if qhat >= (1u128 << 64) {
                qhat = (1u128 << 64) - 1;
                rhat = numerator - qhat * v1;
            }

            // Refinamento de qhat com v2
            while rhat < (1u128 << 64) && (qhat * v2 > ((rhat << 64) | u_third)) {
                qhat -= 1;
                rhat += v1;
            }

            // Multiplica qhat * v em t[0..=n]
            let mut carry = 0u128;
            for i in 0..n {
                let prod = qhat * (v.limbs[i] as u128) + carry;
                t[i] = prod as u64;
                carry = prod >> 64;
            }
            t[n] = carry as u64;

            // Subtrai t[0..=n] de u[j..=j+n]
            let mut borrow = 0u128;
            for i in 0..=n {
                let sub = (t[i] as u128) + borrow;
                let cur = u.limbs[j + i] as u128;
                if cur < sub {
                    u.limbs[j + i] = ((cur + (1u128 << 64)) - sub) as u64;
                    borrow = 1;
                } else {
                    u.limbs[j + i] = (cur - sub) as u64;
                    borrow = 0;
                }
            }

            // Se houve borrow no topo, qhat foi 1 a mais: corrige somando v de volta
            if borrow != 0 {
                qhat -= 1;
                let mut add_carry = 0u128;
                for i in 0..n {
                    let sum = (u.limbs[j + i] as u128) + (v.limbs[i] as u128) + add_carry;
                    u.limbs[j + i] = sum as u64;
                    add_carry = sum >> 64;
                }
                u.limbs[j + n] = (u.limbs[j + n] as u128 + add_carry) as u64;
            }

            q[j] = qhat as u64;
        }

        let mut quot = Self { limbs: q };
        quot.trim();

        // Desnormaliza resto: shift à direita por `shift`
        let rem_limbs = u.limbs[..n].to_vec();
        let mut rem = Self { limbs: rem_limbs };
        rem.trim();
        let final_rem = rem.shr_bits(shift);

        (quot, final_rem)
    }

    #[inline]
    pub fn rem(&self, modulus: &Self) -> Self {
        let (_, r) = self.div_rem(modulus);
        r
    }

    #[inline]
    pub fn mod_mul(&self, other: &Self, modulus: &Self) -> Self {
        let prod = self.mul(other);
        prod.rem(modulus)
    }

    /// Exponenciação Modular: (self ^ exp) mod modulus
    pub fn mod_exp(&self, exp: &Self, modulus: &Self) -> Self {
        if modulus.is_zero() {
            panic!("Modulus cannot be zero");
        }
        if exp.is_zero() {
            return Self::one().rem(modulus);
        }
        let mut base = self.rem(modulus);
        let mut result = Self::one();

        let bit_len = exp.bit_length();
        for i in 0..bit_len {
            let limb_idx = i / 64;
            let bit_idx = i % 64;
            if (exp.limbs[limb_idx] >> bit_idx) & 1 == 1 {
                result = result.mod_mul(&base, modulus);
            }
            if i + 1 < bit_len {
                base = base.mod_mul(&base, modulus);
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bignum_add_sub_mul() {
        let a = BigUint::from_u64(0x1234_5678_9ABC_DEF0);
        let b = BigUint::from_u64(0xFEDC_BA98_7654_3210);
        let sum = a.add(&b);
        let diff = sum.sub(&a);
        assert_eq!(diff, b);

        let p1 = BigUint::from_u64(1000);
        let p2 = BigUint::from_u64(2000);
        let prod = p1.mul(&p2);
        assert_eq!(prod, BigUint::from_u64(2_000_000));
    }

    #[test]
    fn test_bignum_div_rem() {
        let a = BigUint::from_u64(1234567);
        let b = BigUint::from_u64(100);
        let (q, r) = a.div_rem(&b);
        assert_eq!(q, BigUint::from_u64(12345));
        assert_eq!(r, BigUint::from_u64(67));

        // Multi-limb division
        let x = BigUint::from_be_bytes(&[
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
            0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10,
        ]);
        let y = BigUint::from_be_bytes(&[0x05, 0x06, 0x07, 0x08]);
        let (q2, r2) = x.div_rem(&y);
        let reconstructed = q2.mul(&y).add(&r2);
        assert_eq!(reconstructed, x);
        assert!(r2.cmp(&y) == Ordering::Less);
    }

    #[test]
    fn test_bignum_s2_mod_n() {
        let sig = include_bytes!("../tests/fixtures/root.crt");
        let s_bytes = &sig[sig.len() - 256..];
        let n_hex = b"C3D387BD94EA765EB2A131B4CD7DEF64C3536DB8432543DD64CBE6BAC0459D5F18566B6DEF724237DDB1286892FD4AC1B22DE50F84042D8306C1E3C887CB8A319BA3B932FCDA1F9A72C89387CF5069FDB6C3604C5601C0BF89913F1E646430B3DE50D07D65F45C46D5175CD56E3B36F42F1BCAEE26E5375C7DB6EBBCF93525074B54BAFCD0449AA521A68C5EC317C1ED69D09C6F5F558F7A83BF52B4CD04705431A0B8707CEADFE744D1A58C1748EBC00B6B3AD2EFB821CB73AAAE4E0651094DE5EA32AF9626DF4D4B8E204CF9FAB243DDA2F64A6EC7F1B534B5F1A41BED6D16503AA7D177C5E61AE7C4A8F5E51B369F747A4C3BB309E842A58CD431BE0AF531";
        
        let mut n_bytes = Vec::new();
        let mut i = 0;
        while i < n_hex.len() {
            let h = match n_hex[i] { b'0'..=b'9' => n_hex[i] - b'0', b'A'..=b'F' => n_hex[i] - b'A' + 10, _ => 0 };
            let l = match n_hex[i+1] { b'0'..=b'9' => n_hex[i+1] - b'0', b'A'..=b'F' => n_hex[i+1] - b'A' + 10, _ => 0 };
            n_bytes.push((h << 4) | l);
            i += 2;
        }

        let s = BigUint::from_be_bytes(s_bytes);
        let n = BigUint::from_be_bytes(&n_bytes);
        let s2 = s.mod_mul(&s, &n);
        let s2_bytes = s2.to_be_bytes_padded(256).unwrap();
        println!("Rust s2[0..8]: {:02x?}", &s2_bytes[..8]);
        assert_eq!(&s2_bytes[..8], &[0xaf, 0xfa, 0x81, 0xa4, 0x26, 0x41, 0xe2, 0xff]);
    }

    #[test]
    fn test_bignum_mod_exp() {
        // 2^10 mod 1000 = 1024 mod 1000 = 24
        let base = BigUint::from_u64(2);
        let exp = BigUint::from_u64(10);
        let m = BigUint::from_u64(1000);
        assert_eq!(base.mod_exp(&exp, &m), BigUint::from_u64(24));

        // 3^65537 mod 1000000007
        let base2 = BigUint::from_u64(3);
        let exp2 = BigUint::from_u64(65537);
        let m2 = BigUint::from_u64(1_000_000_007);
        let res = base2.mod_exp(&exp2, &m2);
        // We know (3^65537) mod 1_000_000_007 can be checked
        assert!(!res.is_zero());
    }
}

