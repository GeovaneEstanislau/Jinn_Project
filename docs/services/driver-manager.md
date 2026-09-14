# Driver Manager — Jinn OS

## 1. Visão Geral

O `Driver Manager` é o serviço do Jinn responsável por descoberta, carregamento, isolamento, ciclo de vida e atualização de drivers que executam em espaço de usuário. Por design, drivers não correm no kernel; o Driver Manager oferece mecanismos para executar, isolar e comunicar drivers com o resto do sistema via IPC e capacidades.

## 2. Objetivos

- Registrar, validar e iniciar drivers em espaço de usuário.
- Garantir isolamento por namespace e capabilities.
- Fornecer hot-plug, versionamento e atualizações atômicas.
- Coordenar com Process Supervisor, Security Core, Memory Core e Scheduler.
- Minimizar impacto de falhas de drivers no sistema.

## 3. Responsabilidades

- Descoberta e registro de drivers (manifestos, metadados).
- Preparar ambiente de execução (namespaces, cgroups/quota, IOMMU/DMA mappings).
- Supervisionar, reiniciar e aplicar políticas de rate-limit em crashes.
- Gerenciar interfaces de driver (bindings, IPC endpoints) e localizar políticas de acesso.

## 4. Arquitetura Interna

- `Registry`: catálogo de drivers instalados e seus manifests.
- `Launcher`: inicia instâncias de drivers com contexto isolado.
- `Binding Manager`: resolve dependências entre drivers e serviços (ex.: driver de armazenamento e filesystem-service).
- `Policy Engine`: regras de reinício, atualização e throttling.
- `IOMMU/DMA Manager`: coordena com Memory Core e hardware para mappings seguros.

Arquitetura (alto nível):

  [Hardware Events] -> [Driver Manager]
         |                |
     [Probe]        [Registry] <-> [Binding Manager]
                       |                |
                 [Launcher]       [Security Core]
                       |
                 [IOMMU / Memory Core]

## 5. Estruturas de Dados

- `DriverManifest`:
  - `id`, `name`, `version`, `binary_path`, `required_caps`, `resources` (IO, MMIO, irq), `policy`
- `DriverInstance`:
  - `iid`, `manifest_id`, `pid`, `state`, `bound_resources`, `metrics`
- `Binding`:
  - `consumer`, `provider`, `interface_version`, `constraints`

## 6. Fluxo de Funcionamento

1. Descoberta/Instalação
   - Manifesto é registrado; Driver Manager valida assinaturas e requisitos.

2. Probing e Binding
   - Ao detectar hardware (event/ACPI/PCI/USB), Driver Manager localiza driver compatível e resolve bindings.

3. Launch
   - Launcher prepara `LaunchContext` (namespaces, caps, DMA maps) e invoca driver em espaço de usuário.

4. Operação e Supervisão
   - Health Monitor coleta métricas; falhas acionam políticas de reinício/quarentena.

5. Update/Unload
   - Atualizações são aplicadas de forma atômica: criar nova instância, migrar estado (quando possível), trocar endpoints e escolher retire antigo driver.

## 7. Interfaces Públicas

- `dm_register_driver(manifest)` — registra driver.
- `dm_probe(device_info)` — tenta associar device a driver.
- `dm_bind(driver_id, device_id)` — cria binding e inicia driver.
- `dm_unbind(instance_id)` — remove binding e para driver.
- `dm_update(driver_id, new_manifest)` — realiza atualização atômica.

As interfaces são expostas via IPC com autenticação; apenas processos com privilégio de manutenção podem registrar/manipular manifests.

## 8. Integração com outros componentes

- Process Supervisor: Launcher usa APIs do Supervisor para spawn e monitoração.
- Security Core: emissão de capabilities e validação de permissões para acesso a hardware e recursos sensíveis.
- Memory Core / IOMMU: configuração segura de DMA pools e mappings.
- Scheduler: hints de afinidade para drivers sensíveis a latência (por exemplo, drivers de GPU/Áudio).
- Filesystem/Network Services: bindings para expor dispositivos a serviços superiores.

## 9. Segurança

- Assinatura de manifests e verificação de integridade do binário.
- Limitação de capabilities: drivers recebem somente as capacidades necessárias.
- Sandboxing: cada driver roda em seu namespace e, quando aplicável, num ambiente com mitigação (seccomp-like, Wasm sandbox).
- Validação de DMA/IOMMU: checar que endereços e ranges estão dentro de políticas aprovadas.

## 10. Escalabilidade

- Suporte a múltiplas instâncias de drivers em diferentes NUMA nodes.
- Registro e vetorização de probes paralelos para hot-plug massivo.
- Partitioning de drivers por domain/tenant para reduzir interferência.

## 11. Futuras Evoluções

- Modelos de drivers em Wasm para maior segurança e facilidade de atualização.
- Checkpoint/restore de estado de drivers para migração e atualizações sem downtime.
- Políticas ML-assisted para decidir reinício/rollback com base em telemetria histórica.

## 12. Comparação com sistemas modernos

- Linux: kernel-mode drivers vs Jinn user-mode drivers — Jinn prioriza segurança, atualizações e isolamento.
- Zircon/Managarm: inspiração em user-mode drivers; Jinn adiciona foco em IOMMU-aware DMA management e updates atômicos.

## 13. Pseudocódigo

Probe & bind flow:

```pseudo
on_device_event(dev):
  candidates = registry.find_drivers_for(dev)
  for d in candidates:
    if security.verify_manifest(d.manifest):
      binding = binding_manager.resolve(d, dev)
      if binding.ok:
        instance = launcher.spawn_driver(d, binding)
        registry.record_instance(instance)
        return instance
  return error_no_driver

update_driver(manifest_new):
  instances = registry.instances_for(manifest_new.id)
  for inst in instances:
    new_inst = launcher.spawn_driver(manifest_new, inst.bound_resources)
    if new_inst.started:
      migrate_state(inst, new_inst)
      retire(inst)

```

## 14. Diagramas ASCII

Device bind sequence:

  [PCI/USB Event] -> [Driver Manager Probe] -> [Registry find] -> [Launcher spawn]
                                                      |                       |
                                                [Security Core]         [Memory/IOMMU]

Update sequence:

  [New Manifest] -> [Validate] -> [Spawn New] -> [Migrate State] -> [Swap Endpoints] -> [Retire Old]

## 15. Considerações para implementação em Rust

- Crates sugeridos:
  - `jinn-dm-registry` (manifests, search)
  - `jinn-dm-launcher` (spawn, namespaces)
  - `jinn-dm-binding` (resolve interfaces)
  - `jinn-dm-iommu` (coordenação com Memory Core)

- Boas práticas:
  - Uso mínimo de `unsafe` isolado a módulos que tocam recursos físicos e IOMMU.
  - Tipos seguros para `DriverManifest` e `ResourceRange`.
  - Validar inputs estritamente e devolver erros observáveis.
  - Testes de integração em user-space para fluxos de binding e update.

---

Arquivo: [Driver Manager](./driver-manager.md)
