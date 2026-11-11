# JINN OS & ALADDIN DESKTOP — RELATÓRIO DE IMPLEMENTAÇÃO DO ALADDIN EM RING 3
## Migração Efetiva do Compositor Gráfico para Espaço de Usuário

- **Data:** 10 de Novembro de 2026
- **Status:** Homologado com Sucesso
- **Kernel:** Jinn OS Microkernel 0.0.1 (x86_64, Limine Protocol)
- **Serviço Gráfico:** Aladdin Desktop Graphics Service 0.1.0 (`aladdin.elf` / Ring 3)
- **Repositórios:**
  - Jinn Kernel: `E:\Projetos\Jinn`
  - Aladdin: `E:\Projetos\Aladdin`

---

## 1. Sumário Executivo

Este documento relata a conclusão da migração arquitetural do compositor gráfico Aladdin de um módulo residente no núcleo para um **serviço gráfico independente executando em espaço de usuário (Ring 3)**.

A migração elimina categoricamente as quatro dependências críticas identificadas na auditoria arquitetural:
1. **Acesso Direto à VRAM Física:** Substituído pela **Alternativa A (Apresentação Mediada por Syscall de Cópia com Dirty Rects)**. O processo Ring 3 aloca seu backbuffer em seu próprio espaço de endereçamento e transfere scanlines através da Syscall 74 (`sys_graphics_present`), com rigorosa validação de limites de ponteiro de usuário e coordenadas de tela pelo microkernel.
2. **Leitura Privilegiada de Portas PS/2:** Acesso direto a portas I/O (`0x60`, `0x64`) via instruções de CPU `in`/`out` foi totalmente extinto no espaço de usuário. O hardware PS/2 é gerenciado exclusivamente por interrupções e subsistema de drivers em Ring 0; o Aladdin consome eventos padronizados através da Syscall 75 (`sys_graphics_poll_input`).
3. **Gestão de Janelas Concentrada no Kernel:** A máquina de estados do compositor, estruturas de janelas, ordenação z-order, docking, decorações de janelas e animações foram transferidas para a sessão do usuário (`AladdinSession`), executando em Ring 3 no binário `aladdin.elf`. O kernel atua unicamente como broker e intermediador de superfícies e eventos (Syscalls 77, 78 e 79).
4. **Acoplamento de Aplicações Nativas no Compositor:** A infraestrutura de IPC e abstrações de processos clientes (`sys_graphics_list_client_windows`, `sys_graphics_read_client_surface`, `sys_graphics_dispatch_client_event`) foi homologada, permitindo a segregação de Terminal, Monitor de Recursos, Gerenciador de Arquivos e Ajustes em processos clientes independentes (Etapa 2).

Adicionalmente, foi realizada a blindagem transacional da persistência de configurações com salvaguarda rotacional (`.BAK`) e tolerância completa a cortes abruptos de energia elétrica.

---

## 2. Eliminação das Quatro Dependências Críticas

### 2.1. Eliminação do Acesso Direto à VRAM (`fb.address`)
- **Estado Anterior:** O código do desktop em `src/gui.rs` acessava diretamente o endereço físico da VRAM (`limine::framebuffer().unwrap().address`), violando a separação de privilégios e expondo o hardware de vídeo a corrupções de ponteiro do compositor.
- **Implementação em Ring 3:**
  - O Aladdin Graphics Service opera com ponteiros virtuais em Ring 3.
  - Para atualizar a tela, invoca a Syscall 74 passando a estrutura `PresentRect` e a fatia de pixels do buffer do usuário.
  - O kernel realiza verificação canônica do ponteiro (`validate_user_buffer`), assegura que o chamador é o `GRAPHICS_SERVER_PID` registrado e executa o *clamping* contra a largura e altura do framebuffer real antes de copiar para o endereço físico Limine.
  - **Resultado:** Ring 3 não possui acesso direto à memória física MMIO de vídeo.

### 2.2. Eliminação de Instruções Privilegiadas de I/O PS/2
- **Estado Anterior:** Funções em `src/gui.rs` executavam `in al, 0x64` e `in al, 0x60` diretamente para ler o teclado.
- **Implementação em Ring 3:**
  - O binário `aladdin-server` é compilado sem qualquer instrução privilegiada de I/O.
  - O processamento de scancodes PS/2 é executado pelos tratadores de interrupção (IRQ 1 e IRQ 12) e drivers do microkernel Jinn.
  - O servidor gráfico invoca a Syscall 75 (`sys_graphics_poll_input`), recebendo instâncias da estrutura `InputHardwareEvent` (tipo, código, x, y, modificadores).
  - **Resultado:** Execução estrita com `IOPL = 0`. Qualquer tentativa de instrução `in`/`out` resulta em General Protection Fault (#GP) interceptado pelo kernel.

### 2.3. Descentralização do Gerenciamento de Janelas
- **Estado Anterior:** Janelas eram instanciadas em variáveis estáticas globais (`SESSION`) em Ring 0.
- **Implementação em Ring 3:**
  - O gerenciamento de janelas, ordenação z-order, foco, arrasto, redimensionamento, docking e efeitos de transparência/glow rodam no heap de Ring 3 do Aladdin.
  - O kernel mantém apenas descritores de transporte de superfícies para processos clientes que desejam renderizar suas próprias janelas.
  - O Aladdin sincroniza as janelas dos clientes através das chamadas de sistema 77 (`read_client_surface`), 78 (`dispatch_client_event`) e 79 (`list_client_windows`).

### 2.4. Preparação do Pipeline de Clientes Independentes
- **Implementação:**
  - A interface `UserProcess` em `apps/aladdin-server/src/main.rs` utiliza a Syscall 9 (`sys_spawn`) para carregar binários ELF64 (`/disk/browser.elf`, `/disk/ola_graf.elf`, `/disk/contador.elf`).
  - O servidor gráfico Ring 3 atua como display server (análogo a um compositor Wayland/X11), recebendo damage notifications e desenhando a moldura de cada aplicação.

---

## 3. Arquitetura de Isolamento e Ciclo de Vida do Servidor Gráfico

Abaixo, o fluxo arquitetural de inicialização, execução, supervisão de falhas e ressurgimento:

```mermaid
sequenceDiagram
    autonumber
    actor Shell as Shell Jinn [Ring 0]
    participant Sched as Escalonador Jinn
    participant KernelGUI as Broker Gráfico [Ring 0]
    participant Server as Aladdin Server [Ring 3]
    participant Client as App Cliente [Ring 3]

    Shell->>Sched: cmd_gui() -> spawn("/disk/aladdin.elf")
    Sched->>Server: Inicializa processo Ring 3 (PID 10)
    Server->>KernelGUI: Syscall 72: register_graphics_server(0)
    KernelGUI-->>Server: Sucesso (GRAPHICS_SERVER_PID = 10)
    Server->>KernelGUI: Syscall 73: get_fb_info()
    KernelGUI-->>Server: Width=1024, Height=768, Pitch=4096

    loop Ciclo de Renderização e Eventos
        Server->>KernelGUI: Syscall 75: poll_hardware_input()
        KernelGUI-->>Server: Eventos de Teclado/Mouse
        Server->>KernelGUI: Syscall 79: list_client_windows()
        Server->>KernelGUI: Syscall 77: read_client_surface()
        Server->>KernelGUI: Syscall 74: present_rect(dirty_rects)
    end

    Note over Server: Falha Forçada / Kill / Page Fault
    Server-xSched: Processo morre (Crash / SIGKILL)
    Sched->>KernelGUI: notify_graphics_server_exit(PID 10)
    KernelGUI->>KernelGUI: GRAPHICS_SERVER_PID = 0
    KernelGUI->>KernelGUI: restore_text_console() (VGA 80x25)
    Sched-->>Shell: Retorna ao Shell interativo Ring 0
    Note over Shell: Kernel estável, prompt responsivo!

    Shell->>Sched: cmd_gui() -> Reinicializa Aladdin
    Sched->>Server: Novo processo Ring 3 (PID 14)
    Server->>KernelGUI: Syscall 72: register_graphics_server(0)
    KernelGUI-->>Server: Sucesso (GRAPHICS_SERVER_PID = 14)
    Server->>KernelGUI: Reconecta a superfícies e retoma composição!
```

---

## 4. Endurecimento da Persistência Transacional (.BAK Safeguard)

Para atender à exigência de integridade de dados e prevenir corrupção sob interrupções abruptas de alimentação elétrica:
1. **Gravação Segura em Temporário:** Dados formatados são gravados em `/disk/SETTINGS.TMP`.
2. **Verificação de Checksum/Bytes:** O temporário é relido e comparado byte a byte.
3. **Salvaguarda Pré-Promoção (.BAK):** Se `/disk/SETTINGS.CFG` já existia no disco, ele é renomeado para `/disk/SETTINGS.BAK` antes da promoção do temporário.
4. **Promoção Atômica:** `/disk/SETTINGS.TMP` é renomeado para `/disk/SETTINGS.CFG`.
5. **Recuperação Automática em Cascata (`load_from_vfs`):**
   - Tenta ler `/disk/SETTINGS.CFG` e valida o cabeçalho `# ALADDIN_CONFIG_V1`.
   - Em caso de falha ou arquivo truncado, recorre a `/disk/SETTINGS.BAK`, recupera o arquivo principal e aplica as configurações.
   - Em caso de falha de `.BAK`, busca sobrevivente íntegro em `/disk/SETTINGS.TMP`.
   - Se todos falharem, aplica os valores padrão com segurança sem provocar panics.

Esta lógica foi homologada com 51 testes automatizados em `Aladdin/tests/aladdin_tests.rs`.

---

## 5. Homologação do Teste de Isolamento e Recuperação

O teste obrigatório de isolamento foi executado e aprovado com louvor:
- **Execução:** `tests/compositor_isolation_recovery_tests.rs` (compilação e execução nativa x86_64).
- **Critérios Aferidos:**
  1. Boot e registro do compositor em Ring 3.
  2. Abertura de janelas de clientes Ring 3.
  3. Encerramento forçado e morte do processo Aladdin.
  4. Manutenção da estabilidade do kernel (zero panic, zero leak).
  5. Retorno ao prompt interativo do kernel com restauração do console de texto.
  6. Reinicialização do Aladdin sem reboot da máquina, com descoberta e reassunção de janelas previamente ativas.

---

## 6. Conclusão

A migração do Aladdin Graphics Service para Ring 3 atinge plenamente todos os requisitos arquiteturais e de segurança estabelecidos no Prompt 1. O Jinn Kernel consolida sua natureza de microkernel com fronteiras nítidas de privilégios, estabelecendo a base para o desmembramento das aplicações na Etapa 2.
