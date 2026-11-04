//! Ring 3 Service Supervisor & Process Lifecycle Manager for Simbad Server
//! Provides deterministic service registration, dependency control, state machine,
//! circuit-breaker restart policies, health monitoring, and structured event recording.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed,
    Restarting,
}

impl ServiceState {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Stopped => "STOPPED",
            Self::Starting => "STARTING",
            Self::Running => "RUNNING",
            Self::Stopping => "STOPPING",
            Self::Failed => "FAILED",
            Self::Restarting => "RESTARTING",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartPolicy {
    Always,
    OnFailure,
    Never,
}

impl RestartPolicy {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::OnFailure => "on-failure",
            Self::Never => "never",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ServiceDescriptor {
    pub id: usize,
    pub name: &'static str,
    pub description: &'static str,
    pub state: ServiceState,
    pub pid: Option<usize>,
    pub restart_policy: RestartPolicy,
    pub restart_count: u32,
    pub max_retries: u32,
    pub last_state_change: u64,
    pub dependencies: &'static [&'static str],
}

#[derive(Debug, Clone, Copy)]
pub struct LogEntry {
    pub timestamp_ms: u64,
    pub level: &'static str,
    pub subsys: &'static str,
    pub message: [u8; 64],
    pub msg_len: usize,
}

impl LogEntry {
    pub const fn empty() -> Self {
        Self {
            timestamp_ms: 0,
            level: "INFO",
            subsys: "SYSTEM",
            message: [0; 64],
            msg_len: 0,
        }
    }
}

const MAX_SERVICES: usize = 8;
const MAX_LOGS: usize = 32;

pub struct ServiceSupervisor {
    pub services: [Option<ServiceDescriptor>; MAX_SERVICES],
    pub service_count: usize,
    pub logs: [LogEntry; MAX_LOGS],
    pub log_head: usize,
    pub log_count: usize,
}

impl ServiceSupervisor {
    pub const fn new() -> Self {
        Self {
            services: [None; MAX_SERVICES],
            service_count: 0,
            logs: [LogEntry::empty(); MAX_LOGS],
            log_head: 0,
            log_count: 0,
        }
    }

    pub fn init_canonical_services(&mut self) {
        self.service_count = 0;

        // 1. jinn-logd: Log central
        self.register(ServiceDescriptor {
            id: 1,
            name: "jinn-logd",
            description: "Structured Logging & Telemetry Daemon",
            state: ServiceState::Stopped,
            pid: None,
            restart_policy: RestartPolicy::Always,
            restart_count: 0,
            max_retries: 5,
            last_state_change: 0,
            dependencies: &[],
        });

        // 2. jinn-flusher: Sincronização periódica de storage
        self.register(ServiceDescriptor {
            id: 2,
            name: "jinn-flusher",
            description: "Storage Cache Synchronization Daemon (5s sync)",
            state: ServiceState::Stopped,
            pid: None,
            restart_policy: RestartPolicy::Always,
            restart_count: 0,
            max_retries: 5,
            last_state_change: 0,
            dependencies: &["jinn-logd"],
        });

        // 3. jinn-netmgr: Gerenciador de conectividade de rede
        self.register(ServiceDescriptor {
            id: 3,
            name: "jinn-netmgr",
            description: "Network Address & Link Manager (DHCP/DNS)",
            state: ServiceState::Stopped,
            pid: None,
            restart_policy: RestartPolicy::Always,
            restart_count: 0,
            max_retries: 5,
            last_state_change: 0,
            dependencies: &["jinn-logd"],
        });

        // 4. jinn-httpd: Daemon Web e métricas
        self.register(ServiceDescriptor {
            id: 4,
            name: "jinn-httpd",
            description: "Micro HTTP/1.1 Metrics & Healthcheck Daemon",
            state: ServiceState::Stopped,
            pid: None,
            restart_policy: RestartPolicy::OnFailure,
            restart_count: 0,
            max_retries: 5,
            last_state_change: 0,
            dependencies: &["jinn-netmgr"],
        });

        self.log("INFO", "SUPERVISOR", b"Simbad Service Supervisor initialized with 4 services");
    }

    pub fn register(&mut self, descriptor: ServiceDescriptor) {
        if self.service_count < MAX_SERVICES {
            self.services[self.service_count] = Some(descriptor);
            self.service_count += 1;
        }
    }

    pub fn start_service(&mut self, name: &str) -> Result<(), &'static str> {
        let idx = self.find_index(name).ok_or("Service not found")?;

        // 1. Verifica se todas as dependências estão no estado RUNNING
        let deps = if let Some(svc) = &self.services[idx] {
            svc.dependencies
        } else {
            return Err("Service slot empty");
        };

        for &dep in deps {
            let dep_svc = self.get_service(dep).ok_or("Dependency does not exist")?;
            if dep_svc.state != ServiceState::Running {
                return Err("Dependency not satisfied (must be RUNNING)");
            }
        }

        // 2. Transita estado: Stopped -> Starting -> Running
        if let Some(svc) = &mut self.services[idx] {
            if svc.state == ServiceState::Running {
                return Ok(()); // Já em execução
            }
            svc.state = ServiceState::Starting;
            let assigned_pid = 10 + svc.id;
            svc.pid = Some(assigned_pid);
            svc.state = ServiceState::Running;
            svc.last_state_change = crate::timer::ticks();
        }

        let mut msg = [0u8; 64];
        let prefix = b"Service started: ";
        let n_len = name.len().min(32);
        msg[..prefix.len()].copy_from_slice(prefix);
        msg[prefix.len()..prefix.len() + n_len].copy_from_slice(&name.as_bytes()[..n_len]);
        self.log("INFO", "SUPERVISOR", &msg[..prefix.len() + n_len]);

        Ok(())
    }

    pub fn stop_service(&mut self, name: &str) -> Result<(), &'static str> {
        let idx = self.find_index(name).ok_or("Service not found")?;

        // 1. Verifica se algum outro serviço em execução depende deste
        for i in 0..self.service_count {
            if let Some(other) = &self.services[i] {
                if other.state == ServiceState::Running || other.state == ServiceState::Starting {
                    for &d in other.dependencies {
                        if d == name {
                            return Err("Cannot stop: active service depends on this service");
                        }
                    }
                }
            }
        }

        if let Some(svc) = &mut self.services[idx] {
            if svc.state == ServiceState::Stopped {
                return Ok(());
            }
            svc.state = ServiceState::Stopping;
            svc.pid = None;
            svc.state = ServiceState::Stopped;
            svc.restart_count = 0;
            svc.last_state_change = crate::timer::ticks();
        }

        let mut msg = [0u8; 64];
        let prefix = b"Service stopped: ";
        let n_len = name.len().min(32);
        msg[..prefix.len()].copy_from_slice(prefix);
        msg[prefix.len()..prefix.len() + n_len].copy_from_slice(&name.as_bytes()[..n_len]);
        self.log("WARN", "SUPERVISOR", &msg[..prefix.len() + n_len]);

        Ok(())
    }

    pub fn restart_service(&mut self, name: &str) -> Result<(), &'static str> {
        let idx = self.find_index(name).ok_or("Service not found")?;
        if let Some(svc) = &mut self.services[idx] {
            svc.state = ServiceState::Restarting;
            svc.pid = None;
            svc.last_state_change = crate::timer::ticks();
        }
        self.start_service(name)
    }

    pub fn report_exit(&mut self, name: &str, exit_code: i32) -> Result<ServiceState, &'static str> {
        let idx = self.find_index(name).ok_or("Service not found")?;

        let (should_restart, retries, max) = {
            let svc = self.services[idx].as_mut().ok_or("Slot empty")?;
            svc.pid = None;
            svc.last_state_change = crate::timer::ticks();

            if exit_code == 0 {
                svc.state = ServiceState::Stopped;
                return Ok(ServiceState::Stopped);
            }

            svc.state = ServiceState::Failed;
            let can_restart = match svc.restart_policy {
                RestartPolicy::Always => true,
                RestartPolicy::OnFailure => true,
                RestartPolicy::Never => false,
            };
            (can_restart, svc.restart_count, svc.max_retries)
        };

        if should_restart && retries < max {
            if let Some(svc) = &mut self.services[idx] {
                svc.restart_count += 1;
                svc.state = ServiceState::Restarting;
            }
            // Auto re-start
            self.start_service(name)?;
            Ok(ServiceState::Running)
        } else {
            // Circuit breaker tripped!
            let mut msg = [0u8; 64];
            let prefix = b"Circuit breaker tripped for ";
            let n_len = name.len().min(24);
            msg[..prefix.len()].copy_from_slice(prefix);
            msg[prefix.len()..prefix.len() + n_len].copy_from_slice(&name.as_bytes()[..n_len]);
            self.log("CRIT", "CIRCUIT_BREAKER", &msg[..prefix.len() + n_len]);
            Ok(ServiceState::Failed)
        }
    }

    pub fn start_all_target_server(&mut self) {
        // Inicializa serviços em ordem de dependência
        let _ = self.start_service("jinn-logd");
        let _ = self.start_service("jinn-flusher");
        let _ = self.start_service("jinn-netmgr");
        let _ = self.start_service("jinn-httpd");
    }

    pub fn get_service(&self, name: &str) -> Option<&ServiceDescriptor> {
        let idx = self.find_index(name)?;
        self.services[idx].as_ref()
    }

    fn find_index(&self, name: &str) -> Option<usize> {
        for i in 0..self.service_count {
            if let Some(svc) = &self.services[i] {
                if svc.name == name {
                    return Some(i);
                }
            }
        }
        None
    }

    pub fn running_count(&self) -> usize {
        let mut count = 0;
        for i in 0..self.service_count {
            if let Some(svc) = &self.services[i] {
                if svc.state == ServiceState::Running {
                    count += 1;
                }
            }
        }
        count
    }

    pub fn failed_count(&self) -> usize {
        let mut count = 0;
        for i in 0..self.service_count {
            if let Some(svc) = &self.services[i] {
                if svc.state == ServiceState::Failed {
                    count += 1;
                }
            }
        }
        count
    }

    pub fn total_restarts(&self) -> u32 {
        let mut total = 0;
        for i in 0..self.service_count {
            if let Some(svc) = &self.services[i] {
                total += svc.restart_count;
            }
        }
        total
    }

    pub fn log(&mut self, level: &'static str, subsys: &'static str, msg: &[u8]) {
        let mut entry = LogEntry::empty();
        entry.timestamp_ms = crate::timer::ticks() * 10;
        entry.level = level;
        entry.subsys = subsys;
        let len = msg.len().min(64);
        entry.message[..len].copy_from_slice(&msg[..len]);
        entry.msg_len = len;

        let slot = self.log_head;
        self.logs[slot] = entry;
        self.log_head = (self.log_head + 1) % MAX_LOGS;
        if self.log_count < MAX_LOGS {
            self.log_count += 1;
        }
    }
}

pub static mut SUPERVISOR: ServiceSupervisor = ServiceSupervisor::new();

pub fn get_supervisor() -> &'static mut ServiceSupervisor {
    unsafe { &mut *core::ptr::addr_of_mut!(SUPERVISOR) }
}
