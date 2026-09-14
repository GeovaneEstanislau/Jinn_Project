# USB Service — Jinn OS

## 1. Visão Geral

O `USB Service` gerencia a pilha USB, descoberta de dispositivos, enumeration, policies de binding e exposições seguras para drivers em espaço de usuário. Centraliza hot-plug e políticas de segurança para dispositivos USB.

## 2. Objetivos

- Fornecer descoberta confiável e binding de drivers para dispositivos USB.
- Isolar dispositivos por tenant e aplicar políticas de segurança.
- Suportar power management, hot-plug e firmware updates seguros.

## 3. Responsabilidades

- Detecção de dispositivos, enumeration, e criação de device nodes virtuais.
- Autorização de drivers via `Driver Manager` e `Security Core`.
- Fornecer event stream para aplicações interessadas (hotplug, remove).

## 4. Arquitetura Interna

- `USB Controller Interface`: comunica com driver de host controller.
- `Device Manager`: mantém árvore de dispositivos, descriptors e bindings.
- `Power Manager`: políticas de energia por dispositivo e por bus.
- `Firmware Updater`: canal seguro para atualização de firmware de dispositivos.

Topo:

  [Host Controller Driver] -> [USB Service]
                                    |
                          [Device Manager] -> [Driver Manager Bindings]

## 5. Estruturas de Dados

- `USBDevice { id, vid, pid, class, subclass, protocol, descriptors }`
- `USBPort { id, power_state, speed, device }`
- `BindingRequest { device_id, driver_candidates }`

## 6. Fluxo de Funcionamento

1. Hotplug
   - Host controller sinaliza evento; service faz enumeration e cria `USBDevice`.
   - `Driver Manager` é consultado para binding; security verifica assinatura do driver.

2. Power management
   - Políticas de economia: suspend/resume, autosuspend por inactivity.

3. Firmware update
   - Validar nova imagem, aplicar via secure channel, rollback on failure.

## 7. Interfaces Públicas

- `usb_list()` — lista dispositivos.
- `usb_claim(device_id, caps)` — reserva device para driver.
- `usb_release(device_id)`
- `usb_subscribe_events()` — hotplug events.
- `usb_update_firmware(device_id, image)` — admin only.

## 8. Integração com outros componentes

- Driver Manager: bind drivers and spawn instances.
- Security Core: validate driver manifests and authorize device access.
- Process Supervisor: supervise long-running driver processes.

## 9. Segurança

- Device authorization: default-deny; manual or policy-driven allow list.
- Firmware images must be signed and verified.
- Rate limits and quotas to mitigate malicious devices.

## 10. Escalabilidade

- Designed for many devices but typical host has limited USB buses; lightweight event handling and batching of enumerations.

## 11. Futuras Evoluções

- Integration with IoT attestation services for device identity.
- Support for USB-over-IP and secure remote device proxies.

## 12. Comparação com sistemas modernos

- Linux USB stack: mature and flexible; Jinn moves logic to user-space to allow stronger isolation and policy enforcement.

## 13. Pseudocódigo

```pseudo
on_port_connect(port):
  desc = hcd.read_descriptors(port)
  dev = device_manager.create(desc)
  candidates = registry.find_drivers(dev)
  for c in candidates:
    if security.verify(c):
      dm.bind(c, dev)

claim_device(dev, driver):
  if security.authorized(driver, dev):
    assign(dev, driver)
  else:
    return DENIED

```

## 14. Diagramas ASCII

Hotplug flow:

  [HCD] -> [USB Service] -> [Device Manager] -> [Driver Manager]

## 15. Considerações para implementação em Rust

- Crates: `jinn-usb-core`, `jinn-usb-hcd-adapter`, `jinn-usb-firmware`.
- Enumeration parsing e descriptor handling devem ser fortemente testados e fuzzed.
- Firmware update paths devem isolar parsing e verificação de assinaturas.

---

Arquivo: [USB Service](./usb-service.md)
