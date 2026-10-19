/// Jinn OS — JDA: Adaptive Policy Engine & Reference Policies
///
/// Define o contrato de políticas substituíveis para seleção de nós prontos,
/// estimativa de custo de transferência e decisão de pré-busca (prefetching).

use super::graph::{DataflowGraph, OpState};

pub trait JdaPolicy: Send + Sync {
    /// Nome descritivo da política
    fn name(&self) -> &'static str;

    /// Seleciona o próximo nó pronto para despacho a partir do grafo
    fn select_next(&self, graph: &DataflowGraph) -> Option<usize>;

    /// Estima o custo relativo em ciclos para transferir `bytes`
    fn estimate_transfer_cost(&self, bytes: usize, is_zero_copy: bool) -> u64;

    /// Avalia se vale a pena realizar pré-busca de dados com base na confiança e pressão de memória
    fn should_prefetch(&self, confidence: u8, memory_pressure: u8) -> bool;
}

// ── Política 1: Referência FIFO Baseline ─────────────────────────────────────
// Política simples e determinística: seleciona o primeiro nó pronto na ordem do vetor.
pub struct FifoReferencePolicy;

impl JdaPolicy for FifoReferencePolicy {
    fn name(&self) -> &'static str {
        "FIFO-Reference-Baseline"
    }

    fn select_next(&self, graph: &DataflowGraph) -> Option<usize> {
        let mask = graph.ready_mask;
        if mask == 0 {
            return None;
        }

        // Retorna o menor índice com bit ativo
        let trailing_zeros = mask.trailing_zeros() as usize;
        if trailing_zeros < graph.count {
            Some(trailing_zeros)
        } else {
            None
        }
    }

    fn estimate_transfer_cost(&self, bytes: usize, is_zero_copy: bool) -> u64 {
        if is_zero_copy {
            // Custo nominal de atualização de tabela de páginas / TLB flush
            120
        } else {
            // Estimativa de ~0.25 ciclos por byte em memcpy otimizado
            (bytes as u64) / 4 + 50
        }
    }

    fn should_prefetch(&self, _confidence: u8, _memory_pressure: u8) -> bool {
        // Política de referência não realiza prefetch especulativo
        false
    }
}

// ── Política 2: Dataflow Locality & Priority Aware ────────────────────────────
// Prioriza nós com maior prioridade e nós que compartilham buffers com nós recém-executados.
pub struct DataflowLocalityPolicy;

impl JdaPolicy for DataflowLocalityPolicy {
    fn name(&self) -> &'static str {
        "Dataflow-Locality-Priority-Aware"
    }

    fn select_next(&self, graph: &DataflowGraph) -> Option<usize> {
        let mask = graph.ready_mask;
        if mask == 0 {
            return None;
        }

        let mut best_idx: Option<usize> = None;
        let mut highest_score: u32 = 0;

        for i in 0..graph.count {
            if (mask & (1 << i)) != 0 {
                if let Some(node) = &graph.nodes[i] {
                    if node.state == OpState::Ready {
                        let mut score = (node.priority as u32) * 10;
                        // Favorece nós com buffers já alocados (localidade)
                        if node.input_buffer.is_some() {
                            score += 5;
                        }
                        if node.output_buffer.is_some() {
                            score += 5;
                        }
                        if score >= highest_score {
                            highest_score = score;
                            best_idx = Some(i);
                        }
                    }
                }
            }
        }

        best_idx
    }

    fn estimate_transfer_cost(&self, bytes: usize, is_zero_copy: bool) -> u64 {
        if is_zero_copy {
            80 // TLB invlpg direto
        } else {
            (bytes as u64) / 4 + 30
        }
    }

    fn should_prefetch(&self, confidence: u8, memory_pressure: u8) -> bool {
        // Apenas pré-busca se a confiança for alta (> 80) e a pressão de memória for baixa (< 40)
        confidence >= 80 && memory_pressure <= 40
    }
}
