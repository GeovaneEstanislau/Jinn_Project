/// Jinn OS — Unified Device Manager (L1 / L2)
///
/// Gerenciador unificado de dispositivos do sistema operacional.
/// Responsável pelo inventário de hardware, rastreamento de ciclo de vida (descoberta,
/// inicialização, prontidão operacional, suspensão e falha) e desacoplamento do kernel
/// de inicializações estáticas.

use core::sync::atomic::{AtomicUsize, AtomicU32, Ordering};

pub const MAX_DEVICES: usize = 32;

// Constantes bem-conhecidas de identificadores de periféricos de entrada
pub const DEV_KBD0: u32 = 7;
pub const DEV_MOUSE0: u32 = 8;
pub static KBD_DEVICE_ID: AtomicU32 = AtomicU32::new(DEV_KBD0);
pub static MOUSE_DEVICE_ID: AtomicU32 = AtomicU32::new(DEV_MOUSE0);

// Capacidades de Driver e Políticas de Isolamento (Distinção DMA vs PIO)
pub const DRIVER_CAP_PIO: u32 = 0x0001;     // I/O programado mediado por filas de eventos do kernel
pub const DRIVER_CAP_DMA: u32 = 0x0002;     // DMA físico (rejeitado sem IOMMU)
pub const DRIVER_CAP_INPUT: u32 = 0x0004;   // Periférico de entrada de usuário
pub const DRIVER_CAP_STORAGE: u32 = 0x0008; // Controlador de bloco
pub const DRIVER_CAP_NET: u32 = 0x0010;     // Adaptador de rede

// Tipos de Eventos de Hardware Brutos do Kernel -> User-Space
pub const EVENT_TYPE_RAW_BYTE: u16 = 1;     // Byte bruto capturado da porta de dados
pub const EVENT_TYPE_IRQ: u16 = 2;          // Notificação de interrupção física
pub const EVENT_TYPE_OVERFLOW_WARN: u16 = 4;// Alerta de perda/descarte por saturação
pub const EVENT_TYPE_RESET_SYNC: u16 = 5;   // Solicitação de reset de sincronização de protocolo

// Flags de Eventos
pub const EVENT_FLAG_SPURIOUS: u32 = 0x0001;// Interrupção espúria detectada
pub const EVENT_FLAG_OVERFLOW: u32 = 0x0002;// Ocorrência de saturação de fila

// Tipos de Entrada Normalizada User-Space -> Kernel
pub const INPUT_KIND_KEY_DOWN: u16 = 1;
pub const INPUT_KIND_KEY_UP: u16 = 2;
pub const INPUT_KIND_MOUSE_MOVE: u16 = 3;
pub const INPUT_KIND_MOUSE_BUTTON_DOWN: u16 = 4;
pub const INPUT_KIND_MOUSE_BUTTON_UP: u16 = 5;

/// Categoria padronizada de dispositivo de hardware
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceCategory {
    Unknown  = 0,
    Storage  = 1,
    Network  = 2,
    Display  = 3,
    Input    = 4,
    Bus      = 5,
    System   = 6,
}

impl DeviceCategory {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Unknown => "Desconhecido",
            Self::Storage => "Armazenamento",
            Self::Network => "Rede",
            Self::Display => "Video / Display",
            Self::Input   => "Entrada (Input)",
            Self::Bus     => "Barramento (Bus)",
            Self::System  => "Sistema / Chipset",
        }
    }
}

/// Estado operacional no ciclo de vida do dispositivo
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceState {
    Discovered  = 1, // Detectado no barramento, driver pendente
    Initialized = 2, // Driver carregado, registradores configurados
    Operational = 3, // Ativo e servindo requisições do sistema
    Suspended   = 4, // Em baixo consumo ou pausa
    Failed      = 5, // Falha de hardware ou timeout
}

impl DeviceState {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Discovered  => "Descoberto",
            Self::Initialized => "Inicializado",
            Self::Operational => "Operacional",
            Self::Suspended   => "Suspenso",
            Self::Failed      => "Falha",
        }
    }
}

/// Descritor completo de um dispositivo gerenciado (compatível com ABI C e Syscalls)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DeviceDescriptor {
    pub id: u32,
    pub category: DeviceCategory,
    pub state: DeviceState,
    pub vendor_id: u16,
    pub device_id: u16,
    pub name: [u8; 32],
    pub name_len: u32,
    pub bus_info: [u8; 32],
    pub bus_info_len: u32,
    pub capabilities: u32,
}

impl DeviceDescriptor {
    pub const fn empty() -> Self {
        Self {
            id: 0,
            category: DeviceCategory::Unknown,
            state: DeviceState::Discovered,
            vendor_id: 0,
            device_id: 0,
            name: [0; 32],
            name_len: 0,
            bus_info: [0; 32],
            bus_info_len: 0,
            capabilities: 0,
        }
    }

    pub fn new(
        id: u32,
        category: DeviceCategory,
        state: DeviceState,
        vendor_id: u16,
        device_id: u16,
        name_str: &str,
        bus_str: &str,
        capabilities: u32,
    ) -> Self {
        let mut d = Self::empty();
        d.id = id;
        d.category = category;
        d.state = state;
        d.vendor_id = vendor_id;
        d.device_id = device_id;
        d.capabilities = capabilities;

        let nb = name_str.as_bytes();
        let nlen = nb.len().min(31);
        d.name[..nlen].copy_from_slice(&nb[..nlen]);
        d.name_len = nlen as u32;

        let bb = bus_str.as_bytes();
        let blen = bb.len().min(31);
        d.bus_info[..blen].copy_from_slice(&bb[..blen]);
        d.bus_info_len = blen as u32;

        d
    }

    pub fn name_str(&self) -> &str {
        if self.name_len == 0 {
            "unknown"
        } else {
            core::str::from_utf8(&self.name[..self.name_len as usize]).unwrap_or("invalid")
        }
    }

    pub fn bus_info_str(&self) -> &str {
        if self.bus_info_len == 0 {
            "none"
        } else {
            core::str::from_utf8(&self.bus_info[..self.bus_info_len as usize]).unwrap_or("none")
        }
    }
}

/// Registro central de dispositivos
pub struct DeviceRegistry {
    pub devices: [Option<DeviceDescriptor>; MAX_DEVICES],
    pub count: usize,
}

impl DeviceRegistry {
    pub const fn new() -> Self {
        const NONE_DEV: Option<DeviceDescriptor> = None;
        Self {
            devices: [NONE_DEV; MAX_DEVICES],
            count: 0,
        }
    }

    pub fn register(&mut self, mut desc: DeviceDescriptor) -> Result<usize, &'static str> {
        if self.count >= MAX_DEVICES {
            return Err("DeviceRegistry: capacidade maxima excedida");
        }

        // Verifica se já existe dispositivo com mesmo nome ou IDs idênticos
        for slot in self.devices.iter() {
            if let Some(existing) = slot {
                if existing.name_str() == desc.name_str() && existing.bus_info_str() == desc.bus_info_str() {
                    return Ok(existing.id as usize);
                }
            }
        }

        let assigned_id = self.count;
        desc.id = assigned_id as u32;

        for slot in self.devices.iter_mut() {
            if slot.is_none() {
                *slot = Some(desc);
                self.count += 1;
                return Ok(assigned_id);
            }
        }

        Err("DeviceRegistry: sem slots disponiveis")
    }

    pub fn get_by_index(&self, index: usize) -> Option<DeviceDescriptor> {
        if index < MAX_DEVICES {
            self.devices[index]
        } else {
            None
        }
    }

    pub fn find_by_name(&self, name: &str) -> Option<DeviceDescriptor> {
        for slot in self.devices.iter().flatten() {
            if slot.name_str() == name {
                return Some(*slot);
            }
        }
        None
    }

    pub fn update_state(&mut self, id: usize, new_state: DeviceState) -> bool {
        for slot in self.devices.iter_mut() {
            if let Some(desc) = slot {
                if desc.id as usize == id {
                    desc.state = new_state;
                    return true;
                }
            }
        }
        false
    }
}

use crate::memory::SpinLock;

pub static DEVICE_REGISTRY: SpinLock<DeviceRegistry> = SpinLock::new(DeviceRegistry::new());
static REGISTRY_INITIALIZED: AtomicUsize = AtomicUsize::new(0);

/// Inicializa o Unified Device Manager e varre os subsistemas de hardware
pub fn init() {
    if REGISTRY_INITIALIZED.swap(1, Ordering::SeqCst) != 0 {
        return;
    }

    let mut reg = DEVICE_REGISTRY.lock();

    // 1. Registra Barramento PCI Host Bridge
    reg.register(DeviceDescriptor::new(
        0,
        DeviceCategory::Bus,
        DeviceState::Operational,
        0x8086,
        0x1237,
        "pci_bus0",
        "PCI Host 0xCF8/0xCFC",
        0x0001,
    )).ok();

    // 2. Registra controladoras de armazenamento descobertas
    // 2.1 IDE / ATA Legacy
    reg.register(DeviceDescriptor::new(
        0,
        DeviceCategory::Storage,
        DeviceState::Operational,
        0x8086,
        0x7010,
        "ide0",
        "I/O 0x1F0-0x1F7 (Primary)",
        0x0003,
    )).ok();

    // 2.2 Sonda se há AHCI ativo
    if crate::drivers::ahci::has_ahci_controller() {
        reg.register(DeviceDescriptor::new(
            0,
            DeviceCategory::Storage,
            DeviceState::Operational,
            0x8086,
            0x2922,
            "ahci0",
            "PCI Class 01:06:01 (SATA)",
            0x0007,
        )).ok();
    }

    // 2.3 Sonda se há NVMe ativo
    if crate::drivers::nvme::has_nvme_controller() {
        reg.register(DeviceDescriptor::new(
            0,
            DeviceCategory::Storage,
            DeviceState::Operational,
            0x1B36,
            0x0010,
            "nvme0",
            "PCI Class 01:08:02 (PCIe)",
            0x0007,
        )).ok();
    }

    // 2.4 Sonda se há VirtIO-Block ativo
    if crate::drivers::virtio_blk::has_virtio_blk() {
        reg.register(DeviceDescriptor::new(
            0,
            DeviceCategory::Storage,
            DeviceState::Operational,
            0x1AF4,
            0x1001,
            "virtio_blk0",
            "VirtIO Block PCI (DMA)",
            0x0007,
        )).ok();
    }


    // 3. Registra Adaptadores de Rede
    match unsafe { crate::drivers::net::DETECTED_NIC_TYPE } {
        crate::drivers::net::NicType::Virtio => {
            reg.register(DeviceDescriptor::new(
                0,
                DeviceCategory::Network,
                DeviceState::Operational,
                0x1AF4,
                0x1000,
                "net0_virtio",
                "VirtIO-Net PCI (10 Mbps)",
                0x0005,
            )).ok();
        }
        crate::drivers::net::NicType::E1000 => {
            reg.register(DeviceDescriptor::new(
                0,
                DeviceCategory::Network,
                DeviceState::Operational,
                0x8086,
                0x100E,
                "net0_e1000",
                "Intel 82540EM (1 Gbps)",
                0x0007,
            )).ok();
        }
        crate::drivers::net::NicType::Pcnet => {
            reg.register(DeviceDescriptor::new(
                0,
                DeviceCategory::Network,
                DeviceState::Operational,
                0x1022,
                0x2000,
                "net0_pcnet",
                "AMD PCnet-FAST III (100 Mbps)",
                0x0005,
            )).ok();
        }
        crate::drivers::net::NicType::None => {
            reg.register(DeviceDescriptor::new(
                0,
                DeviceCategory::Network,
                DeviceState::Operational,
                0x0000,
                0x0000,
                "net_loopback",
                "Virtual Loopback 127.0.0.1",
                0x0001,
            )).ok();
        }
    }

    // 4. Registra Adaptador de Vídeo Limine Framebuffer GOP
    reg.register(DeviceDescriptor::new(
        0,
        DeviceCategory::Display,
        DeviceState::Operational,
        0x1234,
        0x1111,
        "fb0",
        "Limine GOP 1024x768x32bpp",
        0x000F,
    )).ok();

    // 5. Registra Periféricos de Entrada
    if let Ok(id) = reg.register(DeviceDescriptor::new(
        0,
        DeviceCategory::Input,
        DeviceState::Operational,
        0x0000,
        0x0001,
        "kbd0",
        "PS/2 Keyboard (Port 0x60)",
        0x0003,
    )) {
        KBD_DEVICE_ID.store(id as u32, Ordering::SeqCst);
    }

    if let Ok(id) = reg.register(DeviceDescriptor::new(
        0,
        DeviceCategory::Input,
        DeviceState::Operational,
        0x0000,
        0x0002,
        "mouse0",
        "PS/2 Mouse (IRQ 12)",
        0x0003,
    )) {
        MOUSE_DEVICE_ID.store(id as u32, Ordering::SeqCst);
    }

    // 6. Registra Temporizador e Controlador de Interrupções do Sistema
    reg.register(DeviceDescriptor::new(
        0,
        DeviceCategory::System,
        DeviceState::Operational,
        0x0000,
        0x0000,
        "apic_timer0",
        "Local APIC / PIT (100 Hz)",
        0x0001,
    )).ok();
}

/// Retorna o número de dispositivos cadastrados
pub fn device_count() -> usize {
    DEVICE_REGISTRY.lock().count
}

/// Obtém um descritor de dispositivo pelo índice
pub fn get_device(index: usize) -> Option<DeviceDescriptor> {
    DEVICE_REGISTRY.lock().get_by_index(index)
}

/// Localiza um descritor de dispositivo pelo nome
pub fn find_device(name: &str) -> Option<DeviceDescriptor> {
    DEVICE_REGISTRY.lock().find_by_name(name)
}


/// Gera relatório textual consolidado dos dispositivos gerenciados
pub fn generate_device_summary(buf: &mut [u8]) -> usize {
    let mut offset = 0;

    let header = b"=== Jinn OS Unified Device Manager ===\nID  CATEGORIA        NOME            ESTADO       BARRAMENTO\n----------------------------------------------------------------------\n";
    let hlen = header.len().min(buf.len());
    buf[..hlen].copy_from_slice(&header[..hlen]);
    offset += hlen;

    let count = device_count();
    for i in 0..count {
        if let Some(dev) = get_device(i) {
            if offset + 80 >= buf.len() {
                break;
            }

            // ID
            let id_str = match dev.id {
                0..=9 => [b'0' + dev.id as u8, b' ', b' '],
                10..=99 => [b'0' + (dev.id / 10) as u8, b'0' + (dev.id % 10) as u8, b' '],
                _ => [b'?', b'?', b' '],
            };
            for &b in &id_str {
                if offset < buf.len() { buf[offset] = b; offset += 1; }
            }
            if offset < buf.len() { buf[offset] = b' '; offset += 1; }

            // Categoria (16 chars)
            let cat_bytes = dev.category.name().as_bytes();
            for j in 0..16 {
                let b = if j < cat_bytes.len() { cat_bytes[j] } else { b' ' };
                if offset < buf.len() { buf[offset] = b; offset += 1; }
            }

            // Nome (16 chars)
            let name_bytes = dev.name_str().as_bytes();
            for j in 0..16 {
                let b = if j < name_bytes.len() { name_bytes[j] } else { b' ' };
                if offset < buf.len() { buf[offset] = b; offset += 1; }
            }

            // Estado (12 chars)
            let state_bytes = dev.state.name().as_bytes();
            for j in 0..12 {
                let b = if j < state_bytes.len() { state_bytes[j] } else { b' ' };
                if offset < buf.len() { buf[offset] = b; offset += 1; }
            }

            // Barramento
            let bus_bytes = dev.bus_info_str().as_bytes();
            for &b in bus_bytes {
                if offset < buf.len() { buf[offset] = b; offset += 1; }
            }

            if offset < buf.len() { buf[offset] = b'\n'; offset += 1; }
        }
    }

    offset
}

// ─────────────────────────────────────────────────────────────────────────────
// Subsistema de Drivers em User-Space (Ring 3) e Isolamento de Falhas (Etapa 1)
// ─────────────────────────────────────────────────────────────────────────────

#[inline]
pub fn is_kbd_device(id: u32) -> bool {
    id == DEV_KBD0 || id == KBD_DEVICE_ID.load(Ordering::Relaxed)
}

#[inline]
pub fn is_mouse_device(id: u32) -> bool {
    id == DEV_MOUSE0 || id == MOUSE_DEVICE_ID.load(Ordering::Relaxed)
}

#[inline]
fn is_matching_dev(a: u32, b: u32) -> bool {
    (is_kbd_device(a) && is_kbd_device(b)) || (is_mouse_device(a) && is_mouse_device(b)) || (a == b)
}

/// Descritor de evento de hardware bruto entregue pelo kernel ao driver em Ring 3
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HardwareEvent {
    pub timestamp_ticks: u64,
    pub device_id: u32,
    pub event_type: u16,
    pub code: u16,
    pub data: u32,
    pub flags: u32,
}

impl HardwareEvent {
    pub const fn empty() -> Self {
        Self {
            timestamp_ticks: 0,
            device_id: 0,
            event_type: 0,
            code: 0,
            data: 0,
            flags: 0,
        }
    }
}

pub const EVENT_QUEUE_CAPACITY: usize = 64;

/// Fila circular delimitada (bounded) para eventos brutos de hardware
pub struct HardwareEventQueue {
    pub events: [HardwareEvent; EVENT_QUEUE_CAPACITY],
    pub head: usize,
    pub tail: usize,
    pub count: usize,
    pub dropped_overflow: u64,
    pub spurious_count: u64,
}

impl HardwareEventQueue {
    pub const fn new() -> Self {
        Self {
            events: [HardwareEvent::empty(); EVENT_QUEUE_CAPACITY],
            head: 0,
            tail: 0,
            count: 0,
            dropped_overflow: 0,
            spurious_count: 0,
        }
    }

    pub fn push(&mut self, event: HardwareEvent) -> bool {
        if self.count >= EVENT_QUEUE_CAPACITY {
            self.dropped_overflow = self.dropped_overflow.saturating_add(1);
            return false;
        }
        self.events[self.head] = event;
        self.head = (self.head + 1) % EVENT_QUEUE_CAPACITY;
        self.count += 1;
        true
    }

    pub fn pop(&mut self) -> Option<HardwareEvent> {
        if self.count == 0 {
            None
        } else {
            let ev = self.events[self.tail];
            self.tail = (self.tail + 1) % EVENT_QUEUE_CAPACITY;
            self.count -= 1;
            Some(ev)
        }
    }

    pub fn clear(&mut self) {
        self.head = 0;
        self.tail = 0;
        self.count = 0;
    }
}

/// Estado e autorização de um serviço de driver em execução em Ring 3
pub struct DriverService {
    pub device_id: u32,
    pub owner_pid: usize,
    pub generation: u64,
    pub flags: u32,
    pub is_active: bool,
    pub quota_cpu_ticks: u32,
    pub event_queue: HardwareEventQueue,
    pub events_posted: u64,
    pub events_polled: u64,
    pub inputs_received: u64,
}

impl DriverService {
    pub const fn empty() -> Self {
        Self {
            device_id: 0,
            owner_pid: 0,
            generation: 0,
            flags: 0,
            is_active: false,
            quota_cpu_ticks: 25,
            event_queue: HardwareEventQueue::new(),
            events_posted: 0,
            events_polled: 0,
            inputs_received: 0,
        }
    }
}

pub const MAX_DRIVER_SERVICES: usize = 16;

/// Registro central de supervisão e mediação de drivers de Ring 3
pub struct DriverServiceRegistry {
    pub services: [Option<DriverService>; MAX_DRIVER_SERVICES],
    pub next_generation: u64,
}

impl DriverServiceRegistry {
    pub const fn new() -> Self {
        const NONE_SVC: Option<DriverService> = None;
        Self {
            services: [NONE_SVC; MAX_DRIVER_SERVICES],
            next_generation: 1,
        }
    }

    /// Registra um processo em Ring 3 como driver responsável por um dispositivo
    pub fn register(&mut self, device_id: u32, pid: usize, flags: u32) -> Result<u64, isize> {
        // Regra 1: Rejeição estrita de DMA sem IOMMU (Isolamento de Processo != Isolamento de DMA)
        if (flags & DRIVER_CAP_DMA) != 0 {
            return Err(crate::syscall::EPERM);
        }

        let resolved_id = if is_kbd_device(device_id) {
            KBD_DEVICE_ID.load(Ordering::Relaxed)
        } else if is_mouse_device(device_id) {
            MOUSE_DEVICE_ID.load(Ordering::Relaxed)
        } else {
            device_id
        };

        // Valida se o dispositivo existe no inventário do kernel
        let dev_exists = DEVICE_REGISTRY.lock().get_by_index(resolved_id as usize).is_some();
        if !dev_exists {
            return Err(crate::syscall::ENOENT);
        }

        // Verifica conflito de exclusividade: somente 1 driver ativo por dispositivo
        for slot in self.services.iter() {
            if let Some(svc) = slot {
                if (svc.device_id == resolved_id || is_matching_dev(svc.device_id, resolved_id)) && svc.is_active {
                    return Err(crate::syscall::EBUSY);
                }
            }
        }

        self.next_generation = self.next_generation.wrapping_add(1);
        let gen = self.next_generation;

        for slot in self.services.iter_mut() {
            if slot.is_none() || !slot.as_ref().unwrap().is_active {
                *slot = Some(DriverService {
                    device_id: resolved_id,
                    owner_pid: pid,
                    generation: gen,
                    flags,
                    is_active: true,
                    quota_cpu_ticks: 25,
                    event_queue: HardwareEventQueue::new(),
                    events_posted: 0,
                    events_polled: 0,
                    inputs_received: 0,
                });
                return Ok(gen);
            }
        }

        Err(crate::syscall::ENOMEM)
    }

    /// Desregistra o driver e devolve o controle imediato ao Ring 0
    pub fn unregister(&mut self, device_id: u32, pid: usize) -> Result<(), isize> {
        let resolved_id = if is_kbd_device(device_id) {
            KBD_DEVICE_ID.load(Ordering::Relaxed)
        } else if is_mouse_device(device_id) {
            MOUSE_DEVICE_ID.load(Ordering::Relaxed)
        } else {
            device_id
        };

        for slot in self.services.iter_mut() {
            if let Some(svc) = slot {
                if (svc.device_id == resolved_id || is_matching_dev(svc.device_id, resolved_id)) && svc.owner_pid == pid {
                    svc.is_active = false;
                    svc.generation = svc.generation.wrapping_add(1);
                    svc.event_queue.clear();
                    crate::interrupts::ps2_keyboard::reset_sync();
                    crate::interrupts::ps2_mouse::reset_sync();
                    *slot = None;
                    return Ok(());
                }
            }
        }
        Err(crate::syscall::ENOENT)
    }

    /// Reinicia o serviço de driver, invalidando o generation counter anterior
    pub fn restart(&mut self, device_id: u32, pid: usize) -> Result<u64, isize> {
        let resolved_id = if is_kbd_device(device_id) {
            KBD_DEVICE_ID.load(Ordering::Relaxed)
        } else if is_mouse_device(device_id) {
            MOUSE_DEVICE_ID.load(Ordering::Relaxed)
        } else {
            device_id
        };

        for slot in self.services.iter_mut() {
            if let Some(svc) = slot {
                if svc.device_id == resolved_id || is_matching_dev(svc.device_id, resolved_id) {
                    if svc.owner_pid != pid {
                        return Err(crate::syscall::EPERM);
                    }
                    self.next_generation = self.next_generation.wrapping_add(1);
                    svc.generation = self.next_generation;
                    svc.event_queue.clear();
                    svc.is_active = true;
                    crate::interrupts::ps2_keyboard::reset_sync();
                    crate::interrupts::ps2_mouse::reset_sync();
                    return Ok(svc.generation);
                }
            }
        }
        Err(crate::syscall::ENOENT)
    }

    /// Polling de eventos de hardware pelo driver em Ring 3
    pub fn poll_events(&mut self, device_id: u32, pid: usize, out_slice: &mut [HardwareEvent]) -> Result<usize, isize> {
        let resolved_id = if is_kbd_device(device_id) {
            KBD_DEVICE_ID.load(Ordering::Relaxed)
        } else if is_mouse_device(device_id) {
            MOUSE_DEVICE_ID.load(Ordering::Relaxed)
        } else {
            device_id
        };

        for slot in self.services.iter_mut() {
            if let Some(svc) = slot {
                if svc.device_id == resolved_id || is_matching_dev(svc.device_id, resolved_id) {
                    if svc.owner_pid != pid || !svc.is_active {
                        return Err(crate::syscall::EPERM);
                    }
                    let mut count = 0;
                    while count < out_slice.len() {
                        if let Some(ev) = svc.event_queue.pop() {
                            out_slice[count] = ev;
                            count += 1;
                        } else {
                            break;
                        }
                    }
                    svc.events_polled = svc.events_polled.saturating_add(count as u64);
                    return Ok(count);
                }
            }
        }
        Err(crate::syscall::ENOENT)
    }

    /// Envio de evento normalizado pelo driver em Ring 3 de volta ao kernel
    pub fn dispatch_normalized_input(&mut self, pid: usize, kind: u16, code: u16, x: i32, y: i32) -> Result<(), isize> {
        // Valida se o PID possui autorização de driver ativo com DRIVER_CAP_INPUT
        let mut authorized = false;
        for slot in self.services.iter() {
            if let Some(svc) = slot {
                if svc.owner_pid == pid && svc.is_active && (svc.flags & DRIVER_CAP_INPUT) != 0 {
                    authorized = true;
                    break;
                }
            }
        }
        if !authorized {
            return Err(crate::syscall::EPERM);
        }

        match kind {
            INPUT_KIND_KEY_DOWN => {
                if code != 0 {
                    crate::keyboard_buffer::push(code as u8);
                }
            }
            INPUT_KIND_KEY_UP => {
                // Notificação de liberação de tecla (processada se necessário)
            }
            INPUT_KIND_MOUSE_MOVE | INPUT_KIND_MOUSE_BUTTON_DOWN | INPUT_KIND_MOUSE_BUTTON_UP => {
                let clamped_x = x.max(0).min(1024) as usize;
                let clamped_y = y.max(0).min(768) as usize;
                crate::interrupts::ps2_mouse::update_from_driver(kind, code, clamped_x, clamped_y);
            }
            _ => return Err(crate::syscall::EINVAL),
        }

        for slot in self.services.iter_mut() {
            if let Some(svc) = slot {
                if svc.owner_pid == pid && svc.is_active {
                    svc.inputs_received = svc.inputs_received.saturating_add(1);
                }
            }
        }

        Ok(())
    }

    /// Verifica se há um driver em Ring 3 ativamente atendendo ao dispositivo
    pub fn has_active_driver(&self, device_id: u32) -> bool {
        let resolved_id = if is_kbd_device(device_id) {
            KBD_DEVICE_ID.load(Ordering::Relaxed)
        } else if is_mouse_device(device_id) {
            MOUSE_DEVICE_ID.load(Ordering::Relaxed)
        } else {
            device_id
        };

        for slot in self.services.iter() {
            if let Some(svc) = slot {
                if (svc.device_id == resolved_id || is_matching_dev(svc.device_id, resolved_id)) && svc.is_active {
                    return true;
                }
            }
        }
        false
    }

    /// Enfileira byte capturado pelo ISR / Polling do kernel na fila do driver em Ring 3.
    /// Retorna `true` se o byte foi enfileirado para o driver, ou `false` para indicar fallback em Ring 0.
    pub fn post_raw_hardware_byte(&mut self, device_id: u32, irq: u8, byte: u8) -> bool {
        let resolved_id = if is_kbd_device(device_id) {
            KBD_DEVICE_ID.load(Ordering::Relaxed)
        } else if is_mouse_device(device_id) {
            MOUSE_DEVICE_ID.load(Ordering::Relaxed)
        } else {
            device_id
        };

        for slot in self.services.iter_mut() {
            if let Some(svc) = slot {
                if (svc.device_id == resolved_id || is_matching_dev(svc.device_id, resolved_id)) && svc.is_active {
                    let ticks = crate::timer::ticks();
                    let mut flags = 0u32;
                    if svc.event_queue.dropped_overflow > 0 {
                        flags |= EVENT_FLAG_OVERFLOW;
                    }
                    let ev = HardwareEvent {
                        timestamp_ticks: ticks,
                        device_id: resolved_id,
                        event_type: EVENT_TYPE_RAW_BYTE,
                        code: irq as u16,
                        data: byte as u32,
                        flags,
                    };
                    if !svc.event_queue.push(ev) {
                        svc.event_queue.dropped_overflow = svc.event_queue.dropped_overflow.saturating_add(1);
                    }
                    svc.events_posted = svc.events_posted.saturating_add(1);
                    return true;
                }
            }
        }
        false
    }

    /// Notifica falha do driver e reativa fallback de Ring 0 imediatamente
    pub fn notify_driver_failure(&mut self, device_id: u32) {
        let resolved_id = if is_kbd_device(device_id) {
            KBD_DEVICE_ID.load(Ordering::Relaxed)
        } else if is_mouse_device(device_id) {
            MOUSE_DEVICE_ID.load(Ordering::Relaxed)
        } else {
            device_id
        };

        for slot in self.services.iter_mut() {
            if let Some(svc) = slot {
                if (svc.device_id == resolved_id || is_matching_dev(svc.device_id, resolved_id)) && svc.is_active {
                    svc.is_active = false;
                    svc.generation = svc.generation.wrapping_add(1);
                    svc.event_queue.clear();
                    crate::interrupts::ps2_keyboard::reset_sync();
                    crate::interrupts::ps2_mouse::reset_sync();
                }
            }
        }
    }

    /// Notifica encerramento ou crash de um PID, restaurando fallback para Ring 0
    pub fn notify_pid_exit(&mut self, pid: usize) {
        for slot in self.services.iter_mut() {
            if let Some(svc) = slot {
                if svc.owner_pid == pid && svc.is_active {
                    svc.is_active = false;
                    svc.generation = svc.generation.wrapping_add(1);
                    svc.event_queue.clear();
                    let _dev_id = svc.device_id;
                    crate::interrupts::ps2_keyboard::reset_sync();
                    crate::interrupts::ps2_mouse::reset_sync();
                    crate::serial_write(b"[DRIVER-MGR] Processo de driver encerrou; fallback Ring 0 restaurado\n");
                }
            }
        }
    }
}

pub static DRIVER_SERVICES: SpinLock<DriverServiceRegistry> = SpinLock::new(DriverServiceRegistry::new());

// ── Funções de Interface Pública para ISRs e Agendador ──────────────────────

pub fn has_active_driver(device_id: u32) -> bool {
    DRIVER_SERVICES.lock().has_active_driver(device_id)
}

pub fn post_raw_hardware_byte(device_id: u32, irq: u8, byte: u8) -> bool {
    DRIVER_SERVICES.lock().post_raw_hardware_byte(device_id, irq, byte)
}

pub fn notify_driver_pid_exit(pid: usize) {
    DRIVER_SERVICES.lock().notify_pid_exit(pid);
}

pub fn notify_driver_failure(device_id: u32) {
    DRIVER_SERVICES.lock().notify_driver_failure(device_id);
}

// ── Suíte de Selftests do Driver Manager e Ring 3 Drivers ────────────────────

pub fn run_driver_manager_selftests() -> (usize, usize) {
    let mut passed = 0;
    let mut total = 0;

    let test_pid = 999usize;
    let dev_kbd = KBD_DEVICE_ID.load(Ordering::Relaxed);

    // Teste 1: Registro válido com PIO e INPUT
    total += 1;
    let reg_result = DRIVER_SERVICES.lock().register(dev_kbd, test_pid, DRIVER_CAP_PIO | DRIVER_CAP_INPUT);
    if let Ok(gen) = reg_result {
        if gen > 0 && has_active_driver(dev_kbd) {
            passed += 1;
        }
    }

    // Teste 2: Rejeição estrita de DMA sem IOMMU (EPERM)
    total += 1;
    let dma_result = DRIVER_SERVICES.lock().register(dev_kbd, test_pid + 1, DRIVER_CAP_DMA);
    if dma_result == Err(crate::syscall::EPERM) {
        passed += 1;
    }

    // Teste 3: Rejeição de conflito por exclusividade (EBUSY)
    total += 1;
    let busy_result = DRIVER_SERVICES.lock().register(dev_kbd, test_pid + 2, DRIVER_CAP_PIO | DRIVER_CAP_INPUT);
    if busy_result == Err(crate::syscall::EBUSY) {
        passed += 1;
    }

    // Teste 4: Enfileiramento de bytes e saturação delimitada (bounded queue)
    total += 1;
    let mut queue_ok = true;
    for i in 0..64u8 {
        if !post_raw_hardware_byte(dev_kbd, 1, i) {
            queue_ok = false;
        }
    }
    // 65º byte deve saturar e incrementar dropped_overflow sem pânico
    let _overflow_post = post_raw_hardware_byte(dev_kbd, 1, 0xFF);
    {
        let reg = DRIVER_SERVICES.lock();
        if let Some(Some(svc)) = reg.services.iter().find(|s| s.is_some() && s.as_ref().unwrap().device_id == dev_kbd) {
            if svc.event_queue.dropped_overflow == 0 {
                queue_ok = false;
            }
        } else {
            queue_ok = false;
        }
    }
    if queue_ok {
        passed += 1;
    }

    // Teste 5: Polling seguro de eventos por PID autorizado
    total += 1;
    let mut poll_buf = [HardwareEvent::empty(); 32];
    let poll_unauth = DRIVER_SERVICES.lock().poll_events(dev_kbd, test_pid + 1, &mut poll_buf);
    let poll_auth = DRIVER_SERVICES.lock().poll_events(dev_kbd, test_pid, &mut poll_buf);
    if poll_unauth == Err(crate::syscall::EPERM) && poll_auth.is_ok() && poll_auth.unwrap() == 32 {
        passed += 1;
    }

    // Teste 6: Envio de entrada normalizada e clamping seguro
    total += 1;
    let input_unauth = DRIVER_SERVICES.lock().dispatch_normalized_input(test_pid + 1, INPUT_KIND_KEY_DOWN, b'A' as u16, 0, 0);
    let input_auth = DRIVER_SERVICES.lock().dispatch_normalized_input(test_pid, INPUT_KIND_KEY_DOWN, b'A' as u16, 0, 0);
    let mouse_auth = DRIVER_SERVICES.lock().dispatch_normalized_input(test_pid, INPUT_KIND_MOUSE_MOVE, 0, 2000, -500);
    if input_unauth == Err(crate::syscall::EPERM) && input_auth.is_ok() && mouse_auth.is_ok() {
        passed += 1;
    }

    // Teste 7: Recuperação de falha e restauração de fallback Ring 0 via notify_driver_pid_exit
    total += 1;
    notify_driver_pid_exit(test_pid);
    if !has_active_driver(dev_kbd) {
        passed += 1;
    }

    // Teste 8: Re-registro após falha incrementa generation counter
    total += 1;
    let rereg_result = DRIVER_SERVICES.lock().register(dev_kbd, test_pid + 10, DRIVER_CAP_PIO | DRIVER_CAP_INPUT);
    if let Ok(new_gen) = rereg_result {
        // Limpa estado para não interferir em boot subsequente
        let _ = DRIVER_SERVICES.lock().unregister(dev_kbd, test_pid + 10);
        if new_gen > 1 && !has_active_driver(dev_kbd) {
            passed += 1;
        }
    }

    (passed, total)
}
