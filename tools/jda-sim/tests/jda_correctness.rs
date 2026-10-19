//! JDA Correctness & Verification Test Suite
//!
//! Valida os invariantes formais do modelo Dataflow:
//! 1. Resolução estrita de dependências topológicas
//! 2. Rejeição de ciclos e auto-dependências
//! 3. Barreira de sincronização fork-join (Diamante)
//! 4. Equivalência de dados Zero-Copy vs Cópia Física
//! 5. Cancelamento em cascata em caso de falha de nó
//! 6. Determinismo e ausência de deadlocks

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use jda_sim::executor::AsyncDataflowExecutor;
use jda_sim::graph::{DataBuffer, DataflowGraph, OpKind, OpState};
use jda_sim::policy::{FifoReferencePolicy, PriorityLocalityPolicy};

#[test]
fn test_dag_topological_order() {
    let mut g = DataflowGraph::new();
    let order = Arc::new(parking_lot_sim::Mutex::new(Vec::new()));

    let order_clone1 = Arc::clone(&order);
    let n1 = g.add_node("Produtor", OpKind::Compute, 1, move |_| {
        order_clone1.lock().push(1);
        Ok(Some(DataBuffer::new_owned(vec![42])))
    });

    let order_clone2 = Arc::clone(&order);
    let n2 = g.add_node("Consumidor", OpKind::Compute, 1, move |inp| {
        order_clone2.lock().push(2);
        assert_eq!(inp.unwrap().as_slice(), &[42]);
        Ok(Some(DataBuffer::new_owned(vec![84])))
    });

    assert!(g.add_dependency(n2, n1).is_ok());

    let exec = AsyncDataflowExecutor::new(FifoReferencePolicy, true);
    let rep = exec.execute(&mut g).expect("Execucao falhou");

    assert_eq!(rep.operations_completed, 2);
    let log = order.lock().clone();
    assert_eq!(log, vec![1, 2], "Produtor DEVE executar estritamente antes do consumidor");
}

#[test]
fn test_dag_cycle_detection_simple() {
    let mut g = DataflowGraph::new();
    let n1 = g.add_node("A", OpKind::Compute, 1, |_| Ok(None));
    let n2 = g.add_node("B", OpKind::Compute, 1, |_| Ok(None));

    assert!(g.add_dependency(n2, n1).is_ok());
    // Adicionar ciclo n1 depende de n2 (A -> B -> A)
    let res = g.add_dependency(n1, n2);
    assert!(res.is_err(), "Ciclo A -> B -> A deve ser rejeitado");
}

#[test]
fn test_dag_cycle_detection_triangle() {
    let mut g = DataflowGraph::new();
    let n1 = g.add_node("A", OpKind::Compute, 1, |_| Ok(None));
    let n2 = g.add_node("B", OpKind::Compute, 1, |_| Ok(None));
    let n3 = g.add_node("C", OpKind::Compute, 1, |_| Ok(None));

    assert!(g.add_dependency(n2, n1).is_ok()); // A -> B
    assert!(g.add_dependency(n3, n2).is_ok()); // B -> C
    // Fechando o ciclo C -> A
    assert!(g.add_dependency(n1, n3).is_err(), "Ciclo A -> B -> C -> A deve ser rejeitado");
}

#[test]
fn test_self_dependency_rejected() {
    let mut g = DataflowGraph::new();
    let n = g.add_node("SelfDep", OpKind::Compute, 1, |_| Ok(None));
    assert!(g.add_dependency(n, n).is_err(), "Auto-dependencia deve ser rejeitada");
}

#[test]
fn test_diamond_fork_join() {
    let mut g = DataflowGraph::new();
    let prod = g.add_node("Prod", OpKind::Compute, 3, |_| {
        Ok(Some(DataBuffer::new_owned(vec![10, 20, 30, 40])))
    });

    let b1 = g.add_node("B1", OpKind::Compute, 2, |inp| {
        let sum: u8 = inp.unwrap().as_slice().iter().sum();
        Ok(Some(DataBuffer::new_owned(vec![sum])))
    });

    let b2 = g.add_node("B2", OpKind::Compute, 2, |inp| {
        let xor = inp.unwrap().as_slice().iter().fold(0u8, |acc, &x| acc ^ x);
        Ok(Some(DataBuffer::new_owned(vec![xor])))
    });

    let join = g.add_node("BarrierJoin", OpKind::Barrier, 4, |_| {
        Ok(Some(DataBuffer::new_owned(vec![0xFF])))
    });

    assert!(g.add_dependency(b1, prod).is_ok());
    assert!(g.add_dependency(b2, prod).is_ok());
    assert!(g.add_dependency(join, b1).is_ok());
    assert!(g.add_dependency(join, b2).is_ok());

    let exec = AsyncDataflowExecutor::new(PriorityLocalityPolicy, true);
    let rep = exec.execute(&mut g).expect("Execucao do diamante falhou");

    assert_eq!(rep.operations_completed, 4);
    assert_eq!(g.nodes[join].state, OpState::Completed);
}

#[test]
fn test_zero_copy_equivalence() {
    let raw_payload = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

    // Execução 1: Com cópia física (Owned)
    let mut g_copy = DataflowGraph::new();
    let p_copy = raw_payload.clone();
    let n1 = g_copy.add_node("ProdCopy", OpKind::MemoryTransfer, 1, move |_| {
        Ok(Some(DataBuffer::new_owned(p_copy.clone())))
    });
    let n2 = g_copy.add_node("ConsCopy", OpKind::Compute, 1, |inp| {
        let b = inp.unwrap();
        let doubled: Vec<u8> = b.as_slice().iter().map(|&x| x * 2).collect();
        Ok(Some(DataBuffer::new_owned(doubled)))
    });
    let _ = g_copy.add_dependency(n2, n1);

    let exec_copy = AsyncDataflowExecutor::new(FifoReferencePolicy, false);
    let rep_copy = exec_copy.execute(&mut g_copy).unwrap();
    assert_eq!(rep_copy.copy_operations, 1);
    assert_eq!(rep_copy.zero_copy_operations, 0);

    // Execução 2: Zero-Copy (Shared Arc)
    let mut g_zc = DataflowGraph::new();
    let p_zc = raw_payload.clone();
    let z1 = g_zc.add_node("ProdZC", OpKind::MemoryTransfer, 1, move |_| {
        Ok(Some(DataBuffer::new_owned(p_zc.clone())))
    });
    let z2 = g_zc.add_node("ConsZC", OpKind::Compute, 1, |inp| {
        let b = inp.unwrap();
        let doubled: Vec<u8> = b.as_slice().iter().map(|&x| x * 2).collect();
        Ok(Some(DataBuffer::new_owned(doubled)))
    });
    let _ = g_zc.add_dependency(z2, z1);

    let exec_zc = AsyncDataflowExecutor::new(FifoReferencePolicy, true);
    let rep_zc = exec_zc.execute(&mut g_zc).unwrap();
    assert_eq!(rep_zc.zero_copy_operations, 1);
    assert_eq!(rep_zc.copy_operations, 0);

    // Assegura equivalência estrita dos resultados
    assert_eq!(
        g_copy.nodes[n2].output_buffer.as_ref().unwrap().as_slice(),
        g_zc.nodes[z2].output_buffer.as_ref().unwrap().as_slice()
    );
}

#[test]
fn test_cascading_cancellation_on_failure() {
    let mut g = DataflowGraph::new();

    let n_fail = g.add_node("NodeFails", OpKind::Compute, 1, |_| {
        Err("Simulacao de falha irrecuperavel".to_string())
    });

    let n_dep1 = g.add_node("Dependent1", OpKind::Compute, 1, |_| Ok(None));
    let n_dep2 = g.add_node("Dependent2", OpKind::Compute, 1, |_| Ok(None));

    let _ = g.add_dependency(n_dep1, n_fail);
    let _ = g.add_dependency(n_dep2, n_dep1);

    let exec = AsyncDataflowExecutor::new(FifoReferencePolicy, true);
    let res = exec.execute(&mut g);

    assert!(res.is_err(), "Execucao deve retornar Err");
    assert_eq!(g.nodes[n_fail].state, OpState::Failed);
    assert_eq!(g.nodes[n_dep1].state, OpState::Cancelled);
}

#[test]
fn test_deterministic_reproducibility() {
    for _ in 0..10 {
        let mut g = DataflowGraph::new();
        let counter = Arc::new(AtomicUsize::new(0));

        for i in 0..8 {
            let c = Arc::clone(&counter);
            g.add_node(&format!("Task_{}", i), OpKind::Compute, 1, move |_| {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(None)
            });
        }

        let exec = AsyncDataflowExecutor::new(PriorityLocalityPolicy, true);
        let rep = exec.execute(&mut g).unwrap();
        assert_eq!(rep.operations_completed, 8);
        assert_eq!(counter.load(Ordering::SeqCst), 8);
    }
}

// Pequeno helper de mutex padrão sem dependências externas
mod parking_lot_sim {
    use std::sync::Mutex as StdMutex;
    pub struct Mutex<T>(StdMutex<T>);
    impl<T> Mutex<T> {
        pub fn new(val: T) -> Self {
            Self(StdMutex::new(val))
        }
        pub fn lock(&self) -> std::sync::MutexGuard<'_, T> {
            self.0.lock().unwrap()
        }
    }
}
