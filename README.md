# Jinn Kernel

Jinn é um sistema operacional experimental em Rust focado em pré-carregamento e desempenho.

## Visão geral

- Kernel em Rust usando `#![no_std]` e `#![no_main]`
- Inicialização básica via VGA texto
- Módulos de memória, timer e scheduler
- O objetivo é reduzir configurações desnecessárias e manter o sistema enxuto para a comunidade revisar e manter.

## Estrutura do projeto

- `src/` - kernel principal e módulos ativos
- `kernel/` - outro crate de kernel legado ou auxiliar
- `boot/`, `iso_root/` e `limine/` - infra de boot e configuração
- `docs/` - design e especificações de kernel e serviços

## Como compilar

1. Instale a toolchain `nightly` com `rustup`.
2. Configure o target com `rustup target add x86_64-unknown-none`.
3. Execute:

```powershell
cargo build --release --target x86_64-unknown-none
```

## Como rodar

Atualmente há scripts PowerShell de build e run em `scripts/build.ps1` e `scripts/run.ps1`.

## Próximos passos

- Adicionar suporte de interrupções e PIC
- Desenvolver inicialização de memória mais completa
- Incluir testes e documentação de boot
