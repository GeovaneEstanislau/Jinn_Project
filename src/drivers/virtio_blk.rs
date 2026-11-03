/// Jinn OS — Driver de Armazenamento VirtIO-Block (virtio-blk)
///
/// Implementa suporte nativo a discos virtuais VirtIO PCI (Vendor ID 0x1AF4, Device ID 0x1001)
/// em conformidade com o padrão VirtIO 0.9.5 / 1.0 legado.
/// Permite transferência de blocos via anéis VirtQueue de alto desempenho em QEMU / KVM.

use crate::drivers::block::{BlockDevice, BlockDeviceInfo, BusType, DEFAULT_SECTOR_SIZE, DeviceStatus};
use crate::drivers::pci::{self, PciDevice};
use crate::memory::SpinLock;

pub const PCI_VENDOR_VIRTIO: u16 = 0x1AF4;
pub const PCI_DEVICE_VIRTIO_BLK: u16 = 0x1001;

// ── Registradores VirtIO PCI Legado (Baseados em BAR0 I/O) ────────────────────
const VIRTIO_PCI_QUEUE_PFN:      u16 = 0x08;
const VIRTIO_PCI_QUEUE_NUM:      u16 = 0x0C;
const VIRTIO_PCI_QUEUE_SEL:      u16 = 0x0E;
const VIRTIO_PCI_QUEUE_NOTIFY:   u16 = 0x10;
const VIRTIO_PCI_STATUS:         u16 = 0x12;

// ── Flags de Status do Dispositivo ────────────────────────────────────────────
const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 1;
const VIRTIO_STATUS_DRIVER:      u8 = 2;
const VIRTIO_STATUS_DRIVER_OK:   u8 = 4;

// ── Flags de Descritores de Fila VirtQueue ────────────────────────────────────
const VRING_DESC_F_NEXT:  u16 = 1;
const VRING_DESC_F_WRITE: u16 = 2;

// ── Tipos de Requisição VirtIO-Block ──────────────────────────────────────────
pub const VIRTIO_BLK_T_IN:    u32 = 0; // Leitura (Device escreve no buffer do Guest)
pub const VIRTIO_BLK_T_OUT:   u32 = 1; // Escrita (Guest escreve no buffer do Device)
pub const VIRTIO_BLK_T_FLUSH: u32 = 4; // Flush de cache do disco

pub const VIRTIO_BLK_S_OK:     u8 = 0;

pub const VIRTIO_BLK_QUEUE_SIZE: usize = 16;

#[repr(C, align(16))]
#[derive(Debug, Clone, Copy)]
pub struct VRingDesc {
    pub addr: u64,
    pub len: u32,
    pub flags: u16,
    pub next: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VRingAvail {
    pub flags: u16,
    pub idx: u16,
    pub ring: [u16; VIRTIO_BLK_QUEUE_SIZE],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VRingUsedElem {
    pub id: u32,
    pub len: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VRingUsed {
    pub flags: u16,
    pub idx: u16,
    pub ring: [VRingUsedElem; VIRTIO_BLK_QUEUE_SIZE],
}

/// Cabeçalho de requisição VirtIO-Block (16 bytes)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct VirtioBlkReq {
    pub req_type: u32,
    pub reserved: u32,
    pub sector: u64,
}

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    core::arch::asm!("in al, dx", in("dx") port, out("al") val, options(nomem, nostack, preserves_flags));
    val
}

#[inline]
unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
}

#[inline]
unsafe fn inw(port: u16) -> u16 {
    let val: u16;
    core::arch::asm!("in ax, dx", in("dx") port, out("ax") val, options(nomem, nostack, preserves_flags));
    val
}

#[inline]
unsafe fn outw(port: u16, val: u16) {
    core::arch::asm!("out dx, ax", in("dx") port, in("ax") val, options(nomem, nostack, preserves_flags));
}

#[inline]
unsafe fn outl(port: u16, val: u32) {
    core::arch::asm!("out dx, eax", in("dx") port, in("eax") val, options(nomem, nostack, preserves_flags));
}

pub struct VirtioBlkDevice {
    pub io_base: u16,
    pub capacity_sectors: usize,
    pub is_initialized: bool,
    desc_table: [VRingDesc; VIRTIO_BLK_QUEUE_SIZE],
    avail_ring: VRingAvail,
    used_ring: VRingUsed,
    req_header: VirtioBlkReq,
    status_byte: u8,
    data_buffer: [u8; 512],
}

impl VirtioBlkDevice {
    pub const fn new() -> Self {
        const EMPTY_DESC: VRingDesc = VRingDesc { addr: 0, len: 0, flags: 0, next: 0 };
        Self {
            io_base: 0,
            capacity_sectors: 0,
            is_initialized: false,
            desc_table: [EMPTY_DESC; VIRTIO_BLK_QUEUE_SIZE],
            avail_ring: VRingAvail { flags: 0, idx: 0, ring: [0; VIRTIO_BLK_QUEUE_SIZE] },
            used_ring: VRingUsed { flags: 0, idx: 0, ring: [VRingUsedElem { id: 0, len: 0 }; VIRTIO_BLK_QUEUE_SIZE] },
            req_header: VirtioBlkReq { req_type: 0, reserved: 0, sector: 0 },
            status_byte: 0,
            data_buffer: [0u8; 512],
        }
    }

    /// Localiza o dispositivo PCI VirtIO-Block
    pub fn probe_pci() -> Option<PciDevice> {
        pci::find_device_by_id(PCI_VENDOR_VIRTIO, PCI_DEVICE_VIRTIO_BLK)
    }

    /// Inicializa a controladora VirtIO-Block
    pub fn init(&mut self) -> Result<(), &'static str> {
        let pci_dev = match Self::probe_pci() {
            Some(d) => d,
            None => return Err("VirtIO-Blk: dispositivo PCI nao encontrado"),
        };

        // Habilita Bus Master e I/O space no PCI
        unsafe {
            pci::enable_bus_mastering(pci_dev.bus, pci_dev.slot, pci_dev.func);
        }

        self.io_base = (pci_dev.bar0 & 0xFFFC) as u16;
        if self.io_base == 0 {
            return Err("VirtIO-Blk: BAR0 I/O invalida");
        }

        unsafe {
            // 1. Reset do dispositivo
            outb(self.io_base + VIRTIO_PCI_STATUS, 0);

            // 2. Acknowledge
            outb(self.io_base + VIRTIO_PCI_STATUS, VIRTIO_STATUS_ACKNOWLEDGE);

            // 3. Driver
            outb(self.io_base + VIRTIO_PCI_STATUS, VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER);

            // 4. Seleciona Fila 0 (Request Queue)
            outw(self.io_base + VIRTIO_PCI_QUEUE_SEL, 0);
            let q_size = inw(self.io_base + VIRTIO_PCI_QUEUE_NUM);
            if q_size == 0 {
                return Err("VirtIO-Blk: fila de requisicoes nao suportada");
            }

            // 5. Configura o endereço PFN da fila na memória
            let q_phys = &self.desc_table as *const _ as u64;
            let pfn = (q_phys / 4096) as u32;
            outl(self.io_base + VIRTIO_PCI_QUEUE_PFN, pfn);

            // 6. Driver OK
            outb(self.io_base + VIRTIO_PCI_STATUS, VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_DRIVER_OK);

            // 7. Lê a capacidade do disco a partir da configuração do dispositivo (offset 0x14 do I/O space)
            let cap_low = inw(self.io_base + 0x14) as u64 | ((inw(self.io_base + 0x16) as u64) << 16);
            let cap_high = inw(self.io_base + 0x18) as u64 | ((inw(self.io_base + 0x1A) as u64) << 16);
            let total_sectors = (cap_low | (cap_high << 32)) as usize;

            self.capacity_sectors = if total_sectors > 0 { total_sectors } else { 131072 }; // Fallback 64 MB
            self.is_initialized = true;
        }

        Ok(())
    }

    /// Executa uma transação síncrona de bloco (Leitura ou Escrita)
    pub fn execute_request(&mut self, req_type: u32, lba: usize, buffer: &mut [u8]) -> Result<(), &'static str> {
        if !self.is_initialized || self.io_base == 0 {
            return Err("VirtIO-Blk: nao inicializado");
        }
        if buffer.len() != DEFAULT_SECTOR_SIZE {
            return Err("VirtIO-Blk: tamanho invalido");
        }

        self.req_header.req_type = req_type;
        self.req_header.reserved = 0;
        self.req_header.sector = lba as u64;
        self.status_byte = 0xFF; // Sentinela de aguardo

        if req_type == VIRTIO_BLK_T_OUT {
            self.data_buffer.copy_from_slice(buffer);
        }

        // Descritor 0: Cabeçalho da requisição (Leitura pelo Device)
        self.desc_table[0] = VRING_DESC_F_NEXT.into_desc(
            &self.req_header as *const _ as u64,
            core::mem::size_of::<VirtioBlkReq>() as u32,
            1,
        );

        // Descritor 1: Buffer de dados de 512 bytes
        let data_flags = if req_type == VIRTIO_BLK_T_IN {
            VRING_DESC_F_NEXT | VRING_DESC_F_WRITE // Device escreve
        } else {
            VRING_DESC_F_NEXT // Device lê
        };
        self.desc_table[1] = data_flags.into_desc(
            self.data_buffer.as_mut_ptr() as u64,
            DEFAULT_SECTOR_SIZE as u32,
            2,
        );

        // Descritor 2: Byte de status (Device escreve)
        self.desc_table[2] = VRING_DESC_F_WRITE.into_desc(
            &mut self.status_byte as *mut _ as u64,
            1,
            0,
        );

        // Adiciona à fila de disponíveis
        let avail_idx = self.avail_ring.idx;
        self.avail_ring.ring[(avail_idx as usize) % VIRTIO_BLK_QUEUE_SIZE] = 0;
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        self.avail_ring.idx = avail_idx.wrapping_add(1);

        // Notifica o dispositivo via porta I/O
        unsafe {
            outw(self.io_base + VIRTIO_PCI_QUEUE_NOTIFY, 0);
        }

        // Aguarda a resposta (polling com timeout)
        let mut spins = 0;
        while self.status_byte == 0xFF {
            core::hint::spin_loop();
            spins += 1;
            if spins > 500_000 {
                return Err("VirtIO-Blk: hardware timeout");
            }
        }

        if self.status_byte != VIRTIO_BLK_S_OK {
            return Err("VirtIO-Blk: hardware failure");
        }

        if req_type == VIRTIO_BLK_T_IN {
            buffer.copy_from_slice(&self.data_buffer);
        }

        Ok(())
    }
}

trait FlagsExt {
    fn into_desc(self, addr: u64, len: u32, next: u16) -> VRingDesc;
}

impl FlagsExt for u16 {
    fn into_desc(self, addr: u64, len: u32, next: u16) -> VRingDesc {
        VRingDesc {
            addr,
            len,
            flags: self,
            next,
        }
    }
}

pub struct VirtioBlockDeviceProxy {
    pub total_sectors: u64,
}

impl VirtioBlockDeviceProxy {
    pub fn new() -> Result<Self, &'static str> {
        let dev = VIRTIO_BLK.lock();
        if !dev.is_initialized {
            return Err("VirtIO-Blk nao inicializado");
        }
        Ok(Self {
            total_sectors: dev.capacity_sectors as u64,
        })
    }
}

impl BlockDevice for VirtioBlockDeviceProxy {
    fn name(&self) -> &'static str {
        "virtio_blk0"
    }

    fn block_size(&self) -> usize {
        DEFAULT_SECTOR_SIZE
    }

    fn total_blocks(&self) -> u64 {
        self.total_sectors
    }

    fn read_blocks(&self, start_block: u64, count: usize, buf: &mut [u8]) -> Result<(), &'static str> {
        let mut dev = VIRTIO_BLK.lock();
        for i in 0..count {
            let offset = i * DEFAULT_SECTOR_SIZE;
            if offset + DEFAULT_SECTOR_SIZE > buf.len() {
                return Err("VirtIO-Blk: buffer overflow");
            }
            dev.execute_request(VIRTIO_BLK_T_IN, (start_block + i as u64) as usize, &mut buf[offset..offset + DEFAULT_SECTOR_SIZE])?;
        }
        Ok(())
    }

    fn write_blocks(&self, start_block: u64, count: usize, buf: &[u8]) -> Result<(), &'static str> {
        let mut dev = VIRTIO_BLK.lock();
        for i in 0..count {
            let offset = i * DEFAULT_SECTOR_SIZE;
            if offset + DEFAULT_SECTOR_SIZE > buf.len() {
                return Err("VirtIO-Blk: buffer overflow");
            }
            let mut sec_buf = [0u8; DEFAULT_SECTOR_SIZE];
            sec_buf.copy_from_slice(&buf[offset..offset + DEFAULT_SECTOR_SIZE]);
            dev.execute_request(VIRTIO_BLK_T_OUT, (start_block + i as u64) as usize, &mut sec_buf)?;
        }
        Ok(())
    }

    fn flush(&self) -> Result<(), &'static str> {
        let mut dev = VIRTIO_BLK.lock();
        let mut dummy = [0u8; DEFAULT_SECTOR_SIZE];
        dev.execute_request(VIRTIO_BLK_T_FLUSH, 0, &mut dummy)
    }

    fn device_info(&self) -> BlockDeviceInfo {
        let mut info = BlockDeviceInfo::empty();
        let name = self.name().as_bytes();
        let len = name.len().min(31);
        info.name[..len].copy_from_slice(&name[..len]);
        info.name_len = len;
        let v = b"Red Hat / VirtIO";
        info.vendor[..v.len()].copy_from_slice(v);
        let m = b"QEMU VirtIO Block Device";
        info.model[..m.len()].copy_from_slice(m);
        info.bus_type = BusType::VirtioBlk;
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

pub static VIRTIO_BLK: SpinLock<VirtioBlkDevice> = SpinLock::new(VirtioBlkDevice::new());

/// Inicializa a controladora VirtIO-Block e registra no catálogo de blocos
pub fn init_virtio_blk() {
    let mut dev = VIRTIO_BLK.lock();
    if dev.init().is_ok() {
        drop(dev);
        if let Ok(proxy) = VirtioBlockDeviceProxy::new() {
            let info = proxy.device_info();
            let mut mgr = crate::drivers::block::BLOCK_DEVICE_MANAGER.lock();
            let _ = mgr.register_device(info);
        }
    }
}

/// Informa se há dispositivo VirtIO-Block operacional no sistema
pub fn has_virtio_blk() -> bool {
    VIRTIO_BLK.lock().is_initialized
}
