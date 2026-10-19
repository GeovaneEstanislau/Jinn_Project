//! JDA Synthetic Workload Generator
//!
//! Implementa as classes de teste especificadas na Metodologia de Benchmark:
//! 1. Computação Intensiva
//! 2. Streaming Sequencial de Memória
//! 3. Acessos Aleatórios à Memória
//! 4. Dependência em Diamante (Fork-Join)
//! 5. Pipeline Sequencial Multi-Estágio
//! 6. I/O Assíncrono com Handover de Buffer
//! 7. Pressão de Memória
//! 8. Alta Paralelizabilidade
//! 9. Árvore Previsível
//! 10. Grafo Irregular com Dependências Cruzadas

use crate::graph::{DataBuffer, DataflowGraph, OpKind};

/// 1. Computação Intensiva: Teste de primalidade e cripto-hashing leve
pub fn create_compute_intensive_graph(iterations: usize) -> DataflowGraph {
    let mut g = DataflowGraph::new();
    for i in 0..iterations {
        g.add_node(
            &format!("Compute_{}", i),
            OpKind::Compute,
            2,
            move |_input| {
                let mut count = 0u64;
                for candidate in 2..2000u64 {
                    let mut is_prime = true;
                    for d in 2..=(candidate / 2) {
                        if candidate % d == 0 {
                            is_prime = false;
                            break;
                        }
                    }
                    if is_prime {
                        count += 1;
                    }
                }
                let mut out = Vec::with_capacity(8);
                out.extend_from_slice(&count.to_le_bytes());
                Ok(Some(DataBuffer::new_owned(out)))
            },
        );
    }
    g
}

/// 2. Streaming Sequencial de Memória: Transformação de buffer de 256 KiB
pub fn create_memory_streaming_graph(buffer_size: usize) -> DataflowGraph {
    let mut g = DataflowGraph::new();
    let initial_data = vec![0x42u8; buffer_size];

    let prod = g.add_node("MemProducer", OpKind::MemoryTransfer, 3, move |_| {
        Ok(Some(DataBuffer::new_owned(initial_data.clone())))
    });

    let stage1 = g.add_node("StreamStage1", OpKind::Compute, 2, |input| {
        let buf = input.ok_or("Sem buffer de entrada")?;
        let mut modified = buf.as_slice().to_vec();
        for b in modified.iter_mut() {
            *b = b.wrapping_add(1);
        }
        Ok(Some(DataBuffer::new_owned(modified)))
    });

    let stage2 = g.add_node("StreamStage2", OpKind::Compute, 1, |input| {
        let buf = input.ok_or("Sem buffer de entrada")?;
        let mut sum = 0u64;
        for &b in buf.as_slice() {
            sum = sum.wrapping_add(b as u64);
        }
        Ok(Some(DataBuffer::new_owned(sum.to_le_bytes().to_vec())))
    });

    let _ = g.add_dependency(stage1, prod);
    let _ = g.add_dependency(stage2, stage1);
    g
}

/// 3. Acessos Aleatórios de Memória (Stress de Linhas de Cache)
pub fn create_memory_random_graph(size: usize) -> DataflowGraph {
    let mut g = DataflowGraph::new();
    let data = vec![0x3Cu8; size];

    let prod = g.add_node("RandomMemProducer", OpKind::MemoryTransfer, 2, move |_| {
        Ok(Some(DataBuffer::new_owned(data.clone())))
    });

    let worker = g.add_node("RandomWorker", OpKind::Compute, 2, |input| {
        let buf = input.ok_or("Sem buffer")?;
        let slice = buf.as_slice();
        let len = slice.len();
        let mut sum = 0u32;
        // Pseudo-random stride (LCG)
        let mut lcg: usize = 12345;
        for _ in 0..5000 {
            lcg = lcg.wrapping_mul(1103515245).wrapping_add(12345);
            let idx = lcg % len;
            sum = sum.wrapping_add(slice[idx] as u32);
        }
        Ok(Some(DataBuffer::new_owned(sum.to_le_bytes().to_vec())))
    });

    let _ = g.add_dependency(worker, prod);
    g
}

/// 4. Dependência em Diamante (Fork-Join): Produtor -> 4 Ramos -> Barreira
pub fn create_diamond_graph(buffer_size: usize) -> DataflowGraph {
    let mut g = DataflowGraph::new();
    let payload = vec![0xAAu8; buffer_size];

    let prod = g.add_node("DiamondRoot", OpKind::MemoryTransfer, 3, move |_| {
        Ok(Some(DataBuffer::new_owned(payload.clone())))
    });

    let b0 = g.add_node("Branch_0", OpKind::Compute, 2, |inp| {
        let b = inp.unwrap();
        let sum = b.as_slice().iter().fold(0u8, |acc, &x| acc.wrapping_add(x));
        Ok(Some(DataBuffer::new_owned(vec![sum])))
    });

    let b1 = g.add_node("Branch_1", OpKind::Compute, 2, |inp| {
        let b = inp.unwrap();
        let xor = b.as_slice().iter().fold(0u8, |acc, &x| acc ^ x);
        Ok(Some(DataBuffer::new_owned(vec![xor])))
    });

    let b2 = g.add_node("Branch_2", OpKind::Compute, 2, |inp| {
        let b = inp.unwrap();
        let cnt = b.as_slice().iter().filter(|&&x| x > 0x80).count() as u8;
        Ok(Some(DataBuffer::new_owned(vec![cnt])))
    });

    let b3 = g.add_node("Branch_3", OpKind::Compute, 2, |inp| {
        let b = inp.unwrap();
        let max = b.as_slice().iter().copied().max().unwrap_or(0);
        Ok(Some(DataBuffer::new_owned(vec![max])))
    });

    let join = g.add_node("JoinBarrier", OpKind::Barrier, 4, |_| {
        Ok(Some(DataBuffer::new_owned(vec![0xFF])))
    });

    let _ = g.add_dependency(b0, prod);
    let _ = g.add_dependency(b1, prod);
    let _ = g.add_dependency(b2, prod);
    let _ = g.add_dependency(b3, prod);

    let _ = g.add_dependency(join, b0);
    let _ = g.add_dependency(join, b1);
    let _ = g.add_dependency(join, b2);
    let _ = g.add_dependency(join, b3);

    g
}

/// 5. Pipeline Sequencial Multi-Estágio (8 estágios lineares)
pub fn create_pipeline_graph(stages: usize, payload_size: usize) -> DataflowGraph {
    let mut g = DataflowGraph::new();
    let mut prev = None;

    for i in 0..stages {
        let node_id = g.add_node(
            &format!("Stage_{}", i),
            OpKind::Compute,
            1,
            move |input| {
                let data = match input {
                    Some(b) => b.as_slice().to_vec(),
                    None => vec![0u8; payload_size],
                };
                let mut out = data;
                for b in out.iter_mut() {
                    *b = b.wrapping_add(1);
                }
                Ok(Some(DataBuffer::new_owned(out)))
            },
        );

        if let Some(p) = prev {
            let _ = g.add_dependency(node_id, p);
        }
        prev = Some(node_id);
    }
    g
}

/// 6. I/O Assíncrono com Handover de Buffer
pub fn create_async_io_graph(block_size: usize) -> DataflowGraph {
    let mut g = DataflowGraph::new();
    let disk_sector = vec![0x7Eu8; block_size];

    let io_read = g.add_node("IoReadSector", OpKind::IoAsync, 3, move |_| {
        // Simula I/O assíncrono entregando buffer lido do dispositivo
        Ok(Some(DataBuffer::new_owned(disk_sector.clone())))
    });

    let parser = g.add_node("FsParser", OpKind::Compute, 2, |input| {
        let buf = input.ok_or("Sem dados I/O")?;
        let check = buf.as_slice().iter().fold(0u16, |acc, &x| acc.wrapping_add(x as u16));
        Ok(Some(DataBuffer::new_owned(check.to_le_bytes().to_vec())))
    });

    let _ = g.add_dependency(parser, io_read);
    g
}

/// 7. Pressão de Memória: Alocações concorrentes e liberação
pub fn create_memory_pressure_graph(num_blocks: usize, block_size: usize) -> DataflowGraph {
    let mut g = DataflowGraph::new();
    for i in 0..num_blocks {
        g.add_node(
            &format!("MemBlock_{}", i),
            OpKind::MemoryTransfer,
            1,
            move |_| {
                let mut block = vec![i as u8; block_size];
                block[0] = 0xAA;
                Ok(Some(DataBuffer::new_owned(block)))
            },
        );
    }
    g
}

/// 8. Altamente Paralelizável: 32 nós independentes
pub fn create_highly_parallel_graph(count: usize) -> DataflowGraph {
    let mut g = DataflowGraph::new();
    for i in 0..count {
        g.add_node(&format!("ParallelTask_{}", i), OpKind::Compute, 1, move |_| {
            let mut acc = 0u32;
            for j in 0..1000 {
                acc = acc.wrapping_add((i * j) as u32);
            }
            Ok(Some(DataBuffer::new_owned(acc.to_le_bytes().to_vec())))
        });
    }
    g
}

/// 9. Árvore Binária Previsível
pub fn create_predictable_tree_graph() -> DataflowGraph {
    let mut g = DataflowGraph::new();
    let root = g.add_node("TreeRoot", OpKind::Compute, 3, |_| {
        Ok(Some(DataBuffer::new_owned(vec![100])))
    });

    let left = g.add_node("LeftBranch", OpKind::Compute, 2, |inp| {
        let v = inp.unwrap().as_slice()[0];
        Ok(Some(DataBuffer::new_owned(vec![v / 2])))
    });

    let right = g.add_node("RightBranch", OpKind::Compute, 2, |inp| {
        let v = inp.unwrap().as_slice()[0];
        Ok(Some(DataBuffer::new_owned(vec![v * 2])))
    });

    let _ = g.add_dependency(left, root);
    let _ = g.add_dependency(right, root);
    g
}

/// 10. Grafo Irregular com Dependências Cruzadas
pub fn create_irregular_graph() -> DataflowGraph {
    let mut g = DataflowGraph::new();
    let n0 = g.add_node("N0", OpKind::Compute, 1, |_| Ok(Some(DataBuffer::new_owned(vec![1]))));
    let n1 = g.add_node("N1", OpKind::Compute, 1, |_| Ok(Some(DataBuffer::new_owned(vec![2]))));
    let n2 = g.add_node("N2", OpKind::Compute, 2, |_| Ok(Some(DataBuffer::new_owned(vec![3]))));
    let n3 = g.add_node("N3", OpKind::Compute, 3, |_| Ok(Some(DataBuffer::new_owned(vec![4]))));

    let _ = g.add_dependency(n2, n0);
    let _ = g.add_dependency(n2, n1);
    let _ = g.add_dependency(n3, n2);
    let _ = g.add_dependency(n3, n0);
    g
}
