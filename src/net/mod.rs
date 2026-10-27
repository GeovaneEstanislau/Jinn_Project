//! Jinn OS — Subsistema de Rede em Camadas (L2/L3/L4/Sockets/Firewall)

pub mod arp;
pub mod dhcp;
pub mod dns;
pub mod ethernet;
pub mod icmp;
pub mod ipv4;
pub mod jinnwall;
pub mod socket;
pub mod tcp;
pub mod udp;

use ethernet::{EthernetFrame, ETHERTYPE_ARP, ETHERTYPE_IPV4};
use ipv4::{Ipv4Packet, PROTO_ICMP, PROTO_TCP, PROTO_UDP};
use jinnwall::{FilterVerdict, JINNWALL};

/// Inicializa a pilha de rede e descobre controladores NIC.
pub fn init() {
    let has_virtio = crate::drivers::net::virtio::init();
    if has_virtio {
        if let Some(dev) = crate::drivers::net::virtio::get_device() {
            let mac = dev.mac_address();
            // Dispara DHCP Discover na inicialização
            let mut discover_buf = [0u8; 1536];
            let dhcp_len = dhcp::build_dhcp_discover(&mut discover_buf[42..], mac);

            ethernet::EthernetFrame::write_header(
                &mut discover_buf[0..14],
                ethernet::BROADCAST_MAC,
                mac,
                ETHERTYPE_IPV4,
            );
            ipv4::Ipv4Packet::write_header(
                &mut discover_buf[14..34],
                [0, 0, 0, 0],
                [255, 255, 255, 255],
                PROTO_UDP,
                8 + dhcp_len,
            );
            udp::UdpPacket::write_header(
                &mut discover_buf[34..42],
                dhcp::DHCP_CLIENT_PORT,
                dhcp::DHCP_SERVER_PORT,
                dhcp_len,
            );

            let total_len = 14 + 20 + 8 + dhcp_len;
            let _ = dev.transmit(&discover_buf[..total_len]);
        }
    }

    unsafe {
        dns::DNS_CACHE.init_defaults();
    }
}

/// Processa pacotes de rede recebidos da placa física.
pub fn poll() {
    let dev = match crate::drivers::net::virtio::get_device() {
        Some(d) => d,
        None => return,
    };

    let our_mac = dev.mac_address();
    let mut rx_buf = [0u8; 1536];

    while let Some(len) = dev.receive(&mut rx_buf) {
        if len < 14 { continue; }

        let frame = match EthernetFrame::parse(&rx_buf[..len]) {
            Ok(f) => f,
            Err(_) => continue,
        };

        match frame.ethertype {
            ETHERTYPE_ARP => {
                if let Some(arp) = arp::parse_arp(frame.payload) {
                    let ticks = crate::timer::ticks();
                    unsafe {
                        arp::ARP_CACHE.insert(arp.sender_ip, arp.sender_mac, ticks);
                    }
                    // Se for requisição ARP procurando nosso IP, responde
                    let our_ip = unsafe { dhcp::DHCP_CLIENT.ip };
                    if arp.opcode == arp::ARP_OP_REQUEST && arp.target_ip == our_ip {
                        let mut reply_frame = [0u8; 1536];
                        EthernetFrame::write_header(
                            &mut reply_frame[0..14],
                            arp.sender_mac,
                            our_mac,
                            ETHERTYPE_ARP,
                        );
                        let arp_len = arp::build_arp_reply(
                            &mut reply_frame[14..],
                            our_mac,
                            our_ip,
                            arp.sender_mac,
                            arp.sender_ip,
                        );
                        let _ = dev.transmit(&reply_frame[..(14 + arp_len)]);
                    }
                }
            }
            ETHERTYPE_IPV4 => {
                if let Ok(ip_pkt) = Ipv4Packet::parse(frame.payload) {
                    let ticks = crate::timer::ticks();
                    unsafe {
                        arp::ARP_CACHE.insert(ip_pkt.header.src_ip, frame.src_mac, ticks);
                    }
                    let (src_port, dst_port) = match ip_pkt.header.protocol {
                        PROTO_UDP => {
                            if ip_pkt.payload.len() < 8 {
                                continue; // Pacote UDP truncado
                            }
                            (
                                u16::from_be_bytes([ip_pkt.payload[0], ip_pkt.payload[1]]),
                                u16::from_be_bytes([ip_pkt.payload[2], ip_pkt.payload[3]]),
                            )
                        }
                        PROTO_TCP => {
                            if ip_pkt.payload.len() < 20 {
                                continue; // Pacote TCP truncado
                            }
                            (
                                u16::from_be_bytes([ip_pkt.payload[0], ip_pkt.payload[1]]),
                                u16::from_be_bytes([ip_pkt.payload[2], ip_pkt.payload[3]]),
                            )
                        }
                        _ => (0, 0),
                    };

                    // ── Inspeciona pelo Firewall Stateful JinnWall ───────────
                    let verdict = unsafe {
                        JINNWALL.filter_in(
                            ip_pkt.header.src_ip,
                            ip_pkt.header.dst_ip,
                            ip_pkt.header.protocol,
                            src_port,
                            dst_port,
                            ticks,
                        )
                    };

                    if verdict == FilterVerdict::Drop {
                        continue; // Pacote rejeitado pela política do firewall
                    }

                    // ── Despacha para o protocolo de transporte L4 ──────────
                    match ip_pkt.header.protocol {
                        PROTO_ICMP => {
                            icmp::handle_icmp_packet(
                                ip_pkt.payload,
                                ip_pkt.header.src_ip,
                                ip_pkt.header.dst_ip,
                                frame.src_mac,
                                our_mac,
                            );
                        }
                        PROTO_UDP => {
                            if let Ok(udp) = udp::UdpPacket::parse(ip_pkt.payload) {
                                if udp.dst_port == dhcp::DHCP_CLIENT_PORT {
                                    dhcp::handle_dhcp_reply(udp.payload);
                                } else {
                                    if udp.src_port == dns::DNS_PORT {
                                        dns::handle_dns_reply(udp.payload);
                                    }
                                    socket::dispatch_incoming_packet(PROTO_UDP, ip_pkt.header.src_ip, udp.src_port, udp.dst_port, udp.payload);
                                }
                            }
                        }
                        PROTO_TCP => {
                            // Valida checksum com pseudo-cabeçalho IPv4 antes de entregar à máquina de estados RFC 793
                            if tcp::verify_tcp_checksum(ip_pkt.header.src_ip, ip_pkt.header.dst_ip, ip_pkt.payload) {
                                if let Ok(tcp) = tcp::TcpPacket::parse(ip_pkt.payload) {
                                    tcp::handle_incoming_tcp_packet(ip_pkt.header.src_ip, ip_pkt.header.dst_ip, &tcp);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    // Gerencia temporizadores de retransmissão e persistência TCP em background
    let ticks = crate::timer::ticks();
    tcp::poll_tcp_retransmissions(ticks);
}
