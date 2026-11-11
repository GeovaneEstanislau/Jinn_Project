//! Jinn OS — Testes Unitários e de Estresse de Sockets TCP/IP e Fluxo Contínuo de Rede
//!
//! Valida:
//! 1. Buffer circular de fluxo contínuo (Stream Ring Buffer de 32 KiB)
//! 2. Resolução de leituras parciais (eliminação de descarte de bytes não lidos)
//! 3. Controle de fluxo por janela deslizante (Zero Window e Window Update)
//! 4. Reassentamento e avanço de sequência TCP em segmentos contíguos e parciais
//! 5. Segmentos sobrepostos (overlap) e fora de ordem (OOO)
//! 6. Enquadramento e decodificação HTTP/1.1 (Content-Length e Chunked Transfer)
//! 7. Simulação de barramento loopback com handshake de 3 vias e transferência de 16 KiB

const SOCK_RX_STREAM_SIZE: usize = 65536;

struct TestSocketStream {
    buf: [u8; SOCK_RX_STREAM_SIZE],
    head: usize,
    tail: usize,
    len: usize,
    remote_seq: u32,
    local_win: u16,
    ooo_buf: [u8; 4096],
    ooo_seq: u32,
    ooo_len: usize,
}

impl TestSocketStream {
    fn new(initial_seq: u32) -> Self {
        Self {
            buf: [0u8; SOCK_RX_STREAM_SIZE],
            head: 0,
            tail: 0,
            len: 0,
            remote_seq: initial_seq,
            local_win: SOCK_RX_STREAM_SIZE.min(65535) as u16,
            ooo_buf: [0u8; 4096],
            ooo_seq: 0,
            ooo_len: 0,
        }
    }

    fn available_space(&self) -> usize {
        SOCK_RX_STREAM_SIZE.saturating_sub(self.len)
    }

    fn push_rx(&mut self, data: &[u8]) -> usize {
        let avail = self.available_space();
        let to_store = data.len().min(avail);
        if to_store == 0 {
            return 0;
        }

        let part1 = to_store.min(SOCK_RX_STREAM_SIZE - self.head);
        self.buf[self.head..self.head + part1].copy_from_slice(&data[..part1]);
        let part2 = to_store - part1;
        if part2 > 0 {
            self.buf[..part2].copy_from_slice(&data[part1..to_store]);
        }

        self.head = (self.head + to_store) % SOCK_RX_STREAM_SIZE;
        self.len += to_store;
        self.local_win = self.available_space().min(65535) as u16;
        to_store
    }

    fn pop_rx(&mut self, dest: &mut [u8]) -> Option<usize> {
        if self.len == 0 {
            return None;
        }
        let to_read = dest.len().min(self.len);
        if to_read == 0 {
            return Some(0);
        }

        let part1 = to_read.min(SOCK_RX_STREAM_SIZE - self.tail);
        dest[..part1].copy_from_slice(&self.buf[self.tail..self.tail + part1]);
        let part2 = to_read - part1;
        if part2 > 0 {
            dest[part1..to_read].copy_from_slice(&self.buf[..part2]);
        }

        self.tail = (self.tail + to_read) % SOCK_RX_STREAM_SIZE;
        self.len -= to_read;
        self.local_win = self.available_space().min(65535) as u16;
        Some(to_read)
    }

    fn drain_and_realign_ooo(&mut self) {
        if self.ooo_len == 0 {
            return;
        }

        // Caso 1: OOO totalmente eclipsado
        if (self.remote_seq.wrapping_sub(self.ooo_seq) as i32) >= (self.ooo_len as i32) {
            self.ooo_len = 0;
            return;
        }

        // Caso 2: OOO parcialmente eclipsado
        if (self.remote_seq.wrapping_sub(self.ooo_seq) as i32) > 0 {
            let overlap = self.remote_seq.wrapping_sub(self.ooo_seq) as usize;
            if overlap < self.ooo_len {
                self.ooo_buf.copy_within(overlap..self.ooo_len, 0);
                self.ooo_len -= overlap;
                self.ooo_seq = self.remote_seq;
            } else {
                self.ooo_len = 0;
                return;
            }
        }

        // Caso 3: Início do OOO conecta contiguamente com remote_seq
        if self.ooo_seq == self.remote_seq && self.ooo_len > 0 {
            let mut temp = [0u8; 4096];
            let o_len = self.ooo_len.min(4096);
            temp[..o_len].copy_from_slice(&self.ooo_buf[..o_len]);
            let o_stored = self.push_rx(&temp[..o_len]);
            self.remote_seq = self.remote_seq.wrapping_add(o_stored as u32);
            if o_stored == o_len {
                self.ooo_len = 0;
            } else {
                self.ooo_buf.copy_within(o_stored..o_len, 0);
                self.ooo_len -= o_stored;
                self.ooo_seq = self.ooo_seq.wrapping_add(o_stored as u32);
            }
        }
    }

    fn process_incoming_tcp_payload(&mut self, seq_num: u32, payload: &[u8]) -> (u32, u16) {
        if payload.is_empty() {
            return (self.remote_seq, self.local_win);
        }

        let diff = seq_num.wrapping_sub(self.remote_seq) as i32;

        if diff == 0 {
            let stored = self.push_rx(payload);
            self.remote_seq = self.remote_seq.wrapping_add(stored as u32);
            self.drain_and_realign_ooo();
        } else if diff < 0 {
            let seg_end_diff = seq_num.wrapping_add(payload.len() as u32).wrapping_sub(self.remote_seq) as i32;
            if seg_end_diff > 0 {
                let skip = self.remote_seq.wrapping_sub(seq_num) as usize;
                if skip < payload.len() {
                    let new_data = &payload[skip..];
                    let stored = self.push_rx(new_data);
                    self.remote_seq = self.remote_seq.wrapping_add(stored as u32);
                    self.drain_and_realign_ooo();
                }
            }
        } else {
            // Out-of-order
            if (diff as usize) <= (self.local_win as usize) {
                if self.ooo_len == 0 {
                    let len = payload.len().min(4096);
                    self.ooo_buf[..len].copy_from_slice(&payload[..len]);
                    self.ooo_seq = seq_num;
                    self.ooo_len = len;
                } else if seq_num == self.ooo_seq.wrapping_add(self.ooo_len as u32) {
                    let append_len = payload.len().min(4096 - self.ooo_len);
                    if append_len > 0 {
                        self.ooo_buf[self.ooo_len..self.ooo_len + append_len]
                            .copy_from_slice(&payload[..append_len]);
                        self.ooo_len += append_len;
                    }
                }
            }
        }

        (self.remote_seq, self.local_win)
    }
}

// ── Testes ────────────────────────────────────────────────────────────────────

#[test]
fn test_socket_stream_partial_read_preserves_remaining_bytes() {
    let mut sock = TestSocketStream::new(1000);

    // Cria um pacote MTU de 1460 bytes com padrão conhecido
    let mut packet = [0u8; 1460];
    for (i, b) in packet.iter_mut().enumerate() {
        *b = (i % 251) as u8;
    }

    assert_eq!(sock.push_rx(&packet), 1460);
    assert_eq!(sock.len, 1460);

    // 1ª leitura parcial: 100 bytes (ex: leitor de headers linha a linha)
    let mut chunk1 = [0u8; 100];
    let n1 = sock.pop_rx(&mut chunk1).expect("deve retornar 100 bytes");
    assert_eq!(n1, 100);
    assert_eq!(&chunk1[..], &packet[0..100]);
    assert_eq!(sock.len, 1360, "Os 1360 bytes restantes devem ser rigorosamente preservados no buffer!");

    // 2ª leitura parcial: 250 bytes
    let mut chunk2 = [0u8; 250];
    let n2 = sock.pop_rx(&mut chunk2).expect("deve retornar 250 bytes");
    assert_eq!(n2, 250);
    assert_eq!(&chunk2[..], &packet[100..350]);
    assert_eq!(sock.len, 1110);

    // 3ª leitura: o restante (1110 bytes) em buffer maior
    let mut chunk3 = [0u8; 2048];
    let n3 = sock.pop_rx(&mut chunk3).expect("deve retornar 1110 bytes");
    assert_eq!(n3, 1110);
    assert_eq!(&chunk3[..1110], &packet[350..1460]);
    assert_eq!(sock.len, 0);

    // Buffer agora deve estar vazio
    assert_eq!(sock.pop_rx(&mut chunk1), None);
}

#[test]
fn test_socket_stream_circular_wraparound_integrity() {
    let mut sock = TestSocketStream::new(0);

    // Padrão determinístico
    let make_data = |start_val: usize, count: usize| -> Vec<u8> {
        (0..count).map(|i| ((start_val + i) % 256) as u8).collect()
    };

    // 1. Preenche 24 KiB
    let d1 = make_data(0, 24576);
    assert_eq!(sock.push_rx(&d1), 24576);

    // 2. Consome 20 KiB (tail avança para 20480)
    let mut out1 = vec![0u8; 20480];
    assert_eq!(sock.pop_rx(&mut out1), Some(20480));
    assert_eq!(&out1[..], &d1[..20480]);
    assert_eq!(sock.len, 4096);

    // 3. Insere mais 20 KiB (ultrapassa a barreira dos 32 KiB e faz wrap no head)
    let d2 = make_data(20480, 20480);
    assert_eq!(sock.push_rx(&d2), 20480);
    assert_eq!(sock.len, 24576);

    // 4. Consome os 24576 bytes totais (faz wrap no tail)
    let mut out2 = vec![0u8; 24576];
    assert_eq!(sock.pop_rx(&mut out2), Some(24576));
    assert_eq!(sock.len, 0);

    // Validação de integridade byte a byte
    let expected_stream = [d1[20480..].to_vec(), d2].concat();
    assert_eq!(out2, expected_stream);
}

#[test]
fn test_socket_flow_control_zero_window_and_reopen() {
    let mut sock = TestSocketStream::new(5000);

    let large_chunk = vec![0xAAu8; SOCK_RX_STREAM_SIZE];
    let stored = sock.push_rx(&large_chunk);
    assert_eq!(stored, SOCK_RX_STREAM_SIZE);
    assert_eq!(sock.available_space(), 0);
    assert_eq!(sock.local_win, 0, "Janela local anunciada deve ser 0 quando buffer está cheio!");

    // Tentativa de push adicional deve retornar 0 (Zero Window)
    let extra = [0xFFu8; 512];
    assert_eq!(sock.push_rx(&extra), 0);

    // Consome 4 KiB da aplicação
    let mut drain_buf = [0u8; 4096];
    assert_eq!(sock.pop_rx(&mut drain_buf), Some(4096));
    assert_eq!(sock.available_space(), 4096);
    assert_eq!(sock.local_win, 4096, "Janela local deve reabrir exatamente para 4096 bytes!");

    // Agora o push adicional deve ser aceito
    assert_eq!(sock.push_rx(&extra), 512);
    assert_eq!(sock.available_space(), 4096 - 512);
}

#[test]
fn test_tcp_sequence_tracking_contiguous_and_partial() {
    let mut sock = TestSocketStream::new(100_000);

    // 1. Segmento contíguo regular
    let seg1 = [1u8; 1000];
    let (ack1, win1) = sock.process_incoming_tcp_payload(100_000, &seg1);
    assert_eq!(ack1, 101_000);
    assert_eq!(win1 as usize, SOCK_RX_STREAM_SIZE - 1000);

    // 2. Segmento contíguo seguinte
    let seg2 = [2u8; 1500];
    let (ack2, win2) = sock.process_incoming_tcp_payload(101_000, &seg2);
    assert_eq!(ack2, 102_500);
    assert_eq!(win2 as usize, SOCK_RX_STREAM_SIZE - 2500);

    // 3. Simula buffer quase cheio para forçar absorção parcial
    let remaining_before = sock.available_space();
    let filler = vec![3u8; remaining_before - 200];
    sock.push_rx(&filler);
    sock.remote_seq = sock.remote_seq.wrapping_add(filler.len() as u32);
    assert_eq!(sock.available_space(), 200);

    // Chega segmento de 500 bytes no ponto de sequência esperado
    let seg_large = [4u8; 500];
    let (ack3, win3) = sock.process_incoming_tcp_payload(sock.remote_seq, &seg_large);
    assert_eq!(win3, 0, "Janela deve fechar para 0");
    // remote_seq avançou APENAS 200 bytes (o que coube no buffer!), e NÃO 500!
    assert_eq!(ack3, sock.remote_seq);
}

#[test]
fn test_tcp_overlapping_segments_resilience() {
    let mut sock = TestSocketStream::new(50_000);

    // Envia bytes 0..1000 com seq 50_000
    let part_a: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();
    let (ack1, _) = sock.process_incoming_tcp_payload(50_000, &part_a);
    assert_eq!(ack1, 51_000);

    // Envia segmento sobreposto: começa em 50_600 (400 bytes já vistos) e vai até 51_400 (400 novos bytes)
    let mut part_overlap = Vec::new();
    for i in 600..1400 {
        part_overlap.push((i % 256) as u8);
    }
    let (ack2, _) = sock.process_incoming_tcp_payload(50_600, &part_overlap);
    assert_eq!(ack2, 51_400, "ACK deve avançar apenas pelos 400 bytes novos!");
    assert_eq!(sock.len, 1400);

    // Verifica que o buffer contém exatamente a sequência contínua 0..1400 sem duplicações
    let mut read_all = vec![0u8; 1400];
    assert_eq!(sock.pop_rx(&mut read_all), Some(1400));
    let expected: Vec<u8> = (0..1400).map(|i| (i % 256) as u8).collect();
    assert_eq!(read_all, expected);
}

#[test]
fn test_tcp_out_of_order_reassembly_and_merge() {
    let mut sock = TestSocketStream::new(10_000);

    let seg1: Vec<u8> = (0..500).map(|i| (i % 256) as u8).collect();
    let seg2_gap: Vec<u8> = (500..1000).map(|i| (i % 256) as u8).collect();
    let seg3_ooo: Vec<u8> = (1000..1500).map(|i| (i % 256) as u8).collect();

    // 1. Recebe seg1 (10_000..10_500)
    let (ack1, _) = sock.process_incoming_tcp_payload(10_000, &seg1);
    assert_eq!(ack1, 10_500);

    // 2. Recebe seg3 FORA DE ORDEM (11_000..11_500) — gap de 500 bytes
    let (ack2, _) = sock.process_incoming_tcp_payload(11_000, &seg3_ooo);
    assert_eq!(ack2, 10_500, "ACK repetido deve continuar apontando para o gap (10_500)!");
    assert_eq!(sock.ooo_len, 500);
    assert_eq!(sock.ooo_seq, 11_000);
    assert_eq!(sock.len, 500, "Buffer principal não deve conter dados do gap ainda!");

    // 3. Recebe o segmento faltante seg2 (10_500..11_000)
    let (ack3, _) = sock.process_incoming_tcp_payload(10_500, &seg2_gap);
    assert_eq!(ack3, 11_500, "ACK final deve conectar seg2 E seg3 automaticamente!");
    assert_eq!(sock.ooo_len, 0, "Buffer OOO deve estar limpo após o merge!");
    assert_eq!(sock.len, 1500, "Stream agora possui todos os 1500 bytes contíguos!");

    // Validação dos dados
    let mut stream_data = vec![0u8; 1500];
    assert_eq!(sock.pop_rx(&mut stream_data), Some(1500));
    let expected: Vec<u8> = (0..1500).map(|i| (i % 256) as u8).collect();
    assert_eq!(stream_data, expected);
}

#[test]
fn test_http_chunked_transfer_decoding() {
    let chunked_raw = b"4\r\nWiki\r\n5\r\npedia\r\nF\r\n in \r\n\r\nchunks.\r\n0\r\n\r\n";
    let mut decoded = [0u8; 128];

    // Decodificador simulando browser::decode_chunked_body
    let n = decode_chunked_test(chunked_raw, &mut decoded).expect("Decodificação de chunked válida");
    assert_eq!(n, 24);
    assert_eq!(&decoded[..n], b"Wikipedia in \r\n\r\nchunks.");
}

#[test]
fn test_http_chunked_with_extensions_and_trailers() {
    let chunked_raw = b"A;ext=test\r\n1234567890\r\n0;last\r\n\r\n";
    let mut decoded = [0u8; 128];
    let n = decode_chunked_test(chunked_raw, &mut decoded).expect("Chunked com extensões");
    assert_eq!(n, 10);
    assert_eq!(&decoded[..n], b"1234567890");
}

#[test]
fn test_http_chunked_terminator_detection() {
    assert!(has_chunked_terminator_test(b"HTTP/1.1 200 OK\r\n\r\n4\r\ntest\r\n0\r\n\r\n"));
    assert!(has_chunked_terminator_test(b"4\r\ntest\r\n\r\n0\r\n\r\n"));
    assert!(has_chunked_terminator_test(b"0\r\n\r\n"));

    // Casos negativos
    assert!(!has_chunked_terminator_test(b"HTTP/1.1 200 OK\r\n\r\n4\r\ntest\r\n"));
    assert!(!has_chunked_terminator_test(b"Contem o numero 0 no meio do texto sem terminador"));
}

#[test]
fn test_http_multi_fragment_content_length_simulation() {
    let mut sock = TestSocketStream::new(1);

    let html_body = b"<!DOCTYPE html><html><head><title>Jinn Test</title></head><body><h1>Conectividade Perfeita</h1><p>Fluxo TCP continuo sem perda de dados.</p></body></html>";
    let header = format!(
        "HTTP/1.1 200 OK\r\nServer: Jinn-Kernel/0.3.0\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        html_body.len()
    );

    let full_response = [header.as_bytes(), html_body].concat();

    // Divide a resposta HTTP em 4 fragmentos pequenos simulando pacotes TCP que chegam aos poucos
    let frag1 = &full_response[..50];
    let frag2 = &full_response[50..120];
    let frag3 = &full_response[120..180];
    let frag4 = &full_response[180..];

    sock.push_rx(frag1);
    sock.push_rx(frag2);
    sock.push_rx(frag3);
    sock.push_rx(frag4);

    assert_eq!(sock.len, full_response.len());

    let mut user_rx = vec![0u8; full_response.len() + 100];
    let read_len = sock.pop_rx(&mut user_rx).expect("Leitura completa do buffer");
    assert_eq!(read_len, full_response.len());
    assert_eq!(&user_rx[..read_len], &full_response[..]);
}

#[test]
fn test_loopback_two_way_stream_simulation() {
    // Simula cliente e servidor trocando dados via buffers de loopback
    // client_sock espera sequência remota do servidor (iniciada em 500)
    let mut client_sock = TestSocketStream::new(500);
    // server_sock espera sequência remota do cliente (iniciada em 100)
    let mut server_sock = TestSocketStream::new(100);

    // Cliente envia "GET /metrics HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"
    let req = b"GET /metrics HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
    let (ack_req, _) = server_sock.process_incoming_tcp_payload(100, req);
    assert_eq!(ack_req, 100 + req.len() as u32);

    let mut srv_read = [0u8; 128];
    let n = server_sock.pop_rx(&mut srv_read).expect("Servidor le requisicao");
    assert_eq!(&srv_read[..n], req);

    // Servidor responde HTTP 200 com métricas
    let resp = b"HTTP/1.1 200 OK\r\nContent-Length: 15\r\n\r\njinn_status 1\n";
    let (ack_resp, _) = client_sock.process_incoming_tcp_payload(500, resp);
    assert_eq!(ack_resp, 500 + resp.len() as u32);

    let mut cli_read = [0u8; 128];
    let n_cli = client_sock.pop_rx(&mut cli_read).expect("Cliente le resposta");
    assert_eq!(&cli_read[..n_cli], resp);
}

#[test]
fn test_large_16k_html_stream_transfer_no_loss() {
    let mut sock = TestSocketStream::new(0);

    // Gera um documento HTML volumoso de 16 KiB
    let mut big_html = String::from("<!DOCTYPE html><html><body><h1>Auditoria de Estresse TCP 16 KiB</h1><ul>");
    while big_html.len() < 16000 {
        big_html.push_str("<li>Item de telemetria de rede Jinn OS com dados continuos e seguros.</li>\n");
    }
    big_html.push_str("</ul></body></html>");
    let raw_bytes = big_html.as_bytes();

    // Transmite em blocos de 1440 bytes (MSS comum)
    let mut offset = 0;
    while offset < raw_bytes.len() {
        let chunk_len = (raw_bytes.len() - offset).min(1440);
        let chunk = &raw_bytes[offset..offset + chunk_len];
        let (ack, _) = sock.process_incoming_tcp_payload(offset as u32, chunk);
        assert_eq!(ack, (offset + chunk_len) as u32);
        offset += chunk_len;
    }

    assert_eq!(sock.len, raw_bytes.len());

    // Consome tudo e valida integridade absoluta
    let mut received = vec![0u8; raw_bytes.len()];
    let mut rec_offset = 0;
    while rec_offset < raw_bytes.len() {
        let mut temp = [0u8; 2048];
        let n = sock.pop_rx(&mut temp).expect("Leitura de stream 16k");
        received[rec_offset..rec_offset + n].copy_from_slice(&temp[..n]);
        rec_offset += n;
    }

    assert_eq!(received.len(), raw_bytes.len());
    assert_eq!(&received[..], raw_bytes, "O documento HTML de 16 KiB deve ser reconstituído com 100% de exatidão!");
}

// ── Helpers de Teste ──────────────────────────────────────────────────────────

fn decode_chunked_test(raw: &[u8], out: &mut [u8]) -> Result<usize, ()> {
    let mut src_idx = 0usize;
    let mut dst_idx = 0usize;

    while src_idx < raw.len() {
        let line_end = match raw[src_idx..].iter().position(|&b| b == b'\n') {
            Some(p) => src_idx + p,
            None => break,
        };
        let mut hex_slice = &raw[src_idx..line_end];
        if !hex_slice.is_empty() && hex_slice[hex_slice.len() - 1] == b'\r' {
            hex_slice = &hex_slice[..hex_slice.len() - 1];
        }
        if let Some(semi) = hex_slice.iter().position(|&b| b == b';') {
            hex_slice = &hex_slice[..semi];
        }
        let hex_slice = trim_ascii_test(hex_slice);
        if hex_slice.is_empty() {
            src_idx = line_end + 1;
            continue;
        }

        let mut chunk_size: usize = 0;
        for &b in hex_slice {
            let val = match b {
                b'0'..=b'9' => (b - b'0') as usize,
                b'a'..=b'f' => (b - b'a' + 10) as usize,
                b'A'..=b'F' => (b - b'A' + 10) as usize,
                _ => return Err(()),
            };
            chunk_size = chunk_size.checked_mul(16).ok_or(())?.checked_add(val).ok_or(())?;
        }

        src_idx = line_end + 1;
        if chunk_size == 0 {
            break;
        }

        if src_idx + chunk_size > raw.len() {
            let copy_len = (raw.len() - src_idx).min(out.len() - dst_idx);
            out[dst_idx..dst_idx + copy_len].copy_from_slice(&raw[src_idx..src_idx + copy_len]);
            dst_idx += copy_len;
            break;
        }

        let copy_len = chunk_size.min(out.len() - dst_idx);
        out[dst_idx..dst_idx + copy_len].copy_from_slice(&raw[src_idx..src_idx + copy_len]);
        dst_idx += copy_len;
        src_idx += chunk_size;

        if src_idx < raw.len() && raw[src_idx] == b'\r' {
            src_idx += 1;
        }
        if src_idx < raw.len() && raw[src_idx] == b'\n' {
            src_idx += 1;
        }
    }

    Ok(dst_idx)
}

fn trim_ascii_test(mut slice: &[u8]) -> &[u8] {
    while !slice.is_empty() && (slice[0] == b' ' || slice[0] == b'\t' || slice[0] == b'\r' || slice[0] == b'\n') {
        slice = &slice[1..];
    }
    while !slice.is_empty() && (slice[slice.len() - 1] == b' ' || slice[slice.len() - 1] == b'\t' || slice[slice.len() - 1] == b'\r' || slice[slice.len() - 1] == b'\n') {
        slice = &slice[..slice.len() - 1];
    }
    slice
}

fn is_chunked_complete_test(mut raw: &[u8]) -> bool {
    if raw.starts_with(b"HTTP/1.") {
        if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            raw = &raw[pos + 4..];
        } else if let Some(pos) = raw.windows(2).position(|w| w == b"\n\n") {
            raw = &raw[pos + 2..];
        } else {
            return false;
        }
    }

    let mut src_idx = 0usize;
    while src_idx < raw.len() {
        let line_end = match raw[src_idx..].iter().position(|&b| b == b'\n') {
            Some(p) => src_idx + p,
            None => return false,
        };
        let mut hex_slice = &raw[src_idx..line_end];
        if !hex_slice.is_empty() && hex_slice[hex_slice.len() - 1] == b'\r' {
            hex_slice = &hex_slice[..hex_slice.len() - 1];
        }
        if let Some(semi) = hex_slice.iter().position(|&b| b == b';') {
            hex_slice = &hex_slice[..semi];
        }
        let hex_slice = trim_ascii_test(hex_slice);
        if hex_slice.is_empty() {
            src_idx = line_end + 1;
            continue;
        }

        let mut chunk_size: usize = 0;
        for &b in hex_slice {
            let val = match b {
                b'0'..=b'9' => (b - b'0') as usize,
                b'a'..=b'f' => (b - b'a' + 10) as usize,
                b'A'..=b'F' => (b - b'A' + 10) as usize,
                _ => return false,
            };
            chunk_size = match chunk_size.checked_mul(16).and_then(|c| c.checked_add(val)) {
                Some(c) => c,
                None => return false,
            };
        }

        src_idx = line_end + 1;
        if chunk_size == 0 {
            return true;
        }

        if src_idx + chunk_size > raw.len() {
            return false;
        }
        src_idx += chunk_size;

        if src_idx < raw.len() && raw[src_idx] == b'\r' {
            src_idx += 1;
        }
        if src_idx < raw.len() && raw[src_idx] == b'\n' {
            src_idx += 1;
        }
    }
    false
}

fn has_chunked_terminator_test(body: &[u8]) -> bool {
    is_chunked_complete_test(body)
}

#[test]
fn test_http_chunked_with_embedded_zero_substring_no_false_termination() {
    // Cenário crítico: o corpo do chunk contém a sequência exata "0\r\n\r\n".
    // A implementação ingênua anterior disparava término prematuro.
    let chunk1 = b"5\r\n0\r\n\r\n\r\n";
    let chunk2 = b"5\r\nhello\r\n";
    let chunk_term = b"0\r\n\r\n";

    let mut stream = Vec::new();
    stream.extend_from_slice(chunk1);
    assert!(!has_chunked_terminator_test(&stream), "Chunk 1 com '0\\r\\n\\r\\n' interno não deve terminar o fluxo!");

    stream.extend_from_slice(chunk2);
    assert!(!has_chunked_terminator_test(&stream), "Ainda não terminou (faltando o chunk 0)");

    stream.extend_from_slice(chunk_term);
    assert!(has_chunked_terminator_test(&stream), "Agora o terminador real foi detectado!");

    let mut decoded = [0u8; 64];
    let n = decode_chunked_test(&stream, &mut decoded).expect("Decodificacao chunked perfeita");
    assert_eq!(n, 10);
    assert_eq!(&decoded[..n], b"0\r\n\r\nhello");
}

#[test]
fn test_tcp_ooo_realign_on_overlapping_retransmit() {
    let mut sock = TestSocketStream::new(1000);

    // 1. Recebe segmento fora de ordem: seq 2000..2500 (gap de 1000..2000)
    let ooo_payload = [0xBB; 500];
    let (ack, _) = sock.process_incoming_tcp_payload(2000, &ooo_payload);
    assert_eq!(ack, 1000, "ACK deve permanecer 1000 pois há lacuna");
    assert_eq!(sock.ooo_len, 500);
    assert_eq!(sock.ooo_seq, 2000);

    // 2. Recebe segmento contíguo que avança e cobre parcialmente o OOO: seq 1000..2200
    let overlap_payload = [0xAA; 1200];
    let (ack, _) = sock.process_incoming_tcp_payload(1000, &overlap_payload);
    // Deve consumir 1000..2200 contíguo + drenar os 300 restantes do OOO (2200..2500)!
    assert_eq!(ack, 2500, "ACK deve avançar até 2500 após realignment do OOO");
    assert_eq!(sock.ooo_len, 0, "Buffer OOO deve estar limpo e desbloqueado");

    // 3. Testa que um novo segmento OOO pode ser enfileirado sem bloqueio
    let (ack2, _) = sock.process_incoming_tcp_payload(3000, &[0xCC; 200]);
    assert_eq!(ack2, 2500);
    assert_eq!(sock.ooo_len, 200);
    assert_eq!(sock.ooo_seq, 3000);
}

#[test]
fn test_tcp_large_32k_and_64k_stream_flow_control() {
    let mut sock = TestSocketStream::new(0);

    let mut send_payload = vec![0u8; 65536];
    for (i, b) in send_payload.iter_mut().enumerate() {
        *b = (i % 251) as u8;
    }

    let mut sent = 0;
    let mut rec = Vec::new();
    let mut read_buf = [0u8; 4096];

    while sent < send_payload.len() {
        let chunk_size = 4096.min(send_payload.len() - sent);
        let (ack, _win) = sock.process_incoming_tcp_payload(sent as u32, &send_payload[sent..sent + chunk_size]);
        sent += chunk_size;
        assert_eq!(ack, sent as u32);

        // Drena do stream para simular leitura pelo usuário
        if let Some(n) = sock.pop_rx(&mut read_buf) {
            rec.extend_from_slice(&read_buf[..n]);
        }
    }

    // Drena o restante
    while let Some(n) = sock.pop_rx(&mut read_buf) {
        if n == 0 { break; }
        rec.extend_from_slice(&read_buf[..n]);
    }

    assert_eq!(rec.len(), 65536);
    assert_eq!(rec, send_payload, "64 KiB de stream transmitidos e reconstituídos com integridade 100%");
}

#[test]
fn test_http_resilient_body_delivery_on_slow_server() {
    let http_response = b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<!DOCTYPE html><html><body><h1>Servidor Lento</h1></body></html>";

    let mut sock = TestSocketStream::new(0);
    sock.process_incoming_tcp_payload(0, http_response);

    let mut user_buf = [0u8; 1024];
    let n = sock.pop_rx(&mut user_buf).expect("Leitura da resposta");
    assert_eq!(n, http_response.len());
    assert_eq!(&user_buf[..n], http_response);
}
