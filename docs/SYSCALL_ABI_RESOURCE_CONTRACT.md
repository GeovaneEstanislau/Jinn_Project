# ESPECIFICAÇÃO NORMATIVA — CONTRATO DE RECURSOS E ABI DE SYSCALLS
## Interface Binária de Aplicações (ABI) Ring 3 do Jinn OS Microkernel

**Versão da Norma:** 2.0.0  
**Status:** Oficial / Estável  
**Projetos Afetados:** `jinn` (Kernel) e bibliotecas/aplicações em espaço de usuário (`apps/`)  
**Associação:** Issue Jinn [#44](https://github.com/GeovaneEstanislau/Jinn_Project/issues/44)  

---

## 1. Princípios Arquiteturais e Isolamento de Privilégios

O **Jinn OS** implementa uma arquitetura de microkernel com estrita separação entre o espaço privilegiado do kernel (Ring 0) e o espaço de usuário (Ring 3):

1. **Separação de Espaço de Endereçamento:** Cada processo possui sua própria tabela de páginas de 4 níveis (CR3 / PML4 isolado). A metade superior do espaço de endereços (`0xFFFF_8000_0000_0000..0xFFFF_FFFF_FFFF_FFFF`) mapeia o kernel com bit de privilégio supervisor (Ring 0). A metade inferior (`0x0000_0000_0000_0000..0x0000_7FFF_FFFF_FFFF`) abriga os segmentos de código, dados, heap e pilha do processo.
2. **Imunidade contra Page Faults do Kernel:** O kernel nunca desreferencia ponteiros de usuário diretamente sem validação de limites canônicos e mapeamento físico prévio (`validate_user_buffer`). Tentativas de passar ponteiros nulos (`0x0`) retornam `-EINVAL` (-22) e ponteiros para o espaço de kernel ou não mapeados retornam `-EFAULT` (-14).
3. **Imposição de W^X (Write XOR Execute):** Páginas de código ELF de usuário são mapeadas sem o bit `WRITABLE`. Páginas de heap e pilha de usuário são mapeadas com o bit `NO_EXECUTE` (NX), prevenindo injeção e execução de shellcode.

---

## 2. Convenção de Chamada de Sistema (Syscalls ABI)

As chamadas de sistema no Jinn OS utilizam a interrupção de software vetor `0x80` (`int 0x80`).

### 2.1. Convenção de Registradores
- **Identificador da Syscall:** Registrador `RAX`.
- **Argumento 0:** Registrador `RDI`.
- **Argumento 1:** Registrador `RSI`.
- **Argumento 2:** Registrador `RDX`.
- **Retorno:** Registrador `RAX`.
  - Valores `>= 0`: Operação concluída com sucesso (ou descritor/bytes transferidos).
  - Valores `< 0`: Código de erro negativo padronizado conforme convenção POSIX.
- **Registradores Preservados:** `RBX`, `RSP`, `RBP`, `R12`, `R13`, `R14`, `R15`.

```rust
#[inline(always)]
pub unsafe fn syscall3(nr: u64, arg0: u64, arg1: u64, arg2: u64) -> i64 {
    let ret: i64;
    core::arch::asm!(
        "int 0x80",
        inout("rax") nr => ret,
        in("rdi") arg0,
        in("rsi") arg1,
        in("rdx") arg2,
        options(nostack)
    );
    ret
}
```

---

## 3. Tabela Completa de Syscalls Implementadas

| Número (`RAX`) | Nome Simbólico | Argumentos | Descrição Semântica |
| :--- | :--- | :--- | :--- |
| **0** | `sys_exit` | `arg0: code (i32)` | Finaliza a tarefa atual. Libera todas as páginas, descritores, buffers JDA e janelas de forma idempotente. Não retorna. |
| **1** | `sys_write` | `arg0: fd, arg1: *const u8, arg2: len` | Escreve bytes em arquivo VFS ou envia datagrama em socket unificado. `fd=1` escreve no stdout/serial do kernel. |
| **2** | `sys_read` | `arg0: fd, arg1: *mut u8, arg2: len` | Lê bytes de arquivo VFS ou consome dados da fila RX de socket de rede unificado. |
| **3** | `sys_yield` | *(sem argumentos)* | Cede voluntariamente a CPU para a próxima tarefa elegível na fila de prontos do escalonador. |
| **7** | `sys_alloc_page` | *(sem argumentos)* | Aloca dinamicamente uma página física de 4 KiB e mapeia no heap de usuário (`0x0000_1000_0000..0x0000_2000_0000`). Retorna o endereço virtual. |
| **8** | `sys_free_page` | `arg0: virt_addr (u64)` | Desmapeia e libera uma página de 4 KiB alocada via `sys_alloc_page`. Rejeita páginas de pilha, código ELF ou double-free com `-EFAULT`. |
| **11** | `sys_sleep` | `arg0: ms (u64)` | Suspende a execução da tarefa chamadora pelo período especificado em milissegundos. |
| **12** | `sys_jda_shm_alloc` | `arg0: is_read_only (u64)` | Aloca um buffer compartilhado Zero-Copy JDA de 4 KiB. Retorna handle seguro codificado `(gen << 16) | slot`. |
| **13** | `sys_jda_shm_release`| `arg0: handle (u64)` | Libera o buffer compartilhado JDA e incrementa a geração do slot. Rejeita handles obsoletos/stale com `-EPERM`. |
| **14** | `sys_jda_submit` | `arg0: job_type, arg1: handle, arg2: len` | Submete um job de computação acelera da JDA (ex: checksum Fletcher32 em hardware/pipeline assíncrono). |
| **15** | `sys_open` | `arg0: *const u8 (path), arg1: len, arg2: flags` | Abre arquivo no VFS e associa a um slot livre na tabela unificada de descritores (`3..MAX_TASK_FDS`). |
| **16** | `sys_close` | `arg0: fd (u64)` | Fecha arquivo VFS ou socket de rede na tabela unificada. Rejeita double-close com `-EBADF`. |
| **20** | `sys_window_create`| `arg0: *const u8 (title), arg1: len, arg2: packed_dims` | Solicita ao compositor Aladdin a criação de uma janela cliente. Retorna `win_id`. |
| **21** | `sys_window_update`| `arg0: win_id, arg1: *const u32, arg2: byte_len` | Atualiza o buffer de pixels da janela cliente (`0x00RRGGBB`). Rejeita chamadas de processos não proprietários com `-EPERM`. |
| **22** | `sys_window_close` | `arg0: win_id` | Fecha a janela cliente especificada e desaloca buffers gráficos associados. |
| **25** | `sys_res_stat` | `arg0: target_pid, arg1: *mut ProcessResourceStat, arg2: size` | Consulta telemetria de recursos, tempo de CPU, trocas de contexto e violações de cotas em tempo real. |
| **30** | `sys_socket` | `arg0: domain, arg1: type, arg2: protocol` | Cria socket de rede (ex: `AF_INET=2`, `SOCK_DGRAM=2` ou `SOCK_STREAM=1`) vinculado à tabela unificada de descritores. |
| **31** | `sys_connect` | `arg0: sockfd, arg1: *const SockAddrIn, arg2: size` | Associa o socket a um IP e porta de destino. Rejeita FDs que não sejam do tipo socket com `-EBADF`. |
| **35** | `sys_send` | `arg0: sockfd, arg1: *const u8, arg2: len` | Envia pacote de dados através da interface VirtIO-Net, registrando fluxo de saída na tabela conntrack do JinnWall. |
| **36** | `sys_recv` | `arg0: sockfd, arg1: *mut u8, arg2: len` | Recebe datagrama da fila circular RX do socket. Retorna `0` de forma não bloqueante caso a fila esteja vazia. |

---

## 4. Cotas e Limites de Recursos por Tarefa

O subsistema de contabilidade de recursos do kernel (`resources.rs`) impõe limites fixos por processo (`ResourceLimits`):

```rust
pub struct ResourceLimits {
    pub max_frames: usize,       // Padrão: 128 frames (512 KiB)
    pub max_fds: usize,          // Padrão: 8 descritores unificados
    pub max_windows: usize,      // Padrão: 4 janelas do compositor
    pub max_jda_buffers: usize,  // Padrão: 4 buffers Zero-Copy JDA
    pub max_sockets: usize,      // Padrão: 4 sockets de rede
}
```

### Comportamento em Caso de Violação de Cota
- As funções `check_and_*` barram a operação antes de comprometer qualquer estrutura interna.
- O contador `task.resources.limit_violations` é incrementado monotonicamente.
- O código do último erro é armazenado em `task.resources.last_violation_code` para auditoria via `sys_res_stat`.
- Nenhum pânico ocorre no kernel; o erro POSIX correspondente é devolvido imediatamente à aplicação.

---

## 5. Tabela Normativa de Códigos de Erro POSIX

| Código Simbólico | Valor Inteiro | Descrição Semântica no Jinn OS |
| :--- | :--- | :--- |
| `EPERM` | `-1` | Operação não permitida (violação de acesso cross-PID ou handle JDA de outra tarefa). |
| `ENOENT` | `-2` | Arquivo ou entrada de diretório não encontrada no VFS. |
| `EBADF` | `-9` | Descritor inválido (FD fechado, fora do intervalo `3..8`, ou type mismatch como `connect` em arquivo). |
| `ENOMEM` | `-12` | Memória física ou cota de frames do processo esgotada (`max_frames`). |
| `EFAULT` | `-14` | Falha de segmentação: endereço de ponteiro inválido, fora do espaço de usuário ou não mapeado. |
| `EINVAL` | `-22` | Argumento inválido (parâmetro nulo, alinhamento incorreto ou flags incompatíveis). |
| `EMFILE` | `-24` | Esgotamento de descritores de arquivo, sockets ou janelas do processo (`max_fds`, `max_windows`). |
| `ENOSYS` | `-38` | Funcionalidade ou chamada de sistema não suportada nesta configuração. |
| `ENOBUFS` | `-105` | Esgotamento de buffers compartilhados JDA (`max_jda_buffers`). |

---

## 6. Ciclo de Vida e Encerramento Idempotente

Quando um processo é finalizado por chamada a `sys_exit` ou encerramento pelo escalonador:
1. A flag atômica `task.cleanup_done` é verificada. Se já executada, a rotina retorna imediatamente, impedindo *double-free*.
2. A flag é marcada como `true` e o estado da tarefa transita para `TaskState::Zombie`.
3. Todos os descritores unificados (arquivos e sockets) em `task.fds` são fechados, liberando slots em `SOCKET_TABLE` e no VFS.
4. Todos os buffers JDA abertos pelo processo são devolvidos ao pool com incremento de geração.
5. Todas as janelas do processo no compositor Aladdin são fechadas via `gui::close_windows_for_pid()`.
6. Todas as páginas dinâmicas registradas em `task.dynamic_pages` e páginas da pilha de usuário têm seus frames físicos liberados no alocador de memória.
7. As tabelas de páginas do processo (PML4) são recicladas via `paging::free_user_pml4()`.
