# Diagnóstico do boot do Jinn

Data: 2026-08-20

## Resumo executivo

O projeto já passou por várias validações importantes, mas ainda não está confirmado como bootando corretamente em QEMU/VirtualBox.

O estado atual é:

- O kernel compila com sucesso em modo release para `x86_64-unknown-none`.
- O ELF gerado foi validado como executável (`ET_EXEC`), não como binário dinâmico (`ET_DYN`).
- A ISO é gerada com sucesso via Limine e `xorriso`.
- A boot chain até o carregamento do kernel foi validada parcialmente.
- O boot real do kernel ainda não foi confirmado, pois a execução final em VM não chegou a um estado estável de execução do sistema.

## Arquivos principalmente envolvidos

- [src/main.rs](src/main.rs)
- [src/interrupts.s](src/interrupts.s)
- [src/idt.rs](src/idt.rs)
- [linker.ld](linker.ld)
- [.cargo/config.toml](.cargo/config.toml)
- [iso_root/limine.conf](iso_root/limine.conf)
- [scripts/run.ps1](scripts/run.ps1)

---

## Diagnóstico parcial (confirmado)

### 1) Ambiente de compilação

Verificação concluída:

- `cargo +nightly build --release --target x86_64-unknown-none` conclui com sucesso.
- O build final gera o binário em `target/x86_64-unknown-none/release/jinn`.

Observações:

- O projeto usa `nightly` e target bare-metal `x86_64-unknown-none`.
- Há avisos de compilação, mas não são fatais para a geração do ELF.

### 2) Tipo do ELF

Verificação concluída:

- Leitura direta do cabeçalho do ELF em bytes mostrou:
  - `magic=ELF`
  - `e_type=2`
- O valor `2` corresponde a `ET_EXEC`.

Isso elimina a hipótese de o arquivo estar sendo gerado como binário dinâmico (`ET_DYN`).

### 3) Estrutura da ISO e Limine

Verificação concluída:

- O arquivo [iso_root/limine.conf](iso_root/limine.conf) usa a sintaxe atual do Limine:

```conf
timeout: 0

/Jinn

	protocol: limine

	kernel_path: boot():/boot/jinn_kernel
```

- A ISO é gerada com sucesso pelo script [scripts/run.ps1](scripts/run.ps1) usando `xorriso`.
- O Limine carrega o kernel até o ponto de execução do binário; isto foi observado em execuções anteriores.

### 4) Problema anterior do linker

Verificação concluída:

- O erro `PT_DYNAMIC missing` e o erro de permissões de segmento foi tratado pela revisão do linker e pela separação de segmentos PT_LOAD.
- O atual [linker.ld](linker.ld) foi revisado para remover seções de metadata e evitar sobreposição de permissões:
  - `.text`
  - `.rodata`
  - `.data`
  - `.bss`
  - `/DISCARD/` para remover `.note*`, `.comment*`, `.eh_frame*`, `.gnu_build_id*`, etc.

Resultado verificado:

- O ELF gerado não é mais `ET_DYN`.
- O projeto não está mais falhando na etapa de geração de imagem do ELF.

---

## Diagnóstico total (não confirmado)

### 1) Falha real de boot do kernel

Ainda não foi confirmado que o sistema chegou a executar o código do kernel de forma estável.

O que foi observado:

- Em execuções anteriores houve o erro do Limine: `ELF file type is ET_DYN, but PT_DYNAMIC segment missing`.
- Depois da correção, o tipo passou para `ET_EXEC`, mas não houve confirmação final de boot bem-sucedido.
- O estado atual é: a compilação e empacotamento avançaram, mas o boot final não foi validado em ambiente real de VM.

### 2) Possíveis falhas ainda abertas

Estas são as principais hipóteses para a falha atual de boot:

#### A) Boot stub / ponto de entrada incompleto

O kernel usa `#[no_mangle] pub extern "C" fn _start() -> !` em [src/main.rs](src/main.rs).

Possíveis problemas:

- `_start` ainda não está corretamente configurado para o ambiente bare-metal x86_64.
- Poder haver falta de setup mínimo antes da execução do código Rust.
- `main.rs` pode estar assumindo que o ambiente já está corretamente inicializado quando ainda não está.

#### B) Estado inicial da CPU incorreto

Em kernels bare-metal, o processo de boot exige setup mínimo antes do código Rust principal. Possíveis falhas:

- ausência de controle de `long mode` correto
- ausência de `GDT` adequada
- falha em configurar `CR0`, `CR4` ou `IA32_EFER`
- `rsp` não inicializado corretamente
- stack pointer inválido no primeiro código Rust

#### C) Inicialização de IDT/PIC/PIT antes do estado adequado

O código em [src/main.rs](src/main.rs) chama:

- `memory::init();`
- `timer::init();`
- `pic::remap();`
- `pit::init_hz(100);`
- `idt::init();`
- `sti`

Podem existir problemas de:

- `IDT` no formato errado
- interrupções não configuradas corretamente para o modo x86_64
- stack frame de interrupt handler inconsistente
- `iretq` em [src/interrupts.s](src/interrupts.s) incompatível com o contexto salvo

#### D) Assembly do handler de timer

O arquivo [src/interrupts.s](src/interrupts.s) salva registradores e depois faz `iretq`.

Possíveis falhas:

- alinhamento de pilha errado
- registradores salvos em ordem incompatível com a rotina Rust
- uso do registrador de stack errado em um kernel em `long mode`
- falta de `RSP`/`RFLAGS`/`CS`/`SS` em interrupt context

#### E) Artefatos antigos em VM / ISO

Também precisa ser considerado o risco de o ambiente de teste estar usando um artefato antigo:

- ISO antiga em disco
- VM apontando para uma imagem antiga
- diretório de boot antigo em `iso_root`
- script copiando um binário diferente do esperado

Embora o build atual tenha sido validado, a VM pode ainda estar executando um artefato stale.

#### F) Warnings relevantes de UB

O build emite warnings de `static_mut_refs` e referências a `static mut`. Isso é relevante, porque em kernels bare-metal a operação sobre `static mut` pode causar comportamento indefinido.

Arquivos observados:

- [src/scheduler.rs](src/scheduler.rs)
- [src/idt.rs](src/idt.rs)

Essas warnings não obrigatoriamente quebram o build, mas podem causar falhas difíceis de diagnosticar durante o boot.

---

## Verificações executadas

### Verificação 1: build do kernel

Comando:

```powershell
cargo +nightly build --release --target x86_64-unknown-none
```

Resultado:

- compilação concluída com sucesso
- warnings, mas sem erro fatal de build

### Verificação 2: tipo do ELF

Leitura do cabeçalho do binário gerado.

Resultado:

- `ELF`
- `e_type=2`
- `ET_EXEC`

### Verificação 3: geração da ISO

Comando:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\run.ps1 -NoRun
```

Resultado:

- ISO gerada com sucesso
- gerado em `jinn.iso`

### Verificação 4: execução em QEMU/VirtualBox

Resultado:

- ainda não validado como boot bem-sucedido
- a execução real do kernel não foi comprovada até o momento

---

## Conclusão prática

### Confirmado

- o projeto compila
- o ELF está em formato executável correto para bare-metal
- a ISO é gerada corretamente
- o problema de `ET_DYN` foi eliminado

### Ainda não confirmado

- o kernel entra e executa de forma estável em modo real do sistema operacional

### Hipótese mais provável do problema atual

O problema principal mais provável hoje é que o kernel está falhando na sua fase inicial de execução, antes de entrar no loop principal, e isso pode estar ligado a:

- configuração incorreta do `_start`
- stack / long mode / GDT / IDT iniciais
- interrupções/handlers em assembly
- warnings de `static mut` e UB
- artefatos antigos de ISO/VM

---

## Recomendação de próximos passos

1. Checar o ponto exato de execução do `_start` em [src/main.rs](src/main.rs).
2. Validar se o stack e o estado da CPU estão corretos antes do primeiro Rust.
3. Revisar o handler de interrupção em [src/interrupts.s](src/interrupts.s).
4. Confirmar que a VM está realmente carregando a ISO atual e não uma imagem antiga.
5. Remover ou corrigir o uso de `static mut` em [src/scheduler.rs](src/scheduler.rs) e [src/idt.rs](src/idt.rs) para reduzir riscos de UB.

Este documento representa o diagnóstico atual, parcialmente validado e parcialmente hipotético, com foco em separar o que foi verificado do que ainda precisa ser provado.
