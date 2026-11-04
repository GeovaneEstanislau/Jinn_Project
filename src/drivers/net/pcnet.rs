//! Jinn OS — Driver PCI AMD PCnet-FAST III / PCnet-PCI II (Am79C973 / Am79C970A)
//!
//! Fornece suporte nativo a controladoras de rede AMD PCnet (Vendor 0x1022, Device 0x2000 / 0x2001),
//! o adaptador de rede padrão emulado pelo Oracle VirtualBox (PCnet-FAST III)
//! quando criado como máquina genérica/outros sistemas operacionais.
//! Opera via I/O ports e DMA físico de 32 bits com anéis de descritores RX/TX.

use crate::drivers::pci::{self, PciDevice};

// ── Registradores AMD PCnet em Modo 32 bits (DWIO) ───────────────────────────
// Offsets a partir de io_base:
pub const REG_APROM: usize = 0x00; // 0x00..0x0F: MAC Address PROM
pub const REG_RDP:   u16   = 0x10; // Register Data Port (32 bits)
pub const REG_RAP:   u16   = 0x14; // Register Address Port (32 bits)
pub const REG_RESET: u16   = 0x18; // 32-bit Reset Register
pub const REG_BDP:   u16   = 0x1C; // Bus Configuration Register Data Port (32 bits)

// ── Offsets em Modo 16 bits (antes do chaveamento DWIO) ───────────────────────
pub const REG_RESET_16: u16 = 0x14;

// ── Registradores de Controle e Estado (CSR) ──────────────────────────────────
pub const CSR0_INIT: u32 = 1 << 0;  // 0x0001: Initialize
pub const CSR0_STRT: u32 = 1 << 1;  // 0x0002: Start
pub const CSR0_STOP: u32 = 1 << 2;  // 0x0004: Stop
pub const CSR0_TDMD: u32 = 1 << 3;  // 0x0008: Transmit Demand
pub const CSR0_TXON: u32 = 1 << 4;  // 0x0010: Transmitter On
pub const CSR0_RXON: u32 = 1 << 5;  // 0x0020: Receiver On
pub const CSR0_IENA: u32 = 1 << 6;  // 0x0040: Interrupt Enable
pub const CSR0_INTR: u32 = 1 << 7;  // 0x0080: Interrupt Flag
pub const CSR0_IDON: u32 = 1 << 8;  // 0x0100: Initialization Done (W1C)
pub const CSR0_TINT: u32 = 1 << 9;  // 0x0200: Transmit Interrupt (W1C)
pub const CSR0_RINT: u32 = 1 << 10; // 0x0400: Receive Interrupt (W1C)
pub const CSR0_MERR: u32 = 1 << 11; // 0x0800: Memory Error (W1C)
pub const CSR0_MISS: u32 = 1 << 12; // 0x1000: Missed Frame (W1C)
pub const CSR0_CERR: u32 = 1 << 13; // 0x2000: Collision Error (W1C)
pub const CSR0_BABL: u32 = 1 << 14; // 0x4000: Babble Error (W1C)
pub const CSR0_ERR:  u32 = 1 << 15; // 0x8000: Error Summary

// ── Registradores de Configuração de Barramento (BCR) ─────────────────────────
pub const BCR_SWSTYLE: u32 = 20;    // Software Style Register (BCR20)

// ── Flags de Descritores ──────────────────────────────────────────────────────
pub const DESC_OWN: u16 = 1 << 15; // 0x8000: Owned by controller
pub const DESC_ERR: u16 = 1 << 14; // 0x4000: Error summary
pub const DESC_STP: u16 = 1 << 9;  // 0x0200: Start of Packet
pub const DESC_ENP: u16 = 1 << 8;  // 0x0100: End of Packet

pub const NUM_DESC: usize = 32;
pub const BUFFER_SIZE: usize = 1536;

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    core::arch::asm!("in al, dx", out("al") val, in("dx") port, options(nomem, nostack));
    val
}

#[inline]
unsafe fn inw(port: u16) -> u16 {
    let val: u16;
    core::arch::asm!("in ax, dx", out("ax") val, in("dx") port, options(nomem, nostack));
    val
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

/// Bloco de Inicialização em modo 32 bits (SWSTYLE 2, 28 bytes)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct PcnetInitBlock {
    pub mode: u16,         // 0x00: Modo de operação (0 = normal)
    pub rlen_tlen: u16,    // 0x02: rlen em bits 4..7 (log2(32)=5), tlen em bits 12..15 (log2(32)=5) -> 0x5050
    pub mac: [u8; 6],      // 0x04: Endereço MAC físico
    pub reserved: u16,     // 0x0A: Reservado
    pub ladr: [u8; 8],     // 0x0C: Logical Address Filter (multicast filter, 0)
    pub rx_ring_phys: u32, // 0x14: Endereço físico do anel RX
    pub tx_ring_phys: u32, // 0x18: Endereço físico do anel TX
}

/// Descritor RX de 32 bits (16 bytes)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct PcnetRxDesc {
    pub base_addr: u32,
    pub buf_len: u16,  // Complemento de 2 do buffer | 0xF000
    pub status: u16,   // Bit 15: OWN, Bit 9: STP, Bit 8: ENP, Bit 14: ERR
    pub msg_len: u32,  // Bits 0..11: tamanho real do pacote recebido
    pub reserved: u32,
}

/// Descritor TX de 32 bits (16 bytes)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct PcnetTxDesc {
    pub base_addr: u32,
    pub buf_len: u16,  // Complemento de 2 do tamanho do pacote | 0xF000
    pub status: u16,   // Bit 15: OWN, Bit 9: STP, Bit 8: ENP
    pub misc: u32,
    pub reserved: u32,
}

pub struct PcnetDevice {
    pub io_base: u16,
    pub mac: [u8; 6],
    pub is_available: bool,
    pub rx_cur: usize,
    pub tx_cur: usize,

    rx_ring_phys: u64,
    tx_ring_phys: u64,
    rx_buffers_phys: [u64; NUM_DESC],
    tx_buffers_phys: [u64; NUM_DESC],
}

static mut PCNET_DEVICE: Option<PcnetDevice> = None;

impl PcnetDevice {
    #[inline]
    unsafe fn write_csr(&self, csr: u32, val: u32) {
        outl(self.io_base + REG_RAP, csr);
        outl(self.io_base + REG_RDP, val);
    }

    #[inline]
    unsafe fn read_csr(&self, csr: u32) -> u32 {
        outl(self.io_base + REG_RAP, csr);
        inl(self.io_base + REG_RDP)
    }

    #[inline]
    unsafe fn write_bcr(&self, bcr: u32, val: u32) {
        outl(self.io_base + REG_RAP, bcr);
        outl(self.io_base + REG_BDP, val);
    }

    pub fn mac_address(&self) -> [u8; 6] {
        self.mac
    }

    pub fn is_available(&self) -> bool {
        self.is_available
    }

    /// Transmite um quadro Ethernet bruto através do anel TX DMA.
    pub fn transmit(&mut self, frame: &[u8]) -> Result<(), ()> {
        if !self.is_available || frame.is_empty() || frame.len() > BUFFER_SIZE {
            return Err(());
        }

        let desc_idx = self.tx_cur;
        let tx_virt = crate::memory::paging::phys_to_virt(self.tx_ring_phys) as *mut PcnetTxDesc;

        unsafe {
            let desc = &mut *tx_virt.add(desc_idx);
            // Verifica se o descritor ainda está retido pelo controlador (OWN = 1)
            if (desc.status & DESC_OWN) != 0 {
                return Err(());
            }

            let buf_phys = self.tx_buffers_phys[desc_idx];
            let buf_virt = crate::memory::paging::phys_to_virt(buf_phys) as *mut u8;
            core::ptr::copy_nonoverlapping(frame.as_ptr(), buf_virt, frame.len());

            desc.base_addr = buf_phys as u32;
            desc.buf_len = (-(frame.len() as i16) as u16) | 0xF000;
            desc.misc = 0;
            desc.reserved = 0;
            core::arch::asm!("sfence", options(nostack, preserves_flags));

            // STP | ENP | OWN (inicia envio)
            desc.status = DESC_OWN | DESC_STP | DESC_ENP;
            core::arch::asm!("sfence", options(nostack, preserves_flags));

            // Notifica o controlador com Transmit Demand (CSR0 TDMD | STRT)
            self.write_csr(0, CSR0_TDMD | CSR0_STRT);

            self.tx_cur = (self.tx_cur + 1) % NUM_DESC;
        }

        Ok(())
    }

    /// Recebe o próximo quadro Ethernet do anel RX DMA.
    pub fn receive(&mut self, dest: &mut [u8]) -> Option<usize> {
        if !self.is_available {
            return None;
        }

        let desc_idx = self.rx_cur;
        let rx_virt = crate::memory::paging::phys_to_virt(self.rx_ring_phys) as *mut PcnetRxDesc;

        unsafe {
            let desc = &mut *rx_virt.add(desc_idx);
            core::arch::asm!("lfence", options(nostack, preserves_flags));

            // Se o bit OWN ainda estiver ativo (1), a placa ainda não recebeu um novo pacote
            if (desc.status & DESC_OWN) != 0 {
                return None;
            }

            let has_err = (desc.status & DESC_ERR) != 0;
            let is_packet = (desc.status & (DESC_STP | DESC_ENP)) == (DESC_STP | DESC_ENP);

            let mut copy_len = 0;
            if !has_err && is_packet {
                let raw_len = (desc.msg_len & 0x0FFF) as usize;
                // Descarta 4 bytes de CRC Ethernet se presentes
                let pkt_len = if raw_len > 4 { raw_len - 4 } else { raw_len };
                let to_copy = pkt_len.min(dest.len()).min(BUFFER_SIZE);

                let buf_virt = crate::memory::paging::phys_to_virt(self.rx_buffers_phys[desc_idx]) as *const u8;
                core::ptr::copy_nonoverlapping(buf_virt, dest.as_mut_ptr(), to_copy);
                copy_len = to_copy;
            }

            // Limpa interrupção de recepção em CSR0 se pendente
            self.write_csr(0, CSR0_RINT | CSR0_STRT);

            // Recicla o descritor e devolve a posse para a placa
            desc.buf_len = (-(BUFFER_SIZE as i16) as u16) | 0xF000;
            desc.msg_len = 0;
            core::arch::asm!("sfence", options(nostack, preserves_flags));
            desc.status = DESC_OWN;
            core::arch::asm!("sfence", options(nostack, preserves_flags));

            self.rx_cur = (self.rx_cur + 1) % NUM_DESC;

            if copy_len > 0 {
                Some(copy_len)
            } else {
                None
            }
        }
    }
}

/// Descobre e inicializa dispositivo AMD PCnet (FAST III / PCI II) no barramento PCI.
pub fn init() -> bool {
    // Procura por dispositivo AMD PCnet (Vendor 0x1022, Device 0x2000 ou 0x2001)
    let mut target_dev: Option<PciDevice> = None;
    for &dev_id in &[0x2000, 0x2001] {
        if let Some(dev) = pci::find_device_by_id(0x1022, dev_id) {
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

    // BAR0 é endereço de porta de I/O
    let io_base = (pci_dev.bar0 & !0x3) as u16;
    if io_base == 0 {
        return false;
    }

    unsafe {
        // 1. Reset da placa:
        // Lê porta de reset de 32 bits e 16 bits para forçar o reinício do hardware
        let _ = inl(io_base + REG_RESET);
        let _ = inw(io_base + REG_RESET_16);

        // Breve espera para o chip completar o ciclo de reset
        for _ in 0..10_000 {
            core::hint::spin_loop();
        }

        // 2. Chaveamento para Modo DWIO de 32 bits:
        // Uma escrita de 32 bits com valor 0 na porta RDP (offset 0x10) ativa o modo DWIO 32-bit
        outl(io_base + REG_RDP, 0);

        // 3. Configura Software Style 32-bit (BCR20):
        // Seleciona SWSTYLE = 2 (PCnet-PCI II / FAST III com descritores de 32 bits)
        // Bit 8 (SSIZE32 = 1) + Bit 1 (SWSTYLE = 2) => 0x0102
        outl(io_base + REG_RAP, BCR_SWSTYLE);
        outl(io_base + REG_BDP, 0x0102);

        // 4. Para a placa para configurar os descritores (CSR0 STOP)
        outl(io_base + REG_RAP, 0);
        outl(io_base + REG_RDP, CSR0_STOP);

        // 5. Lê endereço MAC da APROM (primeiros 6 bytes)
        let mut mac = [0u8; 6];
        for i in 0..6 {
            mac[i] = inb(io_base + i as u16);
        }

        if mac == [0; 6] || mac == [0xFF; 6] {
            let dw0 = inl(io_base);
            let dw1 = inl(io_base + 4);
            mac[0] = (dw0 & 0xFF) as u8;
            mac[1] = ((dw0 >> 8) & 0xFF) as u8;
            mac[2] = ((dw0 >> 16) & 0xFF) as u8;
            mac[3] = ((dw0 >> 24) & 0xFF) as u8;
            mac[4] = (dw1 & 0xFF) as u8;
            mac[5] = ((dw1 >> 8) & 0xFF) as u8;
        }

        if mac == [0; 6] || mac == [0xFF; 6] {
            mac = [0x08, 0x00, 0x27, 0x12, 0x34, 0x56]; // OUI VirtualBox
        }

        // 6. Aloca frames para InitBlock, Anéis RX/TX e Buffers
        let init_block_phys = match crate::memory::alloc_frame() {
            Some(f) => f,
            None => return false,
        };
        let init_block_virt = crate::memory::paging::phys_to_virt(init_block_phys);
        core::ptr::write_bytes(init_block_virt as *mut u8, 0, 4096);

        let rx_ring_phys = match crate::memory::alloc_frame() {
            Some(f) => f,
            None => return false,
        };
        let rx_ring_virt = crate::memory::paging::phys_to_virt(rx_ring_phys);
        core::ptr::write_bytes(rx_ring_virt as *mut u8, 0, 4096);

        let tx_ring_phys = match crate::memory::alloc_frame() {
            Some(f) => f,
            None => return false,
        };
        let tx_ring_virt = crate::memory::paging::phys_to_virt(tx_ring_phys);
        core::ptr::write_bytes(tx_ring_virt as *mut u8, 0, 4096);

        let mut rx_buffers_phys = [0u64; NUM_DESC];
        let mut tx_buffers_phys = [0u64; NUM_DESC];

        for i in 0..NUM_DESC {
            rx_buffers_phys[i] = match crate::memory::alloc_frame() {
                Some(f) => f,
                None => return false,
            };
            tx_buffers_phys[i] = match crate::memory::alloc_frame() {
                Some(f) => f,
                None => return false,
            };
        }

        // 7. Configura anel de recepção RX
        let rx_desc = rx_ring_virt as *mut PcnetRxDesc;
        for i in 0..NUM_DESC {
            let d = &mut *rx_desc.add(i);
            d.base_addr = rx_buffers_phys[i] as u32;
            d.buf_len = (-(BUFFER_SIZE as i16) as u16) | 0xF000;
            d.status = DESC_OWN; // A placa passa a ser dona do buffer para receber pacotes
            d.msg_len = 0;
            d.reserved = 0;
        }

        // 8. Configura anel de transmissão TX
        let tx_desc = tx_ring_virt as *mut PcnetTxDesc;
        for i in 0..NUM_DESC {
            let d = &mut *tx_desc.add(i);
            d.base_addr = tx_buffers_phys[i] as u32;
            d.buf_len = 0;
            d.status = 0; // Host é dono inicialmente
            d.misc = 0;
            d.reserved = 0;
        }

        // 9. Preenche o bloco de inicialização (InitBlock)
        let ib = &mut *(init_block_virt as *mut PcnetInitBlock);
        ib.mode = 0; // Modo normal
        // log2(32) = 5 -> RLEN=5 (bits 4..7: 0x50), TLEN=5 (bits 12..15: 0x5000) => 0x5050
        ib.rlen_tlen = 0x5050;
        ib.mac = mac;
        ib.reserved = 0;
        ib.ladr = [0; 8];
        ib.rx_ring_phys = rx_ring_phys as u32;
        ib.tx_ring_phys = tx_ring_phys as u32;

        let dev = PcnetDevice {
            io_base,
            mac,
            is_available: true,
            rx_cur: 0,
            tx_cur: 0,
            rx_ring_phys,
            tx_ring_phys,
            rx_buffers_phys,
            tx_buffers_phys,
        };

        // 10. Programa endereço do InitBlock nos registradores CSR1 e CSR2
        dev.write_csr(1, (init_block_phys & 0xFFFF) as u32);
        dev.write_csr(2, ((init_block_phys >> 16) & 0xFFFF) as u32);

        // 11. Habilita auto-padding de pacotes em CSR4
        dev.write_csr(4, 0x0915);

        // 12. Dispara inicialização via CSR0 INIT
        dev.write_csr(0, CSR0_INIT);

        // Aguarda conclusão (IDON)
        let mut timeout = 100_000;
        while timeout > 0 {
            let csr0 = dev.read_csr(0);
            if (csr0 & CSR0_IDON) != 0 {
                break;
            }
            timeout -= 1;
        }

        // 13. Limpa IDON e inicia a placa (CSR0 STRT | IENA)
        dev.write_csr(0, CSR0_IDON | CSR0_STRT | CSR0_IENA);

        PCNET_DEVICE = Some(dev);
    }

    true
}

pub fn get_device() -> Option<&'static mut PcnetDevice> {
    unsafe { PCNET_DEVICE.as_mut() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pcnet_structures_size_and_alignment() {
        assert_eq!(core::mem::size_of::<PcnetInitBlock>(), 28);
        assert_eq!(core::mem::size_of::<PcnetRxDesc>(), 16);
        assert_eq!(core::mem::size_of::<PcnetTxDesc>(), 16);
        assert_eq!(core::mem::align_of::<PcnetInitBlock>(), 1);
        assert_eq!(core::mem::align_of::<PcnetRxDesc>(), 1);
        assert_eq!(core::mem::align_of::<PcnetTxDesc>(), 1);
    }

    #[test]
    fn test_pcnet_buffer_length_twos_complement() {
        let len: usize = 1536;
        let complement = (-(len as i16) as u16) | 0xF000;
        assert_eq!(complement, 0xFA00);
    }
}
