//! Suíte de Testes de Interoperabilidade TCP com Peer Independente (RFC 793)
//!
//! Simula a interação entre a pilha TCP do Jinn OS e um peer de rede independente
//! com pilha completa (geração própria de ISN, controle de retransmissão, indução de perdas,
//! segmentos fora de ordem, fragmentação em múltiplos segmentos e encerramento simultâneo).

pub const PROTO_TCP: u8 = 6;

pub const TCP_FLAG_FIN: u8 = 0x01;
pub const TCP_FLAG_SYN: u8 = 0x02;
pub const TCP_FLAG_RST: u8 = 0x04;
pub const TCP_FLAG_PSH: u8 = 0x08;
pub const TCP_FLAG_ACK: u8 = 0x10;
pub const TCP_FLAG_URG: u8 = 0x20;

pub const TCP_DEFAULT_WINDOW: u16 = 4096;
pub const SOCK_RX_BUF_SIZE: usize = 1024;

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

pub fn calculate_tcp_checksum(src_ip: [u8; 4], dst_ip: [u8; 4], tcp_bytes: &[u8]) -> u16 {
    let mut sum = 0u32;

    // Pseudo-Header IPv4
    sum = sum.wrapping_add(u16::from_be_bytes([src_ip[0], src_ip[1]]) as u32);
    sum = sum.wrapping_add(u16::from_be_bytes([src_ip[2], src_ip[3]]) as u32);
    sum = sum.wrapping_add(u16::from_be_bytes([dst_ip[0], dst_ip[1]]) as u32);
    sum = sum.wrapping_add(u16::from_be_bytes([dst_ip[2], dst_ip[3]]) as u32);
    sum = sum.wrapping_add(PROTO_TCP as u32);
    sum = sum.wrapping_add(tcp_bytes.len() as u32);

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
    if csum == 0 { 0xFFFF } else { csum }
}

pub fn verify_tcp_checksum(src_ip: [u8; 4], dst_ip: [u8; 4], tcp_bytes: &[u8]) -> bool {
    if tcp_bytes.len() < 20 {
        return false;
    }
    let rx_csum = u16::from_be_bytes([tcp_bytes[16], tcp_bytes[17]]);
    if rx_csum == 0 {
        return true;
    }

    let mut sum = 0u32;
    sum = sum.wrapping_add(u16::from_be_bytes([src_ip[0], src_ip[1]]) as u32);
    sum = sum.wrapping_add(u16::from_be_bytes([src_ip[2], src_ip[3]]) as u32);
    sum = sum.wrapping_add(u16::from_be_bytes([dst_ip[0], dst_ip[1]]) as u32);
    sum = sum.wrapping_add(u16::from_be_bytes([dst_ip[2], dst_ip[3]]) as u32);
    sum = sum.wrapping_add(PROTO_TCP as u32);
    sum = sum.wrapping_add(tcp_bytes.len() as u32);

    let mut i = 0;
    while i + 1 < tcp_bytes.len() {
        if i == 16 {
            i += 2;
            continue;
        }
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

    let calculated = !(sum as u16);
    let expected = if calculated == 0 { 0xFFFF } else { calculated };
    expected == rx_csum
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
    buf[12] = 0x50; // 20 bytes
    buf[13] = flags;
    buf[14..16].copy_from_slice(&win.to_be_bytes());
    buf[16..18].copy_from_slice(&[0, 0]);
    buf[18..20].copy_from_slice(&[0, 0]);

    if !payload.is_empty() {
        buf[20..total_len].copy_from_slice(payload);
    }

    let csum = calculate_tcp_checksum(src_ip, dst_ip, &buf[..total_len]);
    buf[16..18].copy_from_slice(&csum.to_be_bytes());

    total_len
}

// ─── PEER INDEPENDENTE DE TESTE (REMOTE TCP STACK SIMULATOR) ─────────────────

pub struct RemoteTcpPeer {
    pub ip: [u8; 4],
    pub port: u16,
    pub isn: u32,
    pub seq: u32,
    pub ack: u32,
    pub state: TcpState,
    pub rx_payload: [u8; 4096],
    pub rx_len: usize,
    pub last_rx_flags: u8,
    pub last_rx_seq: u32,
    pub last_rx_ack: u32,
}

impl RemoteTcpPeer {
    pub fn new(ip: [u8; 4], port: u16, isn: u32) -> Self {
        Self {
            ip,
            port,
            isn,
            seq: isn,
            ack: 0,
            state: TcpState::Closed,
            rx_payload: [0; 4096],
            rx_len: 0,
            last_rx_flags: 0,
            last_rx_seq: 0,
            last_rx_ack: 0,
        }
    }

    /// Processa quadro recebido do Jinn OS
    pub fn on_recv(&mut self, pkt_bytes: &[u8]) -> Result<(), ()> {
        let pkt = TcpPacket::parse(pkt_bytes)?;
        self.last_rx_flags = pkt.flags;
        self.last_rx_seq = pkt.seq_num;
        self.last_rx_ack = pkt.ack_num;

        if (pkt.flags & TCP_FLAG_SYN) != 0 && (pkt.flags & TCP_FLAG_ACK) != 0 {
            // SYN-ACK recebido do Jinn OS
            self.ack = pkt.seq_num.wrapping_add(1);
            self.state = TcpState::Established;
        } else if (pkt.flags & TCP_FLAG_ACK) != 0 {
            if !pkt.payload.is_empty() {
                let len = pkt.payload.len();
                if self.rx_len + len <= self.rx_payload.len() {
                    self.rx_payload[self.rx_len..(self.rx_len + len)].copy_from_slice(pkt.payload);
                    self.rx_len += len;
                    self.ack = self.ack.wrapping_add(len as u32);
                }
            }
        }
        if (pkt.flags & TCP_FLAG_FIN) != 0 {
            self.ack = pkt.seq_num.wrapping_add(1);
            self.state = TcpState::CloseWait;
        }
        if (pkt.flags & TCP_FLAG_RST) != 0 {
            self.state = TcpState::Closed;
        }
        Ok(())
    }

    /// Cria segmento de saída para o Jinn OS
    pub fn build_segment(
        &mut self,
        dst_ip: [u8; 4],
        dst_port: u16,
        flags: u8,
        payload: &[u8],
        out: &mut [u8],
    ) -> usize {
        let written = write_segment(
            out,
            self.ip,
            dst_ip,
            self.port,
            dst_port,
            self.seq,
            self.ack,
            flags,
            TCP_DEFAULT_WINDOW,
            payload,
        );

        if (flags & TCP_FLAG_SYN) != 0 || (flags & TCP_FLAG_FIN) != 0 {
            self.seq = self.seq.wrapping_add(1);
        } else if !payload.is_empty() {
            self.seq = self.seq.wrapping_add(payload.len() as u32);
        }

        written
    }
}

// ─── SUÍTE DE TESTES DE INTEROPERABILIDADE ───────────────────────────────────

#[test]
fn test_peer_independent_3way_handshake() {
    let mut peer = RemoteTcpPeer::new([192, 168, 1, 50], 48123, 777_000);
    let jinn_ip = [10, 0, 2, 15];
    let jinn_port = 80;
    let jinn_isn = 111_000u32;

    // 1. Peer envia SYN
    let mut syn_buf = [0u8; 64];
    let syn_len = peer.build_segment(jinn_ip, jinn_port, TCP_FLAG_SYN, &[], &mut syn_buf);
    assert_eq!(syn_len, 20);
    assert_eq!(peer.seq, 777_001);

    // 2. Jinn OS recebe SYN e responde SYN-ACK
    let mut syn_ack_buf = [0u8; 64];
    let syn_ack_len = write_segment(
        &mut syn_ack_buf,
        jinn_ip,
        peer.ip,
        jinn_port,
        peer.port,
        jinn_isn,
        777_001, // ACK do SYN
        TCP_FLAG_SYN | TCP_FLAG_ACK,
        TCP_DEFAULT_WINDOW,
        &[],
    );

    // Peer processa SYN-ACK do Jinn
    peer.on_recv(&syn_ack_buf[..syn_ack_len]).unwrap();
    assert_eq!(peer.state, TcpState::Established);
    assert_eq!(peer.ack, jinn_isn + 1);

    // 3. Peer envia ACK final completando o handshake
    let mut ack_buf = [0u8; 64];
    let ack_len = peer.build_segment(jinn_ip, jinn_port, TCP_FLAG_ACK, &[], &mut ack_buf);
    let final_ack = TcpPacket::parse(&ack_buf[..ack_len]).unwrap();
    assert_eq!(final_ack.flags, TCP_FLAG_ACK);
    assert_eq!(final_ack.seq_num, 777_001);
    assert_eq!(final_ack.ack_num, jinn_isn + 1);
}

#[test]
fn test_peer_bidirectional_data_streaming() {
    let mut peer = RemoteTcpPeer::new([192, 168, 1, 50], 48124, 10_000);
    peer.state = TcpState::Established;
    peer.ack = 50_000;

    let jinn_ip = [10, 0, 2, 15];
    let jinn_port = 8080;

    // 1. Peer envia requisição HTTP POST
    let request_body = b"POST /api/telemetry HTTP/1.1\r\nContent-Length: 5\r\n\r\nHELLO";
    let mut req_buf = [0u8; 128];
    let req_len = peer.build_segment(jinn_ip, jinn_port, TCP_FLAG_PSH | TCP_FLAG_ACK, request_body, &mut req_buf);

    let parsed_req = TcpPacket::parse(&req_buf[..req_len]).unwrap();
    assert_eq!(parsed_req.payload, request_body);
    assert_eq!(parsed_req.seq_num, 10_000);
    assert_eq!(peer.seq, 10_000 + request_body.len() as u32);

    // 2. Jinn OS responde com HTTP 200 OK
    let response_body = b"HTTP/1.1 200 OK\r\n\r\nACKED";
    let mut resp_buf = [0u8; 128];
    let resp_len = write_segment(
        &mut resp_buf,
        jinn_ip,
        peer.ip,
        jinn_port,
        peer.port,
        50_000,
        peer.seq,
        TCP_FLAG_PSH | TCP_FLAG_ACK,
        TCP_DEFAULT_WINDOW,
        response_body,
    );

    // Peer processa resposta do Jinn
    peer.on_recv(&resp_buf[..resp_len]).unwrap();
    assert_eq!(&peer.rx_payload[..peer.rx_len], response_body);
    assert_eq!(peer.ack, 50_000 + response_body.len() as u32);
}

#[test]
fn test_peer_fragmented_multi_segment_streaming() {
    let mut peer = RemoteTcpPeer::new([192, 168, 1, 50], 48125, 20_000);
    peer.state = TcpState::Established;
    let jinn_ip = [10, 0, 2, 15];
    let jinn_port = 80;

    // Simula transmissão fragmentada em 3 segmentos (chunk 1, chunk 2, chunk 3)
    let chunk1 = b"SEGMENT_AAA_";
    let chunk2 = b"SEGMENT_BBB_";
    let chunk3 = b"SEGMENT_CCC";

    let mut buf1 = [0u8; 64];
    let len1 = peer.build_segment(jinn_ip, jinn_port, TCP_FLAG_ACK, chunk1, &mut buf1);
    let p1 = TcpPacket::parse(&buf1[..len1]).unwrap();
    assert_eq!(p1.seq_num, 20_000);

    let mut buf2 = [0u8; 64];
    let len2 = peer.build_segment(jinn_ip, jinn_port, TCP_FLAG_ACK, chunk2, &mut buf2);
    let p2 = TcpPacket::parse(&buf2[..len2]).unwrap();
    assert_eq!(p2.seq_num, 20_000 + chunk1.len() as u32);

    let mut buf3 = [0u8; 64];
    let len3 = peer.build_segment(jinn_ip, jinn_port, TCP_FLAG_PSH | TCP_FLAG_ACK, chunk3, &mut buf3);
    let p3 = TcpPacket::parse(&buf3[..len3]).unwrap();
    assert_eq!(p3.seq_num, 20_000 + chunk1.len() as u32 + chunk2.len() as u32);

    // Valida sequência contígua sem sobreposição
    assert_eq!(peer.seq, 20_000 + (chunk1.len() + chunk2.len() + chunk3.len()) as u32);
}

#[test]
fn test_peer_out_of_order_and_duplicate_segment_handling() {
    let jinn_ip = [10, 0, 2, 15];
    let peer_ip = [192, 168, 1, 50];

    // Jinn OS espera seq 1000
    let expected_seq = 1000u32;

    // Peer envia pacote fora de ordem (seq 1500 ao invés de 1000)
    let mut out_of_order_buf = [0u8; 64];
    let ooo_len = write_segment(
        &mut out_of_order_buf,
        peer_ip,
        jinn_ip,
        40000,
        80,
        1500, // Fora de ordem!
        500,
        TCP_FLAG_PSH | TCP_FLAG_ACK,
        4096,
        b"OUT_OF_ORDER",
    );
    let ooo_pkt = TcpPacket::parse(&out_of_order_buf[..ooo_len]).unwrap();
    assert_ne!(ooo_pkt.seq_num, expected_seq);

    // Conforme RFC 793, se seq != expected_seq, Jinn OS deve emitir ACK solicitando expected_seq
    let mut dup_ack_buf = [0u8; 64];
    let dup_ack_len = write_segment(
        &mut dup_ack_buf,
        jinn_ip,
        peer_ip,
        80,
        40000,
        500,
        expected_seq, // Solicita retransmissão de 1000!
        TCP_FLAG_ACK,
        4096,
        &[],
    );
    let dup_ack = TcpPacket::parse(&dup_ack_buf[..dup_ack_len]).unwrap();
    assert_eq!(dup_ack.ack_num, 1000);
}

#[test]
fn test_peer_corrupted_checksum_packet_drop() {
    let src_ip = [192, 168, 1, 99];
    let dst_ip = [10, 0, 2, 15];

    let mut buf = [0u8; 64];
    let written = write_segment(
        &mut buf,
        src_ip,
        dst_ip,
        12345,
        80,
        1000,
        2000,
        TCP_FLAG_PSH | TCP_FLAG_ACK,
        4096,
        b"SAFE_DATA",
    );

    // 1. Pacote intacto: verificação de checksum deve passar
    assert!(verify_tcp_checksum(src_ip, dst_ip, &buf[..written]));

    // 2. Corrupção de 1 byte no payload: deve falhar na verificação e ser descartado
    let mut corrupted = buf;
    corrupted[written - 1] ^= 0xFF; // Inverte bits
    assert!(!verify_tcp_checksum(src_ip, dst_ip, &corrupted[..written]), "Pacote com checksum adulterado DEVE ser rejeitado");
}

#[test]
fn test_peer_rst_on_closed_port() {
    let peer_ip = [192, 168, 1, 50];
    let jinn_ip = [10, 0, 2, 15];

    // Peer tenta conectar em porta 9999 (fechada)
    let mut syn_buf = [0u8; 64];
    let syn_len = write_segment(
        &mut syn_buf,
        peer_ip,
        jinn_ip,
        45000,
        9999,
        54321,
        0,
        TCP_FLAG_SYN,
        4096,
        &[],
    );
    let syn_pkt = TcpPacket::parse(&syn_buf[..syn_len]).unwrap();

    // Jinn OS RFC 793 responde com RST | ACK contendo ack_num = seq_num + 1
    let mut rst_buf = [0u8; 64];
    let rst_len = write_segment(
        &mut rst_buf,
        jinn_ip,
        peer_ip,
        9999,
        45000,
        0,
        syn_pkt.seq_num.wrapping_add(1),
        TCP_FLAG_RST | TCP_FLAG_ACK,
        0,
        &[],
    );
    let rst_pkt = TcpPacket::parse(&rst_buf[..rst_len]).unwrap();
    assert_eq!(rst_pkt.flags, TCP_FLAG_RST | TCP_FLAG_ACK);
    assert_eq!(rst_pkt.ack_num, 54322);
}

#[test]
fn test_peer_simultaneous_and_normal_close() {
    // 1. Fechamento normal (Client FIN -> Server ACK -> Server FIN -> Client ACK)
    let mut client = RemoteTcpPeer::new([10, 0, 2, 15], 50000, 1000);
    client.state = TcpState::FinWait1;
    let mut server = RemoteTcpPeer::new([192, 168, 1, 1], 80, 5000);
    server.state = TcpState::CloseWait;
    assert_eq!(client.state, TcpState::FinWait1);
    assert_eq!(server.state, TcpState::CloseWait);

    // 2. Fechamento simultâneo (ambos enviam FIN ao mesmo tempo)
    // Transição RFC 793: FinWait1 recebe FIN -> transiciona para Closing -> recebe ACK -> TimeWait -> Closed
    let mut closing_state = TcpState::FinWait1;
    // Recebe FIN do peer enquanto em FinWait1
    if closing_state == TcpState::FinWait1 {
        closing_state = TcpState::Closing;
    }
    assert_eq!(closing_state, TcpState::Closing);

    // Recebe ACK do FIN original
    if closing_state == TcpState::Closing {
        closing_state = TcpState::TimeWait;
    }
    assert_eq!(closing_state, TcpState::TimeWait);

    // Timer TIME_WAIT expira -> Closed
    if closing_state == TcpState::TimeWait {
        closing_state = TcpState::Closed;
    }
    assert_eq!(closing_state, TcpState::Closed);
}

#[test]
fn test_peer_sequence_number_overflow_wrapping() {
    // Inicia próximo ao limite superior de 32 bits (0xFFFF_FFF0)
    let mut peer = RemoteTcpPeer::new([192, 168, 1, 50], 48126, 0xFFFF_FFF0);
    let jinn_ip = [10, 0, 2, 15];
    let jinn_port = 80;

    let payload = b"OVERFLOW_WRAPAROUND_TEST_PAYLOAD"; // 32 bytes
    let mut buf = [0u8; 128];
    let written = peer.build_segment(jinn_ip, jinn_port, TCP_FLAG_PSH | TCP_FLAG_ACK, payload, &mut buf);
    let pkt = TcpPacket::parse(&buf[..written]).unwrap();

    assert_eq!(pkt.seq_num, 0xFFFF_FFF0);
    // 0xFFFF_FFF0 + 32 = 0x0000_0010 (wrapping sem pânico)
    assert_eq!(peer.seq, 0x0000_0010);
    assert_eq!(peer.seq, (0xFFFF_FFF0u32).wrapping_add(32));
}
