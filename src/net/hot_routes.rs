//! Jinn OS — Tabela Unificada de Rotas Mais Usadas (`HotRouteTable` — DNS + L2/L3 Fast-Path)
//!
//! Unifica resolução de nomes DNS, gateway Next-Hop IPv4 e endereço MAC L2 confirmado em uma
//! única tabela estática alinhada a 64 bytes no `.bss`, com política Zero-Idle (TTL preguiçoso
//! sob demanda, zero timers em background), ranking por acessos (`hit_count`), evicção LFU/LRU
//! que preserva sementes fixas (`FLAG_PINNED_SEED`) e persistência em `/disk/hot_routes.db`.

use core::sync::atomic::{AtomicBool, Ordering};
use crate::memory::IrqSafeSpinLock;

pub const MAX_HOT_ROUTES: usize = 64;
pub const MAX_HOST_LEN: usize = 96;

pub const FLAG_PINNED_SEED: u8 = 1 << 0;
pub const FLAG_VALID: u8 = 1 << 1;
pub const FLAG_STALE_ALLOWED: u8 = 1 << 2;

pub const MIN_HOT_TTL_SECS: u32 = 120;
pub const MAX_HOT_TTL_SECS: u32 = 86_400;

pub const HOT_ROUTES_DB_PATH: &str = "/disk/hot_routes.db";
const HOT_DB_MAGIC: [u8; 8] = *b"JHRTE1\0\0";
const HOT_DB_VERSION: u32 = 1;
// Record layout: len(1) + host(96) + dst_ip(4) + nh_ip(4) + nh_mac(6) + hits(4) + rtt(4) + exp_unix(8) + rem_secs(8) + flags(1) = 136 bytes
const HOT_RECORD_WIRE_SIZE: usize = 136;
const HOT_DB_MAX_BYTES: usize = 16 + (MAX_HOT_ROUTES * HOT_RECORD_WIRE_SIZE) + 8;

const PINNED_DOMAIN_SEEDS: &[&str] = &[
    "jinn-os.dev.br",
    "68k.news",
    "frogfind.com",
    "example.com",
    "github.com",
    "raw.githubusercontent.com",
];

const PINNED_UPSTREAM_SEEDS: &[(&str, [u8; 4])] = &[
    ("1.1.1.1", [1, 1, 1, 1]),
    ("8.8.8.8", [8, 8, 8, 8]),
];

/// Entrada individual de rota quente (DNS + L2/L3 Next-Hop).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HotRouteEntry {
    pub domain_hash: u64,
    pub hostname: [u8; MAX_HOST_LEN],
    pub hostname_len: u8,
    pub dst_ip: [u8; 4],
    pub next_hop_ip: [u8; 4],
    pub next_hop_mac: [u8; 6],
    pub hit_count: u32,
    pub last_rtt_ticks: u32,
    pub expires_at_tick: u64,
    pub expires_at_unix: u64,
    pub last_used_tick: u64,
    pub flags: u8,
}

impl HotRouteEntry {
    pub const fn empty() -> Self {
        Self {
            domain_hash: 0,
            hostname: [0u8; MAX_HOST_LEN],
            hostname_len: 0,
            dst_ip: [0u8; 4],
            next_hop_ip: [0u8; 4],
            next_hop_mac: [0u8; 6],
            hit_count: 0,
            last_rtt_ticks: 0,
            expires_at_tick: 0,
            expires_at_unix: 0,
            last_used_tick: 0,
            flags: 0,
        }
    }

    #[inline]
    pub fn is_occupied(&self) -> bool {
        self.hostname_len > 0
    }

    #[inline]
    pub fn is_pinned(&self) -> bool {
        (self.flags & FLAG_PINNED_SEED) != 0
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        (self.flags & FLAG_VALID) != 0 && self.dst_ip != [0, 0, 0, 0] && self.next_hop_mac != [0u8; 6]
    }

    #[inline]
    pub fn stale_allowed(&self) -> bool {
        (self.flags & FLAG_STALE_ALLOWED) != 0
    }

    pub fn name_bytes(&self) -> &[u8] {
        let n = (self.hostname_len as usize).min(MAX_HOST_LEN);
        &self.hostname[..n]
    }

    pub fn is_expired(&self, now_tick: u64, now_unix: u64) -> bool {
        if now_unix > 1_700_000_000 && self.expires_at_unix > 1_700_000_000 {
            return now_unix >= self.expires_at_unix;
        }
        if self.expires_at_tick != 0 {
            return now_tick >= self.expires_at_tick;
        }
        false
    }
}

/// Resultado pronto para o Fast-Path Ethernet + IPv4 + TCP SYN sem nova consulta DNS ou ARP.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedRoute {
    pub dst_ip: [u8; 4],
    pub next_hop_ip: [u8; 4],
    pub next_hop_mac: [u8; 6],
    pub hit_count: u32,
    pub last_rtt_ticks: u32,
    pub stale: bool,
}

/// Estrutura compacta compartilhada via Syscalls 97..99 entre Ring 0 e Ring 3 (Aladdin Desktop).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HotRouteEntryUser {
    pub hit_count: u32,
    pub last_rtt_ticks: u32,
    pub ttl_remaining_secs: u32,
    pub hostname: [u8; MAX_HOST_LEN],
    pub hostname_len: u8,
    pub dst_ip: [u8; 4],
    pub next_hop_ip: [u8; 4],
    pub next_hop_mac: [u8; 6],
    pub flags: u8,
    pub valid: u8,
    pub stale: u8,
    pub _reserved: [u8; 2],
}

impl Default for HotRouteEntryUser {
    fn default() -> Self {
        Self::empty()
    }
}

impl HotRouteEntryUser {
    pub const fn empty() -> Self {
        Self {
            hit_count: 0,
            last_rtt_ticks: 0,
            ttl_remaining_secs: 0,
            hostname: [0u8; MAX_HOST_LEN],
            hostname_len: 0,
            dst_ip: [0u8; 4],
            next_hop_ip: [0u8; 4],
            next_hop_mac: [0u8; 6],
            flags: 0,
            valid: 0,
            stale: 0,
            _reserved: [0u8; 2],
        }
    }
}

/// Tabela unificada de 64 rotas quentes alinhada a cache line de 64 bytes em `.bss` estático.
#[repr(align(64))]
pub struct HotRouteTable {
    pub entries: [HotRouteEntry; MAX_HOT_ROUTES],
    pub initialized: bool,
    pub dirty: bool,
    pub fast_hits: u64,
    pub stale_hits: u64,
}

impl HotRouteTable {
    pub const fn empty() -> Self {
        Self {
            entries: [HotRouteEntry::empty(); MAX_HOT_ROUTES],
            initialized: false,
            dirty: false,
            fast_hits: 0,
            stale_hits: 0,
        }
    }

    pub fn clear_all(&mut self) {
        self.entries = [HotRouteEntry::empty(); MAX_HOT_ROUTES];
        self.initialized = false;
        self.dirty = false;
        self.fast_hits = 0;
        self.stale_hits = 0;
    }

    /// Garante a presença das 8 sementes padrão de ecossistema caso ainda não estejam na tabela.
    pub fn ensure_default_seeds(&mut self, default_gateway: [u8; 4]) {
        for &domain in PINNED_DOMAIN_SEEDS.iter() {
            let (norm, len, hash) = match normalize_host(domain) {
                Some(v) => v,
                None => continue,
            };
            if let Some(idx) = self.find_slot_by_hash(hash, &norm[..len]) {
                self.entries[idx].flags |= FLAG_PINNED_SEED | FLAG_STALE_ALLOWED;
            } else if let Some(slot) = self.allocate_slot(true, 0, 0) {
                let mut e = HotRouteEntry::empty();
                e.domain_hash = hash;
                e.hostname = norm;
                e.hostname_len = len as u8;
                e.next_hop_ip = default_gateway;
                e.flags = FLAG_PINNED_SEED | FLAG_STALE_ALLOWED;
                self.entries[slot] = e;
            }
        }

        for &(label, ip) in PINNED_UPSTREAM_SEEDS.iter() {
            let (norm, len, hash) = match normalize_host(label) {
                Some(v) => v,
                None => continue,
            };
            if let Some(idx) = self.find_slot_by_hash(hash, &norm[..len]) {
                self.entries[idx].flags |= FLAG_PINNED_SEED | FLAG_STALE_ALLOWED;
                if self.entries[idx].dst_ip == [0, 0, 0, 0] {
                    self.entries[idx].dst_ip = ip;
                }
            } else if let Some(slot) = self.allocate_slot(true, 0, 0) {
                let mut e = HotRouteEntry::empty();
                e.domain_hash = hash;
                e.hostname = norm;
                e.hostname_len = len as u8;
                e.dst_ip = ip;
                e.next_hop_ip = default_gateway;
                // Aguarda 1a resolucao ARP real confirmar next_hop_mac antes de marcar FLAG_VALID
                e.flags = FLAG_PINNED_SEED | FLAG_STALE_ALLOWED;
                self.entries[slot] = e;
            }
        }
        self.initialized = true;
    }

    fn find_slot_by_hash(&self, hash: u64, norm: &[u8]) -> Option<usize> {
        for (i, e) in self.entries.iter().enumerate() {
            if e.hostname_len as usize == norm.len()
                && e.domain_hash == hash
                && &e.hostname[..norm.len()] == norm
            {
                return Some(i);
            }
        }
        None
    }

    /// Seleciona slot livre ou vítima LFU/LRU, preservando entradas `FLAG_PINNED_SEED`.
    fn allocate_slot(&mut self, allow_replace_non_pinned: bool, now_tick: u64, now_unix: u64) -> Option<usize> {
        for (i, e) in self.entries.iter().enumerate() {
            if !e.is_occupied() {
                return Some(i);
            }
        }
        if !allow_replace_non_pinned {
            return None;
        }

        // 1. Prefere entrada expirada não-fixada com menor contagem de hits
        let mut best_expired: Option<(usize, u32, u64)> = None;
        for (i, e) in self.entries.iter().enumerate() {
            if e.is_pinned() {
                continue;
            }
            if !e.is_valid() || e.is_expired(now_tick, now_unix) {
                let replace = match best_expired {
                    None => true,
                    Some((_, min_hits, oldest_tick)) => {
                        e.hit_count < min_hits || (e.hit_count == min_hits && e.last_used_tick < oldest_tick)
                    }
                };
                if replace {
                    best_expired = Some((i, e.hit_count, e.last_used_tick));
                }
            }
        }
        if let Some((idx, _, _)) = best_expired {
            return Some(idx);
        }

        // 2. Evicção LFU/LRU estrita apenas entre rotas não-fixadas
        let mut best_lfu: Option<(usize, u32, u64)> = None;
        for (i, e) in self.entries.iter().enumerate() {
            if e.is_pinned() {
                continue;
            }
            let replace = match best_lfu {
                None => true,
                Some((_, min_hits, oldest_tick)) => {
                    e.hit_count < min_hits || (e.hit_count == min_hits && e.last_used_tick < oldest_tick)
                }
            };
            if replace {
                best_lfu = Some((i, e.hit_count, e.last_used_tick));
            }
        }
        best_lfu.map(|(idx, _, _)| idx)
    }

    pub fn lookup(&mut self, name: &str, now_tick: u64, now_unix: u64) -> Option<ResolvedRoute> {
        let (norm, len, hash) = normalize_host(name)?;
        let idx = self.find_slot_by_hash(hash, &norm[..len])?;
        let e = &mut self.entries[idx];
        if !e.is_valid() {
            return None;
        }
        if e.is_expired(now_tick, now_unix) {
            // Avaliação preguiçosa: invalida resolução expirada preservando hit_count/PINNED para renovação
            e.flags &= !FLAG_VALID;
            self.dirty = true;
            return None;
        }
        e.hit_count = e.hit_count.saturating_add(1);
        e.last_used_tick = now_tick;
        self.fast_hits = self.fast_hits.saturating_add(1);
        self.dirty = true;
        Some(ResolvedRoute {
            dst_ip: e.dst_ip,
            next_hop_ip: e.next_hop_ip,
            next_hop_mac: e.next_hop_mac,
            hit_count: e.hit_count,
            last_rtt_ticks: e.last_rtt_ticks,
            stale: false,
        })
    }

    pub fn lookup_stale(&mut self, name: &str) -> Option<ResolvedRoute> {
        let (norm, len, hash) = normalize_host(name)?;
        let idx = self.find_slot_by_hash(hash, &norm[..len])?;
        let e = &mut self.entries[idx];
        if !e.stale_allowed() || e.dst_ip == [0, 0, 0, 0] || e.next_hop_mac == [0u8; 6] {
            return None;
        }
        e.hit_count = e.hit_count.saturating_add(1);
        self.stale_hits = self.stale_hits.saturating_add(1);
        self.dirty = true;
        Some(ResolvedRoute {
            dst_ip: e.dst_ip,
            next_hop_ip: e.next_hop_ip,
            next_hop_mac: e.next_hop_mac,
            hit_count: e.hit_count,
            last_rtt_ticks: e.last_rtt_ticks,
            stale: true,
        })
    }

    pub fn lookup_mac_for_ip(&self, dst_ip: [u8; 4], expected_next_hop: [u8; 4], now_tick: u64, now_unix: u64) -> Option<[u8; 6]> {
        if dst_ip == [0, 0, 0, 0] {
            return None;
        }
        for e in self.entries.iter() {
            if !e.is_occupied() || !e.is_valid() {
                continue;
            }
            if e.is_expired(now_tick, now_unix) {
                continue;
            }
            if e.dst_ip == dst_ip && e.next_hop_ip == expected_next_hop && e.next_hop_mac != [0u8; 6] {
                return Some(e.next_hop_mac);
            }
        }
        None
    }

    /// Insere ou atualiza rota resolvida; retorna `(inserted_or_changed, new_hit_count)`.
    pub fn record_resolved(
        &mut self,
        name: &str,
        dst_ip: [u8; 4],
        ttl_secs: u32,
        next_hop_ip: [u8; 4],
        next_hop_mac: [u8; 6],
        rtt_ticks: u32,
        now_tick: u64,
        now_unix: u64,
        pin: bool,
    ) -> (bool, u32) {
        let (norm, len, hash) = match normalize_host(name) {
            Some(v) => v,
            None => return (false, 0),
        };
        if dst_ip == [0, 0, 0, 0] || next_hop_mac == [0u8; 6] {
            return (false, 0);
        }

        let clamped_ttl = clamp_ttl_secs(ttl_secs);
        let exp_tick = now_tick.saturating_add((clamped_ttl as u64) * 100);
        let exp_unix = if now_unix > 1_700_000_000 {
            now_unix.saturating_add(clamped_ttl as u64)
        } else {
            0
        };

        if let Some(idx) = self.find_slot_by_hash(hash, &norm[..len]) {
            let e = &mut self.entries[idx];
            let was_valid = e.is_valid();
            let endpoint_changed = !was_valid
                || e.dst_ip != dst_ip
                || e.next_hop_ip != next_hop_ip
                || e.next_hop_mac != next_hop_mac;
            e.dst_ip = dst_ip;
            e.next_hop_ip = next_hop_ip;
            e.next_hop_mac = next_hop_mac;
            e.hit_count = e.hit_count.saturating_add(1);
            if rtt_ticks > 0 {
                e.last_rtt_ticks = if e.last_rtt_ticks == 0 {
                    rtt_ticks
                } else {
                    ((e.last_rtt_ticks as u64 * 3 + rtt_ticks as u64) / 4) as u32
                };
            }
            e.expires_at_tick = exp_tick;
            e.expires_at_unix = exp_unix;
            e.last_used_tick = now_tick;
            e.flags |= FLAG_VALID | FLAG_STALE_ALLOWED;
            if pin {
                e.flags |= FLAG_PINNED_SEED;
            }
            self.dirty = true;
            return (endpoint_changed, e.hit_count);
        }

        if let Some(slot) = self.allocate_slot(true, now_tick, now_unix) {
            let mut e = HotRouteEntry::empty();
            e.domain_hash = hash;
            e.hostname = norm;
            e.hostname_len = len as u8;
            e.dst_ip = dst_ip;
            e.next_hop_ip = next_hop_ip;
            e.next_hop_mac = next_hop_mac;
            e.hit_count = 1;
            e.last_rtt_ticks = rtt_ticks;
            e.expires_at_tick = exp_tick;
            e.expires_at_unix = exp_unix;
            e.last_used_tick = now_tick;
            e.flags = FLAG_VALID | FLAG_STALE_ALLOWED | if pin { FLAG_PINNED_SEED } else { 0 };
            self.entries[slot] = e;
            self.dirty = true;
            return (true, 1);
        }

        (false, 0)
    }

    pub fn update_rtt_for_ip(&mut self, dst_ip: [u8; 4], next_hop_mac: [u8; 6], rtt_ticks: u32, now_tick: u64) -> bool {
        let mut updated = false;
        for e in self.entries.iter_mut() {
            if !e.is_occupied() || !e.is_valid() {
                continue;
            }
            if e.dst_ip == dst_ip {
                if next_hop_mac != [0u8; 6] && e.next_hop_mac != next_hop_mac {
                    e.next_hop_mac = next_hop_mac;
                }
                if rtt_ticks > 0 {
                    e.last_rtt_ticks = rtt_ticks;
                }
                e.last_used_tick = now_tick;
                self.dirty = true;
                updated = true;
            }
        }
        updated
    }

    pub fn flush_dynamic(&mut self) -> usize {
        let mut removed = 0usize;
        for e in self.entries.iter_mut() {
            if !e.is_occupied() {
                continue;
            }
            if e.is_pinned() {
                // Mantém semente fixa, apenas zera contadores/resolução dinâmica se desejar ou preserva?
                // O flush limpa o histórico dinâmico preservando as rotas PINNED_SEED.
                continue;
            }
            *e = HotRouteEntry::empty();
            removed += 1;
        }
        if removed > 0 {
            self.dirty = true;
        }
        removed
    }

    pub fn invalidate(&mut self, name: &str) -> bool {
        let (norm, len, hash) = match normalize_host(name) {
            Some(v) => v,
            None => return false,
        };
        if let Some(idx) = self.find_slot_by_hash(hash, &norm[..len]) {
            self.entries[idx].flags &= !FLAG_VALID;
            self.entries[idx].expires_at_tick = 0;
            self.entries[idx].expires_at_unix = 0;
            self.dirty = true;
            return true;
        }
        false
    }

    pub fn snapshot_sorted(&self, out: &mut [HotRouteEntryUser], now_tick: u64, now_unix: u64) -> usize {
        if out.is_empty() {
            return 0;
        }
        let mut selected = [false; MAX_HOT_ROUTES];
        let mut written = 0usize;

        while written < out.len() && written < MAX_HOT_ROUTES {
            let mut best_idx: Option<usize> = None;
            for i in 0..MAX_HOT_ROUTES {
                if selected[i] || !self.entries[i].is_occupied() {
                    continue;
                }
                let cand = &self.entries[i];
                let better = match best_idx {
                    None => true,
                    Some(bi) => {
                        let cur = &self.entries[bi];
                        let cand_valid = cand.is_valid() && !cand.is_expired(now_tick, now_unix);
                        let cur_valid = cur.is_valid() && !cur.is_expired(now_tick, now_unix);
                        if cand.hit_count != cur.hit_count {
                            cand.hit_count > cur.hit_count
                        } else if cand_valid != cur_valid {
                            cand_valid
                        } else if cand.is_pinned() != cur.is_pinned() {
                            cand.is_pinned()
                        } else {
                            cand.last_used_tick > cur.last_used_tick
                        }
                    }
                };
                if better {
                    best_idx = Some(i);
                }
            }

            let idx = match best_idx {
                Some(i) => i,
                None => break,
            };
            selected[idx] = true;
            let e = &self.entries[idx];
            let expired = e.is_valid() && e.is_expired(now_tick, now_unix);
            let rem_secs = if !e.is_valid() || expired {
                0
            } else if now_unix > 1_700_000_000 && e.expires_at_unix > now_unix {
                (e.expires_at_unix - now_unix).min(u32::MAX as u64) as u32
            } else if e.expires_at_tick > now_tick {
                ((e.expires_at_tick - now_tick) / 100).min(u32::MAX as u64) as u32
            } else {
                0
            };

            let mut u = HotRouteEntryUser::empty();
            u.hit_count = e.hit_count;
            u.last_rtt_ticks = e.last_rtt_ticks;
            u.ttl_remaining_secs = rem_secs;
            u.hostname = e.hostname;
            u.hostname_len = e.hostname_len;
            u.dst_ip = e.dst_ip;
            u.next_hop_ip = e.next_hop_ip;
            u.next_hop_mac = e.next_hop_mac;
            u.flags = e.flags;
            u.valid = if e.is_valid() && !expired { 1 } else { 0 };
            u.stale = if expired && e.stale_allowed() && e.dst_ip != [0, 0, 0, 0] { 1 } else { 0 };
            out[written] = u;
            written += 1;
        }
        written
    }
}

static HOT_ROUTE_TABLE: IrqSafeSpinLock<HotRouteTable> = IrqSafeSpinLock::new(HotRouteTable::empty());
static HOT_IO_LOCK: AtomicBool = AtomicBool::new(false);
static mut HOT_ROUTES_IO_BUF: [u8; HOT_DB_MAX_BYTES] = [0u8; HOT_DB_MAX_BYTES];
static mut SELFTEST_HOT_TABLE: HotRouteTable = HotRouteTable::empty();

struct HotIoGuard;
impl HotIoGuard {
    fn acquire() -> Self {
        while HOT_IO_LOCK.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        Self
    }
}
impl Drop for HotIoGuard {
    fn drop(&mut self) {
        HOT_IO_LOCK.store(false, Ordering::Release);
    }
}

#[inline]
pub fn clamp_ttl_secs(ttl: u32) -> u32 {
    if ttl == 0 {
        MIN_HOT_TTL_SECS
    } else {
        ttl.clamp(MIN_HOT_TTL_SECS, MAX_HOT_TTL_SECS)
    }
}

pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

pub fn normalize_host(name: &str) -> Option<([u8; MAX_HOST_LEN], usize, u64)> {
    let trimmed = name.trim().trim_end_matches('.');
    if trimmed.is_empty() || trimmed.len() > MAX_HOST_LEN {
        return None;
    }
    let mut out = [0u8; MAX_HOST_LEN];
    let bytes = trimmed.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        let lower = if b.is_ascii_uppercase() { b + 32 } else { b };
        if !(lower.is_ascii_alphanumeric() || lower == b'.' || lower == b'-' || lower == b'_') {
            return None;
        }
        out[i] = lower;
    }
    let hash = fnv1a64(&out[..bytes.len()]);
    Some((out, bytes.len(), hash))
}

fn current_default_gateway() -> [u8; 4] {
    let gw = unsafe { crate::net::dhcp::DHCP_CLIENT.gateway };
    if gw != [0, 0, 0, 0] {
        gw
    } else {
        [10, 0, 2, 2]
    }
}

/// Inicializa `HotRouteTable` carregando `/disk/hot_routes.db` e garantindo sementes do ecossistema.
pub fn init_hot_routes() {
    let loaded = load_hot_routes_from_vfs();
    let gw = current_default_gateway();
    {
        let mut tbl = HOT_ROUTE_TABLE.lock();
        tbl.ensure_default_seeds(gw);
    }
    if !loaded {
        crate::serial_write(b"[NET-FASTROUTE] Sementes iniciais configuradas (PINNED_SEED x8)\n");
    }
}

/// Busca Fast-Path unificada (Hostname -> IPv4 + Next-Hop IP + Next-Hop MAC) sem pacotes DNS/ARP.
pub fn lookup_fast_route(name: &str) -> Option<ResolvedRoute> {
    let now_tick = crate::timer::ticks();
    let now_unix = crate::drivers::rtc::current_unix_timestamp();
    let res = {
        let mut tbl = HOT_ROUTE_TABLE.lock();
        tbl.lookup(name, now_tick, now_unix)
    };
    if let Some(r) = res {
        log_fast_hit(name, &r);
    }
    res
}

/// Busca rápida MAC L2 para envio IPv4/TCP/UDP sem disparo ARP bloqueante.
pub fn lookup_mac_for_ip(dst_ip: [u8; 4]) -> Option<[u8; 6]> {
    let expected_nh = crate::net::ipv4::get_next_hop_ip(dst_ip);
    let now_tick = crate::timer::ticks();
    let now_unix = crate::drivers::rtc::current_unix_timestamp();
    let tbl = HOT_ROUTE_TABLE.lock();
    tbl.lookup_mac_for_ip(dst_ip, expected_nh, now_tick, now_unix)
}

/// Fallback Stale-If-Error caso o servidor DNS upstream falhe após expiração.
pub fn lookup_stale_route(name: &str) -> Option<ResolvedRoute> {
    let res = {
        let mut tbl = HOT_ROUTE_TABLE.lock();
        tbl.lookup_stale(name)
    };
    if let Some(_r) = res {
        crate::serial_write(b"[NET-DNS] Serving stale hot-route IP for '");
        crate::serial_write(name.as_bytes());
        crate::serial_write(b"'\n");
    }
    res
}

/// Registra rota confirmada após DNS + ARP. Persiste `/disk/hot_routes.db` somente se houver
/// inserção ou mudança de IPv4/MAC (e `persist == true`).
pub fn record_route_resolved(
    name: &str,
    dst_ip: [u8; 4],
    ttl_secs: u32,
    next_hop_ip: [u8; 4],
    next_hop_mac: [u8; 6],
    rtt_ticks: u32,
    persist: bool,
) {
    let now_tick = crate::timer::ticks();
    let now_unix = crate::drivers::rtc::current_unix_timestamp();
    let (changed, hits) = {
        let mut tbl = HOT_ROUTE_TABLE.lock();
        tbl.record_resolved(
            name,
            dst_ip,
            ttl_secs,
            next_hop_ip,
            next_hop_mac,
            rtt_ticks,
            now_tick,
            now_unix,
            false,
        )
    };
    if changed {
        log_route_learned(name, dst_ip, next_hop_mac, hits, rtt_ticks);
        if persist {
            save_hot_routes_to_vfs();
        }
    }
}

/// Atualiza o MAC real confirmado das sementes de upstream DNS (`1.1.1.1`, `8.8.8.8`).
pub fn confirm_gateway_seed(dst_ip: [u8; 4], next_hop_ip: [u8; 4], next_hop_mac: [u8; 6]) {
    let mut seed_name: Option<&str> = None;
    for &(label, ip) in PINNED_UPSTREAM_SEEDS.iter() {
        if ip == dst_ip {
            seed_name = Some(label);
            break;
        }
    }
    if let Some(name) = seed_name {
        let now_tick = crate::timer::ticks();
        let now_unix = crate::drivers::rtc::current_unix_timestamp();
        let (changed, _) = {
            let mut tbl = HOT_ROUTE_TABLE.lock();
            tbl.record_resolved(
                name,
                dst_ip,
                MAX_HOT_TTL_SECS,
                next_hop_ip,
                next_hop_mac,
                0,
                now_tick,
                now_unix,
                true,
            )
        };
        if changed {
            save_hot_routes_to_vfs();
        }
    }
}

/// Atualiza amostra de RTT TCP ao concluir 3-way handshake sem gerar I/O síncrono de disco.
pub fn record_connect_rtt(dst_ip: [u8; 4], next_hop_mac: [u8; 6], rtt_ticks: u32) {
    let now_tick = crate::timer::ticks();
    let mut tbl = HOT_ROUTE_TABLE.lock();
    let _ = tbl.update_rtt_for_ip(dst_ip, next_hop_mac, rtt_ticks, now_tick);
}

/// Fixa (PIN) um domínio na tabela de rotas quentes, associando IP resolvido ou explícito.
pub fn pin_hot_route(name: &str, explicit_ip: Option<[u8; 4]>) -> bool {
    if normalize_host(name).is_none() {
        return false;
    }

    let resolved_ip = match explicit_ip {
        Some(ip) if ip != [0, 0, 0, 0] => Some(ip),
        _ => crate::net::dns::resolve(name).ok(),
    };

    let now_tick = crate::timer::ticks();
    let now_unix = crate::drivers::rtc::current_unix_timestamp();

    if let Some(ip) = resolved_ip {
        let nh_ip = crate::net::ipv4::get_next_hop_ip(ip);
        let nh_mac = lookup_mac_for_ip(ip).unwrap_or_else(|| crate::net::arp::resolve_mac_for_ip(ip));
        let (changed_or_ok, _) = {
            let mut tbl = HOT_ROUTE_TABLE.lock();
            tbl.record_resolved(name, ip, MAX_HOT_TTL_SECS, nh_ip, nh_mac, 0, now_tick, now_unix, true)
        };
        let _ = changed_or_ok;
        save_hot_routes_to_vfs();
        return true;
    }

    // Caso offline ou ainda sem IP: cria/marca semente PINNED_SEED
    let (norm, len, hash) = match normalize_host(name) {
        Some(v) => v,
        None => return false,
    };
    let gw = current_default_gateway();
    {
        let mut tbl = HOT_ROUTE_TABLE.lock();
        if let Some(idx) = tbl.find_slot_by_hash(hash, &norm[..len]) {
            tbl.entries[idx].flags |= FLAG_PINNED_SEED | FLAG_STALE_ALLOWED;
            tbl.dirty = true;
        } else if let Some(slot) = tbl.allocate_slot(true, now_tick, now_unix) {
            let mut e = HotRouteEntry::empty();
            e.domain_hash = hash;
            e.hostname = norm;
            e.hostname_len = len as u8;
            e.next_hop_ip = gw;
            e.flags = FLAG_PINNED_SEED | FLAG_STALE_ALLOWED;
            tbl.entries[slot] = e;
            tbl.dirty = true;
        } else {
            return false;
        }
    }
    save_hot_routes_to_vfs();
    true
}

/// Remove rotas dinâmicas da RAM e persiste o estado limpo em `/disk/hot_routes.db`,
/// preservando rotas fixadas (`FLAG_PINNED_SEED`).
pub fn flush_dynamic_routes() -> usize {
    let removed = {
        let mut tbl = HOT_ROUTE_TABLE.lock();
        tbl.flush_dynamic()
    };
    save_hot_routes_to_vfs();
    removed
}

pub fn invalidate_host(name: &str) {
    let mut tbl = HOT_ROUTE_TABLE.lock();
    let _ = tbl.invalidate(name);
}

/// Copia as Top-N entradas ordenadas decrescentemente por `hit_count`.
pub fn snapshot_top_routes(out: &mut [HotRouteEntryUser]) -> usize {
    let now_tick = crate::timer::ticks();
    let now_unix = crate::drivers::rtc::current_unix_timestamp();
    let tbl = HOT_ROUTE_TABLE.lock();
    tbl.snapshot_sorted(out, now_tick, now_unix)
}

/// Salva `/disk/hot_routes.db` somente se houver alterações pendentes em memória (`dirty`).
pub fn save_if_dirty() {
    let is_dirty = {
        let tbl = HOT_ROUTE_TABLE.lock();
        tbl.dirty
    };
    if is_dirty {
        save_hot_routes_to_vfs();
    }
}

/// Persiste `HotRouteTable` em `/disk/hot_routes.db` sem reter o SpinLock durante I/O VFS.
pub fn save_hot_routes_to_vfs() {
    let _io_guard = HotIoGuard::acquire();
    let out = unsafe { &mut *core::ptr::addr_of_mut!(HOT_ROUTES_IO_BUF) };
    let now_tick = crate::timer::ticks();
    let now_unix = crate::drivers::rtc::current_unix_timestamp();

    let mut count = 0u32;
    let mut pos = 16usize;

    {
        let mut tbl = HOT_ROUTE_TABLE.lock();
        for e in tbl.entries.iter() {
            if !e.is_occupied() {
                continue;
            }
            if pos + HOT_RECORD_WIRE_SIZE + 8 > out.len() {
                break;
            }
            let (exp_unix, rem_secs) = if !e.is_valid() {
                (0u64, 0u64)
            } else if now_unix > 1_700_000_000 && e.expires_at_unix > now_unix {
                (e.expires_at_unix, e.expires_at_unix - now_unix)
            } else if e.expires_at_tick > now_tick {
                let rem = (e.expires_at_tick - now_tick) / 100;
                let eu = if now_unix > 1_700_000_000 { now_unix.saturating_add(rem) } else { 0 };
                (eu, rem)
            } else {
                (e.expires_at_unix, 0u64)
            };

            out[pos] = e.hostname_len;
            pos += 1;
            out[pos..pos + MAX_HOST_LEN].copy_from_slice(&e.hostname);
            pos += MAX_HOST_LEN;
            out[pos..pos + 4].copy_from_slice(&e.dst_ip);
            pos += 4;
            out[pos..pos + 4].copy_from_slice(&e.next_hop_ip);
            pos += 4;
            out[pos..pos + 6].copy_from_slice(&e.next_hop_mac);
            pos += 6;
            out[pos..pos + 4].copy_from_slice(&e.hit_count.to_le_bytes());
            pos += 4;
            out[pos..pos + 4].copy_from_slice(&e.last_rtt_ticks.to_le_bytes());
            pos += 4;
            out[pos..pos + 8].copy_from_slice(&exp_unix.to_le_bytes());
            pos += 8;
            out[pos..pos + 8].copy_from_slice(&rem_secs.to_le_bytes());
            pos += 8;
            out[pos] = e.flags;
            pos += 1;
            count += 1;
        }
        tbl.dirty = false;
    }

    out[0..8].copy_from_slice(&HOT_DB_MAGIC);
    out[8..12].copy_from_slice(&HOT_DB_VERSION.to_le_bytes());
    out[12..16].copy_from_slice(&count.to_le_bytes());

    let csum = fnv1a64(&out[..pos]);
    out[pos..pos + 8].copy_from_slice(&csum.to_le_bytes());
    pos += 8;

    let _ = crate::vfs::create(HOT_ROUTES_DB_PATH);
    if let Ok(handle) = crate::vfs::open(HOT_ROUTES_DB_PATH, crate::vfs::O_WRONLY | crate::vfs::O_TRUNC) {
        let _ = crate::vfs::write(HOT_ROUTES_DB_PATH, handle, 0, &out[..pos]);
        let _ = crate::vfs::close(HOT_ROUTES_DB_PATH, handle);
        let _ = crate::vfs::sync_all();
    }
}

/// Carrega `/disk/hot_routes.db` na inicialização, restaurando hits, RTT, IPv4, MAC e TTL válidos.
pub fn load_hot_routes_from_vfs() -> bool {
    let _io_guard = HotIoGuard::acquire();
    let buf = unsafe { &mut *core::ptr::addr_of_mut!(HOT_ROUTES_IO_BUF) };
    let mut read_len = 0usize;

    if let Ok(handle) = crate::vfs::open(HOT_ROUTES_DB_PATH, crate::vfs::O_RDONLY) {
        if let Ok(n) = crate::vfs::read(HOT_ROUTES_DB_PATH, handle, 0, buf) {
            read_len = n;
        }
        let _ = crate::vfs::close(HOT_ROUTES_DB_PATH, handle);
    }

    if read_len < 24 || &buf[0..8] != &HOT_DB_MAGIC {
        return false;
    }
    let version = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
    if version != HOT_DB_VERSION {
        return false;
    }
    let count = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]) as usize;
    let payload_end = 16usize.saturating_add(count.saturating_mul(HOT_RECORD_WIRE_SIZE));
    if payload_end + 8 > read_len {
        return false;
    }

    let expected_csum = u64::from_le_bytes(buf[payload_end..payload_end + 8].try_into().unwrap_or([0; 8]));
    let actual_csum = fnv1a64(&buf[..payload_end]);
    if expected_csum != actual_csum {
        crate::serial_write(b"[NET-FASTROUTE] /disk/hot_routes.db corrompido (checksum mismatch), ignorando\n");
        return false;
    }

    let now_tick = crate::timer::ticks();
    let now_unix = crate::drivers::rtc::current_unix_timestamp();
    let mut restored_valid = 0usize;
    let mut restored_total = 0usize;

    {
        let mut tbl = HOT_ROUTE_TABLE.lock();
        let mut pos = 16usize;
        for _ in 0..count {
            let hlen = (buf[pos] as usize).min(MAX_HOST_LEN);
            pos += 1;
            let mut host_arr = [0u8; MAX_HOST_LEN];
            host_arr.copy_from_slice(&buf[pos..pos + MAX_HOST_LEN]);
            pos += MAX_HOST_LEN;
            let dst_ip: [u8; 4] = buf[pos..pos + 4].try_into().unwrap_or([0; 4]);
            pos += 4;
            let next_hop_ip: [u8; 4] = buf[pos..pos + 4].try_into().unwrap_or([0; 4]);
            pos += 4;
            let next_hop_mac: [u8; 6] = buf[pos..pos + 6].try_into().unwrap_or([0; 6]);
            pos += 6;
            let hit_count = u32::from_le_bytes(buf[pos..pos + 4].try_into().unwrap_or([0; 4]));
            pos += 4;
            let last_rtt_ticks = u32::from_le_bytes(buf[pos..pos + 4].try_into().unwrap_or([0; 4]));
            pos += 4;
            let exp_unix = u64::from_le_bytes(buf[pos..pos + 8].try_into().unwrap_or([0; 8]));
            pos += 8;
            let rem_secs = u64::from_le_bytes(buf[pos..pos + 8].try_into().unwrap_or([0; 8]));
            pos += 8;
            let mut flags = buf[pos];
            pos += 1;

            if hlen == 0 {
                continue;
            }
            let name_str = match core::str::from_utf8(&host_arr[..hlen]) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let (norm, norm_len, hash) = match normalize_host(name_str) {
                Some(v) => v,
                None => continue,
            };

            let valid_time = if now_unix > 1_700_000_000 && exp_unix > 1_700_000_000 {
                exp_unix > now_unix
            } else {
                rem_secs > 0
            };

            let effective_rem = if now_unix > 1_700_000_000 && exp_unix > now_unix {
                exp_unix - now_unix
            } else if valid_time {
                rem_secs
            } else {
                0
            };

            if !valid_time && (flags & FLAG_VALID) != 0 {
                // Mantém entrada para Stale-If-Error / hits preservados, mas exige revalidação
                flags &= !FLAG_VALID;
            }

            if (flags & FLAG_PINNED_SEED) == 0 && (flags & FLAG_VALID) == 0 && hit_count == 0 {
                continue;
            }

            if let Some(slot) = tbl.allocate_slot(true, now_tick, now_unix) {
                let mut e = HotRouteEntry::empty();
                e.domain_hash = hash;
                e.hostname = norm;
                e.hostname_len = norm_len as u8;
                e.dst_ip = dst_ip;
                e.next_hop_ip = next_hop_ip;
                e.next_hop_mac = next_hop_mac;
                e.hit_count = hit_count;
                e.last_rtt_ticks = last_rtt_ticks;
                e.expires_at_unix = exp_unix;
                e.expires_at_tick = if (flags & FLAG_VALID) != 0 {
                    now_tick.saturating_add(effective_rem.max(1) * 100)
                } else {
                    0
                };
                e.last_used_tick = now_tick;
                e.flags = flags;
                tbl.entries[slot] = e;
                restored_total += 1;
                if e.is_valid() {
                    restored_valid += 1;
                }
            }
        }
        tbl.initialized = true;
        tbl.dirty = false;
    }

    crate::serial_write(b"[NET-FASTROUTE] HotRouteTable restaurada de /disk/hot_routes.db (total=");
    serial_write_u64(restored_total as u64);
    crate::serial_write(b", valid=");
    serial_write_u64(restored_valid as u64);
    crate::serial_write(b")\n");
    restored_total > 0
}

fn serial_write_ip(ip: [u8; 4]) {
    for i in 0..4 {
        if i > 0 { crate::serial_write(b"."); }
        serial_write_u64(ip[i] as u64);
    }
}

fn serial_write_mac(mac: [u8; 6]) {
    let hex = b"0123456789abcdef";
    for (i, &b) in mac.iter().enumerate() {
        if i > 0 { crate::serial_write(b":"); }
        crate::serial_write(&[hex[(b >> 4) as usize], hex[(b & 0x0F) as usize]]);
    }
}

fn serial_write_u64(mut v: u64) {
    if v == 0 {
        crate::serial_write(b"0");
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 0;
    while v > 0 {
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        i += 1;
    }
    buf[..i].reverse();
    crate::serial_write(&buf[..i]);
}

fn log_fast_hit(name: &str, r: &ResolvedRoute) {
    crate::serial_write(b"[NET-FASTROUTE] HIT '");
    crate::serial_write(name.as_bytes());
    crate::serial_write(b"' -> ");
    serial_write_ip(r.dst_ip);
    crate::serial_write(b" (hits=");
    serial_write_u64(r.hit_count as u64);
    crate::serial_write(b")\n");
}

fn log_route_learned(name: &str, dst_ip: [u8; 4], mac: [u8; 6], hits: u32, rtt_ticks: u32) {
    crate::serial_write(b"[NET-FASTROUTE] LEARNED '");
    crate::serial_write(name.as_bytes());
    crate::serial_write(b"' -> ");
    serial_write_ip(dst_ip);
    crate::serial_write(b" (MAC ");
    serial_write_mac(mac);
    crate::serial_write(b", hits=");
    serial_write_u64(hits as u64);
    crate::serial_write(b", rtt=");
    serial_write_u64((rtt_ticks as u64) * 10);
    crate::serial_write(b"ms)\n");
}

/// Auto-testes unitários em `.bss` estático (sem estourar pilha do kernel) para validar `HotRouteTable`.
pub fn run_hot_route_selftests() -> (usize, usize) {
    let tbl = unsafe { &mut *core::ptr::addr_of_mut!(SELFTEST_HOT_TABLE) };
    let mut passed = 0usize;
    let total = 10usize;

    let gw = [10, 0, 2, 2];
    let gw_mac = [0x52, 0x55, 0x0a, 0x00, 0x02, 0x02];

    // 1. Semeadura das 8 sementes padrão (PINNED_SEED)
    tbl.clear_all();
    tbl.ensure_default_seeds(gw);
    let mut count_pinned = 0usize;
    for e in tbl.entries.iter() {
        if e.is_occupied() && e.is_pinned() {
            count_pinned += 1;
        }
    }
    if count_pinned == 8 { passed += 1; }

    // 2. Normalização case-insensitive + FNV-1a e TTL clamp mínimo (5s -> 120s)
    tbl.record_resolved("EXAMPLE.COM.", [93, 184, 215, 14], 5, gw, gw_mac, 2, 1000, 1_800_000_000, false);
    if let Some(r) = tbl.lookup("example.com", 1000 + 11_000, 1_800_000_110) {
        if r.dst_ip == [93, 184, 215, 14] && r.hit_count == 2 && r.next_hop_mac == gw_mac {
            passed += 1;
        }
    }

    // 3. Expiração preguiçosa (Zero-Idle) após 120s
    if tbl.lookup("example.com", 1000 + 12_500, 1_800_000_125).is_none() {
        passed += 1;
    }

    // 4. Fallback Stale-If-Error após expiração preserva IP/MAC
    if let Some(stale) = tbl.lookup_stale("example.com") {
        if stale.stale && stale.dst_ip == [93, 184, 215, 14] && stale.next_hop_mac == gw_mac {
            passed += 1;
        }
    }

    // 5. Clamp máximo de TTL (999_999s -> 86_400s)
    if clamp_ttl_secs(999_999) == 86_400 && clamp_ttl_secs(0) == 120 {
        passed += 1;
    }

    // 6. Fast-path MAC lookup por IPv4 destino
    tbl.record_resolved("68k.news", [50, 116, 1, 2], 600, gw, gw_mac, 3, 2000, 1_800_001_000, false);
    if tbl.lookup_mac_for_ip([50, 116, 1, 2], gw, 2050, 1_800_001_001) == Some(gw_mac) {
        passed += 1;
    }

    // 7. Evicção LFU respeita entradas PINNED_SEED
    tbl.clear_all();
    tbl.ensure_default_seeds(gw);
    // Garante IP/MAC válido numa pinned
    tbl.record_resolved("jinn-os.dev.br", [185, 199, 108, 153], 3600, gw, gw_mac, 1, 100, 1_800_000_000, true);
    // Preenche os 56 slots restantes com entradas dinâmicas (hits variados)
    let mut name_buf = [0u8; 24];
    for i in 0..(MAX_HOT_ROUTES - 8) {
        let b0 = b'a' + ((i / 26) as u8);
        let b1 = b'a' + ((i % 26) as u8);
        let suffix = b".test.org";
        name_buf[0] = b0;
        name_buf[1] = b1;
        name_buf[2..11].copy_from_slice(suffix);
        let s = core::str::from_utf8(&name_buf[..11]).unwrap_or("aa.test.org");
        tbl.record_resolved(s, [198, 51, 100, (i as u8).saturating_add(1)], 600, gw, gw_mac, 2, 100 + i as u64, 1_800_000_000, false);
        // Dá hits extras para entradas >= 1, deixando idx 0 com hit=1 (LFU vítima)
        if i > 0 {
            let _ = tbl.lookup(s, 200, 1_800_000_001);
        }
    }
    // Nova inserção deve expulsar "aa.test.org" (menor hit_count=1) e NUNCA uma PINNED_SEED
    tbl.record_resolved("novosite.net", [203, 0, 113, 9], 600, gw, gw_mac, 4, 500, 1_800_000_005, false);
    let evicted_aa = tbl.lookup("aa.test.org", 500, 1_800_000_005).is_none();
    let kept_pinned = tbl.lookup("jinn-os.dev.br", 500, 1_800_000_005).is_some();
    let inserted_new = tbl.lookup("novosite.net", 500, 1_800_000_005).is_some();
    if evicted_aa && kept_pinned && inserted_new {
        passed += 1;
    }

    // 8. Ordenação descendente Top-N por hit_count
    tbl.clear_all();
    tbl.record_resolved("low.org", [1, 2, 3, 4], 300, gw, gw_mac, 2, 100, 1_800_000_000, false);
    tbl.record_resolved("top.org", [5, 6, 7, 8], 300, gw, gw_mac, 1, 100, 1_800_000_000, true);
    let _ = tbl.lookup("top.org", 101, 1_800_000_000);
    let _ = tbl.lookup("top.org", 102, 1_800_000_000);
    let mut snap = [HotRouteEntryUser::empty(); 4];
    let n_snap = tbl.snapshot_sorted(&mut snap, 102, 1_800_000_000);
    if n_snap == 2 && snap[0].hit_count == 3 && snap[1].hit_count == 1 {
        passed += 1;
    }

    // 9. Flush dinâmico preserva rotas fixadas (PINNED_SEED)
    let removed = tbl.flush_dynamic();
    let rem_pinned = tbl.lookup("top.org", 103, 1_800_000_000).is_some();
    let rem_dyn = tbl.lookup("low.org", 103, 1_800_000_000).is_none();
    if removed == 1 && rem_pinned && rem_dyn {
        passed += 1;
    }

    // 10. Rejeição de hostnames inválidos
    if normalize_host("").is_none() && normalize_host("bad host/name").is_none() {
        passed += 1;
    }

    tbl.clear_all();
    (passed, total)
}
