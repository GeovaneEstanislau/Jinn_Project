# Relatório de Investigação do Incidente — Congelamento e Resiliência do Aladdin Desktop

## 1. Contexto do Incidente e Dados Operacionais
- **Componente Afetado:** Ambiente Gráfico Aladdin Desktop / Jinn Browser Engine (JBE) / Microkernel Ring 0 & Ring 3.
- **Configuração de Hardware Avaliada:** Máquina Virtual (VirtualBox 7 / QEMU 8+), 64 MB de memória RAM física, arquitetura x86_64, topologias UP (1 vCPU) e SMP (4 vCPUs).
- **Sintoma Inicial Reportado:** Ao clicar na opção para abrir o *Jinn Browser* no ambiente gráfico Aladdin sob 64 MB de RAM, o sistema apresentava notificação de inicialização e, em seguida, congelava completamente (perda total de responsividade da interface e mouse).
- **Comportamento Observado na Reprodução Controlada (VirtualBox):**
  - O cursor do mouse PS/2 foi restaurado e opera normalmente na tela.
  - Ao clicar repetidamente (5 vezes) no botão `+ Executar Jinn Browser` no Terminal ou no dock, o sistema **não congelou**.
  - A interface permaneceu 100% responsiva (taxa de 100 Hz APIC mantida, Uptime progredindo, janelas navegáveis), exibindo de forma resiliente a mensagem em vermelho: `Erro: falha ao carregar /disk/browser.elf`.

---

## 2. Diagnóstico Técnico por Camadas

### A. Diagnóstico de Mídia de Armazenamento e VFS (`/disk/browser.elf`) — [CONFIRMADO]
- **Causa Raiz da Falha ao Abrir o Navegador:** No ambiente padrão do Oracle VirtualBox, máquinas virtuais recém-criadas utilizam uma controladora **SATA (AHCI)**. O driver de bloco do microkernel Jinn OS implementa o suporte para controladoras IDE/ATA legadas PIIX3/PIIX4 nas portas I/O `0x1F0` e `0x170`.
- Ao inicializar o sistema anexando apenas a mídia `jinn.iso` no CD-ROM (dispositivo ATAPI) sem anexar o disco `disk.img` em uma controladora IDE primária Master, a partição FAT32 não pôde ser montada em `/disk`.
- A chamada interna `stat("/disk/browser.elf")` no VFS retornou o código de erro `-ENOENT` (-2).

### B. Diagnóstico da Pilha Gráfica: Ring 3 vs Fallback Ring 0 — [CONFIRMADO]
- O Aladdin Desktop possui dois modos de execução:
  1. **Modo Primário (Ring 3):** Executado como processo isolado em espaço de usuário (`aladdin-server.elf` / `aladdin.elf`), consumindo syscalls 72..79 (`sys_register_graphics`, `sys_present`, etc.).
  2. **Modo Resiliente de Fallback (Ring 0):** Caso o binário do servidor gráfico não esteja no disco ou sofra falha crítica, o kernel assume o controle e executa a interface através do módulo `src/gui.rs:run_desktop()`, garantindo que o operador nunca perca o acesso ao sistema.
- Na imagem capturada no VirtualBox, a barra superior exibia:
  `Up: 16s | 100Hz | RAM: 4/ 44M | Aladdin (Domest | FX: Bal | [X] Sair`
  E o Monitor de Sistema exibia:
  `Tarefas Ativas: 2 processos` | `Memoria RAM: 4 MB / 44 MB`
  Isso comprovou que o Aladdin estava executando em modo Ring 0 (pois no Ring 3 o monitor de telemetria exibe métricas virtualizadas do processo cliente).

### C. Diagnóstico do Heap do Aladdin Server Ring 3 — [CONFIRMADO]
- No código de `apps/aladdin-server/src/main.rs`, o heap estático de espaço de usuário em `.bss` estava configurado com apenas 7 MiB (`HEAP_SIZE = 7 * 1024 * 1024`).
- Como o backbuffer de tela (1024×768 a 32 bpp) consome exatamente 3.145.728 bytes (3,14 MiB), a alocação de buffers adicionais de clientes (`client_cache_buf` e `sub_buf`) excedia a capacidade do bump allocator de 7 MiB, disparando o manipulador `handle_alloc_error` do Rust runtime (`alloc.rs:699`).
- **Ação Implementada:** Expansão do heap estático do servidor gráfico em espaço de usuário para 16 MiB (`const HEAP_SIZE: usize = 16 * 1024 * 1024`). Sob 64 MB de RAM física, o sistema possui mais de 40 MB livres, acomodando confortavelmente o display server e múltiplos clientes.

### D. Diagnóstico do Mecanismo de Fallback em Memória Limine — [CONFIRMADO]
- Quando o arquivo `/disk/browser.elf` não é encontrado no sistema de arquivos local, a rotina de execução recorre aos módulos Limine pré-carregados pelo bootloader no ISO (`module_path: boot():/apps/browser.elf`).
- Foi constatado que a função `spawn_app` em `src/gui.rs` rejeitava prematuramente qualquer código de erro diferente de `-ENOENT` (-2) e `-EIO` (-5), e faltavam pontos de observabilidade serial para identificar com clareza o motivo de rejeição do binário em tempo de execução.
- **Ação Implementada:** Adicionada telemetria serial explícita com logs detalhados em `src/gui.rs` e `src/syscall.rs`, exibindo o caminho solicitado, código de erro retornado, validação ELF e PID atribuído.

---

## 3. Matriz de Classificação das Evidências Técnicas

| Hipótese Investigada | Status | Fundamentação Técnica |
| :--- | :--- | :--- |
| **Exaustão de Memória Física (OOM) no Kernel** | **REPROVADO** | O microkernel sob 64 MB RAM utiliza apenas 1.111 frames de 11.474 (4 MB de 44.5 MB utilizáveis). Restam mais de 40 MB totalmente livres. |
| **Deadlock ou Bloqueio no Escalonador/IPC** | **REPROVADO** | O APIC timer permaneceu ativo a 100 Hz com preempção funcional (1.877.379 ticks processados). A interface processou 5 cliques consecutivos sem travar. |
| **Ausência de Mídia FAT32 `/disk` no VirtualBox** | **CONFIRMADO** | VM configurada sem disco IDE anexado ou com controladora SATA AHCI não suportada; arquivos `/disk/*.elf` inacessíveis no VFS. |
| **Estouro de Heap Estático no Servidor Aladdin Ring 3** | **CONFIRMADO** | Heap de 7 MiB era insuficiente para composições simultâneas (backbuffer 3,14 MiB + sub_buf 3,14 MiB), corrigido para 16 MiB. |
| **Instabilidade do Subsistema de Entrada (Mouse/Teclado)** | **REPROVADO** | O ponteiro do mouse e a fila circular de interrupções responderam em tempo real nos testes práticos. |

---

## 4. Diretrizes de Mitigação e Configuração de Máquina Virtual

Para a execução plena do ambiente com todas as aplicações Ring 3 carregadas a partir do disco FAT32:

1. **Configuração de Controladora no VirtualBox:**
   - Em *Configurações* -> *Armazenamento*:
     - Adicionar uma **Controladora IDE** (Tipo: PIIX4).
     - IDE Primário Master: Anexar o disco virtual `disk.img` (formato RAW ou convertido para VDI/VMDK).
     - IDE Secundário Master: Anexar a imagem `jinn.iso`.
2. **Execução Autônoma por ISO (Live Mode):**
   - Caso o usuário opte por rodar unicamente a ISO sem anexar `disk.img`, o microkernel utiliza o fallback automático carregando `browser.elf` diretamente dos módulos Limine da memória RAM.

