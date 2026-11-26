//! Jinn OS — Subsistema de Resolução DNS em 3 Camadas (RFC 1035)
//!
//! Camada 1: Tabela Estática Local (`/disk/hosts` + entradas essenciais de sistema).
//! Camada 2: Cache Dinâmico LRU em `.bss` alinhado a 64 bytes com TTL preguiçoso (*Zero-Idle*)
//!           e persistência transacional/epoch-aware em FAT32 (`/disk/dns_cache.db`).
//! Camada 3: Cascata Upstream Resiliente (`primary -> secondary -> fallback`) com *Stale-If-Error*.

use core::sync::atomic::{AtomicBool, Ordering};
use crate::memory::IrqSafeSpinLock;
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

pub const DNS_HOSTS_CAPACITY: usize = 32;
pub const DNS_CACHE_CAPACITY: usize = 128;
pub const MAX_NAME_LEN: usize = 128;
pub const MAX_LABEL_LEN: usize = 63;
pub const MAX_DOMAIN_LEN: usize = 253;
pub const MAX_DNS_PACKET_LEN: usize = 512;
pub const MAX_HOPS: usize = 16;

pub const MIN_DNS_TTL_SECS: u32 = 60;
pub const MAX_DNS_TTL_SECS: u32 = 86400;

const HOSTS_PATH: &str = "/disk/hosts";
const CACHE_DB_PATH: &str = "/disk/dns_cache.db";
const NET_CFG_PATH: &str = "/disk/net.cfg";

const CACHE_DB_MAGIC: [u8; 8] = *b"JDNSC1\0\0";
const CACHE_DB_VERSION: u32 = 1;
const CACHE_RECORD_WIRE_SIZE: usize = 1 + MAX_NAME_LEN + 4 + 4 + 8 + 4 + 4; // 153 bytes
const CACHE_DB_MAX_BYTES: usize = 16 + (DNS_CACHE_CAPACITY * CACHE_RECORD_WIRE_SIZE) + 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsResolveError {
    InterfaceDown,
    ArpFailed,
    Timeout,
    NxDomain,
    ServerFailure,
    InvalidName,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsQueryStatus {
    Pending,
    Resolved([u8; 4], u32, [u8; 4]),
    NxDomain,
    ServerError,
}

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

/// Entrada da Camada 1: Mapeamento estático local (`/disk/hosts` e sistema).
#[derive(Clone, Copy)]
pub struct StaticHostEntry {
    pub domain_hash: u64,
    pub hostname: [u8; MAX_NAME_LEN],
    pub ipv4: [u8; 4],
    pub hits: u32,
    pub hostname_len: u8,
    pub valid: bool,
    pub builtin: bool,
}

impl StaticHostEntry {
    pub const fn empty() -> Self {
        Self {
            domain_hash: 0,
            hostname: [0u8; MAX_NAME_LEN],
            ipv4: [0u8; 4],
            hits: 0,
            hostname_len: 0,
            valid: false,
            builtin: false,
        }
    }
}

/// Entrada da Camada 2: Registro A dinâmico no cache DNS LRU com alinhamento a cacheline.
#[repr(C, align(64))]
#[derive(Clone, Copy)]
pub struct DnsCacheEntry {
    pub domain_hash: u64,
    pub hostname: [u8; MAX_NAME_LEN],
    pub ipv4: [u8; 4],
    pub expires_at_tick: u64,
    pub expires_at_unix: u64,
    pub last_used_tick: u64,
    pub ttl_secs: u32,
    pub hits: u32,
    pub source_upstream: [u8; 4],
    pub hostname_len: u8,
    pub valid: bool,
}

impl DnsCacheEntry {
    pub const fn empty() -> Self {
        Self {
            domain_hash: 0,
            hostname: [0u8; MAX_NAME_LEN],
            ipv4: [0u8; 4],
            expires_at_tick: 0,
            expires_at_unix: 0,
            last_used_tick: 0,
            ttl_secs: 0,
            hits: 0,
            source_upstream: [0u8; 4],
            hostname_len: 0,
            valid: false,
        }
    }
}

/// Tabela de cache dinâmico DNS em `.bss` estático com capacidade para 128 registros A.
#[repr(align(64))]
pub struct DnsCacheTable {
    pub entries: [DnsCacheEntry; DNS_CACHE_CAPACITY],
}

impl DnsCacheTable {
    pub const fn new() -> Self {
        Self {
            entries: [DnsCacheEntry::empty(); DNS_CACHE_CAPACITY],
        }
    }

    pub fn clear(&mut self) -> usize {
        let mut cleared = 0usize;
        for entry in self.entries.iter_mut() {
            if entry.valid {
                *entry = DnsCacheEntry::empty();
                cleared += 1;
            }
        }
        cleared
    }

    /// Consulta cache dinâmico com validação preguiçosa de TTL (*Zero-Idle*):
    /// nenhuma tarefa de varredura roda em segundo plano; `current_tick < expires_at_tick`
    /// é avaliado apenas no momento da consulta.
    pub fn lookup(&mut self, name: &str, current_tick: u64) -> Option<[u8; 4]> {
        let (norm, len, hash) = match normalize_hostname(name) {
            Some(v) => v,
            None => return None,
        };

        for entry in self.entries.iter_mut() {
            if !entry.valid || entry.domain_hash != hash || entry.hostname_len as usize != len {
                continue;
            }
            if entry.hostname[..len] == norm[..len] {
                if entry.expires_at_tick == 0 || current_tick < entry.expires_at_tick {
                    entry.hits = entry.hits.saturating_add(1);
                    entry.last_used_tick = current_tick;
                    return Some(entry.ipv4);
                }
                return None;
            }
        }
        None
    }

    /// Fallback *Stale-If-Error*: retorna o IP em cache expirado se todos os upstreams falharem.
    pub fn lookup_stale(&mut self, name: &str) -> Option<[u8; 4]> {
        let (norm, len, hash) = match normalize_hostname(name) {
            Some(v) => v,
            None => return None,
        };

        for entry in self.entries.iter_mut() {
            if !entry.valid || entry.domain_hash != hash || entry.hostname_len as usize != len {
                continue;
            }
            if entry.hostname[..len] == norm[..len] {
                entry.hits = entry.hits.saturating_add(1);
                return Some(entry.ipv4);
            }
        }
        None
    }

    /// Insere ou atualiza entrada no cache com TTL limitado entre 60s e 86400s e política LRU.
    pub fn insert_with_metadata(
        &mut self,
        name: &str,
        ip: [u8; 4],
        ttl_secs: u32,
        source_upstream: [u8; 4],
        current_tick: u64,
        now_unix: u64,
        initial_hits: u32,
    ) {
        let (norm, len, hash) = match normalize_hostname(name) {
            Some(v) => v,
            None => return,
        };

        let clamped_ttl = ttl_secs.max(MIN_DNS_TTL_SECS).min(MAX_DNS_TTL_SECS);
        let expires_at_tick = if current_tick == 0 {
            0
        } else {
            current_tick.saturating_add((clamped_ttl as u64).saturating_mul(100))
        };
        let expires_at_unix = if now_unix == 0 {
            0
        } else {
            now_unix.saturating_add(clamped_ttl as u64)
        };

        // 1. Atualiza registro existente caso já presente
        for entry in self.entries.iter_mut() {
            if entry.valid && entry.domain_hash == hash && entry.hostname_len as usize == len {
                if entry.hostname[..len] == norm[..len] {
                    entry.ipv4 = ip;
                    entry.expires_at_tick = expires_at_tick;
                    entry.expires_at_unix = expires_at_unix;
                    entry.last_used_tick = current_tick;
                    entry.ttl_secs = clamped_ttl;
                    entry.hits = initial_hits;
                    entry.source_upstream = source_upstream;
                    return;
                }
            }
        }

        let new_entry = DnsCacheEntry {
            domain_hash: hash,
            hostname: norm,
            ipv4: ip,
            expires_at_tick,
            expires_at_unix,
            last_used_tick: current_tick,
            ttl_secs: clamped_ttl,
            hits: initial_hits,
            source_upstream,
            hostname_len: len as u8,
            valid: true,
        };

        // 2. Primeiro slot livre
        for entry in self.entries.iter_mut() {
            if !entry.valid {
                *entry = new_entry;
                return;
            }
        }

        // 3. Reutiliza slot expirado mais antigo (expiração preguiçosa)
        let mut expired_idx: Option<usize> = None;
        let mut oldest_exp_used = u64::MAX;
        for (idx, entry) in self.entries.iter().enumerate() {
            if entry.valid && entry.expires_at_tick != 0 && current_tick >= entry.expires_at_tick {
                if entry.last_used_tick < oldest_exp_used {
                    oldest_exp_used = entry.last_used_tick;
                    expired_idx = Some(idx);
                }
            }
        }
        if let Some(idx) = expired_idx {
            self.entries[idx] = new_entry;
            return;
        }

        // 4. Despejo LRU real (menor last_used_tick)
        let mut lru_idx = 0usize;
        let mut min_used = u64::MAX;
        for (idx, entry) in self.entries.iter().enumerate() {
            if entry.last_used_tick < min_used {
                min_used = entry.last_used_tick;
                lru_idx = idx;
            }
        }
        self.entries[lru_idx] = new_entry;
    }

    pub fn insert(
        &mut self,
        name: &str,
        ip: [u8; 4],
        ttl_secs: u32,
        source_upstream: [u8; 4],
        current_tick: u64,
        now_unix: u64,
    ) {
        self.insert_with_metadata(name, ip, ttl_secs, source_upstream, current_tick, now_unix, 0);
    }
}

/// Configuração de upstreams DNS cascata (`primary -> secondary -> fallback`).
#[derive(Debug, Clone, Copy)]
pub struct DnsConfig {
    pub primary: [u8; 4],
    pub secondary: [u8; 4],
    pub fallback: [u8; 4],
}

impl DnsConfig {
    pub const fn default_cfg() -> Self {
        Self {
            primary: [1, 1, 1, 1],
            secondary: [8, 8, 8, 8],
            fallback: [10, 0, 2, 3],
        }
    }
}

/// Estrutura pública de snapshot para Syscall 93 e UI Aladdin.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DnsEntryUser {
    pub hostname: [u8; MAX_NAME_LEN],
    pub hostname_len: u32,
    pub ipv4: [u8; 4],
    pub source_upstream: [u8; 4],
    pub ttl_remaining_secs: u32,
    pub hits: u32,
    pub is_static_host: u32,
    pub is_stale: u32,
}

impl Default for DnsEntryUser {
    fn default() -> Self {
        Self {
            hostname: [0u8; MAX_NAME_LEN],
            hostname_len: 0,
            ipv4: [0u8; 4],
            source_upstream: [0u8; 4],
            ttl_remaining_secs: 0,
            hits: 0,
            is_static_host: 0,
            is_stale: 0,
        }
    }
}

struct DnsSystemState {
    hosts: [StaticHostEntry; DNS_HOSTS_CAPACITY],
    cache: DnsCacheTable,
    config: DnsConfig,
}

impl DnsSystemState {
    const fn new() -> Self {
        Self {
            hosts: [StaticHostEntry::empty(); DNS_HOSTS_CAPACITY],
            cache: DnsCacheTable::new(),
            config: DnsConfig::default_cfg(),
        }
    }

    fn init_builtin_hosts(&mut self) {
        self.upsert_host_inner("localhost", [127, 0, 0, 1], true);
        self.upsert_host_inner("gateway.jinn", [10, 0, 2, 2], true);
        self.upsert_host_inner("gateway", [10, 0, 2, 2], true);
        self.upsert_host_inner("router", [10, 0, 2, 2], true);
        self.upsert_host_inner("dns", [10, 0, 2, 3], true);
    }

    fn lookup_static_host(&mut self, name: &str) -> Option<[u8; 4]> {
        let (norm, len, hash) = normalize_hostname(name)?;
        for h in self.hosts.iter_mut() {
            if h.valid && h.domain_hash == hash && h.hostname_len as usize == len {
                if h.hostname[..len] == norm[..len] {
                    h.hits = h.hits.saturating_add(1);
                    return Some(h.ipv4);
                }
            }
        }
        None
    }

    fn upsert_host_inner(&mut self, name: &str, ip: [u8; 4], builtin: bool) -> bool {
        let (norm, len, hash) = match normalize_hostname(name) {
            Some(v) => v,
            None => return false,
        };
        for h in self.hosts.iter_mut() {
            if h.valid && h.domain_hash == hash && h.hostname_len as usize == len {
                if h.hostname[..len] == norm[..len] {
                    h.ipv4 = ip;
                    if !builtin {
                        h.builtin = false;
                    }
                    return true;
                }
            }
        }
        for h in self.hosts.iter_mut() {
            if !h.valid {
                *h = StaticHostEntry {
                    domain_hash: hash,
                    hostname: norm,
                    ipv4: ip,
                    hits: 0,
                    hostname_len: len as u8,
                    valid: true,
                    builtin,
                };
                return true;
            }
        }
        // Sobrescreve entrada não embutida mais antiga se cheio
        for h in self.hosts.iter_mut() {
            if h.valid && !h.builtin {
                *h = StaticHostEntry {
                    domain_hash: hash,
                    hostname: norm,
                    ipv4: ip,
                    hits: 0,
                    hostname_len: len as u8,
                    valid: true,
                    builtin,
                };
                return true;
            }
        }
        false
    }

    fn remove_host_inner(&mut self, name: &str) -> bool {
        let (norm, len, hash) = match normalize_hostname(name) {
            Some(v) => v,
            None => return false,
        };
        for h in self.hosts.iter_mut() {
            if h.valid && h.domain_hash == hash && h.hostname_len as usize == len {
                if h.hostname[..len] == norm[..len] {
                    *h = StaticHostEntry::empty();
                    return true;
                }
            }
        }
        false
    }
}

static DNS_TABLE: IrqSafeSpinLock<DnsSystemState> = IrqSafeSpinLock::new(DnsSystemState::new());

// Buffers estáticos em .bss protegidos por AtomicBool para evitar bloqueio de IRQs
// durante operações de disco FAT32 e manter uso zero de stack em I/O.
static DNS_IO_BUSY: AtomicBool = AtomicBool::new(false);
static mut DNS_CACHE_IO_BUF: [u8; CACHE_DB_MAX_BYTES] = [0u8; CACHE_DB_MAX_BYTES];
static mut DNS_HOSTS_IO_BUF: [u8; 4096] = [0u8; 4096];
static SELFTEST_CACHE: IrqSafeSpinLock<DnsCacheTable> = IrqSafeSpinLock::new(DnsCacheTable::new());

struct DnsIoGuard;
impl DnsIoGuard {
    fn acquire() -> Self {
        while DNS_IO_BUSY
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        Self
    }
}
impl Drop for DnsIoGuard {
    fn drop(&mut self) {
        DNS_IO_BUSY.store(false, Ordering::Release);
    }
}

#[inline]
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

pub fn is_valid_hostname(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.len() > MAX_NAME_LEN || bytes[0] == b'.' {
        return false;
    }
    let mut prev_dot = false;
    for (idx, &b) in bytes.iter().enumerate() {
        if b == b'.' {
            if prev_dot {
                return false;
            }
            if idx + 1 == bytes.len() && idx == 0 {
                return false;
            }
            prev_dot = true;
        } else if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' {
            prev_dot = false;
        } else {
            return false;
        }
    }
    true
}

fn normalize_hostname(name: &str) -> Option<([u8; MAX_NAME_LEN], usize, u64)> {
    let trimmed = name.trim().trim_end_matches('.');
    if !is_valid_hostname(trimmed) {
        return None;
    }
    let bytes = trimmed.as_bytes();
    let len = bytes.len();
    if len == 0 || len > MAX_NAME_LEN {
        return None;
    }
    let mut norm = [0u8; MAX_NAME_LEN];
    for i in 0..len {
        norm[i] = bytes[i].to_ascii_lowercase();
    }
    let hash = fnv1a64(&norm[..len]);
    Some((norm, len, hash))
}

/// Inicializa tabela DNS de 3 camadas a partir de defaults, `/disk/hosts`, `/disk/dns_cache.db` e `/disk/net.cfg`.
pub fn init_tables() {
    {
        let mut st = DNS_TABLE.lock();
        st.init_builtin_hosts();
    }
    load_hosts_from_vfs();
    load_dns_config_from_vfs();
    load_dns_cache_from_vfs();
}

/// Retorna servidores DNS configurados `(primary, secondary, fallback)`.
pub fn get_dns_servers() -> ([u8; 4], [u8; 4], [u8; 4]) {
    let st = DNS_TABLE.lock();
    (st.config.primary, st.config.secondary, st.config.fallback)
}

/// Atualiza o servidor fallback em memória durante DHCP ACK sem I/O de disco no caminho de RX.
pub fn set_fallback_runtime(fallback: [u8; 4]) {
    let mut st = DNS_TABLE.lock();
    st.config.fallback = fallback;
}

/// Atualiza servidores DNS ativos e persiste em `/disk/net.cfg`.
pub fn set_dns_servers(primary: Option<[u8; 4]>, secondary: Option<[u8; 4]>, fallback: Option<[u8; 4]>) {
    {
        let mut st = DNS_TABLE.lock();
        if let Some(p) = primary {
            st.config.primary = p;
        }
        if let Some(s) = secondary {
            st.config.secondary = s;
        }
        if let Some(f) = fallback {
            st.config.fallback = f;
        }
    }
    save_dns_config_to_vfs();
}

/// Adiciona ou atualiza um host estático e persiste `/disk/hosts`.
pub fn add_static_host(name: &str, ip: [u8; 4]) -> bool {
    let ok = {
        let mut st = DNS_TABLE.lock();
        st.upsert_host_inner(name, ip, false)
    };
    if ok {
        crate::net::hot_routes::invalidate_host(name);
        save_hosts_to_vfs();
    }
    ok
}

/// Remove um host estático e persiste `/disk/hosts`.
pub fn remove_static_host(name: &str) -> bool {
    let removed = {
        let mut st = DNS_TABLE.lock();
        st.remove_host_inner(name)
    };
    if removed {
        crate::net::hot_routes::invalidate_host(name);
        save_hosts_to_vfs();
    }
    removed
}

/// Limpa entradas dinâmicas do cache DNS em RAM e atualiza `/disk/dns_cache.db`.
pub fn flush_dynamic_cache() -> usize {
    let cleared = {
        let mut st = DNS_TABLE.lock();
        st.cache.clear()
    };
    save_dns_cache_to_vfs();
    cleared
}

/// Copia entradas ativas da tabela local (`HOSTS` primeiro, depois `CACHE` válido) para Ring 3 / shell.
pub fn snapshot_active_entries(out: &mut [DnsEntryUser]) -> usize {
    if out.is_empty() {
        return 0;
    }
    let now_tick = crate::timer::ticks();
    let st = DNS_TABLE.lock();
    let mut written = 0usize;

    for h in st.hosts.iter() {
        if !h.valid {
            continue;
        }
        if written >= out.len() {
            return written;
        }
        out[written] = DnsEntryUser {
            hostname: h.hostname,
            hostname_len: h.hostname_len as u32,
            ipv4: h.ipv4,
            source_upstream: [0, 0, 0, 0],
            ttl_remaining_secs: 0,
            hits: h.hits,
            is_static_host: 1,
            is_stale: 0,
        };
        written += 1;
    }

    for c in st.cache.entries.iter() {
        if !c.valid {
            continue;
        }
        let is_expired = c.expires_at_tick != 0 && now_tick >= c.expires_at_tick;
        if is_expired {
            continue;
        }
        if written >= out.len() {
            return written;
        }
        let rem_secs = if c.expires_at_tick == 0 {
            c.ttl_secs
        } else {
            (c.expires_at_tick.saturating_sub(now_tick) / 100) as u32
        };
        out[written] = DnsEntryUser {
            hostname: c.hostname,
            hostname_len: c.hostname_len as u32,
            ipv4: c.ipv4,
            source_upstream: c.source_upstream,
            ttl_remaining_secs: rem_secs,
            hits: c.hits,
            is_static_host: 0,
            is_stale: 0,
        };
        written += 1;
    }

    written
}

/// Carrega `/disk/hosts` no formato `<IPv4> <hostname> [aliases...]`.
pub fn load_hosts_from_vfs() {
    let _io_guard = DnsIoGuard::acquire();
    let buf = unsafe { &mut *core::ptr::addr_of_mut!(DNS_HOSTS_IO_BUF) };
    let mut read_len = 0usize;
    let mut file_found = false;

    if let Ok(handle) = crate::vfs::open(HOSTS_PATH, crate::vfs::O_RDONLY) {
        if let Ok(n) = crate::vfs::read(HOSTS_PATH, handle, 0, buf) {
            read_len = n;
            file_found = true;
        }
        let _ = crate::vfs::close(HOSTS_PATH, handle);
    }

    if file_found && read_len > 0 {
        if let Ok(text) = core::str::from_utf8(&buf[..read_len]) {
            let mut loaded_count = 0usize;
            {
                let mut st = DNS_TABLE.lock();
                for raw_line in text.lines() {
                    let line = match raw_line.find('#') {
                        Some(idx) => raw_line[..idx].trim(),
                        None => raw_line.trim(),
                    };
                    if line.is_empty() {
                        continue;
                    }
                    let mut tokens = line.split_ascii_whitespace();
                    let ip_tok = match tokens.next() {
                        Some(t) => t,
                        None => continue,
                    };
                    let ip = match parse_ipv4_literal(ip_tok) {
                        Some(ip) => ip,
                        None => continue,
                    };
                    for alias in tokens {
                        if st.upsert_host_inner(alias, ip, false) {
                            loaded_count += 1;
                        }
                    }
                }
            }
            if loaded_count > 0 {
                crate::serial_write(b"[NET-DNS] Hosts estaticos carregados de /disk/hosts (");
                serial_write_usize(loaded_count);
                crate::serial_write(b" entradas)\n");
                return;
            }
        }
    }

    drop(_io_guard);
    save_hosts_to_vfs();
}

/// Grava a tabela estática em `/disk/hosts`.
pub fn save_hosts_to_vfs() {
    let _io_guard = DnsIoGuard::acquire();
    let out = unsafe { &mut *core::ptr::addr_of_mut!(DNS_HOSTS_IO_BUF) };
    let mut pos = 0usize;

    let hdr = b"# Jinn OS / Simbad Static Hosts Table\n";
    out[pos..pos + hdr.len()].copy_from_slice(hdr);
    pos += hdr.len();

    {
        let st = DNS_TABLE.lock();
        for h in st.hosts.iter() {
            if !h.valid || h.hostname_len == 0 {
                continue;
            }
            let name_len = (h.hostname_len as usize).min(MAX_NAME_LEN);
            if pos + 20 + name_len >= out.len() {
                break;
            }
            pos += format_ip_into(&mut out[pos..], h.ipv4);
            out[pos] = b' ';
            pos += 1;
            out[pos..pos + name_len].copy_from_slice(&h.hostname[..name_len]);
            pos += name_len;
            out[pos] = b'\n';
            pos += 1;
        }
    }

    let _ = crate::vfs::create(HOSTS_PATH);
    if let Ok(handle) = crate::vfs::open(HOSTS_PATH, crate::vfs::O_WRONLY | crate::vfs::O_TRUNC) {
        let _ = crate::vfs::write(HOSTS_PATH, handle, 0, &out[..pos]);
        let _ = crate::vfs::close(HOSTS_PATH, handle);
        let _ = crate::vfs::sync_all();
    }
}

/// Persiste entradas dinâmicas ainda não expiradas em `/disk/dns_cache.db`.
pub fn save_dns_cache_to_vfs() {
    let _io_guard = DnsIoGuard::acquire();
    let out = unsafe { &mut *core::ptr::addr_of_mut!(DNS_CACHE_IO_BUF) };
    let now_tick = crate::timer::ticks();
    let now_unix = crate::drivers::rtc::current_unix_timestamp();

    let mut count = 0u32;
    let mut pos = 16usize; // header: magic(8) + version(4) + count(4)

    {
        let st = DNS_TABLE.lock();
        for c in st.cache.entries.iter() {
            if !c.valid || c.hostname_len == 0 {
                continue;
            }
            if c.expires_at_tick != 0 && now_tick >= c.expires_at_tick {
                continue;
            }
            if pos + CACHE_RECORD_WIRE_SIZE + 8 > out.len() {
                break;
            }
            let exp_unix = if c.expires_at_unix > now_unix {
                c.expires_at_unix
            } else if c.expires_at_tick > now_tick {
                now_unix.saturating_add((c.expires_at_tick - now_tick) / 100)
            } else {
                now_unix.saturating_add(c.ttl_secs as u64)
            };

            out[pos] = c.hostname_len;
            pos += 1;
            out[pos..pos + MAX_NAME_LEN].copy_from_slice(&c.hostname);
            pos += MAX_NAME_LEN;
            out[pos..pos + 4].copy_from_slice(&c.ipv4);
            pos += 4;
            out[pos..pos + 4].copy_from_slice(&c.source_upstream);
            pos += 4;
            out[pos..pos + 8].copy_from_slice(&exp_unix.to_le_bytes());
            pos += 8;
            out[pos..pos + 4].copy_from_slice(&c.ttl_secs.to_le_bytes());
            pos += 4;
            out[pos..pos + 4].copy_from_slice(&c.hits.to_le_bytes());
            pos += 4;
            count += 1;
        }
    }

    out[0..8].copy_from_slice(&CACHE_DB_MAGIC);
    out[8..12].copy_from_slice(&CACHE_DB_VERSION.to_le_bytes());
    out[12..16].copy_from_slice(&count.to_le_bytes());

    let csum = fnv1a64(&out[..pos]);
    out[pos..pos + 8].copy_from_slice(&csum.to_le_bytes());
    pos += 8;

    let _ = crate::vfs::create(CACHE_DB_PATH);
    if let Ok(handle) = crate::vfs::open(CACHE_DB_PATH, crate::vfs::O_WRONLY | crate::vfs::O_TRUNC) {
        let _ = crate::vfs::write(CACHE_DB_PATH, handle, 0, &out[..pos]);
        let _ = crate::vfs::close(CACHE_DB_PATH, handle);
        let _ = crate::vfs::sync_all();
    }
}

/// Carrega `/disk/dns_cache.db` na inicialização, restaurando apenas registros ainda válidos pelo RTC UNIX.
pub fn load_dns_cache_from_vfs() {
    let _io_guard = DnsIoGuard::acquire();
    let buf = unsafe { &mut *core::ptr::addr_of_mut!(DNS_CACHE_IO_BUF) };
    let mut read_len = 0usize;

    if let Ok(handle) = crate::vfs::open(CACHE_DB_PATH, crate::vfs::O_RDONLY) {
        if let Ok(n) = crate::vfs::read(CACHE_DB_PATH, handle, 0, buf) {
            read_len = n;
        }
        let _ = crate::vfs::close(CACHE_DB_PATH, handle);
    }

    if read_len < 24 || &buf[0..8] != &CACHE_DB_MAGIC {
        return;
    }
    let ver = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
    let count = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]) as usize;
    if ver != CACHE_DB_VERSION || count > DNS_CACHE_CAPACITY {
        return;
    }
    let expected_len = 16 + count * CACHE_RECORD_WIRE_SIZE + 8;
    if read_len < expected_len {
        return;
    }
    let stored_csum = u64::from_le_bytes([
        buf[expected_len - 8], buf[expected_len - 7], buf[expected_len - 6], buf[expected_len - 5],
        buf[expected_len - 4], buf[expected_len - 3], buf[expected_len - 2], buf[expected_len - 1],
    ]);
    if fnv1a64(&buf[..expected_len - 8]) != stored_csum {
        return;
    }

    let now_tick = crate::timer::ticks().max(1);
    let now_unix = crate::drivers::rtc::current_unix_timestamp();
    let mut restored = 0usize;
    let mut pos = 16usize;

    {
        let mut st = DNS_TABLE.lock();
        for _ in 0..count {
            let name_len = (buf[pos] as usize).min(MAX_NAME_LEN);
            pos += 1;
            let name_bytes = &buf[pos..pos + name_len];
            pos += MAX_NAME_LEN;
            let ip = [buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]];
            pos += 4;
            let source_upstream = [buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]];
            pos += 4;
            let exp_unix = u64::from_le_bytes([
                buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3],
                buf[pos + 4], buf[pos + 5], buf[pos + 6], buf[pos + 7],
            ]);
            pos += 8;
            let ttl_secs = u32::from_le_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]);
            pos += 4;
            let hits = u32::from_le_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]);
            pos += 4;

            if exp_unix > now_unix {
                if let Ok(name_str) = core::str::from_utf8(name_bytes) {
                    let rem_secs = (exp_unix - now_unix).min(MAX_DNS_TTL_SECS as u64) as u32;
                    let clamped_rem = rem_secs.max(1);
                    st.cache.insert_with_metadata(
                        name_str,
                        ip,
                        ttl_secs.max(MIN_DNS_TTL_SECS).min(MAX_DNS_TTL_SECS),
                        source_upstream,
                        now_tick,
                        now_unix,
                        hits,
                    );
                    // Ajusta expires_at_tick/unix exatos para preservar o TTL restante após cold-reboot
                    if let Some((norm, len, hash)) = normalize_hostname(name_str) {
                        for entry in st.cache.entries.iter_mut() {
                            if entry.valid && entry.domain_hash == hash && entry.hostname_len as usize == len {
                                if entry.hostname[..len] == norm[..len] {
                                    entry.expires_at_tick = now_tick.saturating_add((clamped_rem as u64).saturating_mul(100));
                                    entry.expires_at_unix = exp_unix;
                                    entry.ttl_secs = ttl_secs.max(MIN_DNS_TTL_SECS).min(MAX_DNS_TTL_SECS);
                                    entry.hits = hits;
                                    break;
                                }
                            }
                        }
                    }
                    restored += 1;
                }
            }
        }
    }

    if restored > 0 {
        crate::serial_write(b"[NET-DNS] Cache persistente restaurado de /disk/dns_cache.db (");
        serial_write_usize(restored);
        crate::serial_write(b" entradas)\n");
    }
}

/// Estrutura para rastrear consultas DNS em trânsito
#[derive(Clone, Copy)]
pub struct PendingDnsQuery {
    pub active: bool,
    pub tx_id: u16,
    pub name: [u8; MAX_NAME_LEN],
    pub name_len: u8,
    pub start_tick: u64,
    pub status: DnsQueryStatus,
    pub target_server: [u8; 4],
}

impl PendingDnsQuery {
    pub const fn empty() -> Self {
        Self {
            active: false,
            tx_id: 0,
            name: [0u8; MAX_NAME_LEN],
            name_len: 0,
            start_tick: 0,
            status: DnsQueryStatus::Pending,
            target_server: [1, 1, 1, 1],
        }
    }
}

pub const MAX_PENDING_DNS: usize = 8;

pub struct PendingDnsTable {
    pub slots: [Option<PendingDnsQuery>; MAX_PENDING_DNS],
}

impl PendingDnsTable {
    pub const fn new() -> Self {
        Self {
            slots: [None; MAX_PENDING_DNS],
        }
    }

    pub fn insert(&mut self, query: PendingDnsQuery) {
        for slot in self.slots.iter_mut() {
            if slot.is_none() {
                *slot = Some(query);
                return;
            }
        }
        self.slots[0] = Some(query);
    }

    pub fn remove(&mut self, tx_id: u16) {
        for slot in self.slots.iter_mut() {
            if let Some(q) = slot {
                if q.tx_id == tx_id {
                    *slot = None;
                    return;
                }
            }
        }
    }

    pub fn set_target_server(&mut self, tx_id: u16, server: [u8; 4]) {
        for slot in self.slots.iter_mut() {
            if let Some(q) = slot {
                if q.tx_id == tx_id && q.active {
                    q.target_server = server;
                    return;
                }
            }
        }
    }

    pub fn update_status(&mut self, tx_id: u16, status: DnsQueryStatus) {
        for slot in self.slots.iter_mut() {
            if let Some(q) = slot {
                if q.tx_id == tx_id && q.active {
                    q.status = status;
                    return;
                }
            }
        }
    }

    pub fn get_status(&self, tx_id: u16) -> Option<DnsQueryStatus> {
        for slot in self.slots.iter() {
            if let Some(q) = slot {
                if q.tx_id == tx_id && q.active {
                    return Some(q.status);
                }
            }
        }
        None
    }
}

pub static mut PENDING_DNS_TABLE: PendingDnsTable = PendingDnsTable::new();
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

/// Extrai e valida resposta DNS RFC 1035 (Registro A / IPv4), respeitando TTL entre 60s e 86400s.
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
    if (flags & FLAG_QR_RESPONSE) == 0 {
        return Err(DnsError::NotAResponse);
    }
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
                let exp_bytes = exp_name.trim_end_matches('.').as_bytes();
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
        let ttl = u32::from_be_bytes([buf[offset + 4], buf[offset + 5], buf[offset + 6], buf[offset + 7]]);
        let rdlength = u16::from_be_bytes([buf[offset + 8], buf[offset + 9]]) as usize;
        offset += 10;

        if offset + rdlength > buf.len() {
            return Err(DnsError::TruncatedPacket);
        }

        if rtype == QTYPE_A && rclass == QCLASS_IN && rdlength == 4 {
            let ip = [buf[offset], buf[offset + 1], buf[offset + 2], buf[offset + 3]];
            return Ok(DnsAnswer {
                id,
                ip,
                ttl_secs: ttl.max(MIN_DNS_TTL_SECS).min(MAX_DNS_TTL_SECS),
            });
        }

        offset += rdlength;
    }

    Err(DnsError::NoAnswer)
}

/// Trata datagrama DNS recebido da rede, associa à consulta em trânsito e alimenta o cache local.
pub fn handle_dns_reply(payload: &[u8]) {
    if payload.len() < 12 {
        return;
    }
    let rx_tx_id = u16::from_be_bytes([payload[0], payload[1]]);

    unsafe {
        let mut found_name = [0u8; MAX_NAME_LEN];
        let mut found_len = 0usize;
        let mut found_id = 0u16;
        let mut found_server = [1, 1, 1, 1];
        let mut found = false;

        for slot in PENDING_DNS_TABLE.slots.iter() {
            if let Some(q) = slot {
                if q.tx_id == rx_tx_id && q.active {
                    found_len = (q.name_len as usize).min(MAX_NAME_LEN);
                    found_name[..found_len].copy_from_slice(&q.name[..found_len]);
                    found_id = q.tx_id;
                    found_server = q.target_server;
                    found = true;
                    break;
                }
            }
        }
        if !found {
            if let Some(p) = &PENDING_QUERY {
                if p.active && p.tx_id == rx_tx_id {
                    found_len = (p.name_len as usize).min(MAX_NAME_LEN);
                    found_name[..found_len].copy_from_slice(&p.name[..found_len]);
                    found_id = p.tx_id;
                    found_server = p.target_server;
                    found = true;
                }
            }
        }
        if !found {
            return;
        }

        let pending_name_str = match core::str::from_utf8(&found_name[..found_len]) {
            Ok(s) => s,
            Err(_) => return,
        };

        match parse_response(payload, Some(found_id), Some(pending_name_str)) {
            Ok(answer) => {
                let current_tick = crate::timer::ticks().max(1);
                let now_unix = crate::drivers::rtc::current_unix_timestamp();
                {
                    let mut st = DNS_TABLE.lock();
                    st.cache.insert(pending_name_str, answer.ip, answer.ttl_secs, found_server, current_tick, now_unix);
                }
                PENDING_DNS_TABLE.update_status(found_id, DnsQueryStatus::Resolved(answer.ip, answer.ttl_secs, found_server));
            }
            Err(DnsError::NameNotFound) => {
                PENDING_DNS_TABLE.update_status(found_id, DnsQueryStatus::NxDomain);
            }
            Err(DnsError::ServerFailure) | Err(DnsError::Refused) | Err(DnsError::FormatError) => {
                PENDING_DNS_TABLE.update_status(found_id, DnsQueryStatus::ServerError);
            }
            _ => {}
        }
    }
}

pub fn load_dns_config_from_vfs() {
    let mut buf = [0u8; 512];
    if let Ok(handle) = crate::vfs::open(NET_CFG_PATH, crate::vfs::O_RDONLY) {
        if let Ok(n) = crate::vfs::read(NET_CFG_PATH, handle, 0, &mut buf) {
            let _ = crate::vfs::close(NET_CFG_PATH, handle);
            if n > 0 {
                if let Ok(text) = core::str::from_utf8(&buf[..n]) {
                    let mut found_primary = false;
                    let mut st = DNS_TABLE.lock();
                    for line in text.lines() {
                        let line = line.trim();
                        if let Some(val) = line.strip_prefix("DNS_PRIMARY=") {
                            if let Some(ip) = parse_ipv4_literal(val.trim()) {
                                st.config.primary = ip;
                                found_primary = true;
                            }
                        } else if let Some(val) = line.strip_prefix("DNS_SECONDARY=") {
                            if let Some(ip) = parse_ipv4_literal(val.trim()) {
                                st.config.secondary = ip;
                            }
                        } else if let Some(val) = line.strip_prefix("DNS_FALLBACK=") {
                            if let Some(ip) = parse_ipv4_literal(val.trim()) {
                                st.config.fallback = ip;
                            }
                        }
                    }
                    if found_primary {
                        return;
                    }
                }
            }
        } else {
            let _ = crate::vfs::close(NET_CFG_PATH, handle);
        }
    }
    save_dns_config_to_vfs();
}

pub fn save_dns_config_to_vfs() {
    let (p, s, f) = get_dns_servers();
    let mut out = [0u8; 256];
    let mut pos = 0;

    let p1 = b"DNS_PRIMARY=";
    out[pos..pos + p1.len()].copy_from_slice(p1);
    pos += p1.len();
    pos += format_ip_into(&mut out[pos..], p);
    out[pos] = b'\n';
    pos += 1;

    let p2 = b"DNS_SECONDARY=";
    out[pos..pos + p2.len()].copy_from_slice(p2);
    pos += p2.len();
    pos += format_ip_into(&mut out[pos..], s);
    out[pos] = b'\n';
    pos += 1;

    let p3 = b"DNS_FALLBACK=";
    out[pos..pos + p3.len()].copy_from_slice(p3);
    pos += p3.len();
    pos += format_ip_into(&mut out[pos..], f);
    out[pos] = b'\n';
    pos += 1;

    let _ = crate::vfs::create(NET_CFG_PATH);
    if let Ok(handle) = crate::vfs::open(NET_CFG_PATH, crate::vfs::O_WRONLY | crate::vfs::O_TRUNC) {
        let _ = crate::vfs::write(NET_CFG_PATH, handle, 0, &out[..pos]);
        let _ = crate::vfs::close(NET_CFG_PATH, handle);
        let _ = crate::vfs::sync_all();
    }
}

pub fn format_ip_into(dst: &mut [u8], ip: [u8; 4]) -> usize {
    let mut p = 0;
    for i in 0..4 {
        if i > 0 {
            dst[p] = b'.';
            p += 1;
        }
        let b = ip[i];
        if b >= 100 {
            dst[p] = b'0' + (b / 100);
            p += 1;
            dst[p] = b'0' + ((b / 10) % 10);
            p += 1;
            dst[p] = b'0' + (b % 10);
            p += 1;
        } else if b >= 10 {
            dst[p] = b'0' + (b / 10);
            p += 1;
            dst[p] = b'0' + (b % 10);
            p += 1;
        } else {
            dst[p] = b'0' + b;
            p += 1;
        }
    }
    p
}

fn serial_write_usize(mut v: usize) {
    let mut tmp = [0u8; 24];
    let mut idx = 0;
    if v == 0 {
        crate::serial_write(b"0");
        return;
    }
    while v > 0 {
        tmp[idx] = b'0' + (v % 10) as u8;
        v /= 10;
        idx += 1;
    }
    while idx > 0 {
        idx -= 1;
        crate::serial_write(&[tmp[idx]]);
    }
}

pub fn log_dns_resolving(name: &str, server_ip: [u8; 4]) {
    crate::serial_write(b"[NET-DNS] Resolvendo '");
    crate::serial_write(name.as_bytes());
    crate::serial_write(b"' via upstream ");
    let mut ip_buf = [0u8; 16];
    let len = format_ip_into(&mut ip_buf, server_ip);
    crate::serial_write(&ip_buf[..len]);
    crate::serial_write(b"...\n");
}

pub fn log_dns_switch(target_name: &str, server_ip: [u8; 4]) {
    crate::serial_write(b"[NET-DNS] Timeout/falha no ");
    if target_name == "secundario" {
        crate::serial_write(b"primario. Chaveando para secundario ");
    } else {
        crate::serial_write(b"secundario. Chaveando para fallback ");
    }
    let mut ip_buf = [0u8; 16];
    let len = format_ip_into(&mut ip_buf, server_ip);
    crate::serial_write(&ip_buf[..len]);
    crate::serial_write(b"...\n");
}

pub fn log_dns_resolved(name: &str, ip: [u8; 4]) {
    crate::serial_write(b"[NET-DNS] '");
    crate::serial_write(name.as_bytes());
    crate::serial_write(b"' resolvido com sucesso para ");
    let mut ip_buf = [0u8; 16];
    let len = format_ip_into(&mut ip_buf, ip);
    crate::serial_write(&ip_buf[..len]);
    crate::serial_write(b"\n");
}

pub fn log_dns_cache_hit(name: &str, ip: [u8; 4], tier: &str) {
    crate::serial_write(b"[NET-DNS] '");
    crate::serial_write(name.as_bytes());
    crate::serial_write(b"' resolvido com sucesso para ");
    let mut ip_buf = [0u8; 16];
    let len = format_ip_into(&mut ip_buf, ip);
    crate::serial_write(&ip_buf[..len]);
    crate::serial_write(b" (");
    crate::serial_write(tier.as_bytes());
    crate::serial_write(b" HIT, 0 pkts)\n");
}

pub fn log_dns_stale_hit(name: &str, ip: [u8; 4]) {
    crate::serial_write(b"[NET-DNS] Serving stale cached IP for '");
    crate::serial_write(name.as_bytes());
    crate::serial_write(b"' -> ");
    let mut ip_buf = [0u8; 16];
    let len = format_ip_into(&mut ip_buf, ip);
    crate::serial_write(&ip_buf[..len]);
    crate::serial_write(b"\n");
}

pub fn send_query(name: &str, tx_id: u16) -> Result<(), DnsError> {
    let (primary, _, _) = get_dns_servers();
    send_query_to(name, tx_id, Some(primary))
}

pub fn send_query_to(name: &str, tx_id: u16, server_override: Option<[u8; 4]>) -> Result<(), DnsError> {
    let mut dns_payload = [0u8; 512];
    let payload_len = build_query(name, tx_id, &mut dns_payload)?;

    let dev = match crate::drivers::net::get_device() {
        Some(d) => d,
        None => return Err(DnsError::ServerFailure),
    };

    let (primary, _, _) = get_dns_servers();
    let our_ip = unsafe { DHCP_CLIENT.ip };
    let mut dns_server_ip = server_override.unwrap_or(primary);
    if dns_server_ip == [0, 0, 0, 0] {
        dns_server_ip = [1, 1, 1, 1];
    }
    let our_mac = dev.mac_address();
    let target_mac = if let Some(fast_mac) = crate::net::hot_routes::lookup_mac_for_ip(dns_server_ip) {
        fast_mac
    } else {
        let mac = crate::net::arp::resolve_mac_for_ip(dns_server_ip);
        let nh_ip = crate::net::ipv4::get_next_hop_ip(dns_server_ip);
        crate::net::hot_routes::confirm_gateway_seed(dns_server_ip, nh_ip, mac);
        mac
    };

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
    crate::net::NET_DIAGNOSTICS.record_dns_query_sent();
    Ok(())
}

/// Resolve nome de domínio em 3 estágios:
/// Estágio 1 (`/disk/hosts`) -> Estágio 2 (`DnsCacheTable` LRU/TTL) -> Estágio 3 (`Upstream + Stale-If-Error`).
pub fn resolve(name: &str) -> Result<[u8; 4], DnsResolveError> {
    let ticks = crate::timer::ticks();

    // 0. Literal IPv4 direto (ex: "10.0.2.2")
    if let Some(ip) = parse_ipv4_literal(name.trim()) {
        return Ok(ip);
    }

    if !is_valid_hostname(name.trim().trim_end_matches('.')) {
        return Err(DnsResolveError::InvalidName);
    }

    // Estágio 1 (/disk/hosts) tem precedência soberana sobre o cache
    {
        let mut st = DNS_TABLE.lock();
        if let Some(static_ip) = st.lookup_static_host(name) {
            drop(st);
            log_dns_cache_hit(name, static_ip, "HOSTS");
            return Ok(static_ip);
        }
    }

    // Estágio Fast-Path Unificado (`HotRouteTable`: DNS + L2/L3 Next-Hop MAC)
    if let Some(fast_route) = crate::net::hot_routes::lookup_fast_route(name) {
        // Sincroniza contador de hits na camada DnsCacheTable em RAM sem gerar I/O de disco
        {
            let mut st = DNS_TABLE.lock();
            let _ = st.cache.lookup(name, ticks);
        }
        return Ok(fast_route.dst_ip);
    }

    // Estágio 2 (DnsCacheTable LRU com TTL preguiçoso)
    {
        let mut st = DNS_TABLE.lock();
        if let Some(cached_ip) = st.cache.lookup(name, ticks) {
            drop(st);
            log_dns_cache_hit(name, cached_ip, "CACHE");
            return Ok(cached_ip);
        }
    }

    // Estágio 3: Consulta Upstream em Cascata (com fallback Stale-If-Error)
    let dev_available = crate::drivers::net::get_device().map(|d| d.is_available()).unwrap_or(false);
    if !dev_available {
        let stale_opt = {
            let mut st = DNS_TABLE.lock();
            st.cache.lookup_stale(name)
        };
        if let Some(stale_ip) = stale_opt {
            log_dns_stale_hit(name, stale_ip);
            return Ok(stale_ip);
        }
        if let Some(stale_route) = crate::net::hot_routes::lookup_stale_route(name) {
            return Ok(stale_route.dst_ip);
        }
        return Err(DnsResolveError::InterfaceDown);
    }

    let (norm, name_len, _) = normalize_hostname(name).ok_or(DnsResolveError::InvalidName)?;
    let (primary, secondary, fallback) = get_dns_servers();

    let tx_id = (ticks as u16) ^ 0xA55A;
    let query_entry = PendingDnsQuery {
        active: true,
        tx_id,
        name: norm,
        name_len: name_len as u8,
        start_tick: ticks,
        status: DnsQueryStatus::Pending,
        target_server: primary,
    };
    unsafe {
        PENDING_QUERY = Some(query_entry);
        PENDING_DNS_TABLE.insert(query_entry);
    }

    log_dns_resolving(name, primary);

    if send_query_to(name, tx_id, Some(primary)).is_err() {
        unsafe {
            PENDING_QUERY = None;
            PENDING_DNS_TABLE.remove(tx_id);
        }
        let stale_opt = {
            let mut st = DNS_TABLE.lock();
            st.cache.lookup_stale(name)
        };
        if let Some(stale_ip) = stale_opt {
            log_dns_stale_hit(name, stale_ip);
            return Ok(stale_ip);
        }
        if let Some(stale_route) = crate::net::hot_routes::lookup_stale_route(name) {
            return Ok(stale_route.dst_ip);
        }
        return Err(DnsResolveError::ArpFailed);
    }

    let start_tick = crate::timer::ticks();
    let mut last_tx = start_tick;
    let mut stage = 0u8;

    while crate::timer::ticks().saturating_sub(start_tick) < 200 {
        crate::net::poll_if_needed();

        if let Some(status) = unsafe { PENDING_DNS_TABLE.get_status(tx_id) } {
            match status {
                DnsQueryStatus::Resolved(ip, ttl_secs, _) => {
                    let dns_rtt_ticks = crate::timer::ticks().saturating_sub(start_tick).max(1) as u32;
                    unsafe {
                        PENDING_QUERY = None;
                        PENDING_DNS_TABLE.remove(tx_id);
                    }
                    let (nh_ip, nh_mac) = crate::net::ipv4::resolve_route_endpoints(ip);
                    crate::net::hot_routes::record_route_resolved(
                        name,
                        ip,
                        ttl_secs,
                        nh_ip,
                        nh_mac,
                        dns_rtt_ticks,
                        true,
                    );
                    save_dns_cache_to_vfs();
                    log_dns_resolved(name, ip);
                    return Ok(ip);
                }
                DnsQueryStatus::NxDomain => {
                    crate::net::NET_DIAGNOSTICS.record_dns_nxdomain();
                    unsafe {
                        PENDING_QUERY = None;
                        PENDING_DNS_TABLE.remove(tx_id);
                    }
                    return Err(DnsResolveError::NxDomain);
                }
                DnsQueryStatus::ServerError => {
                    if stage == 0 {
                        stage = 1;
                        log_dns_switch("secundario", secondary);
                        unsafe { PENDING_DNS_TABLE.set_target_server(tx_id, secondary); }
                        let _ = send_query_to(name, tx_id, Some(secondary));
                        last_tx = crate::timer::ticks();
                    } else if stage == 1 {
                        stage = 2;
                        log_dns_switch("fallback", fallback);
                        unsafe { PENDING_DNS_TABLE.set_target_server(tx_id, fallback); }
                        let _ = send_query_to(name, tx_id, Some(fallback));
                        last_tx = crate::timer::ticks();
                    } else {
                        unsafe {
                            PENDING_QUERY = None;
                            PENDING_DNS_TABLE.remove(tx_id);
                        }
                        let stale_opt = {
                            let mut st = DNS_TABLE.lock();
                            st.cache.lookup_stale(name)
                        };
                        if let Some(stale_ip) = stale_opt {
                            log_dns_stale_hit(name, stale_ip);
                            return Ok(stale_ip);
                        }
                        return Err(DnsResolveError::ServerFailure);
                    }
                }
                DnsQueryStatus::Pending => {}
            }
        }

        let elapsed = crate::timer::ticks().saturating_sub(start_tick);
        let since_last = crate::timer::ticks().saturating_sub(last_tx);

        if elapsed >= 40 && stage < 2 {
            stage = 2;
            last_tx = crate::timer::ticks();
            log_dns_switch("fallback", fallback);
            unsafe { PENDING_DNS_TABLE.set_target_server(tx_id, fallback); }
            let _ = send_query_to(name, tx_id, Some(fallback));
        } else if elapsed >= 20 && stage < 1 {
            stage = 1;
            last_tx = crate::timer::ticks();
            log_dns_switch("secundario", secondary);
            unsafe { PENDING_DNS_TABLE.set_target_server(tx_id, secondary); }
            let _ = send_query_to(name, tx_id, Some(secondary));
        } else if since_last >= 15 {
            last_tx = crate::timer::ticks();
            let target_server = match stage {
                0 => primary,
                1 => secondary,
                _ => fallback,
            };
            unsafe { PENDING_DNS_TABLE.set_target_server(tx_id, target_server); }
            let _ = send_query_to(name, tx_id, Some(target_server));
        }
        let next_stage = if stage == 0 {
            20u64.saturating_sub(elapsed).max(1)
        } else if stage == 1 {
            40u64.saturating_sub(elapsed).max(1)
        } else {
            15u64.saturating_sub(since_last).max(1)
        };
        crate::net::wait_for_rx_or_tick(next_stage.min(10));
    }

    unsafe {
        PENDING_QUERY = None;
        PENDING_DNS_TABLE.remove(tx_id);
    }

    // Stale-If-Error após esgotar toda a cascata upstream
    let stale_opt = {
        let mut st = DNS_TABLE.lock();
        st.cache.lookup_stale(name)
    };
    if let Some(stale_ip) = stale_opt {
        log_dns_stale_hit(name, stale_ip);
        return Ok(stale_ip);
    }
    if let Some(stale_route) = crate::net::hot_routes::lookup_stale_route(name) {
        return Ok(stale_route.dst_ip);
    }

    crate::net::NET_DIAGNOSTICS.record_dns_timeout();
    Err(DnsResolveError::Timeout)
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

/// Executa suíte de auto-testes de validação da RFC 1035, Cache LRU 128 entradas, Hosts e Stale-If-Error.
/// Retorna `(aprovados, total)`.
pub fn run_dns_selftests() -> (usize, usize) {
    let mut passed = 0;
    let total = 15;

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
    let long_label = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz01234567890123.com";
    if build_query(long_label, 0x1111, &mut qbuf).is_err() {
        passed += 1;
    }

    // 4. Parser RFC 1035 com resposta válida sem compressão e TTL >= 60s
    let mut resp = [0u8; 64];
    resp[0..2].copy_from_slice(&0x1234u16.to_be_bytes());
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

    if let Ok(a) = parse_response(&resp[..off], Some(0x1234), Some("example.com")) {
        if a.ip == [93, 184, 216, 34] && a.ttl_secs == 300 {
            passed += 1;
        }
    }

    // 5. Parser RFC 1035 com compressão (0xC00C)
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
    comp_resp[c_off..c_off+2].copy_from_slice(&[0xC0, 12]); c_off += 2;
    comp_resp[c_off..c_off+2].copy_from_slice(&QTYPE_A.to_be_bytes()); c_off += 2;
    comp_resp[c_off..c_off+2].copy_from_slice(&QCLASS_IN.to_be_bytes()); c_off += 2;
    comp_resp[c_off..c_off+4].copy_from_slice(&600u32.to_be_bytes()); c_off += 4;
    comp_resp[c_off..c_off+2].copy_from_slice(&4u16.to_be_bytes()); c_off += 2;
    comp_resp[c_off..c_off+4].copy_from_slice(&[1, 2, 3, 4]); c_off += 4;

    if let Ok(a) = parse_response(&comp_resp[..c_off], Some(0x5678), Some("org")) {
        if a.ip == [1, 2, 3, 4] {
            passed += 1;
        }
    }

    // 6. Detecção e rejeição de ponteiro circular ou destino inválido
    let mut bad_ptr_resp = comp_resp;
    bad_ptr_resp[12 + 3 + 1 + 4] = 0xC0;
    bad_ptr_resp[12 + 3 + 1 + 5] = 20;
    if parse_response(&bad_ptr_resp[..c_off], Some(0x5678), Some("org")).is_err() {
        passed += 1;
    }

    // 7. NXDOMAIN
    let mut nx_resp = comp_resp;
    nx_resp[3] = RCODE_NXDOMAIN as u8;
    if let Err(DnsError::NameNotFound) = parse_response(&nx_resp[..c_off], Some(0x5678), Some("org")) {
        passed += 1;
    }

    // 8. Truncated TC=1
    let mut tc_resp = comp_resp;
    tc_resp[2] |= 0x02;
    if let Err(DnsError::TruncatedPacket) = parse_response(&tc_resp[..c_off], Some(0x5678), Some("org")) {
        passed += 1;
    }

    // 9. Mismatched TxID & Question
    let mismatch_id = parse_response(&comp_resp[..c_off], Some(0x9999), Some("org"));
    let mismatch_q = parse_response(&comp_resp[..c_off], Some(0x5678), Some("different.com"));
    if let (Err(DnsError::MismatchedTxId), Err(DnsError::MismatchedQuestion)) = (mismatch_id, mismatch_q) {
        passed += 1;
    }

    // 10-14 executam no DnsCacheTable estático em .bss (zero uso excessivo da pilha do kernel)
    {
        let mut cache = SELFTEST_CACHE.lock();
        cache.clear();

        // 10. Inserção, FNV-1a lookup case-insensitive e incremento de hits
        cache.insert("MyHost.Domain.COM", [192, 168, 1, 100], 60, [1, 1, 1, 1], 1000, 1795000000);
        let hit_upper = cache.lookup("MYHOST.DOMAIN.COM", 1000);
        let hit_lower = cache.lookup("myhost.domain.com", 1001);
        if hit_upper == Some([192, 168, 1, 100]) && hit_lower == Some([192, 168, 1, 100]) && cache.entries[0].hits == 2 {
            passed += 1;
        }

        // 11. Expiração preguiçosa com TTL clamp mínimo (60s = 6000 ticks)
        cache.insert("expiring.host", [10, 10, 10, 10], 5, [1, 1, 1, 1], 1000, 1795000000);
        let valid_before = cache.lookup("expiring.host", 6999);
        let expired_at = cache.lookup("expiring.host", 7000);
        if valid_before == Some([10, 10, 10, 10]) && expired_at.is_none() {
            passed += 1;
        }

        // 12. Stale-If-Error recupera entrada expirada quando upstreams falham
        if cache.lookup_stale("expiring.host") == Some([10, 10, 10, 10]) {
            passed += 1;
        }

        // 13. Capacidade completa (128 slots): reutilização de slots expirados e despejo LRU
        cache.clear();
        for i in 0..DNS_CACHE_CAPACITY {
            let mut nb = [0u8; 16];
            nb[0] = b'h';
            nb[1] = b'0' + ((i / 100) as u8);
            nb[2] = b'0' + (((i / 10) % 10) as u8);
            nb[3] = b'0' + ((i % 10) as u8);
            nb[4..8].copy_from_slice(b".org");
            let n_str = core::str::from_utf8(&nb[..8]).unwrap();
            cache.insert(n_str, [10, 0, 1, i as u8], 60, [8, 8, 8, 8], 100 + (i as u64), 1795000000);
        }
        // Acessa h000 para torná-lo mais recente e insere novo domínio -> deve despejar h001 (LRU)
        let _ = cache.lookup("h000.org", 500);
        cache.insert("lru-new.org", [172, 16, 0, 1], 300, [1, 1, 1, 1], 501, 1795000005);
        if cache.lookup("lru-new.org", 501) == Some([172, 16, 0, 1])
            && cache.lookup("h000.org", 502) == Some([10, 0, 1, 0])
            && cache.lookup("h001.org", 502).is_none()
        {
            passed += 1;
        }

        // 14. Substituição prioritária de entrada expirada quando tabela está cheia
        cache.clear();
        for i in 0..DNS_CACHE_CAPACITY {
            let mut nb = [0u8; 16];
            nb[0] = b'e';
            nb[1] = b'0' + ((i / 100) as u8);
            nb[2] = b'0' + (((i / 10) % 10) as u8);
            nb[3] = b'0' + ((i % 10) as u8);
            nb[4..8].copy_from_slice(b".net");
            let n_str = core::str::from_utf8(&nb[..8]).unwrap();
            cache.insert(n_str, [10, 0, 2, i as u8], 60, [1, 1, 1, 1], 100, 1795000000);
        }
        cache.insert("fresh-over-expired.net", [172, 16, 1, 1], 300, [1, 1, 1, 1], 7000, 1795000100);
        if cache.lookup("fresh-over-expired.net", 7000) == Some([172, 16, 1, 1]) {
            passed += 1;
        }
        cache.clear();
    }

    // 15. Lookup imediato de hosts estáticos carregados (/disk/hosts)
    {
        let mut st = DNS_TABLE.lock();
        st.init_builtin_hosts();
        if st.lookup_static_host("LOCALHOST") == Some([127, 0, 0, 1])
            && st.lookup_static_host("gateway.jinn") == Some([10, 0, 2, 2])
        {
            passed += 1;
        }
    }

    (passed, total)
}
