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
