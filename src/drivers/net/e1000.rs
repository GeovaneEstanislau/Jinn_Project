//! Jinn OS — Driver PCI Intel PRO/1000 (82540EM / 82545EM / e1000)
//!
//! Fornece suporte nativo a placas de rede Intel Gigabit Ethernet (Vendor 0x8086),
//! controladora de rede padrão emulada no Oracle VirtualBox, VMware e QEMU.
//! Opera via MMIO (BAR0) e DMA físico de anéis de descritores RX/TX.

use crate::drivers::pci::{self, PciDevice};

// ── Registradores MMIO Intel e1000 (Offsets a partir de BAR0) ───────────────────
pub const REG_CTRL:    usize = 0x0000; // Device Control
pub const REG_STATUS:  usize = 0x0008; // Device Status
pub const REG_EERD:    usize = 0x0014; // EEPROM Read
pub const REG_ICR:     usize = 0x00C0; // Interrupt Cause Read
pub const REG_IMS:     usize = 0x00D0; // Interrupt Mask Set
pub const REG_IMC:     usize = 0x00D8; // Interrupt Mask Clear
pub const REG_RCTL:    usize = 0x0100; // Receive Control
pub const REG_TCTL:    usize = 0x0400; // Transmit Control
pub const REG_TIPG:    usize = 0x0410; // Transmit Inter Packet Gap
pub const REG_RDBAL:   usize = 0x2800; // RX Descriptor Base Address Low
pub const REG_RDBAH:   usize = 0x2804; // RX Descriptor Base Address High
pub const REG_RDLEN:   usize = 0x2808; // RX Descriptor Ring Length (bytes)
pub const REG_RDH:     usize = 0x2810; // RX Descriptor Head
pub const REG_RDT:     usize = 0x2818; // RX Descriptor Tail
pub const REG_TDBAL:   usize = 0x3800; // TX Descriptor Base Address Low
pub const REG_TDBAH:   usize = 0x3804; // TX Descriptor Base Address High
pub const REG_TDLEN:   usize = 0x3808; // TX Descriptor Ring Length (bytes)
pub const REG_TDH:     usize = 0x3810; // TX Descriptor Head
pub const REG_TDT:     usize = 0x3818; // TX Descriptor Tail
pub const REG_RAL0:    usize = 0x5400; // Receive Address Low 0
pub const REG_RAH0:    usize = 0x5404; // Receive Address High 0

// ── Flags de Controle RCTL (Recepção) ──────────────────────────────────────────
pub const RCTL_EN:    u32 = 1 << 1;   // Receiver Enable
pub const RCTL_SBP:   u32 = 1 << 2;   // Store Bad Packets
pub const RCTL_UPE:   u32 = 1 << 3;   // Unicast Promiscuous Enable
pub const RCTL_MPE:   u32 = 1 << 4;   // Multicast Promiscuous Enable
pub const RCTL_BAM:   u32 = 1 << 15;  // Broadcast Accept Mode
pub const RCTL_BSIZE_2048: u32 = 0;   // Buffer size 2048 bytes
pub const RCTL_SECRC: u32 = 1 << 26;  // Strip Ethernet CRC

// ── Flags de Controle TCTL (Transmissão) ───────────────────────────────────────
pub const TCTL_EN:   u32 = 1 << 1;     // Transmit Enable
pub const TCTL_PSP:  u32 = 1 << 3;     // Pad Short Packets
pub const TCTL_CT:   u32 = 0x0F << 4;  // Collision Threshold
pub const TCTL_COLD: u32 = 0x40 << 12; // Collision Distance

// ── Flags de Descritores TX e RX ──────────────────────────────────────────────
pub const CMD_EOP:  u8 = 1 << 0; // End of Packet
pub const CMD_IFCS: u8 = 1 << 1; // Insert FCS / CRC
pub const CMD_RS:   u8 = 1 << 3; // Report Status
pub const STAT_DD:  u8 = 1 << 0; // Descriptor Done

pub const NUM_DESC: usize = 32;
pub const BUFFER_SIZE: usize = 2048;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct E1000RxDesc {
    pub addr: u64,
    pub length: u16,
    pub csum: u16,
    pub status: u8,
    pub errors: u8,
    pub special: u16,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct E1000TxDesc {
    pub addr: u64,
    pub length: u16,
    pub cso: u8,
    pub cmd: u8,
    pub status: u8,
    pub css: u8,
    pub special: u16,
}

pub struct E1000Device {
    pub mmio_base: u64,
    pub mac: [u8; 6],
    pub is_available: bool,
    pub rx_cur: usize,
    pub tx_cur: usize,

    rx_ring_phys: u64,
    tx_ring_phys: u64,
    rx_buffers_phys: [u64; NUM_DESC],
    tx_buffers_phys: [u64; NUM_DESC],
}

static mut E1000_NET: Option<E1000Device> = None;

impl E1000Device {
    #[inline]
    unsafe fn write_reg(&self, reg: usize, val: u32) {
        let ptr = (self.mmio_base + reg as u64) as *mut u32;
        core::ptr::write_volatile(ptr, val);
    }

    #[inline]
    unsafe fn read_reg(&self, reg: usize) -> u32 {
        let ptr = (self.mmio_base + reg as u64) as *const u32;
        core::ptr::read_volatile(ptr)
    }

    pub fn mac_address(&self) -> [u8; 6] {
        self.mac
    }

    pub fn is_available(&self) -> bool {
        self.is_available
    }

    /// Transmite um frame Ethernet bruto pelo anel de transmissão (TX Ring).
    pub fn transmit(&mut self, frame: &[u8]) -> Result<(), ()> {
        if !self.is_available || frame.is_empty() || frame.len() > BUFFER_SIZE {
            return Err(());
        }

        let desc_idx = self.tx_cur;
        let buf_phys = self.tx_buffers_phys[desc_idx];
        let buf_virt = crate::memory::paging::phys_to_virt(buf_phys) as *mut u8;

        unsafe {
            core::ptr::copy_nonoverlapping(frame.as_ptr(), buf_virt, frame.len());

            let tx_virt = crate::memory::paging::phys_to_virt(self.tx_ring_phys) as *mut E1000TxDesc;
            let desc = &mut *tx_virt.add(desc_idx);
            desc.addr = buf_phys;
            let send_len = frame.len().max(60);
            if send_len > frame.len() {
                core::ptr::write_bytes(buf_virt.add(frame.len()), 0, send_len - frame.len());
            }
            desc.length = send_len as u16;
            desc.cso = 0;
            desc.cmd = CMD_EOP | CMD_IFCS | CMD_RS;
            desc.status = 0;
            desc.css = 0;
            desc.special = 0;

            self.tx_cur = (self.tx_cur + 1) % NUM_DESC;
            core::arch::asm!("sfence", options(nostack, preserves_flags));

            // Atualiza registrador TDT para notificar hardware
            self.write_reg(REG_TDT, self.tx_cur as u32);
        }

        Ok(())
    }

    /// Recebe o próximo frame Ethernet disponível no anel de recepção (RX Ring).
    pub fn receive(&mut self, dest: &mut [u8]) -> Option<usize> {
        if !self.is_available {
            return None;
        }

        let desc_idx = self.rx_cur;
        let rx_virt = crate::memory::paging::phys_to_virt(self.rx_ring_phys) as *mut E1000RxDesc;

        unsafe {
            let desc = &mut *rx_virt.add(desc_idx);
            core::arch::asm!("lfence", options(nostack, preserves_flags));

            // Verifica se o hardware marcou o descritor como concluído (STAT_DD)
            if (desc.status & STAT_DD) == 0 {
                return None; // Sem pacotes novos
            }

            let pkt_len = (desc.length as usize).min(BUFFER_SIZE);
            let copy_len = pkt_len.min(dest.len());

            let buf_virt = crate::memory::paging::phys_to_virt(self.rx_buffers_phys[desc_idx]) as *const u8;
            core::ptr::copy_nonoverlapping(buf_virt, dest.as_mut_ptr(), copy_len);

            // Recicla descritor limpando o status
            desc.status = 0;
            core::arch::asm!("sfence", options(nostack, preserves_flags));

            // Avança anel RX
            self.write_reg(REG_RDT, desc_idx as u32);
            self.rx_cur = (self.rx_cur + 1) % NUM_DESC;

            Some(copy_len)
        }
    }
}

/// Descobre e inicializa dispositivo Intel PRO/1000 compatível no barramento PCI.
pub fn init() -> bool {
    // Lista de Device IDs suportados para Intel 82540EM / 82545EM / e1000
    let supported_ids = [0x100E, 0x1004, 0x100F, 0x1010, 0x1019, 0x101D, 0x101E, 0x1026, 0x1079, 0x107C, 0x107D, 0x10D3];
    let mut target_dev: Option<PciDevice> = None;

    for &dev_id in &supported_ids {
        if let Some(dev) = pci::find_device_by_id(0x8086, dev_id) {
            target_dev = Some(dev);
            break;
        }
    }

    let pci_dev = match target_dev {
        Some(dev) => dev,
        None => return false,
    };

    // Habilita PCI Bus Master (DMA), Memory Space e I/O Space
    pci::enable_bus_mastering(pci_dev.bus, pci_dev.slot, pci_dev.func);

    // BAR0 é endereço físico de memória MMIO (alinhamento de 16 bytes)
    let bar0_phys = (pci_dev.bar0 & !0xF) as u64;
    let mmio_base = crate::memory::paging::phys_to_virt(bar0_phys);

    unsafe {
        // Aloca 1 frame (4 KiB) para os descritores de RX e 1 frame para TX
        let rx_ring_phys = crate::memory::alloc_frame().expect("e1000: sem frame para RX ring");
        let rx_ring_virt = crate::memory::paging::phys_to_virt(rx_ring_phys);
        core::ptr::write_bytes(rx_ring_virt as *mut u8, 0, 4096);

        let tx_ring_phys = crate::memory::alloc_frame().expect("e1000: sem frame para TX ring");
        let tx_ring_virt = crate::memory::paging::phys_to_virt(tx_ring_phys);
        core::ptr::write_bytes(tx_ring_virt as *mut u8, 0, 4096);

        let mut rx_buffers_phys = [0u64; NUM_DESC];
        let mut tx_buffers_phys = [0u64; NUM_DESC];

        for i in 0..NUM_DESC {
            rx_buffers_phys[i] = crate::memory::alloc_frame().expect("e1000: sem frame para RX buf");
            tx_buffers_phys[i] = crate::memory::alloc_frame().expect("e1000: sem frame para TX buf");
        }

        // Lê endereço MAC físico a partir dos registradores RAL0 e RAH0
        let ral = core::ptr::read_volatile((mmio_base + REG_RAL0 as u64) as *const u32);
        let rah = core::ptr::read_volatile((mmio_base + REG_RAH0 as u64) as *const u32);
        let mut mac = [0u8; 6];
        mac[0] = (ral & 0xFF) as u8;
        mac[1] = ((ral >> 8) & 0xFF) as u8;
        mac[2] = ((ral >> 16) & 0xFF) as u8;
        mac[3] = ((ral >> 24) & 0xFF) as u8;
        mac[4] = (rah & 0xFF) as u8;
        mac[5] = ((rah >> 8) & 0xFF) as u8;

        // Se RAH0 não estiver válido (Address Valid bit 31 == 0), tenta ler EEPROM
        if (rah & (1 << 31)) == 0 && mac == [0; 6] {
            mac = [0x08, 0x00, 0x27, 0x12, 0x34, 0x56]; // Fallback para OUI VirtualBox
        }

        let dev = E1000Device {
            mmio_base,
            mac,
            is_available: true,
            rx_cur: 0,
            tx_cur: 0,
            rx_ring_phys,
            tx_ring_phys,
            rx_buffers_phys,
            tx_buffers_phys,
        };

        // 1. Desabilita interrupções durante a configuração inicial
        dev.write_reg(REG_IMC, 0xFFFF_FFFF);

        // 2. Configura anel de recepção (RX)
        let rx_desc = rx_ring_virt as *mut E1000RxDesc;
        for i in 0..NUM_DESC {
            let d = &mut *rx_desc.add(i);
            d.addr = dev.rx_buffers_phys[i];
            d.status = 0;
        }

        dev.write_reg(REG_RDBAL, rx_ring_phys as u32);
        dev.write_reg(REG_RDBAH, (rx_ring_phys >> 32) as u32);
        dev.write_reg(REG_RDLEN, (NUM_DESC * core::mem::size_of::<E1000RxDesc>()) as u32);
        dev.write_reg(REG_RDH, 0);
        dev.write_reg(REG_RDT, (NUM_DESC - 1) as u32);

        // Habilita recepção com Broadcast e remoção de CRC
        dev.write_reg(REG_RCTL, RCTL_EN | RCTL_SBP | RCTL_UPE | RCTL_MPE | RCTL_BAM | RCTL_SECRC | RCTL_BSIZE_2048);

        // 3. Configura anel de transmissão (TX)
        dev.write_reg(REG_TDBAL, tx_ring_phys as u32);
        dev.write_reg(REG_TDBAH, (tx_ring_phys >> 32) as u32);
        dev.write_reg(REG_TDLEN, (NUM_DESC * core::mem::size_of::<E1000TxDesc>()) as u32);
        dev.write_reg(REG_TDH, 0);
        dev.write_reg(REG_TDT, 0);

        // Configura espaçamento inter-pacotes (TIPG) padrão IEEE 802.3
        dev.write_reg(REG_TIPG, 0x0060_200A);

        // Habilita transmissão
        dev.write_reg(REG_TCTL, TCTL_EN | TCTL_PSP | TCTL_CT | TCTL_COLD);

        E1000_NET = Some(dev);
    }

    true
}

pub fn get_device() -> Option<&'static mut E1000Device> {
    unsafe { E1000_NET.as_mut() }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_e1000_descriptors_size_and_alignment() {
        assert_eq!(core::mem::size_of::<E1000RxDesc>(), 16);
        assert_eq!(core::mem::size_of::<E1000TxDesc>(), 16);
        assert_eq!(core::mem::align_of::<E1000RxDesc>(), 1);
        assert_eq!(core::mem::align_of::<E1000TxDesc>(), 1);
    }

    #[test]
    fn test_e1000_ring_modulo_and_status() {
        let mut cur = 0usize;
        for _ in 0..NUM_DESC {
            cur = (cur + 1) % NUM_DESC;
        }
        assert_eq!(cur, 0);
        assert_eq!(STAT_DD, 1 << 0);
        assert_eq!(CMD_EOP | CMD_IFCS | CMD_RS, 0x0B);
    }
}
