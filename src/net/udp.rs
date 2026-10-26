//! Jinn OS — Protocolo UDP (Camada de Transporte L4)

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct UdpHeader {
    pub src_port: u16,
    pub dst_port: u16,
    pub length: u16,
    pub checksum: u16,
}

pub struct UdpPacket<'a> {
    pub src_port: u16,
    pub dst_port: u16,
    pub payload: &'a [u8],
}

impl<'a> UdpPacket<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ()> {
        if bytes.len() < 8 {
            return Err(());
        }
        let src_port = u16::from_be_bytes([bytes[0], bytes[1]]);
        let dst_port = u16::from_be_bytes([bytes[2], bytes[3]]);
        let len = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
        if len < 8 {
            return Err(()); // RFC 768: comprimento total de UDP deve ser pelo menos 8 bytes (cabeçalho)
        }
        let payload_len = len - 8;
        let available = bytes.len() - 8;
        let end = 8 + payload_len.min(available);

        Ok(Self {
            src_port,
            dst_port,
            payload: &bytes[8..end],
        })
    }

    pub fn write_header(dest: &mut [u8], src_port: u16, dst_port: u16, payload_len: usize) -> usize {
        let total_len = (8 + payload_len) as u16;
        let sp = src_port.to_be_bytes();
        let dp = dst_port.to_be_bytes();
        let tl = total_len.to_be_bytes();

        dest[0..2].copy_from_slice(&sp);
        dest[2..4].copy_from_slice(&dp);
        dest[4..6].copy_from_slice(&tl);
        dest[6..8].copy_from_slice(&[0, 0]); // Checksum 0 (opcional em IPv4)
        8
    }
}
