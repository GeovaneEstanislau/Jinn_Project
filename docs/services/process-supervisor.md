# Process Supervisor — Jinn OS

## 1. Visão Geral

O `Process Supervisor` é o serviço central responsável por gerenciar o ciclo de vida de processos e serviços no Jinn OS. Em um design de microkernel onde "tudo é um serviço", o Process Supervisor coordena criação, monitoramento, políticas de reinício, assinaturas de identidade e contratos de capabilities.

Ele opera em espaço de usuário como um serviço privilegiado, comunicando-se via IPC com o Kernel (para alocação de recursos), Security Core (para emissão/validação de capabilities), Scheduler (para hints de criação de threads) e Processos de driver (para supervisão de drivers em espaço de usuário).

## 2. Objetivos

- Gerenciar criação, execuções e término de serviços/processos.
- Garantir isolamento e entrega de capabilities apropriadas a partir do Security Core.
- Implementar políticas de reinício, throttling e limites de recurso por contrato.
- Fornecer mecanismos de observabilidade e health checks.
- Integrar-se com Predictive Engine para antecipar demandas de processo.

## 3. Responsabilidades

- Registrar identidades de serviços e aplicar políticas de inicialização.
- Alocar recursos via Memory Core e solicitar afinidade via Scheduler.
- Emitir e revogar capabilities (via Security Core) para processos.
- Implementar supervisão (watchdogs), reinício baseado em políticas e isolamento por namespace.
- Manter catálogo de serviços ativos, estados e métricas.

## 4. Arquitetura Interna

- `Supervisor Core`: loop principal que recebe requisições IPC e gerencia estado.
- `Launcher`: componente que prepara o ambiente (namespaces, mounts, capabilities) e inicia o processo.
- `Policy Engine`: executa políticas de reinício, backoff, e SLA.
- `Health Monitor`: checa liveness/readiness e coordena ações de recuperação.
- `Registry`: armazenamento de metadados dos serviços (identidade, versão, contratos, quotas).

Arquitetura (alto nível):

  [IPC Listener]
        |
  [Supervisor Core] <-> [Registry]
        |             [Policy Engine]
  [Launcher] <-> [Security Core] <-> [Memory Core]
        |
  [Scheduler / Predictive Engine]

## 5. Estruturas de Dados

- `ServiceRecord`:
  - `id`: UUID
  - `name`: string
  - `exec`: path + args
  - `caps_required`: list<CapabilityDesc>
  - `scheduling_profile`: enum
  - `resource_quota`: {cpu, memory, io}
  - `state`: enum {stopped, starting, running, crashed}
  - `restart_policy`: enum {never, on-failure, always, on-threshold}

- `LaunchContext`:
  - `uid`, `gid`, `namespaces`, `cap_tokens`, `affinity_hint`

- `HealthStatus`:
  - `last_ping`, `status_code`, `metrics` (latency, error_rate)

## 6. Fluxo de Funcionamento

1. Registro/Instalação
   - Administrador registra service manifest via RPC para o Process Supervisor.
   - Supervisor valida manifest (políticas, quotas) e grava em `Registry`.

2. Inicialização
   - `Launcher` solicita capabilities ao Security Core.
   - Memory Core provisiona pools/páginas conforme manifesto.
   - Scheduler recebe hint de afinidade (por exemplo, CPU com dados de NUMA).
   - Processo é iniciado em espaço de usuário com namespaces e capacidades limitadas.

3. Supervisão
   - `Health Monitor` recebe heartbeats via IPC.
   - Em falha, `Policy Engine` decide reinício, isolamento adicional ou quarentena.

4. Terminação
   - Revogação de capabilities, limpeza de mappings e liberação de recursos pelo Memory Core.

## 7. Interfaces Públicas

- `ps_register(manifest)` — registra um serviço.
- `ps_start(service_id)` — inicia serviço.
- `ps_stop(service_id)` — solicita parada graciosa.
- `ps_status(service_id)` — retorna `ServiceRecord` e `HealthStatus`.
- `ps_subscribe_events(filter)` — stream de eventos (start/stop/crash).
- `ps_request_capability(service_id, resource, perms)` — inicia fluxo de emissão via Security Core.

Todas as interfaces expostas via IPC devem aceitar chamadas assinadas e autenticadas; somente administradores ou serviços com privilégio podem alterar registros críticos.

## 8. Integração com outros componentes

- Security Core: para emissão/validação/revogação de capabilities e identidade.
- Memory Core: reserva de pools, mapeamento de páginas e DMA para processos que necessitem.
- Scheduler Core: hints de afinidade, perfis de escalonamento e notificações de carga.
- Predictive Engine: fornece previsões de carga e janelas de prioridade para serviços críticos.
- Driver Manager: coordena reinício e ciclos de vida de drivers em espaço de usuário.

## 9. Segurança

- Princípios:
  - Menor privilégio: serviços recebem apenas capabilities estritamente necessárias.
  - Assinatura e attestation: executáveis e manifests devem estar assinados para confiança aumentada.
  - Isolamento de namespace: cada serviço roda em seu conjunto mínimo de namespaces.

- Mitigações de falhas:
  - Backoff e rate-limiting para reinícios (evita loops de crash).
  - Quarentena para serviços com comportamento anômalo detectado pelo Health Monitor.

## 10. Escalabilidade

- Projeto para escalabilidade horizontal: múltiplas instâncias do Process Supervisor podem atuar em cluster de serviços, com um armazenamento de estado replicado (p.ex. RAFT) para Registry.
- Particionamento por domínio/tenant: cada nó pode hospedar um subconjunto de serviços para reduzir latência de supervisão.

## 11. Futuras Evoluções

- Suporte a políticas declarativas (manifestos que contêm SLAs) e compilação de regras para o `Policy Engine`.
- Integração com orquestradores federados para edge/cloud.
- Fast restarts com snapshot+restore de processo (checkpoint/restore) para reduzir tempo de recuperação.

## 12. Comparação com sistemas modernos

- systemd: similar em papel de orquestração local, mas o `Process Supervisor` do Jinn é um serviço isolado (rodando em userspace com capacidades restritas) e integrado nativamente ao Security Core e Predictive Engine.
- init systems em microkernels (ex.: Managarm): abordagem parecida, porém Jinn enfatiza contratos de capabilities e predição pró-ativa.

## 13. Pseudocódigo

Start flow:

```pseudo
register_service(manifest):
  if not validate_manifest(manifest):
    return error
  id = uuid()
  registry.insert(id, manifest)
  return id

start_service(id):
  record = registry.lookup(id)
  ctx = prepare_launch_context(record)
  caps = security.request_caps(record.caps_required)
  mm.reserve_pools(record.resource_quota)
  scheduler.hint_affinity(record.scheduling_profile)
  pid = launcher.spawn(record.exec, ctx, caps)
  registry.update_state(id, running, pid)
  monitor.watch(pid)
  return pid

on_crash(pid):
  id = registry.lookup_by_pid(pid)
  reason = monitor.fetch_crash_reason(pid)
  action = policy_engine.decide(restart_policy, reason)
  if action == restart:
    backoff_wait(id)
    start_service(id)
  elif action == quarantine:
    isolate_resources(id)
  else:
    registry.update_state(id, stopped)

```

## 14. Diagramas ASCII

Inicialização de serviço:

  [Admin CLI] -> [Process Supervisor Registry] -> [Launcher]
                                      |            |
                                [Security Core] [Memory Core]
                                      |
                                [Scheduler / Predictive]

Fluxo de crash/restart:

  [Process] --crash--> [Health Monitor] -> [Policy Engine] -> [Launcher restart / quarantine]

## 15. Considerações para implementação em Rust

- Organização em crates:
  - `jinn-ps-core` (loop principal, IPC listener)
  - `jinn-ps-launcher` (preparação de ambientes e spawn)
  - `jinn-ps-policy` (módulos de políticas configuráveis)
  - `jinn-ps-monitor` (health checks e métricas)

- Práticas recomendadas:
  - Usar tipos fortes para `ServiceRecord` e `CapabilityDesc`.
  - Separar código seguro (validação, parsing) do código que precisa de `unsafe` (interação com kernel).
  - Testes unitários em user-space para `Policy Engine` e `Registry`.
  - Interfaces IPC definidas com schemas (Cap’n Proto/Flatbuffers/Protobuf) para evitar parsing inseguros.
  - Evitar `unwrap()` em pontos de segurança; propagar erros e falhas observáveis.

---

Arquivo: [Process Supervisor](./process-supervisor.md)
