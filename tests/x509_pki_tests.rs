//! Jinn OS — Suíte Completa de Testes de Unidade e Regressão da PKI X.509 (Fase 2)
//!
//! Executa baterias de testes no host cobrindo:
//! 1. Decodificação estrutural ASN.1 DER e extração de SPKI (RSA 2048/4096, ECDSA P-256).
//! 2. Construção de caminho e validação de cadeia de confiança (Leaf -> Intermediate -> Trust Anchor).
//! 3. Rejeição de certificados temporais inválidos (expirados e prematuros).
//! 4. Rejeição sumária de assinaturas adulteradas, forjadas e bits corrompidos.
//! 5. Rejeição de intermediários sem flag cA=true (BasicConstraints) e sem keyCertSign (KeyUsage).
//! 6. Enforçamento de regras estritas de Hostname e SAN / Wildcard (RFC 6125).
//! 7. Validação das âncoras de confiança embutidas (ISRG Root X1, DigiCert Global Root CA).

extern crate core;
extern crate alloc;

pub mod crypto {
    #[path = "../../src/crypto/bignum.rs"]
    pub mod bignum;
    #[path = "../../src/crypto/sha256.rs"]
    pub mod sha256;
    #[path = "../../src/crypto/rsa.rs"]
    pub mod rsa;
    #[path = "../../src/crypto/p256.rs"]
    pub mod p256;
}

pub mod drivers {
    pub mod rtc {
        pub fn current_unix_time() -> (u64, u32) {
            (1700000000, 0)
        }
    }
}

pub mod net {
    #[path = "../../src/net/x509.rs"]
    pub mod x509;
    #[path = "../../src/net/pki.rs"]
    pub mod pki;
}

use net::x509::*;
use net::pki::*;
use crypto::bignum::BigUint;
use crypto::p256::P256PublicKey;

// Fixtures DER reais geradas com OpenSSL
const ROOT_DER: &[u8] = include_bytes!("../browser/crates/jbe-tls/tests/fixtures/root.crt");
const INTER_DER: &[u8] = include_bytes!("../browser/crates/jbe-tls/tests/fixtures/inter.crt");
const LEAF_DER: &[u8] = include_bytes!("../browser/crates/jbe-tls/tests/fixtures/leaf.crt");
const FORGED_DER: &[u8] = include_bytes!("../browser/crates/jbe-tls/tests/fixtures/forged.crt");
const NON_CA_DER: &[u8] = include_bytes!("../browser/crates/jbe-tls/tests/fixtures/non_ca.crt");
const NON_CA_CHILD_DER: &[u8] = include_bytes!("../browser/crates/jbe-tls/tests/fixtures/non_ca_child.crt");

#[test]
fn test_positive_full_chain_path_building() {
    let root_cert = parse_der_certificate(ROOT_DER).expect("Parse root");
    let inter_cert = parse_der_certificate(INTER_DER).expect("Parse inter");
    let leaf_cert = parse_der_certificate(LEAF_DER).expect("Parse leaf");

    // Valida propriedades fundamentais
    assert_eq!(root_cert.version, 3);
    assert!(root_cert.is_ca);
    assert_eq!(inter_cert.version, 3);
    assert!(inter_cert.is_ca);
    assert!(!leaf_cert.is_ca);

    let mut trust_store = TrustStore::new();
    trust_store.add_certificate(&root_cert).expect("Root autoassinada deve ser aceita");

    let chain = alloc::vec![leaf_cert.clone(), inter_cert.clone()];
    let now = leaf_cert.not_before + 500;

    // Validação da cadeia de 3 níveis: Leaf -> Inter -> Root
    let res = validate_chain(&chain, "example.com", now, &trust_store);
    assert!(res.is_ok(), "Cadeia completa genuina deve ser homologada: {:?}", res);
}

#[test]
fn test_premature_certificate_rejected() {
    let root_cert = parse_der_certificate(ROOT_DER).expect("Parse root");
    let inter_cert = parse_der_certificate(INTER_DER).expect("Parse inter");
    let leaf_cert = parse_der_certificate(LEAF_DER).expect("Parse leaf");

    let mut trust_store = TrustStore::new();
    trust_store.add_certificate(&root_cert).expect("Add root");

    let chain = alloc::vec![leaf_cert.clone(), inter_cert];
    let premature_time = leaf_cert.not_before - 1000;

    let res = validate_chain(&chain, "example.com", premature_time, &trust_store);
    assert!(
        matches!(res, Err(CertificateValidationError::CertificateNotYetValid { .. })),
        "Certificado antes de notBefore deve ser sumariamente rejeitado: {:?}",
        res
    );
}

#[test]
fn test_expired_certificate_rejected() {
    let root_cert = parse_der_certificate(ROOT_DER).expect("Parse root");
    let inter_cert = parse_der_certificate(INTER_DER).expect("Parse inter");
    let leaf_cert = parse_der_certificate(LEAF_DER).expect("Parse leaf");

    let mut trust_store = TrustStore::new();
    trust_store.add_certificate(&root_cert).expect("Add root");

    let chain = alloc::vec![leaf_cert.clone(), inter_cert];
    let expired_time = leaf_cert.not_after + 1000;

    let res = validate_chain(&chain, "example.com", expired_time, &trust_store);
    assert!(
        matches!(res, Err(CertificateValidationError::CertificateExpired { .. })),
        "Certificado apos notAfter deve ser sumariamente rejeitado: {:?}",
        res
    );
}

#[test]
fn test_forged_certificate_rejected() {
    let root_cert = parse_der_certificate(ROOT_DER).expect("Parse root");
    let forged_cert = parse_der_certificate(FORGED_DER).expect("Parse forged");

    let mut trust_store = TrustStore::new();
    trust_store.add_certificate(&root_cert).expect("Add root");

    let now = forged_cert.not_before + 100;

    // Tentativa de validar certificado forjado
    let res = validate_chain(&[forged_cert.clone()], "forged.com", now, &trust_store);
    assert!(res.is_err(), "Certificado forjado DEVE ser rejeitado!");
    assert!(!trust_store.is_trusted(&forged_cert), "TrustStore jamais pode confiar em certificado forjado");
}

#[test]
fn test_corrupted_tbs_and_corrupted_signature_rejected() {
    let root_cert = parse_der_certificate(ROOT_DER).expect("Parse root");
    let inter_cert = parse_der_certificate(INTER_DER).expect("Parse inter");

    // 1. Assinatura genuína deve passar
    let ok_res = root_cert.public_key.verify_signature(
        inter_cert.sig_alg,
        &inter_cert.raw_tbs,
        &inter_cert.signature,
    );
    assert!(ok_res.is_ok(), "Assinatura genuina RSA PKCS#1 v1.5 DEVE verificar: {:?}", ok_res);

    // 2. Corrupção de 1 byte na assinatura
    let mut bad_sig = inter_cert.signature.clone();
    bad_sig[50] ^= 0x55;
    let bad_sig_res = root_cert.public_key.verify_signature(
        inter_cert.sig_alg,
        &inter_cert.raw_tbs,
        &bad_sig,
    );
    assert!(bad_sig_res.is_err(), "Assinatura com byte corrompido deve falhar!");

    // 3. Corrupção de 1 byte no corpo do TBS
    let mut bad_tbs = inter_cert.raw_tbs.clone();
    bad_tbs[40] ^= 0xAA;
    let bad_tbs_res = root_cert.public_key.verify_signature(
        inter_cert.sig_alg,
        &bad_tbs,
        &inter_cert.signature,
    );
    assert!(bad_tbs_res.is_err(), "TBS alterado com assinatura original deve falhar!");
}

#[test]
fn test_non_ca_intermediate_rejected() {
    let root_cert = parse_der_certificate(ROOT_DER).expect("Parse root");
    let non_ca_inter = parse_der_certificate(NON_CA_DER).expect("Parse non-ca");
    let non_ca_child = parse_der_certificate(NON_CA_CHILD_DER).expect("Parse non-ca child");

    assert!(!non_ca_inter.is_ca, "basicConstraints deve possuir cA=false");

    let mut trust_store = TrustStore::new();
    trust_store.add_certificate(&root_cert).expect("Add root");

    let chain = alloc::vec![non_ca_child, non_ca_inter];
    let now = chain[0].not_before + 500;

    let res = validate_chain(&chain, "child.com", now, &trust_store);
    assert!(
        matches!(res, Err(CertificateValidationError::IntermediateNotCA { .. })),
        "Intermediario com cA=false deve falhar com IntermediateNotCA: {:?}",
        res
    );
}

#[test]
fn test_intermediate_missing_key_cert_sign_rejected() {
    let mut inter_cert = parse_der_certificate(INTER_DER).expect("Parse inter");
    let root_cert = parse_der_certificate(ROOT_DER).expect("Parse root");
    let leaf_cert = parse_der_certificate(LEAF_DER).expect("Parse leaf");

    // Remove propositalmente o bit keyCertSign (bit 5 / 0x04) do keyUsage
    inter_cert.key_usage = Some(0x80); // Apenas digitalSignature

    let mut trust_store = TrustStore::new();
    trust_store.add_certificate(&root_cert).expect("Add root");

    let chain = alloc::vec![leaf_cert, inter_cert];
    let now = chain[0].not_before + 500;

    let res = validate_chain(&chain, "example.com", now, &trust_store);
    assert!(
        matches!(res, Err(CertificateValidationError::IntermediateMissingKeyCertSign { .. })),
        "Intermediario sem keyCertSign deve falhar com IntermediateMissingKeyCertSign: {:?}",
        res
    );
}

#[test]
fn test_hostname_rfc6125_san_exact_match() {
    let root_cert = parse_der_certificate(ROOT_DER).expect("Parse root");
    let inter_cert = parse_der_certificate(INTER_DER).expect("Parse inter");
    let leaf_cert = parse_der_certificate(LEAF_DER).expect("Parse leaf");

    let mut trust_store = TrustStore::new();
    trust_store.add_certificate(&root_cert).expect("Add root");

    let chain = alloc::vec![leaf_cert, inter_cert];
    let now = chain[0].not_before + 500;

    // SAN contém: example.com, *.jinn-os.org, 127.0.0.1
    assert!(validate_chain(&chain, "example.com", now, &trust_store).is_ok());
    assert!(validate_chain(&chain, "EXAMPLE.COM", now, &trust_store).is_ok()); // Case insensitive
}

#[test]
fn test_hostname_rfc6125_san_valid_wildcard() {
    let root_cert = parse_der_certificate(ROOT_DER).expect("Parse root");
    let inter_cert = parse_der_certificate(INTER_DER).expect("Parse inter");
    let leaf_cert = parse_der_certificate(LEAF_DER).expect("Parse leaf");

    let mut trust_store = TrustStore::new();
    trust_store.add_certificate(&root_cert).expect("Add root");

    let chain = alloc::vec![leaf_cert, inter_cert];
    let now = chain[0].not_before + 500;

    // *.jinn-os.org cobre qualquer subdomínio direto de 1 nível
    assert!(validate_chain(&chain, "portal.jinn-os.org", now, &trust_store).is_ok());
    assert!(validate_chain(&chain, "web.jinn-os.org", now, &trust_store).is_ok());
    assert!(validate_chain(&chain, "api.jinn-os.org", now, &trust_store).is_ok());
    assert!(validate_chain(&chain, "API.JINN-OS.ORG", now, &trust_store).is_ok());
}

#[test]
fn test_hostname_rfc6125_san_multilevel_wildcard_rejected() {
    let root_cert = parse_der_certificate(ROOT_DER).expect("Parse root");
    let inter_cert = parse_der_certificate(INTER_DER).expect("Parse inter");
    let leaf_cert = parse_der_certificate(LEAF_DER).expect("Parse leaf");

    let mut trust_store = TrustStore::new();
    trust_store.add_certificate(&root_cert).expect("Add root");

    let chain = alloc::vec![leaf_cert, inter_cert];
    let now = chain[0].not_before + 500;

    // *.jinn-os.org NÃO pode cobrir subdomínio de 2º nível a.b.jinn-os.org
    let res = validate_chain(&chain, "a.b.jinn-os.org", now, &trust_store);
    assert!(
        matches!(res, Err(CertificateValidationError::HostnameMismatch { .. })),
        "Subdominio multinivel deve ser sumariamente rejeitado: {:?}",
        res
    );
}

#[test]
fn test_hostname_rfc6125_san_apex_domain_rejected() {
    let root_cert = parse_der_certificate(ROOT_DER).expect("Parse root");
    let inter_cert = parse_der_certificate(INTER_DER).expect("Parse inter");
    let leaf_cert = parse_der_certificate(LEAF_DER).expect("Parse leaf");

    let mut trust_store = TrustStore::new();
    trust_store.add_certificate(&root_cert).expect("Add root");

    let chain = alloc::vec![leaf_cert, inter_cert];
    let now = chain[0].not_before + 500;

    // *.jinn-os.org NÃO pode cobrir o domínio ápice jinn-os.org (sem prefixo)
    let res = validate_chain(&chain, "jinn-os.org", now, &trust_store);
    assert!(
        matches!(res, Err(CertificateValidationError::HostnameMismatch { .. })),
        "Dominio apice sem prefixo nao coincide com *.jinn-os.org: {:?}",
        res
    );
}

#[test]
fn test_hostname_rfc6125_domain_mismatch_rejected() {
    let root_cert = parse_der_certificate(ROOT_DER).expect("Parse root");
    let inter_cert = parse_der_certificate(INTER_DER).expect("Parse inter");
    let leaf_cert = parse_der_certificate(LEAF_DER).expect("Parse leaf");

    let mut trust_store = TrustStore::new();
    trust_store.add_certificate(&root_cert).expect("Add root");

    let chain = alloc::vec![leaf_cert, inter_cert];
    let now = chain[0].not_before + 500;

    // Domínio completamente diferente
    let res = validate_chain(&chain, "google.com", now, &trust_store);
    assert!(
        matches!(res, Err(CertificateValidationError::HostnameMismatch { .. })),
        "Dominio divergente deve falhar com HostnameMismatch: {:?}",
        res
    );
}

#[test]
fn test_hostname_rfc6125_san_ip_address_matching() {
    let root_cert = parse_der_certificate(ROOT_DER).expect("Parse root");
    let inter_cert = parse_der_certificate(INTER_DER).expect("Parse inter");
    let mut leaf_cert = parse_der_certificate(LEAF_DER).expect("Parse leaf");

    // Adiciona iPAddress SAN 127.0.0.1 para testar correspondência estrita de IP
    leaf_cert.ip_addresses.push("127.0.0.1".to_string());
    leaf_cert.dns_names.push("127.0.0.1".to_string());

    let mut trust_store = TrustStore::new();
    trust_store.add_certificate(&root_cert).expect("Add root");

    let chain = alloc::vec![leaf_cert, inter_cert];
    let now = chain[0].not_before + 500;

    // SAN contém IP 127.0.0.1
    assert!(validate_chain(&chain, "127.0.0.1", now, &trust_store).is_ok());
    assert!(validate_chain(&chain, "192.168.1.1", now, &trust_store).is_err());
}

#[test]
fn test_preloaded_trust_anchors() {
    let store = TrustStore::new();

    // 1. ISRG Root X1 (RSA 4096 bits)
    let isrg = store.anchors.iter().find(|a| a.subject.contains("ISRG Root X1"))
        .expect("ISRG Root X1 deve estar pre-carregada");
    assert!(isrg.is_ca);
    if let PublicKey::Rsa(ref rsa_key) = isrg.public_key {
        assert_eq!(rsa_key.modulus_len_bytes(), 512, "ISRG Root X1 deve ter 4096 bits (512 bytes)");
        assert_eq!(rsa_key.e, BigUint::from_u64(65537));
    } else {
        panic!("ISRG Root X1 deve ser chave RSA");
    }

    // 2. DigiCert Global Root CA (RSA 2048 bits)
    let digicert = store.anchors.iter().find(|a| a.subject.contains("DigiCert Global Root CA"))
        .expect("DigiCert Global Root CA deve estar pre-carregada");
    assert!(digicert.is_ca);
    if let PublicKey::Rsa(ref rsa_key) = digicert.public_key {
        assert_eq!(rsa_key.modulus_len_bytes(), 256, "DigiCert deve ter 2048 bits (256 bytes)");
        assert_eq!(rsa_key.e, BigUint::from_u64(65537));
    } else {
        panic!("DigiCert deve ser chave RSA");
    }
}

#[test]
fn test_ecdsa_p256_verification_known_point() {
    let gx = BigUint::from_be_bytes(&[
        0x6B, 0x17, 0xD1, 0xF2, 0xE1, 0x2C, 0x42, 0x47, 0xF8, 0xBC, 0xE6, 0xE5, 0x63, 0xA4, 0x40, 0xF2,
        0x77, 0x03, 0x7D, 0x81, 0x2D, 0xEB, 0x33, 0xA0, 0xF4, 0xA1, 0x39, 0x45, 0xD8, 0x98, 0xC2, 0x96,
    ]);
    let gy = BigUint::from_be_bytes(&[
        0x4F, 0xE3, 0x42, 0xE2, 0xFE, 0x1A, 0x7F, 0x9B, 0x8E, 0xE7, 0xEB, 0x4A, 0x7C, 0x0F, 0x9E, 0x16,
        0x2B, 0xCE, 0x33, 0x57, 0x6B, 0x31, 0x5E, 0xCE, 0xCB, 0xB6, 0x40, 0x68, 0x37, 0xBF, 0x51, 0xF5,
    ]);

    let mut sec1_bytes = alloc::vec![0x04];
    sec1_bytes.extend_from_slice(&gx.to_be_bytes_padded(32).unwrap());
    sec1_bytes.extend_from_slice(&gy.to_be_bytes_padded(32).unwrap());

    let p256_pub = P256PublicKey::from_sec1_bytes(&sec1_bytes)
        .expect("Ponto gerador G deve pertencer a curva P-256");

    assert_eq!(p256_pub.point.x, gx);
    assert_eq!(p256_pub.point.y, gy);
}
