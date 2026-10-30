//! Jinn OS — Cliente de Resolução DNS e Cache Local (RFC 1035)
//!
//! Fornece resolução de nomes de domínio (IPv4 / Registros A),
//! montagem defensiva de consultas UDP/53, parser RFC 1035 com suporte a
//! descompressão de ponteiros, proteção estrita contra ponteiros circulares
//! e corrompidos, e cache local com gerenciamento de TTL e substituição.

use crate::net::dhcp::DHCP_CLIENT;
use crate::net::ipv4::{Ipv4Packet, PROTO_UDP};
use crate::net::udp::UdpPacket;
use crate::net::ethernet::{EthernetFrame, ETHERTYPE_IPV4};

pub const DNS_PORT: u16 = 53;
pub const DNS_CLIENT_PORT: u16 = 53535;

pub const QTYPE_A: u16 = 1;
pub const QTYPE_AAAA: u16 = 28;
pub const QCLASS_IN: u16 = 1;

pub const FLAG_QR_RESPONSE: u16 = 0x8000;
pub const FLAG_OPCODE_MASK: u16 = 0x7800;
pub const FLAG_TC_TRUNCATED: u16 = 0x0200;
pub const FLAG_RD_DESIRED: u16 = 0x0100;
pub const FLAG_RA_AVAILABLE: u16 = 0x0080;
pub const RCODE_MASK: u16 = 0x000F;

pub const RCODE_NOERROR: u16 = 0;
pub const RCODE_FORMAT_ERROR: u16 = 1;
pub const RCODE_SERVER_FAILURE: u16 = 2;
pub const RCODE_NXDOMAIN: u16 = 3;
pub const RCODE_NOT_IMPLEMENTED: u16 = 4;
pub const RCODE_REFUSED: u16 = 5;

pub const DNS_CACHE_CAPACITY: usize = 16;
pub const MAX_NAME_LEN: usize = 64;
pub const MAX_LABEL_LEN: usize = 63;
pub const MAX_DOMAIN_LEN: usize = 253;
pub const MAX_DNS_PACKET_LEN: usize = 512;
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

    /// Inicializa entradas estáticas conhecidas (hosts locais essenciais do sistema)
    /// Nota: jinn.org NÃO é inserido como endereço local estático arbitrário;
    /// deve ser resolvido via upstream DNS quando consultado.
    pub fn init_defaults(&mut self) {
        self.insert("localhost", [127, 0, 0, 1], 86400, 0);
        self.insert("gateway", [10, 0, 2, 2], 86400, 0);
        self.insert("router", [10, 0, 2, 2], 86400, 0);
        self.insert("dns", [10, 0, 2, 3], 86400, 0);
    }

    /// Consulta cache local respeitando expiração de TTL e normalização de maiúsculas/minúsculas
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
                        if entry.name[i].to_ascii_lowercase() != name_bytes[i].to_ascii_lowercase() {
                            matched = false;
                            break;
                        }
                    }
                    if matched {
                        // Se expire_tick == 0 é entrada estática permanente
                        if entry.expire_tick == 0 || current_tick <= entry.expire_tick {
                            return Some(entry.ip);
                        }
                    }
                }
            }
        }
        None
    }

    /// Insere ou atualiza entrada no cache com TTL (RFC 1035)
    pub fn insert(&mut self, name: &str, ip: [u8; 4], ttl_secs: u32, current_tick: u64) {
        let name_bytes = name.as_bytes();
        if name_bytes.is_empty() || name_bytes.len() > MAX_NAME_LEN {
            return;
        }

        let clamped_ttl = ttl_secs.max(5).min(86400);
        let expire_tick = if current_tick == 0 {
            0 // Permanente
        } else {
            current_tick.saturating_add((clamped_ttl as u64).saturating_mul(100))
        };

        let mut name_buf = [0u8; MAX_NAME_LEN];
        for i in 0..name_bytes.len() {
            name_buf[i] = name_bytes[i].to_ascii_lowercase();
        }

        // 1. Atualiza se já existir (case-insensitive)
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

        // 2. Insere no primeiro slot livre
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

        // 3. Substitui entrada expirada
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

        // 4. Se todas cheias e nenhuma expirada: substitui a entrada dinâmica mais antiga (menor expire_tick)
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

pub static mut DNS_CACHE: DnsCache = DnsCache::new();

/// Estrutura para rastrear consultas DNS em trânsito
#[derive(Clone, Copy)]
pub struct PendingDnsQuery {
    pub active: bool,
    pub tx_id: u16,
    pub name: [u8; MAX_NAME_LEN],
    pub name_len: u8,
    pub start_tick: u64,
}

impl PendingDnsQuery {
    pub const fn empty() -> Self {
        Self {
            active: false,
            tx_id: 0,
            name: [0u8; MAX_NAME_LEN],
            name_len: 0,
            start_tick: 0,
        }
    }
}

pub static mut PENDING_QUERY: Option<PendingDnsQuery> = None;

/// Monta pacote de consulta DNS (RFC 1035 Standard Query, Tipo A, Classe IN)
pub fn build_query(name: &str, tx_id: u16, out_buf: &mut [u8]) -> Result<usize, DnsError> {
    if out_buf.len() < 64 {
        return Err(DnsError::BufferTooSmall);
    }
    let name_bytes = name.as_bytes();
    if name_bytes.is_empty() || name_bytes.len() > MAX_DOMAIN_LEN {
        return Err(DnsError::InvalidName);
    }

    // Não permite ponto inicial ou consecutivo
    if name_bytes[0] == b'.' {
        return Err(DnsError::InvalidName);
    }

    // Cabeçalho DNS (12 bytes)
    out_buf[0..2].copy_from_slice(&tx_id.to_be_bytes());
    out_buf[2..4].copy_from_slice(&FLAG_RD_DESIRED.to_be_bytes()); // RD=1
    out_buf[4..6].copy_from_slice(&1u16.to_be_bytes());            // QDCOUNT=1
    out_buf[6..8].copy_from_slice(&0u16.to_be_bytes());            // ANCOUNT=0
    out_buf[8..10].copy_from_slice(&0u16.to_be_bytes());           // NSCOUNT=0
    out_buf[10..12].copy_from_slice(&0u16.to_be_bytes());          // ARCOUNT=0

    let mut offset = 12;
    let mut total_wire_name_len = 0usize;

    // Codifica labels do nome (ex: "example.com" -> [7, 'e'..'e', 3, 'c','o','m', 0])
    let mut label_start = 0;
    let mut i = 0;
    while i <= name_bytes.len() {
        let is_sep = i == name_bytes.len() || name_bytes[i] == b'.';
        if is_sep {
            let label = &name_bytes[label_start..i];
            if label.is_empty() {
                // Se for o ponto final no fim do nome (ex: "example.com."), permite terminar
                if i == name_bytes.len() && i > 0 {
                    break;
                }
                return Err(DnsError::InvalidName); // Ponto consecutivo "foo..bar"
            }
            if label.len() > MAX_LABEL_LEN {
                return Err(DnsError::InvalidName);
            }

            // Valida caracteres válidos no label (RFC 1035 / RFC 1123)
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

    // Terminador de nome (label nulo)
    if offset >= out_buf.len() {
        return Err(DnsError::BufferTooSmall);
    }
    out_buf[offset] = 0;
    offset += 1;

    // QTYPE (A = 1) e QCLASS (IN = 1)
    if offset + 4 > out_buf.len() || offset + 4 > MAX_DNS_PACKET_LEN {
        return Err(DnsError::BufferTooSmall);
    }
    out_buf[offset..(offset + 2)].copy_from_slice(&QTYPE_A.to_be_bytes());
    offset += 2;
    out_buf[offset..(offset + 2)].copy_from_slice(&QCLASS_IN.to_be_bytes());
    offset += 2;

    Ok(offset)
}

/// Decodifica um nome de domínio a partir de um pacote DNS RFC 1035.
/// Trata labels normais e ponteiros de compressão (0xC000 | offset).
/// Retorna `(novo_offset_no_fluxo_original, bytes_escritos_no_out_name)`.
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
            // Fim do nome
            let next_wire = original_next_offset.unwrap_or(offset + 1);
            return Ok((next_wire, out_len));
        }

        let flag = b & 0xC0;
        if flag == 0xC0 {
            // Ponteiro comprimido (2 bytes)
            if offset + 1 >= buf.len() {
                return Err(DnsError::TruncatedPacket);
            }
            let ptr_offset = (((b & 0x3F) as usize) << 8) | (buf[offset + 1] as usize);

            // Validação estrita de ponteiro RFC 1035:
            // 1. Destino deve ser anterior ao offset atual (evita loops circulares)
            // 2. Destino deve estar após o cabeçalho (>= 12)
            // 3. Destino deve ser menor que o tamanho do buffer
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
            // Label padrão
            let label_len = b as usize;
            if label_len > MAX_LABEL_LEN {
                return Err(DnsError::CorruptedPointer);
            }
            offset += 1;
            if offset + label_len > buf.len() {
                return Err(DnsError::TruncatedPacket);
            }

            // Adiciona separador '.' se não for o primeiro label
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
            // Flags 0x40 e 0x80 são reservadas pelo RFC 1035
            return Err(DnsError::CorruptedPointer);
        }
    }

    Err(DnsError::TruncatedPacket)
}

/// Extrai e valida resposta DNS RFC 1035 (Registro A / IPv4).
/// Valida transaction ID, flag de resposta, truncation, RCODE, pergunta correspondente
/// e tipo/classe/comprimento de registros.
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

    // Truncation check (TC bit)
    if (flags & FLAG_TC_TRUNCATED) != 0 {
        return Err(DnsError::TruncatedPacket);
    }

    let rcode = flags & RCODE_MASK;
    match rcode {
        RCODE_NOERROR => {}
        RCODE_FORMAT_ERROR => return Err(DnsError::FormatError),
        RCODE_SERVER_FAILURE => return Err(DnsError::ServerFailure),
        RCODE_NXDOMAIN => return Err(DnsError::NameNotFound),
        RCODE_NOT_IMPLEMENTED => return Err(DnsError::NotImplemented),
        RCODE_REFUSED => return Err(DnsError::Refused),
        _ => return Err(DnsError::ServerFailure),
    }

    let qdcount = u16::from_be_bytes([buf[4], buf[5]]) as usize;
    let ancount = u16::from_be_bytes([buf[6], buf[7]]) as usize;

    if ancount == 0 {
        return Err(DnsError::NoAnswer);
    }

    let mut offset = 12;

    // 1. Processa seções Question
    for q_idx in 0..qdcount {
        let mut qname = [0u8; MAX_NAME_LEN];
        let (next_off, qname_len) = read_name(buf, offset, &mut qname)?;
        offset = next_off;

        if offset + 4 > buf.len() {
            return Err(DnsError::TruncatedPacket);
        }

        let qtype = u16::from_be_bytes([buf[offset], buf[offset + 1]]);
        let qclass = u16::from_be_bytes([buf[offset + 2], buf[offset + 3]]);
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
            if qtype != QTYPE_A || qclass != QCLASS_IN {
                // Pergunta não era A/IN
            }
        }
    }

    // 2. Inspeciona seções Answer procurando Registro A compatível
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

        // Se for registro A (IPv4), classe IN (1) com 4 bytes de endereço
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

/// Trata datagrama DNS recebido da rede, associa à consulta em trânsito e alimenta o cache local
pub fn handle_dns_reply(payload: &[u8]) {
    unsafe {
        let pending = match PENDING_QUERY {
            Some(ref p) if p.active => *p,
            _ => return, // Ignora resposta não solicitada
        };

        let pending_name_str = match core::str::from_utf8(&pending.name[..pending.name_len as usize]) {
            Ok(s) => s,
            Err(_) => return,
        };

        if let Ok(answer) = parse_response(payload, Some(pending.tx_id), Some(pending_name_str)) {
            let current_tick = crate::timer::ticks();
            DNS_CACHE.insert(pending_name_str, answer.ip, answer.ttl_secs, current_tick);
            PENDING_QUERY = None;
        }
    }
}

/// Transmite uma consulta DNS padrão para o servidor DNS do gateway
pub fn send_query(name: &str, tx_id: u16) -> Result<(), DnsError> {
    let mut dns_payload = [0u8; 512];
    let payload_len = build_query(name, tx_id, &mut dns_payload)?;

    let dev = match crate::drivers::net::virtio::get_device() {
        Some(d) => d,
        None => return Err(DnsError::ServerFailure),
    };

    let our_ip = unsafe { DHCP_CLIENT.ip };
    let dns_server_ip = unsafe { DHCP_CLIENT.dns };
    let our_mac = dev.mac_address();
    let target_mac = crate::net::arp::resolve_mac_for_ip(dns_server_ip);

    let mut frame = [0u8; 1536];
    EthernetFrame::write_header(&mut frame[0..14], target_mac, our_mac, ETHERTYPE_IPV4);
    Ipv4Packet::write_header(
        &mut frame[14..34],
        our_ip,
        dns_server_ip,
        PROTO_UDP,
        8 + payload_len,
    );
    UdpPacket::write_header(
        &mut frame[34..42],
        DNS_CLIENT_PORT,
        DNS_PORT,
        payload_len,
    );
    frame[42..(42 + payload_len)].copy_from_slice(&dns_payload[..payload_len]);

    // Registra fluxo de saída no Firewall JinnWall (Conntrack)
    unsafe {
        crate::net::jinnwall::JINNWALL.track_out(
            our_ip,
            dns_server_ip,
            PROTO_UDP,
            DNS_CLIENT_PORT,
            DNS_PORT,
            crate::timer::ticks(),
        );
    }

    let total_len = 14 + 20 + 8 + payload_len;
    let _ = dev.transmit(&frame[..total_len]);
    Ok(())
}

/// Resolve nome de domínio consultando cache ou transmitindo query
pub fn resolve(name: &str) -> Option<[u8; 4]> {
    let ticks = crate::timer::ticks();

    // 1. Tenta interpretar como literal IPv4 (ex: "10.0.2.2")
    if let Some(ip) = parse_ipv4_literal(name) {
        return Some(ip);
    }

    // 2. Consulta cache local
    if let Some(cached_ip) = unsafe { DNS_CACHE.lookup(name, ticks) } {
        return Some(cached_ip);
    }

    // 3. Registra consulta em trânsito
    let name_bytes = name.as_bytes();
    if name_bytes.is_empty() || name_bytes.len() > MAX_NAME_LEN {
        return None;
    }
    let mut name_buf = [0u8; MAX_NAME_LEN];
    name_buf[..name_bytes.len()].copy_from_slice(name_bytes);

    let tx_id = (ticks as u16) ^ 0xA55A;
    unsafe {
        PENDING_QUERY = Some(PendingDnsQuery {
            active: true,
            tx_id,
            name: name_buf,
            name_len: name_bytes.len() as u8,
            start_tick: ticks,
        });
    }

    // 4. Transmite consulta pela rede
    if send_query(name, tx_id).is_ok() {
        let start_tick = crate::timer::ticks();
        let mut last_tx = start_tick;
        // Aguarda ate 400 ticks (4,0s a 100 Hz APIC) com retransmissao a cada 80 ticks (800ms)
        while crate::timer::ticks().saturating_sub(start_tick) < 400 {
            crate::net::poll();
            if let Some(ip) = unsafe { DNS_CACHE.lookup(name, crate::timer::ticks()) } {
                unsafe { PENDING_QUERY = None; }
                return Some(ip);
            }
            if crate::timer::ticks().saturating_sub(last_tx) >= 80 {
                last_tx = crate::timer::ticks();
                let _ = send_query(name, tx_id);
            }
            crate::scheduler::yield_now();
        }
    }

    unsafe { PENDING_QUERY = None; }
    None
}

/// Helper para converter literais IPv4 ("A.B.C.D") sem alocador dinâmico
pub fn parse_ipv4_literal(s: &str) -> Option<[u8; 4]> {
    let mut parts = [0u8; 4];
    let mut part_idx = 0;
    let mut current_val: u16 = 0;
    let mut has_digits = false;

    for &b in s.as_bytes() {
        if b == b'.' {
            if !has_digits || current_val > 255 || part_idx >= 3 {
                return None;
            }
            parts[part_idx] = current_val as u8;
            part_idx += 1;
            current_val = 0;
            has_digits = false;
        } else if b.is_ascii_digit() {
            current_val = current_val.saturating_mul(10).saturating_add((b - b'0') as u16);
            has_digits = true;
        } else {
            return None;
        }
    }

    if !has_digits || current_val > 255 || part_idx != 3 {
        return None;
    }
    parts[3] = current_val as u8;
    Some(parts)
}

// ─── SUÍTE DE AUTO-TESTES DO DNS (EXECUÇÃO NO BOOT DO KERNEL) ────────────────

/// Executa suíte de auto-testes de validação da RFC 1035 e Cache DNS.
/// Retorna `(aprovados, total)`.
pub fn run_dns_selftests() -> (usize, usize) {
    let mut passed = 0;
    let total = 12;

    // 1. Construção de query RFC 1035 válida
    let mut qbuf = [0u8; 512];
    let qlen = build_query("example.com", 0x1234, &mut qbuf).unwrap_or(0);
    if qlen == 29 && qbuf[0..2] == [0x12, 0x34] && qbuf[2..4] == [0x01, 0x00] && qbuf[12] == 7 {
        passed += 1;
    }

    // 2. Rejeição de nomes inválidos
    let err_empty = build_query("", 0x1111, &mut qbuf);
    let err_leading = build_query(".example.com", 0x1111, &mut qbuf);
    let err_dotdot = build_query("example..com", 0x1111, &mut qbuf);
    if err_empty.is_err() && err_leading.is_err() && err_dotdot.is_err() {
        passed += 1;
    }

    // 3. Rejeição de labels > 63 bytes
    let long_label = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz01234567890123.com"; // 64 chars
    if build_query(long_label, 0x1111, &mut qbuf).is_err() {
        passed += 1;
    }

    // 4. Parser RFC 1035 com resposta válida sem compressão
    // ID=0x1234, QR=1, RD=1, RA=1, QDCOUNT=1, ANCOUNT=1
    let mut resp = [0u8; 64];
    resp[0..2].copy_from_slice(&0x1234u16.to_be_bytes());
    resp[2..4].copy_from_slice(&(FLAG_QR_RESPONSE | FLAG_RD_DESIRED | FLAG_RA_AVAILABLE).to_be_bytes());
    resp[4..6].copy_from_slice(&1u16.to_be_bytes()); // QDCOUNT=1
    resp[6..8].copy_from_slice(&1u16.to_be_bytes()); // ANCOUNT=1
    // Question: [7, 'e','x','a','m','p','l','e', 3, 'c','o','m', 0], QTYPE=1, QCLASS=1
    let mut off = 12;
    resp[off] = 7; off += 1;
    resp[off..off+7].copy_from_slice(b"example"); off += 7;
    resp[off] = 3; off += 1;
    resp[off..off+3].copy_from_slice(b"com"); off += 3;
    resp[off] = 0; off += 1;
    resp[off..off+2].copy_from_slice(&QTYPE_A.to_be_bytes()); off += 2;
    resp[off..off+2].copy_from_slice(&QCLASS_IN.to_be_bytes()); off += 2;
    // Answer: name=[7, "example", 3, "com", 0], TYPE=1, CLASS=1, TTL=300, RDLEN=4, IP=93.184.216.34
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

    let ans = parse_response(&resp[..off], Some(0x1234), Some("example.com"));
    if let Ok(a) = ans {
        if a.ip == [93, 184, 216, 34] && a.ttl_secs == 300 {
            passed += 1;
        }
    }

    // 5. Parser RFC 1035 com resposta usando ponteiro de compressão (0xC00C)
    let mut comp_resp = [0u8; 64];
    comp_resp[0..2].copy_from_slice(&0x5678u16.to_be_bytes());
    comp_resp[2..4].copy_from_slice(&(FLAG_QR_RESPONSE | FLAG_RD_DESIRED).to_be_bytes());
    comp_resp[4..6].copy_from_slice(&1u16.to_be_bytes());
    comp_resp[6..8].copy_from_slice(&1u16.to_be_bytes());
    let mut c_off = 12;
    comp_resp[c_off] = 3; c_off += 1;
    comp_resp[c_off..c_off+3].copy_from_slice(b"org"); c_off += 3;
    comp_resp[c_off] = 0; c_off += 1;
    comp_resp[c_off..c_off+2].copy_from_slice(&QTYPE_A.to_be_bytes()); c_off += 2;
    comp_resp[c_off..c_off+2].copy_from_slice(&QCLASS_IN.to_be_bytes()); c_off += 2;
    // Answer usa ponteiro 0xC00C (offset 12)
    comp_resp[c_off..c_off+2].copy_from_slice(&[0xC0, 12]); c_off += 2;
    comp_resp[c_off..c_off+2].copy_from_slice(&QTYPE_A.to_be_bytes()); c_off += 2;
    comp_resp[c_off..c_off+2].copy_from_slice(&QCLASS_IN.to_be_bytes()); c_off += 2;
    comp_resp[c_off..c_off+4].copy_from_slice(&600u32.to_be_bytes()); c_off += 4;
    comp_resp[c_off..c_off+2].copy_from_slice(&4u16.to_be_bytes()); c_off += 2;
    comp_resp[c_off..c_off+4].copy_from_slice(&[1, 2, 3, 4]); c_off += 4;

    let ans_comp = parse_response(&comp_resp[..c_off], Some(0x5678), Some("org"));
    if let Ok(a) = ans_comp {
        if a.ip == [1, 2, 3, 4] {
            passed += 1;
        }
    }

    // 6. Detecção e rejeição de ponteiro circular ou destino inválido
    let mut bad_ptr_resp = comp_resp;
    bad_ptr_resp[12 + 3 + 1 + 4] = 0xC0;
    bad_ptr_resp[12 + 3 + 1 + 5] = 20; // Aponta para frente (offset 20 >= offset atual)
    if parse_response(&bad_ptr_resp[..c_off], Some(0x5678), Some("org")).is_err() {
        passed += 1;
    }

    // 7. Rejeição de erro upstream (RCODE = NXDOMAIN)
    let mut nx_resp = comp_resp;
    nx_resp[3] = RCODE_NXDOMAIN as u8;
    if let Err(DnsError::NameNotFound) = parse_response(&nx_resp[..c_off], Some(0x5678), Some("org")) {
        passed += 1;
    }

    // 8. Detecção de pacote truncado (TC=1)
    let mut tc_resp = comp_resp;
    tc_resp[2] |= 0x02; // Bit TC
    if let Err(DnsError::TruncatedPacket) = parse_response(&tc_resp[..c_off], Some(0x5678), Some("org")) {
        passed += 1;
    }

    // 9. Rejeição de discrepância de Transaction ID e Pergunta
    let mismatch_id = parse_response(&comp_resp[..c_off], Some(0x9999), Some("org"));
    let mismatch_q = parse_response(&comp_resp[..c_off], Some(0x5678), Some("different.com"));
    if let (Err(DnsError::MismatchedTxId), Err(DnsError::MismatchedQuestion)) = (mismatch_id, mismatch_q) {
        passed += 1;
    }

    // 10. Cache DNS: inserção, lookup e normalização case-insensitive
    let mut cache = DnsCache::new();
    cache.insert("MyHost.Domain.COM", [192, 168, 1, 100], 60, 1000);
    let hit_upper = cache.lookup("MYHOST.DOMAIN.COM", 1000);
    let hit_lower = cache.lookup("myhost.domain.com", 1000);
    if hit_upper == Some([192, 168, 1, 100]) && hit_lower == Some([192, 168, 1, 100]) {
        passed += 1;
    }

    // 11. Cache DNS: expiração exata de TTL
    // Inserido no tick 1000 com TTL 10s (1000 ticks) -> expira no tick 2000
    cache.insert("expiring.host", [10, 10, 10, 10], 10, 1000);
    let valid_before = cache.lookup("expiring.host", 1999);
    let expired_after = cache.lookup("expiring.host", 2001);
    if valid_before == Some([10, 10, 10, 10]) && expired_after.is_none() {
        passed += 1;
    }

    // 12. Cache DNS: substituição de entrada expirada em capacidade total
    let mut full_cache = DnsCache::new();
    // Preenche 16 posições com TTL expirado
    for i in 0..DNS_CACHE_CAPACITY {
        let name = [b'a' + (i as u8), b'.', b'c', b'o', b'm'];
        let n_str = core::str::from_utf8(&name).unwrap();
        full_cache.insert(n_str, [10, 0, 0, i as u8], 5, 100);
    }
    // No tick 10000, todas as 16 entradas expiraram. Insere nova entrada:
    full_cache.insert("newhost.net", [172, 16, 0, 1], 300, 10000);
    if full_cache.lookup("newhost.net", 10000) == Some([172, 16, 0, 1]) {
        passed += 1;
    }

    (passed, total)
}
