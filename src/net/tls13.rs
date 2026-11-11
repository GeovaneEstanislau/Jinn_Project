//! Jinn Microkernel — TLS 1.3 Key Schedule, Transcript Hash & Handshake (RFC 8446)
//!
//! Implementação completa em Rust `#![no_std]` do Key Schedule e da Máquina de Estados
//! do protocolo TLS 1.3 conforme a RFC 8446 e derivações HKDF (RFC 5869).
//! Integra verificação de cadeias X.509 (Fase 2) e assinaturas digitais RSA-PSS e ECDSA P-256.

use alloc::string::String;
use alloc::vec::Vec;
use crate::crypto::sha256::{Sha256, hmac_sha256, hkdf_extract};
use crate::net::pki::TrustStore;
use crate::net::x509::{ParsedCertificate, PublicKey, SignatureAlgorithm, parse_der_certificate};

// ── Constantes do Protocolo TLS 1.3 (RFC 8446) ────────────────────────────────

pub const TLS13_VERSION: u16 = 0x0304;       // TLS 1.3
pub const TLS12_LEGACY_VERSION: u16 = 0x0303;// TLS 1.2 legacy wire version

/// SHA-256("HelloRetryRequest") - RFC 8446 Seção 4.1.3
pub const HELLO_RETRY_REQUEST_RANDOM: [u8; 32] = [
    0xCF, 0x21, 0xAD, 0x74, 0xE5, 0x9A, 0x61, 0x11,
    0xBE, 0x1D, 0x8C, 0x02, 0x1E, 0x65, 0xB8, 0x91,
    0xC2, 0xA2, 0x11, 0x16, 0x7A, 0xBB, 0x8C, 0x5E,
    0x07, 0x9E, 0x09, 0xE2, 0xC8, 0x88, 0x3E, 0x9E,
];

pub fn is_hello_retry_request(msg: &[u8]) -> bool {
    if msg.len() < 38 {
        return false;
    }
    // msg[0] == HandshakeType::ServerHello (2)
    if msg[0] != 2 {
        return false;
    }
    // O campo random está nos offsets 6..38 (4 bytes cabeçalho handshake + 2 bytes legacy_version 0x0303)
    &msg[6..38] == &HELLO_RETRY_REQUEST_RANDOM[..]
}

// Ciphersuites (RFC 8446 Seção B.4)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CipherSuite {
    TlsAes128GcmSha256,
    TlsAes256GcmSha384,
    TlsChacha20Poly1305Sha256,
    Unknown(u16),
}

impl CipherSuite {
    pub const fn as_u16(&self) -> u16 {
        match self {
            Self::TlsAes128GcmSha256 => 0x1301,
            Self::TlsAes256GcmSha384 => 0x1302,
            Self::TlsChacha20Poly1305Sha256 => 0x1303,
            Self::Unknown(val) => *val,
        }
    }

    pub fn from_u16(val: u16) -> Self {
        match val {
            0x1301 => Self::TlsAes128GcmSha256,
            0x1302 => Self::TlsAes256GcmSha384,
            0x1303 => Self::TlsChacha20Poly1305Sha256,
            other => Self::Unknown(other),
        }
    }

    pub fn key_len(&self) -> usize {
        match self {
            Self::TlsAes128GcmSha256 => 16,
            Self::TlsAes256GcmSha384 => 32,
            Self::TlsChacha20Poly1305Sha256 => 32,
            Self::Unknown(_) => 32,
        }
    }

    pub fn iv_len(&self) -> usize {
        12 // Todos os AEADs padronizados no TLS 1.3 usam IV de 96 bits (12 bytes)
    }

    pub fn hash_len(&self) -> usize {
        match self {
            Self::TlsAes256GcmSha384 => 48,
            _ => 32, // SHA-256 padrão
        }
    }
}

// Algoritmos de Assinatura (SignatureScheme - RFC 8446 Seção 4.2.3)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureScheme {
    RsaPkcs1Sha256,
    RsaPssRsaeSha256,
    EcdsaSecp256r1Sha256,
    Ed25519,
    Unknown(u16),
}

impl SignatureScheme {
    pub const fn as_u16(&self) -> u16 {
        match self {
            Self::RsaPkcs1Sha256 => 0x0401,
            Self::RsaPssRsaeSha256 => 0x0804,
            Self::EcdsaSecp256r1Sha256 => 0x0403,
            Self::Ed25519 => 0x0807,
            Self::Unknown(val) => *val,
        }
    }

    pub fn from_u16(val: u16) -> Self {
        match val {
            0x0401 => Self::RsaPkcs1Sha256,
            0x0804 => Self::RsaPssRsaeSha256,
            0x0403 => Self::EcdsaSecp256r1Sha256,
            0x0807 => Self::Ed25519,
            other => Self::Unknown(other),
        }
    }
}

// Named Groups para ECDHE (RFC 8446 Seção 4.2.7)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedGroup {
    Secp256r1,
    X25519,
    Unknown(u16),
}

impl NamedGroup {
    pub const fn as_u16(&self) -> u16 {
        match self {
            Self::Secp256r1 => 0x0017,
            Self::X25519 => 0x001D,
            Self::Unknown(val) => *val,
        }
    }

    pub fn from_u16(val: u16) -> Self {
        match val {
            0x0017 => Self::Secp256r1,
            0x001D => Self::X25519,
            other => Self::Unknown(other),
        }
    }
}

// Tipos de Mensagem do Handshake (RFC 8446 Seção 4)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandshakeType {
    ClientHello,
    ServerHello,
    NewSessionTicket,
    EndOfEarlyData,
    EncryptedExtensions,
    Certificate,
    CertificateRequest,
    CertificateVerify,
    Finished,
    KeyUpdate,
    MessageHash,
    Unknown(u8),
}

impl HandshakeType {
    pub const fn as_u8(&self) -> u8 {
        match self {
            Self::ClientHello => 1,
            Self::ServerHello => 2,
            Self::NewSessionTicket => 4,
            Self::EndOfEarlyData => 5,
            Self::EncryptedExtensions => 8,
            Self::Certificate => 11,
            Self::CertificateRequest => 13,
            Self::CertificateVerify => 15,
            Self::Finished => 20,
            Self::KeyUpdate => 24,
            Self::MessageHash => 254,
            Self::Unknown(val) => *val,
        }
    }

    pub fn from_u8(val: u8) -> Self {
        match val {
            1 => Self::ClientHello,
            2 => Self::ServerHello,
            4 => Self::NewSessionTicket,
            5 => Self::EndOfEarlyData,
            8 => Self::EncryptedExtensions,
            11 => Self::Certificate,
            13 => Self::CertificateRequest,
            15 => Self::CertificateVerify,
            20 => Self::Finished,
            24 => Self::KeyUpdate,
            254 => Self::MessageHash,
            other => Self::Unknown(other),
        }
    }
}

// Alertas TLS (RFC 8446 Seção 6)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Tls13Alert {
    CloseNotify = 0,
    UnexpectedMessage = 10,
    BadRecordMac = 20,
    HandshakeFailure = 40,
    BadCertificate = 42,
    UnsupportedCertificate = 43,
    CertificateRevoked = 44,
    CertificateExpired = 45,
    CertificateUnknown = 46,
    IllegalParameter = 47,
    UnknownCa = 48,
    AccessDenied = 49,
    DecodeError = 50,
    DecryptError = 51,
    ProtocolVersion = 70,
    InternalError = 80,
    MissingExtension = 109,
    UnsupportedExtension = 110,
}

// Estados do Handshake
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandshakeState {
    Initial,
    ClientHelloSent,
    HelloRetryRequestReceived,
    WaitServerHello,
    WaitEncryptedExtensions,
    WaitCertificate,
    WaitCertificateVerify,
    WaitFinished,
    Connected,
    Closed,
    Error(Tls13Alert),
}

// ── Funções Criptográficas de Derivação HKDF (RFC 5869 / RFC 8446) ─────────────

/// Expansão HKDF-Expand (RFC 5869 Seção 2.3)
///
/// OKM = T(1) || T(2) || ... || T(N)
/// T(0) = ""
/// T(i) = HMAC-Hash(PRK, T(i-1) || info || i)
pub fn hkdf_expand(prk: &[u8; 32], info: &[u8], okm_len: usize) -> Vec<u8> {
    let mut okm = Vec::with_capacity(okm_len);
    let mut t_prev = [0u8; 32];
    let mut t_prev_len = 0usize;
    let n = (okm_len + 31) / 32;

    for i in 1..=n {
        let mut step_buf = Vec::with_capacity(t_prev_len + info.len() + 1);
        if t_prev_len > 0 {
            step_buf.extend_from_slice(&t_prev[..t_prev_len]);
        }
        step_buf.extend_from_slice(info);
        step_buf.push(i as u8);

        let t_i = hmac_sha256(prk, &step_buf);
        t_prev = t_i;
        t_prev_len = 32;

        let to_copy = (okm_len - okm.len()).min(32);
        okm.extend_from_slice(&t_i[..to_copy]);
    }

    okm
}

/// Derivação de rótulo tipado HKDF-Expand-Label (RFC 8446 Seção 7.1)
///
/// ```text
/// struct {
///     uint16 length = Length;
///     opaque label<7..255> = "tls13 " + Label;
///     opaque context<0..255> = Context;
/// } HkdfLabel;
/// ```
pub fn hkdf_expand_label(
    secret: &[u8; 32],
    label: &[u8],
    context: &[u8],
    length: u16,
) -> Vec<u8> {
    let mut hkdf_label = Vec::with_capacity(2 + 1 + 6 + label.len() + 1 + context.len());

    // length: uint16
    hkdf_label.extend_from_slice(&length.to_be_bytes());

    // label: opaque label<7..255> com prefixo "tls13 "
    let full_label_len = 6 + label.len();
    hkdf_label.push(full_label_len as u8);
    hkdf_label.extend_from_slice(b"tls13 ");
    hkdf_label.extend_from_slice(label);

    // context: opaque context<0..255>
    hkdf_label.push(context.len() as u8);
    hkdf_label.extend_from_slice(context);

    hkdf_expand(secret, &hkdf_label, length as usize)
}

/// Derive-Secret(Secret, Label, Transcript-Hash) (RFC 8446 Seção 7.1)
///
/// Derive-Secret(Secret, Label, Messages) =
///     HKDF-Expand-Label(Secret, Label, Transcript-Hash(Messages), Hash.length)
pub fn derive_secret(
    secret: &[u8; 32],
    label: &[u8],
    transcript_hash: &[u8; 32],
) -> [u8; 32] {
    let out = hkdf_expand_label(secret, label, transcript_hash, 32);
    let mut res = [0u8; 32];
    res.copy_from_slice(&out[..32]);
    res
}

// ── Transcript Hash (Histórico Contínuo SHA-256) ───────────────────────────────

/// Acumulador contínuo das mensagens do Handshake para integridade e autenticação
#[derive(Clone, Debug)]
pub struct TranscriptHash {
    hasher: Sha256,
}

impl TranscriptHash {
    pub fn new() -> Self {
        Self {
            hasher: Sha256::new(),
        }
    }

    /// Atualiza o transcript com os bytes brutos da mensagem de handshake (sem cabeçalho de Record)
    pub fn update(&mut self, message_bytes: &[u8]) {
        self.hasher.update(message_bytes);
    }

    /// Obtém o hash corrente SHA-256 (32 bytes) sem consumir ou resetar o acumulador interno
    pub fn current_hash(&self) -> [u8; 32] {
        let cloned_hasher = self.hasher;
        cloned_hasher.finalize()
    }

    /// Reseta o acumulador para o estado inicial
    pub fn reset(&mut self) {
        self.hasher = Sha256::new();
    }
}

// ── Árvore de Chaves TLS 1.3 Key Schedule (RFC 8446 Seção 7.1) ─────────────────

/// Máquina de derivação de segredos e chaves de tráfego do TLS 1.3
#[derive(Debug, Clone)]
pub struct Tls13KeySchedule {
    pub cipher_suite: CipherSuite,
    pub early_secret: [u8; 32],
    pub handshake_secret: Option<[u8; 32]>,
    pub master_secret: Option<[u8; 32]>,

    // Segredos de Tráfego do Handshake
    pub client_handshake_traffic_secret: Option<[u8; 32]>,
    pub server_handshake_traffic_secret: Option<[u8; 32]>,

    // Chaves e IVs do Handshake
    pub client_handshake_key: Option<Vec<u8>>,
    pub client_handshake_iv: Option<[u8; 12]>,
    pub server_handshake_key: Option<Vec<u8>>,
    pub server_handshake_iv: Option<[u8; 12]>,

    // Segredos de Tráfego da Aplicação
    pub client_app_traffic_secret: Option<[u8; 32]>,
    pub server_app_traffic_secret: Option<[u8; 32]>,

    // Chaves e IVs da Aplicação
    pub client_app_key: Option<Vec<u8>>,
    pub client_app_iv: Option<[u8; 12]>,
    pub server_app_key: Option<Vec<u8>>,
    pub server_app_iv: Option<[u8; 12]>,
}

impl Tls13KeySchedule {
    /// Inicializa o Key Schedule calculando o `Early Secret` (RFC 8446 Seção 7.1)
    pub fn new(cipher_suite: CipherSuite) -> Self {
        // Early Secret = HKDF-Extract(salt=0, IKM=0)
        let zero_salt = [0u8; 32];
        let zero_ikm = [0u8; 32];
        let early_secret = hkdf_extract(&zero_salt, &zero_ikm);

        Self {
            cipher_suite,
            early_secret,
            handshake_secret: None,
            master_secret: None,
            client_handshake_traffic_secret: None,
            server_handshake_traffic_secret: None,
            client_handshake_key: None,
            client_handshake_iv: None,
            server_handshake_key: None,
            server_handshake_iv: None,
            client_app_traffic_secret: None,
            server_app_traffic_secret: None,
            client_app_key: None,
            client_app_iv: None,
            server_app_key: None,
            server_app_iv: None,
        }
    }

    /// Transição para Handshake Secret a partir do segredo compartilhado ECDHE
    pub fn derive_handshake_secrets(
        &mut self,
        shared_secret_ecdhe: &[u8],
        transcript_hash_hello: &[u8; 32],
    ) {
        let empty_hash = Sha256::digest(b"");

        // derived = Derive-Secret(Early Secret, "derived", "")
        let derived_early = derive_secret(&self.early_secret, b"derived", &empty_hash);

        // Handshake Secret = HKDF-Extract(salt=derived, IKM=shared_secret_ecdhe)
        let hs_secret = hkdf_extract(&derived_early, shared_secret_ecdhe);
        self.handshake_secret = Some(hs_secret);

        // client_handshake_traffic_secret = Derive-Secret(Handshake Secret, "c hs traffic", ClientHello..ServerHello)
        let c_hs_traffic = derive_secret(&hs_secret, b"c hs traffic", transcript_hash_hello);
        // server_handshake_traffic_secret = Derive-Secret(Handshake Secret, "s hs traffic", ClientHello..ServerHello)
        let s_hs_traffic = derive_secret(&hs_secret, b"s hs traffic", transcript_hash_hello);

        self.client_handshake_traffic_secret = Some(c_hs_traffic);
        self.server_handshake_traffic_secret = Some(s_hs_traffic);

        // Derivação das Chaves AEAD e IVs de handshake
        let k_len = self.cipher_suite.key_len() as u16;
        let c_key = hkdf_expand_label(&c_hs_traffic, b"key", b"", k_len);
        let c_iv_vec = hkdf_expand_label(&c_hs_traffic, b"iv", b"", 12);
        let mut c_iv = [0u8; 12];
        c_iv.copy_from_slice(&c_iv_vec);

        let s_key = hkdf_expand_label(&s_hs_traffic, b"key", b"", k_len);
        let s_iv_vec = hkdf_expand_label(&s_hs_traffic, b"iv", b"", 12);
        let mut s_iv = [0u8; 12];
        s_iv.copy_from_slice(&s_iv_vec);

        self.client_handshake_key = Some(c_key);
        self.client_handshake_iv = Some(c_iv);
        self.server_handshake_key = Some(s_key);
        self.server_handshake_iv = Some(s_iv);
    }

    /// Transição para Master Secret e chaves de tráfego de aplicação após ServerFinished
    pub fn derive_master_secrets(&mut self, transcript_hash_server_finished: &[u8; 32]) -> Result<(), Tls13Alert> {
        let hs_secret = self.handshake_secret.ok_or(Tls13Alert::InternalError)?;
        let empty_hash = Sha256::digest(b"");

        // derived = Derive-Secret(Handshake Secret, "derived", "")
        let derived_hs = derive_secret(&hs_secret, b"derived", &empty_hash);

        // Master Secret = HKDF-Extract(salt=derived, IKM=0)
        let zero_ikm = [0u8; 32];
        let m_secret = hkdf_extract(&derived_hs, &zero_ikm);
        self.master_secret = Some(m_secret);

        // client_application_traffic_secret_0 = Derive-Secret(Master Secret, "c ap traffic", ClientHello..server Finished)
        let c_ap_traffic = derive_secret(&m_secret, b"c ap traffic", transcript_hash_server_finished);
        // server_application_traffic_secret_0 = Derive-Secret(Master Secret, "s ap traffic", ClientHello..server Finished)
        let s_ap_traffic = derive_secret(&m_secret, b"s ap traffic", transcript_hash_server_finished);

        self.client_app_traffic_secret = Some(c_ap_traffic);
        self.server_app_traffic_secret = Some(s_ap_traffic);

        // Derivação das Chaves AEAD e IVs da aplicação
        let k_len = self.cipher_suite.key_len() as u16;
        let c_key = hkdf_expand_label(&c_ap_traffic, b"key", b"", k_len);
        let c_iv_vec = hkdf_expand_label(&c_ap_traffic, b"iv", b"", 12);
        let mut c_iv = [0u8; 12];
        c_iv.copy_from_slice(&c_iv_vec);

        let s_key = hkdf_expand_label(&s_ap_traffic, b"key", b"", k_len);
        let s_iv_vec = hkdf_expand_label(&s_ap_traffic, b"iv", b"", 12);
        let mut s_iv = [0u8; 12];
        s_iv.copy_from_slice(&s_iv_vec);

        self.client_app_key = Some(c_key);
        self.client_app_iv = Some(c_iv);
        self.server_app_key = Some(s_key);
        self.server_app_iv = Some(s_iv);

        Ok(())
    }

    /// Deriva o Resumption Master Secret (RFC 8446 Seção 7.1)
    /// resumption_master_secret = Derive-Secret(Master Secret, "res master", ClientHello..client Finished)
    pub fn derive_resumption_master_secret(&self, transcript_hash_client_finished: &[u8; 32]) -> Result<[u8; 32], Tls13Alert> {
        let m_secret = self.master_secret.ok_or(Tls13Alert::InternalError)?;
        Ok(derive_secret(&m_secret, b"res master", transcript_hash_client_finished))
    }

    /// Calcula o Nonce AEAD combinando o IV com o sequence number via operação XOR (RFC 8446 Seção 5.3)
    pub fn compute_nonce(iv: &[u8; 12], seq_num: u64) -> [u8; 12] {
        let mut nonce = *iv;
        let seq_bytes = seq_num.to_be_bytes();
        // O sequence number de 64 bits (8 bytes) é alinhado à direita com 4 bytes zero à esquerda
        for i in 0..8 {
            nonce[4 + i] ^= seq_bytes[i];
        }
        nonce
    }

    /// Calcula a `verify_data` do bloco Finished (RFC 8446 Seção 4.4.4)
    ///
    /// finished_key = HKDF-Expand-Label(BaseKey, "finished", "", Hash.length)
    /// verify_data = HMAC-Hash(finished_key, Transcript-Hash(Handshake Context))
    pub fn compute_finished_verify_data(
        traffic_secret: &[u8; 32],
        transcript_hash: &[u8; 32],
    ) -> [u8; 32] {
        let finished_key_vec = hkdf_expand_label(traffic_secret, b"finished", b"", 32);
        let mut finished_key = [0u8; 32];
        finished_key.copy_from_slice(&finished_key_vec);
        hmac_sha256(&finished_key, transcript_hash)
    }

    /// Valida o bloco `Finished` recebido do servidor
    pub fn verify_server_finished(
        &self,
        received_verify_data: &[u8],
        transcript_hash_before_finished: &[u8; 32],
    ) -> Result<(), Tls13Alert> {
        let s_hs_traffic = self.server_handshake_traffic_secret.ok_or(Tls13Alert::InternalError)?;
        let expected = Self::compute_finished_verify_data(&s_hs_traffic, transcript_hash_before_finished);

        if received_verify_data.len() != 32 {
            return Err(Tls13Alert::DecryptError);
        }

        // Comparação em tempo constante
        let mut diff = 0u8;
        for i in 0..32 {
            diff |= received_verify_data[i] ^ expected[i];
        }

        if diff == 0 {
            Ok(())
        } else {
            Err(Tls13Alert::DecryptError)
        }
    }

    /// Constrói e valida a mensagem `CertificateVerify` (RFC 8446 Seção 4.4.3)
    ///
    /// The input to the signature calculation is:
    /// - 64 octets of 0x20
    /// - Context string ("TLS 1.3, server CertificateVerify\0")
    /// - 0x00 separator
    /// - Transcript-Hash(Handshake Context)
    pub fn verify_certificate_verify(
        &self,
        server_pubkey: &PublicKey,
        sig_scheme: SignatureScheme,
        signature_bytes: &[u8],
        transcript_hash_before_verify: &[u8; 32],
    ) -> Result<(), Tls13Alert> {
        // Constrói o buffer estruturado da RFC 8446 Seção 4.4.3
        let context_str = b"TLS 1.3, server CertificateVerify";
        let mut verify_payload = Vec::with_capacity(64 + context_str.len() + 1 + 32);

        // 64 espaços (0x20)
        verify_payload.resize(64, 0x20);
        // Context string
        verify_payload.extend_from_slice(context_str);
        // Byte nulo separador
        verify_payload.push(0x00);
        // Transcript hash
        verify_payload.extend_from_slice(transcript_hash_before_verify);

        // Mapeia o esquema de assinatura do TLS 1.3 para a infraestrutura X.509
        let x509_sig_alg = match sig_scheme {
            SignatureScheme::RsaPssRsaeSha256 => SignatureAlgorithm::RsaPssSha256,
            SignatureScheme::RsaPkcs1Sha256 => SignatureAlgorithm::Sha256WithRsa,
            SignatureScheme::EcdsaSecp256r1Sha256 => SignatureAlgorithm::EcdsaWithSha256,
            _ => return Err(Tls13Alert::UnsupportedCertificate),
        };

        // Verificação criptográfica matemática da assinatura
        server_pubkey
            .verify_signature(x509_sig_alg, &verify_payload, signature_bytes)
            .map_err(|_| Tls13Alert::DecryptError)
    }
}

// ── Máquina de Estados do Handshake TLS 1.3 ───────────────────────────────────

/// Gerenciador de conexão cliente TLS 1.3 (RFC 8446)
pub struct Tls13Client {
    pub hostname: String,
    pub state: HandshakeState,
    pub key_schedule: Tls13KeySchedule,
    pub transcript: TranscriptHash,
    pub trust_store: TrustStore,
    pub peer_certificate: Option<ParsedCertificate>,
    pub client_hello1: Option<Vec<u8>>,
    pub client_seq: u64,
    pub server_seq: u64,
}

impl Tls13Client {
    /// Inicializa um novo cliente TLS 1.3 para o host especificado
    pub fn new(hostname: &str, trust_store: TrustStore) -> Self {
        Self {
            hostname: String::from(hostname),
            state: HandshakeState::Initial,
            key_schedule: Tls13KeySchedule::new(CipherSuite::TlsAes128GcmSha256),
            transcript: TranscriptHash::new(),
            trust_store,
            peer_certificate: None,
            client_hello1: None,
            client_seq: 0,
            server_seq: 0,
        }
    }

    /// Gera o pacote `ClientHello` do TLS 1.3 com as extensões obrigatórias:
    /// - `supported_versions` (0x002B -> TLS 1.3 0x0304)
    /// - `server_name` (SNI 0x0000)
    /// - `supported_groups` (0x000A -> secp256r1)
    /// - `key_share` (0x0033 -> secp256r1 pubkey)
    /// - `signature_algorithms` (0x000D -> RSA-PSS, ECDSA P-256)
    pub fn build_client_hello(
        &mut self,
        client_random: &[u8; 32],
        client_public_key_bytes: &[u8],
    ) -> Vec<u8> {
        let mut ch_body = Vec::new();

        // 1. legacy_version (TLS 1.2 = 0x0303)
        ch_body.extend_from_slice(&TLS12_LEGACY_VERSION.to_be_bytes());

        // 2. random (32 bytes)
        ch_body.extend_from_slice(client_random);

        // 3. legacy_session_id (echo compatibility - 32 bytes)
        ch_body.push(32);
        ch_body.extend_from_slice(client_random);

        // 4. cipher_suites
        let suites: [u16; 3] = [
            CipherSuite::TlsAes128GcmSha256.as_u16(),
            CipherSuite::TlsChacha20Poly1305Sha256.as_u16(),
            CipherSuite::TlsAes256GcmSha384.as_u16(),
        ];
        ch_body.extend_from_slice(&((suites.len() * 2) as u16).to_be_bytes());
        for suite in &suites {
            ch_body.extend_from_slice(&suite.to_be_bytes());
        }

        // 5. legacy_compression_methods (1 byte: 0x00 = null)
        ch_body.push(1);
        ch_body.push(0x00);

        // 6. Extensions
        let mut extensions_data = Vec::new();

        // Extension: server_name (SNI - 0x0000)
        let host_bytes = self.hostname.as_bytes();
        let sni_list_len = (host_bytes.len() + 3) as u16;
        let mut sni_ext = Vec::new();
        sni_ext.extend_from_slice(&sni_list_len.to_be_bytes());
        sni_ext.push(0x00); // NameType: host_name
        sni_ext.extend_from_slice(&(host_bytes.len() as u16).to_be_bytes());
        sni_ext.extend_from_slice(host_bytes);

        extensions_data.extend_from_slice(&0x0000u16.to_be_bytes());
        extensions_data.extend_from_slice(&(sni_ext.len() as u16).to_be_bytes());
        extensions_data.extend_from_slice(&sni_ext);

        // Extension: supported_versions (0x002B)
        let mut ver_ext = Vec::new();
        ver_ext.push(2); // 1 versão suportada (2 bytes)
        ver_ext.extend_from_slice(&TLS13_VERSION.to_be_bytes());

        extensions_data.extend_from_slice(&0x002Bu16.to_be_bytes());
        extensions_data.extend_from_slice(&(ver_ext.len() as u16).to_be_bytes());
        extensions_data.extend_from_slice(&ver_ext);

        // Extension: supported_groups (0x000A)
        let groups: [u16; 2] = [NamedGroup::Secp256r1.as_u16(), NamedGroup::X25519.as_u16()];
        let mut grp_ext = Vec::new();
        grp_ext.extend_from_slice(&((groups.len() * 2) as u16).to_be_bytes());
        for g in &groups {
            grp_ext.extend_from_slice(&g.to_be_bytes());
        }

        extensions_data.extend_from_slice(&0x000Au16.to_be_bytes());
        extensions_data.extend_from_slice(&(grp_ext.len() as u16).to_be_bytes());
        extensions_data.extend_from_slice(&grp_ext);

        // Extension: signature_algorithms (0x000D)
        let sig_algs: [u16; 3] = [
            SignatureScheme::RsaPssRsaeSha256.as_u16(),
            SignatureScheme::EcdsaSecp256r1Sha256.as_u16(),
            SignatureScheme::RsaPkcs1Sha256.as_u16(),
        ];
        let mut sig_ext = Vec::new();
        sig_ext.extend_from_slice(&((sig_algs.len() * 2) as u16).to_be_bytes());
        for s in &sig_algs {
            sig_ext.extend_from_slice(&s.to_be_bytes());
        }

        extensions_data.extend_from_slice(&0x000Du16.to_be_bytes());
        extensions_data.extend_from_slice(&(sig_ext.len() as u16).to_be_bytes());
        extensions_data.extend_from_slice(&sig_ext);

        // Extension: key_share (0x0033)
        let mut ks_ext = Vec::new();
        let client_shares_len = 2 + 2 + client_public_key_bytes.len();
        ks_ext.extend_from_slice(&(client_shares_len as u16).to_be_bytes());
        ks_ext.extend_from_slice(&(NamedGroup::Secp256r1.as_u16()).to_be_bytes());
        ks_ext.extend_from_slice(&(client_public_key_bytes.len() as u16).to_be_bytes());
        ks_ext.extend_from_slice(client_public_key_bytes);

        extensions_data.extend_from_slice(&0x0033u16.to_be_bytes());
        extensions_data.extend_from_slice(&(ks_ext.len() as u16).to_be_bytes());
        extensions_data.extend_from_slice(&ks_ext);

        // Concatena as extensões ao ClientHello
        ch_body.extend_from_slice(&(extensions_data.len() as u16).to_be_bytes());
        ch_body.extend_from_slice(&extensions_data);

        // Cria o frame Handshake com cabeçalho (tipo = 1, comprimento = 3 bytes)
        let mut handshake_msg = Vec::with_capacity(4 + ch_body.len());
        handshake_msg.push(HandshakeType::ClientHello.as_u8());
        let ch_len = ch_body.len();
        handshake_msg.push(((ch_len >> 16) & 0xFF) as u8);
        handshake_msg.push(((ch_len >> 8) & 0xFF) as u8);
        handshake_msg.push((ch_len & 0xFF) as u8);
        handshake_msg.extend_from_slice(&ch_body);

        // Atualiza o TranscriptHash com a mensagem completa do ClientHello
        self.transcript.update(&handshake_msg);
        self.client_hello1 = Some(handshake_msg.clone());
        self.state = HandshakeState::ClientHelloSent;

        handshake_msg
    }

    /// Processa mensagem HelloRetryRequest (RFC 8446 Seção 4.4.1)
    /// Substitui ClientHello1 por synthetic message_hash no TranscriptHash
    pub fn handle_hello_retry_request(&mut self, hrr_msg: &[u8]) -> Result<(), Tls13Alert> {
        if !is_hello_retry_request(hrr_msg) {
            return Err(Tls13Alert::IllegalParameter);
        }
        let ch1 = self.client_hello1.as_ref().ok_or(Tls13Alert::InternalError)?;

        // RFC 8446 Seção 4.4.1:
        // message_hash = Handshake {
        //     msg_type: message_hash (254),
        //     length: Hash.length (32),
        //     body: SHA-256(ClientHello1)
        // }
        let ch1_hash = Sha256::digest(ch1);
        let mut message_hash = Vec::with_capacity(4 + 32);
        message_hash.push(HandshakeType::MessageHash.as_u8());
        message_hash.push(0);
        message_hash.push(0);
        message_hash.push(32);
        message_hash.extend_from_slice(&ch1_hash);

        // Substitui ClientHello1 no transcript hash:
        // Transcript-Hash = Hash(message_hash || HelloRetryRequest)
        self.transcript.reset();
        self.transcript.update(&message_hash);
        self.transcript.update(hrr_msg);

        self.state = HandshakeState::HelloRetryRequestReceived;
        Ok(())
    }

    /// Constrói ClientHello2 após recebimento de HelloRetryRequest (RFC 8446)
    pub fn build_client_hello2(
        &mut self,
        client_random: &[u8; 32],
        client_public_key_bytes: &[u8],
    ) -> Vec<u8> {
        let mut ch_body = Vec::new();
        ch_body.extend_from_slice(&TLS12_LEGACY_VERSION.to_be_bytes());
        ch_body.extend_from_slice(client_random);
        ch_body.push(32);
        ch_body.extend_from_slice(client_random);

        let suites: [u16; 3] = [
            CipherSuite::TlsAes128GcmSha256.as_u16(),
            CipherSuite::TlsChacha20Poly1305Sha256.as_u16(),
            CipherSuite::TlsAes256GcmSha384.as_u16(),
        ];
        ch_body.extend_from_slice(&((suites.len() * 2) as u16).to_be_bytes());
        for suite in &suites {
            ch_body.extend_from_slice(&suite.to_be_bytes());
        }
        ch_body.push(1);
        ch_body.push(0x00);

        let mut extensions_data = Vec::new();
        let host_bytes = self.hostname.as_bytes();
        let sni_list_len = (host_bytes.len() + 3) as u16;
        let mut sni_ext = Vec::new();
        sni_ext.extend_from_slice(&sni_list_len.to_be_bytes());
        sni_ext.push(0x00);
        sni_ext.extend_from_slice(&(host_bytes.len() as u16).to_be_bytes());
        sni_ext.extend_from_slice(host_bytes);

        extensions_data.extend_from_slice(&0x0000u16.to_be_bytes());
        extensions_data.extend_from_slice(&(sni_ext.len() as u16).to_be_bytes());
        extensions_data.extend_from_slice(&sni_ext);

        let mut ver_ext = Vec::new();
        ver_ext.push(2);
        ver_ext.extend_from_slice(&TLS13_VERSION.to_be_bytes());
        extensions_data.extend_from_slice(&0x002Bu16.to_be_bytes());
        extensions_data.extend_from_slice(&(ver_ext.len() as u16).to_be_bytes());
        extensions_data.extend_from_slice(&ver_ext);

        let mut ks_ext = Vec::new();
        let client_shares_len = 2 + 2 + client_public_key_bytes.len();
        ks_ext.extend_from_slice(&(client_shares_len as u16).to_be_bytes());
        ks_ext.extend_from_slice(&(NamedGroup::Secp256r1.as_u16()).to_be_bytes());
        ks_ext.extend_from_slice(&(client_public_key_bytes.len() as u16).to_be_bytes());
        ks_ext.extend_from_slice(client_public_key_bytes);

        extensions_data.extend_from_slice(&0x0033u16.to_be_bytes());
        extensions_data.extend_from_slice(&(ks_ext.len() as u16).to_be_bytes());
        extensions_data.extend_from_slice(&ks_ext);

        ch_body.extend_from_slice(&(extensions_data.len() as u16).to_be_bytes());
        ch_body.extend_from_slice(&extensions_data);

        let mut handshake_msg = Vec::with_capacity(4 + ch_body.len());
        handshake_msg.push(HandshakeType::ClientHello.as_u8());
        let ch_len = ch_body.len();
        handshake_msg.push(((ch_len >> 16) & 0xFF) as u8);
        handshake_msg.push(((ch_len >> 8) & 0xFF) as u8);
        handshake_msg.push((ch_len & 0xFF) as u8);
        handshake_msg.extend_from_slice(&ch_body);

        self.transcript.update(&handshake_msg);
        self.state = HandshakeState::WaitServerHello;
        handshake_msg
    }

    /// Processa a mensagem `ServerHello` e efetua a transição para Handshake Secret
    pub fn process_server_hello(
        &mut self,
        server_hello_msg: &[u8],
        shared_secret_ecdhe: &[u8],
    ) -> Result<(), Tls13Alert> {
        if server_hello_msg.len() < 38 {
            return Err(Tls13Alert::DecodeError);
        }

        // Verifica tipo da mensagem
        if server_hello_msg[0] != HandshakeType::ServerHello.as_u8() {
            return Err(Tls13Alert::UnexpectedMessage);
        }

        let body_len = ((server_hello_msg[1] as usize) << 16)
            | ((server_hello_msg[2] as usize) << 8)
            | (server_hello_msg[3] as usize);

        if server_hello_msg.len() < 4 + body_len {
            return Err(Tls13Alert::DecodeError);
        }

        let body = &server_hello_msg[4..4 + body_len];
        let legacy_ver = u16::from_be_bytes([body[0], body[1]]);
        if legacy_ver != TLS12_LEGACY_VERSION {
            return Err(Tls13Alert::ProtocolVersion);
        }

        // Session ID echo
        let sess_id_len = body[34] as usize;
        let mut offset = 35 + sess_id_len;
        if offset + 3 > body.len() {
            return Err(Tls13Alert::DecodeError);
        }

        // Ciphersuite selecionada pelo servidor
        let suite_val = u16::from_be_bytes([body[offset], body[offset + 1]]);
        self.key_schedule.cipher_suite = CipherSuite::from_u16(suite_val);
        offset += 2;

        // Compression method (deve ser 0)
        let comp = body[offset];
        if comp != 0 {
            return Err(Tls13Alert::IllegalParameter);
        }

        // Atualiza o TranscriptHash com a mensagem bruta do ServerHello
        self.transcript.update(&server_hello_msg[..4 + body_len]);

        // Deriva os segredos de handshake
        let transcript_hash = self.transcript.current_hash();
        self.key_schedule.derive_handshake_secrets(shared_secret_ecdhe, &transcript_hash);

        self.state = HandshakeState::WaitEncryptedExtensions;
        Ok(())
    }

    /// Processa a mensagem `EncryptedExtensions`
    pub fn process_encrypted_extensions(&mut self, ee_msg: &[u8]) -> Result<(), Tls13Alert> {
        if ee_msg.is_empty() || ee_msg[0] != HandshakeType::EncryptedExtensions.as_u8() {
            return Err(Tls13Alert::UnexpectedMessage);
        }
        self.transcript.update(ee_msg);
        self.state = HandshakeState::WaitCertificate;
        Ok(())
    }

    /// Processa a mensagem `Certificate` contendo a cadeia X.509 e valida no Trust Store
    pub fn process_certificate(&mut self, cert_msg: &[u8]) -> Result<(), Tls13Alert> {
        if cert_msg.len() < 8 || cert_msg[0] != HandshakeType::Certificate.as_u8() {
            return Err(Tls13Alert::UnexpectedMessage);
        }

        let msg_len = ((cert_msg[1] as usize) << 16)
            | ((cert_msg[2] as usize) << 8)
            | (cert_msg[3] as usize);

        if cert_msg.len() < 4 + msg_len {
            return Err(Tls13Alert::DecodeError);
        }

        let body = &cert_msg[4..4 + msg_len];
        let req_ctx_len = body[0] as usize;
        let mut offset = 1 + req_ctx_len;

        if offset + 3 > body.len() {
            return Err(Tls13Alert::DecodeError);
        }

        let cert_list_len = ((body[offset] as usize) << 16)
            | ((body[offset + 1] as usize) << 8)
            | (body[offset + 2] as usize);
        offset += 3;

        let end_list = offset + cert_list_len;
        if end_list > body.len() {
            return Err(Tls13Alert::DecodeError);
        }

        let mut certificates = Vec::new();
        while offset < end_list {
            if offset + 3 > end_list {
                return Err(Tls13Alert::DecodeError);
            }
            let cert_data_len = ((body[offset] as usize) << 16)
                | ((body[offset + 1] as usize) << 8)
                | (body[offset + 2] as usize);
            offset += 3;

            if offset + cert_data_len > end_list {
                return Err(Tls13Alert::DecodeError);
            }

            let cert_bytes = &body[offset..offset + cert_data_len];
            offset += cert_data_len;

            // Extensões por certificado
            if offset + 2 > end_list {
                return Err(Tls13Alert::DecodeError);
            }
            let ext_len = ((body[offset] as usize) << 8) | (body[offset + 1] as usize);
            offset += 2 + ext_len;

            let parsed = parse_der_certificate(cert_bytes)
                .map_err(|_| Tls13Alert::BadCertificate)?;
            certificates.push(parsed);
        }

        if certificates.is_empty() {
            return Err(Tls13Alert::BadCertificate);
        }

        // Validação da cadeia de certificados com o motor X.509 da Fase 2 (RFC 5280 / RFC 6125)
        crate::net::pki::validate_chain(&certificates, &self.hostname, 0, &self.trust_store)
            .map_err(|_| Tls13Alert::BadCertificate)?;

        self.peer_certificate = Some(certificates[0].clone());
        self.transcript.update(cert_msg);
        self.state = HandshakeState::WaitCertificateVerify;
        Ok(())
    }

    /// Processa e valida criptograficamente a mensagem `CertificateVerify`
    pub fn process_certificate_verify(&mut self, cv_msg: &[u8]) -> Result<(), Tls13Alert> {
        if cv_msg.len() < 8 || cv_msg[0] != HandshakeType::CertificateVerify.as_u8() {
            return Err(Tls13Alert::UnexpectedMessage);
        }

        let msg_len = ((cv_msg[1] as usize) << 16)
            | ((cv_msg[2] as usize) << 8)
            | (cv_msg[3] as usize);

        if cv_msg.len() < 4 + msg_len {
            return Err(Tls13Alert::DecodeError);
        }

        let body = &cv_msg[4..4 + msg_len];
        let sig_scheme_val = u16::from_be_bytes([body[0], body[1]]);
        let sig_scheme = SignatureScheme::from_u16(sig_scheme_val);

        let sig_len = ((body[2] as usize) << 8) | (body[3] as usize);
        if body.len() < 4 + sig_len {
            return Err(Tls13Alert::DecodeError);
        }
        let signature_bytes = &body[4..4 + sig_len];

        let peer_cert = self.peer_certificate.as_ref().ok_or(Tls13Alert::InternalError)?;
        let transcript_hash_before_cv = self.transcript.current_hash();

        // Verificação matemática da assinatura usando a chave pública do certificado do servidor
        self.key_schedule.verify_certificate_verify(
            &peer_cert.public_key,
            sig_scheme,
            signature_bytes,
            &transcript_hash_before_cv,
        )?;

        self.transcript.update(cv_msg);
        self.state = HandshakeState::WaitFinished;
        Ok(())
    }

    /// Processa o bloco `Finished` do servidor e gera o bloco `Finished` do cliente
    pub fn process_server_finished(&mut self, finished_msg: &[u8]) -> Result<Vec<u8>, Tls13Alert> {
        if finished_msg.len() < 4 + 32 || finished_msg[0] != HandshakeType::Finished.as_u8() {
            return Err(Tls13Alert::UnexpectedMessage);
        }

        let msg_len = ((finished_msg[1] as usize) << 16)
            | ((finished_msg[2] as usize) << 8)
            | (finished_msg[3] as usize);

        if finished_msg.len() < 4 + msg_len || msg_len != 32 {
            return Err(Tls13Alert::DecodeError);
        }

        let server_verify_data = &finished_msg[4..4 + 32];
        let transcript_hash_before_finished = self.transcript.current_hash();

        // 1. Valida o HMAC do servidor
        self.key_schedule.verify_server_finished(server_verify_data, &transcript_hash_before_finished)?;

        // 2. Atualiza transcript com o Finished do servidor
        self.transcript.update(finished_msg);

        // 3. Transição para o Master Secret e chaves de tráfego de aplicação
        let transcript_hash_after_server_finished = self.transcript.current_hash();
        self.key_schedule.derive_master_secrets(&transcript_hash_after_server_finished)?;

        // 4. Constrói o bloco Finished do cliente
        let c_hs_traffic = self.key_schedule.client_handshake_traffic_secret.ok_or(Tls13Alert::InternalError)?;
        let client_verify_data = Tls13KeySchedule::compute_finished_verify_data(
            &c_hs_traffic,
            &transcript_hash_after_server_finished,
        );

        let mut client_finished_msg = Vec::with_capacity(4 + 32);
        client_finished_msg.push(HandshakeType::Finished.as_u8());
        client_finished_msg.push(0x00);
        client_finished_msg.push(0x00);
        client_finished_msg.push(32); // Comprimento 32 bytes
        client_finished_msg.extend_from_slice(&client_verify_data);

        // 5. Atualiza transcript com o Finished do cliente
        self.transcript.update(&client_finished_msg);
        self.state = HandshakeState::Connected;

        Ok(client_finished_msg)
    }
}
