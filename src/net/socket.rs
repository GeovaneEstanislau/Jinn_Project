//! Jinn OS — Subsistema de Sockets e ABI de Rede para Aplicações Ring 3

use crate::syscall::{EADDRINUSE, EAGAIN, EBADF, ECONNREFUSED, ECONNRESET, EHOSTUNREACH, EINVAL, EMFILE, ENETDOWN, ENETUNREACH, ENOBUFS, EPERM, ETIMEDOUT};

pub const AF_INET:     u32 = 2;
pub const SOCK_STREAM: u32 = 1; // TCP
pub const SOCK_DGRAM:  u32 = 2; // UDP

pub const MAX_SOCKETS: usize = 32;
pub const MAX_BACKLOG: usize = 8;
pub const MAX_SYN_BACKLOG: usize = 8;
pub const SOCK_RX_BUF_SIZE: usize = 1536;
pub const SOCK_RX_STREAM_SIZE: usize = 65536; // 64 KiB por socket para fluxo continuo HTTP/TCP

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
    pub sock_idx: usize,
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
    pub parent_listener: Option<usize>,

    // Fila de SYN recebidos (handshake em andamento - SynReceived)
    pub syn_queue: [Option<usize>; MAX_SYN_BACKLOG],
    pub syn_count: usize,

    // Fila de conexões estabelecidas aguardando sys_accept (Established)
    pub accept_queue: [Option<usize>; MAX_BACKLOG],
    pub accept_count: usize,

    // Buffer circular de recepcao continua (Stream Ring Buffer de 32 KiB)
    pub rx_head: usize,
    pub rx_tail: usize,
    pub rx_len: usize,
}

impl Socket {
    pub const fn empty() -> Self {
        Self {
            sock_idx: 0,
            owner_pid: 0,
            sock_type: 0,
            local_port: 0,
            remote_ip: [0; 4],
            remote_port: 0,
            is_bound: false,
            is_connected: false,
            tcb: crate::net::tcp::TcpControlBlock::new(),
            backlog: 0,
            parent_listener: None,
            syn_queue: [None; MAX_SYN_BACKLOG],
            syn_count: 0,
            accept_queue: [None; MAX_BACKLOG],
            accept_count: 0,
            rx_head: 0,
            rx_tail: 0,
            rx_len: 0,
        }
    }

    /// Insere uma conexão filha em handshake na fila SYN do listener.
    pub fn push_syn(&mut self, child_idx: usize) -> bool {
        if self.syn_count >= MAX_SYN_BACKLOG {
            return false;
        }
        for slot in self.syn_queue.iter_mut() {
            if slot.is_none() {
                *slot = Some(child_idx);
                self.syn_count += 1;
                return true;
            }
        }
        false
    }

    /// Remove uma conexão filha da fila SYN após conclusão ou aborto.
    pub fn remove_syn(&mut self, child_idx: usize) -> bool {
        for slot in self.syn_queue.iter_mut() {
            if *slot == Some(child_idx) {
                *slot = None;
                self.syn_count = self.syn_count.saturating_sub(1);
                return true;
            }
        }
        false
    }

    /// Insere uma conexão concluída (Established) na fila de accept respeitando o backlog configurado.
    pub fn push_accept(&mut self, child_idx: usize) -> bool {
        let cap = self.backlog.min(MAX_BACKLOG);
        if self.accept_count >= cap {
            return false; // Backlog de conexões estabelecidas esgotado
        }
        self.accept_queue[self.accept_count] = Some(child_idx);
        self.accept_count += 1;
        true
    }

    /// Desenfileira em ordem FIFO a conexão estabelecida mais antiga para entrega via sys_accept.
    pub fn pop_accept(&mut self) -> Option<usize> {
        if self.accept_count == 0 {
            return None;
        }
        let popped = self.accept_queue[0];
        for i in 0..(self.accept_count - 1) {
            self.accept_queue[i] = self.accept_queue[i + 1];
        }
        self.accept_queue[self.accept_count - 1] = None;
        self.accept_count = self.accept_count.saturating_sub(1);
        popped
    }

    /// Remove uma conexão da fila de accept caso tenha sido abortada pelo cliente antes de ser aceita.
    pub fn remove_accept(&mut self, child_idx: usize) -> bool {
        let mut found = None;
        for i in 0..self.accept_count {
            if self.accept_queue[i] == Some(child_idx) {
                found = Some(i);
                break;
            }
        }
        if let Some(idx) = found {
            for i in idx..(self.accept_count - 1) {
                self.accept_queue[i] = self.accept_queue[i + 1];
            }
            self.accept_queue[self.accept_count - 1] = None;
            self.accept_count = self.accept_count.saturating_sub(1);
            true
        } else {
            false
        }
    }

    /// Retorna o espaço disponível no buffer circular de recepção (em bytes)
    pub fn rx_available_space(&self) -> usize {
        SOCK_RX_STREAM_SIZE.saturating_sub(self.rx_len)
    }

    /// Enfileira dados recebidos da rede no buffer de fluxo contínuo (Stream Ring Buffer).
    /// Retorna a quantidade exata de bytes absorvidos.
    pub fn push_rx(&mut self, data: &[u8]) -> usize {
        if self.sock_idx >= MAX_SOCKETS || data.is_empty() {
            return 0;
        }
        let avail = self.rx_available_space();
        let to_store = data.len().min(avail);
        if to_store == 0 {
            return 0;
        }

        unsafe {
            let buf = &mut SOCKET_RX_BUFFERS[self.sock_idx];
            let part1 = to_store.min(SOCK_RX_STREAM_SIZE - self.rx_head);
            buf[self.rx_head..self.rx_head + part1].copy_from_slice(&data[..part1]);
            let part2 = to_store - part1;
            if part2 > 0 {
                buf[..part2].copy_from_slice(&data[part1..to_store]);
            }
        }

        self.rx_head = (self.rx_head + to_store) % SOCK_RX_STREAM_SIZE;
        self.rx_len += to_store;
        // Atualiza dinamicamente a janela de recepção anunciada (RFC 793)
        self.tcb.local_win = self.rx_available_space().min(65535) as u16;
        to_store
    }

    /// Desenfileira dados recebidos para o buffer de destino preservando bytes não lidos.
    pub fn pop_rx(&mut self, dest: &mut [u8]) -> Option<usize> {
        if self.sock_idx >= MAX_SOCKETS || self.rx_len == 0 {
            return None; // Sem dados no momento
        }
        let to_read = dest.len().min(self.rx_len);
        if to_read == 0 {
            return Some(0);
        }

        unsafe {
            let buf = &SOCKET_RX_BUFFERS[self.sock_idx];
            let part1 = to_read.min(SOCK_RX_STREAM_SIZE - self.rx_tail);
            dest[..part1].copy_from_slice(&buf[self.rx_tail..self.rx_tail + part1]);
            let part2 = to_read - part1;
            if part2 > 0 {
                dest[part1..to_read].copy_from_slice(&buf[..part2]);
            }
        }

        self.rx_tail = (self.rx_tail + to_read) % SOCK_RX_STREAM_SIZE;
        self.rx_len -= to_read;
        // Atualiza dinamicamente a janela de recepção anunciada (RFC 793)
        self.tcb.local_win = self.rx_available_space().min(65535) as u16;
        Some(to_read)
    }
}

pub static mut SOCKET_RX_BUFFERS: [[u8; SOCK_RX_STREAM_SIZE]; MAX_SOCKETS] = [[0; SOCK_RX_STREAM_SIZE]; MAX_SOCKETS];
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
                sock.sock_idx = idx;
                sock.owner_pid = pid;
                sock.sock_type = sock_type;
                *slot = Some(sock);
                crate::net::NET_DIAGNOSTICS.sockets_opened.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
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
    let is_loopback = remote_ip[0] == 127 || remote_ip == [127, 0, 0, 1];

    // Validação estrita de interface de rede física/virtual (dispensada para loopback interno)
    let dev_avail = crate::drivers::net::get_device().map(|d| d.is_available()).unwrap_or(false);
    if !is_loopback && !dev_avail {
        return Err(ENETDOWN);
    }

    // Validação de rota de rede para o próximo salto
    let next_hop = crate::net::ipv4::get_next_hop_ip(remote_ip);
    if next_hop == [0, 0, 0, 0] {
        return Err(EHOSTUNREACH);
    }
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
            s.tcb.rto_ticks = 100; // 1,0s base RTO RFC 6298
        }
    }

    // Garante resolucao de proximo salto se não for loopback
    if !is_loopback {
        let _ = crate::net::arp::resolve_mac_for_ip(remote_ip);
    }

    let syn_seq = unsafe { SOCKET_TABLE[sockfd].as_ref().unwrap().tcb.local_seq };
    crate::net::NET_DIAGNOSTICS.record_tcp_syn_sent();
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
    let timeout_ticks = 1500u64; // 6,0s exatos pelo timer APIC (100 Hz)
    while crate::timer::ticks().saturating_sub(start_tick) < timeout_ticks {

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
                    if s.tcb.retransmit_count < 4 {
                        s.tcb.rto_ticks = (s.tcb.rto_ticks * 2).min(800);
                        crate::net::NET_DIAGNOSTICS.record_tcp_retransmit();
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
    crate::net::NET_DIAGNOSTICS.record_tcp_timeout();
    unsafe {
        if let Some(Some(ref mut s)) = SOCKET_TABLE.get_mut(sockfd) {
            s.tcb.state = crate::net::tcp::TcpState::Closed;
            s.is_connected = false;
            s.tcb.unacked_len = 0;
            s.tcb.ooo_len = 0;
            let our_ip = crate::net::dhcp::DHCP_CLIENT.ip;
            crate::net::jinnwall::JINNWALL.clear_connection(
                our_ip,
                remote_ip,
                crate::net::ipv4::PROTO_TCP,
                local_port,
                remote_port,
            );
        }
    }
    Err(ETIMEDOUT)
}

/// Habilita escuta passiva de conexões TCP em uma porta bound (listen) com controle de backlog configurável.
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
        // Validação e delimitação de backlog: 0 é promovido a 1; valores altos limitados a MAX_BACKLOG (RFC / BSD)
        sock.backlog = if backlog == 0 { 1 } else { backlog.min(MAX_BACKLOG) };
        sock.syn_count = 0;
        sock.syn_queue = [None; MAX_SYN_BACKLOG];
        sock.accept_count = 0;
        sock.accept_queue = [None; MAX_BACKLOG];
        Ok(())
    }
}

/// Aceita uma conexão TCP estabelecida a partir da fila de accept FIFO do listener.
pub fn accept_socket(pid: usize, sockfd: usize) -> Result<usize, isize> {
    crate::net::poll();
    unsafe {
        let listening_sock = match SOCKET_TABLE.get_mut(sockfd).and_then(|s| s.as_mut()) {
            Some(s) => {
                if s.owner_pid != pid { return Err(EPERM); }
                if s.tcb.state != crate::net::tcp::TcpState::Listen { return Err(EINVAL); }
                s
            }
            None => return Err(EBADF),
        };

        // Desenfileira da accept_queue; descarta sockets que foram abortados ou fechados antes do accept
        while let Some(child_fd) = listening_sock.pop_accept() {
            if let Some(Some(ref mut child)) = SOCKET_TABLE.get_mut(child_fd) {
                if child.tcb.state == crate::net::tcp::TcpState::Established {
                    // Desassocia o socket filho do listener pai pois agora foi entregue à aplicação
                    child.parent_listener = None;
                    return Ok(child_fd);
                }
            }
        }

        Err(EAGAIN)
    }
}

/// Fecha o socket e libera recursos.
/// Para listener TCP, aborta graciosamente todas as conexões em syn_queue e accept_queue.
/// Para TCP conectado, envia FIN para encerramento gracioso (RFC 793).
pub fn close_socket(pid: usize, sockfd: usize) -> Result<(), isize> {
    unsafe {
        let sock = match SOCKET_TABLE.get_mut(sockfd).and_then(|s| s.as_mut()) {
            Some(s) => s,
            None => return Err(EBADF),
        };
        if sock.owner_pid != pid {
            return Err(EPERM);
        }

        let our_ip = crate::net::dhcp::DHCP_CLIENT.ip;

        // Se for socket de escuta (LISTEN), encerra com RST todas as conexões filhas pendentes
        if sock.tcb.state == crate::net::tcp::TcpState::Listen {
            let syn_q = sock.syn_queue;
            let acc_q = sock.accept_queue;
            sock.syn_queue = [None; MAX_SYN_BACKLOG];
            sock.syn_count = 0;
            sock.accept_queue = [None; MAX_BACKLOG];
            sock.accept_count = 0;
            sock.tcb.state = crate::net::tcp::TcpState::Closed;

            for child_opt in syn_q.iter().chain(acc_q.iter()) {
                if let Some(child_idx) = *child_opt {
                    if let Some(Some(ref mut child)) = SOCKET_TABLE.get_mut(child_idx) {
                        crate::net::tcp::send_tcp_packet(
                            child.local_port,
                            child.remote_ip,
                            child.remote_port,
                            child.tcb.local_seq,
                            child.tcb.remote_seq,
                            crate::net::tcp::TCP_FLAG_RST | crate::net::tcp::TCP_FLAG_ACK,
                            0,
                            &[],
                        );
                        crate::net::jinnwall::JINNWALL.clear_connection(
                            our_ip,
                            child.remote_ip,
                            crate::net::ipv4::PROTO_TCP,
                            child.local_port,
                            child.remote_port,
                        );
                        SOCKET_TABLE[child_idx] = None;
                    }
                }
            }
        } else if sock.sock_type == SOCK_STREAM && sock.tcb.state == crate::net::tcp::TcpState::Established {
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
            sock.tcb.last_tx_tick = crate::timer::ticks();
            // Mantém TCB vivo para encerramento gracioso (órfão), liberando apenas o processo chamador
            sock.owner_pid = usize::MAX;
            crate::net::NET_DIAGNOSTICS.sockets_closed.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            if let Some(Some(task)) = crate::scheduler::get().tasks.get_mut(pid) {
                task.resources.close_socket();
            }
            return Ok(());
        }

        // Se era um socket filho pendente aguardando accept em um listener pai, remove-se das filas do pai
        if let Some(parent_idx) = sock.parent_listener {
            if let Some(Some(ref mut parent)) = SOCKET_TABLE.get_mut(parent_idx) {
                parent.remove_syn(sockfd);
                parent.remove_accept(sockfd);
            }
        }

        // Limpa estado no firewall JinnWall SPI
        crate::net::jinnwall::JINNWALL.clear_connection(
            our_ip,
            sock.remote_ip,
            crate::net::ipv4::PROTO_TCP,
            sock.local_port,
            sock.remote_port,
        );

        crate::net::NET_DIAGNOSTICS.sockets_closed.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        SOCKET_TABLE[sockfd] = None;
    }
    if let Some(Some(task)) = crate::scheduler::get().tasks.get_mut(pid) {
        task.resources.close_socket();
    }
    Ok(())
}

/// Fecha todos os sockets pertencentes ao processo `pid` (chamado no `exit_task`).
/// Notifica endpoints remotos conectados e conexões pendentes com RST para evitar conexões zumbis.
pub fn close_all_for_pid(pid: usize) {
    unsafe {
        let our_ip = crate::net::dhcp::DHCP_CLIENT.ip;
        for slot in SOCKET_TABLE.iter_mut() {
            if let Some(sock) = slot {
                if sock.owner_pid == pid {
                    if sock.tcb.state == crate::net::tcp::TcpState::Listen {
                        let syn_q = sock.syn_queue;
                        let acc_q = sock.accept_queue;
                        sock.syn_queue = [None; MAX_SYN_BACKLOG];
                        sock.syn_count = 0;
                        sock.accept_queue = [None; MAX_BACKLOG];
                        sock.accept_count = 0;
                        for child_opt in syn_q.iter().chain(acc_q.iter()) {
                            if let Some(child_idx) = *child_opt {
                                if let Some(Some(ref mut child)) = SOCKET_TABLE.get_mut(child_idx) {
                                    crate::net::tcp::send_tcp_packet(
                                        child.local_port,
                                        child.remote_ip,
                                        child.remote_port,
                                        child.tcb.local_seq,
                                        child.tcb.remote_seq,
                                        crate::net::tcp::TCP_FLAG_RST | crate::net::tcp::TCP_FLAG_ACK,
                                        0,
                                        &[],
                                    );
                                    crate::net::jinnwall::JINNWALL.clear_connection(
                                        our_ip,
                                        child.remote_ip,
                                        crate::net::ipv4::PROTO_TCP,
                                        child.local_port,
                                        child.remote_port,
                                    );
                                    SOCKET_TABLE[child_idx] = None;
                                }
                            }
                        }
                    } else if sock.sock_type == SOCK_STREAM && (sock.is_connected || sock.tcb.state == crate::net::tcp::TcpState::Established) {
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
        let our_mac = crate::drivers::net::get_device()
            .map(|d| d.mac_address())
            .unwrap_or([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);

        crate::net::jinnwall::JINNWALL.track_out(our_ip, remote_ip, crate::net::ipv4::PROTO_UDP, local_port, remote_port, crate::timer::ticks());

        let mut eth_buf = [0u8; 1536];
        let payload_len = data.len().min(1024);
        let target_mac = crate::net::arp::resolve_mac_for_ip(remote_ip);

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
        if let Some(dev) = crate::drivers::net::get_device() {
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
                        return sock.push_rx(payload) > 0;
                    }
                }
            }
        }
    }
    false
}

/// Recebe dados do buffer do socket para o espaço de usuário.
pub fn recv_socket(pid: usize, sockfd: usize, dest: &mut [u8]) -> Result<usize, isize> {
    if dest.is_empty() {
        return Ok(0);
    }
    crate::net::poll();
    unsafe {
        let sock = match SOCKET_TABLE.get_mut(sockfd).and_then(|s| s.as_mut()) {
            Some(s) => s,
            None => return Err(EBADF),
        };
        if sock.owner_pid != pid {
            return Err(EPERM);
        }
        let prev_win = sock.tcb.local_win;
        match sock.pop_rx(dest) {
            Some(len) => {
                // Notificação de abertura de janela (Window Update ACK) quando há espaço recuperado
                if sock.sock_type == SOCK_STREAM && sock.tcb.state == crate::net::tcp::TcpState::Established {
                    let win_opened = sock.tcb.local_win.saturating_sub(prev_win);
                    let should_update = (prev_win < 2048 && sock.tcb.local_win >= 2048)
                        || (prev_win == 0 && sock.tcb.local_win > 0)
                        || win_opened >= 4096;
                    if should_update {
                        crate::net::tcp::send_tcp_packet(
                            sock.local_port,
                            sock.remote_ip,
                            sock.remote_port,
                            sock.tcb.local_seq,
                            sock.tcb.remote_seq,
                            crate::net::tcp::TCP_FLAG_ACK,
                            sock.tcb.local_win,
                            &[],
                        );
                    }
                }
                Ok(len)
            }
            None => {
                if sock.sock_type == SOCK_STREAM {
                    if sock.tcb.state == crate::net::tcp::TcpState::CloseWait
                        || sock.tcb.state == crate::net::tcp::TcpState::Closed
                        || sock.tcb.state == crate::net::tcp::TcpState::TimeWait
                    {
                        Ok(0) // Conexao encerrada pelo peer e sem dados pendentes (EOF)
                    } else {
                        Err(EAGAIN) // Sem dados no momento, conexao continua aberta
                    }
                } else {
                    Ok(0)
                }
            }
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
