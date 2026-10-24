//! Suíte de Testes Host para Validação do Cliente DNS (RFC 1035), Cache Local e JinnWall SPI

pub const DNS_PORT: u16 = 53;
pub const DNS_CLIENT_PORT: u16 = 53535;

pub const QTYPE_A: u16 = 1;
pub const QCLASS_IN: u16 = 1;

pub const FLAG_QR_RESPONSE: u16 = 0x8000;
pub const FLAG_TC_TRUNCATED: u16 = 0x0200;
pub const FLAG_RD_DESIRED: u16 = 0x0100;
pub const FLAG_RA_AVAILABLE: u16 = 0x0080;

pub const RCODE_NOERROR: u16 = 0;
pub const RCODE_NXDOMAIN: u16 = 3;
pub const RCODE_SERVER_FAILURE: u16 = 2;

pub const MAX_NAME_LEN: usize = 64;
pub const MAX_LABEL_LEN: usize = 63;
pub const MAX_DOMAIN_LEN: usize = 253;
pub const DNS_CACHE_CAPACITY: usize = 16;
pub const MAX_HOPS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsError {
    BufferTooSmall,
    InvalidName,
    InvalidHeader,
    NotAResponse,
    FormatError,
    ServerFailure,
    NameNotFound,
    NotImplemented,
    Refused,
    TruncatedPacket,
    CorruptedPointer,
    MismatchedTxId,
    MismatchedQuestion,
    NoAnswer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DnsAnswer {
    pub id: u16,
    pub ip: [u8; 4],
    pub ttl_secs: u32,
}

#[derive(Clone, Copy)]
pub struct DnsCacheEntry {
    pub name: [u8; MAX_NAME_LEN],
    pub name_len: u8,
    pub ip: [u8; 4],
    pub expire_tick: u64,
}

pub struct DnsCache {
    pub entries: [Option<DnsCacheEntry>; DNS_CACHE_CAPACITY],
}

impl DnsCache {
    pub const fn new() -> Self {
        Self {
            entries: [None; DNS_CACHE_CAPACITY],
        }
    }

    pub fn init_defaults(&mut self) {
        self.insert("localhost", [127, 0, 0, 1], 86400, 0);
        self.insert("gateway", [10, 0, 2, 2], 86400, 0);
        self.insert("router", [10, 0, 2, 2], 86400, 0);
        self.insert("dns", [10, 0, 2, 3], 86400, 0);
    }

    pub fn lookup(&self, name: &str, current_tick: u64) -> Option<[u8; 4]> {
        let name_bytes = name.as_bytes();
        if name_bytes.is_empty() || name_bytes.len() > MAX_NAME_LEN {
            return None;
        }

        for entry_opt in self.entries.iter() {
            if let Some(entry) = entry_opt {
                if entry.name_len as usize == name_bytes.len() {
                    let mut matched = true;
                    for i in 0..name_bytes.len() {
                        if entry.name[i] != name_bytes[i].to_ascii_lowercase() {
                            matched = false;
                            break;
                        }
                    }
                    if matched {
                        if entry.expire_tick == 0 || current_tick <= entry.expire_tick {
                            return Some(entry.ip);
                        }
                    }
                }
            }
        }
        None
    }

    pub fn insert(&mut self, name: &str, ip: [u8; 4], ttl_secs: u32, current_tick: u64) {
        let name_bytes = name.as_bytes();
        if name_bytes.is_empty() || name_bytes.len() > MAX_NAME_LEN {
            return;
        }

        let clamped_ttl = ttl_secs.max(5).min(86400);
        let expire_tick = if current_tick == 0 {
            0
        } else {
            current_tick.saturating_add((clamped_ttl as u64).saturating_mul(100))
        };

        let mut name_buf = [0u8; MAX_NAME_LEN];
        for i in 0..name_bytes.len() {
            name_buf[i] = name_bytes[i].to_ascii_lowercase();
        }

        for entry_opt in self.entries.iter_mut() {
            if let Some(entry) = entry_opt {
                if entry.name_len as usize == name_bytes.len() {
                    let mut matched = true;
                    for i in 0..name_bytes.len() {
                        if entry.name[i] != name_buf[i] {
                            matched = false;
                            break;
                        }
                    }
                    if matched {
                        entry.ip = ip;
                        entry.expire_tick = expire_tick;
                        return;
                    }
                }
            }
        }

        for entry_opt in self.entries.iter_mut() {
            if entry_opt.is_none() {
                *entry_opt = Some(DnsCacheEntry {
                    name: name_buf,
                    name_len: name_bytes.len() as u8,
                    ip,
                    expire_tick,
                });
                return;
            }
        }

        for entry_opt in self.entries.iter_mut() {
            if let Some(entry) = entry_opt {
                if entry.expire_tick != 0 && current_tick > entry.expire_tick {
                    *entry_opt = Some(DnsCacheEntry {
                        name: name_buf,
                        name_len: name_bytes.len() as u8,
                        ip,
                        expire_tick,
                    });
                    return;
                }
            }
        }

        let mut oldest_idx: Option<usize> = None;
        let mut min_expire = u64::MAX;

        for (idx, entry_opt) in self.entries.iter().enumerate() {
            if let Some(entry) = entry_opt {
                if entry.expire_tick != 0 && entry.expire_tick < min_expire {
                    min_expire = entry.expire_tick;
                    oldest_idx = Some(idx);
                }
            }
        }

        if let Some(target_idx) = oldest_idx {
            self.entries[target_idx] = Some(DnsCacheEntry {
                name: name_buf,
                name_len: name_bytes.len() as u8,
                ip,
                expire_tick,
            });
        }
    }
}

pub fn build_query(name: &str, tx_id: u16, out_buf: &mut [u8]) -> Result<usize, DnsError> {
    if out_buf.len() < 64 {
        return Err(DnsError::BufferTooSmall);
    }
    let name_bytes = name.as_bytes();
    if name_bytes.is_empty() || name_bytes.len() > MAX_DOMAIN_LEN {
        return Err(DnsError::InvalidName);
    }

    if name_bytes[0] == b'.' {
        return Err(DnsError::InvalidName);
    }

    out_buf[0..2].copy_from_slice(&tx_id.to_be_bytes());
    out_buf[2..4].copy_from_slice(&FLAG_RD_DESIRED.to_be_bytes());
    out_buf[4..6].copy_from_slice(&1u16.to_be_bytes());
    out_buf[6..8].copy_from_slice(&0u16.to_be_bytes());
    out_buf[8..10].copy_from_slice(&0u16.to_be_bytes());
    out_buf[10..12].copy_from_slice(&0u16.to_be_bytes());

    let mut offset = 12;
    let mut total_wire_name_len = 0usize;

    let mut label_start = 0;
    let mut i = 0;
    while i <= name_bytes.len() {
        let is_sep = i == name_bytes.len() || name_bytes[i] == b'.';
        if is_sep {
            let label = &name_bytes[label_start..i];
            if label.is_empty() {
                if i == name_bytes.len() && i > 0 {
                    break;
                }
                return Err(DnsError::InvalidName);
            }
            if label.len() > MAX_LABEL_LEN {
                return Err(DnsError::InvalidName);
            }

            for &b in label {
                if !b.is_ascii_alphanumeric() && b != b'-' && b != b'_' {
                    return Err(DnsError::InvalidName);
                }
            }

            total_wire_name_len += 1 + label.len();
            if total_wire_name_len > 255 || offset + 1 + label.len() >= out_buf.len() {
                return Err(DnsError::InvalidName);
            }

            out_buf[offset] = label.len() as u8;
            offset += 1;
            out_buf[offset..(offset + label.len())].copy_from_slice(label);
            offset += label.len();

            label_start = i + 1;
        }
        i += 1;
    }

    if offset >= out_buf.len() {
        return Err(DnsError::BufferTooSmall);
    }
    out_buf[offset] = 0;
    offset += 1;

    if offset + 4 > out_buf.len() || offset + 4 > 512 {
        return Err(DnsError::BufferTooSmall);
    }
    out_buf[offset..(offset + 2)].copy_from_slice(&QTYPE_A.to_be_bytes());
    offset += 2;
    out_buf[offset..(offset + 2)].copy_from_slice(&QCLASS_IN.to_be_bytes());
    offset += 2;

    Ok(offset)
}

pub fn read_name(
    buf: &[u8],
    mut offset: usize,
    out_name: &mut [u8; MAX_NAME_LEN],
) -> Result<(usize, usize), DnsError> {
    let mut out_len = 0usize;
    let mut hops = 0usize;
    let mut original_next_offset: Option<usize> = None;

    while offset < buf.len() {
        let b = buf[offset];
        if b == 0 {
            let next_wire = original_next_offset.unwrap_or(offset + 1);
            return Ok((next_wire, out_len));
        }

        let flag = b & 0xC0;
        if flag == 0xC0 {
            if offset + 1 >= buf.len() {
                return Err(DnsError::TruncatedPacket);
            }
            let ptr_offset = (((b & 0x3F) as usize) << 8) | (buf[offset + 1] as usize);

            if ptr_offset >= offset || ptr_offset < 12 || ptr_offset >= buf.len() {
                return Err(DnsError::CorruptedPointer);
            }

            if original_next_offset.is_none() {
                original_next_offset = Some(offset + 2);
            }

            offset = ptr_offset;
            hops += 1;
            if hops > MAX_HOPS {
                return Err(DnsError::CorruptedPointer);
            }
            continue;
        } else if flag == 0 {
            let label_len = b as usize;
            if label_len > MAX_LABEL_LEN {
                return Err(DnsError::CorruptedPointer);
            }
            offset += 1;
            if offset + label_len > buf.len() {
                return Err(DnsError::TruncatedPacket);
            }

            if out_len > 0 {
                if out_len >= MAX_NAME_LEN {
                    return Err(DnsError::InvalidName);
                }
                out_name[out_len] = b'.';
                out_len += 1;
            }

            if out_len + label_len > MAX_NAME_LEN {
                return Err(DnsError::InvalidName);
            }
            for j in 0..label_len {
                out_name[out_len + j] = buf[offset + j].to_ascii_lowercase();
            }
            out_len += label_len;
            offset += label_len;
        } else {
            return Err(DnsError::CorruptedPointer);
        }
    }

    Err(DnsError::TruncatedPacket)
}

pub fn parse_response(
    buf: &[u8],
    expected_id: Option<u16>,
    expected_name: Option<&str>,
) -> Result<DnsAnswer, DnsError> {
    if buf.len() < 12 {
        return Err(DnsError::TruncatedPacket);
    }

    let id = u16::from_be_bytes([buf[0], buf[1]]);
    if let Some(exp) = expected_id {
        if id != exp {
            return Err(DnsError::MismatchedTxId);
        }
    }

    let flags = u16::from_be_bytes([buf[2], buf[3]]);
    let is_response = (flags & FLAG_QR_RESPONSE) != 0;
    if !is_response {
        return Err(DnsError::NotAResponse);
    }

    if (flags & FLAG_TC_TRUNCATED) != 0 {
        return Err(DnsError::TruncatedPacket);
    }

    let rcode = flags & 0x000F;
    match rcode {
        RCODE_NOERROR => {}
        1 => return Err(DnsError::FormatError),
        RCODE_SERVER_FAILURE => return Err(DnsError::ServerFailure),
        RCODE_NXDOMAIN => return Err(DnsError::NameNotFound),
        4 => return Err(DnsError::NotImplemented),
        5 => return Err(DnsError::Refused),
        _ => return Err(DnsError::ServerFailure),
    }

    let qdcount = u16::from_be_bytes([buf[4], buf[5]]) as usize;
    let ancount = u16::from_be_bytes([buf[6], buf[7]]) as usize;

    if ancount == 0 {
        return Err(DnsError::NoAnswer);
    }

    let mut offset = 12;

    for q_idx in 0..qdcount {
        let mut qname = [0u8; MAX_NAME_LEN];
        let (next_off, qname_len) = read_name(buf, offset, &mut qname)?;
        offset = next_off;

        if offset + 4 > buf.len() {
            return Err(DnsError::TruncatedPacket);
        }

        let _qtype = u16::from_be_bytes([buf[offset], buf[offset + 1]]);
        let _qclass = u16::from_be_bytes([buf[offset + 2], buf[offset + 3]]);
        offset += 4;

        if q_idx == 0 {
            if let Some(exp_name) = expected_name {
                let exp_bytes = exp_name.as_bytes();
                let actual_bytes = &qname[..qname_len];
                let mut matches = exp_bytes.len() == actual_bytes.len();
                if matches {
                    for i in 0..exp_bytes.len() {
                        if exp_bytes[i].to_ascii_lowercase() != actual_bytes[i].to_ascii_lowercase() {
                            matches = false;
                            break;
                        }
                    }
                }
                if !matches {
                    return Err(DnsError::MismatchedQuestion);
                }
            }
        }
    }

    for _ in 0..ancount {
        let mut ans_name = [0u8; MAX_NAME_LEN];
        let (next_off, _) = read_name(buf, offset, &mut ans_name)?;
        offset = next_off;

        if offset + 10 > buf.len() {
            return Err(DnsError::TruncatedPacket);
        }

        let rtype = u16::from_be_bytes([buf[offset], buf[offset + 1]]);
        let rclass = u16::from_be_bytes([buf[offset + 2], buf[offset + 3]]);
        let ttl = u32::from_be_bytes([
            buf[offset + 4],
            buf[offset + 5],
            buf[offset + 6],
            buf[offset + 7],
        ]);
        let rdlength = u16::from_be_bytes([buf[offset + 8], buf[offset + 9]]) as usize;
        offset += 10;

        if offset + rdlength > buf.len() {
            return Err(DnsError::TruncatedPacket);
        }

        if rtype == QTYPE_A && rclass == QCLASS_IN && rdlength == 4 {
            let ip = [
                buf[offset],
                buf[offset + 1],
                buf[offset + 2],
                buf[offset + 3],
            ];
            return Ok(DnsAnswer {
                id,
                ip,
                ttl_secs: ttl.max(5).min(86400),
            });
        }

        offset += rdlength;
    }

    Err(DnsError::NoAnswer)
}

// ─── TESTES UNITÁRIOS E DE INTEGRAÇÃO ───────────────────────────────────────

#[test]
fn test_dns_build_query_rfc1035() {
    let mut buf = [0u8; 512];
    let len = build_query("example.com", 0x4321, &mut buf).expect("Query build failed");
    assert_eq!(len, 29);
    assert_eq!(&buf[0..2], &[0x43, 0x21]); // ID
    assert_eq!(&buf[2..4], &[0x01, 0x00]); // RD=1
    assert_eq!(&buf[4..6], &[0x00, 0x01]); // QDCOUNT=1
    assert_eq!(&buf[6..12], &[0; 6]);     // AN/NS/AR=0
    assert_eq!(buf[12], 7);
    assert_eq!(&buf[13..20], b"example");
    assert_eq!(buf[20], 3);
    assert_eq!(&buf[21..24], b"com");
    assert_eq!(buf[24], 0);                // Terminating zero
    assert_eq!(&buf[25..27], &[0x00, 0x01]); // QTYPE=A
    assert_eq!(&buf[27..29], &[0x00, 0x01]); // QCLASS=IN
}

#[test]
fn test_dns_build_query_rejections() {
    let mut buf = [0u8; 512];
    assert_eq!(build_query("", 0x1111, &mut buf), Err(DnsError::InvalidName));
    assert_eq!(build_query(".example.com", 0x1111, &mut buf), Err(DnsError::InvalidName));
    assert_eq!(build_query("example..com", 0x1111, &mut buf), Err(DnsError::InvalidName));
    let long_label = "a".repeat(64) + ".com";
    assert_eq!(build_query(&long_label, 0x1111, &mut buf), Err(DnsError::InvalidName));
}

#[test]
fn test_dns_parser_uncompressed_a_record() {
    let mut resp = [0u8; 64];
    resp[0..2].copy_from_slice(&0x4321u16.to_be_bytes());
    resp[2..4].copy_from_slice(&(FLAG_QR_RESPONSE | FLAG_RD_DESIRED | FLAG_RA_AVAILABLE).to_be_bytes());
    resp[4..6].copy_from_slice(&1u16.to_be_bytes());
    resp[6..8].copy_from_slice(&1u16.to_be_bytes());

    let mut off = 12;
    resp[off] = 7; off += 1;
    resp[off..off+7].copy_from_slice(b"example"); off += 7;
    resp[off] = 3; off += 1;
    resp[off..off+3].copy_from_slice(b"com"); off += 3;
    resp[off] = 0; off += 1;
    resp[off..off+2].copy_from_slice(&QTYPE_A.to_be_bytes()); off += 2;
    resp[off..off+2].copy_from_slice(&QCLASS_IN.to_be_bytes()); off += 2;

    resp[off] = 7; off += 1;
    resp[off..off+7].copy_from_slice(b"example"); off += 7;
    resp[off] = 3; off += 1;
    resp[off..off+3].copy_from_slice(b"com"); off += 3;
    resp[off] = 0; off += 1;
    resp[off..off+2].copy_from_slice(&QTYPE_A.to_be_bytes()); off += 2;
    resp[off..off+2].copy_from_slice(&QCLASS_IN.to_be_bytes()); off += 2;
    resp[off..off+4].copy_from_slice(&300u32.to_be_bytes()); off += 4;
    resp[off..off+2].copy_from_slice(&4u16.to_be_bytes()); off += 2;
    resp[off..off+4].copy_from_slice(&[93, 184, 216, 34]); off += 4;

    let ans = parse_response(&resp[..off], Some(0x4321), Some("example.com")).expect("Parse failed");
    assert_eq!(ans.id, 0x4321);
    assert_eq!(ans.ip, [93, 184, 216, 34]);
    assert_eq!(ans.ttl_secs, 300);
}

#[test]
fn test_dns_parser_compression_pointer() {
    let mut resp = [0u8; 64];
    resp[0..2].copy_from_slice(&0x9999u16.to_be_bytes());
    resp[2..4].copy_from_slice(&(FLAG_QR_RESPONSE | FLAG_RD_DESIRED).to_be_bytes());
    resp[4..6].copy_from_slice(&1u16.to_be_bytes());
    resp[6..8].copy_from_slice(&1u16.to_be_bytes());

    let mut off = 12;
    resp[off] = 4; off += 1;
    resp[off..off+4].copy_from_slice(b"jinn"); off += 4;
    resp[off] = 3; off += 1;
    resp[off..off+3].copy_from_slice(b"org"); off += 3;
    resp[off] = 0; off += 1;
    resp[off..off+2].copy_from_slice(&QTYPE_A.to_be_bytes()); off += 2;
    resp[off..off+2].copy_from_slice(&QCLASS_IN.to_be_bytes()); off += 2;

    // Answer aponta para 12 (0xC00C)
    resp[off..off+2].copy_from_slice(&[0xC0, 12]); off += 2;
    resp[off..off+2].copy_from_slice(&QTYPE_A.to_be_bytes()); off += 2;
    resp[off..off+2].copy_from_slice(&QCLASS_IN.to_be_bytes()); off += 2;
    resp[off..off+4].copy_from_slice(&1800u32.to_be_bytes()); off += 4;
    resp[off..off+2].copy_from_slice(&4u16.to_be_bytes()); off += 2;
    resp[off..off+4].copy_from_slice(&[142, 250, 190, 46]); off += 4;

    let ans = parse_response(&resp[..off], Some(0x9999), Some("jinn.org")).expect("Compression parse failed");
    assert_eq!(ans.ip, [142, 250, 190, 46]);
    assert_eq!(ans.ttl_secs, 1800);
}

#[test]
fn test_dns_parser_anti_tampering_and_loops() {
    let mut resp = [0u8; 64];
    resp[0..2].copy_from_slice(&0x1111u16.to_be_bytes());
    resp[2..4].copy_from_slice(&(FLAG_QR_RESPONSE).to_be_bytes());
    resp[4..6].copy_from_slice(&1u16.to_be_bytes());
    resp[6..8].copy_from_slice(&1u16.to_be_bytes());

    let mut off = 12;
    // Ponteiro que aponta para si mesmo (loop circular) ou para frente
    resp[off] = 0xC0;
    resp[off + 1] = 12; // loop
    off += 2;
    resp[off..off+4].copy_from_slice(&[0, 1, 0, 1]); off += 4;

    let err = parse_response(&resp[..off], Some(0x1111), None);
    assert_eq!(err, Err(DnsError::CorruptedPointer));
}

#[test]
fn test_dns_parser_mismatched_id_and_question() {
    let mut resp = [0u8; 64];
    resp[0..2].copy_from_slice(&0xAAAAu16.to_be_bytes());
    resp[2..4].copy_from_slice(&(FLAG_QR_RESPONSE).to_be_bytes());
    resp[4..6].copy_from_slice(&1u16.to_be_bytes());
    resp[6..8].copy_from_slice(&1u16.to_be_bytes());

    let mut off = 12;
    resp[off] = 3; off += 1;
    resp[off..off+3].copy_from_slice(b"com"); off += 3;
    resp[off] = 0; off += 1;
    resp[off..off+4].copy_from_slice(&[0, 1, 0, 1]); off += 4;
    resp[off..off+2].copy_from_slice(&[0xC0, 12]); off += 2;
    resp[off..off+4].copy_from_slice(&[0, 1, 0, 1]); off += 4;
    resp[off..off+4].copy_from_slice(&60u32.to_be_bytes()); off += 4;
    resp[off..off+2].copy_from_slice(&4u16.to_be_bytes()); off += 2;
    resp[off..off+4].copy_from_slice(&[8, 8, 8, 8]); off += 4;

    // ID discrepante
    assert_eq!(parse_response(&resp[..off], Some(0xBBBB), Some("com")), Err(DnsError::MismatchedTxId));

    // Pergunta discrepante
    assert_eq!(parse_response(&resp[..off], Some(0xAAAA), Some("net")), Err(DnsError::MismatchedQuestion));
}

#[test]
fn test_dns_cache_ttl_and_eviction() {
    let mut cache = DnsCache::new();
    cache.init_defaults();

    // Default estatico
    assert_eq!(cache.lookup("localhost", 1000000), Some([127, 0, 0, 1]));
    assert_eq!(cache.lookup("gateway", 1000000), Some([10, 0, 2, 2]));

    // Dinamico com expiracao
    cache.insert("my-app.dev", [192, 168, 1, 55], 10, 500); // 10s = 1000 ticks -> expira no tick 1500
    assert_eq!(cache.lookup("MY-APP.DEV", 1499), Some([192, 168, 1, 55]));
    assert_eq!(cache.lookup("my-app.dev", 1501), None); // Expirado!
}

#[test]
fn test_jinnwall_firewall_policies_simulation() {
    #[derive(Debug, PartialEq, Eq)]
    enum Profile { PublicSecure, Development }

    struct MockJinnWall {
        profile: Profile,
        inspected: u64,
        accepted: u64,
        dropped: u64,
        table: Vec<([u8; 4], [u8; 4], u16, u16, u8)>,
    }

    impl MockJinnWall {
        fn filter_in(&mut self, src_ip: [u8; 4], dst_ip: [u8; 4], proto: u8, src_port: u16, dst_port: u16) -> bool {
            self.inspected += 1;
            if src_ip == [0, 0, 0, 0] || src_ip == [255, 255, 255, 255] {
                self.dropped += 1;
                return false; // Anti-spoofing drop
            }
            // Conntrack match
            for conn in &self.table {
                if conn.0 == dst_ip && conn.1 == src_ip && conn.2 == dst_port && conn.3 == src_port && conn.4 == proto {
                    self.accepted += 1;
                    return true;
                }
            }
            if self.profile == Profile::Development && (dst_port == 8080 || dst_port == 3000) {
                self.accepted += 1;
                return true;
            }
            self.dropped += 1;
            false
        }

        fn track_out(&mut self, src_ip: [u8; 4], dst_ip: [u8; 4], proto: u8, src_port: u16, dst_port: u16) {
            self.table.push((src_ip, dst_ip, src_port, dst_port, proto));
        }
    }

    let mut fw = MockJinnWall {
        profile: Profile::PublicSecure,
        inspected: 0,
        accepted: 0,
        dropped: 0,
        table: Vec::new(),
    };

    // PublicSecure drops 8080
    assert!(!fw.filter_in([1, 2, 3, 4], [10, 0, 2, 15], 6, 40000, 8080));

    // Alterna para Development -> aceita 8080 e 3000, rejeita 9999
    fw.profile = Profile::Development;
    assert!(fw.filter_in([1, 2, 3, 4], [10, 0, 2, 15], 6, 40000, 8080));
    assert!(fw.filter_in([1, 2, 3, 4], [10, 0, 2, 15], 6, 40000, 3000));
    assert!(!fw.filter_in([1, 2, 3, 4], [10, 0, 2, 15], 6, 40000, 9999));

    // Em PublicSecure, retorno de DNS enviado e aprovado via Conntrack
    fw.profile = Profile::PublicSecure;
    fw.track_out([10, 0, 2, 15], [10, 0, 2, 3], 17, 53535, 53);
    assert!(fw.filter_in([10, 0, 2, 3], [10, 0, 2, 15], 17, 53, 53535));

    assert_eq!(fw.inspected, fw.accepted + fw.dropped);
}
