# Driver Manifest — Jinn OS

Este documento define o formato recomendado para o manifesto de um driver (metadata que permite ao `Driver Manager` descobrir, validar e configurar o driver).

Exemplo (YAML):

```yaml
name: jinn-example-sound
version: 0.1.0
author: Example Author <dev@example.org>
description: Driver de áudio simples para placas virtuais
compatibility:
  kernel: 
    - "jinn-kernel >= 0.1.0"
  abi: "jinn-driver-v1"
resources:
  - type: pci
    id: "0000:00:1f.3"
permissions:
  - iommu
  - mmio
  - irq
interfaces:
  - name: audio-control
    socket: /run/jinn/drivers/audio-control.sock
optional:
  firmware: jinn/sound-fw-v1.bin

config_schema: |
  type: object
  properties:
    sample_rate:
      type: integer
      default: 48000
    channels:
      type: integer
      default: 2
  required: []

# Hooks
start_command: "/usr/bin/jinn-driver-audio --bind /run/jinn/drivers/audio-control.sock"

# assinatura opcional
signature: "...base64..."
```

Recomendações:
- Usar semântica de versão semântica (`semver`) para `version`.
- Manifestos assinados comprovam a integridade e autoria.
- Validar `config_schema` com um validador JSON Schema antes de aplicar.

Integração com `Driver Manager`:
- O `Driver Manager` deve ler o manifesto, validar capacidades requisitadas, prover IOMMU/DMAs e expor endpoints de controle.

Próximo: quero transformar isso em um template YAML e um exemplo real em `docs/examples/`. Posso prosseguir com isso agora.
