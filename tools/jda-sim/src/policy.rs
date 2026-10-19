//! JDA Pluggable Scheduling Policies
//!
//! Implementa as políticas de seleção: Baseline FIFO,
//! Dataflow Locality & Priority Aware, e Política Preditiva.

use crate::graph::{DataflowGraph, NodeId};

pub trait JdaSchedulingPolicy: Send + Sync {
    fn name(&self) -> &'static str;

    fn select_next(
        &self,
        graph: &DataflowGraph,
        ready: &[NodeId],
        last_executed: Option<NodeId>,
    ) -> Option<NodeId>;
}

// ── 1. FIFO Reference Policy (Linha de Base) ──────────────────────────────────
pub struct FifoReferencePolicy;

impl JdaSchedulingPolicy for FifoReferencePolicy {
    fn name(&self) -> &'static str {
        "FIFO-Reference-Baseline"
    }

    fn select_next(
        &self,
        _graph: &DataflowGraph,
        ready: &[NodeId],
        _last_executed: Option<NodeId>,
    ) -> Option<NodeId> {
        ready.first().copied()
    }
}

// ── 2. Dataflow Locality & Priority Aware Policy ──────────────────────────────
pub struct PriorityLocalityPolicy;

impl JdaSchedulingPolicy for PriorityLocalityPolicy {
    fn name(&self) -> &'static str {
        "Dataflow-Locality-Priority-Aware"
    }

    fn select_next(
        &self,
        graph: &DataflowGraph,
        ready: &[NodeId],
        last_executed: Option<NodeId>,
    ) -> Option<NodeId> {
        if ready.is_empty() {
            return None;
        }

        let mut best_node = ready[0];
        let mut best_score = 0i32;

        for &id in ready {
            let node = &graph.nodes[id];
            let mut score = (node.priority as i32) * 10;

            // Bonificação de localidade temporal de cache:
            // Se este nó depende diretamente do nó que acabou de executar,
            // os dados têm altíssima probabilidade de residir na L1/L2!
            if let Some(last_id) = last_executed {
                if node.dependencies.contains(&last_id) {
                    score += 25;
                }
            }

            // Bonificação por possuir buffer já presente
            if node.input_buffer.is_some() {
                score += 5;
            }

            if score > best_score {
                best_score = score;
                best_node = id;
            }
        }

        Some(best_node)
    }
}

// ── 3. Predictive Critical-Path Policy ────────────────────────────────────────
pub struct PredictivePolicy {
    pub confidence_threshold: u8,
}

impl PredictivePolicy {
    pub fn new(confidence_threshold: u8) -> Self {
        Self { confidence_threshold }
    }
}

impl JdaSchedulingPolicy for PredictivePolicy {
    fn name(&self) -> &'static str {
        "Predictive-Critical-Path"
    }

    fn select_next(
        &self,
        graph: &DataflowGraph,
        ready: &[NodeId],
        last_executed: Option<NodeId>,
    ) -> Option<NodeId> {
        if ready.is_empty() {
            return None;
        }

        // Seleciona o nó cujo subgrafo dependente possui o maior caminho crítico
        let mut best_node = ready[0];
        let mut max_dependents = 0;

        for &id in ready {
            let dep_count = graph.nodes[id].dependents.len();
            let mut score = dep_count * 10 + (graph.nodes[id].priority as usize) * 5;

            if let Some(last_id) = last_executed {
                if graph.nodes[id].dependencies.contains(&last_id) {
                    score += 15;
                }
            }

            if score >= max_dependents {
                max_dependents = score;
                best_node = id;
            }
        }

        Some(best_node)
    }
}
