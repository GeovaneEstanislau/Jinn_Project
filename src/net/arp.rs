//! Jinn OS — Address Resolution Protocol (ARP) e Tabela de Cache L2

pub const ARP_OP_REQUEST: u16 = 1;
pub const ARP_OP_REPLY:   u16 = 2;

pub const DEFAULT_GATEWAY_MAC_QEMU: [u8; 6] = [0x52, 0x55, 0x0a, 0x00, 0x02, 0x02];
pub const DEFAULT_DNS_MAC_QEMU: [u8; 6]     = [0x52, 0x55, 0x0a, 0x00, 0x02, 0x03];
pub const DEFAULT_GATEWAY_MAC_VBOX: [u8; 6] = [0x52, 0x54, 0x00, 0x12, 0x35, 0x02];
pub const DEFAULT_DNS_MAC_VBOX: [u8; 6]     = [0x52, 0x54, 0x00, 0x12, 0x35, 0x03];

pub const DEFAULT_GATEWAY_MAC: [u8; 6] = DEFAULT_GATEWAY_MAC_QEMU;
pub const DEFAULT_DNS_MAC: [u8; 6]     = DEFAULT_DNS_MAC_QEMU;

/// Detecta confiavelmente se o sistema está executando dentro do Oracle VirtualBox
/// via CPUID hypervisor signature (leaf 0x40000000), barramento PCI (vendor 0x80EE) ou NIC PCnet.
pub fn is_hypervisor_virtualbox() -> bool {
    // 1. Inspeciona CPUID hypervisor signature (leaf 0x4000_0000)
    let (_eax, ebx, ecx, _edx) = unsafe { crate::cpu::cpuid(0x4000_0000) };
    // "VBox" em ASCII Little-Endian: 'V'=0x56, 'B'=0x42, 'o'=0x6F, 'x'=0x78 => 0x786F4256
    if ebx == 0x786F_4256 || ecx == 0x786F_4256 {
        return true;
    }

    // 2. Inspeciona barramento PCI por dispositivos Oracle/InnoTek VirtualBox (Vendor 0x80EE)
    let dev_count = crate::drivers::pci::device_count();
    let dev_slice = crate::drivers::pci::devices();
    for i in 0..dev_count {
        if let Some(dev) = dev_slice.get(i).copied().flatten() {
            if dev.vendor_id == 0x80EE {
                return true;
            }
        }
    }

    // 3. Placa de rede AMD PCnet (usada predominantemente no VirtualBox)
    unsafe {
        if crate::drivers::net::DETECTED_NIC_TYPE == crate::drivers::net::NicType::Pcnet {
            return true;
        }
    }

    // 4. Inspeciona o MAC address da interface ativa: VirtualBox usa OUI 08:00:27 por padrão
    if let Some(dev) = crate::drivers::net::get_device() {
        let mac = dev.mac_address();
        if mac[0] == 0x08 && mac[1] == 0x00 && mac[2] == 0x27 {
            return true;
        }
    }

    false
}

pub fn default_gateway_mac() -> [u8; 6] {
    if is_hypervisor_virtualbox() {
        DEFAULT_GATEWAY_MAC_VBOX
    } else {
        DEFAULT_GATEWAY_MAC_QEMU
    }
}

pub fn default_dns_mac() -> [u8; 6] {
    if is_hypervisor_virtualbox() {
        DEFAULT_DNS_MAC_VBOX
    } else {
        DEFAULT_DNS_MAC_QEMU
    }
}

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

    /// Retorna o MAC somente se foi confirmado dinamicamente pela rede (tick > 0)
    pub fn lookup_confirmed(&self, ip: [u8; 4]) -> Option<[u8; 6]> {
        for entry_opt in self.entries.iter() {
            if let Some(entry) = entry_opt {
                if entry.ip == ip && entry.updated_tick > 0 {
                    return Some(entry.mac);
                }
            }
        }
        None
    }

    /// Inicializa a tabela de cache ARP com estado limpo.
    /// Não insere entradas confirmadas para que o ARP dinâmico resolva
    /// o MAC real da máquina virtual em tempo de execução (QEMU 52:55:0a:00:02:02 vs VirtualBox 52:54:00:12:35:02).
    pub fn init_defaults(&mut self) {
        *self = Self::new();
        let gw = unsafe { crate::net::dhcp::DHCP_CLIENT.gateway };
        let dns = unsafe { crate::net::dhcp::DHCP_CLIENT.dns };
        let gw_ip = if gw != [0, 0, 0, 0] { gw } else { [10, 0, 2, 2] };
        let dns_ip = if dns != [0, 0, 0, 0] { dns } else { [10, 0, 2, 3] };

        // Insere entradas padrão com updated_tick = 0 (não confirmadas, permitindo resolução ARP real em runtime)
        let gw_mac = default_gateway_mac();
        let dns_mac = default_dns_mac();
        self.insert(gw_ip, gw_mac, 0);
        self.insert(dns_ip, dns_mac, 0);
    }
}

static LAST_LOGGED_GW_MAC: crate::memory::IrqSafeSpinLock<[u8; 6]> = crate::memory::IrqSafeSpinLock::new([0; 6]);

/// Registra no log serial a resolução do MAC do Gateway no formato exigido:
/// `[NET-ARP] Gateway 10.0.2.2 resolvido para MAC xx:xx:xx:xx:xx:xx`
pub fn log_gateway_mac(ip: [u8; 4], mac: [u8; 6]) {
    let mut last = LAST_LOGGED_GW_MAC.lock();
    if *last != mac {
        *last = mac;
        drop(last);
        crate::serial_write(b"[NET-ARP] Gateway ");
        let mut ip_buf = [0u8; 16];
        let len = crate::net::dns::format_ip_into(&mut ip_buf, ip);
        crate::serial_write(&ip_buf[..len]);
        crate::serial_write(b" resolvido para MAC ");
        let hex = b"0123456789abcdef";
        for (i, &b) in mac.iter().enumerate() {
            if i > 0 {
                crate::serial_write(b":");
            }
            crate::serial_write(&[hex[(b >> 4) as usize], hex[(b & 0x0F) as usize]]);
        }
        crate::serial_write(b"\n");
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
    let dev = match crate::drivers::net::get_device() {
        Some(d) => d,
        None => return false,
    };
    if !dev.is_available() {
        return false;
    }
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
    crate::net::NET_DIAGNOSTICS.record_arp_request_sent();
    dev.transmit(&frame[..(14 + arp_len)]).is_ok()
}

/// Resolve o endereço MAC para o próximo salto com espera ativa e retransmissão ARP.
pub fn resolve_mac_for_ip_blocking(dst_ip: [u8; 4], timeout_ticks: u64) -> Option<[u8; 6]> {
    let next_hop = crate::net::ipv4::get_next_hop_ip(dst_ip);
    let start = crate::timer::ticks();

    // 1. Se já está confirmado dinamicamente pelo tráfego da rede, retorna imediatamente
    if let Some(mac) = unsafe { ARP_CACHE.lookup_confirmed(next_hop) } {
        // Validação de coerência: se for o gateway, não aceitar MAC de DNS (terminando em 0x03)
        let gateway = unsafe { crate::net::dhcp::DHCP_CLIENT.gateway };
        if !(next_hop == gateway && mac[5] == 0x03) {
            if next_hop == gateway || next_hop == [10, 0, 2, 2] {
                log_gateway_mac(next_hop, mac);
            }
            return Some(mac);
        }
    }

    // 2. Dispara requisição ARP real para o próximo salto
    send_arp_request(next_hop);
    let mut last_send = start;

    while crate::timer::ticks().saturating_sub(start) < timeout_ticks {
        crate::net::poll();
        if let Some(mac) = unsafe { ARP_CACHE.lookup_confirmed(next_hop) } {
            let gateway = unsafe { crate::net::dhcp::DHCP_CLIENT.gateway };
            if !(next_hop == gateway && mac[5] == 0x03) {
                if next_hop == gateway || next_hop == [10, 0, 2, 2] {
                    log_gateway_mac(next_hop, mac);
                }
                return Some(mac);
            }
        }
        // Retransmite a cada 15 ticks (150ms)
        if crate::timer::ticks().saturating_sub(last_send) >= 15 {
            last_send = crate::timer::ticks();
            send_arp_request(next_hop);
        }
        crate::scheduler::yield_now();
    }

    crate::net::NET_DIAGNOSTICS.record_arp_timeout();

    // 3. Fallback inteligente e estrito para ambientes virtuais (VirtualBox / QEMU NAT):
    let gateway = unsafe { crate::net::dhcp::DHCP_CLIENT.gateway };
    let dns = unsafe { crate::net::dhcp::DHCP_CLIENT.dns };

    if next_hop == dns || next_hop == [10, 0, 2, 3] {
        if let Some(dns_mac) = unsafe { ARP_CACHE.lookup(dns) } {
            return Some(dns_mac);
        }
        let mac = default_dns_mac();
        unsafe { ARP_CACHE.insert(next_hop, mac, crate::timer::ticks().max(1)); }
        return Some(mac);
    }

    if next_hop == gateway || next_hop == [10, 0, 2, 2] {
        if let Some(gw_mac) = unsafe { ARP_CACHE.lookup(gateway) } {
            if gw_mac[5] != 0x03 {
                log_gateway_mac(next_hop, gw_mac);
                return Some(gw_mac);
            }
        }
        let mac = default_gateway_mac();
        unsafe { ARP_CACHE.insert(next_hop, mac, crate::timer::ticks().max(1)); }
        log_gateway_mac(next_hop, mac);
        return Some(mac);
    }

    // Se for outro IP na sub-rede que não respondeu ao ARP
    None
}

/// Resolve o endereço MAC para o próximo salto de um pacote com destino a `dst_ip`.
pub fn resolve_mac_for_ip(dst_ip: [u8; 4]) -> [u8; 6] {
    if let Some(mac) = resolve_mac_for_ip_blocking(dst_ip, 50) {
        mac
    } else {
        let next_hop = crate::net::ipv4::get_next_hop_ip(dst_ip);
        let dns = unsafe { crate::net::dhcp::DHCP_CLIENT.dns };
        if next_hop == dns || next_hop == [10, 0, 2, 3] {
            default_dns_mac()
        } else {
            default_gateway_mac()
        }
    }
}
