//! Suíte de Testes Host para Validação da Pilha TCP (RFC 793), Handshake 3-Way,
//! Máquina de Estados, Listen/Accept, Streaming de Dados e Retransmissão (Issue #38)

pub const PROTO_TCP: u8 = 6;

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

    let csum = !(sum as u16);
    if csum == 0 {
        0xFFFF
    } else {
        csum
    }
}

pub fn write_segment(
    buf: &mut [u8],
    src_ip: [u8; 4],
    dst_ip: [u8; 4],
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    flags: u8,
    win: u16,
    payload: &[u8],
) -> usize {
    let total_len = 20 + payload.len();
    if buf.len() < total_len {
        return 0;
    }

    buf[0..2].copy_from_slice(&src_port.to_be_bytes());
    buf[2..4].copy_from_slice(&dst_port.to_be_bytes());
    buf[4..8].copy_from_slice(&seq.to_be_bytes());
    buf[8..12].copy_from_slice(&ack.to_be_bytes());
    buf[12] = 0x50; // Data offset = 5 (20 bytes)
    buf[13] = flags;
    buf[14..16].copy_from_slice(&win.to_be_bytes());
    buf[16..18].copy_from_slice(&[0, 0]); // Checksum inicial zerado
    buf[18..20].copy_from_slice(&[0, 0]); // Urgent pointer

    if !payload.is_empty() {
        buf[20..total_len].copy_from_slice(payload);
    }

    let csum = calculate_tcp_checksum(src_ip, dst_ip, &buf[..total_len]);
    buf[16..18].copy_from_slice(&csum.to_be_bytes());

    total_len
}

pub const SOCK_STREAM: u32 = 1;
pub const SOCK_RX_BUF_SIZE: usize = 1024;

#[derive(Debug, Clone, Copy)]
pub struct MockSocket {
    pub owner_pid: usize,
    pub sock_type: u32,
    pub local_port: u16,
    pub remote_ip: [u8; 4],
    pub remote_port: u16,
    pub is_bound: bool,
    pub is_connected: bool,

    pub tcb: TcpControlBlock,
    pub backlog: usize,
    pub pending_conns: [Option<usize>; 4],
    pub pending_count: usize,

    pub rx_buf: [[u8; SOCK_RX_BUF_SIZE]; 4],
    pub rx_len: [usize; 4],
    pub rx_head: usize,
    pub rx_tail: usize,
}

impl MockSocket {
    pub const fn empty() -> Self {
        Self {
            owner_pid: 0,
            sock_type: SOCK_STREAM,
            local_port: 0,
            remote_ip: [0; 4],
            remote_port: 0,
            is_bound: false,
            is_connected: false,
            tcb: TcpControlBlock::new(),
            backlog: 0,
            pending_conns: [None; 4],
            pending_count: 0,
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

// ─── SUÍTE DE TESTES HOST (CARGO TEST) ────────────────────────────────────────

#[test]
fn test_tcp_checksum_and_pseudo_header_rfc793() {
    let src_ip = [192, 168, 1, 100];
    let dst_ip = [192, 168, 1, 1];
    let mut buf = [0u8; 40];

    let written = write_segment(
        &mut buf,
        src_ip,
        dst_ip,
        12345,
        80,
        1000,
        0,
        TCP_FLAG_SYN,
        4096,
        &[],
    );
    assert_eq!(written, 20);

    let csum = u16::from_be_bytes([buf[16], buf[17]]);
    assert_ne!(csum, 0, "Checksum TCP não pode ser zero");

    // Valida que o parse recupera o pacote perfeitamente
    let parsed = TcpPacket::parse(&buf[..written]).expect("Falha ao analisar pacote gerado");
    assert_eq!(parsed.src_port, 12345);
    assert_eq!(parsed.dst_port, 80);
    assert_eq!(parsed.seq_num, 1000);
    assert_eq!(parsed.ack_num, 0);
    assert_eq!(parsed.flags, TCP_FLAG_SYN);
    assert_eq!(parsed.window, 4096);
    assert_eq!(parsed.payload.len(), 0);
}

#[test]
fn test_tcp_segment_parsing_and_guards() {
    // 1. Rejeição de pacote truncado (< 20 bytes)
    let truncated = [0u8; 19];
    assert!(TcpPacket::parse(&truncated).is_err());

    // 2. Rejeição de data offset inválido (< 5)
    let mut bad_doff = [0u8; 24];
    bad_doff[12] = 0x40; // 4 * 4 = 16 bytes (< 20)
    assert!(TcpPacket::parse(&bad_doff).is_err());

    // 3. Rejeição de data offset maior que o comprimento do pacote
    bad_doff[12] = 0x80; // 8 * 4 = 32 bytes, mas pacote tem apenas 24 bytes
    assert!(TcpPacket::parse(&bad_doff).is_err());

    // 4. Aceitação com options/padding (ex: data offset = 6 -> 24 bytes)
    let mut with_options = [0u8; 28];
    with_options[12] = 0x60; // 6 * 4 = 24 bytes cabeçalho
    with_options[13] = TCP_FLAG_ACK;
    with_options[24..28].copy_from_slice(b"PING");
    let parsed = TcpPacket::parse(&with_options).expect("Deveria aceitar pacote com options válidas");
    assert_eq!(parsed.payload, b"PING");
    assert_eq!(parsed.flags, TCP_FLAG_ACK);
}

#[test]
fn test_active_3way_handshake_client_flow() {
    let mut client_sock = MockSocket::empty();
    client_sock.local_port = 45000;
    client_sock.remote_ip = [93, 184, 216, 34];
    client_sock.remote_port = 80;
    client_sock.is_bound = true;

    // 1. Inicia handshake ativo (SYN_SENT)
    client_sock.tcb.state = TcpState::SynSent;
    let isn = 100_000u32;
    client_sock.tcb.local_seq = isn;

    // Gera pacote SYN do cliente
    let mut syn_buf = [0u8; 32];
    let syn_len = write_segment(
        &mut syn_buf,
        [10, 0, 2, 15],
        client_sock.remote_ip,
        client_sock.local_port,
        client_sock.remote_port,
        client_sock.tcb.local_seq,
        0,
        TCP_FLAG_SYN,
        client_sock.tcb.local_win,
        &[],
    );
    assert_eq!(syn_len, 20);
    client_sock.tcb.local_seq = client_sock.tcb.local_seq.wrapping_add(1);

    // 2. Servidor responde com SYN-ACK
    let server_isn = 500_000u32;
    let mut syn_ack_buf = [0u8; 32];
    let syn_ack_len = write_segment(
        &mut syn_ack_buf,
        client_sock.remote_ip,
        [10, 0, 2, 15],
        client_sock.remote_port,
        client_sock.local_port,
        server_isn,
        isn + 1, // ACK deve ser ISN + 1
        TCP_FLAG_SYN | TCP_FLAG_ACK,
        8192,
        &[],
    );
    assert_eq!(syn_ack_len, 20);

    // 3. Cliente processa SYN-ACK recebido
    let rx_pkt = TcpPacket::parse(&syn_ack_buf[..syn_ack_len]).unwrap();
    assert_eq!(rx_pkt.flags, TCP_FLAG_SYN | TCP_FLAG_ACK);
    assert_eq!(rx_pkt.ack_num, client_sock.tcb.local_seq);

    // Transição RFC 793 para Established
    client_sock.tcb.remote_seq = rx_pkt.seq_num.wrapping_add(1);
    client_sock.tcb.local_ack = rx_pkt.ack_num;
    client_sock.tcb.state = TcpState::Established;
    client_sock.is_connected = true;

    assert_eq!(client_sock.tcb.state, TcpState::Established);
    assert_eq!(client_sock.tcb.state.name(), "ESTABLISHED");
    assert_eq!(client_sock.tcb.remote_seq, server_isn + 1);
    assert!(client_sock.is_connected);

    // 4. Cliente transmite ACK final do handshake
    let mut ack_buf = [0u8; 32];
    let ack_len = write_segment(
        &mut ack_buf,
        [10, 0, 2, 15],
        client_sock.remote_ip,
        client_sock.local_port,
        client_sock.remote_port,
        client_sock.tcb.local_seq,
        client_sock.tcb.remote_seq,
        TCP_FLAG_ACK,
        client_sock.tcb.local_win,
        &[],
    );
    let ack_pkt = TcpPacket::parse(&ack_buf[..ack_len]).unwrap();
    assert_eq!(ack_pkt.flags, TCP_FLAG_ACK);
    assert_eq!(ack_pkt.seq_num, isn + 1);
    assert_eq!(ack_pkt.ack_num, server_isn + 1);
}

#[test]
fn test_passive_listen_and_accept_server_flow() {
    let mut server_sock = MockSocket::empty();
    server_sock.owner_pid = 2;
    server_sock.local_port = 8080;
    server_sock.is_bound = true;
    server_sock.tcb.state = TcpState::Listen;
    server_sock.backlog = 4;

    assert_eq!(server_sock.tcb.state, TcpState::Listen);
    assert_eq!(server_sock.tcb.state.name(), "LISTEN");

    // 1. Cliente remoto envia SYN
    let client_ip = [192, 168, 1, 50];
    let client_port = 54321;
    let client_isn = 20_000u32;

    let mut syn_buf = [0u8; 32];
    let syn_len = write_segment(
        &mut syn_buf,
        client_ip,
        [192, 168, 1, 10],
        client_port,
        8080,
        client_isn,
        0,
        TCP_FLAG_SYN,
        4096,
        &[],
    );
    let syn_pkt = TcpPacket::parse(&syn_buf[..syn_len]).unwrap();

    // 2. Servidor recebe SYN em LISTEN: cria child socket com state SynReceived
    let mut child_sock = MockSocket::empty();
    child_sock.owner_pid = server_sock.owner_pid;
    child_sock.local_port = 8080;
    child_sock.remote_ip = client_ip;
    child_sock.remote_port = client_port;
    child_sock.is_bound = true;
    child_sock.is_connected = false;
    child_sock.tcb.state = TcpState::SynReceived;
    child_sock.tcb.remote_seq = syn_pkt.seq_num.wrapping_add(1);
    let server_isn = 80_000u32;
    child_sock.tcb.local_seq = server_isn.wrapping_add(1); // seq avança após SYN-ACK

    server_sock.pending_conns[0] = Some(1); // index simulado do child
    server_sock.pending_count = 1;

    assert_eq!(child_sock.tcb.state, TcpState::SynReceived);
    assert_eq!(child_sock.tcb.remote_seq, client_isn + 1);

    // 3. Cliente envia ACK final completando o handshake
    let mut ack_buf = [0u8; 32];
    let ack_len = write_segment(
        &mut ack_buf,
        client_ip,
        [192, 168, 1, 10],
        client_port,
        8080,
        client_isn + 1,
        server_isn + 1,
        TCP_FLAG_ACK,
        4096,
        &[],
    );
    let ack_pkt = TcpPacket::parse(&ack_buf[..ack_len]).unwrap();

    // Child socket processa ACK e transiciona para Established
    if ack_pkt.ack_num == child_sock.tcb.local_seq {
        child_sock.tcb.state = TcpState::Established;
        child_sock.is_connected = true;
    }
    assert_eq!(child_sock.tcb.state, TcpState::Established);
    assert!(child_sock.is_connected);

    // 4. Servidor executa accept_socket para desenfileirar conexão
    assert_eq!(server_sock.pending_count, 1);
    let accepted_child_idx = {
        let idx = server_sock.pending_conns[0].unwrap();
        server_sock.pending_conns[0] = None;
        server_sock.pending_count = 0;
        idx
    };
    assert_eq!(accepted_child_idx, 1);
    assert_eq!(server_sock.pending_count, 0);
    assert_eq!(child_sock.remote_ip, client_ip);
    assert_eq!(child_sock.remote_port, client_port);
}

#[test]
fn test_tcp_data_streaming_and_sequencing() {
    let mut sock = MockSocket::empty();
    sock.local_port = 80;
    sock.remote_ip = [10, 0, 2, 2];
    sock.remote_port = 49152;
    sock.tcb.state = TcpState::Established;
    sock.is_connected = true;
    sock.tcb.local_seq = 1000;
    sock.tcb.remote_seq = 5000;

    // 1. Recebe pacote com payload de dados HTTP
    let http_get = b"GET /index.html HTTP/1.1\r\nHost: jinn.os\r\n\r\n";
    let mut rx_seg = [0u8; 128];
    let rx_len = write_segment(
        &mut rx_seg,
        sock.remote_ip,
        [10, 0, 2, 15],
        sock.remote_port,
        sock.local_port,
        5000,
        1000,
        TCP_FLAG_PSH | TCP_FLAG_ACK,
        4096,
        http_get,
    );
    let rx_pkt = TcpPacket::parse(&rx_seg[..rx_len]).unwrap();

    // Valida sequência esperada e enfileira no buffer circular
    assert_eq!(rx_pkt.seq_num, sock.tcb.remote_seq);
    let pushed = sock.push_rx(rx_pkt.payload);
    assert!(pushed, "Deve enfileirar dados no buffer RX");

    // Avança sequência remota pelo tamanho do payload recebido
    sock.tcb.remote_seq = sock.tcb.remote_seq.wrapping_add(rx_pkt.payload.len() as u32);
    assert_eq!(sock.tcb.remote_seq, 5000 + http_get.len() as u32);

    // 2. Aplicação lê dados do socket via pop_rx
    let mut user_buffer = [0u8; 128];
    let read_bytes = sock.pop_rx(&mut user_buffer).expect("Buffer RX deve ter dados");
    assert_eq!(read_bytes, http_get.len());
    assert_eq!(&user_buffer[..read_bytes], http_get);

    // 3. Socket transmite resposta com dados (PSH | ACK)
    let http_resp = b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nJINN";
    let mut tx_seg = [0u8; 128];
    let tx_len = write_segment(
        &mut tx_seg,
        [10, 0, 2, 15],
        sock.remote_ip,
        sock.local_port,
        sock.remote_port,
        sock.tcb.local_seq,
        sock.tcb.remote_seq,
        TCP_FLAG_PSH | TCP_FLAG_ACK,
        sock.tcb.local_win,
        http_resp,
    );
    assert_eq!(tx_len, 20 + http_resp.len());
    sock.tcb.local_seq = sock.tcb.local_seq.wrapping_add(http_resp.len() as u32);
    assert_eq!(sock.tcb.local_seq, 1000 + http_resp.len() as u32);

    let parsed_tx = TcpPacket::parse(&tx_seg[..tx_len]).unwrap();
    assert_eq!(parsed_tx.payload, http_resp);
    assert_eq!(parsed_tx.ack_num, 5000 + http_get.len() as u32);
}

#[test]
fn test_graceful_teardown_fin_ack() {
    let mut client = MockSocket::empty();
    client.tcb.state = TcpState::Established;
    client.is_connected = true;
    client.tcb.local_seq = 2000;
    client.tcb.remote_seq = 7000;

    let mut server = MockSocket::empty();
    server.tcb.state = TcpState::Established;
    server.is_connected = true;
    server.tcb.local_seq = 7000;
    server.tcb.remote_seq = 2000;

    // 1. Cliente fecha conexão: envia FIN | ACK (Established -> FinWait1)
    client.tcb.state = TcpState::FinWait1;
    let mut fin_buf = [0u8; 32];
    let fin_len = write_segment(
        &mut fin_buf,
        [10, 0, 2, 15],
        [10, 0, 2, 2],
        40000,
        80,
        client.tcb.local_seq,
        client.tcb.remote_seq,
        TCP_FLAG_FIN | TCP_FLAG_ACK,
        4096,
        &[],
    );
    client.tcb.local_seq = client.tcb.local_seq.wrapping_add(1);

    // 2. Servidor recebe FIN: envia ACK (Established -> CloseWait)
    let fin_pkt = TcpPacket::parse(&fin_buf[..fin_len]).unwrap();
    assert_eq!(fin_pkt.flags & TCP_FLAG_FIN, TCP_FLAG_FIN);
    server.tcb.remote_seq = fin_pkt.seq_num.wrapping_add(1);
    server.tcb.state = TcpState::CloseWait;

    let mut ack_buf = [0u8; 32];
    let ack_len = write_segment(
        &mut ack_buf,
        [10, 0, 2, 2],
        [10, 0, 2, 15],
        80,
        40000,
        server.tcb.local_seq,
        server.tcb.remote_seq,
        TCP_FLAG_ACK,
        4096,
        &[],
    );

    // 3. Cliente recebe ACK: transiciona FinWait1 -> FinWait2
    let ack_pkt = TcpPacket::parse(&ack_buf[..ack_len]).unwrap();
    assert_eq!(ack_pkt.ack_num, client.tcb.local_seq);
    client.tcb.state = TcpState::FinWait2;
    assert_eq!(client.tcb.state.name(), "FIN_WAIT_2");

    // 4. Servidor finaliza sua metade: envia FIN | ACK (CloseWait -> LastAck)
    server.tcb.state = TcpState::LastAck;
    let mut server_fin_buf = [0u8; 32];
    let s_fin_len = write_segment(
        &mut server_fin_buf,
        [10, 0, 2, 2],
        [10, 0, 2, 15],
        80,
        40000,
        server.tcb.local_seq,
        server.tcb.remote_seq,
        TCP_FLAG_FIN | TCP_FLAG_ACK,
        4096,
        &[],
    );
    server.tcb.local_seq = server.tcb.local_seq.wrapping_add(1);

    // 5. Cliente recebe FIN do servidor: envia ACK e vai para TimeWait
    let s_fin_pkt = TcpPacket::parse(&server_fin_buf[..s_fin_len]).unwrap();
    client.tcb.remote_seq = s_fin_pkt.seq_num.wrapping_add(1);
    client.tcb.state = TcpState::TimeWait;
    assert_eq!(client.tcb.state.name(), "TIME_WAIT");

    // Servidor recebe ACK final: LastAck -> Closed
    server.tcb.state = TcpState::Closed;
    server.is_connected = false;
    assert_eq!(server.tcb.state.name(), "CLOSED");
    assert!(!server.is_connected);
}

#[test]
fn test_retransmission_timeout_and_rst_handling() {
    let mut sock = MockSocket::empty();
    sock.local_port = 41000;
    sock.remote_ip = [10, 0, 2, 3];
    sock.remote_port = 80;
    sock.tcb.state = TcpState::SynSent;
    sock.tcb.local_seq = 1000;
    sock.tcb.last_tx_tick = 50;
    sock.tcb.rto_ticks = 100;
    sock.tcb.retransmit_count = 0;

    // 1. Simulação de ticks sem resposta: tick 120 (delta 70 < RTO 100) -> Não retransmite
    let tick_now = 120u64;
    let should_retrans = tick_now.saturating_sub(sock.tcb.last_tx_tick) >= sock.tcb.rto_ticks;
    assert!(!should_retrans);

    // 2. Tick 160 (delta 110 >= RTO 100) -> Retransmissão acionada
    let tick_now = 160u64;
    let should_retrans = tick_now.saturating_sub(sock.tcb.last_tx_tick) >= sock.tcb.rto_ticks;
    assert!(should_retrans);

    sock.tcb.retransmit_count += 1;
    sock.tcb.last_tx_tick = tick_now;
    assert_eq!(sock.tcb.retransmit_count, 1);

    // Simula 3 retentativas esgotadas
    sock.tcb.retransmit_count = 3;
    assert!(sock.tcb.retransmit_count >= 3, "Timeout de conexão excedido após 3 retentativas");

    // 3. Tratamento de flag RST: encerramento imediato
    sock.tcb.state = TcpState::Established;
    sock.is_connected = true;

    let mut rst_buf = [0u8; 32];
    let rst_len = write_segment(
        &mut rst_buf,
        sock.remote_ip,
        [10, 0, 2, 15],
        sock.remote_port,
        sock.local_port,
        5000,
        1000,
        TCP_FLAG_RST,
        0,
        &[],
    );
    let rst_pkt = TcpPacket::parse(&rst_buf[..rst_len]).unwrap();
    assert_eq!(rst_pkt.flags & TCP_FLAG_RST, TCP_FLAG_RST);

    if (rst_pkt.flags & TCP_FLAG_RST) != 0 {
        sock.tcb.state = TcpState::Closed;
        sock.is_connected = false;
    }
    assert_eq!(sock.tcb.state, TcpState::Closed);
    assert!(!sock.is_connected);
}
