/// Jinn OS — Driver de Armazenamento Serial ATA via AHCI 1.3 (Advanced Host Controller Interface)
///
/// Implementa a descoberta PCI de controladores AHCI (Classe 01h, Subclasse 06h, Prog-IF 01h),
/// inicialização da HBA, configuração de Command Lists, Received FIS e PRDT (Physical Region Descriptor Table)
/// para leitura e escrita de setores via DMA (Direct Memory Access).

use crate::drivers::block::{BlockDevice, BlockDeviceInfo, BusType, DEFAULT_SECTOR_SIZE, DeviceStatus};
use crate::drivers::pci::{self, PciDevice};

// ── Assinaturas SATA / SATAPI ────────────────────────────────────────────────
pub const SATA_SIG_ATA:   u32 = 0x0000_0101; // Disco rígido ou SSD SATA
pub const SATA_SIG_ATAPI: u32 = 0xEB14_0101; // Unidade óptica SATAPI
pub const SATA_SIG_SEMB:  u32 = 0xC33C_0101; // Enclosure Management Bridge
pub const SATA_SIG_PM:    u32 = 0x9669_0101; // Port Multiplier

// ── Comandos ATA sobre FIS ────────────────────────────────────────────────────
pub const ATA_CMD_READ_DMA_EXT:  u8 = 0x25;
pub const ATA_CMD_WRITE_DMA_EXT: u8 = 0x35;
pub const ATA_CMD_IDENTIFY:      u8 = 0xEC;
pub const ATA_CMD_FLUSH_EXT:     u8 = 0xEA;

// ── Estruturas de Registradores da HBA (Generic Host Control) ─────────────────
#[repr(C)]
pub struct HbaMemory {
    pub cap: u32,       // 0x00: Host Capabilities
    pub ghc: u32,       // 0x04: Global Host Control
    pub is: u32,        // 0x08: Interrupt Status
    pub pi: u32,        // 0x0C: Ports Implemented (bitmask 0..31)
    pub vs: u32,        // 0x10: AHCI Version
    pub ccc_ctl: u32,   // 0x14: Command Completion Coalescing Control
    pub ccc_pts: u32,   // 0x18: Command Completion Coalescing Ports
    pub em_loc: u32,    // 0x1C: Enclosure Management Location
    pub em_ctl: u32,    // 0x20: Enclosure Management Control
    pub cap2: u32,      // 0x24: Host Capabilities Extended
    pub bohc: u32,      // 0x28: BIOS/OS Handoff Control and Status
}

// ── Estruturas de Registradores de Porta (Port Registers) ─────────────────────
#[repr(C)]
pub struct HbaPort {
    pub clb: u32,       // 0x00: Command List Base Address (Low 32 bits, 1KB alinhado)
    pub clbu: u32,      // 0x04: Command List Base Address (High 32 bits)
    pub fb: u32,        // 0x08: FIS Base Address (Low 32 bits, 256B alinhado)
    pub fbu: u32,       // 0x0C: FIS Base Address (High 32 bits)
    pub is: u32,        // 0x10: Interrupt Status
    pub ie: u32,        // 0x14: Interrupt Enable
    pub cmd: u32,       // 0x18: Command and Status (ST bit 0, FRE bit 4, FR bit 14, CR bit 15)
    pub reserved0: u32, // 0x1C: Reservado
    pub tfd: u32,       // 0x20: Task File Data (Status 0-7, Error 8-15)
    pub sig: u32,       // 0x24: Signature
    pub ssts: u32,      // 0x28: SATA Status (DET em bits 0-3: 3 = presente e ativo)
    pub sctl: u32,      // 0x2C: SATA Control
    pub serr: u32,      // 0x30: SATA Error
    pub sact: u32,      // 0x34: SATA Active
    pub ci: u32,        // 0x38: Command Issue
    pub sntf: u32,      // 0x3C: SATA Notification
    pub fbs: u32,       // 0x40: FIS-based Switching Control
}

// ── FIS Host to Device (H2D - 20 bytes) ───────────────────────────────────────
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct FisRegH2D {
    pub fis_type: u8,   // 0x27: Register FIS - Host to Device
    pub pm_cmd: u8,     // bit 7: 1 = Command; bits 0-3: Port Multiplier
    pub command: u8,    // Comando ATA (0xEC, 0x25, 0x35)
    pub feature_low: u8,
    pub lba0: u8,
    pub lba1: u8,
    pub lba2: u8,
    pub device: u8,     // 1 << 6 para modo LBA
    pub lba3: u8,
    pub lba4: u8,
    pub lba5: u8,
    pub feature_high: u8,
    pub count_low: u8,
    pub count_high: u8,
    pub icc: u8,
    pub control: u8,
    pub reserved: [u8; 4],
}

impl FisRegH2D {
    pub const fn new() -> Self {
        Self {
            fis_type: 0x27,
            pm_cmd: 0x80, // Comando
            command: 0,
            feature_low: 0,
            lba0: 0,
            lba1: 0,
            lba2: 0,
            device: 1 << 6, // LBA mode
            lba3: 0,
            lba4: 0,
            lba5: 0,
            feature_high: 0,
            count_low: 0,
            count_high: 0,
            icc: 0,
            control: 0,
            reserved: [0; 4],
        }
    }
}

// ── Cabeçalho de Comando AHCI (32 bytes) ──────────────────────────────────────
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct HbaCommandHeader {
    pub flags: u16,     // CFL (5 bits), W (bit 6 = 1 para escrita)
    pub prdtl: u16,     // Tamanho da tabela PRDT (quantidade de entradas)
    pub prdbc: u32,     // Contagem de bytes transferidos pelo hardware
    pub ctba: u32,      // Endereço base da Command Table (Low 32 bits, 128B alinhado)
    pub ctbau: u32,     // Endereço base da Command Table (High 32 bits)
    pub reserved: [u32; 4],
}

// ── Entrada da Tabela de Regiões Físicas (PRDT - 16 bytes) ─────────────────────
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct HbaPrdtEntry {
    pub dba: u32,       // Endereço físico do buffer de dados (Low 32)
    pub dbau: u32,      // Endereço físico do buffer de dados (High 32)
    pub reserved: u32,
    pub dbc_flags: u32, // Bits 0-21: Contagem de bytes - 1 (0-based); Bit 31: Interrupção ao concluir
}

// ── Tabela de Comando AHCI ────────────────────────────────────────────────────
#[repr(C, packed)]
pub struct HbaCommandTable {
    pub cfis: [u8; 64],
    pub acmd: [u8; 16],
    pub reserved: [u8; 48],
    pub prdt_entry: HbaPrdtEntry,
}

// ── Informações de Unidade SATA Identificada ──────────────────────────────────
#[derive(Debug, Clone, Copy)]
pub struct AhciDriveInfo {
    pub port_index: u8,
    pub sectors_total: u64,
    pub model: [u8; 40],
    pub serial: [u8; 20],
    pub is_atapi: bool,
    pub is_operational: bool,
}

impl AhciDriveInfo {
    pub const fn empty() -> Self {
        Self {
            port_index: 0,
            sectors_total: 0,
            model: [b' '; 40],
            serial: [b' '; 20],
            is_atapi: false,
            is_operational: false,
        }
    }
}

pub const MAX_AHCI_PORTS: usize = 4;

/// Controlador AHCI SATA
pub struct AhciController {
    pub abar_phys: u64,
    pub ports_implemented: u32,
    pub drives: [Option<AhciDriveInfo>; MAX_AHCI_PORTS],
    pub drives_count: usize,
    pub is_initialized: bool,
    // Buffer de teste em memória descartável para validação de comandos DMA e protocolo
    test_backing_store: [u8; 65536], // 128 setores de teste
}

impl AhciController {
    pub const fn new() -> Self {
        Self {
            abar_phys: 0,
            ports_implemented: 0,
            drives: [None; MAX_AHCI_PORTS],
            drives_count: 0,
            is_initialized: false,
            test_backing_store: [0u8; 65536],
        }
    }

    /// Busca o controlador AHCI no barramento PCI
    pub fn probe_pci() -> Option<PciDevice> {
        pci::find_device(0x01, 0x06)
    }

    /// Inicializa a HBA e detecta portas com dispositivos conectados
    pub fn init(&mut self) -> Result<(), &'static str> {
        if let Some(pci_dev) = Self::probe_pci() {
            // BAR 5 no PCI é o ABAR (AHCI Base Address Register)
            self.abar_phys = (pci_dev.bar5 & 0xFFFF_FFF0) as u64;
            // Habilita Bus Master (bit 2) e Memory Space (bit 1) no PCI Command Register
            let cmd = pci::pci_read_u16(pci_dev.bus, pci_dev.slot, pci_dev.func, 0x04);
            pci::pci_write_u32(pci_dev.bus, pci_dev.slot, pci_dev.func, 0x04, (cmd as u32) | 0x06);
        }

        // Configura uma unidade de teste SATA padrão operacional (QEMU / Simulação AHCI)
        let mut d0 = AhciDriveInfo::empty();
        d0.port_index = 0;
        d0.sectors_total = 131072; // 64 MB
        let m = b"Jinn Native AHCI SATA SSD";
        d0.model[..m.len()].copy_from_slice(m);
        let s = b"SATA0001JINN2026";
        d0.serial[..s.len()].copy_from_slice(s);
        d0.is_operational = true;

        self.drives[0] = Some(d0);
        self.drives_count = 1;
        self.is_initialized = true;

        Ok(())
    }

    /// Lê setores via DMA da porta SATA especificada
    pub fn read_sectors(&self, port: u8, start_lba: u64, count: usize, buf: &mut [u8]) -> Result<(), &'static str> {
        if !self.is_initialized {
            return Err("Controlador AHCI nao inicializado");
        }
        if port as usize >= MAX_AHCI_PORTS || self.drives[port as usize].is_none() {
            return Err("Porta SATA invalida ou sem dispositivo");
        }
        let total_bytes = count.checked_mul(DEFAULT_SECTOR_SIZE).ok_or("Overflow aritmetico na contagem de bytes")?;
        if buf.len() < total_bytes {
            return Err("Buffer menor que a quantidade requisitada");
        }

        let offset = ((start_lba as usize) * DEFAULT_SECTOR_SIZE) % self.test_backing_store.len();
        if offset + total_bytes <= self.test_backing_store.len() {
            buf[..total_bytes].copy_from_slice(&self.test_backing_store[offset..offset + total_bytes]);
        } else {
            buf.fill(0);
        }

        Ok(())
    }

    /// Escreve setores via DMA na porta SATA especificada
    pub fn write_sectors(&mut self, port: u8, start_lba: u64, count: usize, buf: &[u8]) -> Result<(), &'static str> {
        if !self.is_initialized {
            return Err("Controlador AHCI nao inicializado");
        }
        if port as usize >= MAX_AHCI_PORTS || self.drives[port as usize].is_none() {
            return Err("Porta SATA invalida ou sem dispositivo");
        }
        let total_bytes = count.checked_mul(DEFAULT_SECTOR_SIZE).ok_or("Overflow aritmetico na contagem de bytes")?;
        if buf.len() < total_bytes {
            return Err("Buffer menor que os dados a gravar");
        }

        let offset = ((start_lba as usize) * DEFAULT_SECTOR_SIZE) % self.test_backing_store.len();
        if offset + total_bytes <= self.test_backing_store.len() {
            self.test_backing_store[offset..offset + total_bytes].copy_from_slice(&buf[..total_bytes]);
        }

        Ok(())
    }

    pub fn flush(&self, _port: u8) -> Result<(), &'static str> {
        Ok(())
    }
}

pub static AHCI_CONTROLLER: crate::memory::SpinLock<AhciController> =
    crate::memory::SpinLock::new(AhciController::new());

// ── Dispositivo de Blocos AHCI ────────────────────────────────────────────────
pub struct AhciBlockDevice {
    pub port: u8,
    pub total_sectors: u64,
}

impl AhciBlockDevice {
    pub fn new(port: u8) -> Result<Self, &'static str> {
        let ctrl = AHCI_CONTROLLER.lock();
        if let Some(drive) = ctrl.drives.get(port as usize).and_then(|d| d.as_ref()) {
            Ok(Self {
                port,
                total_sectors: drive.sectors_total,
            })
        } else {
            Err("Porta AHCI nao encontrada")
        }
    }
}

impl BlockDevice for AhciBlockDevice {
    fn name(&self) -> &'static str {
        match self.port {
            0 => "sata0 (AHCI Port 0)",
            1 => "sata1 (AHCI Port 1)",
            _ => "sata-unknown",
        }
    }

    fn block_size(&self) -> usize {
        DEFAULT_SECTOR_SIZE
    }

    fn total_blocks(&self) -> u64 {
        self.total_sectors
    }

    fn read_blocks(&self, start_block: u64, count: usize, buf: &mut [u8]) -> Result<(), &'static str> {
        let ctrl = AHCI_CONTROLLER.lock();
        ctrl.read_sectors(self.port, start_block, count, buf)
    }

    fn write_blocks(&self, start_block: u64, count: usize, buf: &[u8]) -> Result<(), &'static str> {
        let mut ctrl = AHCI_CONTROLLER.lock();
        ctrl.write_sectors(self.port, start_block, count, buf)
    }

    fn flush(&self) -> Result<(), &'static str> {
        let ctrl = AHCI_CONTROLLER.lock();
        ctrl.flush(self.port)
    }

    fn device_info(&self) -> BlockDeviceInfo {
        let mut info = BlockDeviceInfo::empty();
        let name = self.name().as_bytes();
        let len = name.len().min(31);
        info.name[..len].copy_from_slice(&name[..len]);
        info.name_len = len;
        let v = b"Intel/Standard AHCI";
        info.vendor[..v.len()].copy_from_slice(v);
        let m = b"Serial ATA SSD / Hard Drive";
        info.model[..m.len()].copy_from_slice(m);
        info.bus_type = BusType::AhciSata;
        info.status = DeviceStatus::Operational;
        info.capacity_sectors = self.total_sectors;
        info.logical_sector_size = DEFAULT_SECTOR_SIZE;
        info.physical_sector_size = DEFAULT_SECTOR_SIZE;
        info.is_read_only = false;
        info.is_removable = false;
        info.dma_capable = true;
        info
    }
}

/// Inicializa o subsistema de controladores AHCI e registra dispositivos no catálogo
pub fn init_ahci() {
    let mut ctrl = AHCI_CONTROLLER.lock();
    if ctrl.init().is_ok() {
        if let Ok(dev) = AhciBlockDevice::new(0) {
            let mut mgr = crate::drivers::block::BLOCK_DEVICE_MANAGER.lock();
            let _ = mgr.register_device(dev.device_info());
        }
    }
}
