# ALADDIN GRAPHICS SERVICE ABI REFERENCE
## Especificação da Interface Binária de Aplicação (Ring 3 <-> Ring 0)

- **Versão:** 1.0.0
- **Alvo:** x86_64 Long Mode (Ring 3 User Space)
- **Convenção de Chamada:** Microkernel Jinn System V AMD64 via interrupção por software `int 0x80`
- **Ambiente de Execução:** Processo `aladdin.elf` com privilégio de usuário (`CPL = 3`, `IOPL = 0`)

---

## 1. Convenção de Chamadas de Sistema (Syscalls)

As chamadas de sistema entre o Aladdin Graphics Service e o Jinn Kernel utilizam a interface padrão x86_64:

| Registrador | Função | Descrição |
| :--- | :--- | :--- |
| **RAX** | Entrada / Saída | Número da Syscall (Entrada) / Código de Retorno ou Erro Negativo (Saída) |
| **RDI** | Argumento 0 | Primeiro parâmetro da chamada |
| **RSI** | Argumento 1 | Segundo parâmetro da chamada |
| **RDX** | Argumento 2 | Terceiro parâmetro da chamada |
| **R10** | Argumento 3 | Quarto parâmetro da chamada |
| **R8**  | Argumento 4 | Quinto parâmetro da chamada |
| **R9**  | Argumento 5 | Sexto parâmetro da chamada |

### Códigos de Retorno e Erros (Errno POSIX Padrão)
- **>= 0:** Sucesso (quantidade de bytes transferidos, contagem de itens ou status OK).
- **-1 (`EPERM`):** Operação não permitida (o chamador não é o `GRAPHICS_SERVER_PID` ou não possui permissão sobre o recurso).
- **-12 (`ENOMEM`):** Memória insuficiente no kernel ou na quota do processo.
- **-14 (`EFAULT`):** Violação de endereço de memória (ponteiro nulo, endereço não canônico ou fora dos limites do espaço de usuário).
- **-16 (`EBUSY`):** Recurso ocupado (outro processo já está registrado como servidor gráfico exclusivo).
- **-19 (`ENODEV`):** Dispositivo de vídeo não configurado ou indisponível via Limine.
- **-22 (`EINVAL`):** Argumento inválido (dimensões nulas, retângulo fora dos limites, alinhamento incorreto de buffer).

---

## 2. Catálogo de Chamadas de Sistema do Subsistema Gráfico (Syscalls 72..79)

### Syscall 72: `sys_graphics_register(flags: u32) -> isize`
- **Objetivo:** Registra o processo chamador como Servidor Gráfico exclusivo do sistema.
- **Argumentos:** `RDI = flags` (reservado para futuras extensões, atualmente 0).
- **Retorno:** `0` em caso de sucesso; `-EBUSY` se outro processo já for o servidor gráfico ativo.
- **Efeitos Colaterais:** Armazena atomicamente o PID do chamador em `GRAPHICS_SERVER_PID`; inicializa o mouse PS/2 em modo streaming.

### Syscall 73: `sys_graphics_get_fb_info(out_info: *mut GraphicsFbInfo) -> isize`
- **Objetivo:** Obtém os parâmetros físicos do framebuffer alocado pelo bootloader Limine.
- **Argumentos:** `RDI = out_info` (ponteiro em espaço de usuário para estrutura `GraphicsFbInfo`).
- **Retorno:** `0` em caso de sucesso; `-EINVAL` se ponteiro nulo; `-EFAULT` se endereço inválido; `-ENODEV` se sem vídeo.

### Syscall 74: `sys_graphics_present(rect: *const PresentRect, buffer: *const u32, len_bytes: usize) -> isize`
- **Objetivo:** Apresenta uma região retangular dirty rect para o framebuffer físico do hardware.
- **Argumentos:**
  - `RDI = rect`: Ponteiro para a estrutura `PresentRect`.
  - `RSI = buffer`: Ponteiro para o buffer de pixels 32-bit (ARGB/XRGB).
  - `RDX = len_bytes`: Tamanho do buffer em bytes (deve ser múltiplo de 4 e >= `rect.width * rect.height * 4`).
- **Retorno:** `0` em caso de sucesso; `-EPERM` se chamador não for o servidor gráfico; `-EINVAL` para coordenadas inválidas.

### Syscall 75: `sys_graphics_poll_input(out_event: *mut InputHardwareEvent) -> isize`
- **Objetivo:** Consome o próximo evento de hardware (teclado PS/2 ou mouse) enfileirado pelo kernel.
- **Argumentos:** `RDI = out_event`: Ponteiro para estrutura `InputHardwareEvent`.
- **Retorno:**
  - `1`: Evento consumido e gravado em `out_event`.
  - `0`: Nenhum evento pendente na fila (fila vazia).
  - `< 0`: Código de erro negativo (`-EPERM`, `-EFAULT`).

### Syscall 76: `sys_graphics_unregister() -> isize`
- **Objetivo:** Renuncia voluntariamente ao papel de servidor gráfico e restaura o console textual.
- **Argumentos:** Nenhum.
- **Retorno:** `0` em caso de sucesso; `-EPERM` se o chamador não for o servidor gráfico registrado.

### Syscall 77: `sys_graphics_read_client_surface(win_id: usize, dst_buf: *mut u32, max_bytes: usize) -> isize`
- **Objetivo:** Lê os pixels da janela de um processo cliente para composição no backbuffer do servidor.
- **Argumentos:**
  - `RDI = win_id`: Identificador da janela.
  - `RSI = dst_buf`: Buffer de destino no espaço de usuário do servidor gráfico.
  - `RDX = max_bytes`: Capacidade máxima em bytes do buffer.
- **Retorno:** Quantidade de bytes transferidos em caso de sucesso; `-EPERM` ou `-EINVAL`.

### Syscall 78: `sys_graphics_dispatch_client_event(win_id: usize, event: *const ClientWindowEvent) -> isize`
- **Objetivo:** Roteia evento de teclado, mouse ou redimensionamento para a fila privada da janela do cliente.
- **Argumentos:**
  - `RDI = win_id`: Identificador da janela de destino.
  - `RSI = event`: Ponteiro para a estrutura `ClientWindowEvent`.
- **Retorno:** `0` em caso de sucesso; `-EBUSY` se a fila do cliente estiver cheia; `-EINVAL`.

### Syscall 79: `sys_graphics_list_client_windows(out_ptr: *mut ClientWindowInfoUser, max_count: usize) -> isize`
- **Objetivo:** Enumera todas as janelas ativas criadas por processos clientes.
- **Argumentos:**
  - `RDI = out_ptr`: Ponteiro para array de `ClientWindowInfoUser`.
  - `RSI = max_count`: Capacidade máxima de itens do array.
- **Retorno:** Quantidade de janelas enumeradas; `< 0` em caso de erro.

---

## 3. Estruturas de Dados #[repr(C)]

```rust
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphicsFbInfo {
    pub width:  u32, // Largura em pixels (ex: 1024)
    pub height: u32, // Altura em linhas (ex: 768)
    pub pitch:  u32, // Passo em bytes por scanline (ex: 4096)
    pub bpp:    u32, // Bits por pixel (32 bpp)
    pub format: u32, // 0 = ARGB8888, 1 = XRGB8888
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresentRect {
    pub x:      u32, // Coordenada X inicial (origem superior esquerda)
    pub y:      u32, // Coordenada Y inicial
    pub width:  u32, // Largura da região suja (dirty rect)
    pub height: u32, // Altura da região suja
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputHardwareEvent {
    pub kind:      u32, // 0=None, 1=KeyDown, 2=KeyUp, 3=MouseMove, 4=MouseDown, 5=MouseUp, 6=Scroll
    pub code:      u32, // Código da tecla (ASCII ou virtual) ou máscara de botões do mouse
    pub x:         i32, // Coordenada X absoluta do cursor
    pub y:         i32, // Coordenada Y absoluta do cursor
    pub modifiers: u32, // Flags de teclas modificadoras (Shift=1, Ctrl=2, Alt=4)
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientWindowEvent {
    pub event_type:   u32, // 1=MouseDown, 2=MouseUp, 3=MouseMove, 4=KeyDown, 5=KeyUp, 7=Close, 8=Resize
    pub param1:       u32, // Parâmetro primário (código da tecla, botão do mouse, nova largura)
    pub param2:       u32, // Parâmetro secundário (posição X relativa, nova altura)
    pub param3:       u32, // Parâmetro terciário (posição Y relativa)
    pub timestamp_ms: u64, // Marcação temporal monotônica em milissegundos
    pub reserved:     u64, // Alinhamento para 32 bytes
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientWindowInfoUser {
    pub win_id:    u32,      // Identificador da janela
    pub owner_pid: u32,      // PID do processo proprietário da janela
    pub width:     u32,      // Largura em pixels
    pub height:    u32,      // Altura em pixels
    pub is_open:   u32,      // 1 se aberta, 0 se minimizada/fechada
    pub damaged:   u32,      // 1 se os pixels foram modificados pelo cliente e requerem recomposição
    pub title_len: u32,      // Comprimento do título UTF-8 (máx 48 bytes)
    pub title:     [u8; 48], // Bytes do título da janela
}
```

---

## 4. Garantias de Segurança e Limites de Memória

1. **Canonical Memory Safety:** O kernel impõe que qualquer ponteiro passado por syscalls resida estritamente no intervalo canônico de espaço de usuário:
   $$\text{USER\_CANONICAL\_MIN} = \text{0x0000\_0000\_0001\_0000} \le \text{ptr} < \text{ptr} + \text{len} \le \text{0x0000\_8000\_0000\_0000} = \text{USER\_CANONICAL\_MAX}$$
2. **Prevenção de Ataques de Ponteiro de Kernel:** Tentativas de passar endereços acima de `0x0000_8000_0000_0000` (espaço superior do kernel) resultam em rejeição imediata com `-EFAULT`, impedindo leitura ou escrita não autorizada de estruturas do núcleo.
3. **Alinhamento e Quotas:**
   - Buffers de apresentação e superfícies devem estar alinhados a 4 bytes (`buffer_len_bytes % 4 == 0`).
   - Cada processo possui cota estrita de janelas e memória, impedindo ataques de negação de serviço por esgotamento de memória (OOM).
