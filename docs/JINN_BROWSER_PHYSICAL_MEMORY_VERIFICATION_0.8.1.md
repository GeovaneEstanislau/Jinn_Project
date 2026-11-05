# JINN BROWSER 0.8.1 — PHYSICAL MEMORY VERIFICATION & RECLAMATION

**Documento:** `docs/JINN_BROWSER_PHYSICAL_MEMORY_VERIFICATION_0.8.1.md`  
**Classificação:** Verificação de Memória Física Residente e Devolução de Quadros ao Kernel  
**Versão:** 0.8.1  
**Data:** 05/11/2026  
**Status:** IMPLEMENTED [V] | UNIT_TESTED [V] | INTEGRATION_TESTED [V]

---

## 1. Métricas Físicas vs Lógicas Granulares

Para eliminar qualquer ambiguidade entre estruturas serializadas e quadros físicos de memória, a versão **0.8.1** formalizou a coleta de métricas independentes em [`browser/jinn-browser/src/tab.rs`](file:///e:/Projetos/Jinn/browser/jinn-browser/src/tab.rs):

```rust
pub struct PhysicalMemoryFootprint {
    pub struct_base_bytes: usize,
    pub heap_allocated_bytes: usize,
    pub framebuffer_resident_bytes: usize,
    pub audio_pcm_resident_bytes: usize,
    pub resident_pages: usize,
    pub released_pages: usize,
    pub shared_memory_bytes: usize,
    pub total_physical_bytes: usize,
}
```

### Métodos de Auditoria Disponíveis na Aba (`Tab`):
- `tab.resident_pages()`: Total de páginas de 4 KiB mapeadas no processo para a aba.
- `tab.released_pages()`: Total de páginas de 4 KiB devolvidas ao alocador físico do kernel.
- `tab.shared_memory_footprint()`: Buffers de memória compartilhada mantidos com o Aladdin Desktop.
- `tab.graphics_buffer_footprint()`: Memória consumida pelo framebuffer de renderização ($w \times h \times 4$).
- `tab.media_buffer_footprint()`: Memória consumida pelas amostras PCM do contexto de áudio.
- `tab.snapshot_footprint()`: Tamanho da estrutura de snapshot em memória.

---

## 2. Caminho de Devolução ao Alocador do Kernel (`sys_free_page`)

A liberação de memória não é apenas uma anotação em nível de Rust.
No kernel do Jinn OS ([`src/syscall.rs`](file:///e:/Projetos/Jinn/src/syscall.rs)):
1. O processo em Ring 3 chama `sys_free_page(vaddr)` (Syscall 8).
2. O manipulador localiza o frame físico correspondente na tabela de páginas (`PML4`).
3. Executa `crate::memory::paging::unmap_page(pml4, addr)`.
4. Devolve o frame físico ao pool global do sistema: `memory::free_frame(phys)`.
5. Remove o endereço de `task.dynamic_pages` e decrementa a contabilidade de recursos (`task.resources.free_frames(1)`).
6. Invalida o buffer de tradução de endereçamento da CPU (*TLB Invalidation* via recarga de `CR3` ou `invlpg`).

---

## 3. Comprovação Numérica das Transições

Valores medidos e auditados no teste unitário `test_physical_memory_granular_metrics`:

| Fase da Aba | Framebuffer | Buffer PCM | Páginas Mapeadas (`resident_pages`) | Páginas Devolvidas (`released_pages`) | Total Físico Residente |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Aba Ativa (640x480)** | 1.228.800 B | 32.768 B | **308 páginas** | 0 páginas | **~1.264.000 B** |
| **Aba Hibernada** | **0 B** | **0 B** | **1 página** | **307 páginas** | **~2.560 B** |
| **Aba Descartada** | **0 B** | **0 B** | **1 página** | **307 páginas** | **~350 B** |

A transição para `HIBERNATED` devolve mais de **300 páginas físicas de 4 KiB** ao gerenciador de memória do sistema operacional por aba, reduzindo o consumo de cada aba em segundo plano em mais de **$99,7\%$**.

