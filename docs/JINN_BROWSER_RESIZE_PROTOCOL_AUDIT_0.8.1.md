# JINN BROWSER 0.8.1 — RESIZE PROTOCOL AUDIT & HARDENING

**Documento:** `docs/JINN_BROWSER_RESIZE_PROTOCOL_AUDIT_0.8.1.md`  
**Classificação:** Auditoria de Protocolo IPC, Sincronização e Resiliência  
**Versão:** 0.8.1  
**Data:** 05/11/2026  
**Status:** IMPLEMENTED [V] | UNIT_TESTED [V] | INTEGRATION_TESTED [V]

---

## 1. Visão Geral do Protocolo

A comunicação de redimensionamento entre o servidor gráfico do Aladdin Desktop (`E:\Projetos\Aladdin`) e os clientes de usuário operando em Ring 3 baseia-se no código de evento assíncrono:
```rust
pub const EVENT_WINDOW_RESIZE: u32 = 8;
```
Este evento trafega pela fila circular de eventos da janela (`ClientEventQueue`) via chamada de sistema `sys_window_poll_event` (Syscall 24).

---

## 2. Estrutura do Pacote de Evento e Compatibilidade ABI

O pacote de evento possui layout binário padronizado em C (`#[repr(C)]`), totalizando exatamente 32 bytes:
```rust
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientWindowEvent {
    pub event_type: u32,       // Offset 0:  EVENT_WINDOW_RESIZE = 8
    pub win_id: u32,           // Offset 4:  ID da janela no compositor
    pub x: i32,                // Offset 8:  Nova largura útil do cliente (pixels)
    pub y: i32,                // Offset 12: Nova altura útil do cliente (pixels)
    pub button_or_key: u32,    // Offset 16: 0
    pub modifiers: u32,        // Offset 20: 0
    pub timestamp: u64,        // Offset 24: Tick do timer do sistema
}
```

---

## 3. Vulnerabilidades Auditadas e Mitigações em 0.8.1

### 3.1. Alocação Excessiva por Tamanhos Maliciosos
- **Vulnerabilidade:** Um cliente malicioso poderia passar um buffer com tamanho arbitrariamente imenso no `update_client_buffer`, forçando o compositor a alocar centenas de megabytes no heap.
- **Mitigação:** Foi introduzido o teto limite máximo:
  ```rust
  pub const MAX_CLIENT_BUFFER_PIXELS: usize = 1920 * 1080;
  if buffer.is_empty() || buffer.len() > Self::MAX_CLIENT_BUFFER_PIXELS {
      return Err(WindowOpError::InvalidDimensions);
  }
  ```

### 3.2. Condição de Corrida Durante Redimensionamento em Voo
- **Vulnerabilidade:** Se o usuário maximizar a janela enquanto o cliente estiver no meio de um quadro de renderização, o cliente pode enviar um buffer correspondente às dimensões antigas antes de processar o evento de resize. Na versão 0.8.0, o compositor rejeitava imediatamente com erro.
- **Mitigação em 0.8.1:** O método `update_client_buffer` do Window Manager implementa tolerância transitória:
  ```rust
  if buffer.len() == expected_len {
      if client_buf.len() != expected_len {
          client_buf.resize(expected_len, 0x0018_1825);
      }
      client_buf.copy_from_slice(buffer);
      Ok(())
  } else if buffer.len() == client_buf.len() {
      // Aceita o quadro em voo de dimensão anterior sem travar nem corromper
      client_buf.copy_from_slice(buffer);
      Ok(())
  } else {
      Err(WindowOpError::InvalidDimensions)
  }
  ```

### 3.3. Transbordamento de Fila de Eventos
A fila de eventos `ClientEventQueue` descarta eventos não críticos se atingir a capacidade máxima de 16 itens, mas prioriza a integridade do estado da janela e eventos de redimensionamento e fechamento.

---

## 4. Testes de Regressão e Validação

Em `Aladdin/tests/aladdin_tests.rs`:
- `test_client_window_maximize_and_resize_event_dispatch`: Validou a sequência completa de maximização, emissão do evento 8, redimensionamento atômico do buffer interno e envio de novos buffers em conformidade.
Taxa de aprovação: **100% (43 testes aprovados)**.

