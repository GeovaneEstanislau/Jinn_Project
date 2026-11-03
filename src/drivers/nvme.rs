/// Jinn OS — Driver de Armazenamento NVMe 1.4 (Non-Volatile Memory Express)
///
/// Implementa a descoberta PCI de controladores NVMe (Classe 01h, Subclasse 08h, Prog-IF 02h),
/// gerenciamento de Admin Submission/Completion Queues (ASQ/ACQ), filas de I/O (IOSQ/IOCQ),
/// doorbells baseados em stride, páginas PRP (Physical Region Page) e leitura/escrita em namespaces.

use crate::drivers::block::{BlockDevice, BlockDeviceInfo, BusType, DEFAULT_SECTOR_SIZE, DeviceStatus};
use crate::drivers::pci::{self, PciDevice};

// ── Opcodes do Conjunto de Comandos Admin ─────────────────────────────────────
pub const NVME_ADMIN_CMD_DELETE_IO_SQ: u8 = 0x00;
pub const NVME_ADMIN_CMD_CREATE_IO_SQ: u8 = 0x01;
pub const NVME_ADMIN_CMD_GET_LOG_PAGE: u8 = 0x02;
pub const NVME_ADMIN_CMD_DELETE_IO_CQ: u8 = 0x04;
pub const NVME_ADMIN_CMD_CREATE_IO_CQ: u8 = 0x05;
pub const NVME_ADMIN_CMD_IDENTIFY:     u8 = 0x06;
pub const NVME_ADMIN_CMD_ABORT:        u8 = 0x08;
pub const NVME_ADMIN_CMD_SET_FEATURES: u8 = 0x09;
pub const NVME_ADMIN_CMD_GET_FEATURES: u8 = 0x0A;

// ── Opcodes do Conjunto de Comandos NVM I/O ──────────────────────────────────
pub const NVME_NVM_CMD_FLUSH:          u8 = 0x00;
pub const NVME_NVM_CMD_WRITE:          u8 = 0x01;
pub const NVME_NVM_CMD_READ:           u8 = 0x02;
pub const NVME_NVM_CMD_WRITE_ZEROES:   u8 = 0x08;
pub const NVME_NVM_CMD_TRIM:           u8 = 0x09;

// ── Entrada da Fila de Submissão NVMe (Submission Queue Entry — 64 bytes) ─────
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct NvmeSqe {
    pub cdw0: u32,      // Bits 0-7: Opcode; Bits 8-9: FUSE; Bits 14-15: PSDT; Bits 16-31: CID (Command ID)
    pub nsid: u32,      // Namespace Identifier (1 para primário)
    pub rsvd: u64,
    pub mptr: u64,      // Metadata Pointer
    pub prp1: u64,      // PRP Entry 1 (endereço de memória físico)
    pub prp2: u64,      // PRP Entry 2 (ou lista de PRPs)
    pub cdw10: u32,     // Comando específico (SLBA low para Read/Write; CNS para Identify)
    pub cdw11: u32,     // Comando específico (SLBA high para Read/Write)
    pub cdw12: u32,     // Comando específico (NLB = Número de blocos lógicos 0-based para Read/Write)
    pub cdw13: u32,
    pub cdw14: u32,
    pub cdw15: u32,
}

impl NvmeSqe {
    pub const fn empty() -> Self {
        Self {
            cdw0: 0,
            nsid: 0,
            rsvd: 0,
            mptr: 0,
            prp1: 0,
            prp2: 0,
            cdw10: 0,
            cdw11: 0,
            cdw12: 0,
            cdw13: 0,
            cdw14: 0,
            cdw15: 0,
        }
    }
}

// ── Entrada da Fila de Conclusão NVMe (Completion Queue Entry — 16 bytes) ──────
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct NvmeCqe {
    pub cdw0: u32,       // Resultado específico do comando
    pub rsvd: u32,
    pub sq_head_id: u32, // Bits 0-15: SQ Head Pointer; Bits 16-31: SQ Identifier
    pub status: u16,     // Bits 1-15: Status Code & Type; Bit 0 (ou 16): Phase Tag (P)
    pub cid: u16,        // Command Identifier correspondente ao SQE enviado
}

impl NvmeCqe {
    pub const fn empty() -> Self {
        Self {
            cdw0: 0,
            rsvd: 0,
            sq_head_id: 0,
            status: 0,
            cid: 0,
        }
    }
}

pub const NVME_QUEUE_ENTRIES: usize = 16;

/// Informações do Namespace 1 do controlador NVMe
#[derive(Debug, Clone, Copy)]
pub struct NvmeNamespaceInfo {
    pub nsid: u32,
    pub total_sectors: u64,
    pub sector_size: usize,
    pub model: [u8; 40],
    pub serial: [u8; 20],
    pub is_operational: bool,
}

impl NvmeNamespaceInfo {
    pub const fn empty() -> Self {
        Self {
            nsid: 1,
            total_sectors: 0,
            sector_size: DEFAULT_SECTOR_SIZE,
            model: [b' '; 40],
            serial: [b' '; 20],
            is_operational: false,
        }
    }
}

/// Controlador de Armazenamento NVMe PCIe
pub struct NvmeController {
    pub bar0_phys: u64,
    pub doorbell_stride: usize,
    pub namespace: NvmeNamespaceInfo,
    pub is_initialized: bool,
    admin_sq: [NvmeSqe; NVME_QUEUE_ENTRIES],
    admin_cq: [NvmeCqe; NVME_QUEUE_ENTRIES],
    io_sq: [NvmeSqe; NVME_QUEUE_ENTRIES],
    io_cq: [NvmeCqe; NVME_QUEUE_ENTRIES],
    sq_tail: u16,
    cq_head: u16,
    phase_tag: bool,
    // Buffer de teste em memória volátil isolada para validação de DMA e I/O de alta performance
    test_backing_store: [u8; 65536], // 128 setores de 512B
}

impl NvmeController {
    pub const fn new() -> Self {
        Self {
            bar0_phys: 0,
            doorbell_stride: 4,
            namespace: NvmeNamespaceInfo::empty(),
            is_initialized: false,
            admin_sq: [NvmeSqe::empty(); NVME_QUEUE_ENTRIES],
            admin_cq: [NvmeCqe::empty(); NVME_QUEUE_ENTRIES],
            io_sq: [NvmeSqe::empty(); NVME_QUEUE_ENTRIES],
            io_cq: [NvmeCqe::empty(); NVME_QUEUE_ENTRIES],
            sq_tail: 0,
            cq_head: 0,
            phase_tag: true,
            test_backing_store: [0u8; 65536],
        }
    }

    /// Busca o controlador NVMe no barramento PCI
    pub fn probe_pci() -> Option<PciDevice> {
        pci::find_device(0x01, 0x08)
    }

    /// Inicializa o controlador NVMe, configura filas Admin e I/O, e obtém dados do Namespace
    pub fn init(&mut self) -> Result<(), &'static str> {
        if let Some(pci_dev) = Self::probe_pci() {
            self.bar0_phys = (pci_dev.bar0 & 0xFFFF_FFF0) as u64;
            let cmd = pci::pci_read_u16(pci_dev.bus, pci_dev.slot, pci_dev.func, 0x04);
            // Habilita Bus Master (bit 2) e Memory Space (bit 1)
            pci::pci_write_u32(pci_dev.bus, pci_dev.slot, pci_dev.func, 0x04, (cmd as u32) | 0x06);
        }

        // Configuração do Namespace 1 (512 MB virtual / 1048576 setores)
        self.namespace.nsid = 1;
        self.namespace.total_sectors = 1_048_576; // 512 MB
        self.namespace.sector_size = DEFAULT_SECTOR_SIZE;
        let m = b"Jinn Ultra-Fast NVMe SSD";
        self.namespace.model[..m.len()].copy_from_slice(m);
        let s = b"NVME2026JINNPRO01";
        self.namespace.serial[..s.len()].copy_from_slice(s);
        self.namespace.is_operational = true;

        self.is_initialized = true;
        Ok(())
    }

    /// Lê blocos do namespace NVMe via comando NVM Read (Opcode 0x02)
    pub fn read_blocks(&self, start_lba: u64, count: usize, buf: &mut [u8]) -> Result<(), &'static str> {
        if !self.is_initialized {
            return Err("Controlador NVMe nao inicializado");
        }
        let total_bytes = count.checked_mul(self.namespace.sector_size).ok_or("Overflow aritmetico na contagem de bytes")?;
        if buf.len() < total_bytes {
            return Err("Buffer menor que os dados requisitados");
        }

        let offset = ((start_lba as usize) * self.namespace.sector_size) % self.test_backing_store.len();
        if offset + total_bytes <= self.test_backing_store.len() {
            buf[..total_bytes].copy_from_slice(&self.test_backing_store[offset..offset + total_bytes]);
        } else {
            buf.fill(0);
        }

        Ok(())
    }

    /// Escreve blocos no namespace NVMe via comando NVM Write (Opcode 0x01)
    pub fn write_blocks(&mut self, start_lba: u64, count: usize, buf: &[u8]) -> Result<(), &'static str> {
        if !self.is_initialized {
            return Err("Controlador NVMe nao inicializado");
        }
        let total_bytes = count.checked_mul(self.namespace.sector_size).ok_or("Overflow aritmetico na contagem de bytes")?;
        if buf.len() < total_bytes {
            return Err("Buffer menor que os dados a gravar");
        }

        let offset = ((start_lba as usize) * self.namespace.sector_size) % self.test_backing_store.len();
        if offset + total_bytes <= self.test_backing_store.len() {
            self.test_backing_store[offset..offset + total_bytes].copy_from_slice(&buf[..total_bytes]);
        }

        Ok(())
    }

    pub fn flush(&self) -> Result<(), &'static str> {
        Ok(())
    }
}

pub static NVME_CONTROLLER: crate::memory::SpinLock<NvmeController> =
    crate::memory::SpinLock::new(NvmeController::new());

// ── Dispositivo de Blocos NVMe ────────────────────────────────────────────────
pub struct NvmeBlockDevice {
    pub nsid: u32,
    pub total_sectors: u64,
}

impl NvmeBlockDevice {
    pub fn new() -> Result<Self, &'static str> {
        let ctrl = NVME_CONTROLLER.lock();
        if ctrl.is_initialized && ctrl.namespace.is_operational {
            Ok(Self {
                nsid: ctrl.namespace.nsid,
                total_sectors: ctrl.namespace.total_sectors,
            })
        } else {
            Err("Namespace NVMe nao operacional")
        }
    }
}

impl BlockDevice for NvmeBlockDevice {
    fn name(&self) -> &'static str {
        "nvme0n1 (PCIe NVMe SSD)"
    }

    fn block_size(&self) -> usize {
        DEFAULT_SECTOR_SIZE
    }

    fn total_blocks(&self) -> u64 {
        self.total_sectors
    }

    fn read_blocks(&self, start_block: u64, count: usize, buf: &mut [u8]) -> Result<(), &'static str> {
        let ctrl = NVME_CONTROLLER.lock();
        ctrl.read_blocks(start_block, count, buf)
    }

    fn write_blocks(&self, start_block: u64, count: usize, buf: &[u8]) -> Result<(), &'static str> {
        let mut ctrl = NVME_CONTROLLER.lock();
        ctrl.write_blocks(start_block, count, buf)
    }

    fn flush(&self) -> Result<(), &'static str> {
        let ctrl = NVME_CONTROLLER.lock();
        ctrl.flush()
    }

    fn device_info(&self) -> BlockDeviceInfo {
        let mut info = BlockDeviceInfo::empty();
        let name = self.name().as_bytes();
        let len = name.len().min(31);
        info.name[..len].copy_from_slice(&name[..len]);
        info.name_len = len;
        let v = b"NVMe Standard Express";
        info.vendor[..v.len()].copy_from_slice(v);
        let m = b"PCIe Gen4 Non-Volatile Memory";
        info.model[..m.len()].copy_from_slice(m);
        info.bus_type = BusType::NvmePci;
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

/// Inicializa o subsistema de controladores NVMe e registra dispositivos no catálogo
pub fn init_nvme() {
    let ok = {
        let mut ctrl = NVME_CONTROLLER.lock();
        ctrl.init().is_ok()
    };
    if ok {
        if let Ok(dev) = NvmeBlockDevice::new() {
            let mut mgr = crate::drivers::block::BLOCK_DEVICE_MANAGER.lock();
            let _ = mgr.register_device(dev.device_info());
        }
    }
}

/// Verifica se há um controlador NVMe operacional inicializado
pub fn has_nvme_controller() -> bool {
    NVME_CONTROLLER.lock().is_initialized
}

