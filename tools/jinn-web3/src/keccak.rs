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

    // ========================================================================
    // Reference vectors: standard Ethereum keccak256 (padding byte 0x01)
    // ========================================================================

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
    fn test_keccak_abc() {
        let hash = keccak256(b"abc");
        assert_eq!(
            to_hex_string(&hash),
            "0x4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45"
        );
    }

    #[test]
    fn test_keccak_hello() {
        let hash = keccak256(b"hello");
        assert_eq!(
            to_hex_string(&hash),
            "0x1c8aff950685c2ed4bc3174f3472287b56d9517b9c948127319a09a7a36deac8"
        );
    }

    #[test]
    fn test_keccak_testing() {
        let hash = keccak256(b"testing");
        assert_eq!(
            to_hex_string(&hash),
            "0x5f16f4c7f149ac4f9510d9cf8cf384038ad348b3bcdc01915f95de12df9d1b02"
        );
    }

    #[test]
    fn test_keccak_jinn_network_token() {
        // keccak256("Jinn Network Token") — used for token name hashing
        let hash = keccak256(b"Jinn Network Token");
        // Verify output is 32 bytes and reproducible
        assert_eq!(hash.len(), 32);
        // Run twice to ensure deterministic
        let hash2 = keccak256(b"Jinn Network Token");
        assert_eq!(hash, hash2);
        // Print the hash for cross-reference (will be validated by sha3 crate below)
        println!("keccak256('Jinn Network Token') = {}", to_hex_string(&hash));
    }

    // ========================================================================
    // ERC-20 / BEP-20 function selector tests
    // ========================================================================

    #[test]
    fn test_keccak_transfer_selector() {
        // keccak256("transfer(address,uint256)") selector = 0xa9059cbb
        let hash = keccak256(b"transfer(address,uint256)");
        assert_eq!(&hash[..4], &[0xa9, 0x05, 0x9c, 0xbb]);
    }

    #[test]
    fn test_keccak_balance_of_selector() {
        // keccak256("balanceOf(address)") selector = 0x70a08231
        let hash = keccak256(b"balanceOf(address)");
        assert_eq!(&hash[..4], &[0x70, 0xa0, 0x82, 0x31]);
    }

    #[test]
    fn test_keccak_total_supply_selector() {
        // keccak256("totalSupply()") selector = 0x18160ddd
        let hash = keccak256(b"totalSupply()");
        assert_eq!(&hash[..4], &[0x18, 0x16, 0x0d, 0xdd]);
    }

    #[test]
    fn test_keccak_approve_selector() {
        // keccak256("approve(address,uint256)") selector = 0x095ea7b3
        let hash = keccak256(b"approve(address,uint256)");
        assert_eq!(&hash[..4], &[0x09, 0x5e, 0xa7, 0xb3]);
    }

    #[test]
    fn test_keccak_burn_selector() {
        // keccak256("burn(uint256)") selector = 0x42966c68
        let hash = keccak256(b"burn(uint256)");
        assert_eq!(&hash[..4], &[0x42, 0x96, 0x6c, 0x68]);
    }

    // ========================================================================
    // Multi-block absorption test (input > 136 bytes = rate)
    // ========================================================================

    #[test]
    fn test_keccak_multi_block() {
        // Create a message longer than the rate (136 bytes) to exercise multi-block absorption
        let long_msg = b"The Jinn Network Token ($JINN) is a BEP-20 asset deployed on the BNB Smart Chain. \
This test string exceeds 136 bytes to force multi-block Keccak-f[1600] absorption. \
Jinn OS architecture enforces Ring 3 isolation for all cryptographic operations.";
        assert!(long_msg.len() > 136, "Test data must exceed one Keccak rate block");

        let hash = keccak256(long_msg);
        assert_eq!(hash.len(), 32);

        // Verify determinism: same input produces same output
        let hash2 = keccak256(long_msg);
        assert_eq!(hash, hash2);

        // Cross-validate: hash must differ from empty or single-block inputs
        let empty_hash = keccak256(b"");
        assert_ne!(hash, empty_hash);

        println!("Multi-block ({} bytes) hash = {}", long_msg.len(), to_hex_string(&hash));
    }

    // ========================================================================
    // Padding byte verification: 0x01 (original Keccak) NOT 0x06 (SHA-3 FIPS 202)
    // ========================================================================

    #[test]
    fn test_padding_is_keccak_not_sha3() {
        // SHA-3("abc") = 3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532
        // Keccak-256("abc") = 4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45
        // If we get the Keccak value, the padding byte is 0x01 (correct for Ethereum).
        // If we got the SHA-3 value, the padding byte would be wrong (0x06).
        let hash = keccak256(b"abc");
        let hex = to_hex_string(&hash);
        assert_eq!(hex, "0x4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45",
            "Padding must be 0x01 (Keccak original), NOT 0x06 (SHA-3 FIPS 202)");
        assert_ne!(hex, "0x3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532",
            "If this matches, the padding byte is SHA-3 (0x06) — WRONG for Ethereum");
    }

    // ========================================================================
    // Round constants, rotation offsets, and permutation indices verification
    // ========================================================================

    #[test]
    fn test_round_constants() {
        // Keccak specification round constants (LFSR-derived)
        // Reference: https://keccak.team/keccak_specs_summary.html
        assert_eq!(RC.len(), 24);
        assert_eq!(RC[0], 0x0000000000000001);
        assert_eq!(RC[1], 0x0000000000008082);
        assert_eq!(RC[2], 0x800000000000808a);
        assert_eq!(RC[3], 0x8000000080008000);
        assert_eq!(RC[23], 0x8000000080008008);
        // In Keccak-f[1600], the LFSR repeats certain constants (e.g. index 6 and 20, 5 and 22)
        assert_eq!(RC[6], RC[20]);
        assert_eq!(RC[5], RC[22]);
    }

    #[test]
    fn test_rotation_offsets() {
        // rho step rotation offsets per the Keccak spec
        // These are ((t+1)(t+2)/2) mod 64 for t=0..23, traversed in pi order
        let expected_r: [u32; 24] = [
            1, 3, 6, 10, 15, 21, 28, 36, 45, 55, 2, 14, 27, 41, 56, 8, 25, 43, 62, 18, 39, 61, 20, 44,
        ];
        assert_eq!(R, expected_r);
    }

    #[test]
    fn test_pi_permutation_indices() {
        // pi step permutation: destination index for each lane in rho+pi combined step
        let expected_pi: [usize; 24] = [
            10, 7, 11, 17, 18, 3, 5, 16, 8, 21, 24, 4, 15, 23, 19, 13, 12, 2, 20, 14, 22, 9, 6, 1,
        ];
        assert_eq!(PI, expected_pi);
        // All indices must be in range 1..25 (lane 0 is untouched by pi)
        for &idx in &PI {
            assert!(idx >= 1 && idx <= 24, "PI index {} out of range [1,24]", idx);
        }
        // All 24 non-zero lanes must be visited exactly once
        let mut visited = std::collections::HashSet::new();
        for &idx in &PI {
            assert!(visited.insert(idx), "Duplicate PI index: {}", idx);
        }
    }

    // ========================================================================
    // Keccak-f[1600] permutation validation on a known input state
    // ========================================================================

    #[test]
    fn test_keccak_f1600_permutation() {
        // Start with all-zeros state, apply one keccak-f[1600] permutation
        let mut state = [0u64; 25];
        keccak_f1600(&mut state);

        // After applying keccak-f1600 to the zero state, only a[0] should be
        // the XOR of all 24 round constants (since theta/rho/pi/chi on zero
        // state interact in a specific way). Instead of deriving the exact
        // expected output analytically, we verify:
        // 1. State is no longer all-zeros (permutation actually ran)
        assert_ne!(state, [0u64; 25], "keccak_f1600 must modify the state");

        // 2. Applying keccak_f1600 is deterministic
        let mut state2 = [0u64; 25];
        keccak_f1600(&mut state2);
        assert_eq!(state, state2, "keccak_f1600 must be deterministic");

        // 3. Cross-validate: hash of empty input (which passes all-zero state
        //    through one permutation after padding) produces the known Ethereum result
        let hash = keccak256(b"");
        assert_eq!(
            to_hex_string(&hash),
            "0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
        );
    }

    // ========================================================================
    // Cross-validation with sha3 crate (external reference oracle)
    // ========================================================================

    #[test]
    fn test_cross_validate_with_sha3_crate() {
        use sha3::{Digest, Keccak256};

        let test_vectors: &[&[u8]] = &[
            b"",
            b"abc",
            b"hello",
            b"testing",
            b"Jinn Network Token",
            b"transfer(address,uint256)",
            b"balanceOf(address)",
            b"totalSupply()",
            b"approve(address,uint256)",
            b"burn(uint256)",
        ];

        for &input in test_vectors {
            let our_hash = keccak256(input);
            let mut hasher = Keccak256::new();
            hasher.update(input);
            let reference: [u8; 32] = hasher.finalize().into();
            assert_eq!(our_hash, reference,
                "Mismatch for input {:?}: ours={} ref={}",
                String::from_utf8_lossy(input),
                to_hex_string(&our_hash),
                to_hex_string(&reference),
            );
        }
    }

    #[test]
    fn test_cross_validate_multi_block_with_sha3_crate() {
        use sha3::{Digest, Keccak256};

        // Test with a 300-byte input (> 2 rate blocks)
        let long_input: Vec<u8> = (0u8..=255).cycle().take(300).collect();
        let our_hash = keccak256(&long_input);
        let mut hasher = Keccak256::new();
        hasher.update(&long_input);
        let reference: [u8; 32] = hasher.finalize().into();
        assert_eq!(our_hash, reference, "Multi-block cross-validation failed");
    }
}
