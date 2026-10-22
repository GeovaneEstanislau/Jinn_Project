//! Jinn OS — Protocolo TCP (Camada de Transporte Confiável L4)

pub const TCP_FLAG_FIN: u8 = 0x01;
pub const TCP_FLAG_SYN: u8 = 0x02;
pub const TCP_FLAG_RST: u8 = 0x04;
pub const TCP_FLAG_PSH: u8 = 0x08;
pub const TCP_FLAG_ACK: u8 = 0x10;
pub const TCP_FLAG_URG: u8 = 0x20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpState {
    Closed,
    Listen,
    SynSent,
    SynReceived,
    Established,
    FinWait1,
    FinWait2,
    CloseWait,
    Closing,
    LastAck,
    TimeWait,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct TcpHeader {
    pub src_port: u16,
    pub dst_port: u16,
    pub seq_num: u32,
    pub ack_num: u32,
    pub data_offset_flags: u16, // Data offset (4 bits) + Flags (6 bits)
    pub window_size: u16,
    pub checksum: u16,
    pub urgent_ptr: u16,
}

pub struct TcpPacket<'a> {
    pub src_port: u16,
    pub dst_port: u16,
    pub seq_num: u32,
    pub ack_num: u32,
    pub flags: u8,
    pub window: u16,
    pub payload: &'a [u8],
}

impl<'a> TcpPacket<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ()> {
        if bytes.len() < 20 {
            return Err(());
        }
        let src_port = u16::from_be_bytes([bytes[0], bytes[1]]);
        let dst_port = u16::from_be_bytes([bytes[2], bytes[3]]);
        let seq_num = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let ack_num = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        let doff = ((bytes[12] >> 4) as usize) * 4;
        let flags = bytes[13] & 0x3F;
        let window = u16::from_be_bytes([bytes[14], bytes[15]]);

        if doff < 20 || bytes.len() < doff {
            return Err(());
        }

        Ok(Self {
            src_port,
            dst_port,
            seq_num,
            ack_num,
            flags,
            window,
            payload: &bytes[doff..],
        })
    }
}
