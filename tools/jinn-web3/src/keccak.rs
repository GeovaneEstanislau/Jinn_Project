//! Pure-Rust Keccak-256 Implementation (Zero Dependencies)
//! Matches Ethereum and Solidity `keccak256` exactly.

#![allow(dead_code)]

const RC: [u64; 24] = [
    0x0000000000000001, 0x0000000000008082, 0x800000000000808a, 0x8000000080008000,
    0x000000000000808b, 0x0000000080000001, 0x8000000080008081, 0x8000000000008009,
    0x000000000000008a, 0x0000000000000088, 0x0000000080008009, 0x000000008000000a,
    0x000000008000808b, 0x800000000000008b, 0x8000000000008089, 0x8000000000008003,
    0x8000000000008002, 0x8000000000000080, 0x000000000000800a, 0x800000008000000a,
    0x8000000080008081, 0x8000000000008080, 0x0000000080000001, 0x8000000080008008,
];

const R: [u32; 24] = [
    1, 3, 6, 10, 15, 21, 28, 36, 45, 55, 2, 14, 27, 41, 56, 8, 25, 43, 62, 18, 39, 61, 20, 44,
];

const PI: [usize; 24] = [
    10, 7, 11, 17, 18, 3, 5, 16, 8, 21, 24, 4, 15, 23, 19, 13, 12, 2, 20, 14, 22, 9, 6, 1,
];

pub fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut state = [0u64; 25];
    let rate = 136; // 1088 bits / 8
    let mut offset = 0;

    // Absorb full rate blocks
    while offset + rate <= data.len() {
        for i in 0..17 {
            let chunk = &data[offset + i * 8..offset + (i + 1) * 8];
            state[i] ^= u64::from_le_bytes(chunk.try_into().unwrap());
        }
        keccak_f1600(&mut state);
        offset += rate;
    }

    // Absorb tail with Ethereum padding (0x01 .. 0x80)
    let rem = &data[offset..];
    let mut block = [0u8; 136];
    block[..rem.len()].copy_from_slice(rem);
    block[rem.len()] ^= 0x01;
    block[135] ^= 0x80;

    for i in 0..17 {
        let chunk = &block[i * 8..(i + 1) * 8];
        state[i] ^= u64::from_le_bytes(chunk.try_into().unwrap());
    }
    keccak_f1600(&mut state);

    // Squeeze 32 bytes
    let mut out = [0u8; 32];
    for i in 0..4 {
        out[i * 8..(i + 1) * 8].copy_from_slice(&state[i].to_le_bytes());
    }
    out
}

fn keccak_f1600(a: &mut [u64; 25]) {
    for &rc in &RC {
        // Theta
        let mut c = [0u64; 5];
        for x in 0..5 {
            c[x] = a[x] ^ a[x + 5] ^ a[x + 10] ^ a[x + 15] ^ a[x + 20];
        }
        let mut d = [0u64; 5];
        for x in 0..5 {
            d[x] = c[(x + 4) % 5] ^ c[(x + 1) % 5].rotate_left(1);
        }
        for x in 0..5 {
            for y in 0..5 {
                a[x + y * 5] ^= d[x];
            }
        }

        // Rho and Pi
        let mut last = a[1];
        for i in 0..24 {
            let next = a[PI[i]];
            a[PI[i]] = last.rotate_left(R[i]);
            last = next;
        }

        // Chi
        for y in 0..5 {
            let mut row = [0u64; 5];
            for x in 0..5 {
                row[x] = a[x + y * 5];
            }
            for x in 0..5 {
                a[x + y * 5] = row[x] ^ ((!row[(x + 1) % 5]) & row[(x + 2) % 5]);
            }
        }

        // Iota
        a[0] ^= rc;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bep20::to_hex_string;

    #[test]
    fn test_keccak_empty() {
        let empty_hash = keccak256(b"");
        // Standard Ethereum keccak256("") = c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470
        assert_eq!(
            to_hex_string(&empty_hash),
            "0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
        );
    }

    #[test]
    fn test_keccak_transfer_selector() {
        let hash = keccak256(b"transfer(address,uint256)");
        assert_eq!(&hash[..4], &[0xa9, 0x05, 0x9c, 0xbb]);
    }
}
