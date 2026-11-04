//! Jinn OS — Subsistema Unificado de Controladores de Rede (NIC)
//!
//! Gerencia adaptadores de rede físicos e virtuais:
//! - VirtIO-Net (QEMU / KVM)
//! - Intel 82540EM / 82545EM / PRO/1000 (Oracle VirtualBox / VMware / QEMU)

pub mod e1000;
pub mod virtio;

pub use e1000::E1000Device;
pub use virtio::VirtioNetDevice;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NicType {
    None,
    Virtio,
    E1000,
}

/// Diagnóstico e classificação dos 5 estados de adaptadores de rede
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkState {
    /// 1. Dispositivo inexistente no barramento PCI
    NoPciDevice,
    /// 2. Dispositivo detectado no barramento PCI, mas sem driver correspondente
    UnsupportedPciDevice { vendor_id: u16, device_id: u16 },
    /// 3. Driver carregado, mas sem dispositivo operacional (falha de hardware/MMIO)
    DriverInitFailed,
    /// 4. Dispositivo operacional, mas sem configuração de rede (DHCP pendente/falhou)
    Unconfigured,
    /// 5. Interface configurada com IP, mas sem conectividade (gateway inacessível)
    NoConnectivity,
    /// 6. Interface ativa e operacional
    Operational,
}

impl NetworkState {
    pub const fn description(&self) -> &'static str {
        match self {
            Self::NoPciDevice => "PCI: Sem placa de rede",
            Self::UnsupportedPciDevice { .. } => "PCI: Placa sem driver",
            Self::DriverInitFailed => "Driver: Falha ao inicializar",
            Self::Unconfigured => "Rede: Sem IP (DHCP pendente)",
            Self::NoConnectivity => "Rede: Gateway inacessivel",
            Self::Operational => "Rede: Ativa e operacional",
        }
    }

    pub const fn code(&self) -> u8 {
        match self {
            Self::NoPciDevice => 1,
            Self::UnsupportedPciDevice { .. } => 2,
            Self::DriverInitFailed => 3,
            Self::Unconfigured => 4,
            Self::NoConnectivity => 5,
            Self::Operational => 6,
        }
    }
}

pub fn active_nic_name() -> &'static str {
    unsafe {
        match DETECTED_NIC_TYPE {
            NicType::Virtio => "VirtIO-Net",
            NicType::E1000 => "Intel 82540EM",
            NicType::None => "Nenhum / Loopback",
        }
    }
}

pub fn audit_network_state() -> NetworkState {
    let dev_avail = get_device().map(|d| d.is_available()).unwrap_or(false);
    if dev_avail {
        let is_bound = unsafe {
            match crate::net::dhcp::DHCP_CLIENT.state {
                crate::net::dhcp::DhcpState::Bound(_) => true,
                _ => false,
            }
        };
        if !is_bound {
            return NetworkState::Unconfigured;
        }
        return NetworkState::Operational;
    }

    let mut found_net_pci = None;
    let dev_count = crate::drivers::pci::device_count();
    let dev_slice = crate::drivers::pci::devices();
    for i in 0..dev_count {
        if let Some(dev) = dev_slice.get(i).copied().flatten() {
            if dev.class_code == 0x02 {
                found_net_pci = Some((dev.vendor_id, dev.device_id));
                break;
            }
        }
    }

    match found_net_pci {
        None => NetworkState::NoPciDevice,
        Some((vendor, device)) => {
            let is_virtio = vendor == 0x1AF4 && (device == 0x1000 || device == 0x1041);
            let is_e1000 = vendor == 0x8086 && [0x100E, 0x1004, 0x100F, 0x1019, 0x101E, 0x10D3, 0x1079, 0x107D].contains(&device);
            if is_virtio || is_e1000 {
                NetworkState::DriverInitFailed
            } else {
                NetworkState::UnsupportedPciDevice { vendor_id: vendor, device_id: device }
            }
        }
    }
}

pub static mut DETECTED_NIC_TYPE: NicType = NicType::None;

/// Inicializa e descobre adaptadores de rede no barramento PCI.
/// Tenta VirtIO-Net primeiro; se ausente, tenta Intel e1000 (VirtualBox / VMware).
pub fn init() -> bool {
    if virtio::init() {
        unsafe { DETECTED_NIC_TYPE = NicType::Virtio; }
        return true;
    }
    if e1000::init() {
        unsafe { DETECTED_NIC_TYPE = NicType::E1000; }
        return true;
    }
    unsafe { DETECTED_NIC_TYPE = NicType::None; }
    false
}

pub struct ActiveNicWrapper;

impl ActiveNicWrapper {
    pub fn mac_address(&self) -> [u8; 6] {
        unsafe {
            match DETECTED_NIC_TYPE {
                NicType::Virtio => virtio::get_device().map(|d| d.mac_address()).unwrap_or([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]),
                NicType::E1000 => e1000::get_device().map(|d| d.mac_address()).unwrap_or([0x08, 0x00, 0x27, 0x12, 0x34, 0x56]),
                NicType::None => [0x52, 0x54, 0x00, 0x12, 0x34, 0x56],
            }
        }
    }

    pub fn is_available(&self) -> bool {
        unsafe {
            match DETECTED_NIC_TYPE {
                NicType::Virtio => virtio::get_device().map(|d| d.is_available).unwrap_or(false),
                NicType::E1000 => e1000::get_device().map(|d| d.is_available).unwrap_or(false),
                NicType::None => false,
            }
        }
    }

    pub fn transmit(&mut self, frame: &[u8]) -> Result<(), ()> {
        unsafe {
            match DETECTED_NIC_TYPE {
                NicType::Virtio => virtio::get_device().map(|d| d.transmit(frame)).unwrap_or(Err(())),
                NicType::E1000 => e1000::get_device().map(|d| d.transmit(frame)).unwrap_or(Err(())),
                NicType::None => Err(()),
            }
        }
    }

    pub fn receive(&mut self, dest: &mut [u8]) -> Option<usize> {
        unsafe {
            match DETECTED_NIC_TYPE {
                NicType::Virtio => virtio::get_device().and_then(|d| d.receive(dest)),
                NicType::E1000 => e1000::get_device().and_then(|d| d.receive(dest)),
                NicType::None => None,
            }
        }
    }
}

static mut ACTIVE_NIC: ActiveNicWrapper = ActiveNicWrapper;

pub fn get_device() -> Option<&'static mut ActiveNicWrapper> {
    unsafe {
        Some(&mut ACTIVE_NIC)
    }
}
