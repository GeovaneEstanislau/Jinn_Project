//! Micro HTTP/1.1 Server & Metrics Daemon (jinn-httpd)
//! Provides healthcheck (/healthz), metrics (/metrics), and system info endpoints for Simbad.

pub struct HttpDaemon {
    pub port: u16,
    pub total_requests: u64,
    pub total_responses_200: u64,
    pub total_responses_404: u64,
    pub is_running: bool,
}

impl HttpDaemon {
    pub const fn new() -> Self {
        Self {
            port: 80,
            total_requests: 0,
            total_responses_200: 0,
            total_responses_404: 0,
            is_running: false,
        }
    }

    pub fn start(&mut self) {
        self.is_running = true;
    }

    pub fn stop(&mut self) {
        self.is_running = false;
    }

    /// Processa uma requisição HTTP e preenche o buffer de resposta
    pub fn handle_request(&mut self, request: &[u8], response_buf: &mut [u8]) -> usize {
        self.total_requests = self.total_requests.saturating_add(1);

        // Parse simples do método e caminho
        let req_str = match core::str::from_utf8(request) {
            Ok(s) => s,
            Err(_) => return self.build_400(response_buf),
        };

        let mut lines = req_str.split("\r\n");
        let request_line = match lines.next() {
            Some(l) => l,
            None => return self.build_400(response_buf),
        };

        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("");
        let path = parts.next().unwrap_or("/");

        if method != "GET" && method != "HEAD" {
            return self.build_405(response_buf);
        }

        match path {
            "/" => self.build_root(response_buf),
            "/healthz" => self.build_healthz(response_buf),
            "/metrics" => self.build_metrics(response_buf),
            _ => self.build_404(response_buf),
        }
    }

    fn build_root(&mut self, buf: &mut [u8]) -> usize {
        self.total_responses_200 = self.total_responses_200.saturating_add(1);
        let body = b"{\"system\":\"Jinn Microkernel\",\"manifestation\":\"Simbad Server 0.1.0\",\"status\":\"online\"}\n";
        let header = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: 87\r\n\r\n";
        
        let total_len = header.len() + body.len();
        if buf.len() < total_len { return 0; }
        buf[..header.len()].copy_from_slice(header);
        buf[header.len()..total_len].copy_from_slice(body);
        total_len
    }

    fn build_healthz(&mut self, buf: &mut [u8]) -> usize {
        self.total_responses_200 = self.total_responses_200.saturating_add(1);
        let body = b"healthy\n";
        let header = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nConnection: close\r\nContent-Length: 8\r\n\r\n";
        
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

        // Montagem de métricas estilo Prometheus
        let mut offset = 0;
        let prefix = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n";
        if buf.len() < prefix.len() + 256 { return 0; }
        buf[..prefix.len()].copy_from_slice(prefix);
        offset += prefix.len();

        let s_upt = b"# HELP jinn_uptime_seconds Total system uptime in seconds\njinn_uptime_seconds ";
        buf[offset..offset + s_upt.len()].copy_from_slice(s_upt);
        offset += s_upt.len();
        offset += format_num(uptime_s as usize, &mut buf[offset..]);
        buf[offset] = b'\n';
        offset += 1;

        let s_run = b"# HELP jinn_services_running Total running services in supervisor\njinn_services_running ";
        buf[offset..offset + s_run.len()].copy_from_slice(s_run);
        offset += s_run.len();
        offset += format_num(running_svc, &mut buf[offset..]);
        buf[offset] = b'\n';
        offset += 1;

        let s_fail = b"# HELP jinn_services_failed Total failed services in supervisor\njinn_services_failed ";
        buf[offset..offset + s_fail.len()].copy_from_slice(s_fail);
        offset += s_fail.len();
        offset += format_num(failed_svc, &mut buf[offset..]);
        buf[offset] = b'\n';
        offset += 1;

        offset
    }

    fn build_404(&mut self, buf: &mut [u8]) -> usize {
        self.total_responses_404 = self.total_responses_404.saturating_add(1);
        let resp = b"HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nConnection: close\r\nContent-Length: 9\r\n\r\nNot Found";
        if buf.len() < resp.len() { return 0; }
        buf[..resp.len()].copy_from_slice(resp);
        resp.len()
    }

    fn build_400(&mut self, buf: &mut [u8]) -> usize {
        let resp = b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";
        if buf.len() < resp.len() { return 0; }
        buf[..resp.len()].copy_from_slice(resp);
        resp.len()
    }

    fn build_405(&mut self, buf: &mut [u8]) -> usize {
        let resp = b"HTTP/1.1 405 Method Not Allowed\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";
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
    for j in 0..i {
        if j < out.len() {
            out[j] = temp[i - 1 - j];
        }
    }
    i
}

pub static mut HTTPD: HttpDaemon = HttpDaemon::new();

pub fn get_httpd() -> &'static mut HttpDaemon {
    unsafe { &mut *core::ptr::addr_of_mut!(HTTPD) }
}
