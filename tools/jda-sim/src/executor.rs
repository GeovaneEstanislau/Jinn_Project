//! JDA Async Dataflow Executor
//!
//! Executa o Grafo Acíclico de Operações orientando o despacho pelas
//! políticas plugáveis e instrumentando a telemetria a cada transição.

use std::time::Instant;
use crate::graph::{DataflowGraph, OpState};
use crate::policy::JdaSchedulingPolicy;
use crate::telemetry::TelemetryReport;

pub struct AsyncDataflowExecutor<P: JdaSchedulingPolicy> {
    pub policy: P,
    pub use_zero_copy: bool,
}

impl<P: JdaSchedulingPolicy> AsyncDataflowExecutor<P> {
    pub fn new(policy: P, use_zero_copy: bool) -> Self {
        Self {
            policy,
            use_zero_copy,
        }
    }

    /// Executa o grafo até conclusão total ou falha irrecuperável
    pub fn execute(&self, graph: &mut DataflowGraph) -> Result<TelemetryReport, String> {
        let mut telemetry = TelemetryReport::new();
        telemetry.operations_submitted = graph.nodes.len() as u64;

        let start_wall = Instant::now();
        let mut last_executed = None;

        while !graph.is_completed() {
            let ready_nodes = graph.get_ready_nodes();
            if ready_nodes.is_empty() {
                // Se o grafo não completou mas não há nós prontos, houve deadlock ou falha
                return Err("Bloqueio detectado: nenhum no pronto e grafo incompleto".into());
            }

            // A política escolhe o próximo nó
            let node_id = match self.policy.select_next(graph, &ready_nodes, last_executed) {
                Some(id) => id,
                None => ready_nodes[0],
            };

            graph.nodes[node_id].state = OpState::Running;

            // Extrai a tarefa e o buffer de entrada
            let input_ref = graph.nodes[node_id].input_buffer.clone();
            if let Some(ref buf) = input_ref {
                telemetry.record_transfer(buf.len(), self.use_zero_copy);
            }

            let task_opt = graph.nodes[node_id].task.take();

            let node_start = Instant::now();
            let task_res = if let Some(ref task) = task_opt {
                task(input_ref.as_ref())
            } else {
                Ok(None)
            };
            let node_duration = node_start.elapsed();
            telemetry.latency.record(node_duration);

            // Devolve a closure do nó
            graph.nodes[node_id].task = task_opt;

            match task_res {
                Ok(output) => {
                    telemetry.operations_completed += 1;
                    graph.mark_completed(node_id, output, self.use_zero_copy);
                    last_executed = Some(node_id);
                }
                Err(err) => {
                    telemetry.operations_failed += 1;
                    graph.mark_failed(node_id);
                    return Err(format!("Falha no no {}: {}", node_id, err));
                }
            }
        }

        telemetry.wall_time = start_wall.elapsed();
        Ok(telemetry)
    }
}
