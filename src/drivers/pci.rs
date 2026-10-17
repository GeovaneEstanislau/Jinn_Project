/// Jinn OS — Driver de Enumeração do Barramento PCI
///
/// Implementa a varredura e descoberta de dispositivos no barramento PCI
/// através do Mecanismo de Configuração #1 do x86_64:
///   - Porta de Endereço : 0xCF8 (32 bits)
///   - Porta de Dados    : 0xCFC (32 bits)
///
/// Detecta controladores de armazenamento (IDE, AHCI SATA, NVMe),
/// interfaces de rede (VirtIO, e1000, Realtek) e adaptadores gráficos.

const PCI_CONFIG_ADDRESS: u16 = 0xCF8;
const PCI_CONFIG_DATA:    u16 = 0xCFC;

pub const MAX_PCI_DEVICES: usize = 32;

// ── Funções de I/O de Baixo Nível ─────────────────────────────────────────────

#[inline]
unsafe fn outl(port: u16, val: u32) {
    core::arch::asm!("out dx, eax", in("dx") port, in("eax") val, options(nomem, nostack));
}

#[inline]
unsafe fn inl(port: u16) -> u32 {
    let val: u32;
    core::arch::asm!("in eax, dx", out("eax") val, in("dx") port, options(nomem, nostack));
    val
}

// ── Acesso ao Espaço de Configuração PCI ──────────────────────────────────────

/// Lê um valor de 32 bits do espaço de configuração PCI.
pub fn pci_read_u32(bus: u8, slot: u8, func: u8, offset: u8) -> u32 {
    let address = (1u32 << 31)
        | ((bus as u32) << 16)
        | (((slot & 0x1F) as u32) << 11)
        | (((func & 0x07) as u32) << 8)
        | ((offset as u32) & 0xFC);

    unsafe {
        outl(PCI_CONFIG_ADDRESS, address);
        inl(PCI_CONFIG_DATA)
    }
}

/// Escreve um valor de 32 bits no espaço de configuração PCI.
pub fn pci_write_u32(bus: u8, slot: u8, func: u8, offset: u8, value: u32) {
    let address = (1u32 << 31)
        | ((bus as u32) << 16)
        | (((slot & 0x1F) as u32) << 11)
        | (((func & 0x07) as u32) << 8)
        | ((offset as u32) & 0xFC);

    unsafe {
        outl(PCI_CONFIG_ADDRESS, address);
        outl(PCI_CONFIG_DATA, value);
    }
}

/// Lê uma palavra de 16 bits do espaço de configuração PCI.
pub fn pci_read_u16(bus: u8, slot: u8, func: u8, offset: u8) -> u16 {
    let dword = pci_read_u32(bus, slot, func, offset);
    let shift = (offset & 2) * 8;
    ((dword >> shift) & 0xFFFF) as u16
}

/// Lê um byte de 8 bits do espaço de configuração PCI.
pub fn pci_read_u8(bus: u8, slot: u8, func: u8, offset: u8) -> u8 {
    let dword = pci_read_u32(bus, slot, func, offset);
    let shift = (offset & 3) * 8;
    ((dword >> shift) & 0xFF) as u8
}

// ── Estruturas de Dados ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct PciDevice {
    pub bus: u8,
    pub slot: u8,
    pub func: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub header_type: u8,
    pub bar0: u32,
    pub bar1: u32,
    pub bar2: u32,
    pub bar3: u32,
    pub bar4: u32,
    pub bar5: u32,
    pub irq_line: u8,
}

static mut PCI_DEVICES: [Option<PciDevice>; MAX_PCI_DEVICES] = [None; MAX_PCI_DEVICES];
static mut PCI_DEVICE_COUNT: usize = 0;

// ── Mapeamento Amigável de Classes e Fabricantes ──────────────────────────────

pub fn vendor_name(vendor_id: u16) -> &'static str {
    match vendor_id {
        0x8086 => "Intel Corp.",
        0x1234 => "QEMU / Bochs Virtual Device",
        0x1AF4 => "Red Hat, Inc. (VirtIO)",
        0x80EE => "Oracle VirtualBox",
        0x1022 => "AMD",
        0x10DE => "NVIDIA Corp.",
        0x10EC => "Realtek Semiconductor",
        0x15AD => "VMware",
        0x1000 => "LSI Logic / Broadcom",
        _      => "Desconhecido",
    }
}

pub fn class_name(class: u8, subclass: u8) -> &'static str {
    match (class, subclass) {
        (0x01, 0x01) => "Storage: IDE Controller",
        (0x01, 0x06) => "Storage: SATA Controller (AHCI)",
        (0x01, 0x08) => "Storage: Non-Volatile Memory (NVMe)",
        (0x01, 0x00) => "Storage: SCSI Bus Controller",
        (0x01, _)    => "Storage Controller",

        (0x02, 0x00) => "Network: Ethernet Controller",
        (0x02, 0x80) => "Network: Other Network Controller",
        (0x02, _)    => "Network Controller",

        (0x03, 0x00) => "Display: VGA Compatible Controller",
        (0x03, 0x80) => "Display: Other Display Controller",
        (0x03, _)    => "Display Controller",

        (0x04, 0x01) => "Multimedia: Audio Controller",
        (0x04, 0x03) => "Multimedia: High Definition Audio",
        (0x04, _)    => "Multimedia Device",

        (0x06, 0x00) => "Bridge: Host Bridge",
        (0x06, 0x01) => "Bridge: ISA Bridge",
        (0x06, 0x04) => "Bridge: PCI-to-PCI Bridge",
        (0x06, _)    => "Bridge Device",

        (0x0C, 0x03) => "Serial Bus: USB Controller",
        (0x0C, 0x05) => "Serial Bus: SMBus Controller",
        (0x0C, _)    => "Serial Bus Controller",

        _ => "Outro Dispositivo",
    }
}

// ── Varredura do Barramento ───────────────────────────────────────────────────

fn check_device(bus: u8, slot: u8) {
    let vendor_id = pci_read_u16(bus, slot, 0, 0);
    if vendor_id == 0xFFFF || vendor_id == 0x0000 {
        return; // Dispositivo não conectado neste slot
    }

    let header_type = pci_read_u8(bus, slot, 0, 0x0E);
    let max_funcs = if (header_type & 0x80) != 0 { 8 } else { 1 };

    for func in 0..max_funcs {
        let v_id = pci_read_u16(bus, slot, func, 0);
        if v_id == 0xFFFF || v_id == 0x0000 {
            continue;
        }

        let device_id = pci_read_u16(bus, slot, func, 2);
        let class_rev = pci_read_u32(bus, slot, func, 8);
        let prog_if    = ((class_rev >> 8) & 0xFF) as u8;
        let subclass   = ((class_rev >> 16) & 0xFF) as u8;
        let class_code = ((class_rev >> 24) & 0xFF) as u8;

        let bar0 = pci_read_u32(bus, slot, func, 0x10);
        let bar1 = pci_read_u32(bus, slot, func, 0x14);
        let bar2 = pci_read_u32(bus, slot, func, 0x18);
        let bar3 = pci_read_u32(bus, slot, func, 0x1C);
        let bar4 = pci_read_u32(bus, slot, func, 0x20);
        let bar5 = pci_read_u32(bus, slot, func, 0x24);

        let irq_line = pci_read_u8(bus, slot, func, 0x3C);

        unsafe {
            let count_ptr = core::ptr::addr_of_mut!(PCI_DEVICE_COUNT);
            let dev_ptr = core::ptr::addr_of_mut!(PCI_DEVICES);
            if *count_ptr < MAX_PCI_DEVICES {
                (*dev_ptr)[*count_ptr] = Some(PciDevice {
                    bus,
                    slot,
                    func,
                    vendor_id: v_id,
                    device_id,
                    class_code,
                    subclass,
                    prog_if,
                    header_type: header_type & 0x7F,
                    bar0,
                    bar1,
                    bar2,
                    bar3,
                    bar4,
                    bar5,
                    irq_line,
                });
                *count_ptr += 1;
            }
        }
    }
}

/// Executa a varredura completa do barramento PCI (buses 0..8).
pub fn scan() {
    unsafe {
        let count_ptr = core::ptr::addr_of_mut!(PCI_DEVICE_COUNT);
        *count_ptr = 0;
        let dev_ptr = core::ptr::addr_of_mut!(PCI_DEVICES);
        for i in 0..MAX_PCI_DEVICES {
            (*dev_ptr)[i] = None;
        }

        // Varre os barramentos mais comuns (0..8)
        for bus in 0..8 {
            for slot in 0..32 {
                check_device(bus, slot);
            }
        }
    }
}

/// Retorna a lista de dispositivos PCI encontrados no sistema.
pub fn devices() -> &'static [Option<PciDevice>] {
    unsafe {
        let dev_ptr = core::ptr::addr_of!(PCI_DEVICES) as *const Option<PciDevice>;
        let count = *core::ptr::addr_of!(PCI_DEVICE_COUNT);
        core::slice::from_raw_parts(dev_ptr, count)
    }
}

/// Retorna o número total de dispositivos detectados no barramento PCI.
pub fn device_count() -> usize {
    unsafe { *core::ptr::addr_of!(PCI_DEVICE_COUNT) }
}

/// Procura por um dispositivo PCI pela classe e subclasse.
pub fn find_device(class: u8, subclass: u8) -> Option<PciDevice> {
    unsafe {
        let count = *core::ptr::addr_of!(PCI_DEVICE_COUNT);
        let dev_ptr = core::ptr::addr_of!(PCI_DEVICES);
        for i in 0..count {
            if let Some(dev) = (*dev_ptr)[i] {
                if dev.class_code == class && dev.subclass == subclass {
                    return Some(dev);
                }
            }
        }
    }
    None
}

/// Inicializa o subsistema PCI e faz a descoberta inicial dos dispositivos.
pub fn init() {
    scan();
}
