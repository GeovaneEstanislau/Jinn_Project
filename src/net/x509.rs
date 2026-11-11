//! Jinn Microkernel — Pure Rust minimal X.509 ASN.1 DER certificate parser (RFC 5280)
//!
//! Implementação no_std com decodificação robusta de estruturas ASN.1 DER,
//! extração de chaves públicas SPKI (RSA 2048/4096, ECDSA P-256, Ed25519),
//! metadados de governança (BasicConstraints, KeyUsage, EKU, SAN) e conferência temporal.

use alloc::{format, string::{String, ToString}, vec::Vec};
use crate::crypto::rsa::{RSAPublicKey, parse_pkcs1_rsa_public_key};
use crate::crypto::p256::{P256PublicKey, parse_der_ecdsa_signature};
use crate::crypto::sha256::Sha256;

/// Algoritmos de assinatura X.509 suportados
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignatureAlgorithm {
    Sha256WithRsa,   // 1.2.840.113549.1.1.11
    RsaPssSha256,    // 1.2.840.113549.1.1.10
    EcdsaWithSha256, // 1.2.840.10045.4.3.2
    Sha384WithRsa,   // 1.2.840.113549.1.1.12
    Sha512WithRsa,   // 1.2.840.113549.1.1.13
    Ed25519,         // 1.3.101.112
    Unknown,
}

/// Chave pública de sujeito ou autoridade certificadora
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublicKey {
    Rsa(RSAPublicKey),
    EcdsaP256(P256PublicKey),
    Ed25519(Vec<u8>),
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
            PublicKey::Ed25519(_) => Err("Verificacao Ed25519 nao suportada nesta fase"),
            PublicKey::Raw(_) => Err("Tipo de chave publica nao suportado para verificacao criptografica"),
        }
    }
}

/// Certificado X.509 decodificado com dados criptográficos e extensões
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCertificate {
    pub version: u8,
    pub serial_number: Vec<u8>,
    pub subject: String,
    pub issuer: String,
    pub not_before: u64,
    pub not_after: u64,
    pub dns_names: Vec<String>,
    pub ip_addresses: Vec<String>,
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

    // Version (opcional [0] EXPLICIT INTEGER)
    let mut version = 1u8; // RFC 5280: v1 é padrão caso omitido
    if tbs.peek_tag() == Some(0xA0) {
        let ver_elem = tbs.read_element()?;
        let mut ver_reader = DerReader::new(ver_elem.data);
        let int_elem = ver_reader.read_element()?;
        if int_elem.tag == 0x02 && !int_elem.data.is_empty() {
            version = match int_elem.data[int_elem.data.len() - 1] {
                0x00 => 1,
                0x01 => 2,
                0x02 => 3,
                v => v.saturating_add(1),
            };
        }
    }

    // Serial Number (INTEGER)
    let serial_elem = tbs.read_element()?;
    if serial_elem.tag != 0x02 {
        return Err("Tag INTEGER (0x02) esperada para Serial Number");
    }
    let serial_number = serial_elem.data.to_vec();

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

    // Extensões X.509 (opcional [3] EXPLICIT SEQUENCE)
    let mut dns_names = Vec::new();
    let mut ip_addresses = Vec::new();
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
                                &mut ip_addresses,
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
        let cn = extract_common_name(&subject_name);
        if !cn.is_empty() {
            dns_names.push(cn);
        }
    }

    // 2. Outer SignatureAlgorithm
    let outer_sig_elem = cert_reader.read_element()?;
    let outer_sig_alg = parse_algorithm_identifier(outer_sig_elem.data);

    // 3. Outer SignatureValue (BIT STRING)
    let sig_val_bytes = cert_reader.read_bit_string()?;

    Ok(ParsedCertificate {
        version,
        serial_number,
        subject: subject_name,
        issuer: issuer_name,
        not_before,
        not_after,
        dns_names,
        ip_addresses,
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

// ============================================================================
// ANALISADOR DER E FUNÇÕES AUXILIARES
// ============================================================================

pub struct DerReader<'a> {
    data: &'a [u8],
    offset: usize,
}

pub struct DerElement<'a> {
    pub tag: u8,
    pub data: &'a [u8],
}

impl<'a> DerReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    pub fn is_empty(&self) -> bool {
        self.offset >= self.data.len()
    }

    pub fn peek_tag(&self) -> Option<u8> {
        self.data.get(self.offset).copied()
    }

    pub fn read_element(&mut self) -> Result<DerElement<'a>, &'static str> {
        let (tag, data, _) = self.read_element_with_raw()?;
        Ok(DerElement { tag, data })
    }

    pub fn read_element_with_raw(&mut self) -> Result<(u8, &'a [u8], &'a [u8]), &'static str> {
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

    pub fn read_sequence(&mut self) -> Result<&'a [u8], &'static str> {
        let elem = self.read_element()?;
        if elem.tag == 0x30 {
            Ok(elem.data)
        } else {
            Err("Tag SEQUENCE (0x30) esperada")
        }
    }

    pub fn read_bit_string(&mut self) -> Result<&'a [u8], &'static str> {
        let elem = self.read_element()?;
        if elem.tag == 0x03 && !elem.data.is_empty() {
            // Primeiro byte do BIT STRING indica o número de bits não usados (0..7)
            Ok(&elem.data[1..])
        } else {
            Err("Tag BIT STRING (0x03) esperada")
        }
    }
}

pub fn parse_algorithm_identifier(data: &[u8]) -> SignatureAlgorithm {
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
            // id-Ed25519: 1.3.101.112 -> [0x2B, 0x65, 0x70]
            if oid_elem.data == [0x2B, 0x65, 0x70] {
                return SignatureAlgorithm::Ed25519;
            }
        }
    }
    SignatureAlgorithm::Unknown
}

pub fn parse_spki(spki_bytes: &[u8]) -> Result<(PublicKey, Vec<u8>), &'static str> {
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

    // id-Ed25519: 1.3.101.112 -> [0x2B, 0x65, 0x70]
    if oid_elem.tag == 0x06 && oid_elem.data == [0x2B, 0x65, 0x70] {
        return Ok((PublicKey::Ed25519(key_bits.to_vec()), key_bits.to_vec()));
    }

    Ok((PublicKey::Raw(key_bits.to_vec()), key_bits.to_vec()))
}

pub fn parse_distinguished_name(dn_data: &[u8]) -> String {
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
                                    [0x55, 0x04, 0x08] => "ST=",
                                    [0x55, 0x04, 0x07] => "L=",
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

pub fn extract_common_name(dn: &str) -> String {
    for part in dn.split(',') {
        let p = part.trim();
        if p.starts_with("CN=") {
            return p[3..].to_string();
        }
    }
    dn.to_string()
}

pub fn dn_matches(a: &str, b: &str) -> bool {
    let clean = |s: &str| -> String {
        s.split(',')
            .map(|p| p.trim().to_lowercase())
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join(",")
    };
    clean(a) == clean(b) || a.to_lowercase() == b.to_lowercase()
}

pub fn parse_validity(validity_data: &[u8]) -> Result<(u64, u64), &'static str> {
    let mut reader = DerReader::new(validity_data);
    let nb_elem = reader.read_element()?;
    let na_elem = reader.read_element()?;

    let nb = parse_time_element(nb_elem.tag, nb_elem.data)?;
    let na = parse_time_element(na_elem.tag, na_elem.data)?;

    Ok((nb, na))
}

pub fn parse_time_element(tag: u8, data: &[u8]) -> Result<u64, &'static str> {
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

        Ok(x509_date_to_unix(year, month, day, hour, min, sec))
    } else if tag == 0x18 { // GeneralizedTime: YYYYMMDDHHMMSSZ
        if s.len() < 14 { return Err("GeneralizedTime muito curto"); }
        let year: u16 = s[0..4].parse().map_err(|_| "Ano GeneralizedTime invalido")?;
        let month: u8 = s[4..6].parse().map_err(|_| "Mes invalido")?;
        let day: u8 = s[6..8].parse().map_err(|_| "Dia invalido")?;
        let hour: u8 = s[8..10].parse().map_err(|_| "Hora invalida")?;
        let min: u8 = s[10..12].parse().map_err(|_| "Minuto invalido")?;
        let sec: u8 = s[12..14].parse().map_err(|_| "Segundo invalido")?;

        Ok(x509_date_to_unix(year, month, day, hour, min, sec))
    } else {
        Err("Tag de formato de tempo nao suportada em X.509")
    }
}

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

fn parse_extension(
    ext_data: &[u8],
    dns_names: &mut Vec<String>,
    ip_addresses: &mut Vec<String>,
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
                                let ip_str = format!("{}.{}.{}.{}", san_elem.data[0], san_elem.data[1], san_elem.data[2], san_elem.data[3]);
                                ip_addresses.push(ip_str.clone());
                                dns_names.push(ip_str);
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
