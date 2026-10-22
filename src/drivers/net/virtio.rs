//! Jinn OS — Driver PCI VirtIO-Net (Legacy Interface)
//!
//! Controla interfaces de rede VirtIO (`0x1AF4:0x1000` / `0x1041`) no QEMU/KVM
//! através de filas circulares de DMA (*VirtQueues*) e portas de E/S.

use crate::drivers::pci;

// ── Registradores VirtIO PCI Legados (Offsets a partir de BAR0 I/O Base) ───────
const VIRTIO_PCI_HOST_FEATURES:  u16 = 0x00; // 32 bits (R)
const VIRTIO_PCI_GUEST_FEATURES: u16 = 0x04; // 32 bits (R/W)
const VIRTIO_PCI_QUEUE_PFN:      u16 = 0x08; // 32 bits (R/W) — Endereço físico / 4096
const VIRTIO_PCI_QUEUE_NUM:      u16 = 0x0C; // 16 bits (R) — Tamanho da fila
const VIRTIO_PCI_QUEUE_SEL:      u16 = 0x0E; // 16 bits (R/W) — Fila selecionada
const VIRTIO_PCI_QUEUE_NOTIFY:   u16 = 0x10; // 16 bits (W) — Notifica host
const VIRTIO_PCI_STATUS:         u16 = 0x12; // 8 bits (R/W) — Status do dispositivo
const VIRTIO_PCI_ISR:            u16 = 0x13; // 8 bits (R) — Status de interrupção
const VIRTIO_PCI_MAC:            u16 = 0x14; // 6 bytes — Endereço MAC físico

// ── Bits de Status do Dispositivo VirtIO ───────────────────────────────────────
const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 1;
const VIRTIO_STATUS_DRIVER:      u8 = 2;
const VIRTIO_STATUS_DRIVER_OK:   u8 = 4;
const VIRTIO_STATUS_FAILED:      u8 = 128;

// ── Flags de Descritor VirtQueue ───────────────────────────────────────────────
const VRING_DESC_F_NEXT:  u16 = 1;
const VRING_DESC_F_WRITE: u16 = 2;

pub const QUEUE_SIZE: usize = 8;
pub const BUFFER_SIZE: usize = 1536; // MTU 1500 + cabeçalhos

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
    pub ring: [u16; QUEUE_SIZE],
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
    pub ring: [VRingUsedElem; QUEUE_SIZE],
}

/// Cabeçalho VirtIO-Net anexado a cada pacote transmitido/recebido.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct VirtioNetHdr {
    pub flags: u8,
    pub gso_type: u8,
    pub hdr_len: u16,
    pub gso_size: u16,
    pub csum_start: u16,
    pub csum_offset: u16,
}

// ── I/O Ports de Baixo Nível ──────────────────────────────────────────────────
#[inline]
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    core::arch::asm!("in al, dx", out("al") val, in("dx") port, options(nomem, nostack));
    val
}

#[inline]
unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack));
}

#[inline]
unsafe fn inw(port: u16) -> u16 {
    let val: u16;
    core::arch::asm!("in ax, dx", out("ax") val, in("dx") port, options(nomem, nostack));
    val
}

#[inline]
unsafe fn outw(port: u16, val: u16) {
    core::arch::asm!("out dx, ax", in("dx") port, in("ax") val, options(nomem, nostack));
}

#[inline]
unsafe fn inl(port: u16) -> u32 {
    let val: u32;
    core::arch::asm!("in eax, dx", out("eax") val, in("dx") port, options(nomem, nostack));
    val
}

#[inline]
unsafe fn outl(port: u16, val: u32) {
    core::arch::asm!("out dx, eax", in("dx") port, in("eax") val, options(nomem, nostack));
}

// ── Instância e Estado do Driver VirtIO-Net ────────────────────────────────────
pub struct VirtioNetDevice {
    pub io_base: u16,
    pub mac: [u8; 6],
    pub is_available: bool,
    pub rx_avail_idx: u16,
    pub rx_last_used_idx: u16,
    pub tx_avail_idx: u16,
    pub tx_last_used_idx: u16,

    // Buffers DMA físicos
    rx_queue_phys: u64,
    tx_queue_phys: u64,
    rx_buffers_phys: [u64; QUEUE_SIZE],
    tx_buffers_phys: [u64; QUEUE_SIZE],
}

static mut VIRTIO_NET: Option<VirtioNetDevice> = None;

impl VirtioNetDevice {
    pub fn mac_address(&self) -> [u8; 6] {
        self.mac
    }

    /// Envia um frame Ethernet bruto pelo VirtIO TX Ring.
    pub fn transmit(&mut self, frame: &[u8]) -> Result<(), ()> {
        if !self.is_available {
            return Err(());
        }
        if frame.len() + core::mem::size_of::<VirtioNetHdr>() > BUFFER_SIZE {
            return Err(());
        }

        let desc_idx = (self.tx_avail_idx as usize) % QUEUE_SIZE;
        let buf_phys = self.tx_buffers_phys[desc_idx];
        let buf_virt = crate::memory::paging::phys_to_virt(buf_phys) as *mut u8;

        // 1. Zera cabeçalho VirtIO
        let hdr_size = core::mem::size_of::<VirtioNetHdr>();
        unsafe {
            core::ptr::write_bytes(buf_virt, 0, hdr_size);
            // 2. Copia o frame Ethernet
            core::ptr::copy_nonoverlapping(frame.as_ptr(), buf_virt.add(hdr_size), frame.len());
        }
        let total_len = (hdr_size + frame.len()) as u32;

        let tx_virt = crate::memory::paging::phys_to_virt(self.tx_queue_phys);
        let desc_table = tx_virt as *mut VRingDesc;
        let avail_ring = (tx_virt + (16 * QUEUE_SIZE as u64)) as *mut VRingAvail;

        unsafe {
            // Atualiza descritor
            let desc = &mut *desc_table.add(desc_idx);
            desc.addr = buf_phys;
            desc.len = total_len;
            desc.flags = 0; // Leitura pelo host
            desc.next = 0;

            // Insere no anel de disponíveis
            let avail = &mut *avail_ring;
            avail.ring[(self.tx_avail_idx as usize) % QUEUE_SIZE] = desc_idx as u16;
            self.tx_avail_idx = self.tx_avail_idx.wrapping_add(1);
            core::arch::asm!("sfence", options(nostack, preserves_flags));
            avail.idx = self.tx_avail_idx;
            core::arch::asm!("sfence", options(nostack, preserves_flags));

            // Notifica host sobre a fila 1 (TX)
            outw(self.io_base + VIRTIO_PCI_QUEUE_NOTIFY, 1);
        }

        Ok(())
    }

    /// Tenta ler o próximo frame Ethernet disponível na fila RX.
    pub fn receive(&mut self, dest: &mut [u8]) -> Option<usize> {
        if !self.is_available {
            return None;
        }

        let rx_virt = crate::memory::paging::phys_to_virt(self.rx_queue_phys);
        let used_offset = 4096; // Usado alinhado a 4096 bytes
        let used_ring = (rx_virt + used_offset) as *const VRingUsed;

        unsafe {
            let used = &*used_ring;
            core::arch::asm!("lfence", options(nostack, preserves_flags));
            if self.rx_last_used_idx == used.idx {
                return None; // Sem novos pacotes
            }

            let elem = &used.ring[(self.rx_last_used_idx as usize) % QUEUE_SIZE];
            let desc_idx = (elem.id as usize) % QUEUE_SIZE;
            let packet_len = elem.len as usize;

            let hdr_size = core::mem::size_of::<VirtioNetHdr>();
            let frame_len = if packet_len > hdr_size { packet_len - hdr_size } else { 0 };

            let copied = frame_len.min(dest.len());
            if copied > 0 {
                let buf_virt = crate::memory::paging::phys_to_virt(self.rx_buffers_phys[desc_idx]) as *const u8;
                dest[..copied].copy_from_slice(core::slice::from_raw_parts(buf_virt.add(hdr_size), copied));
            }

            // Recicla o descritor de volta para o Available ring
            let avail_ring = (rx_virt + (16 * QUEUE_SIZE as u64)) as *mut VRingAvail;
            let avail = &mut *avail_ring;
            avail.ring[(self.rx_avail_idx as usize) % QUEUE_SIZE] = desc_idx as u16;
            self.rx_avail_idx = self.rx_avail_idx.wrapping_add(1);
            avail.idx = self.rx_avail_idx;
            core::arch::asm!("sfence", options(nostack, preserves_flags));

            self.rx_last_used_idx = self.rx_last_used_idx.wrapping_add(1);
            Some(copied)
        }
    }
}

/// Inicializa o dispositivo VirtIO-Net descoberto no barramento PCI.
pub fn init() -> bool {
    // Procura o dispositivo PCI VirtIO-Net (Vendor 0x1AF4, Device 0x1000 ou 0x1041)
    let pci_dev = match pci::find_device_by_id(0x1AF4, 0x1000)
        .or_else(|| pci::find_device_by_id(0x1AF4, 0x1041))
    {
        Some(dev) => dev,
        None => {
            // Se não houver VirtIO-Net emulado, inicializa mock para loopback
            unsafe {
                VIRTIO_NET = Some(VirtioNetDevice {
                    io_base: 0,
                    mac: [0x52, 0x54, 0x00, 0x12, 0x34, 0x56],
                    is_available: false,
                    rx_avail_idx: 0,
                    rx_last_used_idx: 0,
                    tx_avail_idx: 0,
                    tx_last_used_idx: 0,
                    rx_queue_phys: 0,
                    tx_queue_phys: 0,
                    rx_buffers_phys: [0; QUEUE_SIZE],
                    tx_buffers_phys: [0; QUEUE_SIZE],
                });
            }
            return false;
        }
    };

    let io_base = (pci_dev.bar0 & !0x3) as u16;
    unsafe {
        // 1. Reset do dispositivo
        outb(io_base + VIRTIO_PCI_STATUS, 0);

        // 2. Acknowledge + Driver
        outb(io_base + VIRTIO_PCI_STATUS, VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER);

        // 3. Lê endereço MAC
        let mut mac = [0u8; 6];
        for i in 0..6 {
            mac[i] = inb(io_base + VIRTIO_PCI_MAC + i as u16);
        }

        // 4. Configuração das VirtQueues
        // Aloca frames para filas RX e TX
        let rx_phys = crate::memory::alloc_frame().expect("VirtIO: sem frame para RX queue");
        let rx_virt = crate::memory::paging::phys_to_virt(rx_phys);
        core::ptr::write_bytes(rx_virt as *mut u8, 0, 4096);

        outw(io_base + VIRTIO_PCI_QUEUE_SEL, 0); // Fila 0 = RX
        outl(io_base + VIRTIO_PCI_QUEUE_PFN, (rx_phys >> 12) as u32);

        let tx_phys = crate::memory::alloc_frame().expect("VirtIO: sem frame para TX queue");
        let tx_virt = crate::memory::paging::phys_to_virt(tx_phys);
        core::ptr::write_bytes(tx_virt as *mut u8, 0, 4096);

        outw(io_base + VIRTIO_PCI_QUEUE_SEL, 1); // Fila 1 = TX
        outl(io_base + VIRTIO_PCI_QUEUE_PFN, (tx_phys >> 12) as u32);

        let mut rx_buffers_phys = [0u64; QUEUE_SIZE];
        let mut tx_buffers_phys = [0u64; QUEUE_SIZE];
        for i in 0..QUEUE_SIZE {
            let rf = crate::memory::alloc_frame().expect("VirtIO: sem frame para RX buf");
            rx_buffers_phys[i] = rf;
            let tf = crate::memory::alloc_frame().expect("VirtIO: sem frame para TX buf");
            tx_buffers_phys[i] = tf;
        }

        let mut dev = VirtioNetDevice {
            io_base,
            mac,
            is_available: true,
            rx_avail_idx: 0,
            rx_last_used_idx: 0,
            tx_avail_idx: 0,
            tx_last_used_idx: 0,
            rx_queue_phys: rx_phys,
            tx_queue_phys: tx_phys,
            rx_buffers_phys,
            tx_buffers_phys,
        };

        // Popula descritores de recepção (RX)
        let rx_desc = rx_virt as *mut VRingDesc;
        let rx_avail = (rx_virt + (16 * QUEUE_SIZE as u64)) as *mut VRingAvail;

        for i in 0..QUEUE_SIZE {
            let desc = &mut *rx_desc.add(i);
            desc.addr = dev.rx_buffers_phys[i];
            desc.len = BUFFER_SIZE as u32;
            desc.flags = VRING_DESC_F_WRITE; // Escrita pela placa
            desc.next = 0;

            (*rx_avail).ring[i] = i as u16;
        }
        (*rx_avail).idx = QUEUE_SIZE as u16;
        dev.rx_avail_idx = QUEUE_SIZE as u16;

        // 5. Conclui handshake com DRIVER_OK
        outb(io_base + VIRTIO_PCI_STATUS, VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_DRIVER_OK);

        // Notifica host da disponibilidade dos buffers RX
        outw(io_base + VIRTIO_PCI_QUEUE_NOTIFY, 0);

        VIRTIO_NET = Some(dev);
    }

    true
}

pub fn get_device() -> Option<&'static mut VirtioNetDevice> {
    unsafe {
        VIRTIO_NET.as_mut()
    }
}
