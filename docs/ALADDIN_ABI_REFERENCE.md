# Aladdin ABI Reference — Especificação Formal da Interface Binária

> **Projeto Jinn OS & Aladdin Desktop**  
> **Status:** Normativo / Estável  
> **Arquitetura Alvo:** x86_64 (AMD64)  
> **Data de Emissão:** 10 de Novembro de 2026  

---

## 1. Convenção de Chamada de Baixo Nível

A transição de privilégio entre espaço de usuário (**Ring 3**) e o microkernel (**Ring 0**) é realizada exclusivamente através da instrução de interrupção por software `int 0x80`.

| Registrador | Função na Entrada | Função no Retorno |
| :--- | :--- | :--- |
| **RAX** | Número identificador da Syscall | Código de retorno (`>= 0` sucesso, `< 0` código de erro errno) |
| **RDI** | Argumento 0 (`arg0` / u64) | Preservado pelo tratador |
| **RSI** | Argumento 1 (`arg1` / u64) | Preservado pelo tratador |
| **RDX** | Argumento 2 (`arg2` / u64) | Preservado pelo tratador |
| **R10** | Argumento 3 (`arg3` / u64) | Preservado pelo tratador |

> [!NOTE]
> A convenção de passagem de argumentos preserva todos os registradores de uso geral não-voláteis (`RBX`, `RBP`, `R12`..`R15`). Os registradores de segmento `CS` e `SS` transitam de `0x2B`/`0x33` (Ring 3) para `0x08`/`0x10` (Ring 0) e retornam via instrução `iretq`.

---

## 2. Tabela de Syscalls do Application Runtime

### 2.1. Syscalls de Processo e E/S Básica

| Nº | Símbolo | Argumentos (RDI, RSI, RDX, R10) | Retorno em RAX | Descrição |
| :---: | :--- | :--- | :---: | :--- |
| **0** | `sys_exit` | `code: i32` | *(não retorna)* | Finaliza o processo atual e dispara limpeza atômica de recursos. |
| **1** | `sys_write` | `fd: u64`, `buf: *const u8`, `len: u64` | `bytes_escritos` / `errno` | Escreve bytes em console (`fd=1` stdout, `fd=2` stderr). |
| **2** | `sys_read` | `fd: u64`, `buf: *mut u8`, `len: u64` | `bytes_lidos` / `errno` | Lê bytes do buffer de teclado (`fd=0`). |
| **3** | `sys_yield` | *(nenhum)* | `0` | Cede voluntariamente o quantum de CPU para outras tarefas. |
| **4** | `sys_getpid` | *(nenhum)* | `pid: u64` | Retorna o identificador único do processo corrente. |
| **11** | `sys_sleep` | `ms: u64` | `0` | Suspende a execução da tarefa pelo período em milissegundos. |

---

### 2.2. Syscalls de Janelas e Gráficos do Cliente

| Nº | Símbolo | Argumentos (RDI, RSI, RDX, R10) | Retorno em RAX | Descrição |
| :---: | :--- | :--- | :---: | :--- |
| **20** | `sys_window_create` | `title_ptr`, `title_len`, `width`, `height` | `win_id: usize` / `errno` | Solicita criação de superfície cliente no compositor. Retorna `win_id > 0`. |
| **21** | `sys_window_update` | `win_id`, `buf_ptr: *const u32`, `len_bytes` | `0` / `errno` | Transfere buffer linear de pixels (0x00RRGGBB) para a janela. |
| **22** | `sys_window_close` | `win_id`, `0`, `0` | `0` / `errno` | Fecha a janela especificada e libera seus buffers no kernel. |
| **24** | `sys_window_poll_event` | `win_id`, `event_ptr: *mut Event`, `flags` | `1`, `0` ou `errno` | Coleta próximo evento de entrada. `1` = evento copiado, `0` = fila vazia. |
| **25** | `sys_res_stat` | `pid`, `stat_ptr: *mut ResStat`, `len` | `0` / `errno` | Consulta limites e métricas de telemetria da tarefa. |
| **26** | `sys_jda_shm_share` | `handle`, `target_pid`, `make_owner_ro` | `0` / `errno` | Compartilha buffer de memória Zero-Copy entre processos. |

---

### 2.3. Syscalls do Servidor Gráfico (Aladdin Graphics Service)

| Nº | Símbolo | Argumentos (RDI, RSI, RDX, R10) | Retorno em RAX | Descrição |
| :---: | :--- | :--- | :---: | :--- |
| **72** | `sys_graphics_register` | `flags: u32` | `0` / `EBUSY` | Registra processo como servidor gráfico exclusivo. |
| **73** | `sys_graphics_get_fb_info` | `out_info: *mut GraphicsFbInfo` | `0` / `errno` | Obtém metadados do Framebuffer físico (resolução, pitch, bpp). |
| **74** | `sys_graphics_present` | `rect: *const PresentRect`, `buf: *const u32`, `len` | `0` / `errno` | Apresenta retângulo de pixels no framebuffer físico. |
| **75** | `sys_graphics_poll_input` | `out_event: *mut InputHardwareEvent` | `1`, `0` ou `errno` | Coleta eventos brutos de teclado e mouse do hardware. |
| **76** | `sys_graphics_unregister` | *(nenhum)* | `0` / `EPERM` | Desregistra servidor gráfico e restaura console VGA Ring 0. |
| **77** | `sys_graphics_read_client_surface` | `win_id`, `dst_buf: *mut u32`, `max_bytes` | `bytes_copiados` | Lê conteúdo da superfície de um cliente para composição. |
| **78** | `sys_graphics_dispatch_client_event`| `win_id`, `event: *const ClientWindowEvent` | `0` / `errno` | Roteia evento de janela para a fila privada do cliente. |
| **79** | `sys_graphics_list_client_windows` | `out_ptr: *mut WindowInfo`, `max_count` | `janelas_ativas` | Lista metadados de todas as janelas clientes registradas. |

---

## 3. Estruturas de Dados Binárias (ABI C-Compatible)

Todas as estruturas transmitidas عبر a ABI são estritamente empacotadas com layout C padrão (`#[repr(C)]`), campos alinhados a potências de 2 e sem preenchimentos implícitos não-determinísticos.

### 3.1. `ClientWindowEvent` (Tamanho: 32 bytes, Alinhamento: 8 bytes)

```rust
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientWindowEvent {
    pub event_type:   u32, // Deslocamento  0 (4 bytes)
    pub param1:       u32, // Deslocamento  4 (4 bytes)
    pub param2:       u32, // Deslocamento  8 (4 bytes)
    pub param3:       u32, // Deslocamento 12 (4 bytes)
    pub timestamp_ms: u64, // Deslocamento 16 (8 bytes)
    pub reserved:     u64, // Deslocamento 24 (8 bytes)
}
```

#### Tipos de Eventos (`event_type`):
- `0` — `EVENT_NONE`: Nulo / fila vazia.
- `1` — `EVENT_MOUSE_DOWN`: Botão do mouse pressionado (`param1` = X relativo, `param2` = Y relativo, `param3` = botão [1=Esq, 2=Dir, 4=Meio]).
- `2` — `EVENT_MOUSE_UP`: Botão do mouse liberado (`param1` = X, `param2` = Y, `param3` = botão).
- `3` — `EVENT_MOUSE_MOVE`: Movimento do cursor na janela (`param1` = X, `param2` = Y).
- `4` — `EVENT_KEY_DOWN`: Tecla pressionada (`param1` = código ASCII / Scancode, `param2` = modificadores [Shift, Ctrl, Alt]).
- `5` — `EVENT_KEY_UP`: Tecla liberada (`param1` = código).
- `6` — `EVENT_SCROLL`: Roda do mouse (`param1` = delta vertical).
- `7` — `EVENT_WINDOW_CLOSE`: Notificação de encerramento enviada pelo usuário via botão [X] ou Alt+F4.
- `8` — `EVENT_WINDOW_RESIZE`: Notificação de redimensionamento (`param1` = nova largura, `param2` = nova altura).
- `9` — `EVENT_FOCUS_GAINED`: A janela recebeu o foco de entrada.
- `10` — `EVENT_FOCUS_LOST`: A janela perdeu o foco.

---

### 3.2. `ClientWindowInfoUser` (Tamanho: 76 bytes, Alinhamento: 4 bytes)

```rust
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientWindowInfoUser {
    pub win_id:    u32,      // Deslocamento  0 (4 bytes)
    pub owner_pid: u32,      // Deslocamento  4 (4 bytes)
    pub width:     u32,      // Deslocamento  8 (4 bytes)
    pub height:    u32,      // Deslocamento 12 (4 bytes)
    pub is_open:   u32,      // Deslocamento 16 (4 bytes)
    pub damaged:   u32,      // Deslocamento 20 (4 bytes)
    pub title_len: u32,      // Deslocamento 24 (4 bytes)
    pub title:     [u8; 48], // Deslocamento 28 (48 bytes)
}
```

---

### 3.3. `PresentRect` (Tamanho: 16 bytes, Alinhamento: 4 bytes)

```rust
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresentRect {
    pub x:      u32, // Deslocamento  0 (4 bytes)
    pub y:      u32, // Deslocamento  4 (4 bytes)
    pub width:  u32, // Deslocamento  8 (4 bytes)
    pub height: u32, // Deslocamento 12 (4 bytes)
}
```

---

### 3.4. `GraphicsFbInfo` (Tamanho: 20 bytes, Alinhamento: 4 bytes)

```rust
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphicsFbInfo {
    pub width:  u32, // Deslocamento  0 (4 bytes)
    pub height: u32, // Deslocamento  4 (4 bytes)
    pub pitch:  u32, // Deslocamento  8 (4 bytes)
    pub bpp:    u32, // Deslocamento 12 (4 bytes)
    pub format: u32, // Deslocamento 16 (4 bytes, 0=XRGB8888)
}
```

---

## 4. Tabela de Códigos de Erro (POSIX Negative Errno)

Todas as syscalls retornam valores negativos em RAX em caso de erro, respeitando os códigos de erro padrão da especificação POSIX:

| Nome do Erro | Valor | Significado | Condições Típicas de Disparo no Runtime |
| :--- | :---: | :--- | :--- |
| **EPERM** | `-1` | Operação não permitida | Processo tentou atualizar, fechar ou ler eventos de janela de outro PID; processo não autorizado chamando Syscalls 72..79. |
| **ENOENT** | `-2` | Arquivo ou entrada não encontrada | Executável ELF especificado não existe no VFS. |
| **ESRCH** | `-3` | Processo não encontrado | PID fornecido não existe na tabela de tarefas do escalonador. |
| **EIO** | `-5` | Erro de E/S de baixo nível | Falha de leitura de bloco físico em disco ou partição. |
| **E2BIG** | `-7` | Lista de argumentos muito grande | Linha de comando `argv` ou ambiente `envp` excede 32 entradas ou 4 KiB. |
| **ENOEXEC** | `-8` | Formato executável inválido | Arquivo ELF com cabeçalho corrompido, arquitetura incorreta ou violação W^X. |
| **EBADF** | `-9` | Identificador de recurso inválido | `win_id` inexistente ou janela já destruída; descritor de arquivo inválido. |
| **ENOMEM** | `-12` | Memória física insuficiente | Alocador de frames (Buddy) sem blocos livres para criar superfícies ou tabelas. |
| **EFAULT** | `-14` | Endereço de memória inválido | Ponteiro fornecido é NULL, aponta para espaço de kernel, endereço não-canônico ou página não mapeada no PML4. |
| **EBUSY** | `-16` | Recurso ocupado | Já existe um servidor gráfico registrado (`sys_graphics_register`); fila de eventos cheia. |
| **EINVAL** | `-22` | Argumento inválido | Dimensões de janela zero ou excedendo 1024x768; título vazio ou com mais de 48 bytes; buffer de tamanho incorreto. |
| **EMFILE** | `-24` | Limite global de recursos atingido | Tabela de janelas do sistema (`CLIENT_WINDOWS`, max 32) saturada. |
| **EDQUOT** | `-122`| Cota de recursos da tarefa esgotada| O processo atingiu seu limite configurado de janelas ativas (`task.resources`). |
