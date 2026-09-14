# Network Service — Jinn OS

## 1. Visão Geral

O `Network Service` fornece pilhas de rede, encaminhamento, políticas de QoS e integração com serviços distribuídos no Jinn. Implementado em espaço de usuário, o serviço oferece stacks modulares (TCP/IP tradicional, QUIC como transporte moderno), offload control (NIC/Firmware), multiplexação de sockets por capacidades e hooks para o Predictive Engine.

## 2. Objetivos

- Oferecer pilhas de transporte confiáveis e de baixa latência (TCP, QUIC).
- Expor APIs seguras e baseadas em capacidades para serviços e drivers.
- Permitir QoS, isolamento de tráfego por tenant e enforcement de políticas de segurança.
- Integrar com Predictive Engine para auto-escalonamento, prefetch de rotas e tuning de parâmetros (congestion control, pacing).

## 3. Responsabilidades

- Gerenciar interfaces de rede físicas e virtuais (vNICs, containers/namespace).
- Implementar pilhas TCP/IP e QUIC, offloads e acelerações (checksum, TSO, LRO) quando seguro.
- Fornecer encaminhamento, NAT, firewalling e políticas por capability.
- Coletar telemetria e alimentar o Predictive Engine.

## 4. Arquitetura Interna

- `Control Plane`: políticas, ACLs, gerenciamento de interfaces e routing.
- `Data Plane`: encaminhamento rápido, piles de sockets, UDP/TCP/QUIC handlers.
- `Offload Manager`: coordena com Driver Manager / NIC firmware para pagamentos de offload.
- `Telemetry & Policy API`: expose predictions, stats e regras dinâmicas.

Topo:

  [Apps/Services] -> [Network API / Capabilities] -> [Control Plane]
                                         |-> [Data Plane] -> [Driver Manager / NIC]
                                         |-> [Predictive Engine]

## 5. Estruturas de Dados

- `NetInterface { name, mac, mtu, capabilities, rx_queue, tx_queue }`
- `SocketControlBlock { sid, proto, local, remote, state, caps }`
- `FlowEntry { src, dst, sport, dport, protocol, qid, stats }`
- `Route { prefix, next_hop, metric, if }`

## 6. Fluxo de Funcionamento

1. Bind/Connect
   - Serviço obtém `CapabilityToken` que define quais interfaces ou endereços pode usar.
   - API valida token e cria `SocketControlBlock` com políticas aplicadas.

2. Envio de pacote
   - Aplicação escreve; kernel/user-net stack aplica segmentation, checksum.
   - Data Plane consulta Offload Manager para usar TSO/Checksum offload quando permitido.

3. Recepção
   - Pacotes chegam ao driver; Driver Manager entrega via IPC a `Network Service`.
   - Data Plane aplica filtros, classificadores e entrega a sockets autorizados.

4. Routing/Forwarding
   - Control Plane mantém tabela de rotas, atualiza por eventos e pela Predictive Engine (route hints).

## 7. Interfaces Públicas

- `net_open(interface_mask, caps)` — criar vNIC/acl.
- `net_socket(proto, opts)` — abrir socket (TCP/UDP/QUIC) com capability.
- `net_bind(sid, addr)` — associar socket.
- `net_send(sid, buf)` / `net_recv(sid)` — operar sobre buffers.
- `net_route_add(route)` / `net_route_del(route)`.
- `net_subscribe_stats(filter)` — stream de telemetria.

APIs devem ser assinadas e aceitar apenas tokens de capability válidos; operações administrativas exigem privilégios adicionais.

## 8. Integração com outros componentes

- Driver Manager: para configurações de NIC, firmware, SR-IOV e offloads.
- Security Core: valida capabilities e aplica políticas por tenant.
- Predictive Engine: fornece hints de rota, escalonamento de flows e tuning de parâmetros TCP/QUIC.
- Scheduler: hints de afinidade para threads de processamento de pacotes de alta prioridade.
- Cache Manager/Filesystem: para acelerar transferências (zero-copy, mmap backing).

## 9. Segurança

- Isolamento por capability: sockets e interfaces vinculados a tokens.
- Filtro por default-deny: regras minimalistas e WHITELIST para acesso a hardware.
- Proteção contra spoofing: checagem de origem e attestation de drivers para offload.
- Rate limiting e policing para evitar DoS interno.

## 10. Escalabilidade

- Data Plane multi-queue e per-CPU RX/TX processing para throughput.
- Sharding de flows por hash e utilização de eBPF-like programmable dataplane para fast-path.
- Distribuição de funções de rede (ontrol-plane em user-space, data-plane acelerado por NICs ou kernel bypass quando seguro).

## 11. Futuras Evoluções

- QUIC como stack nativo com integração a congestion controllers ML-tuned.
- P4/eBPF offloads controlados por políticas (segurança + verificabilidade).
- Network Function Virtualization (NFV) nativa com chaining de serviços via IPC de alta-performance.

## 12. Comparação com sistemas modernos

- Linux: Linux fornece pilhas robustas e extensíveis; Jinn busca oferecer pilhas user-space com control plane separado, capacidades finas e integração nativa com Predictive Engine.
- BSDs: tradição de pilhas bem testadas; Jinn foca em isolamento, QUIC nativo e offload seguro.

## 13. Pseudocódigo

TCP transmit (simplificado):

```pseudo
net_send_tcp(sid, buf):
  scb = lookup_scb(sid)
  if not security.authorized(scb.caps, SEND):
    return PERMISSION_DENIED
  segs = segment(buf, scb.mss)
  for seg in segs:
    if offload.allowed(scb):
      offload.queue(seg)
    else:
      dataplane.enqueue(seg)

on_ack(ack):
  update_congestion_state(ack)
  if predictive.hint_tune_needed():
    apply_tuning(predictive.get_tuning())

```

QUIC handshake (simplified):

```pseudo
quic_connect(ctx):
  init = quic_make_initial()
  send_udp(ctx.sock, init)
  on_received(pkt):
    if quic_is_crypto(pkt):
      process_crypto(pkt)
    if handshake_complete():
      install_crypto_keys()

```

## 14. Diagramas ASCII

Packet flow (host):

  [NIC] -> [Driver Manager] -> [Network Service Data Plane] -> [Socket]
                                              |
                                         [Offload Manager]

Control loop with Predictive Engine:

  [Telemetry] -> [Predictive Engine] -> [Network Service Control Plane] -> [Tuning / Route Hints]

## 15. Considerações para implementação em Rust

- Crates sugeridos: `jinn-net-control`, `jinn-net-dataplane`, `jinn-net-quic`, `jinn-net-telemetry`.
- Usar `no_std` apenas quando necessário; user-space permite usar crates de criptografia maduras (`ring`, `rustls`) para QUIC.
- Zero-copy: integrar buffer pools com Memory Core e suporte a `mmap`/DMA para NICs.
- Segurança: isolar parsing de frames (specially QUIC/TCP options) em módulos auditáveis; evitar `unwrap()` e validar todos os inputs.
- Testes: fuzzing de parsers, benchmarks de throughput/latency e testes de interoperabilidade com implementações padrão (Linux, quiche).

---

Arquivo: [Network Service](./network-service.md)
