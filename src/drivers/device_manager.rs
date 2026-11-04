/// Jinn OS — Unified Device Manager (L1 / L2)
///
/// Gerenciador unificado de dispositivos do sistema operacional.
/// Responsável pelo inventário de hardware, rastreamento de ciclo de vida (descoberta,
/// inicialização, prontidão operacional, suspensão e falha) e desacoplamento do kernel
/// de inicializações estáticas.

use core::sync::atomic::{AtomicUsize, Ordering};

pub const MAX_DEVICES: usize = 32;

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
    reg.register(DeviceDescriptor::new(
        0,
        DeviceCategory::Input,
        DeviceState::Operational,
        0x0000,
        0x0001,
        "kbd0",
        "PS/2 Keyboard (Port 0x60)",
        0x0003,
    )).ok();

    reg.register(DeviceDescriptor::new(
        0,
        DeviceCategory::Input,
        DeviceState::Operational,
        0x0000,
        0x0002,
        "mouse0",
        "PS/2 Mouse (IRQ 12)",
        0x0003,
    )).ok();

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
