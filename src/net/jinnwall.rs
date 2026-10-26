//! Jinn OS — Firewall Stateful Packet Inspection (JinnWall SPI)
//!
//! Filtra e inspeciona pacotes nas camadas L2, L3 e L4 de forma nativa e
//! independente da interface gráfica.

use core::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering};

pub const MAX_CONNTRACK: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnState {
    New,
    Established,
    Related,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterVerdict {
    Accept,
    Drop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirewallProfile {
    /// Bloqueia todo tráfego de entrada não solicitado (Padrão de Segurança).
    PublicSecure = 0,
    /// Permite tráfego local de desenvolvimento restrito (portas 8080 e 3000).
    Development = 1,
}

impl FirewallProfile {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::PublicSecure => "PublicSecure",
            Self::Development => "Development",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ConntrackEntry {
    pub src_ip: [u8; 4],
    pub dst_ip: [u8; 4],
    pub src_port: u16,
    pub dst_port: u16,
    pub protocol: u8,
    pub state: ConnState,
    pub last_seen_tick: u64,
}

pub struct JinnWall {
    pub profile_atomic: AtomicU8,
    pub table: [Option<ConntrackEntry>; MAX_CONNTRACK],
    pub packets_inspected: AtomicU64,
    pub packets_accepted: AtomicU64,
    pub packets_dropped: AtomicU64,
    pub port_scans_blocked: AtomicU32,
}

pub static mut JINNWALL: JinnWall = JinnWall {
    profile_atomic: AtomicU8::new(0), // 0 = PublicSecure
    table: [None; MAX_CONNTRACK],
    packets_inspected: AtomicU64::new(0),
    packets_accepted: AtomicU64::new(0),
    packets_dropped: AtomicU64::new(0),
    port_scans_blocked: AtomicU32::new(0),
};

impl JinnWall {
    pub fn profile(&self) -> FirewallProfile {
        match self.profile_atomic.load(Ordering::SeqCst) {
            1 => FirewallProfile::Development,
            _ => FirewallProfile::PublicSecure,
        }
    }

    pub fn set_profile(&self, prof: FirewallProfile) {
        let val = match prof {
            FirewallProfile::PublicSecure => 0,
            FirewallProfile::Development => 1,
        };
        self.profile_atomic.store(val, Ordering::SeqCst);
    }

    pub fn toggle_profile(&self) -> FirewallProfile {
        let cur = self.profile_atomic.load(Ordering::SeqCst);
        let next = if cur == 0 { 1 } else { 0 };
        self.profile_atomic.store(next, Ordering::SeqCst);
        if next == 1 {
            FirewallProfile::Development
        } else {
            FirewallProfile::PublicSecure
        }
    }

    /// Inspeciona pacote de entrada (INPUT)
    pub fn filter_in(
        &mut self,
        src_ip: [u8; 4],
        dst_ip: [u8; 4],
        protocol: u8,
        src_port: u16,
        dst_port: u16,
        current_tick: u64,
    ) -> FilterVerdict {
        self.packets_inspected.fetch_add(1, Ordering::Relaxed);

        // 1. Defesa Anti-Spoofing e Martian Addresses:
        // - Origem 0.0.0.0 ou 255.255.255.255 (broadcast)
        // - Origem 127.0.0.0/8 (Loopback martiano em interface física externa)
        // - Origem Multicast ou Classe E (224.0.0.0 a 255.255.255.255)
        // - Origem idêntica ao destino (LAND attack / reflexão)
        if src_ip == [0, 0, 0, 0]
            || src_ip == [255, 255, 255, 255]
            || src_ip[0] == 127
            || src_ip[0] >= 224
            || src_ip == dst_ip
        {
            self.packets_dropped.fetch_add(1, Ordering::Relaxed);
            return FilterVerdict::Drop;
        }

        // 2. Tráfego ICMP Echo Reply ou ARP é sempre permitido
        if protocol == crate::net::ipv4::PROTO_ICMP {
            self.packets_accepted.fetch_add(1, Ordering::Relaxed);
            return FilterVerdict::Accept;
        }

        // 3. DHCP Client (porta 68) recebendo de DHCP Server (porta 67) é permitido
        if protocol == crate::net::ipv4::PROTO_UDP && dst_port == 68 && src_port == 67 {
            self.packets_accepted.fetch_add(1, Ordering::Relaxed);
            return FilterVerdict::Accept;
        }

        // 4. Stateful Inspection: Verifica tabela de Conntrack
        // Se este pacote é resposta a uma conexão que nós iniciamos (dst_port == nossa src_port e dst_ip == nosso IP)
        for entry_opt in self.table.iter_mut() {
            if let Some(entry) = entry_opt {
                // Expira entradas antigas (> 60 segundos = 6000 ticks)
                if current_tick.saturating_sub(entry.last_seen_tick) > 6000 {
                    *entry_opt = None;
                    continue;
                }

                // Match de conexão de retorno (respostas a pacotes que enviamos)
                if entry.src_ip == dst_ip && entry.dst_ip == src_ip
                    && entry.src_port == dst_port && entry.dst_port == src_port
                    && entry.protocol == protocol
                {
                    entry.state = ConnState::Established;
                    entry.last_seen_tick = current_tick;
                    self.packets_accepted.fetch_add(1, Ordering::Relaxed);
                    return FilterVerdict::Accept;
                }
            }
        }

        // 5. Se perfil for Development e porta for autorizada explicitamente (8080 ou 3000):
        if self.profile() == FirewallProfile::Development {
            if dst_port == 8080 || dst_port == 3000 {
                self.packets_accepted.fetch_add(1, Ordering::Relaxed);
                return FilterVerdict::Accept;
            }
        }

        // 6. Política Padrão: DROP para conexões de entrada não solicitadas
        self.packets_dropped.fetch_add(1, Ordering::Relaxed);
        FilterVerdict::Drop
    }

    /// Registra pacote de saída (OUTPUT) na tabela Conntrack com suporte a evicção LRU e expiração
    pub fn track_out(
        &mut self,
        src_ip: [u8; 4],
        dst_ip: [u8; 4],
        protocol: u8,
        src_port: u16,
        dst_port: u16,
        current_tick: u64,
    ) {
        // 1. Se fluxo já existe, atualiza last_seen_tick
        for entry_opt in self.table.iter_mut() {
            if let Some(entry) = entry_opt {
                if entry.src_ip == src_ip && entry.dst_ip == dst_ip
                    && entry.src_port == src_port && entry.dst_port == dst_port
                    && entry.protocol == protocol
                {
                    entry.last_seen_tick = current_tick;
                    return;
                }
            }
        }

        // 2. Procura primeiro slot livre
        for entry_opt in self.table.iter_mut() {
            if entry_opt.is_none() {
                *entry_opt = Some(ConntrackEntry {
                    src_ip,
                    dst_ip,
                    src_port,
                    dst_port,
                    protocol,
                    state: ConnState::New,
                    last_seen_tick: current_tick,
                });
                return;
            }
        }

        // 3. Tabela cheia: tenta expirar entradas antigas (> 6000 ticks)
        for entry_opt in self.table.iter_mut() {
            if let Some(entry) = entry_opt {
                if current_tick.saturating_sub(entry.last_seen_tick) > 6000 {
                    *entry_opt = Some(ConntrackEntry {
                        src_ip,
                        dst_ip,
                        src_port,
                        dst_port,
                        protocol,
                        state: ConnState::New,
                        last_seen_tick: current_tick,
                    });
                    return;
                }
            }
        }

        // 4. Saturação sob alta carga: evicção LRU determinística da entrada mais antiga
        let mut oldest_idx = 0;
        let mut oldest_tick = u64::MAX;
        for (i, entry_opt) in self.table.iter().enumerate() {
            if let Some(entry) = entry_opt {
                if entry.last_seen_tick < oldest_tick {
                    oldest_tick = entry.last_seen_tick;
                    oldest_idx = i;
                }
            }
        }
        self.table[oldest_idx] = Some(ConntrackEntry {
            src_ip,
            dst_ip,
            src_port,
            dst_port,
            protocol,
            state: ConnState::New,
            last_seen_tick: current_tick,
        });
    }

    /// Retorna a quantidade atual de fluxos ativos na tabela de conntrack
    pub fn active_connections(&self) -> usize {
        let mut count = 0;
        for e in self.table.iter() {
            if e.is_some() { count += 1; }
        }
        count
    }

    /// Retorna a quantidade total de pacotes inspecionados
    pub fn inspected(&self) -> u64 {
        self.packets_inspected.load(Ordering::Relaxed)
    }

    /// Retorna a quantidade total de pacotes aceitos
    pub fn accepted(&self) -> u64 {
        self.packets_accepted.load(Ordering::Relaxed)
    }

    /// Retorna a quantidade total de pacotes bloqueados (DROP)
    pub fn drops(&self) -> u64 {
        self.packets_dropped.load(Ordering::Relaxed)
    }
}

// ─── SUÍTE DE AUTO-TESTES DO JINNWALL (EXECUÇÃO NO BOOT DO KERNEL) ───────────

/// Executa suíte de auto-testes das políticas e estado do firewall JinnWall.
/// Retorna `(aprovados, total)`.
pub fn run_jinnwall_selftests() -> (usize, usize) {
    let mut passed = 0;
    let total = 12;

    let mut jw = JinnWall {
        profile_atomic: AtomicU8::new(0),
        table: [None; MAX_CONNTRACK],
        packets_inspected: AtomicU64::new(0),
        packets_accepted: AtomicU64::new(0),
        packets_dropped: AtomicU64::new(0),
        port_scans_blocked: AtomicU32::new(0),
    };

    // 1. Perfil inicial deve ser PublicSecure
    if jw.profile() == FirewallProfile::PublicSecure && jw.profile().name() == "PublicSecure" {
        passed += 1;
    }

    // 2. Alternância atômica de perfil
    let p_dev = jw.toggle_profile();
    let p_sec = jw.toggle_profile();
    if p_dev == FirewallProfile::Development && p_sec == FirewallProfile::PublicSecure {
        passed += 1;
    }

    // 3. Em PublicSecure, tráfego de entrada não solicitado para porta 8080 deve ser DROPPED
    jw.set_profile(FirewallProfile::PublicSecure);
    let v_drop = jw.filter_in([192, 168, 1, 50], [10, 0, 2, 15], crate::net::ipv4::PROTO_TCP, 54321, 8080, 100);
    if v_drop == FilterVerdict::Drop {
        passed += 1;
    }

    // 4. Em Development, tráfego para porta 8080 deve ser ACCEPTED
    jw.set_profile(FirewallProfile::Development);
    let v_dev_8080 = jw.filter_in([192, 168, 1, 50], [10, 0, 2, 15], crate::net::ipv4::PROTO_TCP, 54321, 8080, 100);
    if v_dev_8080 == FilterVerdict::Accept {
        passed += 1;
    }

    // 5. Em Development, porta 3000 deve ser ACCEPTED, mas porta 9999 deve ser DROPPED
    let v_dev_3000 = jw.filter_in([192, 168, 1, 50], [10, 0, 2, 15], crate::net::ipv4::PROTO_TCP, 54321, 3000, 100);
    let v_dev_9999 = jw.filter_in([192, 168, 1, 50], [10, 0, 2, 15], crate::net::ipv4::PROTO_TCP, 54321, 9999, 100);
    if v_dev_3000 == FilterVerdict::Accept && v_dev_9999 == FilterVerdict::Drop {
        passed += 1;
    }

    // 6. Conntrack: fluxo de saída registrado aceita pacote de retorno em PublicSecure
    jw.set_profile(FirewallProfile::PublicSecure);
    jw.track_out([10, 0, 2, 15], [93, 184, 216, 34], crate::net::ipv4::PROTO_UDP, 53535, 53, 500);
    assert_eq!(jw.active_connections(), 1);
    let v_ret = jw.filter_in([93, 184, 216, 34], [10, 0, 2, 15], crate::net::ipv4::PROTO_UDP, 53, 53535, 505);
    if v_ret == FilterVerdict::Accept {
        passed += 1;
    }

    // 7. Defesa Anti-Spoofing Básica: origem 0.0.0.0 ou 255.255.255.255 é sempre DROPPED
    let v_spoof1 = jw.filter_in([0, 0, 0, 0], [10, 0, 2, 15], crate::net::ipv4::PROTO_TCP, 1234, 80, 600);
    let v_spoof2 = jw.filter_in([255, 255, 255, 255], [10, 0, 2, 15], crate::net::ipv4::PROTO_TCP, 1234, 80, 600);
    if v_spoof1 == FilterVerdict::Drop && v_spoof2 == FilterVerdict::Drop {
        passed += 1;
    }

    // 8. Defesa Anti-Spoofing Avançada: loopback 127.0.0.1 e multicast 224.0.0.1 na interface externa são DROPPED
    let v_loopback = jw.filter_in([127, 0, 0, 1], [10, 0, 2, 15], crate::net::ipv4::PROTO_UDP, 53, 53535, 700);
    let v_multicast = jw.filter_in([224, 0, 0, 1], [10, 0, 2, 15], crate::net::ipv4::PROTO_UDP, 53, 53535, 700);
    if v_loopback == FilterVerdict::Drop && v_multicast == FilterVerdict::Drop {
        passed += 1;
    }

    // 9. Defesa Anti-LAND Attack: origem idêntica ao destino é rejeitada
    let v_land = jw.filter_in([10, 0, 2, 15], [10, 0, 2, 15], crate::net::ipv4::PROTO_TCP, 80, 80, 800);
    if v_land == FilterVerdict::Drop {
        passed += 1;
    }

    // 10. Conntrack Expiration: conexões com mais de 60s (6000 ticks) de inatividade expiram
    let v_expired = jw.filter_in([93, 184, 216, 34], [10, 0, 2, 15], crate::net::ipv4::PROTO_UDP, 53, 53535, 500 + 6001);
    if v_expired == FilterVerdict::Drop {
        passed += 1;
    }

    // 11. Conntrack Saturation e Evicção LRU sem panic
    for i in 0..MAX_CONNTRACK {
        jw.track_out([10, 0, 2, 15], [192, 168, 1, (i + 1) as u8], crate::net::ipv4::PROTO_TCP, 30000 + (i as u16), 80, 1000 + (i as u64));
    }
    if jw.active_connections() == MAX_CONNTRACK {
        // Insere a 65ª conexão (evicta a mais antiga com tick 1000)
        jw.track_out([10, 0, 2, 15], [172, 16, 0, 1], crate::net::ipv4::PROTO_TCP, 45000, 443, 2000);
        if jw.active_connections() == MAX_CONNTRACK {
            passed += 1;
        }
    }

    // 12. Contadores de telemetria refletem exatamente os pacotes processados
    if jw.inspected() > 0 && jw.accepted() > 0 && jw.drops() > 0 {
        if jw.inspected() == jw.accepted() + jw.drops() {
            passed += 1;
        }
    }

    (passed, total)
}
