//! Jinn OS — Protocolo ICMPv4 (Echo Request / Ping)

pub const ICMP_TYPE_ECHO_REPLY:   u8 = 0;
pub const ICMP_TYPE_ECHO_REQUEST: u8 = 8;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct IcmpHeader {
    pub icmp_type: u8,
    pub code: u8,
    pub checksum: u16, // Big-Endian
    pub id: u16,
    pub seq: u16,
}

pub fn handle_icmp_packet(
    payload: &[u8],
    src_ip: [u8; 4],
    dst_ip: [u8; 4],
    src_mac: [u8; 6],
    our_mac: [u8; 6],
) {
    if payload.len() < 8 {
        return;
    }
    let icmp_type = payload[0];
    let _code = payload[1];

    if icmp_type == ICMP_TYPE_ECHO_REQUEST {
        // Constrói Echo Reply
        let mut reply_buf = [0u8; 1536];
        // 14 bytes Ethernet + 20 bytes IPv4 + payload.len()
        let total_eth_len = 14 + 20 + payload.len();
        if total_eth_len > reply_buf.len() {
            return;
        }

        // 1. Ethernet Header
        crate::net::ethernet::EthernetFrame::write_header(
            &mut reply_buf[0..14],
            src_mac,
            our_mac,
            crate::net::ethernet::ETHERTYPE_IPV4,
        );

        // 2. IPv4 Header
        crate::net::ipv4::Ipv4Packet::write_header(
            &mut reply_buf[14..34],
            dst_ip,
            src_ip,
            crate::net::ipv4::PROTO_ICMP,
            payload.len(),
        );

        // 3. ICMP Payload com Type = 0 (Reply)
        let icmp_dest = &mut reply_buf[34..(34 + payload.len())];
        icmp_dest.copy_from_slice(payload);
        icmp_dest[0] = ICMP_TYPE_ECHO_REPLY;
        icmp_dest[2] = 0; // Zera checksum antes do cálculo
        icmp_dest[3] = 0;

        let chk = crate::net::ipv4::calculate_checksum(icmp_dest).to_be();
        let chk_bytes = chk.to_ne_bytes();
        icmp_dest[2] = chk_bytes[0];
        icmp_dest[3] = chk_bytes[1];

        // 4. Envia pelo driver VirtIO-Net
        if let Some(dev) = crate::drivers::net::virtio::get_device() {
            let _ = dev.transmit(&reply_buf[..total_eth_len]);
        }
    }
}
