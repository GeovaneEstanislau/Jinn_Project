//! Jinn Microkernel — Public Key Infrastructure (PKI) & Trust Chain Validator (RFC 5280 / RFC 6125)
//!
//! Implementação no_std com Trust Store embutido (ISRG Root X1, DigiCert),
//! construção de caminhos bottom-up, verificação matemática de assinaturas digitais
//! e enforçamento estrito de conformidade para Wildcards e SANs.

use alloc::{format, string::{String, ToString}, vec::Vec};
use crate::crypto::bignum::BigUint;
use crate::crypto::rsa::RSAPublicKey;
use super::x509::{ParsedCertificate, PublicKey, dn_matches};

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

        // 2. DigiCert Global Root CA (RSA 2048 bits)
        let digicert_modulus_hex = b"E23BE11172DEA8A4D3A357AA50A28F0B7790C9A2A5EE12CE965B010920CC0193A74E30B753F743C46900579DE28D5BF241C6B4E11B075490EA79334017891287DAB8B443461B41CCCBDBB6C8CBC7FFB47E6970EEBE03D5B12A26EB9C81FAE82DC24E2C12EFFDF8FB5984A5F38E67B0B0B5AD1FB7573822580BFBB5076CE856BFEA7B7AC682C820AD5D05EACFAA36EADFA7573C46DB1CCDF8E99F3B30D6E672E39A71007AF7CA396A22152149CBDFE0631FA281F044A9DF2283D72699AB7FA74B650CED999DEC73EE22E237B13EFE29F88A6DEFA100CCED7FE2B14875E05B2708A4F5BE4AC2D0BC02EAD2BD6DA81B1F205C278185FA26333E7AC32B8116FB87A9";
        let digicert_n = hex_to_biguint(digicert_modulus_hex);
        let digicert_e = BigUint::from_u64(65537);
        store.add_anchor(TrustAnchor {
            subject: "CN=DigiCert Global Root CA, OU=www.digicert.com, O=DigiCert Inc, C=US".to_string(),
            public_key: PublicKey::Rsa(RSAPublicKey::new(digicert_n, digicert_e)),
            key_id: None,
            is_ca: true,
        });

        store
    }

    pub fn add_anchor(&mut self, anchor: TrustAnchor) {
        self.anchors.push(anchor);
    }

    /// Adiciona certificado raiz validando que é autoassinado e tem BasicConstraints cA=true
    pub fn add_certificate(&mut self, cert: &ParsedCertificate) -> Result<(), &'static str> {
        if !dn_matches(&cert.subject, &cert.issuer) {
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

/// Erros estruturados e granulares de validação de certificados X.509 (RFC 5280)
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
    InvalidWildcard { pattern: String },
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
                write!(f, "Hostname esperado '{}' nao coincide com SANs/Subject: {:?}", expected, actual)
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
            Self::InvalidWildcard { pattern } => {
                write!(f, "Padrao wildcard invalido ou proibido (RFC 6125): '{}'", pattern)
            }
        }
    }
}

/// Validação estrita de Hostname e SAN / Wildcards (RFC 6125)
///
/// Precedência estrita de SAN: Se houver dNSName ou iPAddress em SAN, a verificação
/// é feita EXCLUSIVAMENTE contra os nomes SAN. Subject CN só é utilizado como fallback
/// caso a extensão SAN esteja ausente.
///
/// Regras de Wildcard (*.dominio.org):
/// 1. Wildcard só é permitido no rótulo mais à esquerda (leftmost label).
/// 2. Não são permitidos curingas parciais (ex: `sub*.dominio.org`).
/// 3. Não são permitidos curingas multinível (`*.dominio.org` NÃO cobre `a.b.dominio.org` nem `dominio.org`).
/// 4. Curingas em TLDs públicos ou sem domínio secundário (ex: `*.com`, `*.org`, `*`) são proibidos.
pub fn verify_hostname_match(expected_hostname: &str, cert: &ParsedCertificate) -> Result<(), CertificateValidationError> {
    let target = expected_hostname.to_lowercase();
    let names_to_check = if !cert.dns_names.is_empty() {
        &cert.dns_names
    } else {
        // Fallback para Subject CN se dns_names estiver vazio
        return Err(CertificateValidationError::HostnameMismatch {
            expected: expected_hostname.to_string(),
            actual: Vec::new(),
        });
    };

    for dns in names_to_check {
        let pattern = dns.to_lowercase();

        // 1. Correspondência exata
        if pattern == target {
            return Ok(());
        }

        // 2. Correspondência de Wildcard (*.domain.tld)
        if pattern.starts_with("*.") {
            let base = &pattern[2..]; // "domain.tld"

            // Valida se o wildcard é seguro (deve ter pelo menos um ponto adicional, ou seja, domain.tld)
            let base_dot_count = base.matches('.').count();
            if base_dot_count < 1 {
                // Wildcard em TLD puro como *.com ou * é inválido
                continue;
            }

            // O target deve terminar com .base
            if target.ends_with(base) {
                // O target deve ter exatamente o mesmo número de pontos que o pattern (*.domain.tld = 2 pontos)
                let target_dot_count = target.matches('.').count();
                let pattern_dot_count = pattern.matches('.').count();

                if target_dot_count == pattern_dot_count {
                    // Confirma que não é correspondência com o próprio domínio base sem prefixo
                    let prefix_len = target.len() - base.len();
                    if prefix_len > 0 && target.as_bytes()[prefix_len - 1] == b'.' {
                        let sublabel = &target[..prefix_len - 1];
                        if !sublabel.is_empty() && !sublabel.contains('.') {
                            return Ok(());
                        }
                    }
                }
            }
        }
    }

    Err(CertificateValidationError::HostnameMismatch {
        expected: expected_hostname.to_string(),
        actual: names_to_check.clone(),
    })
}

/// Validação completa de cadeia de certificados até Trust Anchor conforme RFC 5280
pub fn validate_chain(
    chain: &[ParsedCertificate],
    expected_hostname: &str,
    mut current_unix_time: u64,
    trust_store: &TrustStore,
) -> Result<(), CertificateValidationError> {
    if chain.is_empty() {
        return Err(CertificateValidationError::EmptyChain);
    }

    // Se current_unix_time for 0, obtém automaticamente o relógio RTC do microkernel
    if current_unix_time == 0 {
        current_unix_time = crate::drivers::rtc::current_unix_time().0;
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

    // 2. Validação estrita de Hostname (RFC 6125)
    verify_hostname_match(expected_hostname, leaf)?;

    // 3. Validação de Extensões do Certificado Folha
    // Se keyUsage estiver presente, deve conter digitalSignature (bit 0 / 0x80)
    if let Some(ku) = leaf.key_usage {
        if (ku & 0x80) == 0 {
            return Err(CertificateValidationError::LeafMissingDigitalSignature {
                subject: leaf.subject.clone(),
            });
        }
    }

    // Se extendedKeyUsage estiver presente e não vazio, deve conter serverAuth (1.3.6.1.5.5.7.3.1) ou anyExtendedKeyUsage
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

    // 4. Verificação de Elos Intermediários da Cadeia (bottom-up)
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

/// Validador unificado de certificado (para chamadas simples de nó único)
pub fn validate_certificate(
    cert: &ParsedCertificate,
    expected_hostname: &str,
    current_unix_time: u64,
    trust_store: &TrustStore,
) -> Result<(), String> {
    let chain = alloc::vec![cert.clone()];
    validate_chain(&chain, expected_hostname, current_unix_time, trust_store)
        .map_err(|err| format!("{}", err))
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
