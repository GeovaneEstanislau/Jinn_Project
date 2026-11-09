//! Pure Rust minimal X.509 ASN.1 DER certificate parser, PKI chain builder, and cryptographic validator (RFC 5280)

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::{format, string::{String, ToString}, vec::Vec};

use crate::bignum::BigUint;
use crate::rsa::{RSAPublicKey, parse_pkcs1_rsa_public_key};
use crate::p256::{P256PublicKey, parse_der_ecdsa_signature};
use crate::sha256::Sha256;

/// Algoritmos de assinatura X.509 suportados
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignatureAlgorithm {
    Sha256WithRsa,   // 1.2.840.113549.1.1.11
    RsaPssSha256,    // 1.2.840.113549.1.1.10
    EcdsaWithSha256, // 1.2.840.10045.4.3.2
    Sha384WithRsa,   // 1.2.840.113549.1.1.12
    Sha512WithRsa,   // 1.2.840.113549.1.1.13
    Unknown,
}

/// Chave pública de sujeito ou autoridade certificadora
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublicKey {
    Rsa(RSAPublicKey),
    EcdsaP256(P256PublicKey),
    Raw(Vec<u8>),
}

impl PublicKey {
    /// Verificação criptográfica de assinatura sobre os bytes brutos do TBSCertificate
    pub fn verify_signature(
        &self,
        sig_alg: SignatureAlgorithm,
        tbs_raw: &[u8],
        signature_bytes: &[u8],
    ) -> Result<(), &'static str> {
        match self {
            PublicKey::Rsa(rsa_key) => match sig_alg {
                SignatureAlgorithm::Sha256WithRsa => {
                    let hash = Sha256::digest(tbs_raw);
                    rsa_key.verify_pkcs1v15_sha256(&hash, signature_bytes)
                }
                SignatureAlgorithm::RsaPssSha256 => {
                    let hash = Sha256::digest(tbs_raw);
                    rsa_key.verify_pss_sha256(&hash, signature_bytes)
                }
                _ => Err("Algoritmo de assinatura incompativel com chave RSA"),
            },
            PublicKey::EcdsaP256(p256_key) => match sig_alg {
                SignatureAlgorithm::EcdsaWithSha256 => {
                    let hash = Sha256::digest(tbs_raw);
                    let (r, s) = parse_der_ecdsa_signature(signature_bytes)?;
                    p256_key.verify_signature(&hash, &r, &s)
                }
                _ => Err("Algoritmo de assinatura incompativel com chave ECDSA P-256"),
            },
            PublicKey::Raw(_) => Err("Tipo de chave publica nao suportado para verificacao criptografica"),
        }
    }
}

/// Certificado X.509 decodificado com dados criptográficos e extensões
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCertificate {
    pub subject: String,
    pub issuer: String,
    pub not_before: u64,
    pub not_after: u64,
    pub dns_names: Vec<String>,
    pub public_key: PublicKey,
    pub raw_public_key: Vec<u8>,
    pub sig_alg: SignatureAlgorithm,
    pub signature: Vec<u8>,
    pub raw_tbs: Vec<u8>,
    // Extensões de Governança PKI
    pub is_ca: bool,
    pub path_len_constraint: Option<u32>,
    pub key_usage: Option<u16>,
    pub extended_key_usage: Vec<Vec<u8>>,
    pub subject_key_id: Option<Vec<u8>>,
    pub authority_key_id: Option<Vec<u8>>,
}

/// Âncora de Confiança Raiz (Trust Anchor) com chave pública autenticada
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustAnchor {
    pub subject: String,
    pub public_key: PublicKey,
    pub key_id: Option<Vec<u8>>,
    pub is_ca: bool,
}

/// Repositório de Âncoras de Confiança Raiz (Trust Store)
#[derive(Debug, Clone)]
pub struct TrustStore {
    pub anchors: Vec<TrustAnchor>,
}

impl TrustStore {
    pub fn new() -> Self {
        let mut store = Self {
            anchors: Vec::new(),
        };

        // 1. ISRG Root X1 (Let's Encrypt Root CA - RSA 4096 bits)
        // Módulo RSA genuíno extraído do repositório canônico mundial
        let isrg_modulus_hex = b"ADE82473F41437F39B9E2B57281C87BEDCB7DF38908C6E3CE657A078F775C2A2FEF56A6EF6004F28DBDE68866C4493B6B163FD14126BBF1FD2EA319B217ED1333CBA48F5DD79DFB3B8FF12F1219A4BC18A8671694A66666C8F7E3C70BFAD292206F3E4C0E680AEE24B8FB7997E94039FD347977C99482353E838AE4F0A6F832ED149578C8074B6DA2FD0388D7B0370211B75F2303CFA8FAEDDDA63ABEB164FC28E114B7ECF0BE8FFB5772EF4B27B4AE04C12250C708D0329A0E15324EC13D9EE19BF10B34A8C3F89A36151DEAC870794F46371EC2EE26F5B9881E1895C34796C76EF3B906279E6DBA49A2F26C5D010E10EDED9108E16FBB7F7A8F7C7E50207988F360895E7E237960D36759EFB0E72B11D9BBC03F94905D881DD05B42AD641E9AC0176950A0FD8DFD5BD121F352F28176CD298C1A80964776E4737BACEAC595E689D7F72D689C50641293E593EDD26F524C911A75AA34C401F46A199B5A73A516E863B9E7D72A712057859ED3E5178150B038F8DD02F05B23E7B4A1C4B730512FCC6EAE050137C439374B3CA74E78E1F0108D030D45B7136B407BAC130305C48B7823B98A67D608AA2A32982CCBABD83041BA2830341A1D605F11BC2B6F0A87C863B46A8482A88DC769A76BF1F6AA53D198FEB38F364DEC82B0D0A28FFF7DBE21542D422D0275DE179FE18E77088AD4EE6D98B3AC6DD27516EFFBC64F533434F";
        let isrg_n = hex_to_biguint(isrg_modulus_hex);
        let isrg_e = BigUint::from_u64(65537);
        store.add_anchor(TrustAnchor {
            subject: "CN=ISRG Root X1, O=Internet Security Research Group, C=US".to_string(),
            public_key: PublicKey::Rsa(RSAPublicKey::new(isrg_n, isrg_e)),
            key_id: None,
            is_ca: true,
        });

        store
    }

    pub fn add_anchor(&mut self, anchor: TrustAnchor) {
        self.anchors.push(anchor);
    }

    /// Adiciona certificado raiz validando que é autoassinado
    pub fn add_certificate(&mut self, cert: &ParsedCertificate) -> Result<(), &'static str> {
        if cert.subject != cert.issuer {
            return Err("Certificado raiz deve ser autoassinado (Subject == Issuer)");
        }
        if !cert.is_ca {
            return Err("Certificado raiz deve possuir basicConstraints CA=TRUE");
        }
        // Verifica a assinatura do próprio certificado raiz
        cert.public_key.verify_signature(cert.sig_alg, &cert.raw_tbs, &cert.signature)?;

        self.add_anchor(TrustAnchor {
            subject: cert.subject.clone(),
            public_key: cert.public_key.clone(),
            key_id: cert.subject_key_id.clone(),
            is_ca: true,
        });
        Ok(())
    }

    /// Método de compatibilidade legado (apenas registra intenção de nome, se chave não estiver pré-carregada)
    pub fn add_root(&mut self, _root_common_name: &str) {
        // No-op seguro: nomes puros sem chave pública não são mais adicionados
    }

    pub fn find_matching_anchor(&self, issuer_dn: &str, authority_key_id: Option<&[u8]>) -> Option<&TrustAnchor> {
        for anchor in &self.anchors {
            // Se authority_key_id foi especificado e âncora possui key_id, devem coincidir
            if let (Some(akid), Some(skid)) = (authority_key_id, &anchor.key_id) {
                if akid != skid.as_slice() {
                    continue;
                }
            }
            if dn_matches(&anchor.subject, issuer_dn) {
                return Some(anchor);
            }
        }
        None
    }

    /// Verificação estrita de confiança: O certificado é aceito SOMENTE se sua assinatura digital
    /// verificar criptograficamente contra uma âncora de confiança autêntica do TrustStore.
    pub fn is_trusted(&self, cert: &ParsedCertificate) -> bool {
        if let Some(anchor) = self.find_matching_anchor(&cert.issuer, cert.authority_key_id.as_deref()) {
            anchor.public_key.verify_signature(cert.sig_alg, &cert.raw_tbs, &cert.signature).is_ok()
        } else {
            false
        }
    }
}

/// Erros estruturados e granulares de validação de certificados X.509
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CertificateValidationError {
    EmptyChain,
    CertificateExpired { not_after: u64, now: u64, subject: String },
    CertificateNotYetValid { not_before: u64, now: u64, subject: String },
    HostnameMismatch { expected: String, actual: Vec<String> },
    UntrustedRootIssuer { issuer: String },
    RootSignatureVerificationFailed { issuer: String, reason: &'static str },
    IntermediateSignatureVerificationFailed { issuer: String, subject: String, reason: &'static str },
    IntermediateNotCA { subject: String },
    IntermediateMissingKeyCertSign { subject: String },
    PathLenConstraintExceeded { max: u32, actual: u32 },
    LeafMissingDigitalSignature { subject: String },
    LeafMissingServerAuth { subject: String },
    IssuerSubjectMismatch { expected_issuer: String, actual_subject: String },
}

impl core::fmt::Display for CertificateValidationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EmptyChain => write!(f, "Cadeia de certificados vazia"),
            Self::CertificateExpired { not_after, now, subject } => {
                write!(f, "Certificado '{}' expirado: notAfter={} < now={}", subject, not_after, now)
            }
            Self::CertificateNotYetValid { not_before, now, subject } => {
                write!(f, "Certificado '{}' ainda nao valido: notBefore={} > now={}", subject, not_before, now)
            }
            Self::HostnameMismatch { expected, actual } => {
                write!(f, "Hostname esperado '{}' nao coincide com SANs: {:?}", expected, actual)
            }
            Self::UntrustedRootIssuer { issuer } => {
                write!(f, "Emissor raiz '{}' nao reconhecido no TrustStore", issuer)
            }
            Self::RootSignatureVerificationFailed { issuer, reason } => {
                write!(f, "FALHA CRIPTOGRAFICA na assinatura da raiz '{}': {}", issuer, reason)
            }
            Self::IntermediateSignatureVerificationFailed { issuer, subject, reason } => {
                write!(f, "FALHA CRIPTOGRAFICA na assinatura do intermediario '{}' emitido por '{}': {}", subject, issuer, reason)
            }
            Self::IntermediateNotCA { subject } => {
                write!(f, "Certificado intermediario '{}' viola basicConstraints: nao e CA (cA=false)", subject)
            }
            Self::IntermediateMissingKeyCertSign { subject } => {
                write!(f, "Certificado intermediario '{}' viola keyUsage: bit keyCertSign ausente", subject)
            }
            Self::PathLenConstraintExceeded { max, actual } => {
                write!(f, "Limite de profundidade pathLenConstraint violado: max={}, atual={}", max, actual)
            }
            Self::LeafMissingDigitalSignature { subject } => {
                write!(f, "Certificado folha '{}' viola keyUsage: bit digitalSignature ausente", subject)
            }
            Self::LeafMissingServerAuth { subject } => {
                write!(f, "Certificado folha '{}' viola extendedKeyUsage: OID serverAuth ausente", subject)
            }
            Self::IssuerSubjectMismatch { expected_issuer, actual_subject } => {
                write!(f, "Incompatibilidade na cadeia: emissor esperado '{}', sujeito recebido '{}'", expected_issuer, actual_subject)
            }
        }
    }
}

/// Analisador DER completo RFC 5280
pub fn parse_der_certificate(der: &[u8]) -> Result<ParsedCertificate, &'static str> {
    let mut reader = DerReader::new(der);
    let cert_seq = reader.read_sequence()?;
    let mut cert_reader = DerReader::new(cert_seq);

    // 1. TBSCertificate (salva bytes brutos para conferência de hash)
    let (tbs_tag, tbs_bytes, raw_tbs) = cert_reader.read_element_with_raw()?;
    if tbs_tag != 0x30 {
        return Err("Tag SEQUENCE (0x30) esperada para TBSCertificate");
    }

    let mut tbs = DerReader::new(tbs_bytes);

    // Version (opcional [0] EXPLICIT)
    if tbs.peek_tag() == Some(0xA0) {
        let _ = tbs.read_element()?;
    }

    // Serial Number
    let _serial = tbs.read_element()?;

    // Signature AlgorithmIdentifier dentro do TBS
    let tbs_sig_alg_elem = tbs.read_element()?;
    let tbs_sig_alg = parse_algorithm_identifier(tbs_sig_alg_elem.data);

    // Issuer DN
    let issuer_bytes = tbs.read_sequence()?;
    let issuer_name = parse_distinguished_name(issuer_bytes);

    // Validity (notBefore, notAfter)
    let validity_bytes = tbs.read_sequence()?;
    let (not_before, not_after) = parse_validity(validity_bytes)?;

    // Subject DN
    let subject_bytes = tbs.read_sequence()?;
    let subject_name = parse_distinguished_name(subject_bytes);

    // SubjectPublicKeyInfo
    let spki_bytes = tbs.read_sequence()?;
    let (public_key, raw_public_key) = parse_spki(spki_bytes)?;

    // Extensões X.509 (opcional [3] EXPLICIT)
    let mut dns_names = Vec::new();
    let mut is_ca = false;
    let mut path_len_constraint = None;
    let mut key_usage = None;
    let mut extended_key_usage = Vec::new();
    let mut subject_key_id = None;
    let mut authority_key_id = None;

    while !tbs.is_empty() {
        if let Ok(elem) = tbs.read_element() {
            if elem.tag == 0xA3 {
                // Extensions sequence
                if let Ok(seq) = DerReader::new(elem.data).read_sequence() {
                    let mut ext_reader = DerReader::new(seq);
                    while !ext_reader.is_empty() {
                        if let Ok(ext) = ext_reader.read_sequence() {
                            parse_extension(
                                ext,
                                &mut dns_names,
                                &mut is_ca,
                                &mut path_len_constraint,
                                &mut key_usage,
                                &mut extended_key_usage,
                                &mut subject_key_id,
                                &mut authority_key_id,
                            );
                        }
                    }
                }
            }
        }
    }

    if dns_names.is_empty() && !subject_name.is_empty() {
        dns_names.push(extract_common_name(&subject_name));
    }

    // 2. Outer SignatureAlgorithm
    let outer_sig_elem = cert_reader.read_element()?;
    let outer_sig_alg = parse_algorithm_identifier(outer_sig_elem.data);

    // 3. Outer SignatureValue (BIT STRING)
    let sig_val_bytes = cert_reader.read_bit_string()?;

    Ok(ParsedCertificate {
        subject: subject_name,
        issuer: issuer_name,
        not_before,
        not_after,
        dns_names,
        public_key,
        raw_public_key,
        sig_alg: if outer_sig_alg != SignatureAlgorithm::Unknown { outer_sig_alg } else { tbs_sig_alg },
        signature: sig_val_bytes.to_vec(),
        raw_tbs: raw_tbs.to_vec(),
        is_ca,
        path_len_constraint,
        key_usage,
        extended_key_usage,
        subject_key_id,
        authority_key_id,
    })
}

/// Validação completa de cadeia de certificados até Trust Anchor conforme RFC 5280
pub fn validate_chain(
    chain: &[ParsedCertificate],
    expected_hostname: &str,
    current_unix_time: u64,
    trust_store: &TrustStore,
) -> Result<(), CertificateValidationError> {
    if chain.is_empty() {
        return Err(CertificateValidationError::EmptyChain);
    }

    let leaf = &chain[0];

    // 1. Validação temporal do certificado folha
    if current_unix_time > 0 {
        if current_unix_time < leaf.not_before {
            return Err(CertificateValidationError::CertificateNotYetValid {
                not_before: leaf.not_before,
                now: current_unix_time,
                subject: leaf.subject.clone(),
            });
        }
        if current_unix_time > leaf.not_after {
            return Err(CertificateValidationError::CertificateExpired {
                not_after: leaf.not_after,
                now: current_unix_time,
                subject: leaf.subject.clone(),
            });
        }
    }

    // 2. Validação de Hostname (SAN / CN)
    let mut hostname_matched = false;
    let target = expected_hostname.to_lowercase();

    for dns in &leaf.dns_names {
        let pattern = dns.to_lowercase();
        if pattern == target {
            hostname_matched = true;
            break;
        }
        if pattern.starts_with("*.") {
            let base = &pattern[2..];
            if target.ends_with(base) && target.matches('.').count() == pattern.matches('.').count() {
                hostname_matched = true;
                break;
            }
        }
    }

    if !hostname_matched {
        return Err(CertificateValidationError::HostnameMismatch {
            expected: expected_hostname.to_string(),
            actual: leaf.dns_names.clone(),
        });
    }

    // 3. Validação de Extensões do Certificado Folha
    // Se keyUsage estiver presente, deve conter digitalSignature (bit 0 / 0x80)
    if let Some(ku) = leaf.key_usage {
        if (ku & 0x80) == 0 {
            return Err(CertificateValidationError::LeafMissingDigitalSignature {
                subject: leaf.subject.clone(),
            });
        }
    }

    // Se extendedKeyUsage estiver presente e não vazio, deve conter serverAuth (1.3.6.1.5.5.7.3.1)
    const OID_SERVER_AUTH: &[u8] = &[0x2B, 0x06, 0x01, 0x05, 0x05, 0x07, 0x03, 0x01];
    const OID_ANY_EKU: &[u8] = &[0x55, 0x1D, 0x25, 0x00];
    if !leaf.extended_key_usage.is_empty() {
        let has_server_auth = leaf.extended_key_usage.iter().any(|eku| eku == OID_SERVER_AUTH || eku == OID_ANY_EKU);
        if !has_server_auth {
            return Err(CertificateValidationError::LeafMissingServerAuth {
                subject: leaf.subject.clone(),
            });
        }
    }

    // 4. Verificação de Elos Intermediários da Cadeia
    for i in 0..chain.len() - 1 {
        let child = &chain[i];
        let parent = &chain[i + 1];

        // Validação temporal do intermediário
        if current_unix_time > 0 {
            if current_unix_time < parent.not_before {
                return Err(CertificateValidationError::CertificateNotYetValid {
                    not_before: parent.not_before,
                    now: current_unix_time,
                    subject: parent.subject.clone(),
                });
            }
            if current_unix_time > parent.not_after {
                return Err(CertificateValidationError::CertificateExpired {
                    not_after: parent.not_after,
                    now: current_unix_time,
                    subject: parent.subject.clone(),
                });
            }
        }

        // Issuer do filho deve coincidir com Subject do pai
        if !dn_matches(&parent.subject, &child.issuer) {
            return Err(CertificateValidationError::IssuerSubjectMismatch {
                expected_issuer: child.issuer.clone(),
                actual_subject: parent.subject.clone(),
            });
        }

        // Pai deve ser uma Autoridade Certificadora válida (cA=true)
        if !parent.is_ca {
            return Err(CertificateValidationError::IntermediateNotCA {
                subject: parent.subject.clone(),
            });
        }

        // Pai deve possuir permissão keyCertSign no keyUsage (bit 5 / 0x04)
        if let Some(ku) = parent.key_usage {
            if (ku & 0x04) == 0 {
                return Err(CertificateValidationError::IntermediateMissingKeyCertSign {
                    subject: parent.subject.clone(),
                });
            }
        }

        // Limite de profundidade de intermediários (pathLenConstraint)
        if let Some(max_path) = parent.path_len_constraint {
            if (i as u32) > max_path {
                return Err(CertificateValidationError::PathLenConstraintExceeded {
                    max: max_path,
                    actual: i as u32,
                });
            }
        }

        // VERIFICAÇÃO CRIPTOGRÁFICA DA ASSINATURA:
        // A chave pública do pai DEVE verificar a assinatura sobre os bytes brutos do TBS do filho!
        if let Err(reason) = parent.public_key.verify_signature(child.sig_alg, &child.raw_tbs, &child.signature) {
            return Err(CertificateValidationError::IntermediateSignatureVerificationFailed {
                issuer: parent.subject.clone(),
                subject: child.subject.clone(),
                reason,
            });
        }
    }

    // 5. Verificação da Raiz com Trust Store
    let top = &chain[chain.len() - 1];

    let anchor = trust_store.find_matching_anchor(&top.issuer, top.authority_key_id.as_deref())
        .ok_or_else(|| CertificateValidationError::UntrustedRootIssuer { issuer: top.issuer.clone() })?;

    // VERIFICAÇÃO CRIPTOGRÁFICA DA ASSINATURA DA RAIZ:
    // A chave pública genuína da âncora de confiança DEVE verificar matematicamente a assinatura do topo!
    if let Err(reason) = anchor.public_key.verify_signature(top.sig_alg, &top.raw_tbs, &top.signature) {
        return Err(CertificateValidationError::RootSignatureVerificationFailed {
            issuer: anchor.subject.clone(),
            reason,
        });
    }

    Ok(())
}

/// Validador unificado de certificado (compatibilidade com interface anterior)
pub fn validate_certificate(
    cert: &ParsedCertificate,
    expected_hostname: &str,
    current_unix_time: u64,
    trust_store: &TrustStore,
) -> Result<(), String> {
    #[cfg(not(feature = "std"))]
    let chain = alloc::vec![cert.clone()];
    #[cfg(feature = "std")]
    let chain = vec![cert.clone()];

    validate_chain(&chain, expected_hostname, current_unix_time, trust_store)
        .map_err(|err| format!("{}", err))
}

// ============================================================================
// ANALISADOR DER E FUNÇÕES AUXILIARES
// ============================================================================

struct DerReader<'a> {
    data: &'a [u8],
    offset: usize,
}

struct DerElement<'a> {
    tag: u8,
    data: &'a [u8],
}

impl<'a> DerReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn is_empty(&self) -> bool {
        self.offset >= self.data.len()
    }

    fn peek_tag(&self) -> Option<u8> {
        self.data.get(self.offset).copied()
    }

    fn read_element(&mut self) -> Result<DerElement<'a>, &'static str> {
        let (tag, data, _) = self.read_element_with_raw()?;
        Ok(DerElement { tag, data })
    }

    fn read_element_with_raw(&mut self) -> Result<(u8, &'a [u8], &'a [u8]), &'static str> {
        if self.offset >= self.data.len() {
            return Err("EOF inesperado lendo tag DER");
        }
        let start_offset = self.offset;
        let tag = self.data[self.offset];
        self.offset += 1;

        if self.offset >= self.data.len() {
            return Err("EOF inesperado lendo comprimento DER");
        }
        let first_len = self.data[self.offset];
        self.offset += 1;

        let len = if first_len & 0x80 == 0 {
            first_len as usize
        } else {
            let num_octets = (first_len & 0x7F) as usize;
            if num_octets > 4 || self.offset + num_octets > self.data.len() {
                return Err("Octetos de comprimento DER invalidos");
            }
            let mut l = 0usize;
            for _ in 0..num_octets {
                l = (l << 8) | (self.data[self.offset] as usize);
                self.offset += 1;
            }
            l
        };

        if self.offset + len > self.data.len() {
            return Err("Comprimento de elemento DER ultrapassa o buffer");
        }

        let elem_data = &self.data[self.offset..self.offset + len];
        self.offset += len;
        let raw_elem = &self.data[start_offset..self.offset];

        Ok((tag, elem_data, raw_elem))
    }

    fn read_sequence(&mut self) -> Result<&'a [u8], &'static str> {
        let elem = self.read_element()?;
        if elem.tag == 0x30 {
            Ok(elem.data)
        } else {
            Err("Tag SEQUENCE (0x30) esperada")
        }
    }

    fn read_bit_string(&mut self) -> Result<&'a [u8], &'static str> {
        let elem = self.read_element()?;
        if elem.tag == 0x03 && !elem.data.is_empty() {
            // Primeiro byte do BIT STRING indica o número de bits não usados (0..7)
            Ok(&elem.data[1..])
        } else {
            Err("Tag BIT STRING (0x03) esperada")
        }
    }
}

fn parse_algorithm_identifier(data: &[u8]) -> SignatureAlgorithm {
    let mut reader = DerReader::new(data);
    if let Ok(oid_elem) = reader.read_element() {
        if oid_elem.tag == 0x06 {
            // sha256WithRSAEncryption: 1.2.840.113549.1.1.11 -> [0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0B]
            if oid_elem.data == [0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0B] {
                return SignatureAlgorithm::Sha256WithRsa;
            }
            // rsaPSS: 1.2.840.113549.1.1.10 -> [0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0A]
            if oid_elem.data == [0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0A] {
                return SignatureAlgorithm::RsaPssSha256;
            }
            // ecdsa-with-SHA256: 1.2.840.10045.4.3.2 -> [0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x04, 0x03, 0x02]
            if oid_elem.data == [0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x04, 0x03, 0x02] {
                return SignatureAlgorithm::EcdsaWithSha256;
            }
            // sha384WithRSAEncryption: 1.2.840.113549.1.1.12
            if oid_elem.data == [0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0C] {
                return SignatureAlgorithm::Sha384WithRsa;
            }
            // sha512WithRSAEncryption: 1.2.840.113549.1.1.13
            if oid_elem.data == [0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0D] {
                return SignatureAlgorithm::Sha512WithRsa;
            }
        }
    }
    SignatureAlgorithm::Unknown
}

fn parse_spki(spki_bytes: &[u8]) -> Result<(PublicKey, Vec<u8>), &'static str> {
    let mut reader = DerReader::new(spki_bytes);
    let alg_bytes = reader.read_sequence()?;
    let key_bits = reader.read_bit_string()?;

    let mut alg_reader = DerReader::new(alg_bytes);
    let oid_elem = alg_reader.read_element()?;

    // rsaEncryption: 1.2.840.113549.1.1.1 -> [0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01]
    if oid_elem.tag == 0x06 && oid_elem.data == [0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01] {
        let rsa_key = parse_pkcs1_rsa_public_key(key_bits)?;
        return Ok((PublicKey::Rsa(rsa_key), key_bits.to_vec()));
    }

    // id-ecPublicKey: 1.2.840.10045.2.1 -> [0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x02, 0x01]
    if oid_elem.tag == 0x06 && oid_elem.data == [0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x02, 0x01] {
        let p256_key = P256PublicKey::from_sec1_bytes(key_bits)?;
        return Ok((PublicKey::EcdsaP256(p256_key), key_bits.to_vec()));
    }

    Ok((PublicKey::Raw(key_bits.to_vec()), key_bits.to_vec()))
}

fn parse_distinguished_name(dn_data: &[u8]) -> String {
    let mut reader = DerReader::new(dn_data);
    let mut name_parts = Vec::new();

    while !reader.is_empty() {
        if let Ok(elem) = reader.read_element() {
            if elem.tag == 0x31 { // SET
                let mut set_reader = DerReader::new(elem.data);
                if let Ok(seq) = set_reader.read_sequence() {
                    let mut atv = DerReader::new(seq);
                    if let Ok(oid_elem) = atv.read_element() {
                        if let Ok(val_elem) = atv.read_element() {
                            if let Ok(val_str) = core::str::from_utf8(val_elem.data) {
                                let prefix = match oid_elem.data {
                                    [0x55, 0x04, 0x03] => "CN=",
                                    [0x55, 0x04, 0x0A] => "O=",
                                    [0x55, 0x04, 0x0B] => "OU=",
                                    [0x55, 0x04, 0x06] => "C=",
                                    _ => "",
                                };
                                name_parts.push(format!("{}{}", prefix, val_str));
                            }
                        }
                    }
                }
            }
        }
    }

    name_parts.join(", ")
}

fn extract_common_name(dn: &str) -> String {
    for part in dn.split(',') {
        let p = part.trim();
        if p.starts_with("CN=") {
            return p[3..].to_string();
        }
    }
    dn.to_string()
}

fn dn_matches(a: &str, b: &str) -> bool {
    let clean = |s: &str| -> String {
        s.split(',')
            .map(|p| p.trim().to_lowercase())
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join(",")
    };
    clean(a) == clean(b) || a.to_lowercase() == b.to_lowercase()
}

fn parse_validity(validity_data: &[u8]) -> Result<(u64, u64), &'static str> {
    let mut reader = DerReader::new(validity_data);
    let nb_elem = reader.read_element()?;
    let na_elem = reader.read_element()?;

    let nb = parse_time_element(nb_elem.tag, nb_elem.data)?;
    let na = parse_time_element(na_elem.tag, na_elem.data)?;

    Ok((nb, na))
}

fn parse_time_element(tag: u8, data: &[u8]) -> Result<u64, &'static str> {
    let s = core::str::from_utf8(data).map_err(|_| "UTF-8 invalido na data")?;
    if tag == 0x17 { // UTCTime: YYMMDDHHMMSSZ
        if s.len() < 12 { return Err("UTCTime muito curto"); }
        let year_val: u16 = s[0..2].parse().map_err(|_| "Ano UTCTime invalido")?;
        let year = if year_val >= 50 { 1900 + year_val } else { 2000 + year_val };
        let month: u8 = s[2..4].parse().map_err(|_| "Mes invalido")?;
        let day: u8 = s[4..6].parse().map_err(|_| "Dia invalido")?;
        let hour: u8 = s[6..8].parse().map_err(|_| "Hora invalida")?;
        let min: u8 = s[8..10].parse().map_err(|_| "Minuto invalido")?;
        let sec: u8 = s[10..12].parse().map_err(|_| "Segundo invalido")?;

        Ok(crate::x509_date_to_unix(year, month, day, hour, min, sec))
    } else if tag == 0x18 { // GeneralizedTime: YYYYMMDDHHMMSSZ
        if s.len() < 14 { return Err("GeneralizedTime muito curto"); }
        let year: u16 = s[0..4].parse().map_err(|_| "Ano GeneralizedTime invalido")?;
        let month: u8 = s[4..6].parse().map_err(|_| "Mes invalido")?;
        let day: u8 = s[6..8].parse().map_err(|_| "Dia invalido")?;
        let hour: u8 = s[8..10].parse().map_err(|_| "Hora invalida")?;
        let min: u8 = s[10..12].parse().map_err(|_| "Minuto invalido")?;
        let sec: u8 = s[12..14].parse().map_err(|_| "Segundo invalido")?;

        Ok(crate::x509_date_to_unix(year, month, day, hour, min, sec))
    } else {
        Err("Tag de formato de tempo nao suportada em X.509")
    }
}

fn parse_extension(
    ext_data: &[u8],
    dns_names: &mut Vec<String>,
    is_ca: &mut bool,
    path_len_constraint: &mut Option<u32>,
    key_usage: &mut Option<u16>,
    extended_key_usage: &mut Vec<Vec<u8>>,
    subject_key_id: &mut Option<Vec<u8>>,
    authority_key_id: &mut Option<Vec<u8>>,
) {
    let mut reader = DerReader::new(ext_data);
    if let Ok(oid_elem) = reader.read_element() {
        if oid_elem.tag != 0x06 { return; }
        let oid = oid_elem.data;

        // Leitura de critical (opcional BOOLEAN) e extnValue (OCTET STRING)
        let mut extn_val_bytes = None;
        while !reader.is_empty() {
            if let Ok(elem) = reader.read_element() {
                if elem.tag == 0x04 { // OCTET STRING
                    extn_val_bytes = Some(elem.data);
                    break;
                }
            }
        }

        let val_bytes = match extn_val_bytes {
            Some(v) => v,
            None => return,
        };

        // 1. SAN: 2.5.29.17 -> [0x55, 0x1D, 0x11]
        if oid == [0x55, 0x1D, 0x11] {
            if let Ok(seq) = DerReader::new(val_bytes).read_sequence() {
                let mut san_reader = DerReader::new(seq);
                while !san_reader.is_empty() {
                    if let Ok(san_elem) = san_reader.read_element() {
                        if san_elem.tag == 0x82 { // context [2] dNSName
                            if let Ok(s) = core::str::from_utf8(san_elem.data) {
                                dns_names.push(s.to_string());
                            }
                        } else if san_elem.tag == 0x87 { // context [7] iPAddress
                            if san_elem.data.len() == 4 {
                                dns_names.push(format!("{}.{}.{}.{}", san_elem.data[0], san_elem.data[1], san_elem.data[2], san_elem.data[3]));
                            }
                        }
                    }
                }
            }
        }
        // 2. basicConstraints: 2.5.29.19 -> [0x55, 0x1D, 0x13]
        else if oid == [0x55, 0x1D, 0x13] {
            if let Ok(seq) = DerReader::new(val_bytes).read_sequence() {
                let mut bc_reader = DerReader::new(seq);
                while !bc_reader.is_empty() {
                    if let Ok(bc_elem) = bc_reader.read_element() {
                        if bc_elem.tag == 0x01 && !bc_elem.data.is_empty() { // BOOLEAN cA
                            *is_ca = bc_elem.data[0] != 0;
                        } else if bc_elem.tag == 0x02 && !bc_elem.data.is_empty() { // INTEGER pathLenConstraint
                            let mut plen = 0u32;
                            for &b in bc_elem.data {
                                plen = (plen << 8) | (b as u32);
                            }
                            *path_len_constraint = Some(plen);
                        }
                    }
                }
            }
        }
        // 3. keyUsage: 2.5.29.15 -> [0x55, 0x1D, 0x0F]
        else if oid == [0x55, 0x1D, 0x0F] {
            let mut val_reader = DerReader::new(val_bytes);
            if let Ok(bits) = val_reader.read_bit_string() {
                if !bits.is_empty() {
                    let b1 = bits[0];
                    let b2 = if bits.len() > 1 { bits[1] } else { 0 };
                    *key_usage = Some((b1 as u16) | ((b2 as u16) << 8));
                }
            } else if val_bytes.len() >= 2 {
                let b1 = val_bytes[val_bytes.len() - 1];
                *key_usage = Some(b1 as u16);
            }
        }
        // 4. extendedKeyUsage: 2.5.29.37 -> [0x55, 0x1D, 0x25]
        else if oid == [0x55, 0x1D, 0x25] {
            if let Ok(seq) = DerReader::new(val_bytes).read_sequence() {
                let mut eku_reader = DerReader::new(seq);
                while !eku_reader.is_empty() {
                    if let Ok(elem) = eku_reader.read_element() {
                        if elem.tag == 0x06 { // OID
                            extended_key_usage.push(elem.data.to_vec());
                        }
                    }
                }
            }
        }
        // 5. subjectKeyIdentifier: 2.5.29.14 -> [0x55, 0x1D, 0x0E]
        else if oid == [0x55, 0x1D, 0x0E] {
            if val_bytes.len() >= 2 && val_bytes[0] == 0x04 {
                let len = val_bytes[1] as usize;
                if val_bytes.len() >= 2 + len {
                    *subject_key_id = Some(val_bytes[2..2 + len].to_vec());
                }
            } else {
                *subject_key_id = Some(val_bytes.to_vec());
            }
        }
        // 6. authorityKeyIdentifier: 2.5.29.35 -> [0x55, 0x1D, 0x23]
        else if oid == [0x55, 0x1D, 0x23] {
            if let Ok(seq) = DerReader::new(val_bytes).read_sequence() {
                let mut akid_reader = DerReader::new(seq);
                while !akid_reader.is_empty() {
                    if let Ok(elem) = akid_reader.read_element() {
                        if elem.tag == 0x80 { // [0] keyIdentifier
                            *authority_key_id = Some(elem.data.to_vec());
                            break;
                        }
                    }
                }
            }
        }
    }
}

fn hex_to_biguint(hex: &[u8]) -> BigUint {
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let mut i = 0;
    while i < hex.len() {
        let hi = hex_nibble(hex[i]);
        let lo = hex_nibble(hex[i + 1]);
        bytes.push((hi << 4) | lo);
        i += 2;
    }
    BigUint::from_be_bytes(&bytes)
}

fn hex_nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => 0,
    }
}
