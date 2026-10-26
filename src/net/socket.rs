//! Jinn OS — Subsistema de Sockets e ABI de Rede para Aplicações Ring 3

use crate::syscall::{EADDRINUSE, EBADF, EINVAL, EMFILE, EPERM};

pub const AF_INET:     u32 = 2;
pub const SOCK_STREAM: u32 = 1; // TCP
pub const SOCK_DGRAM:  u32 = 2; // UDP

pub const MAX_SOCKETS: usize = 16;
pub const SOCK_RX_BUF_SIZE: usize = 1024;

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
pub fn connect_socket(pid: usize, sockfd: usize, remote_ip: [u8; 4], remote_port: u16) -> Result<(), isize> {
    unsafe {
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
            // Atribui porta efêmera aleatória/incremental
            sock.local_port = 40000 + (sockfd as u16 * 100);
        }
        sock.is_connected = true;
        Ok(())
    }
}

/// Fecha o socket e libera recursos.
pub fn close_socket(pid: usize, sockfd: usize) -> Result<(), isize> {
    unsafe {
        let sock = match SOCKET_TABLE.get_mut(sockfd).and_then(|s| s.as_mut()) {
            Some(s) => s,
            None => return Err(EBADF),
        };
        if sock.owner_pid != pid {
            return Err(EPERM);
        }
        SOCKET_TABLE[sockfd] = None;
    }
    if let Some(Some(task)) = crate::scheduler::get().tasks.get_mut(pid) {
        task.resources.close_socket();
    }
    Ok(())
}

/// Fecha todos os sockets pertencentes ao processo `pid` (chamado no `exit_task`).
pub fn close_all_for_pid(pid: usize) {
    unsafe {
        for slot in SOCKET_TABLE.iter_mut() {
            if let Some(sock) = slot {
                if sock.owner_pid == pid {
                    *slot = None;
                }
            }
        }
    }
    if let Some(Some(task)) = crate::scheduler::get().tasks.get_mut(pid) {
        task.resources.open_sockets = 0;
    }
}

/// Envia dados por um socket.
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

        let our_ip = crate::net::dhcp::DHCP_CLIENT.ip;
        let our_mac = crate::drivers::net::virtio::get_device()
            .map(|d| d.mac_address())
            .unwrap_or([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);

        // Registra saída no JinnWall (Stateful Conntrack)
        let proto = if sock_type == SOCK_STREAM {
            crate::net::ipv4::PROTO_TCP
        } else {
            crate::net::ipv4::PROTO_UDP
        };
        crate::net::jinnwall::JINNWALL.track_out(our_ip, remote_ip, proto, local_port, remote_port, crate::timer::ticks());

        // Constrói frame Ethernet + IPv4 + UDP/TCP
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

        let transport_len = if sock_type == SOCK_DGRAM { 8 + payload_len } else { 20 + payload_len };
        crate::net::ipv4::Ipv4Packet::write_header(
            &mut eth_buf[14..34],
            our_ip,
            remote_ip,
            proto,
            transport_len,
        );

        if sock_type == SOCK_DGRAM {
            crate::net::udp::UdpPacket::write_header(&mut eth_buf[34..42], local_port, remote_port, payload_len);
            eth_buf[42..(42 + payload_len)].copy_from_slice(&data[..payload_len]);
        }

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
