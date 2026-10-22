//! Jinn OS — Processamento de Frames Ethernet II (Camada de Enlace L2)

pub const ETHERTYPE_IPV4: u16 = 0x0800;
pub const ETHERTYPE_ARP:  u16 = 0x0806;

pub const BROADCAST_MAC: [u8; 6] = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct EthernetHeader {
    pub dst_mac: [u8; 6],
    pub src_mac: [u8; 6],
    pub ethertype: u16, // Big-Endian
}

pub struct EthernetFrame<'a> {
    pub dst_mac: [u8; 6],
    pub src_mac: [u8; 6],
    pub ethertype: u16,
    pub payload: &'a [u8],
}

impl<'a> EthernetFrame<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ()> {
        if bytes.len() < 14 {
            return Err(());
        }
        let mut dst_mac = [0u8; 6];
        dst_mac.copy_from_slice(&bytes[0..6]);
        let mut src_mac = [0u8; 6];
        src_mac.copy_from_slice(&bytes[6..12]);
        let ethertype = u16::from_be_bytes([bytes[12], bytes[13]]);

        Ok(Self {
            dst_mac,
            src_mac,
            ethertype,
            payload: &bytes[14..],
        })
    }

    pub fn write_header(dest: &mut [u8], dst_mac: [u8; 6], src_mac: [u8; 6], ethertype: u16) {
        dest[0..6].copy_from_slice(&dst_mac);
        dest[6..12].copy_from_slice(&src_mac);
        let et_be = ethertype.to_be_bytes();
        dest[12..14].copy_from_slice(&et_be);
    }
}
