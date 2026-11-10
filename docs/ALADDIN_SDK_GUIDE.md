# Aladdin SDK Guide — Guia de Desenvolvimento de Aplicações

> **Aladdin Desktop SDK (`aladdin-sdk`)**  
> **Versão:** 0.1.0  
> **Compatibilidade:** Jinn OS Alpha 0.0.3+ / Aladdin Desktop Ring 3  
> **Público-Alvo:** Desenvolvedores de Software de Usuário (Ring 3)  

---

## 1. Introdução

O **Aladdin SDK** é a biblioteca oficial para criação de aplicações gráficas e interativas que executam em modo de usuário (**Ring 3**) sobre o microkernel **Jinn OS** e o ambiente desktop **Aladdin**.

### Características Principais:
- **`#![no_std]` Nativo:** Compatível com ambientes sem runtime C (libc) ou libstd convencional.
- **Zero Alocações Ocultas:** Todo o gerenciamento de memória é explícito e previsível.
- **Segurança de Tipos:** Abstrações ergonômicas (`WindowHandle`, `Canvas`, `Color`, `ClientWindowEvent`) sobrepondo a complexidade das syscalls brutas.
- **Eficiência Energética e de CPU:** Primitivas cooperativas de tratamento de eventos que suspendem a execução quando não há eventos, reduzindo o consumo a zero ciclos de CPU ociosos.

---

## 2. Estrutura de um Projeto de Aplicação

Um projeto de aplicação padrão possui a seguinte configuração no `Cargo.toml`:

```toml
[package]
name = "meu-app"
version = "0.1.0"
edition = "2021"

[dependencies]
aladdin-sdk = { path = "../../sdk/aladdin-sdk" }

[[bin]]
name = "meu-app"
path = "src/main.rs"
test = false
bench = false
```

Configuração de compilação em `.cargo/config.toml`:

```toml
[build]
target = "x86_64-jinn-user.json"

[target.x86_64-jinn-user]
rustflags = [
    "-C", "link-arg=-nostdlib",
    "-C", "link-arg=-static",
    "-C", "relocation-model=static",
]
```

---

## 3. Módulos do SDK

### 3.1. `prelude`
Importe todos os tipos essenciais com uma única instrução:
```rust
use aladdin_sdk::prelude::*;
```

### 3.2. `window` — Gerenciamento de Janelas
- `open_window(title: &str, width: u32, height: u32) -> Result<WindowHandle, WindowError>`: Cria e abre uma nova janela cliente.
- `WindowHandle::blit(&self, buffer: &[u32]) -> Result<(), WindowError>`: Apresenta o buffer de pixels na janela.
- `WindowHandle::poll_event(&self) -> Option<ClientWindowEvent>`: Coleta o próximo evento sem bloquear.
- `WindowHandle::wait_event(&self) -> ClientWindowEvent`: Aguarda o próximo evento cedendo voluntariamente a CPU (`yield_now`).
- `WindowHandle::wait_event_timeout(&self, timeout_ms: u64) -> Option<ClientWindowEvent>`: Aguarda evento com limite de tempo.
- `WindowHandle::close(self) -> Result<(), WindowError>`: Destrói a janela e libera os recursos no compositor.

### 3.3. `graphics` — Superfície de Desenho (Canvas 2D)
- `Color::rgb(r, g, b)` e `Color::rgba(r, g, b, a)`: Cores pré-definidas (`WHITE`, `BLACK`, `RED`, `GREEN`, `BLUE`, `CYAN`, `CARD_BG`).
- `Canvas::new(buffer: &mut [u32], width: usize, height: usize)`: Constrói uma visão de desenho sobre um slice linear de pixels.
- `canvas.clear(color)`: Preenche todo o buffer com uma cor uniforme.
- `canvas.set_pixel(x, y, color)`: Define a cor de um pixel com verificação de limites.
- `canvas.fill_rect(x, y, w, h, color)`: Desenha retângulo sólido.
- `canvas.draw_rect(x, y, w, h, color)`: Desenha moldura de retângulo.
- `canvas.draw_line(x0, y0, x1, y1, color)`: Algoritmo de Bresenham rápido.
- `canvas.draw_gradient(c_top, c_bottom)`: Degradê vertical linear suave.

### 3.4. `events` — Notificações e Entrada
Estrutura `ClientWindowEvent`:
- `ev.is_close()`: Retorna `true` se o usuário solicitou fechar a janela.
- `ev.is_key_down()` e `ev.key_code()`: Retorna código da tecla pressionada.
- `ev.is_mouse_down()`, `ev.mouse_coords()`, `ev.mouse_button()`: Coordenadas e botão do clique.
- `ev.is_resize()` e `ev.resize_dims()`: Dimensões após redimensionamento.

### 3.5. `process`, `console` e `time`
- `exit(code: i32) -> !`: Termina o processo com código de saída.
- `getpid() -> u64`: Obtém o PID atual.
- `yield_now()`: Cede o quantum de CPU.
- `sleep(ms: u64)`: Pausa a tarefa por milissegundos.
- `print(str)`, `println(str)`, `print_num(val)`, `print_hex(val)`: Saída em console/serial.

---

## 4. Exemplo Completo: Aplicativo Interativo

Abaixo está a implementação completa de uma aplicação Ring 3 interativa:

```rust
#![no_std]
#![no_main]

use aladdin_sdk::prelude::*;

const WIN_W: u32 = 320;
const WIN_H: u32 = 240;

// Buffer de pixels estático alocado em .bss da tarefa
static mut FRAMEBUFFER: [u32; (WIN_W * WIN_H) as usize] = [0; (WIN_W * WIN_H) as usize];

#[no_mangle]
pub extern "C" fn _start() -> ! {
    println("[App] Inicializando janela...");

    let win = match open_window("Meu Aplicativo", WIN_W, WIN_H) {
        Ok(w) => w,
        Err(err) => {
            println("[App] Erro ao criar janela!");
            exit(1);
        }
    };

    let fb = unsafe { &mut *core::ptr::addr_of_mut!(FRAMEBUFFER) };
    let mut canvas = Canvas::new(fb, WIN_W as usize, WIN_H as usize).unwrap();

    let mut click_pos: Option<(usize, usize)> = None;

    loop {
        // Coleta e trata todos os eventos pendentes na fila
        while let Some(ev) = win.poll_event() {
            if ev.is_close() {
                let _ = win.close();
                exit(0);
            }
            if let Some(key) = ev.key_code() {
                if key == 0x1B { // Tecla ESC
                    let _ = win.close();
                    exit(0);
                }
            }
            if ev.is_mouse_down() {
                if let Some((x, y)) = ev.mouse_coords() {
                    click_pos = Some((x as usize, y as usize));
                }
            }
        }

        // Renderiza cena gráfica
        canvas.draw_gradient(Color::CARD_BG, Color::rgb(20, 35, 60));
        canvas.draw_rect(0, 0, WIN_W as usize, WIN_H as usize, Color::CYAN);
        canvas.fill_rect(40, 40, 240, 60, Color::rgb(30, 41, 59));

        if let Some((cx, cy)) = click_pos {
            canvas.fill_rect(cx.saturating_sub(4), cy.saturating_sub(4), 9, 9, Color::RED);
        }

        // Apresenta na tela via cópia tamponada
        let _ = win.blit(canvas.buffer);

        // Suspende a execução por 16 ms (~60 FPS) para poupar CPU
        sleep(16);
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    println("[App] Falha crítica detectada!");
    exit(101);
}
```
