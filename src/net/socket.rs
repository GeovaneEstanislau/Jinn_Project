//! Jinn OS — Subsistema de Sockets e ABI de Rede para Aplicações Ring 3

use crate::syscall::{EADDRINUSE, EAGAIN, EBADF, ECONNREFUSED, ECONNRESET, EINVAL, EMFILE, ENOBUFS, EPERM, ETIMEDOUT};

pub const AF_INET:     u32 = 2;
pub const SOCK_STREAM: u32 = 1; // TCP
pub const SOCK_DGRAM:  u32 = 2; // UDP

pub const MAX_SOCKETS: usize = 16;
pub const SOCK_RX_BUF_SIZE: usize = 1536;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SockAddrIn {
    pub sin_family: u16,
    pub sin_port: u16, // Big-Endian
    pub sin_addr: [u8; 4],
    pub sin_zero: [u8; 8],
}

#[derive(Debug, Clone, Copy)]
pub struct Socket {
    pub owner_pid: usize,
    pub sock_type: u32,
    pub local_port: u16,
    pub remote_ip: [u8; 4],
    pub remote_port: u16,
    pub is_bound: bool,
    pub is_connected: bool,

    // Controle de estado e conexões TCP (RFC 793)
    pub tcb: crate::net::tcp::TcpControlBlock,
    pub backlog: usize,
    pub pending_conns: [Option<usize>; 4],
    pub pending_count: usize,

    // Fila circular de recepção para o socket
    pub rx_buf: [[u8; SOCK_RX_BUF_SIZE]; 4],
    pub rx_len: [usize; 4],
    pub rx_head: usize,
    pub rx_tail: usize,
}

impl Socket {
    pub const fn empty() -> Self {
        Self {
            owner_pid: 0,
            sock_type: 0,
            local_port: 0,
            remote_ip: [0; 4],
            remote_port: 0,
            is_bound: false,
            is_connected: false,
            tcb: crate::net::tcp::TcpControlBlock::new(),
            backlog: 0,
            pending_conns: [None; 4],
            pending_count: 0,
            rx_buf: [[0; SOCK_RX_BUF_SIZE]; 4],
            rx_len: [0; 4],
            rx_head: 0,
            rx_tail: 0,
        }
    }

    /// Enfileira dados recebidos da rede na fila do socket
    pub fn push_rx(&mut self, data: &[u8]) -> bool {
        let next_head = (self.rx_head + 1) % 4;
        if next_head == self.rx_tail {
            return false; // Fila cheia
        }
        let len = data.len().min(SOCK_RX_BUF_SIZE);
        self.rx_buf[self.rx_head][..len].copy_from_slice(&data[..len]);
        self.rx_len[self.rx_head] = len;
        self.rx_head = next_head;
        true
    }

    /// Desenfileira dados recebidos para o buffer de destino
    pub fn pop_rx(&mut self, dest: &mut [u8]) -> Option<usize> {
        if self.rx_head == self.rx_tail {
            return None; // Fila vazia
        }
        let len = self.rx_len[self.rx_tail].min(dest.len());
        dest[..len].copy_from_slice(&self.rx_buf[self.rx_tail][..len]);
        self.rx_tail = (self.rx_tail + 1) % 4;
        Some(len)
    }
}

pub static mut SOCKET_TABLE: [Option<Socket>; MAX_SOCKETS] = [None; MAX_SOCKETS];
pub static mut NEXT_EPHEMERAL_PORT: u16 = 40000;

/// Cria um novo socket associado ao processo `pid`.
pub fn create_socket(pid: usize, domain: u32, sock_type: u32, _protocol: u32) -> Result<usize, isize> {
    if domain != AF_INET || (sock_type != SOCK_STREAM && sock_type != SOCK_DGRAM) {
        return Err(EINVAL);
    }

    // Verifica limites de recursos
    if let Some(Some(task)) = crate::scheduler::get().tasks.get_mut(pid) {
        if let Err(e) = task.resources.check_and_open_socket(&task.limits) {
            return Err(e);
        }
    }

    unsafe {
        for (idx, slot) in SOCKET_TABLE.iter_mut().enumerate() {
            if slot.is_none() {
                let mut sock = Socket::empty();
                sock.owner_pid = pid;
                sock.sock_type = sock_type;
                *slot = Some(sock);
                return Ok(idx);
            }
        }
    }

    if let Some(Some(task)) = crate::scheduler::get().tasks.get_mut(pid) {
        task.resources.close_socket();
    }
    Err(EMFILE)
}

/// Associa o socket a uma porta local específica (bind).
pub fn bind_socket(pid: usize, sockfd: usize, local_port: u16) -> Result<(), isize> {
    if local_port == 0 {
        return Err(EINVAL);
    }
    unsafe {
        // Verifica se a porta já está em uso por outro socket
        for (idx, slot) in SOCKET_TABLE.iter().enumerate() {
            if idx != sockfd {
                if let Some(s) = slot {
                    if s.local_port == local_port && s.is_bound {
                        return Err(EADDRINUSE);
                    }
                }
            }
        }

        let sock = match SOCKET_TABLE.get_mut(sockfd).and_then(|s| s.as_mut()) {
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
}

/// Conecta o socket a um IP e porta remotos.
/// Para UDP (SOCK_DGRAM), apenas registra o endpoint remoto.
/// Para TCP (SOCK_STREAM), executa o handshake de 3 vias (SYN -> SYN-ACK -> ACK) com timeout e retransmissões.
pub fn connect_socket(pid: usize, sockfd: usize, remote_ip: [u8; 4], remote_port: u16) -> Result<(), isize> {
    let (sock_type, local_port) = unsafe {
        let sock = match SOCKET_TABLE.get_mut(sockfd).and_then(|s| s.as_mut()) {
            Some(s) => s,
            None => return Err(EBADF),
        };
        if sock.owner_pid != pid {
            return Err(EPERM);
        }
        sock.remote_ip = remote_ip;
        sock.remote_port = remote_port;
        if sock.local_port == 0 {
            let port = NEXT_EPHEMERAL_PORT;
            NEXT_EPHEMERAL_PORT = if NEXT_EPHEMERAL_PORT >= 60000 { 40000 } else { NEXT_EPHEMERAL_PORT + 1 };
            sock.local_port = port;
            sock.is_bound = true;
        }
        (sock.sock_type, sock.local_port)
    };

    if sock_type == SOCK_DGRAM {
        unsafe {
            if let Some(Some(ref mut s)) = SOCKET_TABLE.get_mut(sockfd) {
                s.is_connected = true;
            }
        }
        return Ok(());
    }

    // ── Handshake TCP Ativo (RFC 793: SYN -> SYN-ACK -> ACK) ────────────────
    unsafe {
        if let Some(Some(ref mut s)) = SOCKET_TABLE.get_mut(sockfd) {
            s.tcb.state = crate::net::tcp::TcpState::SynSent;
            s.tcb.local_seq = (crate::timer::ticks() as u32).wrapping_mul(1000).wrapping_add(local_port as u32);
            s.tcb.last_tx_tick = crate::timer::ticks();
            s.tcb.retransmit_count = 0;
        }
    }

    // Envia requisicao ARP caso o IP remoto ainda nao esteja na tabela ARP
    unsafe {
        if crate::net::arp::ARP_CACHE.lookup(remote_ip).is_none() {
            let _ = crate::net::arp::send_arp_request(remote_ip);
        }
    }

    let syn_seq = unsafe { SOCKET_TABLE[sockfd].as_ref().unwrap().tcb.local_seq };
    crate::net::tcp::send_tcp_packet(
        local_port,
        remote_ip,
        remote_port,
        syn_seq,
        0,
        crate::net::tcp::TCP_FLAG_SYN,
        crate::net::tcp::TCP_DEFAULT_WINDOW,
        &[],
    );
    unsafe {
        if let Some(Some(ref mut s)) = SOCKET_TABLE.get_mut(sockfd) {
            s.tcb.local_seq = s.tcb.local_seq.wrapping_add(1);
        }
    }

    // Loop de espera ativa com polling de rede e retransmissões automáticas (RTO)
    let start_tick = crate::timer::ticks();
    while crate::timer::ticks().saturating_sub(start_tick) < 300 {
        crate::net::poll();

        unsafe {
            if let Some(Some(ref s)) = SOCKET_TABLE.get(sockfd) {
                if s.tcb.state == crate::net::tcp::TcpState::Established {
                    return Ok(());
                }
                if s.tcb.state == crate::net::tcp::TcpState::Closed {
                    return Err(ECONNREFUSED);
                }
            }
        }

        // Retransmissão de SYN em caso de perda
        let now = crate::timer::ticks();
        unsafe {
            if let Some(Some(ref mut s)) = SOCKET_TABLE.get_mut(sockfd) {
                if s.tcb.state == crate::net::tcp::TcpState::SynSent
                    && now.saturating_sub(s.tcb.last_tx_tick) >= s.tcb.rto_ticks
                {
                    if s.tcb.retransmit_count < 3 {
                        s.tcb.retransmit_count += 1;
                        s.tcb.last_tx_tick = now;
                        let retrans_seq = s.tcb.local_seq.wrapping_sub(1);
                        crate::net::tcp::send_tcp_packet(
                            s.local_port,
                            remote_ip,
                            remote_port,
                            retrans_seq,
                            0,
                            crate::net::tcp::TCP_FLAG_SYN,
                            s.tcb.local_win,
                            &[],
                        );
                    }
                }
            }
        }
        crate::scheduler::yield_now();
    }

    // Timeout atingido sem resposta do peer
    unsafe {
        if let Some(Some(ref mut s)) = SOCKET_TABLE.get_mut(sockfd) {
            s.tcb.state = crate::net::tcp::TcpState::Closed;
            s.is_connected = false;
        }
    }
    Err(ETIMEDOUT)
}

/// Habilita escuta passiva de conexões TCP em uma porta bound (listen).
pub fn listen_socket(pid: usize, sockfd: usize, backlog: usize) -> Result<(), isize> {
    unsafe {
        let sock = match SOCKET_TABLE.get_mut(sockfd).and_then(|s| s.as_mut()) {
            Some(s) => s,
            None => return Err(EBADF),
        };
        if sock.owner_pid != pid {
            return Err(EPERM);
        }
        if !sock.is_bound || sock.local_port == 0 {
            return Err(EINVAL);
        }
        if sock.sock_type != SOCK_STREAM {
            return Err(EINVAL);
        }
        sock.tcb.state = crate::net::tcp::TcpState::Listen;
        sock.backlog = backlog.clamp(1, 4);
        Ok(())
    }
}

/// Aceita uma conexão TCP estabelecida a partir da fila de conexões pendentes (accept).
pub fn accept_socket(pid: usize, sockfd: usize) -> Result<usize, isize> {
    crate::net::poll();
    unsafe {
        let _ = match SOCKET_TABLE.get(sockfd).and_then(|s| s.as_ref()) {
            Some(s) => {
                if s.owner_pid != pid { return Err(EPERM); }
                if s.tcb.state != crate::net::tcp::TcpState::Listen { return Err(EINVAL); }
                s
            }
            None => return Err(EBADF),
        };

        if let Some(ref mut listening_sock) = SOCKET_TABLE[sockfd] {
            for i in 0..listening_sock.pending_count {
                if let Some(child_fd) = listening_sock.pending_conns[i] {
                    if let Some(Some(child)) = SOCKET_TABLE.get(child_fd) {
                        if child.tcb.state == crate::net::tcp::TcpState::Established {
                            listening_sock.pending_conns[i] = None;
                            for j in i..(listening_sock.pending_count - 1) {
                                listening_sock.pending_conns[j] = listening_sock.pending_conns[j + 1];
                            }
                            listening_sock.pending_conns[listening_sock.pending_count - 1] = None;
                            listening_sock.pending_count = listening_sock.pending_count.saturating_sub(1);
                            return Ok(child_fd);
                        }
                    }
                }
            }
        }

        Err(EAGAIN)
    }
}

/// Fecha o socket e libera recursos.
/// Para TCP em Established, envia FIN para encerramento gracioso (RFC 793).
pub fn close_socket(pid: usize, sockfd: usize) -> Result<(), isize> {
    unsafe {
        let sock = match SOCKET_TABLE.get_mut(sockfd).and_then(|s| s.as_mut()) {
            Some(s) => s,
            None => return Err(EBADF),
        };
        if sock.owner_pid != pid {
            return Err(EPERM);
        }

        if sock.sock_type == SOCK_STREAM && sock.tcb.state == crate::net::tcp::TcpState::Established {
            crate::net::tcp::send_tcp_packet(
                sock.local_port,
                sock.remote_ip,
                sock.remote_port,
                sock.tcb.local_seq,
                sock.tcb.remote_seq,
                crate::net::tcp::TCP_FLAG_FIN | crate::net::tcp::TCP_FLAG_ACK,
                sock.tcb.local_win,
                &[],
            );
            sock.tcb.local_seq = sock.tcb.local_seq.wrapping_add(1);
            sock.tcb.state = crate::net::tcp::TcpState::FinWait1;
        }

        // Limpa estado no firewall JinnWall SPI
        let our_ip = crate::net::dhcp::DHCP_CLIENT.ip;
        crate::net::jinnwall::JINNWALL.clear_connection(
            our_ip,
            sock.remote_ip,
            crate::net::ipv4::PROTO_TCP,
            sock.local_port,
            sock.remote_port,
        );

        SOCKET_TABLE[sockfd] = None;
    }
    if let Some(Some(task)) = crate::scheduler::get().tasks.get_mut(pid) {
        task.resources.close_socket();
    }
    Ok(())
}

/// Fecha todos os sockets pertencentes ao processo `pid` (chamado no `exit_task`).
/// Notifica endpoints remotos conectados com RST para evitar conexões zumbis.
pub fn close_all_for_pid(pid: usize) {
    unsafe {
        let our_ip = crate::net::dhcp::DHCP_CLIENT.ip;
        for slot in SOCKET_TABLE.iter_mut() {
            if let Some(sock) = slot {
                if sock.owner_pid == pid {
                    if sock.sock_type == SOCK_STREAM && (sock.is_connected || sock.tcb.state == crate::net::tcp::TcpState::Established) {
                        crate::net::tcp::send_tcp_packet(
                            sock.local_port,
                            sock.remote_ip,
                            sock.remote_port,
                            sock.tcb.local_seq,
                            sock.tcb.remote_seq,
                            crate::net::tcp::TCP_FLAG_RST | crate::net::tcp::TCP_FLAG_ACK,
                            0,
                            &[],
                        );
                        crate::net::jinnwall::JINNWALL.clear_connection(
                            our_ip,
                            sock.remote_ip,
                            crate::net::ipv4::PROTO_TCP,
                            sock.local_port,
                            sock.remote_port,
                        );
                    }
                    *slot = None;
                }
            }
        }
    }
    if let Some(Some(task)) = crate::scheduler::get().tasks.get_mut(pid) {
        task.resources.open_sockets = 0;
    }
}

/// Envia dados por um socket UDP ou TCP.
pub fn send_socket(pid: usize, sockfd: usize, data: &[u8]) -> Result<usize, isize> {
    unsafe {
        let (sock_type, local_port, remote_ip, remote_port) = {
            let sock = match SOCKET_TABLE.get(sockfd).and_then(|s| s.as_ref()) {
                Some(s) => s,
                None => return Err(EBADF),
            };
            if sock.owner_pid != pid {
                return Err(EPERM);
            }
            if !sock.is_connected {
                return Err(EINVAL);
            }
            (sock.sock_type, sock.local_port, sock.remote_ip, sock.remote_port)
        };

        if sock_type == SOCK_STREAM {
            let sock = SOCKET_TABLE[sockfd].as_mut().unwrap();
            if sock.tcb.state != crate::net::tcp::TcpState::Established {
                return Err(ECONNRESET);
            }

            // Controle de fluxo por Janela Deslizante (RFC 793)
            let in_flight = sock.tcb.local_seq.wrapping_sub(sock.tcb.local_ack) as usize;
            let win_avail = (sock.tcb.remote_win as usize).saturating_sub(in_flight);
            if win_avail == 0 {
                return Err(EAGAIN); // Janela anunciada pelo peer esgotada
            }

            let buf_avail = 4096usize.saturating_sub(sock.tcb.unacked_len);
            if buf_avail == 0 {
                return Err(ENOBUFS); // Buffer de retransmissão esgotado
            }

            let send_cap = win_avail.min(buf_avail).min(1024);
            let payload_len = data.len().min(send_cap);
            if payload_len == 0 {
                return Err(EAGAIN);
            }

            let sent = crate::net::tcp::send_tcp_packet(
                local_port,
                remote_ip,
                remote_port,
                sock.tcb.local_seq,
                sock.tcb.remote_seq,
                crate::net::tcp::TCP_FLAG_PSH | crate::net::tcp::TCP_FLAG_ACK,
                sock.tcb.local_win,
                &data[..payload_len],
            );
            if sent {
                let start_idx = sock.tcb.unacked_len;
                sock.tcb.unacked_buf[start_idx..(start_idx + payload_len)].copy_from_slice(&data[..payload_len]);
                sock.tcb.unacked_len += payload_len;
                sock.tcb.local_seq = sock.tcb.local_seq.wrapping_add(payload_len as u32);
                sock.tcb.last_tx_tick = crate::timer::ticks();
                return Ok(payload_len);
            } else {
                return Err(ENOBUFS);
            }
        }

        // UDP (SOCK_DGRAM)
        let our_ip = crate::net::dhcp::DHCP_CLIENT.ip;
        let our_mac = crate::drivers::net::virtio::get_device()
            .map(|d| d.mac_address())
            .unwrap_or([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);

        crate::net::jinnwall::JINNWALL.track_out(our_ip, remote_ip, crate::net::ipv4::PROTO_UDP, local_port, remote_port, crate::timer::ticks());

        let mut eth_buf = [0u8; 1536];
        let payload_len = data.len().min(1024);
        let target_mac = crate::net::arp::ARP_CACHE.lookup(remote_ip)
            .unwrap_or(crate::net::ethernet::BROADCAST_MAC);

        crate::net::ethernet::EthernetFrame::write_header(
            &mut eth_buf[0..14],
            target_mac,
            our_mac,
            crate::net::ethernet::ETHERTYPE_IPV4,
        );

        let transport_len = 8 + payload_len;
        crate::net::ipv4::Ipv4Packet::write_header(
            &mut eth_buf[14..34],
            our_ip,
            remote_ip,
            crate::net::ipv4::PROTO_UDP,
            transport_len,
        );

        crate::net::udp::UdpPacket::write_header(&mut eth_buf[34..42], local_port, remote_port, payload_len);
        eth_buf[42..(42 + payload_len)].copy_from_slice(&data[..payload_len]);

        let total_frame_len = 14 + 20 + transport_len;
        if let Some(dev) = crate::drivers::net::virtio::get_device() {
            let _ = dev.transmit(&eth_buf[..total_frame_len]);
        }
        Ok(payload_len)
    }
}

/// Despacha um payload recebido e aprovado pelo firewall JinnWall exclusivamente para o socket correspondente à porta de destino.
pub fn dispatch_incoming_packet(proto: u8, src_ip: [u8; 4], src_port: u16, dst_port: u16, payload: &[u8]) -> bool {
    if dst_port == 0 {
        return false;
    }
    let sock_type = if proto == crate::net::ipv4::PROTO_TCP { SOCK_STREAM } else { SOCK_DGRAM };
    unsafe {
        for slot in SOCKET_TABLE.iter_mut() {
            if let Some(ref mut sock) = slot {
                // Roteamento exclusivo: deve coincidir o tipo de socket e a porta local explicitamente bound/conectada
                if sock.sock_type == sock_type && sock.local_port != 0 && sock.local_port == dst_port {
                    // Se o socket estiver conectado a um endpoint específico, valida IP e porta remotos
                    if !sock.is_connected || (sock.remote_ip == src_ip && (sock.remote_port == src_port || sock.remote_port == 0)) {
                        return sock.push_rx(payload);
                    }
                }
            }
        }
    }
    false
}

/// Recebe dados do buffer do socket para o espaço de usuário.
pub fn recv_socket(pid: usize, sockfd: usize, dest: &mut [u8]) -> Result<usize, isize> {
    crate::net::poll();
    unsafe {
        let sock = match SOCKET_TABLE.get_mut(sockfd).and_then(|s| s.as_mut()) {
            Some(s) => s,
            None => return Err(EBADF),
        };
        if sock.owner_pid != pid {
            return Err(EPERM);
        }
        match sock.pop_rx(dest) {
            Some(len) => Ok(len),
            None => Ok(0), // Fila vazia
        }
    }
}

/// Obtém informações do peer remoto (IP e porta) de um socket.
pub fn get_socket_peer(sockfd: usize) -> Option<([u8; 4], u16)> {
    unsafe {
        let slot = &SOCKET_TABLE[sockfd];
        slot.as_ref().map(|s| (s.remote_ip, s.remote_port))
    }
}
