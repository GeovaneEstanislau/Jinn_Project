# JINN BROWSER 0.8.0 — FULLSCREEN SUBSYSTEM AUDIT

**Documento:** `docs/JINN_BROWSER_FULLSCREEN_AUDIT_0.8.0.md`  
**Classificação:** Auditoria Técnica e Diagnóstico de Causa Raiz  
**Versão:** 0.8.0  
**Data:** 05/11/2026  
**Status:** Auditado e Concluído [V]

---

## 1. Sumário Executivo

Na versão 0.7.0 do Jinn Browser, a tentativa de acionamento do modo de tela cheia ou maximização da janela do navegador através do gerenciador de janelas do Aladdin Desktop resultava em travamento imediato, corrupção visual ou falha de renderização.

Esta auditoria técnica identificou a causa raiz do problema, cobrindo o pipeline completo:
1. Aplicação gráfica em Ring 3 (`apps/ola-mundo/src/browser.rs` e `apps/ola-mundo/src/main.rs`).
2. Protocolo de eventos e chamadas de sistema gráficas (`src/interrupts/syscall.rs` e `src/syscall_user.rs`).
3. Gerenciador de janelas e compositor gráfico do Aladdin (`Aladdin/src/wm/manager.rs` e `Aladdin/src/desktop/session.rs`).

---

## 2. Diagnóstico da Causa Raiz

A auditoria isolou quatro falhas interdependentes que inviabilizavam o modo de tela cheia:

### 2.1. Dimensões e Buffer Estático Hardcoded em Ring 3
No cliente de usuário `apps/ola-mundo/src/browser.rs`, a resolução do navegador estava fixada em constantes rígidas:
```rust
pub const WIN_W: usize = 300;
pub const WIN_H: usize = 180;
```
Adicionalmente, em `apps/ola-mundo/src/main.rs`, o buffer de renderização era declarado estaticamente como:
```rust
static mut WIN_BUF: [u32; 300 * 180] = [0xFF333333; 300 * 180];
```
Esse buffer continha exatamente 54.000 pixels (216 KiB). Quando o usuário clicava no botão de maximizar no Aladdin Desktop, a geometria da janela era expandida para preencher o desktop (tipicamente $1022 \times 652$ pixels, totalizando 666.344 pixels = ~2,6 MiB). O cliente continuava enviando apenas 54.000 pixels via `sys_window_blit`.

### 2.2. Rejeição e Incompatibilidade de Tamanho no Compositor
No compositor do Aladdin (`Aladdin/src/wm/manager.rs`), a rotina `update_client_buffer` impunha uma checagem restritiva:
```rust
let expected_len = (w * h) as usize;
if buffer.len() != expected_len {
    return false;
}
```
Como o cliente Ring 3 enviava um buffer de tamanho 54.000 enquanto a janela maximizada exigia 666.344 pixels, a chamada era sistematicamente rejeitada com erro silencioso (`false`), paralisando a renderização da janela.

### 2.3. Ausência de Notificação de Redimensionamento ao Cliente
O Aladdin Desktop processava o clique no botão de maximização internamente:
- Alterava `win.state` para `WindowState::Maximized`.
- Atualizava as coordenadas $(x, y, w, h)$ na estrutura de janelas.
- **Entretanto**, o Aladdin nunca notificava a aplicação cliente de que sua área útil havia sido alterada. O enum de eventos não possuía um evento `EVENT_WINDOW_RESIZE`, deixando o cliente desinformado sobre a nova dimensão da sua janela.

### 2.4. Violação de Stride e Falha de Bounds no Desenho
As rotinas de desenho em `apps/ola-mundo/src/browser.rs` (`render`, `fill_rect`, `draw_rect_border`) utilizavam fórmulas de índice baseadas em stride fixo:
```rust
let idx = y * WIN_W + x;
buf[idx] = color;
```
Quando os limites visuais tentavam acomodar posições superiores a $x \ge 300$ ou $y \ge 180$, ocorria tentativa de indexação fora dos limites do array, resultando em pânico de execução ou comportamento visual imprevisível.

---

## 3. Matriz de Auditoria dos Componentes

| Componente | Arquivo Auditado | Vulnerabilidade Identificada | Gravidade | Status da Correção |
| :--- | :--- | :--- | :--- | :--- |
| **Ring 3 App** | [apps/ola-mundo/src/browser.rs](file:///e:/Projetos/Jinn/apps/ola-mundo/src/browser.rs) | Dimensões e strides hardcoded (300x180) | Crítica | [V] Corrigido dinamicamente |
| **Ring 3 App** | [apps/ola-mundo/src/main.rs](file:///e:/Projetos/Jinn/apps/ola-mundo/src/main.rs) | Buffer estático insuficiente (54k px) | Crítica | [V] Expandido para MAX 1024x768 |
| **Aladdin SDK** | [Aladdin/src/contracts/input.rs](file:///e:/Projetos/Aladdin/src/contracts/input.rs) | Falta do código de evento de redimensionamento | Alta | [V] Adicionado `EVENT_WINDOW_RESIZE = 8` |
| **Aladdin WM** | [Aladdin/src/wm/manager.rs](file:///e:/Projetos/Aladdin/src/wm/manager.rs) | Inflexibilidade no tamanho de buffer | Alta | [V] Realocação dinâmica de buffer |
| **Aladdin Session** | [Aladdin/src/desktop/session.rs](file:///e:/Projetos/Aladdin/src/desktop/session.rs) | Maximização não despachava evento ao cliente | Crítica | [V] Despacho do evento com largura/altura |
| **Controle de Estado** | [browser/jinn-browser/src/fullscreen.rs](file:///e:/Projetos/Jinn/browser/jinn-browser/src/fullscreen.rs) | Inexistência de máquina de estados formal | Média | [V] Implementada máquina de 6 estados |

---

## 4. Conclusão da Auditoria

O modo de tela cheia não falhava por instabilidade no kernel ou no driver gráfico, mas sim pela ausência de um protocolo bidirecional de redimensionamento dinâmico entre o gerenciador de janelas e o processo cliente, combinado com buffers estáticos subdimensionados.

Com as causas formalmente isoladas, a implementação da versão 0.8.0 resolveu integralmente cada um dos pontos mapeados.

