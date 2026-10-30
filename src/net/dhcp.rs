//! Jinn OS — Cliente DHCP para Auto-Configuração de IP

pub const DHCP_SERVER_PORT: u16 = 67;
pub const DHCP_CLIENT_PORT: u16 = 68;

pub const MAGIC_COOKIE: [u8; 4] = [99, 130, 83, 99];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DhcpState {
    Init,
    Discovering,
    Requesting([u8; 4]),
    Bound([u8; 4]),
}

pub struct DhcpConfig {
    pub state: DhcpState,
    pub ip: [u8; 4],
    pub netmask: [u8; 4],
    pub gateway: [u8; 4],
    pub dns: [u8; 4],
}

pub static mut DHCP_CLIENT: DhcpConfig = DhcpConfig {
    state: DhcpState::Init,
    ip: [10, 0, 2, 15], // Padrão seguro para QEMU
    netmask: [255, 255, 255, 0],
    gateway: [10, 0, 2, 2],
    dns: [10, 0, 2, 3],
};

pub fn build_dhcp_discover(buffer: &mut [u8], mac: [u8; 6]) -> usize {
    // Zera buffer
    for b in buffer.iter_mut() { *b = 0; }

    buffer[0] = 1; // Boot Request
    buffer[1] = 1; // Ethernet
    buffer[2] = 6; // MAC Len
    buffer[3] = 0; // Hops

    // XID
    buffer[4..8].copy_from_slice(&[0x12, 0x34, 0x56, 0x78]);
    // Flags (Broadcast 0x8000)
    buffer[10..12].copy_from_slice(&[0x80, 0x00]);
    // Client MAC
    buffer[28..34].copy_from_slice(&mac);

    // Magic Cookie em offset 236
    buffer[236..240].copy_from_slice(&MAGIC_COOKIE);

    // Opções:
    // Opção 53: DHCP Discover (len 1, val 1)
    buffer[240] = 53;
    buffer[241] = 1;
    buffer[242] = 1;

    // Opção 55: Parameter Request List (1=Subnet, 3=Router, 6=DNS)
    buffer[243] = 55;
    buffer[244] = 3;
    buffer[245] = 1;
    buffer[246] = 3;
    buffer[247] = 6;

    // Opção 255: End
    buffer[248] = 255;

    300 // Tamanho padrão de datagrama DHCP
}

pub fn handle_dhcp_reply(payload: &[u8]) {
    if payload.len() < 240 {
        return;
    }
    if payload[236..240] != MAGIC_COOKIE {
        return;
    }
    let yiaddr = [payload[16], payload[17], payload[18], payload[19]];
    let mut msg_type = 0u8;
    let mut subnet_mask = [0u8; 4];
    let mut router_ip = [0u8; 4];
    let mut dns_ip = [0u8; 4];

    let mut i = 240;
    while i < payload.len() {
        let opt = payload[i];
        if opt == 255 { break; }
        if opt == 0 { i += 1; continue; }
        if i + 1 >= payload.len() { break; }
        let opt_len = payload[i + 1] as usize;
        let opt_data = &payload[(i + 2)..((i + 2 + opt_len).min(payload.len()))];

        if opt == 53 && !opt_data.is_empty() {
            msg_type = opt_data[0];
        } else if opt == 1 && opt_data.len() >= 4 {
            subnet_mask = [opt_data[0], opt_data[1], opt_data[2], opt_data[3]];
        } else if opt == 3 && opt_data.len() >= 4 {
            router_ip = [opt_data[0], opt_data[1], opt_data[2], opt_data[3]];
        } else if opt == 6 && opt_data.len() >= 4 {
            dns_ip = [opt_data[0], opt_data[1], opt_data[2], opt_data[3]];
        }
        i += 2 + opt_len;
    }

    unsafe {
        // Se for Offer (2) ou Ack (5)
        if (msg_type == 2 || msg_type == 5) && yiaddr != [0, 0, 0, 0] {
            DHCP_CLIENT.ip = yiaddr;
            if subnet_mask != [0, 0, 0, 0] {
                DHCP_CLIENT.netmask = subnet_mask;
            }
            if router_ip != [0, 0, 0, 0] {
                DHCP_CLIENT.gateway = router_ip;
            }
            if dns_ip != [0, 0, 0, 0] {
                DHCP_CLIENT.dns = dns_ip;
            }
            DHCP_CLIENT.state = DhcpState::Bound(yiaddr);
        }
    }
}
