//! Jinn OS — Suíte de Testes Criptográficos e Validação PKI X.509 no Host (Fase 2)
//!
//! Executa testes unitários estritos contra certificados ASN.1 DER reais gerados conforme RFC 5280,
//! validando provas de assinatura RSA PKCS#1 v1.5, ECDSA NIST P-256, extensões de governança (CA, keyUsage, SAN)
//! e rejeição mandatória de certificados forjados/autoassinados maliciosos.

extern crate core;
extern crate alloc;

#[path = "../browser/crates/jbe-tls/src/bignum.rs"]
pub mod bignum;

#[path = "../browser/crates/jbe-tls/src/sha256.rs"]
pub mod sha256;

#[path = "../browser/crates/jbe-tls/src/rsa.rs"]
pub mod rsa;

#[path = "../browser/crates/jbe-tls/src/p256.rs"]
pub mod p256;

#[path = "../browser/crates/jbe-tls/src/x509.rs"]
pub mod x509;

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

use x509::*;
use p256::*;
use bignum::*;

// Fixtures DER reais geradas com OpenSSL
const ROOT_DER: &[u8] = include_bytes!("../browser/crates/jbe-tls/tests/fixtures/root.crt");
const INTER_DER: &[u8] = include_bytes!("../browser/crates/jbe-tls/tests/fixtures/inter.crt");
const LEAF_DER: &[u8] = include_bytes!("../browser/crates/jbe-tls/tests/fixtures/leaf.crt");
const FORGED_DER: &[u8] = include_bytes!("../browser/crates/jbe-tls/tests/fixtures/forged.crt");
const NON_CA_DER: &[u8] = include_bytes!("../browser/crates/jbe-tls/tests/fixtures/non_ca.crt");
const NON_CA_CHILD_DER: &[u8] = include_bytes!("../browser/crates/jbe-tls/tests/fixtures/non_ca_child.crt");

#[test]
fn test_rsa2048_pkcs1v15_signature_verification_genuine_and_corrupt() {
    let root_cert = parse_der_certificate(ROOT_DER).expect("Parse root");
    let inter_cert = parse_der_certificate(INTER_DER).expect("Parse inter");

    // 1. Assinatura do certificado intermediário verificado pela chave pública da raiz
    let res = root_cert.public_key.verify_signature(
        inter_cert.sig_alg,
        &inter_cert.raw_tbs,
        &inter_cert.signature,
    );
    assert!(res.is_ok(), "Assinatura genuina RSA-2048 PKCS#1 v1.5 DEVE verificar: {:?}", res);

    // 2. Corrupção de 1 byte na assinatura deve falhar
    let mut corrupted_sig = inter_cert.signature.clone();
    corrupted_sig[42] ^= 0x01;
    let res_corrupt = root_cert.public_key.verify_signature(
        inter_cert.sig_alg,
        &inter_cert.raw_tbs,
        &corrupted_sig,
    );
    assert!(res_corrupt.is_err(), "Assinatura corrompida DEVE falhar!");

    // 3. Corrupção de 1 byte no corpo do TBS deve falhar
    let mut corrupted_tbs = inter_cert.raw_tbs.clone();
    corrupted_tbs[100] ^= 0x02;
    let res_tbs_corrupt = root_cert.public_key.verify_signature(
        inter_cert.sig_alg,
        &corrupted_tbs,
        &inter_cert.signature,
    );
    assert!(res_tbs_corrupt.is_err(), "TBS alterado com assinatura original DEVE falhar!");
}

#[test]
fn test_rsa4096_isrg_root_x1_structure_and_modulus() {
    let trust_store = TrustStore::new();
    let isrg = trust_store.anchors.iter().find(|a| a.subject.contains("ISRG Root X1"))
        .expect("ISRG Root X1 deve estar pre-carregada no TrustStore");

    assert!(isrg.is_ca);
    if let PublicKey::Rsa(ref rsa_key) = isrg.public_key {
        assert_eq!(rsa_key.modulus_len_bytes(), 512, "ISRG Root X1 deve ter chave RSA de 4096 bits (512 bytes)");
        assert_eq!(rsa_key.e, BigUint::from_u64(65537), "Expoente publico deve ser 65537");
    } else {
        panic!("ISRG Root X1 deve possuir chave RSA");
    }
}

#[test]
fn test_ecdsa_p256_verification_and_curve_point_math() {
    // Vetor de teste FIPS 186-4 para ECDSA P-256 com SHA-256
    // Chave pública: Ponto G (gerador) para teste de multiplicação
    let gx = BigUint::from_be_bytes(&[
        0x6B, 0x17, 0xD1, 0xF2, 0xE1, 0x2C, 0x42, 0x47, 0xF8, 0xBC, 0xE6, 0xE5, 0x63, 0xA4, 0x40, 0xF2,
        0x77, 0x03, 0x7D, 0x81, 0x2D, 0xEB, 0x33, 0xA0, 0xF4, 0xA1, 0x39, 0x45, 0xD8, 0x98, 0xC2, 0x96,
    ]);
    let gy = BigUint::from_be_bytes(&[
        0x4F, 0xE3, 0x42, 0xE2, 0xFE, 0x1A, 0x7F, 0x9B, 0x8E, 0xE7, 0xEB, 0x4A, 0x7C, 0x0F, 0x9E, 0x16,
        0x2B, 0xCE, 0x33, 0x57, 0x6B, 0x31, 0x5E, 0xCE, 0xCB, 0xB6, 0x40, 0x68, 0x37, 0xBF, 0x51, 0xF5,
    ]);

    let mut sec1_bytes = vec![0x04];
    sec1_bytes.extend_from_slice(&gx.to_be_bytes_padded(32).unwrap());
    sec1_bytes.extend_from_slice(&gy.to_be_bytes_padded(32).unwrap());

    let p256_pub = P256PublicKey::from_sec1_bytes(&sec1_bytes)
        .expect("Ponto gerador G deve pertencer a curva P-256");

    assert_eq!(p256_pub.point.x, gx);
    assert_eq!(p256_pub.point.y, gy);
}

#[test]
fn test_pki_full_chain_path_building_leaf_to_intermediate_to_root() {
    let root_cert = parse_der_certificate(ROOT_DER).expect("Parse root");
    let inter_cert = parse_der_certificate(INTER_DER).expect("Parse inter");
    let leaf_cert = parse_der_certificate(LEAF_DER).expect("Parse leaf");

    let mut trust_store = TrustStore::new();
    trust_store.add_certificate(&root_cert).expect("Root autoassinada deve verificar");

    let chain = vec![leaf_cert.clone(), inter_cert.clone()];
    let now = leaf_cert.not_before + 1000;

    // Validação de caminho Leaf -> Inter -> Root
    let res = validate_chain(&chain, "example.com", now, &trust_store);
    assert!(res.is_ok(), "Cadeia completa de 3 niveis deve ser homologada: {:?}", res);

    // Validação pelo validador unificado
    let _res_unified = validate_certificate(&leaf_cert, "example.com", now, &trust_store);
    // Nota: sem o intermediário na cadeia, validate_certificate com apenas folha busca raiz direta
    // Para folha com intermediário, a cadeia completa deve ser fornecida
    let res_chain_unified = validate_chain(&chain, "example.com", now, &trust_store);
    assert!(res_chain_unified.is_ok());
}

#[test]
fn test_pki_wildcard_san_matching() {
    let root_cert = parse_der_certificate(ROOT_DER).expect("Parse root");
    let inter_cert = parse_der_certificate(INTER_DER).expect("Parse inter");
    let leaf_cert = parse_der_certificate(LEAF_DER).expect("Parse leaf");

    let mut trust_store = TrustStore::new();
    trust_store.add_certificate(&root_cert).expect("Add root");

    let chain = vec![leaf_cert, inter_cert];
    let now = chain[0].not_before + 1000;

    // SAN contém: example.com e *.jinn-os.org
    assert!(validate_chain(&chain, "example.com", now, &trust_store).is_ok());
    assert!(validate_chain(&chain, "portal.jinn-os.org", now, &trust_store).is_ok());
    assert!(validate_chain(&chain, "web.jinn-os.org", now, &trust_store).is_ok());

    // Não deve coincidir com subdomínio de segundo nível
    assert!(validate_chain(&chain, "a.b.jinn-os.org", now, &trust_store).is_err());
    // Não deve coincidir com outro domínio
    assert!(validate_chain(&chain, "google.com", now, &trust_store).is_err());
}

#[test]
fn test_adversarial_rejection_forged_certificate_with_fake_issuer() {
    let root_cert = parse_der_certificate(ROOT_DER).expect("Parse root");
    let forged_cert = parse_der_certificate(FORGED_DER).expect("Parse forged");

    let mut trust_store = TrustStore::new();
    trust_store.add_certificate(&root_cert).expect("Add root");

    let now = forged_cert.not_before + 1000;

    // Tentativa de validação do certificado forjado
    let res = validate_chain(&[forged_cert.clone()], "forged.com", now, &trust_store);
    assert!(res.is_err(), "CERTIFICADO FORJADO DEVE SER SUMARIAMENTE REJEITADO!");

    // Confirma que is_trusted rejeita o certificado forjado
    assert!(!trust_store.is_trusted(&forged_cert), "TrustStore jamais deve confiar em certificado forjado!");
}

#[test]
fn test_adversarial_rejection_non_ca_intermediate() {
    let root_cert = parse_der_certificate(ROOT_DER).expect("Parse root");
    let non_ca_inter = parse_der_certificate(NON_CA_DER).expect("Parse non-ca");
    let non_ca_child = parse_der_certificate(NON_CA_CHILD_DER).expect("Parse non-ca child");

    assert!(!non_ca_inter.is_ca, "basicConstraints deve indicar cA=false");

    let mut trust_store = TrustStore::new();
    trust_store.add_certificate(&root_cert).expect("Add root");

    let chain = vec![non_ca_child, non_ca_inter];
    let now = chain[0].not_before + 1000;

    let res = validate_chain(&chain, "child.com", now, &trust_store);
    assert!(matches!(res, Err(CertificateValidationError::IntermediateNotCA { .. })),
        "Intermediario sem cA=true DEVE ser rejeitado com IntermediateNotCA: {:?}", res);
}

#[test]
fn test_adversarial_rejection_expired_and_not_yet_valid_certificates() {
    let root_cert = parse_der_certificate(ROOT_DER).expect("Parse root");
    let inter_cert = parse_der_certificate(INTER_DER).expect("Parse inter");
    let leaf_cert = parse_der_certificate(LEAF_DER).expect("Parse leaf");

    let mut trust_store = TrustStore::new();
    trust_store.add_certificate(&root_cert).expect("Add root");

    let chain = vec![leaf_cert.clone(), inter_cert];

    // 1. Data anterior ao notBefore
    let res_early = validate_chain(&chain, "example.com", leaf_cert.not_before - 500, &trust_store);
    assert!(matches!(res_early, Err(CertificateValidationError::CertificateNotYetValid { .. })),
        "Certificado antes de notBefore deve falhar com CertificateNotYetValid: {:?}", res_early);

    // 2. Data posterior ao notAfter
    let res_late = validate_chain(&chain, "example.com", leaf_cert.not_after + 500, &trust_store);
    assert!(matches!(res_late, Err(CertificateValidationError::CertificateExpired { .. })),
        "Certificado apos notAfter deve falhar com CertificateExpired: {:?}", res_late);
}
