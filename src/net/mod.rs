//! Jinn OS — Subsistema de Rede em Camadas (L2/L3/L4/Sockets/Firewall)

pub mod arp;
pub mod dhcp;
pub mod diagnostics;
pub mod dns;
pub mod ethernet;
pub mod icmp;
pub mod ipv4;
pub mod jinnwall;
pub mod pki;
pub mod socket;
pub mod tcp;
pub mod tls13;
pub mod udp;
pub mod x509;

pub use diagnostics::NET_DIAGNOSTICS;
pub use pki::{TrustStore, TrustAnchor, CertificateValidationError, validate_chain, validate_certificate, verify_hostname_match};
pub use x509::{ParsedCertificate, PublicKey, SignatureAlgorithm, parse_der_certificate, x509_date_to_unix};

use ethernet::{EthernetFrame, ETHERTYPE_ARP, ETHERTYPE_IPV4};
use ipv4::{Ipv4Packet, PROTO_ICMP, PROTO_TCP, PROTO_UDP};
use jinnwall::{FilterVerdict, JINNWALL};

/// Inicializa a pilha de rede e descobre controladores NIC.
pub fn init() {
    let has_nic = crate::drivers::net::init();
    if has_nic {
        if let Some(dev) = crate::drivers::net::get_device() {
            let mac = dev.mac_address();
            let nic_name = crate::drivers::net::active_nic_name();
            crate::serial_write(b"[NET-INIT] Driver ativo detectado: ");
            crate::serial_write(nic_name.as_bytes());
            crate::serial_write(b" (MAC ");
            let hex = b"0123456789abcdef";
            for (i, &b) in mac.iter().enumerate() {
                if i > 0 { crate::serial_write(b":"); }
                crate::serial_write(&[hex[(b >> 4) as usize], hex[(b & 0x0F) as usize]]);
            }
            crate::serial_write(b")\n");

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
        arp::ARP_CACHE.init_defaults();
    }
    dns::init_tables();

    // Dispara requisição ARP proativa para o default gateway
    arp::send_arp_request([10, 0, 2, 2]);
}

static NET_POLL_LOCK: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
static LAST_NET_POLL_TICK: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static NET_WAITERS: [core::sync::atomic::AtomicUsize; 8] = [
    core::sync::atomic::AtomicUsize::new(0), core::sync::atomic::AtomicUsize::new(0),
    core::sync::atomic::AtomicUsize::new(0), core::sync::atomic::AtomicUsize::new(0),
    core::sync::atomic::AtomicUsize::new(0), core::sync::atomic::AtomicUsize::new(0),
    core::sync::atomic::AtomicUsize::new(0), core::sync::atomic::AtomicUsize::new(0),
];
static NET_WAITER_COUNT: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

#[inline]
pub fn register_net_waiter(pid: usize) {
    if pid == 0 { return; }
    for slot in NET_WAITERS.iter() {
        let cur = slot.load(core::sync::atomic::Ordering::Relaxed);
        if cur == pid {
            return;
        }
    }
    for slot in NET_WAITERS.iter() {
        if slot.compare_exchange(0, pid, core::sync::atomic::Ordering::AcqRel, core::sync::atomic::Ordering::Relaxed).is_ok() {
            NET_WAITER_COUNT.fetch_add(1, core::sync::atomic::Ordering::Release);
            return;
        }
    }
}

#[inline]
pub fn unregister_net_waiter(pid: usize) {
    if pid == 0 { return; }
    for slot in NET_WAITERS.iter() {
        if slot.compare_exchange(pid, 0, core::sync::atomic::Ordering::AcqRel, core::sync::atomic::Ordering::Relaxed).is_ok() {
            NET_WAITER_COUNT.fetch_sub(1, core::sync::atomic::Ordering::Release);
        }
    }
}

/// Acorda tarefas dormentes/bloqueadas aguardando RX de rede (seguro fora ou dentro de ISR via IrqSafeSpinLock).
pub fn wake_net_waiters() {
    if NET_WAITER_COUNT.load(core::sync::atomic::Ordering::Acquire) == 0 {
        return;
    }
    for slot in NET_WAITERS.iter() {
        let pid = slot.load(core::sync::atomic::Ordering::Acquire);
        if pid != 0 {
            crate::scheduler::wake_task(pid);
        }
    }
}

/// Bloqueia/dorme a tarefa atual até chegada de IRQ de rede ou expiração de `timeout_ticks` (Zero-Idle).
pub fn wait_for_rx_or_tick(timeout_ticks: u64) {
    let pid = crate::scheduler::current_pid();
    register_net_waiter(pid);
    if !has_pending_work() {
        let wake_tick = crate::timer::ticks().saturating_add(timeout_ticks.max(1));
        crate::scheduler::get().sleep_task(pid, wake_tick);
        crate::scheduler::yield_now();
    }
    unregister_net_waiter(pid);
    poll_if_needed();
}

/// Informa se a ISR de rede ou a fila loopback sinalizou pacotes pendentes para processamento imediato.
#[inline]
pub fn has_pending_work() -> bool {
    crate::drivers::net::NET_IRQ_PENDING.load(core::sync::atomic::Ordering::Acquire)
        || tcp::has_loopback_pending()
}

#[inline]
fn dhcp_is_bound() -> bool {
    unsafe { matches!(dhcp::DHCP_CLIENT.state, dhcp::DhcpState::Bound(_)) }
}

/// Processa rede sob demanda via IRQ (Zero-Idle), restringindo fallback apenas à janela inicial ou waiters ativos.
pub fn poll_if_needed() {
    let pending = crate::drivers::net::NET_IRQ_PENDING.swap(false, core::sync::atomic::Ordering::AcqRel);
    let loopback = tcp::has_loopback_pending();
    let ticks = crate::timer::ticks();
    if pending || loopback {
        LAST_NET_POLL_TICK.store(ticks, core::sync::atomic::Ordering::Relaxed);
        poll();
        return;
    }
    let verified = crate::drivers::net::NET_IRQ_VERIFIED.load(core::sync::atomic::Ordering::Acquire);
    let active_waiters = NET_WAITER_COUNT.load(core::sync::atomic::Ordering::Acquire) > 0;
    let bound = dhcp_is_bound();
    if verified && bound && !active_waiters {
        return;
    }
    let last = LAST_NET_POLL_TICK.load(core::sync::atomic::Ordering::Relaxed);
    let fallback_interval = if !verified {
        5
    } else if !bound {
        25
    } else {
        50
    };
    if ticks.saturating_sub(last) >= fallback_interval {
        LAST_NET_POLL_TICK.store(ticks, core::sync::atomic::Ordering::Relaxed);
        poll();
    }
}

/// Processa pacotes de rede recebidos da placa física e barramento loopback.
pub fn poll() {
    // Limpa pendência antes de drenar o anel para evitar lost-wakeup caso novo pacote chegue durante a leitura
    crate::drivers::net::NET_IRQ_PENDING.store(false, core::sync::atomic::Ordering::Release);
    // Processa tráfego de loopback interno independente do hardware de rede
    tcp::drain_loopback();

    // Garante exclusão mútua em SMP para evitar corrupção de anéis VirtIO e condições de corrida de sockets
    if NET_POLL_LOCK.compare_exchange(false, true, core::sync::atomic::Ordering::Acquire, core::sync::atomic::Ordering::Relaxed).is_err() {
        return;
    }

    struct NetPollGuard;
    impl Drop for NetPollGuard {
        fn drop(&mut self) {
            NET_POLL_LOCK.store(false, core::sync::atomic::Ordering::Release);
        }
    }
    let _guard = NetPollGuard;

    let dev = match crate::drivers::net::get_device() {
        Some(d) => d,
        None => return,
    };

    let our_mac = dev.mac_address();
    let mut rx_buf = [0u8; 1536];
    let mut drained_any = false;

    while let Some(len) = dev.receive(&mut rx_buf) {
        drained_any = true;
        if len < 14 { continue; }

        NET_DIAGNOSTICS.record_rx_ethernet(len);

        let frame = match EthernetFrame::parse(&rx_buf[..len]) {
            Ok(f) => f,
            Err(_) => {
                NET_DIAGNOSTICS.record_rx_error();
                continue;
            }
        };

        match frame.ethertype {
            ETHERTYPE_ARP => {
                if let Some(arp) = arp::parse_arp(frame.payload) {
                    let ticks = crate::timer::ticks().max(1);
                    unsafe {
                        arp::ARP_CACHE.insert(arp.sender_ip, arp.sender_mac, ticks);
                    }
                    NET_DIAGNOSTICS.record_arp_reply_rcvd();

                    let gw = unsafe { dhcp::DHCP_CLIENT.gateway };
                    let gw_ip = if gw != [0, 0, 0, 0] { gw } else { [10, 0, 2, 2] };
                    if arp.sender_ip == gw_ip {
                        arp::log_gateway_mac(gw_ip, arp.sender_mac);
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
                    NET_DIAGNOSTICS.ip_rx_packets.fetch_add(1, core::sync::atomic::Ordering::Relaxed);

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
                        NET_DIAGNOSTICS.firewall_drops.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
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
                            NET_DIAGNOSTICS.udp_rx_packets.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
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
                            } else {
                                NET_DIAGNOSTICS.record_tcp_csum_error();
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

    if drained_any {
        wake_net_waiters();
    }
}
