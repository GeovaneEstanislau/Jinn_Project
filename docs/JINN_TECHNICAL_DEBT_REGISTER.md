# 📋 Jinn OS & Aladdin Desktop — Registro de Débitos Técnicos

> **Documento Oficial de Engenharia — Etapa A (Consolidação Tecnológica)**  
> **Data de Emissão:** 16 de Novembro de 2026 | **Branch:** `main` | **Commit Base:** `447780e`  
> **Objetivo:** Catalogação rigorosa, rastreabilidade e priorização de todos os débitos arquiteturais, falhas latentes e limitações estruturais identificadas na auditoria do código-fonte.

---

## 1. Classificação de Severidade

* **CRÍTICA:** Impede a execução correta do sistema, corrompe dados em memória, quebra isolamento de anéis de proteção ou causa congelamento total do sistema operacional.
* **ALTA:** Funcionalidade essencial quebrada, bloqueada ou regredida; limita gravemente a capacidade de expansão para recursos computacionais avançados.
* **MÉDIA:** Limitação operacional conhecida com impacto em desempenho, contenção de recursos ou suporte incompleto a padrões (RFC/POSIX).
* **BAIXA:** Débitos de manutenção, warnings de compilação, acoplamento pontual em scripts ou convenções estilísticas sem impacto em tempo de execução.

---

## 2. Inventário Exaustivo de Débitos Técnicos

### `DEBT-FPU-01`: Ausência de Salvamento/Restauração de Contexto FPU/SSE/AVX no Escalonador
* **Subsistema:** Escalonador & CPU (`src/scheduler.rs`, `src/interrupts/stubs.s`, `src/cpu.rs`)
* **Severidade:** `CRÍTICA`
* **Descrição do Débito:** O microkernel habilita extensões SSE no registrador `CR4` (`OSFXSR` e `OSXMMEXCPT`), permitindo que instruções SSE (`MOVUPS`, `ADDPS`, etc.) sejam executadas sem gerar `#UD`. No entanto, a struct `Task` armazena unicamente os registradores de uso geral (`RAX..R15`, `RSP`, `RIP`, `RFLAGS`). O tratador de timer em assembly (`stubs.s`) e a função `context_switch` não reservam espaço nem executam `FXSAVE` / `FXRSTOR` (ou `XSAVE` / `XRSTOR`).
* **Impacto Operacional:** Se duas ou mais tarefas realizarem operações numéricas, vetoriais ou renderização gráfica utilizando registradores `XMM0..XMM15` simultaneamente, a preempção corromperá silenciosamente os estados computacionais de ambas as tarefas, gerando erros aritméticos imprevisíveis e falhas de renderização.
* **Causa Raiz:** A troca de contexto foi desenhada inicialmente para tarefas de kernel puro sem ponto flutuante.
* **Plano de Mitigação (Etapa B):**
  1. Estender a struct `Task` com um buffer de 512 bytes alinhado a 16 bytes (`#[repr(align(16))]`) para salvar o bloco `FXSAVE_AREA`.
  2. Implementar chaveamento preguiçoso (*Lazy FPU switching*) via `CR0.TS` (Task Switched bit) e interrupção `#NM` (Device Not Available, Vetor 7), ou salvamento síncrono por tarefa se o overhead for desprezível.
  3. Suportar `XSAVE` e `XRSTOR` se `CPUID.01H:ECX.XSAVE[bit 26]` estiver disponível para suporte a registradores AVX `YMM`.
* **Critério de Fechamento:** Teste de estresse com duas tarefas concorrentes executando cálculos matriciais contínuos com `XMM` sem nenhuma divergência de resultados após $10^6$ context switches.

---

### `DEBT-RING3-01`: Regressão Operacional no Jinn Browser e Gerenciador de Arquivos (Ring 3)
* **Subsistema:** Espaço de Usuário & IPC (`userspace/browser.c`, `userspace/fileman.c`, `src/syscall.rs`, `aladdin/src/wm.rs`)
* **Severidade:** `ALTA`
* **Descrição do Débito:**
  1. **Jinn Browser:** O processo é instanciado via `sys_exec`, os logs confirmam a carga, mas a janela cliente não aparece desenhada na área de trabalho do Aladdin.
  2. **Gerenciador de Arquivos:** Ao clicar no ícone correspondente, o sistema entra em congelamento total (*hard freeze*).
* **Impacto Operacional:** Duas das três aplicações nativas do ecossistema de usuário estão inoperantes para o usuário final, comprometendo a validação prática da pilha de rede e do sistema de arquivos.
* **Causa Raiz:**
  * O Browser aloca sua janela via `sys_window_create`, mas há um descompasso no mapeamento de IDs de janela compartilhados entre o descritor do kernel e o loop de composição do `AladdinSession`, fazendo com que a superfície permaneça com flag oculta ou sem mapeamento de buffer de pixels compartilhado.
  * O Gerenciador de Arquivos tenta ler recursivamente o VFS ou a fila de mensagens de janela antes que o descritor de IPC esteja completamente inicializado, entrando em loop de espera ativa (*spin-wait*) em Ring 3 ou esgotando a fila circular de 16 mensagens sem cooperar (`sys_yield`).
* **Plano de Mitigação:**
  1. Auditar o retorno de `sys_window_create` e o registro no gerenciador de janelas `AladdinSession`.
  2. Implementar timeout cooperativo ou retorno imediato com código `-EAGAIN` nas leituras de IPC para evitar travamento síncrono.
  3. Assegurar mapeamento da memória de vídeo do cliente no espaço de endereçamento do compositor.
* **Critério de Fechamento:** Browser e Gerenciador de Arquivos abertos simultaneamente sobre o Aladdin Desktop, navegando no VFS e recebendo eventos de mouse/teclado de forma fluida a 60 FPS.

---

### `DEBT-SMP-01`: Escalonador com Fila Única e Spinlock Global sob Multiprocessamento
* **Subsistema:** Escalonador & Concorrência (`src/scheduler.rs`, `src/sync.rs`)
* **Severidade:** `MÉDIA`
* **Descrição do Débito:** A estrutura `Scheduler` mantém uma única fila global de processos protegida por um `Spinlock<SchedulerInner>`. Quando múltiplos núcleos (Application Processors - APs) estão ativos, cada tick de interrupção de timer em qualquer CPU tenta adquirir o mesmo spinlock para selecionar a próxima tarefa.
* **Impacto Operacional:** Contenção severa no barramento de interconexão de memória sob alta carga multi-core (*cache line bouncing*), aumento da latência de troca de contexto e perda de localidade de cache L1/L2 das CPUs.
* **Causa Raiz:** Arquitetura inicial voltada para simplicidade de implementação no boot Limine SMP.
* **Plano de Mitigação (Etapa D):**
  1. Migrar para filas de execução por CPU (*per-CPU runqueues*).
  2. Implementar algoritmo de roubo de trabalho cooperativo (*work-stealing*) para balanceamento de carga entre CPUs ociosas e ocupadas.
  3. Adicionar afinidade de CPU na struct `Task`.
* **Critério de Fechamento:** Escalonamento multi-core com mais de 4 CPUs virtuais operando sem contenção perceptível no lock global do scheduler.

---

### `DEBT-IPC-01`: Fila Circular de Eventos com Capacidade Fixa Estática (16 Itens)
* **Subsistema:** Comunicação Inter-Processos (`src/ipc.rs`, `src/syscall.rs`)
* **Severidade:** `MÉDIA`
* **Descrição do Débito:** As filas de mensagens entre janelas do Aladdin e os processos clientes de Ring 3 utilizam um buffer circular fixo de 16 elementos. Se um aplicativo de usuário processa eventos a uma taxa inferior à geração de eventos de movimento de mouse ou digitação rápida do teclado, a fila enche.
* **Impacto Operacional:** Eventos subsequentes são silenciosamente descartados com `-ENOSPC` ou os processos clientes entram em impasse caso tentem sincronizar com o compositor de forma bloqueante.
* **Causa Raiz:** Estrutura estática `[WindowMessage; 16]` declarada para evitar alocações dinâmicas no kernel durante despacho de IRQs.
* **Plano de Mitigação:**
  1. Aumentar a capacidade estática para 64 ou 128 elementos ou implementar uma lista encadeada protegida baseada em nós do heap do kernel com limite máximo de cota por processo.
  2. Coalescer eventos de mouse redundantes (ex.: múltiplos eventos consecutivos de `MouseMove` são condensados na última coordenada $(X, Y)$ disponível).
* **Critério de Fechamento:** Movimentação ultrarrápida do cursor do mouse sobre janelas clientes sem perda de pacotes ou engasgos na resposta visual.

---

### `DEBT-FS-01`: Driver Ext2 Operando em Modo Somente-Leitura (ROFS)
* **Subsistema:** Sistema de Arquivos (`src/fs/ext2.rs`)
* **Severidade:** `MÉDIA`
* **Descrição do Débito:** O driver Ext2 implementa com precisão a leitura de superbloco, grupos de blocos, tabelas de inodes, ponteiros diretos/indiretos e entradas de diretório. No entanto, as rotinas de escrita (`write`, `mkdir`, `create`, `unlink`) não atualizam os bitmaps de alocação de blocos e inodes no disco nem gravam metadados atualizados de volta na partição IDE.
* **Impacto Operacional:** Não é possível salvar arquivos de configuração, logs de navegação ou downloads de rede de forma persistente na partição Ext2 secundária (`/ext2`).
* **Causa Raiz:** Priorização da integridade do sistema no estágio inicial de desenvolvimento para evitar corrupção acidental da imagem de disco em testes.
* **Plano de Mitigação:**
  1. Implementar alocação de blocos e inodes via bitmap em memória com flush síncrono para o driver IDE.
  2. Implementar atualização de contagem de diretórios e carimbos de data/hora (*mtime/ctime*).
  3. Implementar mecanismo de journal básico ou garantir fsync defensivo na finalização de escrita.
* **Critério de Fechamento:** Criação, edição e leitura de um arquivo de texto de 1 MiB na partição Ext2 persistido entre reinicializações do QEMU.

---

### `DEBT-MEM-01`: Ausência de Semântica Copy-on-Write (CoW) no `sys_fork`
* **Subsistema:** Gestão de Memória Virtual (`src/syscall.rs`, `src/memory/paging.rs`)
* **Severidade:** `MÉDIA`
* **Descrição do Débito:** Ao clonar um processo via `sys_fork`, o kernel duplica integralmente todas as tabelas de páginas e aloca novos frames físicos para todas as páginas de dados e pilha do processo pai, em vez de compartilhar as páginas físicas marcadas como somente-leitura com a flag `COW`.
* **Impacto Operacional:** Sob o teto de 64 MB de memória física (perfil Legacy), a clonagem de processos que consomem alguns megabytes consome instantaneamente o pool do Buddy Allocator, elevando o risco de exaustão de memória física (`-ENOMEM`).
* **Causa Raiz:** Complexidade de interceptação de falta de página (`#PF`, vetor 14) com resolução de contagem de referência de frames físicos.
* **Plano de Mitigação:**
  1. Introduzir contador de referências (*refcount*) em cada frame físico gerenciado pelo Buddy Allocator.
  2. Configurar páginas clonadas no filho e no pai sem a flag `WRITABLE`.
  3. No tratador de `#PF`, se a falta for causada por tentativa de escrita em página compartilhada, duplicar o frame apenas naquele instante.
* **Critério de Fechamento:** Redução de 80% no consumo de frames físicos durante chamadas consecutivas de `sys_fork` seguidas de `sys_exec`.

---

### `DEBT-NET-01`: Ausência de Retransmissão Adaptativa na Pilha TCP
* **Subsistema:** Rede (`src/net/tcp.rs`)
* **Severidade:** `MÉDIA`
* **Descrição do Débito:** O cliente TCP implementa com sucesso o handshake de três vias (SYN, SYN-ACK, ACK) e transmissão de segmentos de dados, porém a lógica de retransmissão utiliza um temporizador estático arbitrário sem cálculo contínuo de RTT (Round-Trip Time) e sem controle de fluxo adaptativo por janela deslizante dinâmica.
* **Impacto Operacional:** Em conexões com jitter ou latência variável, transmissões de pacotes HTTP/TLS volumosos podem sofrer quedas de throughput ou expiração prematura da conexão.
* **Causa Raiz:** O objetivo da pilha de rede foi prover conectividade direta e determinística para o emulador em loopback/NAT (`10.0.2.2`).
* **Plano de Mitigação:**
  1. Implementar algoritmo de Jacobson/Karels para cálculo dinâmico de RTT e RTO (*Retransmission Timeout*).
  2. Ajustar a janela de recepção (*Advertised Window*) com base no buffer livre do socket.
* **Critério de Fechamento:** Download completo de um binário de 5 MiB via TCP sem perda de pacotes sob emulação de perda de 2% de rede.

---

### `DEBT-BUILD-01`: Avisos de Compilação no_std e Caminhos Rígidos nos Scripts PowerShell
* **Subsistema:** Infraestrutura de Build & Automação (`scripts/build.ps1`, `scripts/run.ps1`)
* **Severidade:** `BAIXA`
* **Descrição do Débito:** O pipeline de compilação emite avisos de variáveis não utilizadas e conversões de tipo em módulos de rede e drivers legados. Além disso, os scripts PowerShell contêm caminhos absolutos e checagens dependentes da variável de ambiente local.
* **Impacto Operacional:** Poluição no log de compilação que pode mascarar avisos críticos de integridade de tipos ou desalinhamentos de memória.
* **Causa Raiz:** Evolução acelerada das branches de desenvolvimento sem limpeza estrita com `#![deny(warnings)]`.
* **Plano de Mitigação:**
  1. Limpar variáveis mortas e dead code em `src/net/` e `src/drivers/`.
  2. Adicionar flags `-D warnings` na compilação de release.
  3. Parametrizar dinamicamente os scripts com resolução via `$PSScriptRoot`.
* **Critério de Fechamento:** Execução de `cargo check --target x86_64-unknown-none` sem nenhum warning emitido.

---

## 3. Matriz de Priorização para Mitigação

A tabela abaixo estabelece a ordem estrita de resolução dos débitos para suportar as próximas etapas arquiteturais:

| Ordem | Código do Débito | Subsistema | Severidade | Bloqueador de Qual Etapa? |
|:---:|---|---|:---:|---|
| **1º** | `DEBT-FPU-01` | Escalonador & CPU | `CRÍTICA` | **Etapa B** (Processamento Numérico e FPU) |
| **2º** | `DEBT-RING3-01` | Espaço de Usuário / IPC | `ALTA` | Validação de Ring 3 & Estabilidade de Aplicações |
| **3º** | `DEBT-IPC-01` | Filas de Mensagens IPC | `MÉDIA` | **Etapa C** (GPU & Compositor Aladdin) |
| **4º** | `DEBT-SMP-01` | Escalonador SMP | `MÉDIA` | **Etapa D** (Escalonador Adaptativo e Preempção) |
| **5º** | `DEBT-MEM-01` | Gestão de Memória (CoW) | `MÉDIA` | **Etapa E** (Aquecimento de Recursos em 64 MB) |
| **6º** | `DEBT-FS-01` | Sistema de Arquivos Ext2 | `MÉDIA` | Persistência de Dados em Ring 3 |
| **7º** | `DEBT-NET-01` | Pilha TCP | `MÉDIA` | Download Contínuo de Mídia e Páginas Web |
| **8º** | `DEBT-BUILD-01` | Scripts e Avisos de Build | `BAIXA` | Manutenção Contínua e Integração |
