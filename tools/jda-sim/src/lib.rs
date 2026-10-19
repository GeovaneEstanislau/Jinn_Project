//! JDA Simulator & Reference Prototype Crate
//!
//! Fornece o motor de grafo assíncrono dirigido (DAG), políticas de escalonamento,
//! executor com simulação de Zero-Copy vs Cópia de Buffers, telemetria estatística
//! e suíte de cargas de trabalho sintéticas.

pub mod graph;
pub mod policy;
pub mod executor;
pub mod telemetry;
pub mod workloads;

pub use graph::{DataflowGraph, DataflowNode, DataBuffer, OpKind, OpState, NodeId};
pub use policy::{JdaSchedulingPolicy, FifoReferencePolicy, PriorityLocalityPolicy, PredictivePolicy};
pub use executor::AsyncDataflowExecutor;
pub use telemetry::{LatencyTracker, LatencyStats, TelemetryReport};
