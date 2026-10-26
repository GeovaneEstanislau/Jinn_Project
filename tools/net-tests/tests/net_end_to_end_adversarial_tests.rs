//! Suíte de Testes Host de Ponta a Ponta para Rede e Firewall JinnWall SPI (Issue #43)
//!
//! Valida:
//! 1. Roteamento de Entrada por Portas e Isolamento Estrito de Sockets
//! 2. Defesa Anti-Spoofing Completa (0.0.0.0, 255.255.255.255, 127.0.0.0/8, Multicast/Classe E, LAND attack)
//! 3. Saturação, Evicção LRU e Expiração na Tabela Conntrack
//! 4. Robustez de Buffers contra Pacotes Truncados e Malformados (L2/L3/L4)
//! 5. Dinâmica de Perfis JinnWall (PublicSecure vs Development) e Invariantes de Telemetria

use std::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering};

// ─── CONSTANTES E ESTRUTURAS ESPELHADAS DO KERNEL ───────────────────────────

pub const MAX_CONNTRACK: usize = 64;
pub const MAX_SOCKETS: usize = 16;
pub const SOCK_RX_BUF_SIZE: usize = 1024;

pub const PROTO_ICMP: u8 = 1;
pub const PROTO_TCP:  u8 = 6;
pub const PROTO_UDP:  u8 = 17;

pub const SOCK_STREAM: u32 = 1;
pub const SOCK_DGRAM:  u32 = 2;

pub const EBADF:      isize = -9;
pub const EINVAL:     isize = -22;
pub const EMFILE:     isize = -24;
pub const EPERM:      isize = -1;
pub const EADDRINUSE: isize = -98;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterVerdict {
    Accept,
    Drop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirewallProfile {
    PublicSecure = 0,
    Development = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnState {
    New,
    Established,
    Related,
    Invalid,
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

pub struct TestJinnWall {
    pub profile_atomic: AtomicU8,
    pub table: [Option<ConntrackEntry>; MAX_CONNTRACK],
    pub packets_inspected: AtomicU64,
    pub packets_accepted: AtomicU64,
    pub packets_dropped: AtomicU64,
    pub port_scans_blocked: AtomicU32,
}

impl TestJinnWall {
    pub fn new(profile: FirewallProfile) -> Self {
        let p_val = match profile {
            FirewallProfile::PublicSecure => 0,
            FirewallProfile::Development => 1,
        };
        Self {
            profile_atomic: AtomicU8::new(p_val),
            table: [None; MAX_CONNTRACK],
            packets_inspected: AtomicU64::new(0),
            packets_accepted: AtomicU64::new(0),
            packets_dropped: AtomicU64::new(0),
            port_scans_blocked: AtomicU32::new(0),
        }
    }

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
        if protocol == PROTO_ICMP {
            self.packets_accepted.fetch_add(1, Ordering::Relaxed);
            return FilterVerdict::Accept;
        }

        // 3. DHCP Client (porta 68) recebendo de DHCP Server (porta 67) é permitido
        if protocol == PROTO_UDP && dst_port == 68 && src_port == 67 {
            self.packets_accepted.fetch_add(1, Ordering::Relaxed);
            return FilterVerdict::Accept;
        }

        // 4. Stateful Inspection: Verifica tabela de Conntrack
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

    pub fn active_connections(&self) -> usize {
        self.table.iter().filter(|e| e.is_some()).count()
    }

    pub fn inspected(&self) -> u64 {
        self.packets_inspected.load(Ordering::Relaxed)
    }

    pub fn accepted(&self) -> u64 {
        self.packets_accepted.load(Ordering::Relaxed)
    }

    pub fn drops(&self) -> u64 {
        self.packets_dropped.load(Ordering::Relaxed)
    }
}

// ─── ESTRUTURAS DE SOCKET PARA TESTES DE ROTEAMENTO EXCLUSIVO ───────────────

#[derive(Clone, Copy)]
pub struct TestSocket {
    pub owner_pid: usize,
    pub sock_type: u32,
    pub local_port: u16,
    pub remote_ip: [u8; 4],
    pub remote_port: u16,
    pub is_bound: bool,
    pub is_connected: bool,

    pub rx_buf: [[u8; SOCK_RX_BUF_SIZE]; 4],
    pub rx_len: [usize; 4],
    pub rx_head: usize,
    pub rx_tail: usize,
}

impl TestSocket {
    pub const fn empty() -> Self {
        Self {
            owner_pid: 0,
            sock_type: 0,
            local_port: 0,
            remote_ip: [0; 4],
            remote_port: 0,
            is_bound: false,
            is_connected: false,
            rx_buf: [[0; SOCK_RX_BUF_SIZE]; 4],
            rx_len: [0; 4],
            rx_head: 0,
            rx_tail: 0,
        }
    }

    pub fn push_rx(&mut self, data: &[u8]) -> bool {
        let next_head = (self.rx_head + 1) % 4;
        if next_head == self.rx_tail {
            return false;
        }
        let len = data.len().min(SOCK_RX_BUF_SIZE);
        self.rx_buf[self.rx_head][..len].copy_from_slice(&data[..len]);
        self.rx_len[self.rx_head] = len;
        self.rx_head = next_head;
        true
    }

    pub fn pop_rx(&mut self, dest: &mut [u8]) -> Option<usize> {
        if self.rx_head == self.rx_tail {
            return None;
        }
        let len = self.rx_len[self.rx_tail].min(dest.len());
        dest[..len].copy_from_slice(&self.rx_buf[self.rx_tail][..len]);
        self.rx_tail = (self.rx_tail + 1) % 4;
        Some(len)
    }
}

pub struct TestSocketTable {
    pub slots: [Option<TestSocket>; MAX_SOCKETS],
}

impl TestSocketTable {
    pub const fn new() -> Self {
        Self {
            slots: [None; MAX_SOCKETS],
        }
    }

    pub fn create_socket(&mut self, pid: usize, sock_type: u32) -> Result<usize, isize> {
        for (idx, slot) in self.slots.iter_mut().enumerate() {
            if slot.is_none() {
                let mut sock = TestSocket::empty();
                sock.owner_pid = pid;
                sock.sock_type = sock_type;
                *slot = Some(sock);
                return Ok(idx);
            }
        }
        Err(EMFILE)
    }

    pub fn bind_socket(&mut self, pid: usize, sockfd: usize, local_port: u16) -> Result<(), isize> {
        if local_port == 0 {
            return Err(EINVAL);
        }
        // Validação de porta já em uso (anti-colisão)
        for (idx, slot) in self.slots.iter().enumerate() {
            if idx != sockfd {
                if let Some(s) = slot {
                    if s.local_port == local_port && s.is_bound {
                        return Err(EADDRINUSE);
                    }
                }
            }
        }
        let sock = match self.slots.get_mut(sockfd).and_then(|s| s.as_mut()) {
            Some(s) => s,
            None => return Err(EBADF),
        };
        if sock.owner_pid != pid {
            return Err(EPERM);
        }
        sock.local_port = local_port;
        sock.is_bound = true;
        Ok(())
    }

    pub fn connect_socket(&mut self, pid: usize, sockfd: usize, remote_ip: [u8; 4], remote_port: u16) -> Result<(), isize> {
        let sock = match self.slots.get_mut(sockfd).and_then(|s| s.as_mut()) {
            Some(s) => s,
            None => return Err(EBADF),
        };
        if sock.owner_pid != pid {
            return Err(EPERM);
        }
        sock.remote_ip = remote_ip;
        sock.remote_port = remote_port;
        if sock.local_port == 0 {
            sock.local_port = 40000 + (sockfd as u16 * 100);
        }
        sock.is_connected = true;
        Ok(())
    }

    pub fn dispatch_incoming_packet(&mut self, proto: u8, src_ip: [u8; 4], src_port: u16, dst_port: u16, payload: &[u8]) -> bool {
        if dst_port == 0 {
            return false;
        }
        let sock_type = if proto == PROTO_TCP { SOCK_STREAM } else { SOCK_DGRAM };
        for slot in self.slots.iter_mut() {
            if let Some(ref mut sock) = slot {
                if sock.sock_type == sock_type && sock.local_port != 0 && sock.local_port == dst_port {
                    if !sock.is_connected || (sock.remote_ip == src_ip && (sock.remote_port == src_port || sock.remote_port == 0)) {
                        return sock.push_rx(payload);
                    }
                }
            }
        }
        false
    }
}

// ─── PARSERS L2/L3/L4 DEFENSIVOS PARA TESTES DE ROBUSTEZ ────────────────────

pub fn parse_ethernet(bytes: &[u8]) -> Result<(&[u8], &[u8], u16, &[u8]), ()> {
    if bytes.len() < 14 {
        return Err(());
    }
    let ethertype = u16::from_be_bytes([bytes[12], bytes[13]]);
    Ok((&bytes[0..6], &bytes[6..12], ethertype, &bytes[14..]))
}

pub fn parse_ipv4(bytes: &[u8]) -> Result<([u8; 4], [u8; 4], u8, &[u8]), ()> {
    if bytes.len() < 20 {
        return Err(());
    }
    let ver = bytes[0] >> 4;
    let ihl = (bytes[0] & 0x0F) as usize * 4;
    if ver != 4 || ihl < 20 || bytes.len() < ihl {
        return Err(());
    }
    let total_len = u16::from_be_bytes([bytes[2], bytes[3]]) as usize;
    if total_len < ihl {
        return Err(());
    }
    let packet_len = total_len.min(bytes.len());
    let mut src = [0u8; 4];
    let mut dst = [0u8; 4];
    src.copy_from_slice(&bytes[12..16]);
    dst.copy_from_slice(&bytes[16..20]);
    let proto = bytes[9];
    Ok((src, dst, proto, &bytes[ihl..packet_len]))
}

pub fn parse_udp(bytes: &[u8]) -> Result<(u16, u16, &[u8]), ()> {
    if bytes.len() < 8 {
        return Err(());
    }
    let src_port = u16::from_be_bytes([bytes[0], bytes[1]]);
    let dst_port = u16::from_be_bytes([bytes[2], bytes[3]]);
    let len = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
    if len < 8 {
        return Err(());
    }
    let payload_len = len - 8;
    let available = bytes.len() - 8;
    let end = 8 + payload_len.min(available);
    Ok((src_port, dst_port, &bytes[8..end]))
}

pub fn parse_tcp(bytes: &[u8]) -> Result<(u16, u16, u8, &[u8]), ()> {
    if bytes.len() < 20 {
        return Err(());
    }
    let src_port = u16::from_be_bytes([bytes[0], bytes[1]]);
    let dst_port = u16::from_be_bytes([bytes[2], bytes[3]]);
    let doff = ((bytes[12] >> 4) as usize) * 4;
    let flags = bytes[13] & 0x3F;
    if doff < 20 || bytes.len() < doff {
        return Err(());
    }
    Ok((src_port, dst_port, flags, &bytes[doff..]))
}

// ═════════════════════════════════════════════════════════════════════════════
// SUÍTE DE TESTES INTEGRADOS DE REDE E FIREWALL
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_anti_spoofing_martian_addresses_and_land_attack() {
    let mut fw = TestJinnWall::new(FirewallProfile::PublicSecure);
    let our_ip = [10, 0, 2, 15];

    // 1. Origem 0.0.0.0 fora de DHCP deve ser rejeitada
    let v_zero = fw.filter_in([0, 0, 0, 0], our_ip, PROTO_TCP, 1234, 8080, 100);
    assert_eq!(v_zero, FilterVerdict::Drop, "Origem 0.0.0.0 deve ser descartada");

    // 2. Origem Broadcast 255.255.255.255 deve ser rejeitada
    let v_bcast = fw.filter_in([255, 255, 255, 255], our_ip, PROTO_UDP, 5000, 5000, 101);
    assert_eq!(v_bcast, FilterVerdict::Drop, "Origem 255.255.255.255 deve ser descartada");

    // 3. Loopback martiano vindo da interface externa (127.0.0.1 e 127.10.20.30)
    let v_loop1 = fw.filter_in([127, 0, 0, 1], our_ip, PROTO_TCP, 80, 40000, 102);
    let v_loop2 = fw.filter_in([127, 254, 1, 99], our_ip, PROTO_UDP, 53, 53535, 103);
    assert_eq!(v_loop1, FilterVerdict::Drop, "Loopback 127.0.0.1 externo deve ser descartado");
    assert_eq!(v_loop2, FilterVerdict::Drop, "Loopback 127.254.1.99 externo deve ser descartado");

    // 4. Origem Multicast (224.0.0.1) ou Classe E (240.0.0.1)
    let v_multi = fw.filter_in([224, 0, 0, 1], our_ip, PROTO_UDP, 5353, 5353, 104);
    let v_classe_e = fw.filter_in([240, 1, 2, 3], our_ip, PROTO_TCP, 1000, 2000, 105);
    assert_eq!(v_multi, FilterVerdict::Drop, "Multicast de origem deve ser descartado");
    assert_eq!(v_classe_e, FilterVerdict::Drop, "Classe E de origem deve ser descartada");

    // 5. LAND attack: origem idêntica ao destino (our_ip == our_ip)
    let v_land = fw.filter_in(our_ip, our_ip, PROTO_TCP, 40000, 40000, 106);
    assert_eq!(v_land, FilterVerdict::Drop, "LAND attack com src == dst deve ser descartado");

    // Invariante de observabilidade: todos os 7 pacotes adversariais foram inspecionados e descartados
    assert_eq!(fw.inspected(), 7);
    assert_eq!(fw.drops(), 7);
    assert_eq!(fw.accepted(), 0);
}

#[test]
fn test_strict_port_routing_and_socket_isolation() {
    let mut table = TestSocketTable::new();

    // Cria três sockets
    let s1 = table.create_socket(1, SOCK_DGRAM).expect("Socket 1 deve ser criado");
    let s2 = table.create_socket(1, SOCK_DGRAM).expect("Socket 2 deve ser criado");
    let s3 = table.create_socket(1, SOCK_STREAM).expect("Socket 3 deve ser criado");

    // Associa portas distintas
    table.bind_socket(1, s1, 5000).expect("Bind s1 na porta 5000");
    table.bind_socket(1, s2, 6000).expect("Bind s2 na porta 6000");

    // Tentar reusar porta 5000 no socket 3 deve falhar com EADDRINUSE
    let bind_conflict = table.bind_socket(1, s3, 5000);
    assert_eq!(bind_conflict, Err(EADDRINUSE), "Porta 5000 já em uso deve retornar EADDRINUSE");

    // Conecta s3 a um servidor remoto
    table.connect_socket(1, s3, [10, 0, 2, 2], 80).expect("Connect s3");
    let s3_local_port = table.slots[s3].unwrap().local_port;
    assert!(s3_local_port >= 40000, "Porta efêmera deve ser >= 40000");

    // 1. Pacote UDP destinado à porta 5000
    let payload_s1 = b"Mensagem para porta 5000";
    let dispatched1 = table.dispatch_incoming_packet(PROTO_UDP, [10, 0, 2, 2], 53, 5000, payload_s1);
    assert!(dispatched1, "Pacote para porta 5000 deve ser entregue");

    // 2. Pacote UDP destinado à porta 6000
    let payload_s2 = b"Mensagem para porta 6000";
    let dispatched2 = table.dispatch_incoming_packet(PROTO_UDP, [10, 0, 2, 2], 53, 6000, payload_s2);
    assert!(dispatched2, "Pacote para porta 6000 deve ser entregue");

    // 3. Pacote UDP destinado à porta não registrada (7000)
    let dispatched_orphan = table.dispatch_incoming_packet(PROTO_UDP, [10, 0, 2, 2], 53, 7000, b"Orphan");
    assert!(!dispatched_orphan, "Pacote para porta 7000 sem socket deve ser rejeitado");

    // 4. Pacote para porta 0
    let dispatched_zero = table.dispatch_incoming_packet(PROTO_UDP, [10, 0, 2, 2], 53, 0, b"Zero");
    assert!(!dispatched_zero, "Pacote para porta 0 deve ser rejeitado");

    // Valida que Socket 1 recebeu EXCLUSIVAMENTE seu payload
    let mut rx_buf1 = [0u8; 128];
    let read1 = table.slots[s1].as_mut().unwrap().pop_rx(&mut rx_buf1).expect("S1 tem payload");
    assert_eq!(&rx_buf1[..read1], payload_s1);
    assert_eq!(table.slots[s1].as_mut().unwrap().pop_rx(&mut rx_buf1), None, "Fila de S1 deve estar vazia");

    // Valida que Socket 2 recebeu EXCLUSIVAMENTE seu payload
    let mut rx_buf2 = [0u8; 128];
    let read2 = table.slots[s2].as_mut().unwrap().pop_rx(&mut rx_buf2).expect("S2 tem payload");
    assert_eq!(&rx_buf2[..read2], payload_s2);
    assert_eq!(table.slots[s2].as_mut().unwrap().pop_rx(&mut rx_buf2), None, "Fila de S2 deve estar vazia");

    // 5. Teste de isolamento de conexão em Socket 3 (conectado em 10.0.2.2:80):
    // Pacote vindo de IP forjado [10.0.2.99] para a porta de s3 não deve ser aceito
    let dispatched_spoofed_remote = table.dispatch_incoming_packet(
        PROTO_TCP,
        [10, 0, 2, 99], // IP diferente do configurado na conexão
        80,
        s3_local_port,
        b"Invasao TCP",
    );
    assert!(!dispatched_spoofed_remote, "Socket conectado deve rejeitar tráfego de remote_ip diferente");
}

#[test]
fn test_conntrack_saturation_lru_eviction_and_timeouts() {
    let mut fw = TestJinnWall::new(FirewallProfile::PublicSecure);
    let our_ip = [10, 0, 2, 15];

    // Preenche a tabela até a capacidade total (64 conexões)
    for i in 0..MAX_CONNTRACK {
        let dst_ip = [192, 168, 10, (i + 1) as u8];
        let local_port = 30000 + (i as u16);
        let current_tick = 1000 + (i as u64);
        fw.track_out(our_ip, dst_ip, PROTO_TCP, local_port, 80, current_tick);
    }
    assert_eq!(fw.active_connections(), MAX_CONNTRACK, "Tabela de conntrack deve estar totalmente ocupada");

    // Inserção sob saturação (65ª conexão no tick 2000):
    // Deve aplicar evicção LRU determinística no slot mais antigo (tick 1000, porta 30000)
    let new_dst_ip = [172, 16, 0, 1];
    fw.track_out(our_ip, new_dst_ip, PROTO_TCP, 45000, 443, 2000);
    assert_eq!(fw.active_connections(), MAX_CONNTRACK, "Tabela deve manter 64 conexões sem estourar limites");

    // A nova conexão deve ser aprovada para retorno
    let v_new_ret = fw.filter_in(new_dst_ip, our_ip, PROTO_TCP, 443, 45000, 2005);
    assert_eq!(v_new_ret, FilterVerdict::Accept, "Conexão recente inserida deve ter retorno aceito");

    // A conexão mais antiga evictada (porta 30000 para [192, 168, 10, 1]) agora deve ser DROPPED
    let v_evicted_ret = fw.filter_in([192, 168, 10, 1], our_ip, PROTO_TCP, 80, 30000, 2010);
    assert_eq!(v_evicted_ret, FilterVerdict::Drop, "Conexão evictada não deve mais aceitar retorno em PublicSecure");

    // Teste de expiração por timeout de inatividade (> 60 segundos / 6000 ticks):
    // Conexão criada no tick 1001 tentando responder no tick 7500 (> 6000 ticks depois)
    let v_timed_out = fw.filter_in([192, 168, 10, 2], our_ip, PROTO_TCP, 80, 30001, 7500);
    assert_eq!(v_timed_out, FilterVerdict::Drop, "Conexão inativa por mais de 60s deve expirar e ser descartada");
}

#[test]
fn test_buffer_robustness_against_truncated_frames() {
    // 1. Ethernet truncado (< 14 bytes)
    assert_eq!(parse_ethernet(&[]), Err(()));
    assert_eq!(parse_ethernet(&[0u8; 13]), Err(()));
    let valid_eth = [
        0x52, 0x54, 0x00, 0x12, 0x34, 0x56, // Dst MAC
        0x52, 0x54, 0x00, 0xAA, 0xBB, 0xCC, // Src MAC
        0x08, 0x00,                         // IPv4
        0x45, 0x00,                         // Payload
    ];
    let eth_ok = parse_ethernet(&valid_eth);
    assert!(eth_ok.is_ok());
    let (_, _, et, payload) = eth_ok.unwrap();
    assert_eq!(et, 0x0800);
    assert_eq!(payload.len(), 2);

    // 2. IPv4 truncado (< 20 bytes ou ihl inconsistente)
    assert_eq!(parse_ipv4(&[]), Err(()));
    assert_eq!(parse_ipv4(&[0x45; 19]), Err(()));

    // Versão IPv6 passada para parser IPv4
    let mut ip_v6_raw = [0u8; 20];
    ip_v6_raw[0] = 0x60; // Version 6
    assert_eq!(parse_ipv4(&ip_v6_raw), Err(()));

    // IHL menor que 5 (menos de 20 bytes)
    let mut ip_bad_ihl = [0u8; 20];
    ip_bad_ihl[0] = 0x44; // IHL 4 = 16 bytes
    assert_eq!(parse_ipv4(&ip_bad_ihl), Err(()));

    // Total length mentiroso (declara 1500 bytes mas buffer só tem 20)
    let mut ip_truncated_payload = [0u8; 24];
    ip_truncated_payload[0] = 0x45;
    ip_truncated_payload[2..4].copy_from_slice(&1500u16.to_be_bytes()); // total_len 1500
    let parsed_ip = parse_ipv4(&ip_truncated_payload);
    assert!(parsed_ip.is_ok(), "Deve aceitar e restringir o payload ao buffer real");
    let (_, _, _, ip_pay) = parsed_ip.unwrap();
    assert_eq!(ip_pay.len(), 4, "Payload deve ser limitado a 24 - 20 = 4 bytes, sem out-of-bounds");

    // 3. UDP truncado (< 8 bytes ou length field < 8)
    assert_eq!(parse_udp(&[]), Err(()));
    assert_eq!(parse_udp(&[0u8; 7]), Err(()));

    let mut udp_bad_len = [0u8; 12];
    udp_bad_len[4..6].copy_from_slice(&5u16.to_be_bytes()); // RFC 768: len 5 é inválido
    assert_eq!(parse_udp(&udp_bad_len), Err(()));

    let mut udp_valid = [0u8; 16];
    udp_valid[0..2].copy_from_slice(&53535u16.to_be_bytes());
    udp_valid[2..4].copy_from_slice(&53u16.to_be_bytes());
    udp_valid[4..6].copy_from_slice(&16u16.to_be_bytes()); // total_len 16
    let parsed_udp = parse_udp(&udp_valid).expect("UDP válido");
    assert_eq!(parsed_udp.0, 53535);
    assert_eq!(parsed_udp.1, 53);
    assert_eq!(parsed_udp.2.len(), 8);

    // 4. TCP truncado (< 20 bytes ou data_offset < 5)
    assert_eq!(parse_tcp(&[]), Err(()));
    assert_eq!(parse_tcp(&[0u8; 19]), Err(()));

    let mut tcp_bad_doff = [0u8; 20];
    tcp_bad_doff[12] = 0x40; // Data offset 4 = 16 bytes (< 20)
    assert_eq!(parse_tcp(&tcp_bad_doff), Err(()));

    let mut tcp_valid = [0u8; 24];
    tcp_valid[0..2].copy_from_slice(&40000u16.to_be_bytes());
    tcp_valid[2..4].copy_from_slice(&80u16.to_be_bytes());
    tcp_valid[12] = 0x50; // Data offset 5 = 20 bytes
    tcp_valid[13] = 0x02; // SYN flag
    let parsed_tcp = parse_tcp(&tcp_valid).expect("TCP válido");
    assert_eq!(parsed_tcp.0, 40000);
    assert_eq!(parsed_tcp.1, 80);
    assert_eq!(parsed_tcp.2, 0x02); // SYN
    assert_eq!(parsed_tcp.3.len(), 4);
}

#[test]
fn test_adversarial_traffic_storm_fuzzing() {
    let mut fw = TestJinnWall::new(FirewallProfile::PublicSecure);
    let mut table = TestSocketTable::new();
    let sock = table.create_socket(1, SOCK_DGRAM).expect("Cria socket");
    table.bind_socket(1, sock, 8080).expect("Bind 8080");

    let mut rng_seed: u32 = 0xDEADBEEF;
    let mut next_u32 = || {
        rng_seed = rng_seed.wrapping_mul(1664525).wrapping_add(1013904223);
        rng_seed
    };

    // Submete 5000 pacotes arbitrários com ruído
    let mut packet_buf = [0u8; 256];
    for _ in 0..5000 {
        let p_len = (next_u32() % 256) as usize;
        for b in packet_buf.iter_mut().take(p_len) {
            *b = (next_u32() & 0xFF) as u8;
        }

        // Tenta fazer o parse através das camadas sem provocar panic
        if let Ok((_, _, _, ip_data)) = parse_ethernet(&packet_buf[..p_len]) {
            if let Ok((src_ip, dst_ip, proto, l4_data)) = parse_ipv4(ip_data) {
                let (src_p, dst_p) = match proto {
                    PROTO_UDP => parse_udp(l4_data).map(|u| (u.0, u.1)).unwrap_or((0, 0)),
                    PROTO_TCP => parse_tcp(l4_data).map(|t| (t.0, t.1)).unwrap_or((0, 0)),
                    _ => (0, 0),
                };
                let verdict = fw.filter_in(src_ip, dst_ip, proto, src_p, dst_p, 100);
                if verdict == FilterVerdict::Accept {
                    table.dispatch_incoming_packet(proto, src_ip, src_p, dst_p, l4_data);
                }
            }
        }
    }

    // Invariante de integridade: inspeções == aceitos + descartados
    assert_eq!(fw.inspected(), fw.accepted() + fw.drops());
}

#[test]
fn test_firewall_profile_dynamic_switching_and_isolation() {
    let mut fw = TestJinnWall::new(FirewallProfile::PublicSecure);
    let our_ip = [10, 0, 2, 15];
    let remote_ip = [192, 168, 1, 100];

    // 1. Em PublicSecure: porta 8080 e porta 3000 NÃO solicitadas devem ser DROPPED
    assert_eq!(fw.filter_in(remote_ip, our_ip, PROTO_TCP, 50000, 8080, 10), FilterVerdict::Drop);
    assert_eq!(fw.filter_in(remote_ip, our_ip, PROTO_TCP, 50000, 3000, 11), FilterVerdict::Drop);
    assert_eq!(fw.filter_in(remote_ip, our_ip, PROTO_TCP, 50000, 9999, 12), FilterVerdict::Drop);

    // 2. Alterna para Development: portas 8080 e 3000 passam a ser ACCEPTED
    let next_p = fw.toggle_profile();
    assert_eq!(next_p, FirewallProfile::Development);
    assert_eq!(fw.filter_in(remote_ip, our_ip, PROTO_TCP, 50000, 8080, 13), FilterVerdict::Accept);
    assert_eq!(fw.filter_in(remote_ip, our_ip, PROTO_TCP, 50000, 3000, 14), FilterVerdict::Accept);
    // Portas não autorizadas (ex: 22, 9999) continuam sendo DROPPED mesmo em Development
    assert_eq!(fw.filter_in(remote_ip, our_ip, PROTO_TCP, 50000, 22, 15), FilterVerdict::Drop);
    assert_eq!(fw.filter_in(remote_ip, our_ip, PROTO_TCP, 50000, 9999, 16), FilterVerdict::Drop);

    // 3. Alterna de volta para PublicSecure: portas 8080 e 3000 voltam a ser imediatamente bloqueadas
    let back_p = fw.toggle_profile();
    assert_eq!(back_p, FirewallProfile::PublicSecure);
    assert_eq!(fw.filter_in(remote_ip, our_ip, PROTO_TCP, 50000, 8080, 17), FilterVerdict::Drop);
    assert_eq!(fw.filter_in(remote_ip, our_ip, PROTO_TCP, 50000, 3000, 18), FilterVerdict::Drop);
}
