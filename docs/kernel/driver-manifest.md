# Driver Manifest Specification — Jinn OS

## 1. Visão Geral

No Jinn OS, todos os drivers executam no espaço de usuário (*user-space*) como processos isolados. Para que o `Driver Manager` e o `Security Core` possam carregar, inicializar e conceder privilégios a um driver, ele precisa ser acompanhado de um **Manifesto**. 

O Manifesto de Driver é um arquivo declarativo em formato **TOML** que define metadados, permissões de segurança estritas (capabilities), recursos de hardware necessários e a ABI do driver.

## 2. Estrutura TOML

O formato TOML foi escolhido por ser legível para humanos e de fácil parsing estático durante a construção do sistema.

A estrutura do manifesto é dividida em seções principais:
1. `[driver]` - Metadados básicos
2. `[resources]` - Recursos de hardware solicitados
3. `[capabilities]` - Permissões do Security Core
4. `[dependencies]` - Dependências de outros serviços/drivers
5. `[abi]` - Especificação da interface binária

---

## 3. Metadata Obrigatória (`[driver]`)

Define a identidade e as características fundamentais do driver.

```toml
[driver]
name = "nvme-core"
version = "1.0.0"
author = "Jinn OS Team"
description = "NVMe Storage Driver for Jinn OS"
class = "storage"         # Classes: storage, network, display, audio, input, etc.
auto_start = true         # Iniciar automaticamente se o hardware for detectado
```

---

## 4. Recursos Necessários (`[resources]`)

Define os recursos físicos de hardware (MMIO, portas I/O, interrupções) que o driver solicita para operar. O sistema só iniciará o driver se esses recursos estiverem disponíveis e o dispositivo for autenticado (ex: via barramento PCI).

```toml
[resources]
# Solicita acesso a registradores MMIO via barramento PCI
pci_class = "01:08:02"    # Mass Storage Controller, NVMe
pci_vendor_id = "0x8086"  # Opcional (restritivo a vendor)
pci_device_id = "0xF1A6"  # Opcional (restritivo a device)

# Solicita IRQ (Interrupções) exclusivas ou compartilhadas
irq = { type = "msi-x", count = 4 }

# Portas legadas I/O x86 (apenas se absolutamente necessário)
# io_ports = ["0x1F0-0x1F7", "0x3F6"]
```

---

## 5. Permissões e Capabilities (`[capabilities]`)

Esta é a seção mais crítica. Sob o modelo Zero-Trust, o driver só terá acesso aos recursos e syscalls explicitamente declarados aqui.

```toml
[capabilities]
# Permissões do Memory Core
memory = [
  "DMA_ALLOC",       # Permissão para alocar páginas contíguas p/ DMA
  "PIN_PAGES"        # Permissão para impedir swap de páginas de I/O
]

# Permissões do Security Core / IPC
ipc = [
  "CREATE_CHANNEL",  # Criar canais IPC para escutar clientes
  "GRANT_CAPS"       # Pode delegar acesso a buffers criados
]

# Acesso ao VFS (Virtual File System)
# Drivers geralmente não acessam arquivos diretamente, apenas expõem blocos
vfs = [] 
```

---

## 6. ABI Specification (`[abi]`)

Define como outros serviços se comunicarão com este driver via IPC. O Jinn OS usa um sistema de tipagem estrita para mensagens IPC.

```toml
[abi]
protocol = "jinn.storage.block.v1"
endpoints = [
  { name = "control", type = "rpc", description = "Admin commands (format, flush)" },
  { name = "data", type = "ring_buffer", description = "High-throughput block read/write" }
]
```

---

## 7. Exemplos Práticos

### Exemplo 1: Driver de Teclado PS/2 (Legado)

```toml
[driver]
name = "ps2-kbd"
version = "0.1.0"
class = "input"
auto_start = true

[resources]
io_ports = ["0x60", "0x64"]
irq = { type = "legacy", count = 1, line = 1 }

[capabilities]
memory = []
ipc = ["CREATE_CHANNEL"]

[abi]
protocol = "jinn.input.keyboard.v1"
endpoints = [
  { name = "events", type = "ring_buffer", direction = "tx" }
]
```

### Exemplo 2: Driver de Placa de Rede Intel e1000

```toml
[driver]
name = "e1000"
version = "1.2.0"
class = "network"
auto_start = true

[resources]
pci_vendor_id = "0x8086"
pci_device_id = "0x100E"
irq = { type = "msi", count = 1 }

[capabilities]
memory = ["DMA_ALLOC", "PIN_PAGES"]
ipc = ["CREATE_CHANNEL"]

[abi]
protocol = "jinn.network.ethernet.v1"
endpoints = [
  { name = "tx_ring", type = "ring_buffer", direction = "rx" },
  { name = "rx_ring", type = "ring_buffer", direction = "tx" }
]
```

---

## 8. Template Básico (`manifest_template.toml`)

Use este template ao iniciar o desenvolvimento de um novo driver.

```toml
# ==========================================
# Jinn OS - Driver Manifest Template
# ==========================================

[driver]
name = ""
version = "0.1.0"
author = ""
description = ""
class = "misc"            # storage, network, display, audio, input, misc
auto_start = false

[resources]
# Descomente e preencha o que for necessário
# pci_class = ""          # Ex: "02:00:00" (Network/Ethernet)
# pci_vendor_id = ""      # Ex: "0x10EC" (Realtek)
# pci_device_id = ""
# irq = { type = "msi-x", count = 1 }
# io_ports = []

[capabilities]
memory = []               # Ex: ["DMA_ALLOC", "PIN_PAGES"]
ipc = ["CREATE_CHANNEL"]

[dependencies]
# Outros drivers/serviços requeridos
# Ex: pci_bus = "1.0.0"

[abi]
protocol = ""             # Ex: "jinn.device.custom.v1"
endpoints = []
```
