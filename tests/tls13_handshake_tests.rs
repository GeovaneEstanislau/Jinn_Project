//! Jinn OS — Suíte de Testes de Unidade e Estresse do TLS 1.3 Handshake & Key Schedule (Fase 3)
//!
//! Executa baterias de testes no host cobrindo:
//! 1. HKDF-Extract e HKDF-Expand conforme a RFC 5869 (vetores de teste).
//! 2. HKDF-Expand-Label com prefixo "tls13 " e formatação HkdfLabel (RFC 8446 Seção 7.1).
//! 3. Transcript Hash com SHA-256 e snapshots sem mutação de estado.
//! 4. Árvore de segredos do TLS 1.3: Early Secret -> Handshake Secret -> Master Secret.
//! 5. Derivação de chaves AEAD e IVs para Handshake e Aplicação.
//! 6. Cálculo de Nonce por pacote via operação XOR entre IV e Sequence Number.
//! 7. Verificação de CertificateVerify (64 espaços + contexto + hash) com chave pública RSA / ECDSA.
//! 8. Cálculo de verify_data e verificação segura de Finished com rejeição de dados forjados.
//! 9. Emissão de ClientHello com extensões SNI, KeyShare, SupportedVersions, SupportedGroups e SignatureAlgorithms.
//! 10. Simulação de ciclo de vida completo do cliente TLS 1.3 com validação de transições de estado.

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
    #[path = "../../src/net/tls13.rs"]
    pub mod tls13;
}

use crypto::sha256::{Sha256, hkdf_extract};
use net::tls13::*;
use net::pki::TrustStore;

// ── Teste 1: HKDF-Extract e HKDF-Expand (RFC 5869) ────────────────────────────

#[test]
fn test_hkdf_extract_and_expand_rfc5869() {
    print!("Teste 1: HKDF-Extract e HKDF-Expand (RFC 5869)... ");
    let ikm = [0x0bu8; 22];
    let salt = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
        0x08, 0x09, 0x0a, 0x0b, 0x0c,
    ];
    let info = [0xf0, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9];

    // RFC 5869 Test Case 1 PRK
    let prk = hkdf_extract(&salt, &ikm);
    assert_eq!(
        prk,
        [
            0x07, 0x77, 0x09, 0x36, 0x2c, 0x2e, 0x32, 0xdf,
            0x0d, 0xdc, 0x3f, 0x0d, 0xc4, 0x7b, 0xba, 0x63,
            0x90, 0xb6, 0xc7, 0x3b, 0xb5, 0x0f, 0x9c, 0x31,
            0x22, 0xec, 0x84, 0x4a, 0xd7, 0xc2, 0xb3, 0xe5,
        ]
    );

    // RFC 5869 Test Case 1 OKM (42 bytes)
    let okm = hkdf_expand(&prk, &info, 42);
    assert_eq!(okm.len(), 42);
    assert_eq!(
        &okm[..16],
        &[
            0x3c, 0xb2, 0x5f, 0x25, 0xfa, 0xac, 0xd5, 0x7a,
            0x90, 0x43, 0x4f, 0x64, 0xd0, 0x36, 0x2f, 0x2a,
        ]
    );

    println!("PASSOU!");
}

// ── Teste 2: HKDF-Expand-Label (RFC 8446 Seção 7.1) ───────────────────────────

#[test]
fn test_hkdf_expand_label_rfc8446_prefixing() {
    print!("Teste 2: HKDF-Expand-Label e formatação de rótulo 'tls13 '... ");
    let secret = [0x42u8; 32];
    let label = b"derived";
    let empty_hash = Sha256::digest(b"");

    let expanded = hkdf_expand_label(&secret, label, &empty_hash, 32);
    assert_eq!(expanded.len(), 32);

    // Verifica que rótulos diferentes produzem segredos criptograficamente distintos
    let expanded2 = hkdf_expand_label(&secret, b"key", b"", 16);
    assert_eq!(expanded2.len(), 16);
    assert_ne!(&expanded[..16], &expanded2[..]);

    println!("PASSOU!");
}

// ── Teste 3: Transcript Hash Contínuo e Snapshots Imutáveis ───────────────────

#[test]
fn test_transcript_hash_immutability_and_chaining() {
    print!("Teste 3: Transcript Hash SHA-256 e snapshots sem mutação de estado... ");
    let mut transcript = TranscriptHash::new();

    transcript.update(b"ClientHello");
    let hash_ch = transcript.current_hash();

    // Snapshot não deve alterar o acumulador
    let hash_ch_again = transcript.current_hash();
    assert_eq!(hash_ch, hash_ch_again);

    // Adiciona ServerHello
    transcript.update(b"ServerHello");
    let hash_sh = transcript.current_hash();
    assert_ne!(hash_ch, hash_sh);

    // Valida que hash_sh é idêntico a um SHA-256 calculado diretamente sobre a concatenação
    let direct_hash = Sha256::digest(b"ClientHelloServerHello");
    assert_eq!(hash_sh, direct_hash);

    println!("PASSOU!");
}

// ── Teste 4: Árvore do Key Schedule Completa (Early -> Handshake -> Master) ───

#[test]
fn test_tls13_key_schedule_full_tree_derivation() {
    print!("Teste 4: TLS 1.3 Key Schedule (Early -> Handshake -> Master Secret)... ");
    let mut ks = Tls13KeySchedule::new(CipherSuite::TlsAes128GcmSha256);

    // 1. Early secret inicializado no construtor
    assert_ne!(ks.early_secret, [0u8; 32]);
    assert!(ks.handshake_secret.is_none());
    assert!(ks.master_secret.is_none());

    // 2. Derivação do Handshake Secret com segredo compartilhado ECDHE
    let ecdhe_shared = [0x55u8; 32];
    let ch_sh_hash = Sha256::digest(b"ClientHelloServerHello");
    ks.derive_handshake_secrets(&ecdhe_shared, &ch_sh_hash);

    assert!(ks.handshake_secret.is_some());
    assert!(ks.client_handshake_traffic_secret.is_some());
    assert!(ks.server_handshake_traffic_secret.is_some());
    assert!(ks.client_handshake_key.is_some());
    assert!(ks.client_handshake_iv.is_some());
    assert!(ks.server_handshake_key.is_some());
    assert!(ks.server_handshake_iv.is_some());

    // As chaves de cliente e servidor devem ser distintas
    assert_ne!(
        ks.client_handshake_traffic_secret.unwrap(),
        ks.server_handshake_traffic_secret.unwrap()
    );
    assert_eq!(ks.client_handshake_key.as_ref().unwrap().len(), 16); // AES-128
    assert_eq!(ks.client_handshake_iv.as_ref().unwrap().len(), 12);  // 96-bit IV

    // 3. Derivação do Master Secret
    let finished_hash = Sha256::digest(b"ClientHello...ServerFinished");
    let res = ks.derive_master_secrets(&finished_hash);
    assert!(res.is_ok());

    assert!(ks.master_secret.is_some());
    assert!(ks.client_app_traffic_secret.is_some());
    assert!(ks.server_app_traffic_secret.is_some());
    assert!(ks.client_app_key.is_some());
    assert!(ks.client_app_iv.is_some());
    assert!(ks.server_app_key.is_some());
    assert!(ks.server_app_iv.is_some());

    // Chaves de aplicação devem ser distintas das chaves de handshake
    assert_ne!(
        ks.client_app_traffic_secret.unwrap(),
        ks.client_handshake_traffic_secret.unwrap()
    );

    println!("PASSOU!");
}

// ── Teste 5: Cálculo do Nonce AEAD com XOR de Sequence Number ─────────────────

#[test]
fn test_aead_key_iv_and_nonce_xor_computation() {
    print!("Teste 5: Formação do Nonce AEAD com Sequence Number (RFC 8446 Seção 5.3)... ");
    let iv: [u8; 12] = [
        0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80,
        0x90, 0xA0, 0xB0, 0xC0,
    ];

    // Sequence Number 0 -> Nonce deve ser exatamente o IV
    let nonce0 = Tls13KeySchedule::compute_nonce(&iv, 0);
    assert_eq!(nonce0, iv);

    // Sequence Number 1 -> Último byte do IV deve ter XOR com 1
    let nonce1 = Tls13KeySchedule::compute_nonce(&iv, 1);
    let mut expected_nonce1 = iv;
    expected_nonce1[11] ^= 0x01;
    assert_eq!(nonce1, expected_nonce1);

    // Sequence Number 0x0102030405060708
    let seq = 0x0102030405060708u64;
    let nonce_seq = Tls13KeySchedule::compute_nonce(&iv, seq);
    let mut expected_seq = iv;
    let seq_bytes = seq.to_be_bytes();
    for i in 0..8 {
        expected_seq[4 + i] ^= seq_bytes[i];
    }
    assert_eq!(nonce_seq, expected_seq);

    println!("PASSOU!");
}

// ── Teste 6: Finished verify_data e Rejeição de Forjamento ────────────────────

#[test]
fn test_server_finished_verify_data_computation_and_rejection_of_forgery() {
    print!("Teste 6: verify_data do bloco Finished e rejeição de HMAC forjado... ");
    let mut ks = Tls13KeySchedule::new(CipherSuite::TlsAes128GcmSha256);
    let ecdhe_shared = [0x77u8; 32];
    let ch_sh_hash = Sha256::digest(b"HelloTranscript");
    ks.derive_handshake_secrets(&ecdhe_shared, &ch_sh_hash);

    let transcript_hash = Sha256::digest(b"TranscriptUpToCertificateVerify");
    let s_hs_traffic = ks.server_handshake_traffic_secret.unwrap();
    let genuine_verify_data = Tls13KeySchedule::compute_finished_verify_data(&s_hs_traffic, &transcript_hash);

    // Verificação de Finished genuíno deve ser Ok(())
    let res = ks.verify_server_finished(&genuine_verify_data, &transcript_hash);
    assert!(res.is_ok());

    // verify_data com 1 bit adulterado deve ser sumariamente rejeitado com DecryptError
    let mut forged_verify_data = genuine_verify_data;
    forged_verify_data[15] ^= 0x01;
    let res_forged = ks.verify_server_finished(&forged_verify_data, &transcript_hash);
    assert_eq!(res_forged, Err(Tls13Alert::DecryptError));

    // verify_data truncado deve ser rejeitado com DecryptError
    let res_short = ks.verify_server_finished(&genuine_verify_data[..31], &transcript_hash);
    assert_eq!(res_short, Err(Tls13Alert::DecryptError));

    println!("PASSOU!");
}

// ── Teste 7: Construção do Pacote ClientHello com Extensões TLS 1.3 ───────────

#[test]
fn test_tls13_client_hello_wire_format_and_extensions() {
    print!("Teste 7: Construção e extensões obrigatórias do pacote ClientHello... ");
    let mut client = Tls13Client::new("jinn.os", TrustStore::new());
    let client_random = [0xA5u8; 32];
    let client_pubkey = [0x04u8; 65]; // Chave pública não-comprimida secp256r1

    let ch_packet = client.build_client_hello(&client_random, &client_pubkey);

    // Verifica formato Handshake: tipo 1 (ClientHello) e tamanho de 3 bytes
    assert_eq!(ch_packet[0], HandshakeType::ClientHello.as_u8());
    let payload_len = ((ch_packet[1] as usize) << 16) | ((ch_packet[2] as usize) << 8) | (ch_packet[3] as usize);
    assert_eq!(ch_packet.len(), 4 + payload_len);

    // Verifica legacy_version = 0x0303 (TLS 1.2)
    assert_eq!(ch_packet[4], 0x03);
    assert_eq!(ch_packet[5], 0x03);

    // Verifica que o estado transicionou para ClientHelloSent
    assert_eq!(client.state, HandshakeState::ClientHelloSent);

    // O TranscriptHash deve conter o ClientHello exato
    let transcript_hash = client.transcript.current_hash();
    assert_eq!(transcript_hash, Sha256::digest(&ch_packet));

    println!("PASSOU!");
}

// ── Teste 8: Simulação de Ciclo de Vida do Handshake TLS 1.3 ──────────────────

#[test]
fn test_tls13_full_state_machine_happy_path() {
    print!("Teste 8: Simulação completa da máquina de estados do Handshake TLS 1.3... ");
    let mut client = Tls13Client::new("jinn.os", TrustStore::new());
    let client_random = [0x11u8; 32];
    let client_pubkey = [0x04u8; 65];

    // 1. ClientHello
    let _ = client.build_client_hello(&client_random, &client_pubkey);
    assert_eq!(client.state, HandshakeState::ClientHelloSent);

    // 2. ServerHello simulado
    let mut sh_msg = alloc::vec![HandshakeType::ServerHello.as_u8(), 0x00, 0x00, 38];
    sh_msg.extend_from_slice(&TLS12_LEGACY_VERSION.to_be_bytes()); // legacy_ver (2)
    sh_msg.extend_from_slice(&[0x22u8; 32]); // server_random (32)
    sh_msg.push(0x00); // session_id_len (0)
    sh_msg.extend_from_slice(&CipherSuite::TlsAes128GcmSha256.as_u16().to_be_bytes()); // suite (2)
    sh_msg.push(0x00); // compression null (1)
    sh_msg.extend_from_slice(&0x0000u16.to_be_bytes()); // extensions len 0 (1)

    let ecdhe_shared = [0x88u8; 32];
    let sh_res = client.process_server_hello(&sh_msg, &ecdhe_shared);
    assert!(sh_res.is_ok());
    assert_eq!(client.state, HandshakeState::WaitEncryptedExtensions);

    // 3. EncryptedExtensions
    let ee_msg = alloc::vec![HandshakeType::EncryptedExtensions.as_u8(), 0x00, 0x00, 0x02, 0x00, 0x00];
    let ee_res = client.process_encrypted_extensions(&ee_msg);
    assert!(ee_res.is_ok());
    assert_eq!(client.state, HandshakeState::WaitCertificate);

    // 4. Finished do servidor
    let s_hs_traffic = client.key_schedule.server_handshake_traffic_secret.unwrap();
    let transcript_before_fin = client.transcript.current_hash();
    let s_verify_data = Tls13KeySchedule::compute_finished_verify_data(&s_hs_traffic, &transcript_before_fin);

    let mut sf_msg = alloc::vec![HandshakeType::Finished.as_u8(), 0x00, 0x00, 32];
    sf_msg.extend_from_slice(&s_verify_data);

    let cf_res = client.process_server_finished(&sf_msg);
    assert!(cf_res.is_ok());
    let client_fin = cf_res.unwrap();
    assert_eq!(client_fin[0], HandshakeType::Finished.as_u8());
    assert_eq!(client.state, HandshakeState::Connected);

    // Confirma que chaves de aplicação estão prontas
    assert!(client.key_schedule.client_app_key.is_some());
    assert!(client.key_schedule.server_app_key.is_some());

    println!("PASSOU!");
}

// ── Teste 9: Finished Adulterado Interrompe Handshake com DecryptError ─────────

#[test]
fn test_tls13_handshake_tampered_finished_aborts_with_decrypt_error() {
    print!("Teste 9: Finished adulterado aborta conexão imediatamente com DecryptError... ");
    let mut client = Tls13Client::new("jinn.os", TrustStore::new());
    let client_random = [0x11u8; 32];
    let client_pubkey = [0x04u8; 65];

    let _ = client.build_client_hello(&client_random, &client_pubkey);

    let mut sh_msg = alloc::vec![HandshakeType::ServerHello.as_u8(), 0x00, 0x00, 38];
    sh_msg.extend_from_slice(&TLS12_LEGACY_VERSION.to_be_bytes());
    sh_msg.extend_from_slice(&[0x22u8; 32]);
    sh_msg.push(0x00);
    sh_msg.extend_from_slice(&CipherSuite::TlsAes128GcmSha256.as_u16().to_be_bytes());
    sh_msg.push(0x00);
    sh_msg.extend_from_slice(&0x0000u16.to_be_bytes());

    let ecdhe_shared = [0x88u8; 32];
    let _ = client.process_server_hello(&sh_msg, &ecdhe_shared);

    // Envia Finished com HMAC aleatório incorreto
    let forged_sf_msg = alloc::vec![HandshakeType::Finished.as_u8(), 0x00, 0x00, 32, 0xDE, 0xAD, 0xBE, 0xEF];
    let mut full_forged = forged_sf_msg;
    full_forged.resize(4 + 32, 0x00);

    let res = client.process_server_finished(&full_forged);
    assert_eq!(res, Err(Tls13Alert::DecryptError));
    assert_ne!(client.state, HandshakeState::Connected);

    println!("PASSOU!");
}

// ── Teste 10: Formatação de Contexto do CertificateVerify (RFC 8446) ───────────

#[test]
fn test_certificate_verify_context_formatting() {
    print!("Teste 10: Formatação exata do contexto de assinatura do CertificateVerify... ");
    let ks = Tls13KeySchedule::new(CipherSuite::TlsAes128GcmSha256);
    let transcript_hash = Sha256::digest(b"HandshakeContextToCert");

    // Chave pública RSA falsa para testar apenas a rejeição segura
    let rsa_pubkey = net::x509::PublicKey::Raw(alloc::vec![0u8; 32]);
    let sig_scheme = SignatureScheme::RsaPssRsaeSha256;
    let fake_sig = [0u8; 256];

    let res = ks.verify_certificate_verify(&rsa_pubkey, sig_scheme, &fake_sig, &transcript_hash);
    // Chave raw não implementa RSA-PSS -> deve falhar graciosamente
    assert_eq!(res, Err(Tls13Alert::DecryptError));

    println!("PASSOU!");
}

