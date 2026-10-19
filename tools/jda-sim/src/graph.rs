//! JDA Dataflow Directed Acyclic Graph (DAG) Engine
//!
//! Modela nós de computação, transferência de dados e barreiras,
//! com resolução estrita de dependências e detecção de ciclos.

use std::sync::Arc;

pub type NodeId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Compute,
    MemoryTransfer,
    IoAsync,
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

/// Representação flexível de buffer de dados com suporte a Cópia vs Zero-Copy (Arc)
#[derive(Debug, Clone)]
pub enum DataBuffer {
    /// Cópia dedicada de memória (tradicional)
    Owned(Vec<u8>),
    /// Referência compartilhada somente leitura (Zero-Copy)
    Shared(Arc<Vec<u8>>),
}

impl DataBuffer {
    pub fn new_owned(data: Vec<u8>) -> Self {
        Self::Owned(data)
    }

    pub fn new_shared(data: Arc<Vec<u8>>) -> Self {
        Self::Shared(data)
    }

    pub fn as_slice(&self) -> &[u8] {
        match self {
            Self::Owned(v) => v.as_slice(),
            Self::Shared(a) => a.as_slice(),
        }
    }

    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn is_zero_copy(&self) -> bool {
        matches!(self, Self::Shared(_))
    }

    pub fn to_owned_copy(&self) -> Self {
        Self::Owned(self.as_slice().to_vec())
    }

    pub fn to_shared_ref(&self) -> Self {
        match self {
            Self::Shared(a) => Self::Shared(Arc::clone(a)),
            Self::Owned(v) => Self::Shared(Arc::new(v.clone())),
        }
    }
}

pub type TaskFn = Box<dyn Fn(Option<&DataBuffer>) -> Result<Option<DataBuffer>, String> + Send + Sync>;

pub struct DataflowNode {
    pub id: NodeId,
    pub name: String,
    pub kind: OpKind,
    pub state: OpState,
    pub priority: u8,
    pub dependencies: Vec<NodeId>,
    pub dependents: Vec<NodeId>,
    pub dependencies_resolved: usize,
    pub input_buffer: Option<DataBuffer>,
    pub output_buffer: Option<DataBuffer>,
    pub task: Option<TaskFn>,
}

pub struct DataflowGraph {
    pub nodes: Vec<DataflowNode>,
}

impl DataflowGraph {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    pub fn add_node<F>(&mut self, name: &str, kind: OpKind, priority: u8, task: F) -> NodeId
    where
        F: Fn(Option<&DataBuffer>) -> Result<Option<DataBuffer>, String> + Send + Sync + 'static,
    {
        let id = self.nodes.len();
        self.nodes.push(DataflowNode {
            id,
            name: name.to_string(),
            kind,
            state: OpState::Ready, // Se não tiver dependências, nasce pronto
            priority,
            dependencies: Vec::new(),
            dependents: Vec::new(),
            dependencies_resolved: 0,
            input_buffer: None,
            output_buffer: None,
            task: Some(Box::new(task)),
        });
        id
    }

    /// Adiciona dependência: `consumer` só pode executar após `producer` concluir
    pub fn add_dependency(&mut self, consumer: NodeId, producer: NodeId) -> Result<(), String> {
        if consumer >= self.nodes.len() || producer >= self.nodes.len() {
            return Err("Identificador de no inexistente".into());
        }
        if consumer == producer {
            return Err("Auto-dependencia ciclica nao permitida".into());
        }

        self.nodes[consumer].dependencies.push(producer);
        self.nodes[producer].dependents.push(consumer);

        // Se ainda tem dependências pendentes, coloca em espera
        if self.nodes[consumer].dependencies_resolved < self.nodes[consumer].dependencies.len() {
            self.nodes[consumer].state = OpState::WaitingDependencies;
        }

        // Valida aciclicidade
        self.detect_cycles()?;
        Ok(())
    }

    /// Detecção de ciclos via algoritmo DFS
    pub fn detect_cycles(&self) -> Result<(), String> {
        let n = self.nodes.len();
        let mut visited = vec![false; n];
        let mut in_stack = vec![false; n];

        for i in 0..n {
            if !visited[i] && self.dfs_cycle(i, &mut visited, &mut in_stack) {
                return Err("Ciclo detectado no grafo de dependencias".into());
            }
        }
        Ok(())
    }

    fn dfs_cycle(&self, u: usize, visited: &mut [bool], in_stack: &mut [bool]) -> bool {
        visited[u] = true;
        in_stack[u] = true;

        for &v in &self.nodes[u].dependents {
            if !visited[v] {
                if self.dfs_cycle(v, visited, in_stack) {
                    return true;
                }
            } else if in_stack[v] {
                return true;
            }
        }

        in_stack[u] = false;
        false
    }

    /// Retorna todos os nós prontos para execução
    pub fn get_ready_nodes(&self) -> Vec<NodeId> {
        self.nodes
            .iter()
            .filter(|n| n.state == OpState::Ready)
            .map(|n| n.id)
            .collect()
    }

    /// Notifica conclusão de um nó e propaga buffers e resoluções
    pub fn mark_completed(
        &mut self,
        node_id: NodeId,
        output: Option<DataBuffer>,
        use_zero_copy: bool,
    ) {
        self.nodes[node_id].state = OpState::Completed;
        self.nodes[node_id].output_buffer = output.clone();

        let dependents = self.nodes[node_id].dependents.clone();

        for dep_id in dependents {
            let dep = &mut self.nodes[dep_id];
            dep.dependencies_resolved += 1;

            // Propagação de buffer de dados
            if let Some(ref out) = output {
                if dep.input_buffer.is_none() {
                    dep.input_buffer = if use_zero_copy {
                        Some(out.to_shared_ref())
                    } else {
                        Some(out.to_owned_copy())
                    };
                }
            }

            if dep.dependencies_resolved >= dep.dependencies.len()
                && dep.state == OpState::WaitingDependencies
            {
                dep.state = OpState::Ready;
            }
        }
    }

    pub fn mark_failed(&mut self, node_id: NodeId) {
        self.nodes[node_id].state = OpState::Failed;
        // Cancela dependentes em cascata
        let dependents = self.nodes[node_id].dependents.clone();
        for dep_id in dependents {
            self.nodes[dep_id].state = OpState::Cancelled;
        }
    }

    pub fn is_completed(&self) -> bool {
        self.nodes.iter().all(|n| {
            matches!(
                n.state,
                OpState::Completed | OpState::Failed | OpState::Cancelled
            )
        })
    }
}
