# Appendix: Memory Core Completion

Este arquivo contém as seções finais que precisam ser adicionadas ao memory-core.md

## 11. Comparação com Sistemas Modernos

### Linux Kernel Memory Management vs Jinn

| Aspecto | Linux | Jinn |
|---------|-------|------|
| **Allocator** | Buddy + Slab | Buddy + Slab + Predictive |
| **Complexity** | ~50k LOC | ~5k LOC (alvo) |
| **NUMA Support** | Sim, complexo | Sim, simples |
| **Capabilities** | Não | Sim (forte modelo) |
| **Swapping** | Sim, complexo | Sim, simples |
| **Transparency** | Kernel policy | Plugável |
| **Fault tolerance** | Moderate | High (isolamento) |

**Diferenças-chave:**
- Linux integra memory management profundamente com VFS, paging, etc
- Jinn isola memory core para verificabilidade
- Jinn adiciona Predictive Pool para uso proativo

### seL4 Memory Handling vs Jinn

| Aspecto | seL4 | Jinn |
|---------|------|------|
| **Formal Verification** | Sim (completo) | Em progresso |
| **Capability Model** | Sim (forte) | Sim (compatível) |
| **Allocators** | Slab apenas | Buddy + Slab |
| **NUMA** | Não | Sim |
| **Performance** | Ótima | Promising |

### Zircon (Fuchsia) vs Jinn

| Aspecto | Zircon | Jinn |
|---------|--------|------|
| **Language** | C++ | Rust |
| **VMO (Virtual Memory Object)** | Sim | Planned |
| **VMX (Virtual Memory Extended)** | Sim | Planned |
| **Predictive** | Não | Sim |

---

## 12. Pseudocódigo Avançado

### Algorithm: Buddy Allocator

```pseudo
function buddy_alloc(order):
    // order = log2(size)
    // Find free block
    for o = order to MAX_ORDER:
        if free_list[o] is not empty:
            block = free_list[o].pop()
            
            // Split down to requested order
            while o > order:
                o = o - 1
                buddy = split(block)
                free_list[o].push(buddy)
            
            mark_allocated(block)
            return block
    
    // No free block found
    try_compact()
    return buddy_alloc(order)  // Retry

function buddy_free(block, order):
    mark_free(block)
    
    // Try to coalesce with buddy
    while order < MAX_ORDER:
        buddy = get_buddy(block, order)
        
        if buddy.is_free and buddy.order == order:
            block = coalesce(block, buddy)
            order = order + 1
        else:
            break
    
    free_list[order].push(block)
```

### Algorithm: Slab Allocator

```pseudo
function slab_alloc(size):
    // Find appropriate slab cache
    cache = find_cache_for_size(size)
    
    // Try per-CPU hot cache
    cpu_cache = cache.per_cpu_caches[current_cpu()]
    if cpu_cache.has_free():
        return cpu_cache.pop()
    
    // Try partial slabs
    if cache.partial_slabs is not empty:
        slab = cache.partial_slabs.pop()
        object = slab.allocate()
        if slab.is_full():
            cache.full_slabs.push(slab)
        else:
            cache.partial_slabs.push(slab)
        return object
    
    // Allocate new slab from buddy
    new_slab = buddy_alloc(SLAB_ORDER)
    cache.partial_slabs.push(new_slab)
    return slab_alloc(size)  // Retry

function slab_free(ptr, size):
    cache = find_cache_for_size(size)
    slab = find_slab_for_pointer(ptr)
    
    // Mark as free in bitmap
    slab.mark_free(ptr)
    
    // Update slab lists
    if slab.was_full:
        cache.full_slabs.remove(slab)
        cache.partial_slabs.push(slab)
    
    if slab.is_empty:
        cache.partial_slabs.remove(slab)
        cache.empty_slabs.push(slab)
        
        // After timeout, can free to buddy
```

### Algorithm: Copy-on-Write

```pseudo
function fork_address_space(parent):
    child = create_address_space()
    
    // Copy parent's page table structure
    for each page in parent.pages:
        mark_cow(page)
        child.add_mapping(page.vaddr, page.paddr, READ_ONLY | COW)
        parent.update_pte(page.vaddr, READ_ONLY | COW)
    
    return child

function handle_cow_fault(vaddr, task):
    page_table = task.address_space.page_tables
    pte = page_table.lookup(vaddr)
    
    if not pte.is_cow:
        return  // Not a COW page
    
    // Allocate new page
    old_paddr = pte.physical_address
    new_paddr = buddy_alloc(PAGE_ORDER)
    
    // Copy content
    memcpy(new_paddr, old_paddr, PAGE_SIZE)
    
    // Update page table
    pte.physical_address = new_paddr
    pte.writable = true
    pte.cow = false
    
    // Invalidate TLB
    invalidate_tlb(vaddr)
```

---

## 13. Diagramas ASCII Avançados

### Buddy Allocator Structure

```
                    Free List States

MAX_ORDER (1GB)    ┌──────────────┐
                   │  [1 block]   │
                   └──────────────┘
                        │
                    [Split/Coalesce]
                        │
                   ┌─────┴─────┐
                   ▼           ▼
ORDER-1 (512MB)  [Block]    [Block]
                   
                   ...
                   
ORDER-1 (8KB)     ┌──┐┌──┐┌──┐┌──┐
                  │B1││B2││B3││B4│
                  └──┘└──┘└──┘└──┘
                   │
              [Allocate/Free]
                   │
ORDER-0 (4KB)  [single pages]

Buddy Property:
  Block at address A has buddy at A ⊕ (1 << order)
```

### Virtual Memory Hierarchy

```
┌──────────────────────────────────────────┐
│   Virtual Address (48 bits x86_64)        │
│   ┌──┬──────┬──────┬──────┬────────────┐ │
│   │ │ PML4 │ PDPT │  PDT │   PT │Offset │  │
│   │ │(9b)  │(9b)  │ (9b) │ (9b) │ (12b)│  │
│   └──┴──────┴──────┴──────┴────────┴────┘ │
└──────────────────────────────────────────┘
           │        │        │        │
    ┌──────▼──┐ ┌───▼──┐ ┌──▼───┐ ┌─▼──┐
    │ PML4T   │ │PDPT  │ │ PDT  │ │ PT │
    │(512 ent)│ │(512) │ │(512) │ │(512)│
    └─────────┘ └──────┘ └──────┘ └────┘
            │              │
    ┌───────▼────┐   ┌────▼──────┐
    │ Physical   │   │ Physical  │
    │ Page (4KB) │   │ Page (4KB)│
    └────────────┘   └───────────┘
```

### NUMA Node Distribution

```
NUMA Node 0              NUMA Node 1
CPU0 CPU1 CPU2     CPU4 CPU5 CPU6
  │    │    │         │    │    │
  └────┬────┘         └────┬────┘
       │                   │
   ┌───▼──────┐       ┌────▼───┐
   │  Buddy   │       │ Buddy  │
   │  8GB     │       │  8GB   │
   └───┬──────┘       └────┬───┘
       │                   │
   ┌───▼────────────────┬──▼───┐
   │                    │      │
   ▼                    ▼      ▼
DRAM (Node0)        DRAM (Node1)

Hints:
  • Allocate local wenn possible
  • Cross-node access penalty: ~10x latency
  • Migration on access pattern change
```

### Memory Isolation Model

```
Domain A          Domain B         Domain C
┌─────────┐      ┌─────────┐      ┌─────────┐
│ Task 1  │      │ Task 2  │      │ Task 3  │
│ Pages   │      │ Pages   │      │ Pages   │
│ 0x1000  │      │ 0x2000  │      │ 0x3000  │
└────┬────┘      └────┬────┘      └────┬────┘
     │                │                │
     │ Capability     │ Capability     │ Capability
     │ {R, W}         │ {R}            │ {R, W, X}
     │                │                │
     └────┬───────────┴────────────────┬┘
          │                            │
    ┌─────▼────────────────────────────▼──┐
    │     Shared Page (Mediation)          │
    │     Capability-based access control  │
    └──────────────────────────────────────┘
```

---

## 14. Considerações para Implementação em Rust

### Safe Memory Pool Abstraction

```rust
// Safe wrapper for unsafe memory operations
pub struct MemoryPool {
    allocator: Box<dyn MemoryAllocator>,
    metrics: Arc<Mutex<PoolMetrics>>,
}

impl MemoryPool {
    pub fn allocate(&mut self, size: usize) -> Result<Box<[u8]>, AllocError> {
        let ptr = self.allocator.allocate(size, 0)?;
        
        // SAFETY: allocator returned valid, non-null pointer for `size` bytes
        let slice = unsafe {
            core::slice::from_raw_parts_mut(ptr, size)
        };
        
        // Convert to Box (takes ownership)
        Ok(Box::from_raw(slice))
    }
}

// Users can now write safe code without dealing with raw pointers
let pool = create_memory_pool();
let buffer: Box<[u8]> = pool.allocate(1024)?;  // Type-safe
```

### Atomic Page Operations

```rust
pub struct AtomicPage {
    state: AtomicU8,
    ref_count: AtomicU32,
    physical_addr: PhysicalAddress,
}

impl AtomicPage {
    pub fn try_allocate(&self) -> Result<(), AllocError> {
        self.state.compare_exchange(
            PageState::Free as u8,
            PageState::Used as u8,
            Ordering::Acquire,
            Ordering::Relaxed,
        )?;
        Ok(())
    }
    
    pub fn inc_ref(&self) {
        self.ref_count.fetch_add(1, Ordering::Acquire);
    }
    
    pub fn dec_ref(&self) -> u32 {
        self.ref_count.fetch_sub(1, Ordering::Release)
    }
}
```

### Unsafe Justification

```rust
/// Restore a page from disk (NECESSARY unsafe)
///
/// SAFETY: This function reads from disk and writes to a physical address.
/// Preconditions:
/// - `page` must point to a valid, allocated page
/// - `disk_block` must contain valid page data
/// - No other task can be writing to `page` concurrently
/// - Interrupts must be disabled
///
/// MITIGATION:
/// - Caller must hold exclusive capability for page
/// - Page state is checked atomically before/after
/// - Disk I/O completion verified before returning
unsafe fn restore_page_from_disk(
    page: &Page,
    disk_block: u64,
) -> Result<(), IOError> {
    // DMA read from disk
    disk_driver.read_block(
        disk_block,
        page.physical_address,
        PAGE_SIZE,
    ).await?;
    
    page.state.store(PageState::Used as u8, Ordering::Release);
    Ok(())
}
```

### Testing Memory Allocators

```rust
#[cfg(test)]
mod memory_tests {
    #[test]
    fn test_buddy_allocator_correctness() {
        let mut buddy = BuddyAllocator::new(256 * PAGE_SIZE);  // 1MB
        
        // Allocate and free multiple sizes
        let p1 = buddy.alloc(4096).unwrap();   // 1 page
        let p2 = buddy.alloc(8192).unwrap();   // 2 pages
        let p3 = buddy.alloc(1024).unwrap();   // Request smaller
        
        buddy.free(p2, 8192);
        buddy.free(p1, 4096);
        
        // Should be able to allocate 3 pages (coalesced)
        let p4 = buddy.alloc(12288).unwrap();
        
        assert_eq!(buddy.fragmentation(), 0.0);
    }
    
    #[test]
    fn test_cow_semantics() {
        let parent_space = create_address_space();
        parent_space.map_page(0x1000, PADDR(0x2000), RW);
        
        let child_space = fork_address_space(&parent_space);
        
        // Child page is CoW
        assert!(child_space.is_cow(0x1000));
        
        // Trigger fault
        child_space.write_page(0x1000, &[0xFF; 4096]).unwrap();
        
        // No longer CoW, has new physical page
        assert!(!child_space.is_cow(0x1000));
        assert_ne!(
            child_space.phys_addr(0x1000),
            parent_space.phys_addr(0x1000)
        );
    }
}
```

---

## 15. Roadmap de Implementação

### Fase 0 (v0.0.1 — Atual)
- [x] Bitmap allocator (bootstrap)
- [x] Basic page structures
- [x] Documentação inicial

### Fase 1 (v0.0.2 — Próximas 2 semanas)
- [ ] Buddy allocator completo
- [ ] Slab allocator
- [ ] Basic page mapping
- [ ] Simple page faults

### Fase 2 (v0.1 — 1-2 meses)
- [ ] Copy-on-Write (CoW)
- [ ] Capability system
- [ ] Shared pages
- [ ] NUMA awareness básica

### Fase 3 (v0.2 — 2-3 meses)
- [ ] Predictive Engine integration
- [ ] Pool pre-allocation
- [ ] Swapping support

### Fase 4 (v1.0 — 6+ months)
- [ ] Formal verification
- [ ] Complete NUMA support
- [ ] Performance optimization

---

**Document Status:** 📋 Complete  
**Version:** 1.0  
**Last Updated:** 2026-08-17
