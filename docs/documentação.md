# Documentação do Jinn OS

## Propósito

Este documento fornece uma visão geral da documentação técnica do Jinn OS, explica a organização dos arquivos e orienta contribuições futuras.

## Organização de Diretórios

- docs/architecture/ — visão estratégica, princípios e arquitetura de alto nível do Jinn.
- docs/kernel/ — documentos dos componentes do microkernel.
- docs/services/ — serviços em espaço de usuário que oferecem funcionalidades de sistema.
- docs/drivers/ — serviços de driver que gerenciam dispositivos e aceleradores.
- docs/applications/ — guias de RFC e arquitetura de espaço de usuário.
- docs/roadmap/ — planejamento e metas de evolução do projeto.

## Como Navegar

1. Comece por docs/architecture/jinn-technical-vision.md para entender a visão e os princípios do Jinn.
2. Estude os núcleos em docs/kernel/ para ver os subsistemas centrais.
3. Veja os serviços em docs/services/ para entender como a plataforma é construída em espaço de usuário.
4. Consulte docs/drivers/ para compreender o modelo de drivers isolados do Jinn.
5. Use docs/applications/ para políticas de RFC e design de espaço de usuário.
6. Planeje o futuro com docs/roadmap/roadmap.md.

## Convenções

- Todos os documentos seguem uma estrutura comum: Visão Geral, Objetivos, Responsabilidades, Arquitetura Interna, Estruturas de Dados, Fluxo de Funcionamento, Interfaces, Integração, Segurança, Escalabilidade, Futuras Evoluções, Comparações, Pseudocódigo, Diagramas ASCII e Considerações Rust.
- Use termos consistentes como Predictive Engine, Capability, IPC, Microkernel e Service.
- Prefira descrições específicas de Jinn em vez de comparações genéricas.

## Contribuindo

1. Abra um RFC em docs/rfc/ ou use a estrutura docs/applications/rfc.txt como orientação.
2. Mantenha documentos concisos, com exemplos claros e referências internas.
3. Atualize os README.md de cada diretório se adicionar novos documentos.

## Estado Atual

A documentação do Jinn está em evolução. Use docs/README.md como ponto de entrada e verifique se novos subsistemas seguem a arquitetura proposta.
