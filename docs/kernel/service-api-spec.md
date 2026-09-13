# Service API Specifications — Jinn OS

## 1. Visão Geral

No Jinn OS, a comunicação entre processos (IPC) é a base de todo o ecossistema. Para garantir a interoperabilidade, manutenibilidade e segurança, todas as APIs de serviços em *user-space* devem seguir as **Convenções de Service API**.

A especificação de serviço (*Service API Spec*) define:
- Os esquemas (schemas) de mensagens.
- O formato de serialização padrão.
- Regras rígidas para tratamento de erros.
- Versionamento e compatibilidade binária.

## 2. Formato de Serialização Padrão

Para manter a latência do IPC síncrono próxima de zero, o Jinn OS exige o uso de formatos **Zero-Copy Serialization** baseados no mapeamento de memória.

**Formatos Suportados Oficialmente:**
1. **Cap'n Proto (Padrão e Recomendado):** Otimizado para alocação em arena, não requer parsing (estruturas em memória são idênticas ao wire-format).
2. **FlatBuffers:** Alternativa suportada para ambientes com restrição severa de memória.
3. **Raw Structs (C-ABI):** Permitido *apenas* para Fast-Path IPC via registradores de CPU (cargas < 64 bytes).

*Atenção: Formatos que exigem parsing de texto ou alocação dinâmica intensa (JSON, XML, Protobuf tradicional) são explicitamente banidos para interfaces de sistema essenciais.*

---

## 3. Estrutura de uma Service API (Cap'n Proto)

Cada serviço expõe sua interface primária através de um arquivo `.capnp`.

### Exemplo: Serviço de Sistema de Arquivos (VFS)

```capnp
@0xabcdef1234567890; # ID único do arquivo gerado

# Todos os protocolos Jinn usam o namespace base
using Cxx = import "/capnp/c++.capnp";
$Cxx.namespace("jinn::vfs::v1");

# Tipos Básicos de Retorno
struct Error {
  code @0 :Int32;
  message @1 :Text;
}

# A interface primária
interface VirtualFileSystem {
  
  # Operação Simples Síncrona
  openFile @0 (path :Text, flags :UInt32) -> (fd :UInt64, error :Error);
  
  # Operação com Capability Passing
  # O servidor de arquivos retorna uma nova Capability para o arquivo
  openFileWithCap @1 (path :Text, flags :UInt32) -> (fileCap :Capability, error :Error);
  
  # Envio Assíncrono com Zero-Copy
  # O 'data' é mapeado via Shared Memory se exceder o MTU de mensagem pequena
  writeStream @2 (fd :UInt64, offset :UInt64, data :Data) -> (written :UInt64, error :Error);
}
```

---

## 4. Tipagem Estrita e Capabilities

No Jinn OS, a passagem de permissões (capabilities) ocorre de forma nativa através do schema IPC.

Se um serviço fornece acesso a um recurso, ele **NÃO** retorna handles inteiros descritivos (como FDs do POSIX tradicionais que são globais por processo). Ele deve retornar e transacionar **Capability Tokens**.

```capnp
# O tipo "Capability" no Cap'n Proto do Jinn OS mapeia diretamente
# para o envio do token do C-Space via IPC Core.
interface DeviceManager {
  # Requere a capacidade de "ADMIN" para ser invocado
  requestHardwareAccess @0 (deviceString :Text, adminCap :Capability) -> (deviceCap :Capability);
}
```

---

## 5. Convenções de Tratamento de Erros

A API do Jinn OS desencoraja exceções e retornos globais obscuros (`errno`). O tratamento de erros deve ser explícito em todas as assinaturas de resposta.

**Regras:**
1. Toda função RPC deve retornar uma struct explícita de `Result` ou conter um campo `error :Error`.
2. Códigos de erro devem seguir a tabela padrão `JinnErrorCodes` (ex: `0 = SUCCESS`, `1 = ERR_ACCESS_DENIED`, etc).
3. Respostas que geram erro não devem alocar objetos complexos de resposta, apenas a struct de erro.

```capnp
# Padrão Recomendado (Result Wrapper)
struct OpenResult {
  union {
    success @0 :Capability;
    error @1 :Error;
  }
}
```

---

## 6. Versionamento

O versionamento é imposto no nível de namespace e UUID do esquema.

- Modificações aditivas (adicionar novo método ao final da interface) **NÃO** quebram a versão primária.
- Alteração da ordem de métodos, remoção de métodos ou mudança em tipos existentes **QUEBRAM** a ABI.
- APIs incompatíveis requerem um novo namespace (ex: `jinn::vfs::v2`) e um novo arquivo de schema.

---

## 7. Template de Schema (Boilerplate)

```capnp
# service_template.capnp
# Template base para novos serviços Jinn OS

@0x[GERAR_UUID_AQUI];

$Cxx.namespace("jinn::[DOMINIO]::v1");

struct Error {
  code @0 :UInt32;
  message @1 :Text;
}

# Wrapper genérico
struct OperationResult {
  union {
    success @0 :Void;
    error @1 :Error;
  }
}

interface [NomeDoServico] {
  # Exemplo de método ping/pong de diagnóstico
  ping @0 () -> (status :UInt32);
  
  # Adicione métodos do serviço aqui
}
```

---

## 8. Integração e Compilação

Para compilar um `.capnp` e gerar as stubs do cliente e servidor em Rust:

1. No `Cargo.toml` do serviço, adicione a dependência de compilação:
```toml
[build-dependencies]
capnpc = "0.14"

[dependencies]
capnp = "0.14"
capnp-rpc = "0.14"
```

2. No `build.rs`:
```rust
fn main() {
    capnpc::CompilerCommand::new()
        .file("src/schema/my_service.capnp")
        .run()
        .expect("Compiling Cap'n Proto schema");
}
```

Os provedores de serviço devem implementar a *trait* gerada no lado `Server` e registrá-la no *IPC Core Server Loop*.
