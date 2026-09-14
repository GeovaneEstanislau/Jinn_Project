# Documentação do Jinn OS

Este diretório contém a documentação técnica do Jinn OS, organizada por arquitetura, kernel, serviços e drivers.

## Visão Geral do Projeto

- [Project Overview](PROJECT_OVERVIEW.md)
- [Jinn Technical Vision](architecture/jinn-technical-vision.md)

## Kernel

- [Scheduler Core](kernel/scheduler-core.md)
- [Memory Core](kernel/memory-core.md)
- [Security Core](kernel/security-core.md)
- [IPC Core](kernel/ipc-core.md)

## Serviços

- [Process Supervisor](services/process-supervisor.md)
- [Driver Manager](services/driver-manager.md)
- [Cache Manager](services/cache-manager.md)
- [Predictive Engine](services/predictive-engine.md)
- [Filesystem Service](services/filesystem-service.md)
- [Network Service](services/network-service.md)

## Drivers

- [Storage Service](drivers/storage-service.md)
- [USB Service](drivers/usb-service.md)
- [Audio Service](drivers/audio-service.md)
- [GPU Service](drivers/gpu-service.md)

## Estrutura Recomendada

- `architecture/` — visão arquitetural e documentos gerais do Jinn.
- `kernel/` — componentes do microkernel do Jinn.
- `services/` — serviços em espaço de usuário e drivers.
- `rfc/` — propostas formais e especificações.
- `roadmap/` — planejamento de evolução e prioridades.
