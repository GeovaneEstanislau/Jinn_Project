/// Jinn OS — Testes Unitários e de Conformidade do Unified Device Manager (Fase 2)
///
/// Valida registro de dispositivos, máquina de estados, resolução por ID/nome,
/// controle de limites e integridade de estruturas VirtIO-Block.

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceState {
    Discovered  = 1,
    Initialized = 2,
    Operational = 3,
    Suspended   = 4,
    Failed      = 5,
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

#[derive(Debug, Clone, Copy)]
pub struct SimDeviceDescriptor {
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

impl SimDeviceDescriptor {
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
        let mut d = Self {
            id,
            category,
            state,
            vendor_id,
            device_id,
            name: [0; 32],
            name_len: 0,
            bus_info: [0; 32],
            bus_info_len: 0,
            capabilities,
        };
        let nb = name_str.as_bytes();
        let nl = nb.len().min(31);
        d.name[..nl].copy_from_slice(&nb[..nl]);
        d.name_len = nl as u32;

        let bb = bus_str.as_bytes();
        let bl = bb.len().min(31);
        d.bus_info[..bl].copy_from_slice(&bb[..bl]);
        d.bus_info_len = bl as u32;
        d
    }

    pub fn name_str(&self) -> &str {
        std::str::from_utf8(&self.name[..self.name_len as usize]).unwrap_or("unknown")
    }

    pub fn bus_info_str(&self) -> &str {
        std::str::from_utf8(&self.bus_info[..self.bus_info_len as usize]).unwrap_or("none")
    }
}

pub const MAX_SIM_DEVICES: usize = 32;

pub struct SimDeviceRegistry {
    pub devices: [Option<SimDeviceDescriptor>; MAX_SIM_DEVICES],
    pub count: usize,
}

impl SimDeviceRegistry {
    pub fn new() -> Self {
        Self {
            devices: [None; MAX_SIM_DEVICES],
            count: 0,
        }
    }

    pub fn register(&mut self, mut desc: SimDeviceDescriptor) -> Result<usize, &'static str> {
        if self.count >= MAX_SIM_DEVICES {
            return Err("capacidade maxima excedida");
        }
        for slot in self.devices.iter().flatten() {
            if slot.name_str() == desc.name_str() && slot.bus_info_str() == desc.bus_info_str() {
                return Ok(slot.id as usize);
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
        Err("sem slots disponiveis")
    }

    pub fn get_by_index(&self, index: usize) -> Option<SimDeviceDescriptor> {
        if index < MAX_SIM_DEVICES { self.devices[index] } else { None }
    }

    pub fn find_by_name(&self, name: &str) -> Option<SimDeviceDescriptor> {
        for slot in self.devices.iter().flatten() {
            if slot.name_str() == name {
                return Some(*slot);
            }
        }
        None
    }

    pub fn update_state(&mut self, id: usize, state: DeviceState) -> bool {
        for slot in self.devices.iter_mut() {
            if let Some(desc) = slot {
                if desc.id as usize == id {
                    desc.state = state;
                    return true;
                }
            }
        }
        false
    }
}

#[test]
fn test_device_registry_registration_and_lookup() {
    let mut reg = SimDeviceRegistry::new();
    let id0 = reg.register(SimDeviceDescriptor::new(
        0, DeviceCategory::Bus, DeviceState::Operational, 0x8086, 0x1237, "pci_bus0", "PCI Host", 1
    )).expect("Deveria registrar PCI Host");
    assert_eq!(id0, 0);

    let id1 = reg.register(SimDeviceDescriptor::new(
        0, DeviceCategory::Storage, DeviceState::Operational, 0x8086, 0x7010, "ide0", "Port 0x1F0", 3
    )).expect("Deveria registrar IDE0");
    assert_eq!(id1, 1);

    let id2 = reg.register(SimDeviceDescriptor::new(
        0, DeviceCategory::Network, DeviceState::Operational, 0x8086, 0x100E, "net0_e1000", "Intel 82540EM", 7
    )).expect("Deveria registrar e1000");
    assert_eq!(id2, 2);

    assert_eq!(reg.count, 3);
    assert_eq!(reg.find_by_name("ide0").unwrap().vendor_id, 0x8086);
    assert_eq!(reg.find_by_name("net0_e1000").unwrap().device_id, 0x100E);
    assert!(reg.find_by_name("inexistente").is_none());
}

#[test]
fn test_device_registry_duplicate_idempotency() {
    let mut reg = SimDeviceRegistry::new();
    let id1 = reg.register(SimDeviceDescriptor::new(
        0, DeviceCategory::Display, DeviceState::Operational, 0x1234, 0x1111, "fb0", "Limine GOP", 0xF
    )).unwrap();

    // Registra o mesmo dispositivo novamente
    let id2 = reg.register(SimDeviceDescriptor::new(
        0, DeviceCategory::Display, DeviceState::Operational, 0x1234, 0x1111, "fb0", "Limine GOP", 0xF
    )).unwrap();

    assert_eq!(id1, id2);
    assert_eq!(reg.count, 1, "Dispositivo identico nao deve duplicar slot");
}

#[test]
fn test_device_registry_state_transition() {
    let mut reg = SimDeviceRegistry::new();
    let id = reg.register(SimDeviceDescriptor::new(
        0, DeviceCategory::Storage, DeviceState::Discovered, 0x1AF4, 0x1001, "virtio_blk0", "PCI 00:04.0", 5
    )).unwrap();

    assert_eq!(reg.get_by_index(id).unwrap().state, DeviceState::Discovered);

    // Transição: Inicializado
    assert!(reg.update_state(id, DeviceState::Initialized));
    assert_eq!(reg.get_by_index(id).unwrap().state, DeviceState::Initialized);

    // Transição: Operacional
    assert!(reg.update_state(id, DeviceState::Operational));
    assert_eq!(reg.get_by_index(id).unwrap().state, DeviceState::Operational);

    // Transição: Falha
    assert!(reg.update_state(id, DeviceState::Failed));
    assert_eq!(reg.get_by_index(id).unwrap().state, DeviceState::Failed);
}

#[test]
fn test_virtio_block_request_packet_invariants() {
    #[repr(C, packed)]
    struct VirtioBlkReq {
        req_type: u32,
        reserved: u32,
        sector: u64,
    }

    assert_eq!(std::mem::size_of::<VirtioBlkReq>(), 16, "Cabecalho VirtIO Block deve ter exatamente 16 bytes");

    let req = VirtioBlkReq {
        req_type: 0, // VIRTIO_BLK_T_IN
        reserved: 0,
        sector: 2048,
    };

    let rtype = { req.req_type };
    let sec = { req.sector };
    assert_eq!(rtype, 0);
    assert_eq!(sec, 2048);
}

#[test]
fn test_device_summary_generation() {
    let mut reg = SimDeviceRegistry::new();
    reg.register(SimDeviceDescriptor::new(
        0, DeviceCategory::Bus, DeviceState::Operational, 0x8086, 0x1237, "pci_bus0", "PCI Host", 1
    )).unwrap();
    reg.register(SimDeviceDescriptor::new(
        0, DeviceCategory::Storage, DeviceState::Operational, 0x8086, 0x7010, "ide0", "Port 0x1F0", 3
    )).unwrap();
    reg.register(SimDeviceDescriptor::new(
        0, DeviceCategory::Network, DeviceState::Operational, 0x8086, 0x100E, "net0_e1000", "PCI 00:03.0", 7
    )).unwrap();

    assert_eq!(reg.count, 3);
    for i in 0..reg.count {
        let dev = reg.get_by_index(i).unwrap();
        assert!(!dev.name_str().is_empty());
        assert!(!dev.category.name().is_empty());
        assert_eq!(dev.state, DeviceState::Operational);
    }
}
