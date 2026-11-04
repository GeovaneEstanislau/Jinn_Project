//! Pure-Rust Merkle Tree Generator and Verifier for Jinn PoUC Bounties
//! Exactly mirrors `JinnRewardDistributor.sol`'s `_verifyProof` algorithm.

#![allow(dead_code)]

use crate::keccak::keccak256;

/// Computes the leaf hash for a PoUC reward:
/// `keccak256(bytes.concat(keccak256(abi.encode(index, contributor, amount))))`
pub fn compute_pouc_leaf(index: u64, contributor: &[u8; 20], amount_wei: &[u8; 32]) -> [u8; 32] {
    // ABI encode (uint256 index, address contributor, uint256 amount) = 32 + 32 + 32 = 96 bytes
    let mut encoded = [0u8; 96];

    // index (left-padded 32 bytes)
    encoded[24..32].copy_from_slice(&index.to_be_bytes());
    // contributor (left-padded address: 12 zero bytes + 20 bytes address)
    encoded[44..64].copy_from_slice(contributor);
    // amount (32 bytes)
    encoded[64..96].copy_from_slice(amount_wei);

    let inner_hash = keccak256(&encoded);
    keccak256(&inner_hash)
}

/// Combines two sibling hashes with lexicographical sorting, as in OpenZeppelin and Solidity:
/// if (a <= b) keccak256(a || b) else keccak256(b || a)
pub fn hash_pair(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let mut combined = [0u8; 64];
    if a <= b {
        combined[..32].copy_from_slice(a);
        combined[32..].copy_from_slice(b);
    } else {
        combined[..32].copy_from_slice(b);
        combined[32..].copy_from_slice(a);
    }
    keccak256(&combined)
}

/// Verifies a Merkle proof against a root and leaf hash.
pub fn verify_proof(proof: &[[u8; 32]], root: &[u8; 32], leaf: &[u8; 32]) -> bool {
    let mut computed = *leaf;
    for sibling in proof {
        computed = hash_pair(&computed, sibling);
    }
    &computed == root
}

/// Builds a balanced Merkle tree from an arbitrary list of leaves.
/// Returns (root, layers).
pub fn build_merkle_tree(leaves: &[[u8; 32]]) -> ([u8; 32], Vec<Vec<[u8; 32]>>) {
    assert!(!leaves.is_empty(), "Cannot build Merkle tree from empty leaves");
    let mut layers: Vec<Vec<[u8; 32]>> = Vec::new();
    layers.push(leaves.to_vec());

    while layers.last().unwrap().len() > 1 {
        let current = layers.last().unwrap();
        let mut next_layer = Vec::new();

        let mut i = 0;
        while i < current.len() {
            if i + 1 < current.len() {
                next_layer.push(hash_pair(&current[i], &current[i + 1]));
            } else {
                // Odd leaf is paired with itself or promoted
                next_layer.push(current[i]);
            }
            i += 2;
        }
        layers.push(next_layer);
    }

    let root = layers.last().unwrap()[0];
    (root, layers)
}

/// Generates a cryptographic sibling proof for a specific leaf index in the tree.
pub fn generate_merkle_proof(layers: &[Vec<[u8; 32]>], mut leaf_index: usize) -> Vec<[u8; 32]> {
    let mut proof = Vec::new();

    for layer in &layers[..layers.len() - 1] {
        let pair_index = if leaf_index % 2 == 0 {
            leaf_index + 1
        } else {
            leaf_index - 1
        };

        if pair_index < layer.len() {
            proof.push(layer[pair_index]);
        }
        leaf_index /= 2;
    }

    proof
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_leaf_merkle() {
        let contributor = [0x42; 20];
        let amount = [0x10; 32];
        let leaf = compute_pouc_leaf(0, &contributor, &amount);

        let (root, layers) = build_merkle_tree(&[leaf]);
        assert_eq!(root, leaf);

        let proof = generate_merkle_proof(&layers, 0);
        assert!(verify_proof(&proof, &root, &leaf));
    }

    #[test]
    fn test_multi_leaf_merkle_verification() {
        let mut leaves = Vec::new();
        for i in 0..4 {
            let addr = [i as u8; 20];
            let amt = [i as u8 + 1; 32];
            leaves.push(compute_pouc_leaf(i, &addr, &amt));
        }

        let (root, layers) = build_merkle_tree(&leaves);

        for (i, leaf) in leaves.iter().enumerate() {
            let proof = generate_merkle_proof(&layers, i);
            assert!(verify_proof(&proof, &root, leaf));

            // Tamper test: wrong leaf must fail
            let tampered_leaf = [0xff; 32];
            assert!(!verify_proof(&proof, &root, &tampered_leaf));
        }
    }
}
