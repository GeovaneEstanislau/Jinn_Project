# Security Core — Jinn OS

## 1. Visão Geral

O `Security Core` é o subsistema fundamental responsável por garantir a integridade, o isolamento e o controle de acesso de todos os componentes do Jinn OS. Construído sobre os pilares de **Zero Trust**, **Privilégio Mínimo** (*Least Privilege*) e **Segurança Baseada em Capacidades** (*Capability-Based Security*), o Security Core elimina o conceito tradicional de superusuário monolítico (`root`), substituindo-o por tokens de capacidade delegáveis, tipados e criptograficamente verificáveis.

No Jinn OS, nenhum processo, driver ou serviço possui autoridade intrínseca. Qualquer operação sobre recursos do sistema — seja acesso a uma faixa de memória, comunicação através de um canal IPC, mapeamento de interrupção ou controle de hardware — exige a apresentação de uma capacidade válida mediada pelo kernel.

### Princípios de Design

- **Zero Trust por Padrão:** Nenhum serviço ou driver é confiável por omissão. Todas as transações entre domínios são explicitamente autenticadas e autorizadas.
- **Segurança por Capacidades Tipadas:** Acesso a recursos é governado por tokens intransferíveis e verificáveis que encapsulam objeto, direitos e validade.
- **Atenuação Estrita de Privilégios (*Privilege Confinement*):** Um domínio só pode transferir direitos iguais ou menores aos que possui, impedindo a escalada de privilégios.
- **Revogabilidade em Cascata:** Capacidades delegadas mantêm linhagem hierárquica, permitindo revogação pontual ou em árvore completa sem impactar outros subsistemas.
- **Desempenho Previsível e Baixa Latência:** A validação de capacidades no caminho crítico do IPC opera com alvos de latência `<100ns` através de caches locais e tabelas lock-free.
- **Isolamento de Falhas:** O comprometimento de um driver em user-space limita o raio de dano estritamente ao seu próprio espaço de endereçamento e C-Space.

---

## 2. Objetivos

- ✅ Implementar modelo puro de segurança baseado em capacidades (*Object Capabilities*).
- ✅ Mediar 100% das comunicações interprocessos (IPC) com verificação em tempo real.
- ✅ Prover isolamento forte (*sandboxing*) para drivers e serviços em user-space.
- ✅ Garantir revogação determinística e controle de ciclo de vida de privilégios.
- ✅ Integrar atestação de integridade de serviços (*Hardware & Software Attestation*).
- ✅ Manter trilha de auditoria (*Audit Log*) imutável e de alto desempenho para eventos de segurança.
- ✅ Oferecer validação de capacidades no hot path com overhead inferior a 5% da latência de IPC.

---

## 3. Responsabilidades

- **Gerenciamento de C-Spaces (*Capability Spaces*):** Manter e isolar as tabelas de capacidades associadas a cada domínio de segurança.
- **Emissão e Verificação de Tokens:** Validar requisições contra o *Policy Engine* e emitir tokens assinados com direitos delimitados.
- **Mediação de Acesso a Recursos:** Interceptar e validar tentativas de acesso a páginas físicas, portas I/O, interrupções e canais IPC.
- **Delegação e Atenuação:** Validar que qualquer capacidade gerada a partir de outra possua permissões estritamente contidas no conjunto original.
- **Revogação Hierárquica:** Rastrear a árvore de derivação (*derivation tree*) de capacidades e revogar descendentes quando o nó pai for invalidado.
- **Auditoria e Telemetria:** Registrar tentativas de acesso negadas (*denials*), anomalias de execução e eventos críticos para consumo pelo supervisor do sistema e pela Predictive Engine.

---

## 4. Arquitetura Interna

O Security Core é organizado em componentes modulares desacoplados:

```
┌─────────────────────────────────────────────────────────────────┐
│                      Process Supervisor / Admin                 │
│              (Configuração de políticas e auditoria)            │
└────────────────────────┬────────────────────────────────────────┘
                         │ (políticas de alto nível)
┌────────────────────────▼────────────────────────────────────────┐
│                      Policy Engine                              │
│   ┌──────────────┐ ┌──────────────┐ ┌──────────────┐            │
│   │ Driver Rules │ │ Service ACLs │ │ Sandbox Defs │ (plugins)  │
│   └──────────────┘ └──────────────┘ └──────────────┘            │
│   • Avaliação de regras declarativas                            │
│   • Cache de decisões de autorização (Fast-Path Decision Cache) │
└────────────────────────┬────────────────────────────────────────┘
                         │ (decisões allow / deny)
┌────────────────────────▼────────────────────────────────────────┐
│                   Auth & Capability Manager                     │
│  ┌─────────────────────────┐  ┌──────────────────────────────┐  │
│  │   Capability Store      │  │    Attestation Engine        │  │
│  │   (C-Spaces por Domínio)│  │    (Assinaturas / Hashes)    │  │
│  └─────────────────────────┘  └──────────────────────────────┘  │
│  • Emissão, derivação e revogação de tokens                      │
│  • Gerenciamento da árvore de linhagem de privilégios           │
└────────────────────────┬────────────────────────────────────────┘
                         │ (validação no kernel)
┌────────────────────────▼────────────────────────────────────────┐
│                 Kernel Security Enforcement                     │
│  [IPC Validation Gate] ── [Memory Cap Gate] ── [I/O Cap Gate]   │
│  • Validação O(1) inline no caminho de chamadas e mensagens     │
└────────────────────────┬────────────────────────────────────────┘
                         │ (eventos de segurança)
┌────────────────────────▼────────────────────────────────────────┐
│                      Audit Logger Engine                        │
│  • Ring buffer atômico sem bloqueio para registros de auditoria │
└─────────────────────────────────────────────────────────────────┘
```

### Componentes Principais

1. **Policy Engine:** Motor que avalia manifestos de drivers e políticas do sistema para conceder conjuntos iniciais de capacidades durante a inicialização de serviços.
2. **Capability Store:** Estrutura residente em memória que indexa os *Capability Spaces* (C-Spaces) de cada processo e as referências aos objetos do kernel.
3. **Enforcement Gates (Portais de Aplicação):** Pequenas rotinas embutidas no subsistema de IPC, memória e agendamento que realizam checagens imediatas de validade de slot e máscara de permissão.
4. **Audit Logger:** Buffer circular de alta velocidade que registra violações de acesso sem degradar o desempenho dos nós concorrentes.

---

## 5. Estruturas de Dados Principais

```rust
use core::sync::atomic::{AtomicU64, AtomicU32, Ordering};

/// Identificador único global de um domínio de segurança (processo/serviço)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct DomainId(pub u32);

/// Identificador único de uma capacidade emitida
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CapabilityId(pub u64);

/// Tipos de recursos protegidos por capacidades
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ResourceType {
    MemoryPage = 1,
    SharedMemory = 2,
    IpcEndpoint = 3,
    IpcChannel = 4,
    InterruptLine = 5,
    PortIO = 6,
    DeviceMMIO = 7,
    TaskControl = 8,
    SecurityManager = 9,
}

/// Máscara de permissões finas sobre o recurso
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PermissionFlags {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
    pub grant: bool,      // Permissão para delegar a terceiros
    pub mutate: bool,     // Permissão para alterar metadados do recurso
    pub attenuate: bool,  // Permissão para derivar com menos privilégios
    pub revoke: bool,     // Permissão para revogar instâncias derivadas
}

impl PermissionFlags {
    pub const READ_ONLY: Self = Self {
        read: true, write: false, execute: false, grant: false, mutate: false, attenuate: false, revoke: false,
    };
    
    pub const READ_WRITE: Self = Self {
        read: true, write: true, execute: false, grant: false, mutate: false, attenuate: false, revoke: false,
    };

    pub const FULL_CONTROL: Self = Self {
        read: true, write: true, execute: true, grant: true, mutate: true, attenuate: true, revoke: true,
    };

    /// Verifica se este conjunto de permissões contém estritamente outro (atenuação válida)
    pub fn contains(&self, other: &Self) -> bool {
        (!other.read || self.read) &&
        (!other.write || self.write) &&
        (!other.execute || self.execute) &&
        (!other.grant || self.grant) &&
        (!other.mutate || self.mutate) &&
        (!other.attenuate || self.attenuate) &&
        (!other.revoke || self.revoke)
    }
}

/// Token de Capacidade concreto
#[repr(C)]
pub struct CapabilityToken {
    pub id: CapabilityId,
    pub parent_id: Option<CapabilityId>,
    pub owner_domain: DomainId,
    pub resource_type: ResourceType,
    pub resource_handle: u64, // Endereço físico, ID de canal, IRQ, etc.
    pub permissions: PermissionFlags,
    pub generation: u32,
    pub is_valid: bool,
}

/// C-Space: Tabela de capacidades isolada por processo
pub const MAX_CAPABILITIES_PER_DOMAIN: usize = 1024;

pub struct CSpace {
    pub domain_id: DomainId,
    pub slots: [Option<CapabilityToken>; MAX_CAPABILITIES_PER_DOMAIN],
    pub free_slot_index: usize,
}

/// Registro de auditoria de segurança
#[repr(C)]
pub struct AuditEntry {
    pub timestamp_tsc: u64,
    pub subject: DomainId,
    pub resource_type: ResourceType,
    pub resource_id: u64,
    pub attempted_action: u8,
    pub granted: bool,
    pub error_code: u32,
}
```

---

## 6. Fluxos de Execução

### 1. Inicialização e Bootstrapping de Confiança
1. O kernel inicia o `Security Core` durante o boot (`_start`).
2. É criado o `Domain 0` (Root Kernel Services) com a capacidade mestra `FULL_CONTROL`.
3. O `Policy Engine` carrega a tabela imutável de permissões do sistema.
4. Conforme novos serviços (ex: Driver Manager) são lançados pelo supervisor, recebem seus C-Spaces iniciais pré-populados estritamente com os recursos declarados em seus manifestos.

### 2. Validação de Comunicação IPC (*Fast-Path*)
```
Sender (Domain A)               IPC Gate / Security Core              Receiver (Domain B)
      │                                    │                                    │
      ├─ send_message(cap_slot, msg) ────►│                                    │
      │                                    ├─ 1. Lookup slot no CSpace(A)       │
      │                                    ├─ 2. Verifica token.is_valid        │
      │                                    ├─ 3. Valida target == Domain B      │
      │                                    ├─ 4. Checa perms.write              │
      │                                    │                                    │
      │                                    ├── [SE VÁLIDO: O(1)] ──────────────►│ Entrega msg
      │                                    │                                    │
      │◄─ Retorna ERR_ACCESS_DENIED ───────┴── [SE INVÁLIDO: Log Audit]         │
```

### 3. Delegação de Capacidade com Atenuação
1. O Domínio A invoca a syscall `sec_cap_grant(src_slot, target_domain, attenuated_perms)`.
2. O Security Core valida:
   - Se `CSpace(A)[src_slot]` existe e possui a flag `grant == true`.
   - Se `src_token.permissions.contains(&attenuated_perms)` é verdadeiro.
3. Se válido, aloca um slot livre em `CSpace(B)` e cria um novo token derivado apontando `parent_id = Some(src_token.id)`.

### 4. Revogação em Cascata
1. O proprietário original invoca `sec_cap_revoke(slot)`.
2. O Security Core localiza o token e localiza recursivamente na árvore todos os tokens com `parent_id == token.id`.
3. Todos os tokens descendentes são invalidados atomicamente (`is_valid = false`).
4. Os recursos associados (ex: mapeamentos de memória compartilhada) são desfeitos e o TLB é invalidado.

---

## 7. Interfaces Públicas (Syscalls & Kernel API)

```rust
/// Syscalls expostas pelo subsistema de segurança para o user-space
pub trait SecuritySyscalls {
    /// Concede/transfere uma capacidade atenuada para outro domínio
    fn sys_cap_grant(
        src_slot: usize,
        target_domain: DomainId,
        attenuated_perms: PermissionFlags,
    ) -> Result<usize, SecurityError>; // Retorna o slot atribuído no destino

    /// Revoga uma capacidade e todos os seus descendentes derivados
    fn sys_cap_revoke(slot: usize) -> Result<(), SecurityError>;

    /// Inspeciona os direitos e status de um slot de capacidade
    fn sys_cap_inspect(slot: usize) -> Result<CapabilityToken, SecurityError>;

    /// Solicita a validação de atestação de integridade de um executável
    fn sys_service_attest(
        target: DomainId,
        expected_digest: &[u8; 32],
    ) -> Result<bool, SecurityError>;
}

/// Códigos de erro específicos do subsistema de segurança
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityError {
    InvalidSlot,
    SlotOccupied,
    CapabilityRevoked,
    AccessDenied,
    CannotAttenuate,
    GrantNotAllowed,
    TargetDomainNotFound,
    CSpaceFull,
    AuditBufferFull,
}
```

---

## 8. Integração com Outros Componentes

### Integração com IPC Core
- Toda mensagem em trânsito tem seu endpoint validado contra o C-Space do remetente.
- Suporte à transferência de capacidades empacotadas na própria mensagem IPC (*capability payload*), alocando slots automaticamente no receptor sob consentimento mútuo.

### Integração com Memory Core
- O `PageCapabilitySet` do subsistema de memória consulta o Security Core para autorizar mapeamentos virtuais (`PROT_READ`, `PROT_WRITE`, `PROT_EXEC`).
- Operações de memória compartilhada (DMA / Zero-Copy) exigem que ambos os domínios apresentem tokens compatíveis.

### Integração com Driver Manager & I/O
- Acesso a portas legadas x86 (`in`/`out`) e registradores MMIO é encapsulado em capacidades de hardware. O kernel só programa o TSS I/O Bitmap ou page tables para drivers que portem a capacidade apropriada.

---

## 9. Segurança e Isolamento

### Mitigação de Ataques Conhecidos

| Vetor de Ataque | Mecanismo de Proteção no Jinn |
|-----------------|-------------------------------|
| **Confused Deputy** | Tokens encapsulam explicitamente o domínio proprietário e o contexto do objeto; serviços não podem ser induzidos a usar autoridade alheia. |
| **Escalação de Privilégios** | Algoritmo de atenuação estrita impede que qualquer capacidade derivada exceda as permissões do emissor. |
| **Use-After-Revoke (TOCTOU)** | Checagens de token no hot path utilizam geração de token atômica e invalidação imediata de mapeamentos físicos. |
| **Driver Forgery / Spoofing** | Identidades de serviço são assinadas na inicialização e validadas contra o digest do ELF antes de receber o C-Space inicial. |
| **Denial of Service (DoS) em C-Space** | Cada processo possui cota rígida de capacidades (máx. 1024 slots) evitando esgotamento de memória no kernel. |

---

## 10. Escalabilidade e Limites

### Metas de Desempenho

| Operação | Alvo de Latência | Estratégia de Implementação |
|----------|------------------|-----------------------------|
| Validação de Slot (Hot Path) | `<100 ns` | Indexação direta por array fixo no CSpace local |
| Emissão de Capacidade | `<2 μs` | Alocação atômica em slot livre sem busca linear |
| Revogação em Cascata | `<15 μs` | Rastreamento por árvore de ponteiros direta |
| Escrita em Log de Auditoria | `<50 ns` | Atomic Ring Buffer sem travas (lock-free) |

### Limites do Sistema

- **Máximo de Domínios Concorrentes:** 4.096 domínios.
- **Capacidades por Domínio:** 1.024 slots.
- **Capacidade do Buffer de Auditoria:** 16.384 entradas em buffer circular contínuo.

---

## 11. Comparação com Sistemas Modernos

| Aspecto | Linux (SELinux/POSIX) | seL4 Microkernel | Fuchsia (Zircon) | Jinn OS Security Core |
|---------|-----------------------|------------------|------------------|-----------------------|
| **Modelo Base** | ACLs + Domínios Tipo | Capabilities Puras (CSpace) | Handles com Direitos | Tokens de Capacidade Híbridos |
| **Revogação** | Complexa / Assíncrona | Suporte nativo em árvore | Invalidação de Handle | Revogação Hierárquica em Cascata |
| **Verificabilidade** | Difícil (>30M LOC) | Verificado Formalmente | Em desenvolvimento | Projetado para Verificação em Rust |
| **Atenuação** | Não nativa | Sim | Sim (Rights Reduction) | Sim (Atenuação Estrita Tipada) |
| **Auditoria** | Auditd (User-space pesado)| Mínima no kernel | Event Log | Lock-free Ring Buffer Integrado |

---

## 12. Pseudocódigo Avançado

### Algoritmo 1: Emissão e Delegação com Atenuação

```pseudo
function cap_grant(src_domain, src_slot, target_domain, requested_perms):
    // 1. Localiza C-Space de origem
    src_cspace = get_cspace(src_domain)
    if src_slot >= MAX_CAPABILITIES:
        return ERR_INVALID_SLOT
        
    src_token = src_cspace.slots[src_slot]
    if src_token is NULL or not src_token.is_valid:
        return ERR_CAPABILITY_REVOKED
        
    // 2. Valida se a capacidade atual permite delegação
    if not src_token.permissions.grant:
        log_audit(src_domain, ACTION_GRANT, DENIED)
        return ERR_GRANT_NOT_ALLOWED
        
    // 3. Valida se as permissões solicitadas são um subconjunto estrito (atenuação)
    if not src_token.permissions.contains(requested_perms):
        log_audit(src_domain, ACTION_ATTENUATE, DENIED)
        return ERR_CANNOT_ATTENUATE
        
    // 4. Localiza slot livre no domínio de destino
    target_cspace = get_cspace(target_domain)
    target_slot = target_cspace.find_free_slot()
    if target_slot is NULL:
        return ERR_CSPACE_FULL
        
    // 5. Cria token derivado com novo ID e vínculo de parentesco
    new_token = CapabilityToken {
        id: generate_unique_cap_id(),
        parent_id: src_token.id,
        owner_domain: target_domain,
        resource_type: src_token.resource_type,
        resource_handle: src_token.resource_handle,
        permissions: requested_perms,
        generation: src_token.generation + 1,
        is_valid: true
    }
    
    target_cspace.slots[target_slot] = new_token
    log_audit(src_domain, ACTION_GRANT, SUCCESS)
    return target_slot
```

### Algoritmo 2: Validação Inline no Hot-Path de IPC

```pseudo
function validate_ipc_access(sender_domain, target_domain, cap_slot):
    cspace = get_cspace(sender_domain)
    
    // Acesso direto O(1)
    token = cspace.slots[cap_slot]
    
    if token is not NULL and token.is_valid:
        if token.resource_type == RESOURCE_IPC_CHANNEL and token.permissions.write:
            if token.resource_handle == target_domain.channel_id:
                return ALLOW_FAST_PATH
                
    // Se falhar a verificação rápida
    log_audit(sender_domain, ACTION_IPC_SEND, DENIED)
    return ERR_ACCESS_DENIED
```

### Algoritmo 3: Revogação Recursiva em Cascata

```pseudo
function cap_revoke(domain_id, slot):
    cspace = get_cspace(domain_id)
    target_token = cspace.slots[slot]
    
    if target_token is NULL or not target_token.is_valid:
        return ERR_INVALID_SLOT
        
    if not target_token.permissions.revoke and target_token.owner_domain != domain_id:
        return ERR_ACCESS_DENIED
        
    // Invalida o token raiz
    target_token.is_valid = false
    
    // Invalida todos os descendentes em todos os C-Spaces
    revoke_descendants(target_token.id)
    
    // Dispara limpeza de recursos associados
    cleanup_resource_mappings(target_token)
    
    return SUCCESS

function revoke_descendants(parent_id):
    for each cspace in all_active_cspaces():
        for slot = 0 to MAX_CAPABILITIES:
            token = cspace.slots[slot]
            if token is not NULL and token.is_valid:
                if token.parent_id == parent_id:
                    token.is_valid = false
                    // Recursão para sub-árvores
                    revoke_descendants(token.id)
```

---

## 13. Diagramas ASCII Avançados

### Árvore de Linhagem e Revogação de Privilégios

```
                  [ Root Domain / Kernel ]
                             │
                  Cap_001 {FULL_CONTROL}
                             │
               ┌─────────────┴─────────────┐
               ▼                           ▼
      [ Driver Manager ]          [ Process Supervisor ]
      Cap_002 {Read, Write, Grant} Cap_003 {Read, Grant}
               │                           │
        ┌──────┴──────┐                    │
        ▼             ▼                    ▼
   [ Audio Driver ] [ Network Driver ]  [ App Sandbox ]
   Cap_004 {Read}   Cap_005 {Read, Write} Cap_006 {Read}
        │
    (Revogando Cap_002 invalida atomicamente Cap_004 e Cap_005)
```

### Validação de Canal IPC com Gate de Segurança

```
 ┌───────────────┐                               ┌───────────────┐
 │ Processo A    │                               │ Processo B    │
 │ (C-Space A)   │                               │ (C-Space B)   │
 │ Slot 3: [Cap] │                               │ Slot 7: [Cap] │
 └───────┬───────┘                               └───────▲───────┘
         │ send(slot: 3, msg)                            │
         ▼                                               │
 ┌───────────────────────────────────────────────────────┴───────┐
 │                   IPC Enforcement Gate                        │
 │  1. Valida slot 3 no CSpace A (Tipo: IPC, Perms: Write)       │
 │  2. Compara token.target com DomainId(B)                      │
 │  3. Valida timestamp de revogação                             │
 └───────────────────────────────┬───────────────────────────────┘
                                 │
                   [ Decisão: Permitido ] ──► Mensagem entregue
```

---

## 14. Considerações para Implementação em Rust

### Tipagem Forte e Bitflags Seguras

```rust
// Garantia em tempo de compilação contra bits inválidos
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CapRights(u32);

impl CapRights {
    pub const READ: u32    = 1 << 0;
    pub const WRITE: u32   = 1 << 1;
    pub const EXEC: u32    = 1 << 2;
    pub const GRANT: u32   = 1 << 3;
    pub const REVOKE: u32  = 1 << 4;

    #[inline(always)]
    pub fn has_right(&self, right: u32) -> bool {
        (self.0 & right) == right
    }

    #[inline(always)]
    pub fn is_subset_of(&self, parent: CapRights) -> bool {
        (self.0 & !parent.0) == 0
    }
}
```

### Justificativas de `unsafe` e Mitigações

1. **Acesso Concorrente a C-Spaces por Diferentes CPUs:**
   - **Risco:** Data races em slots de capacidade durante leitura no IPC e revogação simultânea.
   - **Mitigação:** Cada slot é encapsulado em campos atômicos (`AtomicU64` para o ID e `AtomicBool` para `is_valid`) garantindo leituras *lock-free* com semântica de memória `Acquire`/`Release`.
2. **Manipulação de Page Tables via Memory Gate:**
   - **Risco:** Aplicação incorreta de bits `NX` ou `RW` permitindo injeção de código.
   - **Mitigação:** O Security Core opera estritamente através do `MemoryCore` validando limites físicos antes de invocar instruções de CPU (`invlpg`).

---

## 15. Roadmap de Implementação

### Fase 0 (v0.0.1 — Atual)
- [x] Especificação arquitetural completa e modelo conceitual
- [x] Definição de estruturas básicas de tokens e domínios

### Fase 1 (v0.0.2 — Próximo Milestone)
- [ ] Implementação do `CSpace` em Rust no kernel
- [ ] Primitivas de concessão (`sys_cap_grant`) e validação
- [ ] Integração do gate de checagem no caminho de envio do IPC

### Fase 2 (v0.1.0)
- [ ] Árvore de derivação e revogação recursiva em cascata
- [ ] Validação de integridade de executáveis (digest de ELF)
- [ ] Mapeamento seguro de portas I/O e MMIO via capacidades

### Fase 3 (v0.2.0)
- [ ] Integração com TPM 2.0 / Secure Boot para Hardware Attestation
- [ ] Exportação de telemetria de segurança para o Predictive Engine

### Fase 4 (v1.0.0)
- [ ] Verificação formal de invariantes de não-escalação de privilégios
- [ ] C-Spaces dinâmicos com particionamento NUMA

---

## 16. Checklist de Revisão

- [x] Separação estrita entre mecanismo de validação e políticas de autorização
- [x] Modelo de Capabilities 100% tipado e sem privilégios implícitos
- [x] Suporte documentado para atenuação estrita e revogação em cascata
- [x] Estruturas de dados Rust completas (`CapabilityToken`, `CSpace`, `PermissionFlags`, `AuditEntry`)
- [x] Pseudocódigos para os 3 algoritmos essenciais (Emissão, Validação e Revogação)
- [x] Diagramas ASCII de arquitetura, linhagem e mediação de IPC
- [x] Padrões de código seguro em Rust sem rely desnecessário de `unsafe`
- [x] Metas de latência (<100ns no hot path) e limites claros do sistema

---

Arquivo: [Security Core](./security-core.md)
