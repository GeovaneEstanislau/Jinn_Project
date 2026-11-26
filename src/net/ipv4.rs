//! Jinn OS — Protocolo IPv4 (Camada de Rede L3)

pub const PROTO_ICMP: u8 = 1;
pub const PROTO_TCP:  u8 = 6;
pub const PROTO_UDP:  u8 = 17;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Ipv4Header {
    pub ver_ihl: u8,       // Version (4) + IHL (5) = 0x45
    pub tos: u8,
    pub total_len: u16,    // Big-Endian
    pub id: u16,
    pub flags_frag: u16,
    pub ttl: u8,
    pub protocol: u8,
    pub checksum: u16,     // Big-Endian
    pub src_ip: [u8; 4],
    pub dst_ip: [u8; 4],
}

pub struct Ipv4Packet<'a> {
    pub header: Ipv4Header,
    pub payload: &'a [u8],
}

pub fn calculate_checksum(data: &[u8]) -> u16 {
    let mut sum = 0u32;
    let mut i = 0;
    while i + 1 < data.len() {
        let word = u16::from_be_bytes([data[i], data[i + 1]]) as u32;
        sum = sum.wrapping_add(word);
        i += 2;
    }
    if i < data.len() {
        let word = u16::from_be_bytes([data[i], 0]) as u32;
        sum = sum.wrapping_add(word);
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

impl<'a> Ipv4Packet<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ()> {
        if bytes.len() < 20 {
            return Err(());
        }
        let ver = bytes[0] >> 4;
        let ihl = (bytes[0] & 0x0F) as usize * 4;
        if ver != 4 || ihl < 20 || bytes.len() < ihl {
            return Err(());
        }

        let hdr = unsafe { core::ptr::read_unaligned(bytes.as_ptr() as *const Ipv4Header) };
        let total_len = u16::from_be(hdr.total_len) as usize;
        let packet_len = total_len.min(bytes.len());
        if packet_len < ihl {
            return Err(());
        }

        Ok(Self {
            header: hdr,
            payload: &bytes[ihl..packet_len],
        })
    }

    pub fn write_header(
        dest: &mut [u8],
        src_ip: [u8; 4],
        dst_ip: [u8; 4],
        protocol: u8,
        payload_len: usize,
    ) -> usize {
        let total_len = (20 + payload_len) as u16;
        let mut hdr = Ipv4Header {
            ver_ihl: 0x45, // IPv4, 5 dwords = 20 bytes
            tos: 0,
            total_len: total_len.to_be(),
            id: 0x1234u16.to_be(),
            flags_frag: 0x4000u16.to_be(), // Don't Fragment
            ttl: 64,
            protocol,
            checksum: 0,
            src_ip,
            dst_ip,
        };
        let hdr_bytes = unsafe {
            core::slice::from_raw_parts(&hdr as *const Ipv4Header as *const u8, 20)
        };
        hdr.checksum = calculate_checksum(hdr_bytes).to_be();

        unsafe {
            core::ptr::copy_nonoverlapping(&hdr as *const Ipv4Header as *const u8, dest.as_mut_ptr(), 20);
        }
        20
    }
}

/// Determina se um endereço IP de destino reside na sub-rede local ou requer roteamento via gateway padrão
pub fn is_local_subnet(dst_ip: [u8; 4], our_ip: [u8; 4], netmask: [u8; 4]) -> bool {
    if netmask == [0, 0, 0, 0] || our_ip == [0, 0, 0, 0] {
        return false;
    }
    (dst_ip[0] & netmask[0] == our_ip[0] & netmask[0])
        && (dst_ip[1] & netmask[1] == our_ip[1] & netmask[1])
        && (dst_ip[2] & netmask[2] == our_ip[2] & netmask[2])
        && (dst_ip[3] & netmask[3] == our_ip[3] & netmask[3])
}

/// Determina o IP do próximo salto (Next-Hop IP) para um pacote com destino a `dst_ip`.
/// Se o destino estiver na rede local, o próximo salto é o próprio `dst_ip`.
/// Se o destino for externo (Internet), o próximo salto é o default gateway.
pub fn get_next_hop_ip(dst_ip: [u8; 4]) -> [u8; 4] {
    if dst_ip[0] == 127 {
        return dst_ip;
    }
    let (our_ip, netmask, gateway) = unsafe {
        (
            crate::net::dhcp::DHCP_CLIENT.ip,
            crate::net::dhcp::DHCP_CLIENT.netmask,
            crate::net::dhcp::DHCP_CLIENT.gateway,
        )
    };
    if is_local_subnet(dst_ip, our_ip, netmask) {
        dst_ip
    } else if gateway != [0, 0, 0, 0] {
        gateway
    } else {
        [10, 0, 2, 2]
    }
}

/// Resolve em uma chamada os parâmetros finais de roteamento L3+L2 (`next_hop_ip`, `next_hop_mac`),
/// consultando primeiramente o Fast-Path `HotRouteTable` antes de recorrer ao ARP bloqueante.
pub fn resolve_route_endpoints(dst_ip: [u8; 4]) -> ([u8; 4], [u8; 6]) {
    let nh_ip = get_next_hop_ip(dst_ip);
    if let Some(mac) = crate::net::hot_routes::lookup_mac_for_ip(dst_ip) {
        return (nh_ip, mac);
    }
    let mac = crate::net::arp::resolve_mac_for_ip(dst_ip);
    (nh_ip, mac)
}

/// Constrói diretamente os cabeçalhos Ethernet + IPv4 usando o MAC confirmado do Fast-Path.
pub fn write_fast_route_headers(
    frame_buf: &mut [u8],
    src_mac: [u8; 6],
    dst_mac: [u8; 6],
    src_ip: [u8; 4],
    dst_ip: [u8; 4],
    protocol: u8,
    transport_len: usize,
) -> usize {
    if frame_buf.len() < 34 {
        return 0;
    }
    crate::net::ethernet::EthernetFrame::write_header(
        &mut frame_buf[0..14],
        dst_mac,
        src_mac,
        crate::net::ethernet::ETHERTYPE_IPV4,
    );
    Ipv4Packet::write_header(
        &mut frame_buf[14..34],
        src_ip,
        dst_ip,
        protocol,
        transport_len,
    );
    34
}
