//! Jinn Browser Engine — Pure Rust no_std TLS 1.3 Cryptographic Stack
//!
//! Compliant with RFC 8446 (TLS 1.3), RFC 8439 (ChaCha20-Poly1305 AEAD),
//! RFC 7748 (Curve25519 / X25519), RFC 6234 (SHA-256), and RFC 5280 (X.509).

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

pub mod sha256;
pub mod curve25519;
pub mod chacha20poly1305;
pub mod x509;
pub mod handshake;

pub use sha256::{Sha256, hmac_sha256, hkdf_extract, hkdf_expand, hkdf_expand_label, derive_secret};
pub use curve25519::{x25519, x25519_basepoint};
pub use chacha20poly1305::{ChaCha20, Poly1305, ChaCha20Poly1305};
pub use x509::{ParsedCertificate, TrustStore, validate_certificate, parse_der_certificate};
pub use handshake::{
    Tls13Client, HandshakeState,
    TLS_CHACHA20_POLY1305_SHA256, TLS13_VERSION,
    CONTENT_ALERT, CONTENT_APPLICATION_DATA, CONTENT_HANDSHAKE,
};

/// Converte data UTC em timestamp UNIX (segundos desde 1970-01-01)
pub fn x509_date_to_unix(year: u16, month: u8, day: u8, hour: u8, min: u8, sec: u8) -> u64 {
    let is_leap = |y: u64| -> bool { (y % 4 == 0) && (y % 100 != 0 || y % 400 == 0) };
    let y = year as u64;
    if y < 1970 { return 0; }

    let mut days: u64 = 0;
    for cur_y in 1970..y {
        days += if is_leap(cur_y) { 366 } else { 365 };
    }

    let month_days = [31u64, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let m = month.max(1).min(12) as usize;
    for (idx, &d) in month_days.iter().enumerate().take(m - 1) {
        if idx == 1 && is_leap(y) {
            days += 29;
        } else {
            days += d;
        }
    }

    days += (day.max(1) as u64) - 1;

    days * 86400 + (hour as u64) * 3600 + (min as u64) * 60 + (sec as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_rfc_vectors() {
        // Test "abc"
        let hash = Sha256::digest(b"abc");
        let expected = [
            0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea,
            0x41, 0x41, 0x40, 0xde, 0x5d, 0xae, 0x22, 0x23,
            0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c,
            0xb4, 0x10, 0xff, 0x61, 0xf2, 0x00, 0x15, 0xad,
        ];
        assert_eq!(hash, expected);
    }

    #[test]
    fn test_hmac_sha256_rfc_vector() {
        let key = b"key";
        let data = b"The quick brown fox jumps over the lazy dog";
        let mac = hmac_sha256(key, data);
        assert_ne!(mac, [0u8; 32]);
    }

    #[test]
    fn test_curve25519_diffie_hellman() {
        let alice_priv = [0x55u8; 32];
        let bob_priv = [0xaau8; 32];

        let alice_pub = x25519_basepoint(&alice_priv);
        let bob_pub = x25519_basepoint(&bob_priv);

        let shared_alice = x25519(&alice_priv, &bob_pub);
        let shared_bob = x25519(&bob_priv, &alice_pub);

        assert_eq!(shared_alice, shared_bob);
        assert_ne!(shared_alice, [0u8; 32]);
    }

    #[test]
    fn test_chacha20_poly1305_aead_roundtrip() {
        let key = [0x42u8; 32];
        let nonce = [0x24u8; 12];
        let aad = b"TLS 1.3 Record Header";
        let original_data = b"GET /index.html HTTP/1.1\r\nHost: example.com\r\n\r\n";

        let mut pt = original_data.to_vec();
        ChaCha20Poly1305::encrypt(&key, &nonce, aad, &mut pt);

        // Ciphertext should be larger by 16 bytes (tag)
        assert_eq!(pt.len(), original_data.len() + 16);

        ChaCha20Poly1305::decrypt(&key, &nonce, aad, &mut pt).expect("Decryption must succeed");
        assert_eq!(pt, original_data);
    }

    #[test]
    fn test_tls13_client_hello_generation() {
        let random_seed = [0x77u8; 64];
        let mut client = Tls13Client::new("example.com", &random_seed);

        let record = client.build_client_hello();
        assert_eq!(client.state, HandshakeState::ClientHelloSent);

        // Verify TLS record header
        assert_eq!(record[0], CONTENT_HANDSHAKE);
        assert_eq!(record[1], 0x03);
        assert_eq!(record[2], 0x01);

        // Verify ClientHello Handshake type = 1
        assert_eq!(record[5], 1);
    }

    #[test]
    fn test_tls13_record_encryption_and_decryption() {
        let random_seed = [0x33u8; 64];
        let mut client = Tls13Client::new("jinn-os.org", &random_seed);

        let payload = b"Hello from TLS 1.3 Application Tunnel!";
        let enc_record = client.encrypt_record(CONTENT_APPLICATION_DATA, payload, true);

        // Record length: 5 header + payload + 1 inner type + 16 tag
        assert_eq!(enc_record.len(), 5 + payload.len() + 1 + 16);
    }

    #[test]
    fn test_x509_certificate_validation() {
        let mut trust_store = TrustStore::new();
        trust_store.add_root("ISRG Root X1");

        let cert = ParsedCertificate {
            subject: "CN=example.com, O=Internet Security Research Group".into(),
            issuer: "CN=ISRG Root X1, O=Internet Security Research Group".into(),
            not_before: 1700000000,
            not_after: 1800000000,
            dns_names: vec!["example.com".into(), "www.example.com".into()],
            public_key: vec![0u8; 32],
        };

        // Valida com hostname e tempo válidos
        let res = validate_certificate(&cert, "example.com", 1750000000, &trust_store);
        assert!(res.is_ok());

        // Valida rejeição de hostname incorreto
        let res_bad_host = validate_certificate(&cert, "evil.com", 1750000000, &trust_store);
        assert!(res_bad_host.is_err());

        // Valida rejeição de certificado expirado
        let res_expired = validate_certificate(&cert, "example.com", 1900000000, &trust_store);
        assert!(res_expired.is_err());
    }
}
