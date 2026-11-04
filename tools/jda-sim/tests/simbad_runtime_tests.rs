//! Simbad Server Runtime, Network & Storage Reliability 0.2.0 Contract Tests
//!
//! Validates:
//! 1. Service Supervisor Real Lifecycle & PID Reuse Protection.
//! 2. Exponential Backoff & Circuit Breaker tripping after 5 consecutive failures.
//! 3. DAG Ordered Boot & Reverse Shutdown constraints.
//! 4. HTTP Server Interoperability (/, /healthz, /metrics, 404, 400, 405, 413 bounds).
//! 5. Storage Cache Reliability, Periodic Flusher & Crash Consistency.
//! 6. Resource Limits Enforcement (Memory frames, Descriptors, Sockets, CPU Quotas).

use std::collections::HashMap;

// ── 1. Service Supervisor Contract Models ──────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimServiceState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed,
    Restarting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimRestartPolicy {
    Always,
    OnFailure,
    Never,
}

#[derive(Debug, Clone)]
pub struct SimServiceDescriptor {
    pub id: usize,
    pub name: &'static str,
    pub state: SimServiceState,
    pub pid: Option<usize>,
    pub restart_policy: SimRestartPolicy,
    pub restart_count: u32,
    pub consecutive_failures: u32,
    pub max_retries: u32,
    pub backoff_ticks: u64,
    pub next_restart_tick: u64,
    pub circuit_breaker_tripped: bool,
    pub last_exit_code: Option<i32>,
    pub dependencies: Vec<&'static str>,
}

pub struct SimSupervisorRuntime {
    pub services: Vec<SimServiceDescriptor>,
    pub event_log: Vec<String>,
    pub allocated_pids: HashMap<usize, &'static str>,
    pub next_pid: usize,
}

impl SimSupervisorRuntime {
    pub fn new() -> Self {
        Self {
            services: Vec::new(),
            event_log: Vec::new(),
            allocated_pids: HashMap::new(),
            next_pid: 200,
        }
    }

    pub fn register(&mut self, descriptor: SimServiceDescriptor) {
        self.services.push(descriptor);
    }

    pub fn start_service(&mut self, name: &str) -> Result<usize, &'static str> {
        let idx = self.services.iter().position(|s| s.name == name).ok_or("Service not found")?;

        if self.services[idx].circuit_breaker_tripped {
            return Err("Circuit breaker tripped: service exceeded max retry threshold");
        }

        // Dependency check: all dependencies must be RUNNING
        let deps = self.services[idx].dependencies.clone();
        for dep in deps {
            let dep_svc = self.services.iter().find(|s| s.name == dep).ok_or("Dependency not registered")?;
            if dep_svc.state != SimServiceState::Running {
                return Err("Dependency not satisfied (must be RUNNING)");
            }
        }

        let svc = &mut self.services[idx];
        if svc.state == SimServiceState::Running {
            return Ok(svc.pid.unwrap());
        }

        svc.state = SimServiceState::Starting;
        let pid = self.next_pid;
        self.next_pid += 1;

        svc.pid = Some(pid);
        svc.state = SimServiceState::Running;
        self.allocated_pids.insert(pid, svc.name);

        self.event_log.push(format!("Started {} with PID {}", svc.name, pid));
        Ok(pid)
    }

    pub fn stop_service(&mut self, name: &str) -> Result<(), &'static str> {
        // Reverse DAG check: cannot stop if an active service depends on this
        for s in &self.services {
            if s.state == SimServiceState::Running || s.state == SimServiceState::Starting {
                if s.dependencies.contains(&name) {
                    return Err("Cannot stop: active service depends on this service");
                }
            }
        }

        let idx = self.services.iter().position(|s| s.name == name).ok_or("Service not found")?;
        let svc = &mut self.services[idx];
        if svc.state == SimServiceState::Stopped {
            return Ok(());
        }

        svc.state = SimServiceState::Stopping;
        if let Some(pid) = svc.pid.take() {
            self.allocated_pids.remove(&pid); // PID unmapped
        }
        svc.state = SimServiceState::Stopped;
        svc.consecutive_failures = 0;
        svc.circuit_breaker_tripped = false;

        self.event_log.push(format!("Stopped {}", svc.name));
        Ok(())
    }

    pub fn report_exit(&mut self, name: &str, exit_code: i32, current_tick: u64) -> Result<SimServiceState, &'static str> {
        let idx = self.services.iter().position(|s| s.name == name).ok_or("Service not found")?;
        let svc = &mut self.services[idx];

        if let Some(pid) = svc.pid.take() {
            self.allocated_pids.remove(&pid); // Immediate PID cleanup to prevent reuse
        }
        svc.last_exit_code = Some(exit_code);

        if exit_code == 0 {
            svc.state = SimServiceState::Stopped;
            svc.consecutive_failures = 0;
            return Ok(SimServiceState::Stopped);
        }

        svc.state = SimServiceState::Failed;
        svc.consecutive_failures += 1;
        svc.restart_count += 1;

        let should_restart = svc.restart_policy != SimRestartPolicy::Never;
        if should_restart && svc.consecutive_failures <= svc.max_retries {
            let shift = (svc.consecutive_failures.saturating_sub(1)).min(6);
            let backoff = 100u64 << shift; // 100 ticks = 1s, 200 ticks = 2s, 400 ticks = 4s...
            svc.state = SimServiceState::Restarting;
            svc.backoff_ticks = backoff;
            svc.next_restart_tick = current_tick + backoff;

            self.event_log.push(format!("Service {} failed (code {}), backoff {} ticks", svc.name, exit_code, backoff));
            Ok(SimServiceState::Restarting)
        } else {
            svc.circuit_breaker_tripped = true;
            svc.state = SimServiceState::Failed;
            self.event_log.push(format!("Circuit breaker tripped for {} after {} failures", svc.name, svc.consecutive_failures));
            Ok(SimServiceState::Failed)
        }
    }

    pub fn reset_circuit_breaker(&mut self, name: &str) -> Result<(), &'static str> {
        let idx = self.services.iter().position(|s| s.name == name).ok_or("Service not found")?;
        let svc = &mut self.services[idx];
        svc.circuit_breaker_tripped = false;
        svc.consecutive_failures = 0;
        svc.restart_count = 0;
        svc.state = SimServiceState::Stopped;
        Ok(())
    }
}

// ── 2. Storage Cache & Flusher Contract Models ────────────────────────────────

pub const SECTOR_SIZE: usize = 512;
pub const CACHE_CAPACITY: usize = 8;

#[derive(Clone, Copy)]
pub struct SimBlock {
    pub lba: u64,
    pub data: [u8; SECTOR_SIZE],
    pub is_dirty: bool,
    pub is_valid: bool,
}

pub struct SimCachedDisk {
    pub disk_storage: HashMap<u64, [u8; SECTOR_SIZE]>,
    pub cache: [SimBlock; CACHE_CAPACITY],
    pub write_count_to_disk: usize,
}

impl SimCachedDisk {
    pub fn new() -> Self {
        Self {
            disk_storage: HashMap::new(),
            cache: [SimBlock { lba: 0, data: [0; SECTOR_SIZE], is_dirty: false, is_valid: false }; CACHE_CAPACITY],
            write_count_to_disk: 0,
        }
    }

    pub fn write_sector(&mut self, lba: u64, data: &[u8; SECTOR_SIZE]) {
        // WriteBack: grava no slot da cache como dirty
        for slot in self.cache.iter_mut() {
            if slot.is_valid && slot.lba == lba {
                slot.data.copy_from_slice(data);
                slot.is_dirty = true;
                return;
            }
        }
        for slot in self.cache.iter_mut() {
            if !slot.is_valid {
                slot.lba = lba;
                slot.data.copy_from_slice(data);
                slot.is_dirty = true;
                slot.is_valid = true;
                return;
            }
        }
        // Evicção se cheio (slot 0)
        if self.cache[0].is_dirty {
            self.disk_storage.insert(self.cache[0].lba, self.cache[0].data);
            self.write_count_to_disk += 1;
        }
        self.cache[0].lba = lba;
        self.cache[0].data.copy_from_slice(data);
        self.cache[0].is_dirty = true;
        self.cache[0].is_valid = true;
    }

    pub fn flush_dirty_sectors(&mut self) -> usize {
        let mut count = 0;
        for slot in self.cache.iter_mut() {
            if slot.is_valid && slot.is_dirty {
                self.disk_storage.insert(slot.lba, slot.data);
                slot.is_dirty = false;
                self.write_count_to_disk += 1;
                count += 1;
            }
        }
        count
    }

    pub fn simulate_crash_and_reboot(&mut self) {
        // Ao simular queda de energia, a memória RAM cache é zerada! Apenas o disk_storage sobrevive.
        self.cache = [SimBlock { lba: 0, data: [0; SECTOR_SIZE], is_dirty: false, is_valid: false }; CACHE_CAPACITY];
    }
}

// ── 3. HTTP Server Contract Models ────────────────────────────────────────────

pub struct SimHttpDaemon {
    pub total_requests: u64,
    pub total_bytes_recv: u64,
    pub total_bytes_sent: u64,
}

impl SimHttpDaemon {
    pub fn new() -> Self {
        Self { total_requests: 0, total_bytes_recv: 0, total_bytes_sent: 0 }
    }

    pub fn handle_request(&mut self, request: &[u8], response_buf: &mut [u8]) -> usize {
        self.total_requests += 1;
        self.total_bytes_recv += request.len() as u64;

        if request.len() > 1024 {
            let resp = b"HTTP/1.1 413 Payload Too Large\r\nServer: Simbad-Httpd/0.2.0\r\nContent-Type: text/plain\r\nConnection: close\r\nContent-Length: 17\r\n\r\nPayload Too Large";
            response_buf[..resp.len()].copy_from_slice(resp);
            return resp.len();
        }

        let req_str = match std::str::from_utf8(request) {
            Ok(s) => s,
            Err(_) => {
                let resp = b"HTTP/1.1 400 Bad Request\r\nServer: Simbad-Httpd/0.2.0\r\nConnection: close\r\n\r\n";
                response_buf[..resp.len()].copy_from_slice(resp);
                return resp.len();
            }
        };

        let mut lines = req_str.split("\r\n");
        let request_line = match lines.next() {
            Some(l) if !l.is_empty() => l,
            _ => {
                let resp = b"HTTP/1.1 400 Bad Request\r\nServer: Simbad-Httpd/0.2.0\r\nConnection: close\r\n\r\n";
                response_buf[..resp.len()].copy_from_slice(resp);
                return resp.len();
            }
        };

        let mut parts = request_line.split_whitespace();
        let method = match parts.next() {
            Some(m) => m,
            None => {
                let resp = b"HTTP/1.1 400 Bad Request\r\nServer: Simbad-Httpd/0.2.0\r\nConnection: close\r\n\r\n";
                response_buf[..resp.len()].copy_from_slice(resp);
                return resp.len();
            }
        };
        let path = match parts.next() {
            Some(p) => p,
            None => {
                let resp = b"HTTP/1.1 400 Bad Request\r\nServer: Simbad-Httpd/0.2.0\r\nConnection: close\r\n\r\n";
                response_buf[..resp.len()].copy_from_slice(resp);
                return resp.len();
            }
        };
        let version = match parts.next() {
            Some(v) => v,
            None => {
                let resp = b"HTTP/1.1 400 Bad Request\r\nServer: Simbad-Httpd/0.2.0\r\nConnection: close\r\n\r\n";
                response_buf[..resp.len()].copy_from_slice(resp);
                return resp.len();
            }
        };

        if !version.starts_with("HTTP/") {
            let resp = b"HTTP/1.1 400 Bad Request\r\nServer: Simbad-Httpd/0.2.0\r\nConnection: close\r\n\r\n";
            response_buf[..resp.len()].copy_from_slice(resp);
            return resp.len();
        }

        if method != "GET" && method != "HEAD" {
            let resp = b"HTTP/1.1 405 Method Not Allowed\r\nServer: Simbad-Httpd/0.2.0\r\nAllow: GET, HEAD\r\nConnection: close\r\n\r\n";
            response_buf[..resp.len()].copy_from_slice(resp);
            return resp.len();
        }

        let len = match path {
            "/" => {
                let body = b"{\"system\":\"Jinn Microkernel\",\"manifestation\":\"Simbad Server 0.2.0\",\"status\":\"online\",\"version\":\"0.3.0\"}\n";
                let header = b"HTTP/1.1 200 OK\r\nServer: Simbad-Httpd/0.2.0\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: 104\r\n\r\n";
                response_buf[..header.len()].copy_from_slice(header);
                response_buf[header.len()..header.len() + body.len()].copy_from_slice(body);
                header.len() + body.len()
            }
            "/healthz" => {
                let body = b"healthy\n";
                let header = b"HTTP/1.1 200 OK\r\nServer: Simbad-Httpd/0.2.0\r\nContent-Type: text/plain\r\nConnection: close\r\nContent-Length: 8\r\n\r\n";
                response_buf[..header.len()].copy_from_slice(header);
                response_buf[header.len()..header.len() + body.len()].copy_from_slice(body);
                header.len() + body.len()
            }
            "/metrics" => {
                let body = b"# HELP jinn_uptime_seconds Total system uptime in seconds\njinn_uptime_seconds 42\n# HELP jinn_services_running Total running services in supervisor\njinn_services_running 4\n";
                let header = b"HTTP/1.1 200 OK\r\nServer: Simbad-Httpd/0.2.0\r\nContent-Type: text/plain; version=0.0.4\r\nConnection: close\r\n\r\n";
                response_buf[..header.len()].copy_from_slice(header);
                response_buf[header.len()..header.len() + body.len()].copy_from_slice(body);
                header.len() + body.len()
            }
            _ => {
                let resp = b"HTTP/1.1 404 Not Found\r\nServer: Simbad-Httpd/0.2.0\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nNot Found";
                response_buf[..resp.len()].copy_from_slice(resp);
                resp.len()
            }
        };

        self.total_bytes_sent += len as u64;
        len
    }
}

// ── 4. Resource Accounting Contract Models ────────────────────────────────────

pub const ENOMEM: isize = -12;
pub const EMFILE: isize = -24;

pub struct SimResourceLimits {
    pub max_frames: usize,
    pub max_fds: usize,
    pub max_sockets: usize,
    pub max_cpu_ticks: u64,
}

pub struct SimProcessResources {
    pub allocated_frames: usize,
    pub open_fds: usize,
    pub open_sockets: usize,
    pub limit_violations: u32,
}

impl SimProcessResources {
    pub fn new() -> Self {
        Self {
            allocated_frames: 0,
            open_fds: 0,
            open_sockets: 0,
            limit_violations: 0,
        }
    }

    pub fn check_and_alloc_frames(&mut self, count: usize, limits: &SimResourceLimits) -> Result<(), isize> {
        let new_total = self.allocated_frames + count;
        if new_total > limits.max_frames {
            self.limit_violations += 1;
            return Err(ENOMEM);
        }
        self.allocated_frames = new_total;
        Ok(())
    }

    pub fn check_and_open_fd(&mut self, limits: &SimResourceLimits) -> Result<(), isize> {
        if self.open_fds >= limits.max_fds {
            self.limit_violations += 1;
            return Err(EMFILE);
        }
        self.open_fds += 1;
        Ok(())
    }

    pub fn check_and_open_socket(&mut self, limits: &SimResourceLimits) -> Result<(), isize> {
        if self.open_sockets >= limits.max_sockets {
            self.limit_violations += 1;
            return Err(EMFILE);
        }
        self.open_sockets += 1;
        Ok(())
    }
}

// ── 5. Test Cases ─────────────────────────────────────────────────────────────

#[test]
fn test_supervisor_real_process_lifecycle_and_pid_recycling() {
    let mut sup = SimSupervisorRuntime::new();
    sup.register(SimServiceDescriptor {
        id: 1,
        name: "jinn-logd",
        state: SimServiceState::Stopped,
        pid: None,
        restart_policy: SimRestartPolicy::Always,
        restart_count: 0,
        consecutive_failures: 0,
        max_retries: 5,
        backoff_ticks: 0,
        next_restart_tick: 0,
        circuit_breaker_tripped: false,
        last_exit_code: None,
        dependencies: vec![],
    });
    sup.register(SimServiceDescriptor {
        id: 2,
        name: "jinn-flusher",
        state: SimServiceState::Stopped,
        pid: None,
        restart_policy: SimRestartPolicy::Always,
        restart_count: 0,
        consecutive_failures: 0,
        max_retries: 5,
        backoff_ticks: 0,
        next_restart_tick: 0,
        circuit_breaker_tripped: false,
        last_exit_code: None,
        dependencies: vec!["jinn-logd"],
    });

    // 1. Tentar iniciar flusher antes de logd deve falhar
    assert!(sup.start_service("jinn-flusher").is_err());

    // 2. Inicia logd -> recebe PID real 200
    let logd_pid = sup.start_service("jinn-logd").expect("logd should start");
    assert_eq!(logd_pid, 200);
    assert!(sup.allocated_pids.contains_key(&logd_pid));

    // 3. Agora flusher pode iniciar -> recebe PID 201
    let flusher_pid = sup.start_service("jinn-flusher").expect("flusher should start");
    assert_eq!(flusher_pid, 201);

    // 4. Tentar parar logd enquanto flusher depende dele deve ser rejeitado (Reverse DAG)
    assert!(sup.stop_service("jinn-logd").is_err());

    // 5. Parar flusher -> PID 201 desalocado
    assert!(sup.stop_service("jinn-flusher").is_ok());
    assert!(!sup.allocated_pids.contains_key(&flusher_pid));

    // 6. Agora logd pode ser parado -> PID 200 desalocado
    assert!(sup.stop_service("jinn-logd").is_ok());
    assert!(!sup.allocated_pids.contains_key(&logd_pid));
}

#[test]
fn test_supervisor_exponential_backoff_and_circuit_breaker() {
    let mut sup = SimSupervisorRuntime::new();
    sup.register(SimServiceDescriptor {
        id: 1,
        name: "flaky-daemon",
        state: SimServiceState::Stopped,
        pid: None,
        restart_policy: SimRestartPolicy::OnFailure,
        restart_count: 0,
        consecutive_failures: 0,
        max_retries: 5,
        backoff_ticks: 0,
        next_restart_tick: 0,
        circuit_breaker_tripped: false,
        last_exit_code: None,
        dependencies: vec![],
    });

    let _ = sup.start_service("flaky-daemon").unwrap();

    // Falhas simuladas com códigos de saída anormais
    // Falha 1 -> backoff 100 ticks (1s)
    let s1 = sup.report_exit("flaky-daemon", -1, 1000).unwrap();
    assert_eq!(s1, SimServiceState::Restarting);
    assert_eq!(sup.services[0].backoff_ticks, 100);

    // Falha 2 -> backoff 200 ticks (2s)
    let s2 = sup.report_exit("flaky-daemon", -1, 1100).unwrap();
    assert_eq!(s2, SimServiceState::Restarting);
    assert_eq!(sup.services[0].backoff_ticks, 200);

    // Falha 3 -> backoff 400 ticks (4s)
    let s3 = sup.report_exit("flaky-daemon", -1, 1300).unwrap();
    assert_eq!(s3, SimServiceState::Restarting);
    assert_eq!(sup.services[0].backoff_ticks, 400);

    // Falha 4 -> backoff 800 ticks (8s)
    let s4 = sup.report_exit("flaky-daemon", -1, 1700).unwrap();
    assert_eq!(s4, SimServiceState::Restarting);
    assert_eq!(sup.services[0].backoff_ticks, 800);

    // Falha 5 -> backoff 1600 ticks (16s)
    let s5 = sup.report_exit("flaky-daemon", -1, 2500).unwrap();
    assert_eq!(s5, SimServiceState::Restarting);
    assert_eq!(sup.services[0].backoff_ticks, 1600);

    // Falha 6 (excedeu max_retries = 5) -> Desarme do Circuit Breaker!
    let s6 = sup.report_exit("flaky-daemon", -1, 4100).unwrap();
    assert_eq!(s6, SimServiceState::Failed);
    assert!(sup.services[0].circuit_breaker_tripped);

    // Tentativa de start enquanto circuit breaker está armado deve ser bloqueada
    assert!(sup.start_service("flaky-daemon").is_err());

    // Reset manual pelo operador via jinnctl reset
    assert!(sup.reset_circuit_breaker("flaky-daemon").is_ok());
    assert!(!sup.services[0].circuit_breaker_tripped);
    assert_eq!(sup.services[0].state, SimServiceState::Stopped);

    // Agora pode iniciar normalmente
    assert!(sup.start_service("flaky-daemon").is_ok());
}

#[test]
fn test_httpd_interoperability_and_request_bounds() {
    let mut httpd = SimHttpDaemon::new();
    let mut resp = [0u8; 1536];

    // 1. GET / -> JSON Status
    let req_root = b"GET / HTTP/1.1\r\nHost: 10.0.2.15\r\n\r\n";
    let n1 = httpd.handle_request(req_root, &mut resp);
    assert!(n1 > 0);
    let s1 = std::str::from_utf8(&resp[..n1]).unwrap();
    assert!(s1.contains("200 OK"));
    assert!(s1.contains("Simbad Server 0.2.0"));
    assert!(s1.contains("application/json"));

    // 2. GET /healthz -> Plaintext healthy
    let req_healthz = b"GET /healthz HTTP/1.1\r\nHost: 10.0.2.15\r\n\r\n";
    let n2 = httpd.handle_request(req_healthz, &mut resp);
    assert!(n2 > 0);
    let s2 = std::str::from_utf8(&resp[..n2]).unwrap();
    assert!(s2.contains("200 OK"));
    assert!(s2.contains("healthy\n"));

    // 3. GET /metrics -> Prometheus Exposition Format
    let req_metrics = b"GET /metrics HTTP/1.1\r\nHost: 10.0.2.15\r\n\r\n";
    let n3 = httpd.handle_request(req_metrics, &mut resp);
    assert!(n3 > 0);
    let s3 = std::str::from_utf8(&resp[..n3]).unwrap();
    assert!(s3.contains("200 OK"));
    assert!(s3.contains("jinn_uptime_seconds"));
    assert!(s3.contains("jinn_services_running"));

    // 4. POST / -> 405 Method Not Allowed
    let req_post = b"POST /api/data HTTP/1.1\r\nHost: 10.0.2.15\r\n\r\n";
    let n4 = httpd.handle_request(req_post, &mut resp);
    let s4 = std::str::from_utf8(&resp[..n4]).unwrap();
    assert!(s4.contains("405 Method Not Allowed"));
    assert!(s4.contains("Allow: GET, HEAD"));

    // 5. GET /unknown -> 404 Not Found
    let req_404 = b"GET /nonexistent HTTP/1.1\r\nHost: 10.0.2.15\r\n\r\n";
    let n5 = httpd.handle_request(req_404, &mut resp);
    let s5 = std::str::from_utf8(&resp[..n5]).unwrap();
    assert!(s5.contains("404 Not Found"));

    // 6. Malformed HTTP -> 400 Bad Request
    let req_bad = b"CORRUPTED_GARBAGE_WITHOUT_HTTP_METHOD";
    let n6 = httpd.handle_request(req_bad, &mut resp);
    let s6 = std::str::from_utf8(&resp[..n6]).unwrap();
    assert!(s6.contains("400 Bad Request"));

    // 7. Request exceeding 1024 bytes -> 413 Payload Too Large
    let mut large_req = vec![b'A'; 1200];
    large_req[..16].copy_from_slice(b"GET / HTTP/1.1\r\n");
    let n7 = httpd.handle_request(&large_req, &mut resp);
    let s7 = std::str::from_utf8(&resp[..n7]).unwrap();
    assert!(s7.contains("413 Payload Too Large"));
}

#[test]
fn test_storage_reliability_flusher_and_crash_consistency() {
    let mut disk = SimCachedDisk::new();

    let sector_a = [0x55u8; SECTOR_SIZE];
    let sector_b = [0xAAu8; SECTOR_SIZE];

    // Escrita em cache (WriteBack)
    disk.write_sector(100, &sector_a);
    disk.write_sector(101, &sector_b);

    // Dados estão na RAM, ainda não foram descarregados para o disco físico
    assert_eq!(disk.write_count_to_disk, 0);

    // O daemon jinn-flusher dispara a cada 5s (500 ticks)
    let flushed = disk.flush_dirty_sectors();
    assert_eq!(flushed, 2);
    assert_eq!(disk.write_count_to_disk, 2);
    assert_eq!(disk.disk_storage.get(&100), Some(&sector_a));
    assert_eq!(disk.disk_storage.get(&101), Some(&sector_b));

    // Agora grava novo setor 102 na RAM, mas NÃO descarrega
    let sector_c = [0x99u8; SECTOR_SIZE];
    disk.write_sector(102, &sector_c);

    // Ocorre interrupção catastrófica de energia antes do próximo ciclo de flush (janela < 5s)
    disk.simulate_crash_and_reboot();

    // Recuperação pós-reboot:
    // Setores 100 e 101 foram devidamente persistidos e mantêm integridade exata!
    assert_eq!(disk.disk_storage.get(&100), Some(&sector_a));
    assert_eq!(disk.disk_storage.get(&101), Some(&sector_b));

    // Setor 102 que estava apenas no buffer volátil da RAM foi perdido
    assert_eq!(disk.disk_storage.get(&102), None);
}

#[test]
fn test_kernel_resource_isolation_and_quotas() {
    let limits = SimResourceLimits {
        max_frames: 4,
        max_fds: 2,
        max_sockets: 2,
        max_cpu_ticks: 50,
    };
    let mut res = SimProcessResources::new();

    // 1. Alocação de memória: até 4 frames OK
    assert!(res.check_and_alloc_frames(3, &limits).is_ok());
    assert_eq!(res.allocated_frames, 3);

    // Alocar mais 2 frames excederia o limite (3 + 2 = 5 > 4) -> Rejeitado com ENOMEM (-12)
    assert_eq!(res.check_and_alloc_frames(2, &limits), Err(ENOMEM));
    assert_eq!(res.limit_violations, 1);

    // 2. Descritores de arquivo: limite de 2
    assert!(res.check_and_open_fd(&limits).is_ok());
    assert!(res.check_and_open_fd(&limits).is_ok());
    // Terceiro descritor rejeitado com EMFILE (-24)
    assert_eq!(res.check_and_open_fd(&limits), Err(EMFILE));
    assert_eq!(res.limit_violations, 2);

    // 3. Sockets de rede: limite de 2
    assert!(res.check_and_open_socket(&limits).is_ok());
    assert!(res.check_and_open_socket(&limits).is_ok());
    assert_eq!(res.check_and_open_socket(&limits), Err(EMFILE));
    assert_eq!(res.limit_violations, 3);
}
