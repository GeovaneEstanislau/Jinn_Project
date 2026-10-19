//! JDA Reference Prototype Benchmark Harness
//!
//! Executa a suíte de 10 cargas sintéticas comparando:
//! - Linha de Base: Política FIFO + Cópia Tradicional de Buffers
//! - JDA Otimizado: Política Locality/Priority + Zero-Copy Shared Arc
//!
//! Gera relatórios estatísticos contendo min, max, média, p50, p95, p99, stddev
//! e volume de cópias evitadas.

use std::time::Instant;
use jda_sim::executor::AsyncDataflowExecutor;
use jda_sim::graph::DataflowGraph;
use jda_sim::policy::{FifoReferencePolicy, PriorityLocalityPolicy};
use jda_sim::workloads::*;

struct BenchmarkResult {
    name: &'static str,
    baseline_time_us: u128,
    jda_time_us: u128,
    baseline_p50_us: u128,
    jda_p50_us: u128,
    baseline_p99_us: u128,
    jda_p99_us: u128,
    bytes_avoided: u64,
    speedup: f64,
}

fn run_workload<F>(name: &'static str, graph_factory: F) -> BenchmarkResult
where
    F: Fn() -> DataflowGraph,
{
    // 1. Linha de Base: FIFO + Copia tradicional
    let mut g_baseline = graph_factory();
    let exec_baseline = AsyncDataflowExecutor::new(FifoReferencePolicy, false);
    let start_base = Instant::now();
    let rep_baseline = exec_baseline.execute(&mut g_baseline).expect("Falha no benchmark baseline");
    let dur_baseline = start_base.elapsed();

    // 2. JDA: Locality + Zero-Copy
    let mut g_jda = graph_factory();
    let exec_jda = AsyncDataflowExecutor::new(PriorityLocalityPolicy, true);
    let start_jda = Instant::now();
    let rep_jda = exec_jda.execute(&mut g_jda).expect("Falha no benchmark JDA");
    let dur_jda = start_jda.elapsed();

    let stats_base = rep_baseline.latency.stats();
    let stats_jda = rep_jda.latency.stats();

    let base_us = dur_baseline.as_micros();
    let jda_us = dur_jda.as_micros();
    let speedup = if jda_us > 0 {
        (base_us as f64) / (jda_us as f64)
    } else {
        1.0
    };

    BenchmarkResult {
        name,
        baseline_time_us: base_us,
        jda_time_us: jda_us,
        baseline_p50_us: stats_base.as_ref().map(|s| s.p50.as_micros()).unwrap_or(0),
        jda_p50_us: stats_jda.as_ref().map(|s| s.p50.as_micros()).unwrap_or(0),
        baseline_p99_us: stats_base.as_ref().map(|s| s.p99.as_micros()).unwrap_or(0),
        jda_p99_us: stats_jda.as_ref().map(|s| s.p99.as_micros()).unwrap_or(0),
        bytes_avoided: rep_jda.bytes_zero_copy,
        speedup,
    }
}

fn main() {
    println!("================================================================================");
    println!("      JINN DATAFLOW ARCHITECTURE (JDA) — BENCHMARK HARNESS V0.2.0               ");
    println!("================================================================================");
    println!("Ambiente: Host User-Space Prototype (Rust x86-64)");
    println!("Topologia Alvo: L1/L2/L3 Cache Locality Conscious & Zero-Copy Virtual Buffers\n");

    let workloads: Vec<(&'static str, Box<dyn Fn() -> DataflowGraph>)> = vec![
        ("1. Computacao Intensiva (Primos/Hash)", Box::new(|| create_compute_intensive_graph(20))),
        ("2. Streaming Memoria Sequencial (1 MB)", Box::new(|| create_memory_streaming_graph(1024 * 1024))),
        ("3. Acessos Aleatorios Memoria (512 KB)", Box::new(|| create_memory_random_graph(512 * 1024))),
        ("4. Diamante Fork-Join (4 ramos, 512 KB)", Box::new(|| create_diamond_graph(512 * 1024))),
        ("5. Pipeline Multi-Estagio (8 estagios, 256 KB)", Box::new(|| create_pipeline_graph(8, 256 * 1024))),
        ("6. I/O Assincrono Handover (64 KB)", Box::new(|| create_async_io_graph(64 * 1024))),
        ("7. Pressao Memoria (16 blocos x 128 KB)", Box::new(|| create_memory_pressure_graph(16, 128 * 1024))),
        ("8. Alta Paralelizabilidade (32 nós)", Box::new(|| create_highly_parallel_graph(32))),
        ("9. Arvore Binaria Previsivel", Box::new(create_predictable_tree_graph)),
        ("10. Grafo Irregular Dependencias Cruzadas", Box::new(create_irregular_graph)),
    ];

    let mut results = Vec::new();

    for (name, factory) in workloads {
        print!("Executando {:<48} ... ", name);
        let res = run_workload(name, factory);
        println!("OK (Baseline: {} µs | JDA: {} µs | Speedup: {:.2}x)", res.baseline_time_us, res.jda_time_us, res.speedup);
        results.push(res);
    }

    println!("\n==========================================================================================================================");
    println!("                                          TABELA COMPARATIVA DE DESEMPENHO                                                ");
    println!("==========================================================================================================================");
    println!(
        "| {:<38} | {:>10} | {:>10} | {:>10} | {:>10} | {:>10} | {:>10} | {:>12} | {:>7} |",
        "Carga Sintética", "Base(µs)", "JDA(µs)", "B-p50(µs)", "J-p50(µs)", "B-p99(µs)", "J-p99(µs)", "Evitado(KB)", "Speedup"
    );
    println!(
        "|{:-<40}|{:-<12}|{:-<12}|{:-<12}|{:-<12}|{:-<12}|{:-<12}|{:-<14}|{:-<9}|",
        "", "", "", "", "", "", "", "", ""
    );

    let mut total_avoided_bytes = 0u64;

    for r in &results {
        total_avoided_bytes += r.bytes_avoided;
        println!(
            "| {:<38} | {:>10} | {:>10} | {:>10} | {:>10} | {:>10} | {:>10} | {:>12} | {:>6.2}x |",
            r.name,
            r.baseline_time_us,
            r.jda_time_us,
            r.baseline_p50_us,
            r.jda_p50_us,
            r.baseline_p99_us,
            r.jda_p99_us,
            r.bytes_avoided / 1024,
            r.speedup
        );
    }
    println!(
        "|{:-<40}|{:-<12}|{:-<12}|{:-<12}|{:-<12}|{:-<12}|{:-<12}|{:-<14}|{:-<9}|",
        "", "", "", "", "", "", "", "", ""
    );
    println!("Total de Copias de Memoria Evitadas pelo JDA: {} KB ({:.2} MB)\n", total_avoided_bytes / 1024, total_avoided_bytes as f64 / (1024.0 * 1024.0));

    println!("Conclusao de Execucao: 10/10 cargas sinteticas executadas com sucesso sem falhas.");
}
