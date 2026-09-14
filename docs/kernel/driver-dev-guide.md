# Driver Development Guide — Jinn OS

## 1. Introdução

Bem-vindo ao guia de desenvolvimento de drivers para o Jinn OS. O Jinn OS adota uma arquitetura de microkernel estrita, o que significa que **todos os drivers executam no espaço de usuário (*user-space*)**, isolados uns dos outros e do kernel central.

Desenvolver um driver para Jinn assemelha-se mais a desenvolver um serviço de rede do que a programar um módulo de kernel tradicional (como no Linux). 

### Vantagens do Modelo Jinn:
- **Segurança:** Um bug no driver não causa um *Kernel Panic*.
- **Atualização a Quente:** Drivers podem ser reiniciados ou atualizados sem reiniciar o sistema.
- **Desenvolvimento Fácil:** Uso de ferramentas e depuradores de espaço de usuário comuns (`gdb`, `lldb`).
- **Rust First:** A linguagem oficial é Rust, garantindo segurança de memória sem garbage collection.

---

## 2. Anatomia de um Driver Jinn

Um driver Jinn é um binário ELF autônomo. Ele é composto por três partes principais:

1. **Manifesto (`driver.toml`):** Declara as permissões, recursos de hardware e metadados.
2. **Setup de Ambiente:** Recebe o `C-Space` inicial com *capabilities* e mapeamentos de memória.
3. **Event Loop IPC:** Onde o driver escuta e responde a requisições de clientes ou interrupções de hardware via mensagens IPC.

---

## 3. Estrutura do Projeto (Rust)

Recomendamos usar a biblioteca base `jinn-driver-sdk`.

Crie um novo projeto Cargo:
```bash
cargo new --bin my_driver
cd my_driver
```

No seu `Cargo.toml`:
```toml
[package]
name = "my_driver"
version = "0.1.0"
edition = "2024"

[dependencies]
jinn-driver-sdk = { path = "../../sdk" }
jinn-ipc-capnp = { path = "../../ipc" } # Interface RPC
capnp = "0.14"
```

E crie o arquivo de manifesto `driver.toml` na raiz do projeto (veja a especificação em `driver-manifest.md`).

---

## 4. O Ciclo de Vida do Driver

### 1. Inicialização e Handshake
Quando o Jinn OS inicia seu driver, ele passa os identificadores essenciais via registradores ou argumentos iniciais (`rdi`, `rsi`). O SDK abstrata isso na função de entrada.

```rust
use jinn_driver_sdk::{DriverContext, ResourceRights, HardwarePort};

fn main() -> Result<(), DriverError> {
    // 1. Inicializa o contexto conectando-se ao Driver Manager
    let mut ctx = DriverContext::init()?;

    // 2. Valida se recebemos os recursos solicitados no driver.toml
    let ioports = ctx.take_io_ports("0x60").expect("Failed to get PS/2 IO ports");
    let irq = ctx.take_irq(1).expect("Failed to get IRQ 1");

    // 3. Registra os endpoints IPC (Cap'n Proto)
    let endpoint = ctx.create_ipc_endpoint("events");

    // 4. Inicia o loop principal
    run_loop(ctx, ioports, irq, endpoint)
}
```

### 2. Tratamento de Interrupções de Hardware (IRQs)

Interrupções no Jinn são traduzidas em mensagens IPC de tipo assíncrono. O kernel mascara a interrupção no APIC e envia uma notificação para a thread registrada. O driver deve responder ao hardware e então pedir ao kernel para desmascarar a linha.

```rust
fn run_loop(
    mut ctx: DriverContext, 
    mut ports: HardwarePort, 
    irq: IrqHandle,
    endpoint: IpcEndpoint
) -> Result<(), DriverError> {
    
    // Registra o tratador de eventos no poll assíncrono
    let mut poll = jinn_driver_sdk::EventPoll::new();
    poll.add(&irq, Token::IRQ);
    poll.add(&endpoint, Token::IPC);

    loop {
        // Bloqueia esperando um evento (Hardware ou Mensagem)
        for event in poll.wait(None) {
            match event.token {
                Token::IRQ => {
                    // Lê os dados do hardware
                    let scancode = ports.read_u8(0x60);
                    
                    // Avisa o kernel que terminamos e a IRQ pode ser desmascarada
                    irq.ack(); 
                    
                    // Processa e encaminha aos clientes...
                    broadcast_key(scancode, &endpoint);
                },
                Token::IPC => {
                    // Trata mensagens de clientes (RPC)
                    handle_client_message(endpoint.recv()?);
                }
            }
        }
    }
}
```

---

## 5. Transferência de Memória (Zero-Copy e DMA)

Drivers frequentemente precisam mover grandes quantidades de dados (ex: rede, disco, GPU).

### DMA (Direct Memory Access)
Para usar DMA, o driver deve possuir a capability `DMA_ALLOC`.

```rust
// O SDK aloca páginas fisicamente contíguas e retorna o endereço físico para o hardware
let dma_buffer = ctx.allocate_dma_buffer(4096)?;
let paddr = dma_buffer.physical_address();

// Configura o dispositivo de hardware com o endereço físico
setup_hardware_dma_rings(ports, paddr);
```

### Zero-Copy Compartilhada com Clientes
Para não copiar dados para o cliente via buffer limitado do IPC, o driver pode mapear páginas para o cliente.

```rust
fn handle_read_request(req: IpcMessage, dma_buffer: &DmaBuffer) {
    // Transfere o buffer para o domínio do cliente no modo Read-Only
    let capability = ctx.share_memory_page(
        req.sender_domain(), 
        dma_buffer.vaddr(), 
        Permission::READ_ONLY
    );
    
    // Responde com o Token da capability
    req.reply_with_capability(capability);
}
```

---

## 6. Debugging e Testes

Como drivers são processos comuns, você pode usar `gdb` ou rodá-los mockados no Linux/macOS.

1. **Mocking Hardware:** O `jinn-driver-sdk` permite mockar IO Ports e IRQs com o cargo feature `mock-hw`.
2. **Unit Tests:** Escreva testes normais em Rust (`cargo test`).
3. **Tracing:** Use a biblioteca `jinn-trace` que mapeia `log::info!` para o sistema de auditoria e logging central do Jinn OS, suportando exportação para QEMU serial.

```rust
log::info!("Device initialized with MAC: {:02x?}", mac_addr);
```

---

## 7. Melhores Práticas e Armadilhas (Pitfalls)

- **Nunca use polling bloqueante ativo (Busy Wait):** Sempre prefira IRQs ou o `poll.wait()` do IPC. O escalonador penaliza tarefas em busy wait.
- **Evite alocação dinâmica (Heap) no caminho crítico de interrupção:** Use buffers pré-alocados ou object pools para garantir que o driver responda à IRQ rapidamente.
- **Zero Trust:** Valide todo payload vindo via IPC, mesmo de outros serviços do sistema.
- **Timeouts:** Ao fazer chamadas síncronas (`rpc_call`) para outro serviço, sempre defina um limite (`timeout`). Não espere para sempre por um sistema de arquivos travado.

---

Bem-vindo à programação segura de sistemas.
