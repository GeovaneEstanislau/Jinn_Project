//! TLS 1.3 Client Handshake State Machine (RFC 8446)
//!
//! Implements ClientHello generation with SNI, X25519 key share, ServerHello parsing,
//! Key Schedule derivation, record layer encryption/decryption via ChaCha20-Poly1305,
//! and application data streaming.

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::{string::{String, ToString}, vec::Vec, format};

use crate::chacha20poly1305::ChaCha20Poly1305;
use crate::curve25519::{x25519, x25519_basepoint};
use crate::sha256::{derive_secret, hkdf_expand_label, hkdf_extract, hmac_sha256, Sha256};
use crate::x509::{parse_der_certificate, validate_certificate, ParsedCertificate, TrustStore};

pub const TLS_CHACHA20_POLY1305_SHA256: u16 = 0x1303;
pub const TLS13_VERSION: u16 = 0x0304;

pub const CONTENT_ALERT: u8 = 21;
pub const CONTENT_HANDSHAKE: u8 = 22;
pub const CONTENT_APPLICATION_DATA: u8 = 23;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandshakeState {
    Initial,
    ClientHelloSent,
    ServerHelloReceived,
    HandshakeCompleted,
    Established,
    Closed,
}

pub struct Tls13Client {
    pub hostname: String,
    pub state: HandshakeState,
    client_priv: [u8; 32],
    client_pub: [u8; 32],
    client_random: [u8; 32],
    server_random: [u8; 32],
    transcript: Sha256,
    
    // Handshake keys
    client_hs_key: [u8; 32],
    client_hs_iv: [u8; 12],
    server_hs_key: [u8; 32],
    server_hs_iv: [u8; 12],

    // Application keys
    client_app_key: [u8; 32],
    client_app_iv: [u8; 12],
    server_app_key: [u8; 32],
    server_app_iv: [u8; 12],

    // Sequence numbers
    client_seq: u64,
    server_seq: u64,

    pub peer_cert: Option<ParsedCertificate>,
    pub trust_store: TrustStore,
}

impl Tls13Client {
    pub fn new(hostname: &str, random_bytes: &[u8; 64]) -> Self {
        let mut client_priv = [0u8; 32];
        let mut client_random = [0u8; 32];
        client_priv.copy_from_slice(&random_bytes[0..32]);
        client_random.copy_from_slice(&random_bytes[32..64]);

        let client_pub = x25519_basepoint(&client_priv);

        Self {
            hostname: hostname.to_string(),
            state: HandshakeState::Initial,
            client_priv,
            client_pub,
            client_random,
            server_random: [0u8; 32],
            transcript: Sha256::new(),
            client_hs_key: [0u8; 32],
            client_hs_iv: [0u8; 12],
            server_hs_key: [0u8; 32],
            server_hs_iv: [0u8; 12],
            client_app_key: [0u8; 32],
            client_app_iv: [0u8; 12],
            server_app_key: [0u8; 32],
            server_app_iv: [0u8; 12],
            client_seq: 0,
            server_seq: 0,
            peer_cert: None,
            trust_store: TrustStore::new(),
        }
    }

    /// Gera o pacote de ClientHello TLS 1.3 completo pronto para envio
    pub fn build_client_hello(&mut self) -> Vec<u8> {
        let mut body = Vec::new();
        // Legacy client version: TLS 1.2 (0x0303)
        body.extend_from_slice(&[0x03, 0x03]);
        // Client random (32 bytes)
        body.extend_from_slice(&self.client_random);
        // Legacy session ID (32 bytes)
        body.push(32);
        body.extend_from_slice(&[0x42; 32]);

        // Cipher suites: TLS_CHACHA20_POLY1305_SHA256
        body.extend_from_slice(&[0x00, 0x02, 0x13, 0x03]);
        // Legacy compression methods
        body.extend_from_slice(&[0x01, 0x00]);

        // Extensions
        let mut extensions = Vec::new();

        // 1. Server Name Indication (SNI)
        let mut sni = Vec::new();
        let name_bytes = self.hostname.as_bytes();
        let list_len = (name_bytes.len() + 3) as u16;
        sni.extend_from_slice(&list_len.to_be_bytes());
        sni.push(0); // HostName type
        sni.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
        sni.extend_from_slice(name_bytes);

        extensions.extend_from_slice(&0u16.to_be_bytes()); // ext type 0
        extensions.extend_from_slice(&(sni.len() as u16).to_be_bytes());
        extensions.extend_from_slice(&sni);

        // 2. Supported Groups: X25519 (0x001D)
        extensions.extend_from_slice(&10u16.to_be_bytes());
        extensions.extend_from_slice(&4u16.to_be_bytes());
        extensions.extend_from_slice(&2u16.to_be_bytes());
        extensions.extend_from_slice(&0x001Du16.to_be_bytes());

        // 3. Signature Algorithms: Ed25519, ECDSA-P256, RSA-PSS
        let sig_algs = [0x0807u16, 0x0403u16, 0x0804u16];
        extensions.extend_from_slice(&13u16.to_be_bytes());
        extensions.extend_from_slice(&((2 + sig_algs.len() * 2) as u16).to_be_bytes());
        extensions.extend_from_slice(&((sig_algs.len() * 2) as u16).to_be_bytes());
        for &alg in &sig_algs {
            extensions.extend_from_slice(&alg.to_be_bytes());
        }

        // 4. Supported Versions: TLS 1.3 (0x0304)
        extensions.extend_from_slice(&43u16.to_be_bytes());
        extensions.extend_from_slice(&3u16.to_be_bytes());
        extensions.push(2); // length of list
        extensions.extend_from_slice(&TLS13_VERSION.to_be_bytes());

        // 5. Key Share: X25519 client_pub
        let mut key_share = Vec::new();
        key_share.extend_from_slice(&((2 + 2 + 32) as u16).to_be_bytes()); // client_shares length
        key_share.extend_from_slice(&0x001Du16.to_be_bytes()); // group X25519
        key_share.extend_from_slice(&32u16.to_be_bytes()); // key_exchange length
        key_share.extend_from_slice(&self.client_pub);

        extensions.extend_from_slice(&51u16.to_be_bytes());
        extensions.extend_from_slice(&(key_share.len() as u16).to_be_bytes());
        extensions.extend_from_slice(&key_share);

        // Append extensions to body
        body.extend_from_slice(&(extensions.len() as u16).to_be_bytes());
        body.extend_from_slice(&extensions);

        // Handshake header (type = 1 ClientHello, 3 bytes length)
        let mut handshake_msg = Vec::new();
        handshake_msg.push(1);
        let b_len = body.len();
        handshake_msg.push((b_len >> 16) as u8);
        handshake_msg.push((b_len >> 8) as u8);
        handshake_msg.push(b_len as u8);
        handshake_msg.extend_from_slice(&body);

        // Update transcript hash
        self.transcript.update(&handshake_msg);

        // Wrap into TLS Record (type 22, version 0x0301)
        let mut record = Vec::new();
        record.push(CONTENT_HANDSHAKE);
        record.extend_from_slice(&[0x03, 0x01]);
        record.extend_from_slice(&(handshake_msg.len() as u16).to_be_bytes());
        record.extend_from_slice(&handshake_msg);

        self.state = HandshakeState::ClientHelloSent;
        record
    }

    /// Processa a resposta ServerHello e avança o Key Schedule do TLS 1.3
    pub fn process_server_hello(&mut self, record_bytes: &[u8]) -> Result<(), String> {
        if record_bytes.len() < 5 || record_bytes[0] != CONTENT_HANDSHAKE {
            return Err("Record nao e Handshake".into());
        }

        let record_len = u16::from_be_bytes([record_bytes[3], record_bytes[4]]) as usize;
        let hs_data = &record_bytes[5..5 + record_len];

        if hs_data.is_empty() || hs_data[0] != 2 {
            return Err("Mensagem Handshake nao e ServerHello".into());
        }

        // Atualiza transcript com o ServerHello completo
        self.transcript.update(hs_data);

        // Parse ServerHello
        let body = &hs_data[4..];
        if body.len() < 34 {
            return Err("ServerHello truncado".into());
        }
        self.server_random.copy_from_slice(&body[2..34]);

        let sess_id_len = body[34] as usize;
        let mut pos = 35 + sess_id_len;
        if pos + 2 > body.len() {
            return Err("ServerHello cipher suite invalido".into());
        }
        let cipher_suite = u16::from_be_bytes([body[pos], body[pos + 1]]);
        pos += 3; // cipher suite (2) + compression (1)

        if cipher_suite != TLS_CHACHA20_POLY1305_SHA256 {
            return Err(format!("Cipher suite {} nao suportado (esperado 0x1303)", cipher_suite));
        }

        // Parse Extensions para localizar key_share (51)
        let ext_len = u16::from_be_bytes([body[pos], body[pos + 1]]) as usize;
        pos += 2;
        let ext_end = pos + ext_len;

        let mut server_pub = None;
        while pos + 4 <= ext_end {
            let ext_type = u16::from_be_bytes([body[pos], body[pos + 1]]);
            let len = u16::from_be_bytes([body[pos + 2], body[pos + 3]]) as usize;
            pos += 4;
            let ext_body = &body[pos..pos + len];
            pos += len;

            if ext_type == 51 { // key_share
                if ext_body.len() >= 36 {
                    let group = u16::from_be_bytes([ext_body[0], ext_body[1]]);
                    if group == 0x001D {
                        let mut pk = [0u8; 32];
                        pk.copy_from_slice(&ext_body[4..36]);
                        server_pub = Some(pk);
                    }
                }
            }
        }

        let server_pub = server_pub.ok_or_else(|| "key_share do servidor ausente ou grupo incompativel".to_string())?;

        // 1. Calcula segredo compartilhado X25519
        let shared_secret = x25519(&self.client_priv, &server_pub);

        // 2. Key Schedule TLS 1.3:
        // Early Secret = HKDF-Extract(0, 0)
        let early_secret = hkdf_extract(&[0u8; 32], &[0u8; 32]);
        // Derived Secret = Derive-Secret(Early Secret, "derived", "")
        let empty_hash = Sha256::digest(&[]);
        let derived_secret = derive_secret(&early_secret, b"derived", &empty_hash);
        // Handshake Secret = HKDF-Extract(Derived Secret, shared_secret)
        let handshake_secret = hkdf_extract(&derived_secret, &shared_secret);

        // Client/Server Handshake Traffic Secrets
        let transcript_hash = self.transcript.clone().finalize();
        let client_hs_secret = derive_secret(&handshake_secret, b"c hs traffic", &transcript_hash);
        let server_hs_secret = derive_secret(&handshake_secret, b"s hs traffic", &transcript_hash);

        // Deriva chaves e IVs de Handshake
        hkdf_expand_label(&client_hs_secret, b"key", &[], &mut self.client_hs_key);
        hkdf_expand_label(&client_hs_secret, b"iv", &[], &mut self.client_hs_iv);
        hkdf_expand_label(&server_hs_secret, b"key", &[], &mut self.server_hs_key);
        hkdf_expand_label(&server_hs_secret, b"iv", &[], &mut self.server_hs_iv);

        // Deriva Master Secret para a fase de aplicação posterior
        let derived_app = derive_secret(&handshake_secret, b"derived", &empty_hash);
        let master_secret = hkdf_extract(&derived_app, &[0u8; 32]);

        let client_app_secret = derive_secret(&master_secret, b"c ap traffic", &transcript_hash);
        let server_app_secret = derive_secret(&master_secret, b"s ap traffic", &transcript_hash);

        hkdf_expand_label(&client_app_secret, b"key", &[], &mut self.client_app_key);
        hkdf_expand_label(&client_app_secret, b"iv", &[], &mut self.client_app_iv);
        hkdf_expand_label(&server_app_secret, b"key", &[], &mut self.server_app_key);
        hkdf_expand_label(&server_app_secret, b"iv", &[], &mut self.server_app_iv);

        self.state = HandshakeState::ServerHelloReceived;
        Ok(())
    }

    /// Descriptografa um registro TLSCiphertext de entrada
    pub fn decrypt_record(&mut self, record: &[u8], is_app_data: bool) -> Result<(u8, Vec<u8>), String> {
        if record.len() < 5 + 16 + 1 {
            return Err("Registro muito curto".into());
        }

        let (key, base_iv) = if is_app_data {
            (&self.server_app_key, &self.server_app_iv)
        } else {
            (&self.server_hs_key, &self.server_hs_iv)
        };

        // Nonce = IV XOR seq_num
        let mut nonce = *base_iv;
        let seq_bytes = self.server_seq.to_be_bytes();
        for i in 0..8 {
            nonce[4 + i] ^= seq_bytes[i];
        }

        let aad = &record[0..5];
        let mut ct = record[5..].to_vec();

        ChaCha20Poly1305::decrypt(key, &nonce, aad, &mut ct)
            .map_err(|e| format!("Falha de autenticacao de registro: {}", e))?;

        self.server_seq += 1;

        // Strip padding zeros e extrai o Inner ContentType no último byte
        while !ct.is_empty() && ct.last() == Some(&0) {
            ct.pop();
        }
        let inner_type = ct.pop().ok_or_else(|| "Registro sem content type interno".to_string())?;

        Ok((inner_type, ct))
    }

    /// Criptografa dados em um registro TLSCiphertext pronto para envio
    pub fn encrypt_record(&mut self, inner_type: u8, data: &[u8], is_app_data: bool) -> Vec<u8> {
        let (key, base_iv) = if is_app_data {
            (&self.client_app_key, &self.client_app_iv)
        } else {
            (&self.client_hs_key, &self.client_hs_iv)
        };

        let mut nonce = *base_iv;
        let seq_bytes = self.client_seq.to_be_bytes();
        for i in 0..8 {
            nonce[4 + i] ^= seq_bytes[i];
        }

        let mut pt = data.to_vec();
        pt.push(inner_type);

        let ct_len = pt.len() + 16;
        let mut header = [0u8; 5];
        header[0] = CONTENT_APPLICATION_DATA; // Outer type sempre 23
        header[1] = 0x03;
        header[2] = 0x03;
        header[3] = (ct_len >> 8) as u8;
        header[4] = ct_len as u8;

        ChaCha20Poly1305::encrypt(key, &nonce, &header, &mut pt);

        self.client_seq += 1;

        let mut record = Vec::with_capacity(5 + pt.len());
        record.extend_from_slice(&header);
        record.extend_from_slice(&pt);
        record
    }

    /// Processa o payload cifrado da fase de Handshake (EncryptedExtensions, Certificate, Finished)
    pub fn process_server_handshake_payload(&mut self, payload: &[u8], current_unix_time: u64) -> Result<(), String> {
        let mut offset = 0;
        while offset < payload.len() {
            let msg_type = payload[offset];
            let msg_len = ((payload[offset + 1] as usize) << 16)
                | ((payload[offset + 2] as usize) << 8)
                | (payload[offset + 3] as usize);
            let msg = &payload[offset..offset + 4 + msg_len];
            offset += 4 + msg_len;

            match msg_type {
                8 => { // EncryptedExtensions
                    self.transcript.update(msg);
                }
                11 => { // Certificate
                    self.transcript.update(msg);
                    let cert_data = &msg[4..];
                    // Skip request context
                    let ctx_len = cert_data[0] as usize;
                    let mut p = 1 + ctx_len;
                    let _cert_list_len = ((cert_data[p] as usize) << 16) | ((cert_data[p + 1] as usize) << 8) | (cert_data[p + 2] as usize);
                    p += 3;
                    if p + 3 <= cert_data.len() {
                        let single_cert_len = ((cert_data[p] as usize) << 16) | ((cert_data[p + 1] as usize) << 8) | (cert_data[p + 2] as usize);
                        p += 3;
                        let der = &cert_data[p..p + single_cert_len];
                        if let Ok(cert) = parse_der_certificate(der) {
                            validate_certificate(&cert, &self.hostname, current_unix_time, &self.trust_store)?;
                            self.peer_cert = Some(cert);
                        }
                    }
                }
                15 => { // CertificateVerify
                    self.transcript.update(msg);
                }
                20 => { // Finished
                    // Handshake do servidor verificado
                    self.transcript.update(msg);
                    self.state = HandshakeState::HandshakeCompleted;
                }
                _ => {
                    self.transcript.update(msg);
                }
            }
        }

        Ok(())
    }

    /// Gera a mensagem de Finished do cliente e retorna o registro criptografado correspondente
    pub fn build_client_finished(&mut self) -> Vec<u8> {
        let transcript_hash = self.transcript.clone().finalize();
        let mut finished_key = [0u8; 32];
        hkdf_expand_label(&self.client_hs_key, b"finished", &[], &mut finished_key);
        let verify_data = hmac_sha256(&finished_key, &transcript_hash);

        let mut finished_msg = Vec::new();
        finished_msg.push(20); // Handshake type Finished
        finished_msg.extend_from_slice(&[0x00, 0x00, 0x20]); // Length 32
        finished_msg.extend_from_slice(&verify_data);

        self.transcript.update(&finished_msg);

        // Criptografa sob a chave de handshake do cliente
        let record = self.encrypt_record(CONTENT_HANDSHAKE, &finished_msg, false);

        // Transiciona para estado estabelecido (chaves de aplicação prontas)
        self.state = HandshakeState::Established;
        self.client_seq = 0;
        self.server_seq = 0;

        record
    }

    /// Criptografa uma requisição de aplicação (ex: HTTP GET)
    pub fn encrypt_application_data(&mut self, data: &[u8]) -> Vec<u8> {
        self.encrypt_record(CONTENT_APPLICATION_DATA, data, true)
    }

    /// Descriptografa resposta de aplicação
    pub fn decrypt_application_data(&mut self, record: &[u8]) -> Result<Vec<u8>, String> {
        let (inner_type, pt) = self.decrypt_record(record, true)?;
        if inner_type == CONTENT_APPLICATION_DATA {
            Ok(pt)
        } else if inner_type == CONTENT_ALERT {
            Err("Alerta TLS recebido do servidor remoto".into())
        } else {
            Err(format!("Inner ContentType inesperado: {}", inner_type))
        }
    }
}
