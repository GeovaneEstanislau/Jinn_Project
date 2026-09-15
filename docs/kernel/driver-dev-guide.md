# Driver Development Guide — Jinn OS

## 1. Introduction

Welcome to the driver development guide for Jinn OS. Jinn OS utilizes a strict microkernel architecture, meaning **all drivers run in user-space**, isolated from one another and from the core kernel.

Developing a driver for Jinn feels more like building a network microservice than writing a traditional monolithic kernel module (such as in Linux).

### Advantages of the Jinn Model:

* **Safety:** A driver crash will not cause a system-wide *Kernel Panic*.
* **Hot Reloading:** Drivers can be restarted, patched, or upgraded without rebooting the system.
* **Straightforward Development:** Standard user-space tools and debuggers (`gdb`, `lldb`) work out of the box.
* **Rust First:** Rust is the official language, guaranteeing memory safety without garbage collection overhead.

---

## 2. Anatomy of a Jinn Driver

A Jinn driver is a standalone ELF binary composed of three primary components:

1. **Manifest (`driver.toml`):** Declares required capabilities, hardware resources, and metadata.
2. **Environment Setup:** Receives the initial `C-Space` containing capabilities and physical/virtual memory mappings.
3. **IPC Event Loop:** Listens and responds to client requests or hardware interrupts dispatched via IPC messages.

---

## 3. Project Structure (Rust)

We recommend using the standard `jinn-driver-sdk` crate.

Create a new Cargo binary project:

```bash
cargo new --bin my_driver
cd my_driver

```

In your `Cargo.toml`:

```toml
[package]
name = "my_driver"
version = "0.1.0"
edition = "2024"

[dependencies]
jinn-driver-sdk = { path = "../../sdk" }
jinn-ipc-capnp = { path = "../../ipc" } # RPC interface definition
capnp = "0.14"

```

Create a `driver.toml` manifest in your project's root directory (see `driver-manifest.md` for full schema options).

---

## 4. Driver Lifecycle

### 1. Initialization and Handshake

When Jinn OS spawns a driver, it passes base capabilities and context handles through registers/startup arguments (`rdi`, `rsi`). The SDK abstracts this within its initialization routines.

```rust
use jinn_driver_sdk::{DriverContext, ResourceRights, HardwarePort};

fn main() -> Result<(), DriverError> {
    // 1. Initialize the context and connect to the Driver Manager
    let mut ctx = DriverContext::init()?;

    // 2. Validate and claim hardware resources declared in driver.toml
    let ioports = ctx.take_io_ports("0x60").expect("Failed to get PS/2 IO ports");
    let irq = ctx.take_irq(1).expect("Failed to get IRQ 1");

    // 3. Register IPC endpoints (Cap'n Proto)
    let endpoint = ctx.create_ipc_endpoint("events");

    // 4. Enter the primary event loop
    run_loop(ctx, ioports, irq, endpoint)
}

```

### 2. Handling Hardware Interrupts (IRQs)

Jinn OS converts hardware interrupts into asynchronous IPC notification events. The kernel masks the interrupt at the APIC level and signals the registered driver thread. The driver handles the hardware state and signals the kernel to unmask the line.

```rust
fn run_loop(
    mut ctx: DriverContext, 
    mut ports: HardwarePort, 
    irq: IrqHandle,
    endpoint: IpcEndpoint
) -> Result<(), DriverError> {
    
    // Register event sources with the polling engine
    let mut poll = jinn_driver_sdk::EventPoll::new();
    poll.add(&irq, Token::IRQ);
    poll.add(&endpoint, Token::IPC);

    loop {
        // Block waiting for events (hardware interrupts or IPC requests)
        for event in poll.wait(None) {
            match event.token {
                Token::IRQ => {
                    // Read hardware registers
                    let scancode = ports.read_u8(0x60);
                    
                    // Signal the kernel to acknowledge and unmask the IRQ line
                    irq.ack(); 
                    
                    // Dispatch processed input to client endpoints
                    broadcast_key(scancode, &endpoint);
                },
                Token::IPC => {
                    // Handle client RPC requests
                    handle_client_message(endpoint.recv()?);
                }
            }
        }
    }
}

```

---

## 5. Memory Transfer (Zero-Copy & DMA)

High-throughput devices (network cards, storage controllers, GPUs) require high-performance memory pipelines.

### DMA (Direct Memory Access)

DMA allocations require the driver manifest to hold the `DMA_ALLOC` capability.

```rust
// Allocate physically contiguous pages and retrieve the hardware-accessible physical address
let dma_buffer = ctx.allocate_dma_buffer(4096)?;
let paddr = dma_buffer.physical_address();

// Program the hardware controller with the physical address pointer
setup_hardware_dma_rings(ports, paddr);

```

### Client Shared Memory (Zero-Copy)

To avoid throughput bottlenecks caused by IPC payload copying, map driver-managed physical pages directly into the client's virtual address space.

```rust
fn handle_read_request(req: IpcMessage, dma_buffer: &DmaBuffer) {
    // Map the buffer into the client domain with Read-Only permissions
    let capability = ctx.share_memory_page(
        req.sender_domain(), 
        dma_buffer.vaddr(), 
        Permission::READ_ONLY
    );
    
    // Return the capability token to the caller
    req.reply_with_capability(capability);
}

```

---

## 6. Debugging and Testing

Because drivers run as standard user-space programs, you can run and inspect them without kernel-level debuggers:

1. **Hardware Mocking:** Compile using the `mock-hw` feature flag in `jinn-driver-sdk` to simulate I/O ports and interrupt events on macOS or standard Linux environments.
2. **Unit Testing:** Run test suites using standard Cargo tooling (`cargo test`).
3. **Tracing:** Use `jinn-trace`, which routes `log::info!` macros directly to the centralized Jinn OS audit pipeline or serial output under QEMU.

```rust
log::info!("Device initialized with MAC: {:02x?}", mac_addr);

```

---

## 7. Best Practices & Pitfalls

* **Avoid Busy-Waiting:** Do not run CPU-spinning polling loops (`while status != READY`). Always block on `poll.wait()` or wait for hardware IRQ notifications. The scheduler lowers priorities on spinning user-space threads.
* **Avoid Heap Allocations in IRQ Handlers:** Use pre-allocated buffers or fixed ring buffers along the critical path to keep interrupt response times deterministic.
* **Zero-Trust Input Validation:** Treat all incoming IPC messages as untrusted input, even when received from other system-level services.
* **Set Synchronous Timeouts:** Always enforce explicit timeouts on outbound synchronous calls (`rpc_call`). Never allow an unresponsive dependency (such as a hung file system) to freeze your driver.

---

Welcome to safe systems programming.