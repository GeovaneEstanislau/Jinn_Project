//! Pure Rust minimal X.509 ASN.1 DER certificate parser and validator (RFC 5280)

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::{string::{String, ToString}, vec::Vec, format};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCertificate {
    pub subject: String,
    pub issuer: String,
    pub not_before: u64,
    pub not_after: u64,
    pub dns_names: Vec<String>,
    pub public_key: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct TrustStore {
    trusted_roots: Vec<String>,
}

impl TrustStore {
    pub fn new() -> Self {
        let mut store = Self {
            trusted_roots: Vec::new(),
        };
        // Carrega CAs Raiz canônicas suportadas
        store.add_root("ISRG Root X1");
        store.add_root("DigiCert Global Root G2");
        store.add_root("GlobalSign Root CA");
        store.add_root("Jinn OS Root CA");
        store
    }

    pub fn add_root(&mut self, root_common_name: &str) {
        self.trusted_roots.push(root_common_name.to_string());
    }

    pub fn is_trusted(&self, cert: &ParsedCertificate) -> bool {
        for root in &self.trusted_roots {
            if cert.issuer.contains(root) || cert.subject.contains(root) {
                return true;
            }
        }
        false
    }
}

pub fn parse_der_certificate(der: &[u8]) -> Result<ParsedCertificate, &'static str> {
    let mut reader = DerReader::new(der);
    let cert_seq = reader.read_sequence()?;
    let mut cert_reader = DerReader::new(cert_seq);

    // 1. TBSCertificate
    let tbs_bytes = cert_reader.read_sequence()?;
    let mut tbs = DerReader::new(tbs_bytes);

    // Version (optional [0])
    if tbs.peek_tag() == Some(0xA0) {
        let _ = tbs.read_element()?;
    }

    // Serial Number
    let _serial = tbs.read_element()?;

    // Signature Algorithm
    let _sig_alg = tbs.read_element()?;

    // Issuer
    let issuer_bytes = tbs.read_sequence()?;
    let issuer_name = parse_distinguished_name(issuer_bytes);

    // Validity
    let validity_bytes = tbs.read_sequence()?;
    let (not_before, not_after) = parse_validity(validity_bytes)?;

    // Subject
    let subject_bytes = tbs.read_sequence()?;
    let subject_name = parse_distinguished_name(subject_bytes);

    // SubjectPublicKeyInfo
    let spki_bytes = tbs.read_sequence()?;
    let mut spki = DerReader::new(spki_bytes);
    let _spki_alg = spki.read_sequence()?;
    let pubkey_bytes = spki.read_bit_string()?;

    // Extensions (optional [3])
    let mut dns_names = Vec::new();
    while !tbs.is_empty() {
        if let Ok(elem) = tbs.read_element() {
            if elem.tag == 0xA3 {
                // Extensions sequence
                if let Ok(seq) = DerReader::new(elem.data).read_sequence() {
                    let mut ext_reader = DerReader::new(seq);
                    while !ext_reader.is_empty() {
                        if let Ok(ext) = ext_reader.read_sequence() {
                            parse_extension(ext, &mut dns_names);
                        }
                    }
                }
            }
        }
    }

    // Se SAN não estiver presente, utiliza o Subject CN como fallback
    if dns_names.is_empty() && !subject_name.is_empty() {
        dns_names.push(subject_name.clone());
    }

    Ok(ParsedCertificate {
        subject: subject_name,
        issuer: issuer_name,
        not_before,
        not_after,
        dns_names,
        public_key: pubkey_bytes.to_vec(),
    })
}

pub fn validate_certificate(
    cert: &ParsedCertificate,
    expected_hostname: &str,
    current_unix_time: u64,
    trust_store: &TrustStore,
) -> Result<(), String> {
    // 1. Validação temporal
    if current_unix_time > 0 {
        if current_unix_time < cert.not_before {
            return Err(format!(
                "Certificado ainda nao e valido (notBefore={} > now={})",
                cert.not_before, current_unix_time
            ));
        }
        if current_unix_time > cert.not_after {
            return Err(format!(
                "Certificado expirado (notAfter={} < now={})",
                cert.not_after, current_unix_time
            ));
        }
    }

    // 2. Validação de Hostname (SAN / CN)
    let mut hostname_matched = false;
    let target = expected_hostname.to_lowercase();

    for dns in &cert.dns_names {
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
        return Err(format!(
            "Hostname '{}' nao coincide com SAN do certificado: {:?}",
            expected_hostname, cert.dns_names
        ));
    }

    // 3. Validação de Cadeia de Confiança
    if !trust_store.is_trusted(cert) {
        return Err(format!(
            "Emissor do certificado '{}' nao reconhecido no trust store local",
            cert.issuer
        ));
    }

    Ok(())
}

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
        if self.offset >= self.data.len() {
            return Err("Unexpected EOF in DER");
        }
        let tag = self.data[self.offset];
        self.offset += 1;

        if self.offset >= self.data.len() {
            return Err("Unexpected EOF reading length");
        }
        let first_len = self.data[self.offset];
        self.offset += 1;

        let len = if first_len & 0x80 == 0 {
            first_len as usize
        } else {
            let num_octets = (first_len & 0x7F) as usize;
            if num_octets > 4 || self.offset + num_octets > self.data.len() {
                return Err("Invalid DER length octets");
            }
            let mut l = 0usize;
            for _ in 0..num_octets {
                l = (l << 8) | (self.data[self.offset] as usize);
                self.offset += 1;
            }
            l
        };

        if self.offset + len > self.data.len() {
            return Err("DER element length exceeds buffer");
        }

        let elem_data = &self.data[self.offset..self.offset + len];
        self.offset += len;

        Ok(DerElement { tag, data: elem_data })
    }

    fn read_sequence(&mut self) -> Result<&'a [u8], &'static str> {
        let elem = self.read_element()?;
        if elem.tag == 0x30 {
            Ok(elem.data)
        } else {
            Err("Expected SEQUENCE tag (0x30)")
        }
    }

    fn read_bit_string(&mut self) -> Result<&'a [u8], &'static str> {
        let elem = self.read_element()?;
        if elem.tag == 0x03 && !elem.data.is_empty() {
            // First byte of BIT STRING is unused bits count
            Ok(&elem.data[1..])
        } else {
            Err("Expected BIT STRING tag (0x03)")
        }
    }
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
                    let _oid = atv.read_element();
                    if let Ok(val) = atv.read_element() {
                        if let Ok(s) = core::str::from_utf8(val.data) {
                            name_parts.push(s.to_string());
                        }
                    }
                }
            }
        }
    }

    name_parts.join(", ")
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
    let s = core::str::from_utf8(data).map_err(|_| "Invalid UTF-8 in time")?;
    if tag == 0x17 { // UTCTime: YYMMDDHHMMSSZ
        if s.len() < 12 { return Err("UTCTime too short"); }
        let year_val: u16 = s[0..2].parse().map_err(|_| "Bad year")?;
        let year = if year_val >= 50 { 1900 + year_val } else { 2000 + year_val };
        let month: u8 = s[2..4].parse().map_err(|_| "Bad month")?;
        let day: u8 = s[4..6].parse().map_err(|_| "Bad day")?;
        let hour: u8 = s[6..8].parse().map_err(|_| "Bad hour")?;
        let min: u8 = s[8..10].parse().map_err(|_| "Bad min")?;
        let sec: u8 = s[10..12].parse().map_err(|_| "Bad sec")?;

        Ok(crate::x509_date_to_unix(year, month, day, hour, min, sec))
    } else if tag == 0x18 { // GeneralizedTime: YYYYMMDDHHMMSSZ
        if s.len() < 14 { return Err("GeneralizedTime too short"); }
        let year: u16 = s[0..4].parse().map_err(|_| "Bad year")?;
        let month: u8 = s[4..6].parse().map_err(|_| "Bad month")?;
        let day: u8 = s[6..8].parse().map_err(|_| "Bad day")?;
        let hour: u8 = s[8..10].parse().map_err(|_| "Bad hour")?;
        let min: u8 = s[10..12].parse().map_err(|_| "Bad min")?;
        let sec: u8 = s[12..14].parse().map_err(|_| "Bad sec")?;

        Ok(crate::x509_date_to_unix(year, month, day, hour, min, sec))
    } else {
        Err("Unsupported time tag in X.509")
    }
}

fn parse_extension(ext_data: &[u8], dns_names: &mut Vec<String>) {
    let mut reader = DerReader::new(ext_data);
    if let Ok(oid_elem) = reader.read_element() {
        // SAN OID: 2.5.29.17 -> [0x55, 0x1D, 0x11]
        let is_san = oid_elem.tag == 0x06 && oid_elem.data == [0x55, 0x1D, 0x11];
        if is_san {
            // Read until octet string containing GeneralNames
            while !reader.is_empty() {
                if let Ok(elem) = reader.read_element() {
                    if elem.tag == 0x04 { // OCTET STRING
                        if let Ok(seq) = DerReader::new(elem.data).read_sequence() {
                            let mut san_reader = DerReader::new(seq);
                            while !san_reader.is_empty() {
                                if let Ok(san_elem) = san_reader.read_element() {
                                    if san_elem.tag == 0x82 { // context [2] dNSName
                                        if let Ok(s) = core::str::from_utf8(san_elem.data) {
                                            dns_names.push(s.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

