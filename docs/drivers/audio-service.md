# Audio Service — Jinn OS

## 1. Visão Geral

O `Audio Service` provê gerenciamento de dispositivos de áudio, mixers, rotas, políticas de baixa latência e integração com drivers e o Scheduler para garantir reprodução e captura com qualidade e determinismo.

## 2. Objetivos

- Baixa latência para reprodução/entrada em tempo real.
- Isolamento entre aplicações (mixers por tenant) e políticas de QoS.
- Sincronização de clocks, suporte a sample-rate conversion e offload quando disponível.

## 3. Responsabilidades

- Gerenciar pipelines de áudio (capture -> processing -> render).
- Coordenar buffers de DMA com Memory Core e Driver Manager.
- Expor APIs para configuração de mixers, volumes, rotas e políticas de prioridade.

## 4. Arquitetura Interna

- `Stream Manager`: cria/gerencia streams de entrada/saída.
- `Mixer`: compõe múltiplos streams com políticas de prioridade.
- `Clock Sync`: mantém sincronização entre dispositivos e aplicações.
- `Latency Controller`: políticas para garantir deadlines.

Topo:

  [App] -> [Audio API] -> [Stream Manager] -> [Mixer] -> [Driver Manager]
                                      |-> [Scheduler hints]
                                      |-> [Memory Core DMA Pools]

## 5. Estruturas de Dados

- `Stream { id, owner, sample_rate, channels, format, buffer_handle, priority }`
- `Clock { domain, skew, offset }`
- `Pipeline { nodes[], latency_budget }`

## 6. Fluxo de Funcionamento

1. Play flow
   - App requests stream; Audio Service allocates DMA buffer and registers with driver.
   - Scheduler may be hinted to prioritize audio processing threads.

2. Mix
   - Mixer pulls frames from active streams, applies resampling, effects and writes to output buffer.

3. Underflow handling
   - Preemptive warming via Predictive Engine; fallback silence and notify application.

## 7. Interfaces Públicas

- `audio_open_stream(params)`
- `audio_write(stream, frames)` / `audio_read(stream, frames)`
- `audio_set_route(stream, device)`
- `audio_subscribe_events()` — underflow/overflow/latency alerts

APIs exigem capabilities quando acessam hardware direto; aplicações de usuário recebem handles limitados.

## 8. Integração com outros componentes

- Driver Manager: provisioning de drivers e configurações de hw-params.
- Memory Core: alocação de buffers DMA com baixos saltos de cache.
- Scheduler: hints de afinidade e prioridade para reduzir jitter.
- Predictive Engine: pre-warming e previsão de picos para evitar underflow.

## 9. Segurança

- Isolamento de streams por tenant; políticas para acesso a dispositivos exclusivos.
- Rate limiting e quotas para impedir DoS por streams maliciosos.

## 10. Escalabilidade

- Escalar por número de streams e por devices; per-node mixing e cross-node forwarding para clusters.

## 11. Futuras Evoluções

- Offload DSP pipelines para aceleradores (GPU/DSP) com sandboxed plugins.
- ML-based jitter smoothing and adaptive buffering.

## 12. Comparação com sistemas modernos

- Pipewire/JACK: Jinn busca oferecer similaridade em flexibilidade e baixa-latência, mas com isolamento e capacidades fortes inerentes ao microkernel.

## 13. Pseudocódigo

```pseudo
open_stream(params):
  if not security.authorized(requester, AUDIO):
    return DENIED
  buf = mm.reserve_dma_pool(params.buffer_size)
  stream = stream_manager.create(requester, params, buf)
  scheduler.hint_affinity(stream.thread)
  return stream.id

mix_tick():
  active = streams.active()
  frames = mixer.pull_frames(active)
  for out_dev in outputs:
    driver.submit(out_dev, frames)

```

## 14. Diagramas ASCII

Play path:

  [App] -> [Audio API] -> [Stream Manager] -> [Mixer] -> [Driver]
                                          |
                                   [Memory Core DMA Pool]

## 15. Considerações para implementação em Rust

- Crates: `jinn-audio-core`, `jinn-audio-mixer`, `jinn-audio-clock`.
- Real-time constraints: evitar allocations on hot path; usar ring buffers e pre-allocated pools.
- Isolar unsafe para manipulação de buffers DMA e interação com drivers.
- Testes: latency/jitter benchmarks, stress tests com muitos streams.

---

Arquivo: [Audio Service](./audio-service.md)
