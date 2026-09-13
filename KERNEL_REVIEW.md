# Kernel Review - Jinn

## Estado atual

O kernel ativo do projeto está no crate raiz `jinn`, com a implementação em `src/`.

Arquivos principais:
- `src/main.rs` - entry point do kernel e inicialização
- `src/vga.rs` - saída de texto via VGA `0xb8000`
- `src/memory.rs` - bump allocator com heap de 16 KiB
- `src/timer.rs` - contador de ticks atômico
- `src/scheduler.rs` - scheduler round-robin simples

## Observações

- O projeto compila com `cargo build --release --target x86_64-unknown-none`.
- `src/vga.rs` já fornece uma base estável para mensagens de boot em modo texto.
- `src/memory.rs` define um allocator funcional, mas a função `allocate` ainda não é usada no kernel.
- `src/timer.rs` implementa um contador de ticks em memória partilhada, útil para controle básico de tempo.
- `src/scheduler.rs` mantém estados de tarefas e faz rondas simples, porém ainda não realiza troca de contexto real.

## Pontos de melhoria

- `src/main.rs` declara `KERNEL_NAME` e `KERNEL_VERSION`, mas eles não são exibidos atualmente.
- `src/memory.rs` contém funções não utilizadas (`allocate`, `align_up`), que podem ser ativadas quando houver alocação dinâmica.
- `src/scheduler.rs` inclui variantes de estado `Waiting` e `Completed` que ainda não são usadas.
- O loop principal em `src/main.rs` usa `spin_loop` para simular tempo; será necessário usar interrupções reais para um kernel mais realista.

## Observações sobre o layout do repositório

- Existe um crate auxiliar/legado em `kernel/` com `kernel/Cargo.toml` e `kernel/src/main.rs`.
- O build e os scripts atuais (`scripts/build.ps1`, `scripts/run.ps1`) operam sobre o crate raiz `jinn`, não sobre `kernel/jinn_kernel`.

## Recomendações para publicar no GitHub

1. manter `README.md` com instruções de compilação e execução.
2. documentar quais crates são ativos e quais são artefatos legados (`kernel/` fica legível como histórico ou removível).
3. incluir `.gitignore` para artefatos de build e imagens de boot.
4. adicionar um `CONTRIBUTING.md` quando houver comunidade interessada.

## Conclusão

O kernel atual já tem uma base funcional de boot e componentes básicos. A implementação está pronta para revisão pública, com a recomendação de limpar o histórico de crates legados e avançar para interrupções e troca de contexto.
