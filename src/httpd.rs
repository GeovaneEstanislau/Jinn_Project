//! Micro HTTP/1.1 Server & Metrics Daemon (jinn-httpd)
//! Provides healthcheck (/healthz), metrics (/metrics), and system info endpoints for Simbad.
//! Supports real TCP listening, connection acceptance, request bounds, malformed HTTP defense,
//! and Prometheus exposition metrics.

pub const MAX_REQUEST_SIZE: usize = 1024;
pub const MAX_RESPONSE_SIZE: usize = 1536;

pub struct HttpDaemon {
    pub port: u16,
    pub total_requests: u64,
    pub total_responses_200: u64,
    pub total_responses_400: u64,
    pub total_responses_404: u64,
    pub total_responses_405: u64,
    pub total_responses_413: u64,
    pub total_bytes_sent: u64,
    pub total_bytes_recv: u64,
    pub is_running: bool,
    pub listen_sock: Option<usize>,
}

impl HttpDaemon {
    pub const fn new() -> Self {
        Self {
            port: 80,
            total_requests: 0,
            total_responses_200: 0,
            total_responses_400: 0,
            total_responses_404: 0,
            total_responses_405: 0,
            total_responses_413: 0,
            total_bytes_sent: 0,
            total_bytes_recv: 0,
            is_running: false,
            listen_sock: None,
        }
    }

    pub fn start(&mut self) {
        self.is_running = true;
    }

    pub fn stop(&mut self) {
        self.is_running = false;
        if let Some(sock) = self.listen_sock.take() {
            let pid = crate::scheduler::current_pid();
            let _ = crate::net::socket::close_socket(pid, sock);
        }
    }

    /// Processa uma requisição HTTP e preenche o buffer de resposta
    pub fn handle_request(&mut self, request: &[u8], response_buf: &mut [u8]) -> usize {
        self.total_requests = self.total_requests.saturating_add(1);
        self.total_bytes_recv = self.total_bytes_recv.saturating_add(request.len() as u64);

        // 1. Limite de tamanho de requisição (anti-exaustão de buffer)
        if request.len() > MAX_REQUEST_SIZE {
            return self.build_413(response_buf);
        }

        // 2. Parse UTF-8 e validação básica
        let req_str = match core::str::from_utf8(request) {
            Ok(s) => s,
            Err(_) => return self.build_400(response_buf),
        };

        let mut lines = req_str.split("\r\n");
        let request_line = match lines.next() {
            Some(l) if !l.is_empty() => l,
            _ => return self.build_400(response_buf),
        };

        let mut parts = request_line.split_whitespace();
        let method = match parts.next() {
            Some(m) => m,
            None => return self.build_400(response_buf),
        };
        let path = match parts.next() {
            Some(p) => p,
            None => return self.build_400(response_buf),
        };
        let version = match parts.next() {
            Some(v) => v,
            None => return self.build_400(response_buf),
        };

        // Valida protocolo HTTP
        if !version.starts_with("HTTP/") {
            return self.build_400(response_buf);
        }

        // 3. Validação de método (GET e HEAD permitidos; outros rejeitados com 405)
        if method != "GET" && method != "HEAD" {
            return self.build_405(response_buf);
        }

        // 4. Roteamento de endpoints canônicos Simbad
        let resp_len = match path {
            "/" => self.build_root(response_buf),
            "/healthz" => self.build_healthz(response_buf),
            "/metrics" => self.build_metrics(response_buf),
            _ => self.build_404(response_buf),
        };

        self.total_bytes_sent = self.total_bytes_sent.saturating_add(resp_len as u64);
        resp_len
    }

    /// Servidor de polling não-bloqueante integrado à pilha de sockets TCP do Jinn
    pub fn poll_server(&mut self) -> bool {
        if !self.is_running {
            return false;
        }

        let pid = crate::scheduler::current_pid();

        // Inicializa o socket de escuta se ainda não criado
        if self.listen_sock.is_none() {
            if let Ok(sock) = crate::net::socket::create_socket(pid, crate::net::socket::AF_INET, crate::net::socket::SOCK_STREAM, 0) {
                if crate::net::socket::bind_socket(pid, sock, self.port).is_ok() {
                    if crate::net::socket::listen_socket(pid, sock, 4).is_ok() {
                        self.listen_sock = Some(sock);
                    } else {
                        let _ = crate::net::socket::close_socket(pid, sock);
                        return false;
                    }
                } else {
                    let _ = crate::net::socket::close_socket(pid, sock);
                    return false;
                }
            } else {
                return false;
            }
        }

        let l_sock = match self.listen_sock {
            Some(s) => s,
            None => return false,
        };

        // Tenta aceitar uma nova conexão estabelecida (não-bloqueante)
        match crate::net::socket::accept_socket(pid, l_sock) {
            Ok(client_fd) => {
                let mut req_buf = [0u8; MAX_REQUEST_SIZE];
                let mut resp_buf = [0u8; MAX_RESPONSE_SIZE];

                match crate::net::socket::recv_socket(pid, client_fd, &mut req_buf) {
                    Ok(n) if n > 0 => {
                        let resp_len = self.handle_request(&req_buf[..n], &mut resp_buf);
                        if resp_len > 0 {
                            let _ = crate::net::socket::send_socket(pid, client_fd, &resp_buf[..resp_len]);
                        }
                    }
                    _ => {}
                }

                // Encerra a conexão HTTP/1.1 (Connection: close)
                let _ = crate::net::socket::close_socket(pid, client_fd);
                true
            }
            Err(_) => false,
        }
    }

    fn build_root(&mut self, buf: &mut [u8]) -> usize {
        self.total_responses_200 = self.total_responses_200.saturating_add(1);
        let body = b"{\"system\":\"Jinn Microkernel\",\"manifestation\":\"Simbad Server 0.2.0\",\"status\":\"online\",\"version\":\"0.3.0\"}\n";
        let header = b"HTTP/1.1 200 OK\r\nServer: Simbad-Httpd/0.2.0\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: 104\r\n\r\n";
        
        let total_len = header.len() + body.len();
        if buf.len() < total_len { return 0; }
        buf[..header.len()].copy_from_slice(header);
        buf[header.len()..total_len].copy_from_slice(body);
        total_len
    }

    fn build_healthz(&mut self, buf: &mut [u8]) -> usize {
        self.total_responses_200 = self.total_responses_200.saturating_add(1);
        let body = b"healthy\n";
        let header = b"HTTP/1.1 200 OK\r\nServer: Simbad-Httpd/0.2.0\r\nContent-Type: text/plain\r\nConnection: close\r\nContent-Length: 8\r\n\r\n";
        
        let total_len = header.len() + body.len();
        if buf.len() < total_len { return 0; }
        buf[..header.len()].copy_from_slice(header);
        buf[header.len()..total_len].copy_from_slice(body);
        total_len
    }

    fn build_metrics(&mut self, buf: &mut [u8]) -> usize {
        self.total_responses_200 = self.total_responses_200.saturating_add(1);
        let supervisor = crate::supervisor::get_supervisor();
        let running_svc = supervisor.running_count();
        let failed_svc = supervisor.failed_count();
        let uptime_s = (crate::timer::ticks() * 10) / 1000;
        let flushes = crate::flusher::get_flusher().total_flushes;
        let ctx_switches = crate::scheduler::context_switches();

        let mut offset = 0;
        let prefix = b"HTTP/1.1 200 OK\r\nServer: Simbad-Httpd/0.2.0\r\nContent-Type: text/plain; version=0.0.4\r\nConnection: close\r\n\r\n";
        if buf.len() < prefix.len() + 512 { return 0; }
        buf[..prefix.len()].copy_from_slice(prefix);
        offset += prefix.len();

        let s_upt = b"# HELP jinn_uptime_seconds Total system uptime in seconds\n# TYPE jinn_uptime_seconds counter\njinn_uptime_seconds ";
        buf[offset..offset + s_upt.len()].copy_from_slice(s_upt);
        offset += s_upt.len();
        offset += format_num(uptime_s as usize, &mut buf[offset..]);
        buf[offset] = b'\n';
        offset += 1;

        let s_run = b"# HELP jinn_services_running Total running services in supervisor\n# TYPE jinn_services_running gauge\njinn_services_running ";
        buf[offset..offset + s_run.len()].copy_from_slice(s_run);
        offset += s_run.len();
        offset += format_num(running_svc, &mut buf[offset..]);
        buf[offset] = b'\n';
        offset += 1;

        let s_fail = b"# HELP jinn_services_failed Total failed services in supervisor\n# TYPE jinn_services_failed counter\njinn_services_failed ";
        buf[offset..offset + s_fail.len()].copy_from_slice(s_fail);
        offset += s_fail.len();
        offset += format_num(failed_svc, &mut buf[offset..]);
        buf[offset] = b'\n';
        offset += 1;

        let s_req = b"# HELP jinn_http_requests_total Total HTTP requests handled\n# TYPE jinn_http_requests_total counter\njinn_http_requests_total ";
        buf[offset..offset + s_req.len()].copy_from_slice(s_req);
        offset += s_req.len();
        offset += format_num(self.total_requests as usize, &mut buf[offset..]);
        buf[offset] = b'\n';
        offset += 1;

        let s_flush = b"# HELP jinn_storage_flushes_total Total periodic cache flushes executed\n# TYPE jinn_storage_flushes_total counter\njinn_storage_flushes_total ";
        buf[offset..offset + s_flush.len()].copy_from_slice(s_flush);
        offset += s_flush.len();
        offset += format_num(flushes as usize, &mut buf[offset..]);
        buf[offset] = b'\n';
        offset += 1;

        let s_ctx = b"# HELP jinn_cpu_context_switches_total Total scheduler context switches\n# TYPE jinn_cpu_context_switches_total counter\njinn_cpu_context_switches_total ";
        buf[offset..offset + s_ctx.len()].copy_from_slice(s_ctx);
        offset += s_ctx.len();
        offset += format_num(ctx_switches as usize, &mut buf[offset..]);
        buf[offset] = b'\n';
        offset += 1;

        offset
    }

    fn build_404(&mut self, buf: &mut [u8]) -> usize {
        self.total_responses_404 = self.total_responses_404.saturating_add(1);
        let resp = b"HTTP/1.1 404 Not Found\r\nServer: Simbad-Httpd/0.2.0\r\nContent-Type: text/plain\r\nConnection: close\r\nContent-Length: 9\r\n\r\nNot Found";
        if buf.len() < resp.len() { return 0; }
        buf[..resp.len()].copy_from_slice(resp);
        resp.len()
    }

    fn build_400(&mut self, buf: &mut [u8]) -> usize {
        self.total_responses_400 = self.total_responses_400.saturating_add(1);
        let resp = b"HTTP/1.1 400 Bad Request\r\nServer: Simbad-Httpd/0.2.0\r\nContent-Type: text/plain\r\nConnection: close\r\nContent-Length: 11\r\n\r\nBad Request";
        if buf.len() < resp.len() { return 0; }
        buf[..resp.len()].copy_from_slice(resp);
        resp.len()
    }

    fn build_405(&mut self, buf: &mut [u8]) -> usize {
        self.total_responses_405 = self.total_responses_405.saturating_add(1);
        let resp = b"HTTP/1.1 405 Method Not Allowed\r\nServer: Simbad-Httpd/0.2.0\r\nAllow: GET, HEAD\r\nContent-Type: text/plain\r\nConnection: close\r\nContent-Length: 18\r\n\r\nMethod Not Allowed";
        if buf.len() < resp.len() { return 0; }
        buf[..resp.len()].copy_from_slice(resp);
        resp.len()
    }

    fn build_413(&mut self, buf: &mut [u8]) -> usize {
        self.total_responses_413 = self.total_responses_413.saturating_add(1);
        let resp = b"HTTP/1.1 413 Payload Too Large\r\nServer: Simbad-Httpd/0.2.0\r\nContent-Type: text/plain\r\nConnection: close\r\nContent-Length: 17\r\n\r\nPayload Too Large";
        if buf.len() < resp.len() { return 0; }
        buf[..resp.len()].copy_from_slice(resp);
        resp.len()
    }
}

fn format_num(mut n: usize, out: &mut [u8]) -> usize {
    if n == 0 {
        if !out.is_empty() {
            out[0] = b'0';
            return 1;
        }
        return 0;
    }
    let mut temp = [0u8; 20];
    let mut i = 0;
    while n > 0 && i < temp.len() {
        temp[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    if out.len() < i { return 0; }
    for j in 0..i {
        out[j] = temp[i - 1 - j];
    }
    i
}

pub static mut HTTPD: HttpDaemon = HttpDaemon::new();

pub fn get_httpd() -> &'static mut HttpDaemon {
    unsafe { &mut *core::ptr::addr_of_mut!(HTTPD) }
}
