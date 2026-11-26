# JHCS-RS Host Compute Breakeven Benchmark Report

> **Environment:** OS `windows` | Arch `x86_64` | AVX2 `true` | FMA `true` | Buffers `#[repr(align(64))]`  
> **GPU Adapter (`wgpu`):** `JHCS WGPU [NVIDIA GeForce GTX 1650]`  

## 1. Breakeven Summary (Scalar CPU vs SIMD AVX2/FMA vs GPU `wgpu`)

- **Element-wise `vector_add` SIMD Breakeven (incl. Broker overhead):** Scalar CPU wins across tested sizes (Broker dispatch exceeds SIMD savings on memory-bound stream)
- **Matrix Multiplication `matmul` SIMD Breakeven (incl. Broker overhead):** `16x16` (`256` elements, `8192` FLOPs)
- **Matrix Multiplication `matmul` GPU (`wgpu`) Breakeven vs CPU/SIMD:** `512x512` (`262144` elements) — amortized staging + PCIe upload/download overhead.

## 2. Workload A: 2D Grid / Vector Stream (`OP_VECTOR_ADD`, `8^2` .. `1024^2`)

| Grid | Elements | Broker Overhead (ns) | Scalar Direct (ns) | Scalar (M elem/s) | SIMD Kernel (ns) | SIMD End-to-End (ns) | E2E SIMD (M elem/s) | Winner |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | :--- |
| `8x8` | 64 | 773 | 6 | 10666.7 | 14 | 787 | 81.3 | **Scalar CPU** |
| `64x64` | 4096 | 782 | 474 | 8641.4 | 460 | 1242 | 3297.9 | **Scalar CPU** |
| `256x256` | 65536 | 775 | 9486 | 6908.7 | 9063 | 9838 | 6661.5 | **Scalar CPU** |
| `512x512` | 262144 | 862 | 41448 | 6324.6 | 46977 | 47839 | 5479.7 | **Scalar CPU** |
| `1024x1024` | 1048576 | 1495 | 1067080 | 982.7 | 1536355 | 1537850 | 681.8 | **Scalar CPU** |

## 3. Workload B: Dense Matrix Multiply (`OP_MATMUL`, `8^2` .. `1024^2`)

| Size | Elements | FLOPs | Broker (ns) | Scalar (ns) | SIMD Kernel (ns) | SIMD E2E (ns) | SIMD GFLOP/s | wgpu Upload/Dispatch/Down (ns) | wgpu E2E (ns) | wgpu GFLOP/s | Winner |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | :--- | ---: | ---: | :--- |
| `8x8` | 64 | 1024 | 914 | 639 | 221 | 1135 | 0.90 | 1231000/232820/4810980 | 6712574 | 0.00 | **Scalar CPU** |
| `16x16` | 256 | 8192 | 998 | 5671 | 1793 | 2791 | 2.94 | 1385100/327460/225340 | 2371158 | 0.00 | **SIMD (AVX2/FMA)** |
| `32x32` | 1024 | 65536 | 969 | 13157 | 6348 | 7317 | 8.96 | 935380/200500/230740 | 1699049 | 0.04 | **SIMD (AVX2/FMA)** |
| `64x64` | 4096 | 524288 | 1137 | 67053 | 38240 | 39377 | 13.31 | 1630340/593680/245960 | 2982857 | 0.18 | **SIMD (AVX2/FMA)** |
| `128x128` | 16384 | 4194304 | 1250 | 404995 | 282970 | 284220 | 14.76 | 700780/111620/385500 | 1437830 | 2.92 | **SIMD (AVX2/FMA)** |
| `256x256` | 65536 | 33554432 | 2383 | 2717766 | 2272800 | 2275183 | 14.75 | 1066280/191080/720400 | 2286723 | 14.67 | **SIMD (AVX2/FMA)** |
| `512x512` | 262144 | 268435456 | 3550 | 19741200 | 15570500 | 15574050 | 17.24 | 1065750/130600/2989450 | 4484550 | 59.86 | **GPU (wgpu)** |
| `1024x1024` | 1048576 | 2147483648 | 2850 | 233903500 | 232898500 | 232901350 | 9.22 | 2568800/231100/16729500 | 20345850 | 105.55 | **GPU (wgpu)** |

## 4. Reproducibility

```bash
cargo run --release -p benchmark-breakeven
```
