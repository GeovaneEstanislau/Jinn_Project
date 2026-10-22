//! Jinn OS — Firewall Stateful Packet Inspection (JinnWall SPI)
//!
//! Filtra e inspeciona pacotes nas camadas L2, L3 e L4 de forma nativa e
//! independente da interface gráfica.

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

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
    PublicSecure,
    /// Permite tráfego local de desenvolvimento (ex: portas 8080, 3000).
    Development,
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
    pub profile: FirewallProfile,
    pub table: [Option<ConntrackEntry>; MAX_CONNTRACK],
    pub packets_inspected: AtomicU64,
    pub packets_accepted: AtomicU64,
    pub packets_dropped: AtomicU64,
    pub port_scans_blocked: AtomicU32,
}

pub static mut JINNWALL: JinnWall = JinnWall {
    profile: FirewallProfile::PublicSecure,
    table: [None; MAX_CONNTRACK],
    packets_inspected: AtomicU64::new(0),
    packets_accepted: AtomicU64::new(0),
    packets_dropped: AtomicU64::new(0),
    port_scans_blocked: AtomicU32::new(0),
};

impl JinnWall {
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

        // 1. Defesa Anti-Spoofing: origem 0.0.0.0 ou 255.255.255.255 é descartada
        if src_ip == [0, 0, 0, 0] || src_ip == [255, 255, 255, 255] {
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
                // Expira entradas antigas (> 60 segundos)
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

        // 5. Se perfil for Development e porta for autorizada:
        if self.profile == FirewallProfile::Development {
            if dst_port == 8080 || dst_port == 3000 {
                self.packets_accepted.fetch_add(1, Ordering::Relaxed);
                return FilterVerdict::Accept;
            }
        }

        // 6. Política Padrão: DROP para conexões de entrada não solicitadas
        self.packets_dropped.fetch_add(1, Ordering::Relaxed);
        FilterVerdict::Drop
    }

    /// Registra pacote de saída (OUTPUT) na tabela Conntrack
    pub fn track_out(
        &mut self,
        src_ip: [u8; 4],
        dst_ip: [u8; 4],
        protocol: u8,
        src_port: u16,
        dst_port: u16,
        current_tick: u64,
    ) {
        // Atualiza ou insere na tabela Conntrack
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
    }

    /// Retorna a quantidade atual de fluxos ativos na tabela de conntrack
    pub fn active_connections(&self) -> usize {
        let mut count = 0;
        for e in self.table.iter() {
            if e.is_some() { count += 1; }
        }
        count
    }

    /// Retorna a quantidade total de pacotes bloqueados (DROP)
    pub fn drops(&self) -> u64 {
        self.packets_dropped.load(core::sync::atomic::Ordering::Relaxed)
    }
}
