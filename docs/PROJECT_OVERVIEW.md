# Overview do Projeto Jinn OS

Jinn é um sistema operacional experimental em Rust criado para servir como um kernel mínimo com boot via Limine e um scheduler simples. O objetivo é oferecer uma base clara para estudar boot, infraestrutura de kernel e organização de serviços/arquitetura.

## O que o projeto oferece hoje

- Kernel básico em Rust com `#![no_std]` e `#![no_main]`
- Suporte a VGA texto e mensagens iniciais de boot
- Scheduler cooperativo de tarefas com `yield_now()` e round-robin
- Módulos de timer, memória, PIC/IDT e integração com Limine
- Documentação técnica extensa em `docs/`
- Scripts de build e execução para QEMU

## O que não é suportado ainda

- Não é um sistema operacional pronto para produção
- Suporte a dispositivos reais e drivers é incompleto
- Não há um kernel preemptivo totalmente funcional
- O bootloader do Limine não está integrado como dependência gerenciada
- Recursos de segurança, IPC e serviços ainda são conceituais em muitos casos

## Estrutura do projeto

- `src/` - código do kernel principal
- `boot/` - arquivos de boot estáticos e configuração mínima
- `iso_root/` - raiz de ISO usada pelo script de geração de imagem
- `limine/` - arquivos do Limine baixados/manuais, não fazem parte do código-fonte do kernel
- `docs/` - documentação técnica e roadmap
- `scripts/` - scripts auxiliares para build, criação de ISO e execução
- `kernel/` - crate auxiliar/legado
- `linker.ld`, `target.json` - configurações de linking e target

## Como contribuir

1. Abra uma issue para bugs ou propostas de features.
2. Faça forks e branches descritivos (`feature/`, `fix/`).
3. Mantenha commits pequenos e claros.
4. Atualize a documentação ao adicionar novos conceitos.
5. Execute:

```powershell
rustup toolchain install nightly
rustup target add x86_64-unknown-none
cargo build --release --target x86_64-unknown-none
```

## Observações sobre publicação no GitHub

- Excluir artefatos gerados: `jinn.iso`, `qemu.log`, e `iso_root/boot/jinn_kernel/jinn`
- Ignorar arquivos temporários, binários e o diretório `limine/`
- Manter apenas o código-fonte, scripts, documentação e arquivos de configuração
- Garantir que não haja dados pessoais ou segredos no repositório
