# JINN BROWSER 0.8.0 — SECURITY & ISOLATION AUDIT

**Documento:** `docs/JINN_BROWSER_SECURITY_AUDIT_0.8.0.md`  
**Classificação:** Auditoria de Segurança, Isolamento de Privilégios e Robustez  
**Versão:** 0.8.0  
**Data:** 05/11/2026  
**Status:** Auditado e Aprovado [V]

---

## 1. Princípios de Segurança e Modelo de Ameaças

O Jinn Browser opera estritamente em **Ring 3 (Modo de Usuário)**, sem privilégios de kernel e sem acesso direto a:
- Tabelas de páginas do kernel (`CR3`).
- Portas de E/S físicas do processador (`in`/`out`).
- Registradores de interrupção ou de controle de sistema (`CR0`, `CR4`).
- Framebuffer físico de vídeo da placa gráfica.

Toda interação gráfica é intermediada pelo compositor do Aladdin Desktop através do manipulador de chamadas de sistema do kernel (`src/interrupts/syscall.rs`).

---

## 2. Auditoria das Syscalls Gráficas

Foram auditadas integralmente as três chamadas de sistema que suportam o ecossistema gráfico:

### 2.1. `sys_window_create` (Syscall 20)
- **Assinatura:** `sys_window_create(title: *const u8, len: usize, width: u32, height: u32) -> i64`
- **Validações de Segurança:**
  - Checagem de ponteiro de título: verifica se o endereço reside estritamente no espaço de endereçamento de usuário ($< 0x0000\_7FFF\_FFFF\_FFFF$).
  - Limite de comprimento de título: restrito a no máximo 128 bytes para evitar estouro de buffer de pilha.
  - Limite de dimensões: restringe largura máxima a 1920 e altura máxima a 1080.
  - Prevenção de Integer Overflow: validação explícita de que $w \times h \times 4$ não excede `usize::MAX`.

### 2.2. `sys_window_blit` (Syscall 21)
- **Assinatura:** `sys_window_blit(window_id: u64, buffer: *const u8, size: usize) -> i64`
- **Validações de Segurança:**
  - Verificação de posse: valida se o `window_id` foi criado pelo processo chamador (identificado por seu PID atual). Processos não podem sobrescrever janelas de terceiros.
  - Validação de ponteiro de buffer: verificação de que o intervalo `[buffer, buffer + size)` é legível pelo processo em Ring 3.
  - Verificação de tamanho: `size` deve ser exatamente igual a $w \times h \times 4$ correspondente à resolução corrente da janela negociada no redimensionamento.

### 2.3. `sys_window_poll_event` (Syscall 22)
- **Assinatura:** `sys_window_poll_event(window_id: u64, event_out: *mut u8) -> i64`
- **Validações de Segurança:**
  - Isolamento estrito de fila de eventos: cada janela mantém uma fila circular FIFO privada no compositor. Um processo não pode interceptar eventos direcionados a outras janelas.
  - Ponteiro de destino: verificação de escrita segura no buffer fornecido em Ring 3.

---

## 3. Prevenção de Transbordamento de Buffer e Bounds Safety

Na versão 0.8.0, o risco de escrita fora dos limites de memória durante o redimensionamento da janela foi completamente mitigado:

```rust
// Garantia de Bounds Checking em apps/ola-mundo/src/browser.rs:
let idx = py * stride_w + px;
if idx < buf.len() && py < max_h {
    buf[idx] = color;
}
```

Mesmo que o compositor envie um evento malicioso com dimensões gigantescas ($w = 99999, h = 99999$):
1. O cliente limita o tamanho contra `MAX_WIN_W = 1024` e `MAX_WIN_H = 768`.
2. O tamanho máximo do buffer estático alocado é exatamente $1024 \times 768 \times 4 = 3.145.728$ bytes.
3. A condição `idx < buf.len()` intercepta e anula qualquer tentativa de escrita além dos limites físicos do array.

---

## 4. Auditoria de Concorrência e IPC

A comunicação de eventos e buffers entre o servidor Aladdin e os clientes Ring 3 opera sob semântica de cópia controlada e troca de mensagens assíncronas com barreiras de memória:
- **Zero Race Conditions:** O compositor consome buffers clientes apenas durante o ciclo de composição de tela do desktop, utilizando dupla paginação (*double-buffering*) interna.
- **Tolerância a Encerramento Abrupto:** Caso o processo do navegador seja encerrado ou falhe com Page Fault, o compositor detecta a desconexão e remove a janela sem travar o desktop.

