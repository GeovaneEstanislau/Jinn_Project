/// Jinn OS — JDA: Dataflow Graph & Operation Model
///
/// Implementa o Grafo Acíclico Dirigido (DAG) de operações orientadas a fluxo de dados.
///
/// Cada nó representa uma unidade de computação, transferência de memória ou I/O
/// cujas dependências são resolvidas via máscaras de bits de 64 bits sem travamentos.

pub const MAX_JDA_NODES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Compute,
    MemoryTransfer,
    IoRead,
    IoWrite,
    Barrier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpState {
    WaitingDependencies,
    Ready,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionTarget {
    AnyCpu,
    Core(u8),
    VectorUnit,
    Accelerator(u8),
}

/// Descritor de buffer de memória com suporte a transferência Zero-Copy.
#[derive(Debug, Clone, Copy)]
pub struct DataBufferRef {
    pub phys_addr: u64,
    pub virt_addr: u64,
    pub size_bytes: usize,
    pub is_shared: bool,
    pub owner_pid: u32,
}

impl DataBufferRef {
    pub const fn empty() -> Self {
        Self {
            phys_addr: 0,
            virt_addr: 0,
            size_bytes: 0,
            is_shared: false,
            owner_pid: 0,
        }
    }
}

/// Nó do grafo Dataflow
#[derive(Debug, Clone, Copy)]
pub struct DataflowNode {
    pub id: u32,
    pub kind: OpKind,
    pub state: OpState,
    pub target: ExecutionTarget,
    pub priority: u8,

    /// Máscara de dependências obrigatórias (bits 0..63 representam nós predecessores)
    pub dependencies_mask: u64,

    /// Máscara de dependências que já concluíram com sucesso
    pub resolved_deps_mask: u64,

    /// Buffer de entrada de dados (consumido pela operação)
    pub input_buffer: Option<DataBufferRef>,

    /// Buffer de saída de dados (produzido pela operação)
    pub output_buffer: Option<DataBufferRef>,

    /// Ciclos TSC de início e término para telemetria de latência
    pub cycles_started: u64,
    pub cycles_finished: u64,

    /// Código de retorno / status de execução
    pub result_code: i32,
}

impl DataflowNode {
    pub const fn empty() -> Self {
        Self {
            id: 0,
            kind: OpKind::Compute,
            state: OpState::WaitingDependencies,
            target: ExecutionTarget::AnyCpu,
            priority: 0,
            dependencies_mask: 0,
            resolved_deps_mask: 0,
            input_buffer: None,
            output_buffer: None,
            cycles_started: 0,
            cycles_finished: 0,
            result_code: 0,
        }
    }

    /// Verifica se todas as dependências do nó foram satisfeitas
    #[inline]
    pub fn are_dependencies_met(&self) -> bool {
        (self.dependencies_mask & !self.resolved_deps_mask) == 0
    }
}

/// Grafo de fluxo de dados em memória pré-alocada (no_std)
pub struct DataflowGraph {
    pub nodes: [Option<DataflowNode>; MAX_JDA_NODES],
    pub count: usize,
    pub ready_mask: u64,
}

impl DataflowGraph {
    pub const fn new() -> Self {
        const NONE_NODE: Option<DataflowNode> = None;
        Self {
            nodes: [NONE_NODE; MAX_JDA_NODES],
            count: 0,
            ready_mask: 0,
        }
    }

    /// Adiciona um novo nó ao grafo
    pub fn add_node(
        &mut self,
        kind: OpKind,
        target: ExecutionTarget,
        priority: u8,
        input: Option<DataBufferRef>,
        output: Option<DataBufferRef>,
    ) -> Result<u32, &'static str> {
        if self.count >= MAX_JDA_NODES {
            return Err("Limite maximo de nos no grafo JDA atingido");
        }

        let id = self.count as u32;
        let node = DataflowNode {
            id,
            kind,
            state: OpState::Ready, // Inicialmente sem dependências, está pronto
            target,
            priority,
            dependencies_mask: 0,
            resolved_deps_mask: 0,
            input_buffer: input,
            output_buffer: output,
            cycles_started: 0,
            cycles_finished: 0,
            result_code: 0,
        };

        self.nodes[id as usize] = Some(node);
        self.ready_mask |= 1 << id;
        self.count += 1;
        Ok(id)
    }

    /// Estabelece uma dependência: `consumer_id` só pode executar após `producer_id` concluir
    pub fn add_dependency(&mut self, consumer_id: u32, producer_id: u32) -> Result<(), &'static str> {
        if consumer_id as usize >= self.count || producer_id as usize >= self.count {
            return Err("Identificador de no invalido");
        }
        if consumer_id == producer_id {
            return Err("Auto-dependencia ciclica nao permitida");
        }

        if let Some(consumer) = self.nodes[consumer_id as usize].as_mut() {
            consumer.dependencies_mask |= 1 << producer_id;
            // Se o produtor ainda não concluiu, consumidor fica em WaitingDependencies
            if !consumer.are_dependencies_met() {
                consumer.state = OpState::WaitingDependencies;
                self.ready_mask &= !(1 << consumer_id);
            }
        }
        Ok(())
    }

    /// Notifica que o nó `node_id` concluiu com sucesso
    pub fn mark_completed(&mut self, node_id: u32, cycles_finished: u64) {
        if node_id as usize >= self.count {
            return;
        }

        if let Some(node) = self.nodes[node_id as usize].as_mut() {
            node.state = OpState::Completed;
            node.cycles_finished = cycles_finished;
        }
        self.ready_mask &= !(1 << node_id);

        let completed_bit = 1 << node_id;
        // Propaga resolução para todos os dependentes
        for i in 0..self.count {
            if let Some(node) = self.nodes[i].as_mut() {
                if (node.dependencies_mask & completed_bit) != 0 {
                    node.resolved_deps_mask |= completed_bit;
                    if node.state == OpState::WaitingDependencies && node.are_dependencies_met() {
                        node.state = OpState::Ready;
                        self.ready_mask |= 1 << i;
                    }
                }
            }
        }
    }

    /// Limpa o grafo para nova rodada de execução
    pub fn reset(&mut self) {
        for slot in self.nodes.iter_mut() {
            *slot = None;
        }
        self.count = 0;
        self.ready_mask = 0;
    }
}
