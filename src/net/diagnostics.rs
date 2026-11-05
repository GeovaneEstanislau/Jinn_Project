//! Jinn OS — Subsistema Unificado de Diagnóstico e Telemetria de Rede (Marco 0.5.0)
//!
//! Fornece contadores atômicos não-bloqueantes para contabilidade rigorosa por camada
//! (L2 Ethernet, ARP, L3 IPv4, L4 UDP, L4 TCP, DNS, Sockets e Firewall SPI) e por
//! controladora (VirtIO, e1000, AMD PCnet). Permite isolar exatamente em qual etapa
//! do pipeline de rede uma falha de conexão ocorreu.

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

#[derive(Debug)]
pub struct NetDiagnostics {
    // ── Camada 2: Ethernet & Controladora Física ─────────────────────────────
    pub rx_packets: AtomicU64,
    pub tx_packets: AtomicU64,
    pub rx_bytes:   AtomicU64,
    pub tx_bytes:   AtomicU64,
    pub rx_errors:  AtomicU64,
    pub tx_errors:  AtomicU64,
    pub dma_errors: AtomicU64,

    // ── Resolução de Endereço: ARP ───────────────────────────────────────────
    pub arp_requests_sent: AtomicU32,
    pub arp_replies_sent:  AtomicU32,
    pub arp_requests_rcvd: AtomicU32,
    pub arp_replies_rcvd:  AtomicU32,
    pub arp_timeouts:      AtomicU32,

    // ── Camada 3: IPv4 ───────────────────────────────────────────────────────
    pub ip_rx_packets:  AtomicU64,
    pub ip_tx_packets:  AtomicU64,
    pub ip_csum_errors: AtomicU32,
    pub ip_dropped:     AtomicU32,

    // ── Camada 4: UDP & Resolução DNS ────────────────────────────────────────
    pub udp_rx_packets:   AtomicU64,
    pub udp_tx_packets:   AtomicU64,
    pub dns_queries_sent: AtomicU32,
    pub dns_replies_rcvd: AtomicU32,
    pub dns_timeouts:     AtomicU32,
    pub dns_nxdomain:     AtomicU32,

    // ── Camada 4: TCP (RFC 793 / RFC 6298) ───────────────────────────────────
    pub tcp_syn_sent:     AtomicU32,
    pub tcp_syn_ack_rcvd: AtomicU32,
    pub tcp_established:  AtomicU32,
    pub tcp_rst_rcvd:     AtomicU32,
    pub tcp_rst_sent:     AtomicU32,
    pub tcp_retransmits:  AtomicU32,
    pub tcp_timeouts:     AtomicU32,
    pub tcp_csum_errors:  AtomicU32,

    // ── Sockets & Firewall SPI ───────────────────────────────────────────────
    pub sockets_opened: AtomicU32,
    pub sockets_closed: AtomicU32,
    pub firewall_drops: AtomicU32,
}

impl NetDiagnostics {
    pub const fn new() -> Self {
        Self {
            rx_packets: AtomicU64::new(0),
            tx_packets: AtomicU64::new(0),
            rx_bytes:   AtomicU64::new(0),
            tx_bytes:   AtomicU64::new(0),
            rx_errors:  AtomicU64::new(0),
            tx_errors:  AtomicU64::new(0),
            dma_errors: AtomicU64::new(0),

            arp_requests_sent: AtomicU32::new(0),
            arp_replies_sent:  AtomicU32::new(0),
            arp_requests_rcvd: AtomicU32::new(0),
            arp_replies_rcvd:  AtomicU32::new(0),
            arp_timeouts:      AtomicU32::new(0),

            ip_rx_packets:  AtomicU64::new(0),
            ip_tx_packets:  AtomicU64::new(0),
            ip_csum_errors: AtomicU32::new(0),
            ip_dropped:     AtomicU32::new(0),

            udp_rx_packets:   AtomicU64::new(0),
            udp_tx_packets:   AtomicU64::new(0),
            dns_queries_sent: AtomicU32::new(0),
            dns_replies_rcvd: AtomicU32::new(0),
            dns_timeouts:     AtomicU32::new(0),
            dns_nxdomain:     AtomicU32::new(0),

            tcp_syn_sent:     AtomicU32::new(0),
            tcp_syn_ack_rcvd: AtomicU32::new(0),
            tcp_established:  AtomicU32::new(0),
            tcp_rst_rcvd:     AtomicU32::new(0),
            tcp_rst_sent:     AtomicU32::new(0),
            tcp_retransmits:  AtomicU32::new(0),
            tcp_timeouts:     AtomicU32::new(0),
            tcp_csum_errors:  AtomicU32::new(0),

            sockets_opened: AtomicU32::new(0),
            sockets_closed: AtomicU32::new(0),
            firewall_drops: AtomicU32::new(0),
        }
    }

    #[inline]
    pub fn record_rx_ethernet(&self, len: usize) {
        self.rx_packets.fetch_add(1, Ordering::Relaxed);
        self.rx_bytes.fetch_add(len as u64, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_tx_ethernet(&self, len: usize) {
        self.tx_packets.fetch_add(1, Ordering::Relaxed);
        self.tx_bytes.fetch_add(len as u64, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_rx_error(&self) {
        self.rx_errors.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_tx_error(&self) {
        self.tx_errors.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_dma_error(&self) {
        self.dma_errors.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_arp_request_sent(&self) {
        self.arp_requests_sent.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_arp_reply_rcvd(&self) {
        self.arp_replies_rcvd.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_arp_timeout(&self) {
        self.arp_timeouts.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_dns_query_sent(&self) {
        self.dns_queries_sent.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_dns_reply_rcvd(&self) {
        self.dns_replies_rcvd.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_dns_timeout(&self) {
        self.dns_timeouts.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_dns_nxdomain(&self) {
        self.dns_nxdomain.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_tcp_syn_sent(&self) {
        self.tcp_syn_sent.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_tcp_syn_ack_rcvd(&self) {
        self.tcp_syn_ack_rcvd.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_tcp_established(&self) {
        self.tcp_established.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_tcp_rst_rcvd(&self) {
        self.tcp_rst_rcvd.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_tcp_retransmit(&self) {
        self.tcp_retransmits.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_tcp_timeout(&self) {
        self.tcp_timeouts.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_tcp_csum_error(&self) {
        self.tcp_csum_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Reinicia todos os contadores de telemetria e diagnóstico para zero.
    pub fn reset(&self) {
        self.rx_packets.store(0, Ordering::Relaxed);
        self.tx_packets.store(0, Ordering::Relaxed);
        self.rx_bytes.store(0, Ordering::Relaxed);
        self.tx_bytes.store(0, Ordering::Relaxed);
        self.rx_errors.store(0, Ordering::Relaxed);
        self.tx_errors.store(0, Ordering::Relaxed);
        self.dma_errors.store(0, Ordering::Relaxed);

        self.arp_requests_sent.store(0, Ordering::Relaxed);
        self.arp_replies_sent.store(0, Ordering::Relaxed);
        self.arp_requests_rcvd.store(0, Ordering::Relaxed);
        self.arp_replies_rcvd.store(0, Ordering::Relaxed);
        self.arp_timeouts.store(0, Ordering::Relaxed);

        self.ip_rx_packets.store(0, Ordering::Relaxed);
        self.ip_tx_packets.store(0, Ordering::Relaxed);
        self.ip_csum_errors.store(0, Ordering::Relaxed);
        self.ip_dropped.store(0, Ordering::Relaxed);

        self.udp_rx_packets.store(0, Ordering::Relaxed);
        self.udp_tx_packets.store(0, Ordering::Relaxed);
        self.dns_queries_sent.store(0, Ordering::Relaxed);
        self.dns_replies_rcvd.store(0, Ordering::Relaxed);
        self.dns_timeouts.store(0, Ordering::Relaxed);
        self.dns_nxdomain.store(0, Ordering::Relaxed);

        self.tcp_syn_sent.store(0, Ordering::Relaxed);
        self.tcp_syn_ack_rcvd.store(0, Ordering::Relaxed);
        self.tcp_established.store(0, Ordering::Relaxed);
        self.tcp_rst_rcvd.store(0, Ordering::Relaxed);
        self.tcp_rst_sent.store(0, Ordering::Relaxed);
        self.tcp_retransmits.store(0, Ordering::Relaxed);
        self.tcp_timeouts.store(0, Ordering::Relaxed);
        self.tcp_csum_errors.store(0, Ordering::Relaxed);

        self.sockets_opened.store(0, Ordering::Relaxed);
        self.sockets_closed.store(0, Ordering::Relaxed);
        self.firewall_drops.store(0, Ordering::Relaxed);
    }
}

pub static NET_DIAGNOSTICS: NetDiagnostics = NetDiagnostics::new();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostics_counters() {
        let diag = NetDiagnostics::new();
        diag.record_rx_ethernet(64);
        diag.record_tx_ethernet(128);
        assert_eq!(diag.rx_packets.load(Ordering::Relaxed), 1);
        assert_eq!(diag.rx_bytes.load(Ordering::Relaxed), 64);
        assert_eq!(diag.tx_packets.load(Ordering::Relaxed), 1);
        assert_eq!(diag.tx_bytes.load(Ordering::Relaxed), 128);

        diag.record_tcp_syn_sent();
        diag.record_tcp_syn_ack_rcvd();
        diag.record_tcp_established();
        assert_eq!(diag.tcp_syn_sent.load(Ordering::Relaxed), 1);
        assert_eq!(diag.tcp_syn_ack_rcvd.load(Ordering::Relaxed), 1);
        assert_eq!(diag.tcp_established.load(Ordering::Relaxed), 1);
    }
}

