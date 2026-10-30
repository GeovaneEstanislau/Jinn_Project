//! Jinn OS — Address Resolution Protocol (ARP) e Tabela de Cache L2

pub const ARP_OP_REQUEST: u16 = 1;
pub const ARP_OP_REPLY:   u16 = 2;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct ArpPacket {
    pub htype: u16,       // 0x0001 (Ethernet)
    pub ptype: u16,       // 0x0800 (IPv4)
    pub hlen: u8,         // 6
    pub plen: u8,         // 4
    pub opcode: u16,      // 1 = Request, 2 = Reply
    pub sender_mac: [u8; 6],
    pub sender_ip: [u8; 4],
    pub target_mac: [u8; 6],
    pub target_ip: [u8; 4],
}

#[derive(Debug, Clone, Copy)]
pub struct ArpEntry {
    pub ip: [u8; 4],
    pub mac: [u8; 6],
    pub updated_tick: u64,
}

pub struct ArpTable {
    pub entries: [Option<ArpEntry>; 16],
}

impl ArpTable {
    pub const fn new() -> Self {
        Self {
            entries: [None; 16],
        }
    }

    pub fn insert(&mut self, ip: [u8; 4], mac: [u8; 6], tick: u64) {
        // Se já existe, atualiza
        for entry_opt in self.entries.iter_mut() {
            if let Some(entry) = entry_opt {
                if entry.ip == ip {
                    entry.mac = mac;
                    entry.updated_tick = tick;
                    return;
                }
            }
        }
        // Senão, insere no primeiro slot livre
        for entry_opt in self.entries.iter_mut() {
            if entry_opt.is_none() {
                *entry_opt = Some(ArpEntry { ip, mac, updated_tick: tick });
                return;
            }
        }
        // Se cheio, substitui o slot 0
        self.entries[0] = Some(ArpEntry { ip, mac, updated_tick: tick });
    }

    pub fn lookup(&self, ip: [u8; 4]) -> Option<[u8; 6]> {
        for entry_opt in self.entries.iter() {
            if let Some(entry) = entry_opt {
                if entry.ip == ip {
                    return Some(entry.mac);
                }
            }
        }
        None
    }
}

pub static mut ARP_CACHE: ArpTable = ArpTable::new();

pub fn parse_arp(bytes: &[u8]) -> Option<ArpPacket> {
    if bytes.len() < core::mem::size_of::<ArpPacket>() {
        return None;
    }
    let p = unsafe { core::ptr::read_unaligned(bytes.as_ptr() as *const ArpPacket) };
    if u16::from_be(p.htype) != 1 || u16::from_be(p.ptype) != 0x0800 {
        return None;
    }
    Some(ArpPacket {
        htype: u16::from_be(p.htype),
        ptype: u16::from_be(p.ptype),
        hlen: p.hlen,
        plen: p.plen,
        opcode: u16::from_be(p.opcode),
        sender_mac: p.sender_mac,
        sender_ip: p.sender_ip,
        target_mac: p.target_mac,
        target_ip: p.target_ip,
    })
}

pub fn build_arp_reply(
    buffer: &mut [u8],
    our_mac: [u8; 6],
    our_ip: [u8; 4],
    target_mac: [u8; 6],
    target_ip: [u8; 4],
) -> usize {
    let p = ArpPacket {
        htype: 1u16.to_be(),
        ptype: 0x0800u16.to_be(),
        hlen: 6,
        plen: 4,
        opcode: ARP_OP_REPLY.to_be(),
        sender_mac: our_mac,
        sender_ip: our_ip,
        target_mac,
        target_ip,
    };
    let sz = core::mem::size_of::<ArpPacket>();
    unsafe {
        core::ptr::copy_nonoverlapping(&p as *const ArpPacket as *const u8, buffer.as_mut_ptr(), sz);
    }
    sz
}

pub fn build_arp_request(
    buffer: &mut [u8],
    our_mac: [u8; 6],
    our_ip: [u8; 4],
    target_ip: [u8; 4],
) -> usize {
    let p = ArpPacket {
        htype: 1u16.to_be(),
        ptype: 0x0800u16.to_be(),
        hlen: 6,
        plen: 4,
        opcode: ARP_OP_REQUEST.to_be(),
        sender_mac: our_mac,
        sender_ip: our_ip,
        target_mac: [0, 0, 0, 0, 0, 0],
        target_ip,
    };
    let sz = core::mem::size_of::<ArpPacket>();
    unsafe {
        core::ptr::copy_nonoverlapping(&p as *const ArpPacket as *const u8, buffer.as_mut_ptr(), sz);
    }
    sz
}

pub fn send_arp_request(target_ip: [u8; 4]) -> bool {
    let dev = match crate::drivers::net::virtio::get_device() {
        Some(d) => d,
        None => return false,
    };
    let our_mac = dev.mac_address();
    let our_ip = unsafe { crate::net::dhcp::DHCP_CLIENT.ip };

    let mut frame = [0u8; 64];
    crate::net::ethernet::EthernetFrame::write_header(
        &mut frame[0..14],
        crate::net::ethernet::BROADCAST_MAC,
        our_mac,
        crate::net::ethernet::ETHERTYPE_ARP,
    );
    let arp_len = build_arp_request(&mut frame[14..], our_mac, our_ip, target_ip);
    dev.transmit(&frame[..(14 + arp_len)]).is_ok()
}

/// Resolve o endereço MAC para o próximo salto de um pacote com destino a `dst_ip`.
/// Se `dst_ip` for externo, busca o MAC do gateway padrão configurado no sistema.
pub fn resolve_mac_for_ip(dst_ip: [u8; 4]) -> [u8; 6] {
    let next_hop = crate::net::ipv4::get_next_hop_ip(dst_ip);
    if let Some(mac) = unsafe { ARP_CACHE.lookup(next_hop) } {
        mac
    } else {
        // Envia requisição ARP para o próximo salto (host local ou gateway)
        send_arp_request(next_hop);
        crate::net::ethernet::BROADCAST_MAC
    }
}
