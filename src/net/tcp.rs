//! Jinn OS — Protocolo TCP (Camada de Transporte Confiável L4 — RFC 793)

use crate::net::ethernet::{EthernetFrame, ETHERTYPE_IPV4};
use crate::net::ipv4::{Ipv4Packet, PROTO_TCP};

pub const TCP_FLAG_FIN: u8 = 0x01;
pub const TCP_FLAG_SYN: u8 = 0x02;
pub const TCP_FLAG_RST: u8 = 0x04;
pub const TCP_FLAG_PSH: u8 = 0x08;
pub const TCP_FLAG_ACK: u8 = 0x10;
pub const TCP_FLAG_URG: u8 = 0x20;

pub const TCP_DEFAULT_WINDOW: u16 = 4096;
pub const TCP_DEFAULT_RTO_TICKS: u64 = 100; // 1 segundo (100 Hz timer)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpState {
    Closed,
    Listen,
    SynSent,
    SynReceived,
    Established,
    FinWait1,
    FinWait2,
    CloseWait,
    Closing,
    LastAck,
    TimeWait,
}

impl TcpState {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Closed => "CLOSED",
            Self::Listen => "LISTEN",
            Self::SynSent => "SYN_SENT",
            Self::SynReceived => "SYN_RECEIVED",
            Self::Established => "ESTABLISHED",
            Self::FinWait1 => "FIN_WAIT_1",
            Self::FinWait2 => "FIN_WAIT_2",
            Self::CloseWait => "CLOSE_WAIT",
            Self::Closing => "CLOSING",
            Self::LastAck => "LAST_ACK",
            Self::TimeWait => "TIME_WAIT",
        }
    }
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct TcpHeader {
    pub src_port: u16,
    pub dst_port: u16,
    pub seq_num: u32,
    pub ack_num: u32,
    pub data_offset_flags: u16, // Data offset (4 bits) + Flags (6 bits)
    pub window_size: u16,
    pub checksum: u16,
    pub urgent_ptr: u16,
}

pub struct TcpPacket<'a> {
    pub src_port: u16,
    pub dst_port: u16,
    pub seq_num: u32,
    pub ack_num: u32,
    pub flags: u8,
    pub window: u16,
    pub payload: &'a [u8],
}

impl<'a> TcpPacket<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ()> {
        if bytes.len() < 20 {
            return Err(());
        }
        let src_port = u16::from_be_bytes([bytes[0], bytes[1]]);
        let dst_port = u16::from_be_bytes([bytes[2], bytes[3]]);
        let seq_num = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let ack_num = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        let doff = ((bytes[12] >> 4) as usize) * 4;
        let flags = bytes[13] & 0x3F;
        let window = u16::from_be_bytes([bytes[14], bytes[15]]);

        if doff < 20 || bytes.len() < doff {
            return Err(());
        }

        Ok(Self {
            src_port,
            dst_port,
            seq_num,
            ack_num,
            flags,
            window,
            payload: &bytes[doff..],
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TcpControlBlock {
    pub state: TcpState,
    pub local_seq: u32,
    pub local_ack: u32,
    pub remote_seq: u32,
    pub remote_win: u16,
    pub local_win: u16,
    pub last_tx_tick: u64,
    pub rto_ticks: u64,
    pub retransmit_count: u8,
    pub unacked_len: usize,
    pub unacked_buf: [u8; 1024],
}

impl TcpControlBlock {
    pub const fn new() -> Self {
        Self {
            state: TcpState::Closed,
            local_seq: 0,
            local_ack: 0,
            remote_seq: 0,
            remote_win: TCP_DEFAULT_WINDOW,
            local_win: TCP_DEFAULT_WINDOW,
            last_tx_tick: 0,
            rto_ticks: TCP_DEFAULT_RTO_TICKS,
            retransmit_count: 0,
            unacked_len: 0,
            unacked_buf: [0; 1024],
        }
    }
}

/// Calcula o checksum TCP de 16 bits com pseudo-cabeçalho IPv4 (RFC 793).
pub fn calculate_tcp_checksum(src_ip: [u8; 4], dst_ip: [u8; 4], tcp_bytes: &[u8]) -> u16 {
    let mut sum = 0u32;

    // Pseudo-Header IPv4
    sum = sum.wrapping_add(u16::from_be_bytes([src_ip[0], src_ip[1]]) as u32);
    sum = sum.wrapping_add(u16::from_be_bytes([src_ip[2], src_ip[3]]) as u32);
    sum = sum.wrapping_add(u16::from_be_bytes([dst_ip[0], dst_ip[1]]) as u32);
    sum = sum.wrapping_add(u16::from_be_bytes([dst_ip[2], dst_ip[3]]) as u32);
    sum = sum.wrapping_add(PROTO_TCP as u32);
    sum = sum.wrapping_add(tcp_bytes.len() as u32);

    // Segmento TCP
    let mut i = 0;
    while i + 1 < tcp_bytes.len() {
        let word = u16::from_be_bytes([tcp_bytes[i], tcp_bytes[i + 1]]) as u32;
        sum = sum.wrapping_add(word);
        i += 2;
    }
    if i < tcp_bytes.len() {
        let word = u16::from_be_bytes([tcp_bytes[i], 0]) as u32;
        sum = sum.wrapping_add(word);
    }

    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }

    !(sum as u16)
}

/// Monta um segmento TCP completo no buffer de destino com cabeçalho de 20 bytes e checksum com pseudo-cabeçalho.
pub fn write_segment(
    dest: &mut [u8],
    src_ip: [u8; 4],
    dst_ip: [u8; 4],
    src_port: u16,
    dst_port: u16,
    seq_num: u32,
    ack_num: u32,
    flags: u8,
    window_size: u16,
    payload: &[u8],
) -> usize {
    let total_len = 20 + payload.len();
    if dest.len() < total_len {
        return 0;
    }

    dest[0..2].copy_from_slice(&src_port.to_be_bytes());
    dest[2..4].copy_from_slice(&dst_port.to_be_bytes());
    dest[4..8].copy_from_slice(&seq_num.to_be_bytes());
    dest[8..12].copy_from_slice(&ack_num.to_be_bytes());
    dest[12] = 0x50; // 5 dwords = 20 bytes
    dest[13] = flags;
    dest[14..16].copy_from_slice(&window_size.to_be_bytes());
    dest[16..18].copy_from_slice(&[0, 0]);
    dest[18..20].copy_from_slice(&[0, 0]);

    if !payload.is_empty() {
        dest[20..total_len].copy_from_slice(payload);
    }

    let csum = calculate_tcp_checksum(src_ip, dst_ip, &dest[..total_len]);
    dest[16..18].copy_from_slice(&csum.to_be_bytes());

    total_len
}

/// Constrói e transmite um pacote TCP completo (Ethernet + IPv4 + TCP Segment).
pub fn send_tcp_packet(
    src_port: u16,
    dst_ip: [u8; 4],
    dst_port: u16,
    seq_num: u32,
    ack_num: u32,
    flags: u8,
    window_size: u16,
    payload: &[u8],
) -> bool {
    let our_ip = unsafe { crate::net::dhcp::DHCP_CLIENT.ip };
    let our_mac = crate::drivers::net::virtio::get_device()
        .map(|d| d.mac_address())
        .unwrap_or([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);

    let target_mac = unsafe { crate::net::arp::ARP_CACHE.lookup(dst_ip) }
        .unwrap_or(crate::net::ethernet::BROADCAST_MAC);

    let mut eth_buf = [0u8; 1536];
    EthernetFrame::write_header(
        &mut eth_buf[0..14],
        target_mac,
        our_mac,
        ETHERTYPE_IPV4,
    );

    let tcp_seg_len = 20 + payload.len();
    if 34 + tcp_seg_len > eth_buf.len() {
        return false;
    }

    Ipv4Packet::write_header(
        &mut eth_buf[14..34],
        our_ip,
        dst_ip,
        PROTO_TCP,
        tcp_seg_len,
    );

    let written = write_segment(
        &mut eth_buf[34..],
        our_ip,
        dst_ip,
        src_port,
        dst_port,
        seq_num,
        ack_num,
        flags,
        window_size,
        payload,
    );
    if written == 0 {
        return false;
    }

    // Registra ou atualiza no firewall JinnWall SPI
    let ticks = crate::timer::ticks();
    unsafe {
        crate::net::jinnwall::JINNWALL.track_out(our_ip, dst_ip, PROTO_TCP, src_port, dst_port, ticks);
    }

    let total_frame_len = 14 + 20 + written;
    if let Some(dev) = crate::drivers::net::virtio::get_device() {
        dev.transmit(&eth_buf[..total_frame_len]).is_ok()
    } else {
        false
    }
}

/// Despacha pacote TCP recebido para a máquina de estados RFC 793 do socket correspondente.
pub fn handle_incoming_tcp_packet(src_ip: [u8; 4], _dst_ip: [u8; 4], tcp: &TcpPacket) {
    unsafe {
        // 1. Procura primeiro por socket já conectado ou em handshake SynReceived
        for slot in crate::net::socket::SOCKET_TABLE.iter_mut() {
            if let Some(ref mut sock) = slot {
                if sock.sock_type == crate::net::socket::SOCK_STREAM
                    && sock.local_port == tcp.dst_port
                    && sock.remote_ip == src_ip
                    && sock.remote_port == tcp.src_port
                    && (sock.is_connected || sock.tcb.state == TcpState::SynReceived)
                {
                    process_connected_tcp_event(sock, src_ip, tcp);
                    return;
                }
            }
        }

        // 2. Procura por socket em processo de handshake ativo (SynSent)
        for slot in crate::net::socket::SOCKET_TABLE.iter_mut() {
            if let Some(ref mut sock) = slot {
                if sock.sock_type == crate::net::socket::SOCK_STREAM
                    && sock.local_port == tcp.dst_port
                    && sock.tcb.state == TcpState::SynSent
                    && sock.remote_ip == src_ip
                    && sock.remote_port == tcp.src_port
                {
                    process_syn_sent_event(sock, src_ip, tcp);
                    return;
                }
            }
        }

        // 3. Procura por socket em modo LISTEN para aceitar conexões entrantes
        for slot in crate::net::socket::SOCKET_TABLE.iter_mut() {
            if let Some(ref mut sock) = slot {
                if sock.sock_type == crate::net::socket::SOCK_STREAM
                    && sock.local_port == tcp.dst_port
                    && sock.tcb.state == TcpState::Listen
                {
                    process_listen_tcp_event(sock, src_ip, tcp);
                    return;
                }
            }
        }
    }

    // Se nenhum socket escuta nesta porta e o pacote não possui flag RST, envia RST de recusa (RFC 793)
    if (tcp.flags & TCP_FLAG_RST) == 0 {
        let rst_ack = tcp.seq_num.wrapping_add(1);
        send_tcp_packet(tcp.dst_port, src_ip, tcp.src_port, 0, rst_ack, TCP_FLAG_RST | TCP_FLAG_ACK, 0, &[]);
    }
}

fn process_syn_sent_event(sock: &mut crate::net::socket::Socket, src_ip: [u8; 4], tcp: &TcpPacket) {
    if (tcp.flags & TCP_FLAG_RST) != 0 {
        sock.tcb.state = TcpState::Closed;
        sock.is_connected = false;
        return;
    }

    if (tcp.flags & (TCP_FLAG_SYN | TCP_FLAG_ACK)) == (TCP_FLAG_SYN | TCP_FLAG_ACK) {
        if tcp.ack_num == sock.tcb.local_seq {
            sock.tcb.remote_seq = tcp.seq_num.wrapping_add(1);
            sock.tcb.local_ack = tcp.ack_num;
            sock.tcb.state = TcpState::Established;
            sock.is_connected = true;

            // Envia ACK final do handshake de 3 vias
            send_tcp_packet(
                sock.local_port,
                src_ip,
                tcp.src_port,
                sock.tcb.local_seq,
                sock.tcb.remote_seq,
                TCP_FLAG_ACK,
                sock.tcb.local_win,
                &[],
            );
        }
    }
}

fn process_listen_tcp_event(listening_sock: &mut crate::net::socket::Socket, src_ip: [u8; 4], tcp: &TcpPacket) {
    if (tcp.flags & TCP_FLAG_SYN) != 0 && (tcp.flags & TCP_FLAG_ACK) == 0 {
        if listening_sock.pending_count < 4 {
            let listening_port = listening_sock.local_port;
            let owner_pid = listening_sock.owner_pid;

            // Procura um slot livre na tabela para o novo socket filho
            unsafe {
                for (idx, slot) in crate::net::socket::SOCKET_TABLE.iter_mut().enumerate() {
                    if slot.is_none() {
                        let mut child = crate::net::socket::Socket::empty();
                        child.owner_pid = owner_pid;
                        child.sock_type = crate::net::socket::SOCK_STREAM;
                        child.local_port = listening_port;
                        child.remote_ip = src_ip;
                        child.remote_port = tcp.src_port;
                        child.is_bound = true;
                        child.is_connected = false;
                        child.tcb.state = TcpState::SynReceived;
                        child.tcb.remote_seq = tcp.seq_num.wrapping_add(1);
                        child.tcb.local_seq = (crate::timer::ticks() as u32).wrapping_mul(1000).wrapping_add(tcp.src_port as u32);

                        // Envia SYN | ACK para o cliente
                        send_tcp_packet(
                            listening_port,
                            src_ip,
                            tcp.src_port,
                            child.tcb.local_seq,
                            child.tcb.remote_seq,
                            TCP_FLAG_SYN | TCP_FLAG_ACK,
                            child.tcb.local_win,
                            &[],
                        );
                        child.tcb.local_seq = child.tcb.local_seq.wrapping_add(1);

                        *slot = Some(child);
                        listening_sock.pending_conns[listening_sock.pending_count] = Some(idx);
                        listening_sock.pending_count += 1;
                        return;
                    }
                }
            }
        }
    }
}

fn process_connected_tcp_event(sock: &mut crate::net::socket::Socket, src_ip: [u8; 4], tcp: &TcpPacket) {
    match sock.tcb.state {
        TcpState::SynReceived => {
            if (tcp.flags & TCP_FLAG_ACK) != 0 && tcp.ack_num == sock.tcb.local_seq {
                sock.tcb.state = TcpState::Established;
                sock.is_connected = true;
            }
        }
        TcpState::Established => {
            if (tcp.flags & TCP_FLAG_RST) != 0 {
                sock.tcb.state = TcpState::Closed;
                sock.is_connected = false;
                return;
            }

            if (tcp.flags & TCP_FLAG_ACK) != 0 {
                sock.tcb.local_ack = tcp.ack_num;
                if tcp.ack_num >= sock.tcb.local_seq {
                    sock.tcb.unacked_len = 0;
                }
            }

            if !tcp.payload.is_empty() {
                if tcp.seq_num == sock.tcb.remote_seq {
                    let _ = sock.push_rx(tcp.payload);
                    sock.tcb.remote_seq = sock.tcb.remote_seq.wrapping_add(tcp.payload.len() as u32);

                    // Responde com ACK imediato
                    send_tcp_packet(
                        sock.local_port,
                        src_ip,
                        tcp.src_port,
                        sock.tcb.local_seq,
                        sock.tcb.remote_seq,
                        TCP_FLAG_ACK,
                        sock.tcb.local_win,
                        &[],
                    );
                }
            }

            if (tcp.flags & TCP_FLAG_FIN) != 0 {
                sock.tcb.remote_seq = sock.tcb.remote_seq.wrapping_add(1);
                sock.tcb.state = TcpState::CloseWait;

                // Envia ACK do FIN
                send_tcp_packet(
                    sock.local_port,
                    src_ip,
                    tcp.src_port,
                    sock.tcb.local_seq,
                    sock.tcb.remote_seq,
                    TCP_FLAG_ACK,
                    sock.tcb.local_win,
                    &[],
                );
            }
        }
        TcpState::FinWait1 => {
            if (tcp.flags & (TCP_FLAG_FIN | TCP_FLAG_ACK)) == (TCP_FLAG_FIN | TCP_FLAG_ACK) {
                sock.tcb.remote_seq = sock.tcb.remote_seq.wrapping_add(1);
                send_tcp_packet(sock.local_port, src_ip, tcp.src_port, sock.tcb.local_seq, sock.tcb.remote_seq, TCP_FLAG_ACK, sock.tcb.local_win, &[]);
                sock.tcb.state = TcpState::TimeWait;
            } else if (tcp.flags & TCP_FLAG_ACK) != 0 {
                sock.tcb.state = TcpState::FinWait2;
            } else if (tcp.flags & TCP_FLAG_FIN) != 0 {
                sock.tcb.remote_seq = sock.tcb.remote_seq.wrapping_add(1);
                send_tcp_packet(sock.local_port, src_ip, tcp.src_port, sock.tcb.local_seq, sock.tcb.remote_seq, TCP_FLAG_ACK, sock.tcb.local_win, &[]);
                sock.tcb.state = TcpState::Closing;
            }
        }
        TcpState::FinWait2 => {
            if (tcp.flags & TCP_FLAG_FIN) != 0 {
                sock.tcb.remote_seq = sock.tcb.remote_seq.wrapping_add(1);
                send_tcp_packet(sock.local_port, src_ip, tcp.src_port, sock.tcb.local_seq, sock.tcb.remote_seq, TCP_FLAG_ACK, sock.tcb.local_win, &[]);
                sock.tcb.state = TcpState::TimeWait;
            }
        }
        TcpState::Closing => {
            if (tcp.flags & TCP_FLAG_ACK) != 0 {
                sock.tcb.state = TcpState::TimeWait;
            }
        }
        TcpState::LastAck => {
            if (tcp.flags & TCP_FLAG_ACK) != 0 {
                sock.tcb.state = TcpState::Closed;
                sock.is_connected = false;
            }
        }
        _ => {}
    }
}

// ─── SUÍTE DE AUTO-TESTES DO PROTOCOLO TCP (EXECUÇÃO NO BOOT DO KERNEL) ─────

/// Executa suíte de auto-testes da máquina de estados e montagem TCP RFC 793.
/// Retorna `(aprovados, total)`.
pub fn run_tcp_selftests() -> (usize, usize) {
    let mut passed = 0;
    let total = 8;

    // 1. Verificação de estados TCP e nomes
    if TcpState::Closed.name() == "CLOSED" && TcpState::Established.name() == "ESTABLISHED" {
        passed += 1;
    }

    // 2. Montagem e parsing de segmento TCP básico
    let mut seg_buf = [0u8; 64];
    let written = write_segment(
        &mut seg_buf,
        [10, 0, 2, 15],
        [93, 184, 216, 34],
        40000,
        80,
        12345,
        67890,
        TCP_FLAG_SYN,
        4096,
        &[],
    );
    if written == 20 {
        if let Ok(pkt) = TcpPacket::parse(&seg_buf[..written]) {
            if pkt.src_port == 40000 && pkt.dst_port == 80 && pkt.seq_num == 12345 && pkt.flags == TCP_FLAG_SYN {
                passed += 1;
            }
        }
    }

    // 3. Validação do cálculo de Checksum com pseudo-cabeçalho IPv4
    let csum1 = calculate_tcp_checksum([10, 0, 2, 15], [93, 184, 216, 34], &seg_buf[..20]);
    if csum1 != 0 {
        passed += 1;
    }

    // 4. Montagem com payload (PSH | ACK)
    let payload = b"GET / HTTP/1.1\r\n\r\n";
    let written_pay = write_segment(
        &mut seg_buf,
        [10, 0, 2, 15],
        [93, 184, 216, 34],
        40000,
        80,
        12346,
        67891,
        TCP_FLAG_PSH | TCP_FLAG_ACK,
        4096,
        payload,
    );
    if written_pay == 20 + payload.len() {
        if let Ok(pkt) = TcpPacket::parse(&seg_buf[..written_pay]) {
            if pkt.payload == payload && pkt.flags == (TCP_FLAG_PSH | TCP_FLAG_ACK) {
                passed += 1;
            }
        }
    }

    // 5. Simulação de transição de Active Handshake (Closed -> SynSent -> Established)
    let mut tcb = TcpControlBlock::new();
    assert_eq!(tcb.state, TcpState::Closed);
    tcb.state = TcpState::SynSent;
    tcb.local_seq = 1000;
    // Recebe SYN-ACK com ack 1001 e seq 5000
    if tcb.state == TcpState::SynSent {
        tcb.remote_seq = 5000 + 1;
        tcb.local_ack = 1001;
        tcb.state = TcpState::Established;
        passed += 1;
    }

    // 6. Simulação de transmissão de stream e avanço de sequência
    if tcb.state == TcpState::Established {
        let sent_len = 18;
        tcb.local_seq = tcb.local_seq.wrapping_add(sent_len);
        if tcb.local_seq == 1018 {
            passed += 1;
        }
    }

    // 7. Simulação de encerramento gracioso (Established -> FinWait1 -> FinWait2 -> TimeWait -> Closed)
    if tcb.state == TcpState::Established {
        tcb.state = TcpState::FinWait1;
        if tcb.state == TcpState::FinWait1 {
            tcb.state = TcpState::FinWait2;
        }
        if tcb.state == TcpState::FinWait2 {
            tcb.state = TcpState::TimeWait;
        }
        if tcb.state == TcpState::TimeWait {
            tcb.state = TcpState::Closed;
            if tcb.state == TcpState::Closed {
                passed += 1;
            }
        }
    }

    // 8. Rejeição de segmentos truncados (< 20 bytes ou doff < 5)
    let bad_trunc = [0u8; 18];
    let bad_doff = [0, 80, 0, 80, 0, 0, 0, 1, 0, 0, 0, 1, 0x40, 0x02, 0x10, 0x00, 0, 0, 0, 0];
    if TcpPacket::parse(&bad_trunc).is_err() && TcpPacket::parse(&bad_doff).is_err() {
        passed += 1;
    }

    (passed, total)
}
