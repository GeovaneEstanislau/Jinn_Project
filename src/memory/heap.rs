/// Jinn OS — Kernel Heap Allocator (Bump + Free List)
///
/// A two-phase allocator designed for zero-overhead kernel use:
///
/// Phase 1 — Bump Allocator
///   Fast sequential allocation from a fixed heap region.
///   O(1) alloc, no free. Used during early boot.
///
/// Phase 2 — Free List (singly linked, sorted by address)
///   When blocks are freed, they're inserted into a sorted linked list.
///   Adjacent free blocks are coalesced on free.
///   O(n) alloc/free, but n stays small in kernel use.
///
/// The heap region is mapped at HEAP_START and grows to at most HEAP_SIZE.
/// Physical frames are lazily committed from the frame allocator.

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

// ── Constants ─────────────────────────────────────────────────────────────────
pub const HEAP_START: usize = 0xFFFF_C000_0000_0000;
pub const HEAP_SIZE:  usize = 16 * 1024 * 1024; // 16 MiB kernel heap
const HEAP_END:       usize = HEAP_START + HEAP_SIZE;
const MIN_ALIGN:      usize = 16;

// ── Free List Node ────────────────────────────────────────────────────────────
#[repr(C)]
struct FreeNode {
    size: usize,
    next: *mut FreeNode,
}

// ── Heap Allocator ────────────────────────────────────────────────────────────
pub struct KernelHeap {
    bump_ptr: UnsafeCell<usize>,
    free_list: UnsafeCell<*mut FreeNode>,
    lock: AtomicBool,
}

unsafe impl Sync for KernelHeap {}

impl KernelHeap {
    pub const fn new() -> Self {
        Self {
            bump_ptr: UnsafeCell::new(HEAP_START),
            free_list: UnsafeCell::new(core::ptr::null_mut()),
            lock: AtomicBool::new(false),
        }
    }

    fn acquire(&self) {
        while self.lock.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
    }

    fn release(&self) {
        self.lock.store(false, Ordering::Release);
    }

    /// Initialize the heap: map the first HEAP_SIZE bytes via paging.
    pub fn init(&self) {
        use crate::memory;
        use crate::memory::paging;

        let cr3 = paging::current_cr3();
        let pml4 = paging::phys_to_virt(cr3) as *mut paging::PageTable;

        let pages = HEAP_SIZE / 4096;
        for i in 0..pages {
            let virt = (HEAP_START + i * 4096) as u64;
            if let Some(phys) = memory::alloc_frame() {
                unsafe { paging::map_page(pml4, virt, phys, paging::KERNEL_RW); }
            }
        }
    }

    unsafe fn alloc_bump(&self, layout: Layout) -> *mut u8 {
        let bump = &mut *self.bump_ptr.get();
        let align = layout.align().max(MIN_ALIGN);
        let start = (*bump + align - 1) & !(align - 1);
        let end = start.checked_add(layout.size()).unwrap_or(0);
        if end > HEAP_END {
            return core::ptr::null_mut();
        }
        *bump = end;
        start as *mut u8
    }

    unsafe fn try_alloc_free_list(&self, layout: Layout) -> *mut u8 {
        let align = layout.align().max(MIN_ALIGN);
        let size  = (layout.size() + MIN_ALIGN - 1) & !(MIN_ALIGN - 1);
        let min_size = size.max(core::mem::size_of::<FreeNode>());

        let head = &mut *self.free_list.get();
        let mut prev: *mut *mut FreeNode = head as *mut _;
        let mut cur = *head;

        while !cur.is_null() {
            let node_size = (*cur).size;
            let addr = cur as usize;
            let aligned_start = (addr + align - 1) & !(align - 1);
            let wasted = aligned_start - addr;

            if node_size >= wasted + size {
                // Carve allocation out of this free node
                let remaining = node_size - wasted - size;
                *prev = (*cur).next;

                // If there's leftover space, reinsert it as a free node
                if remaining >= core::mem::size_of::<FreeNode>() {
                    let leftover = (aligned_start + size) as *mut FreeNode;
                    (*leftover).size = remaining;
                    (*leftover).next = *prev;
                    *prev = leftover;
                }

                return aligned_start as *mut u8;
            }

            prev = &mut (*cur).next as *mut _;
            cur  = (*cur).next;
        }

        core::ptr::null_mut()
    }
}

unsafe impl GlobalAlloc for KernelHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.acquire();
        let ptr = {
            let from_free = self.try_alloc_free_list(layout);
            if !from_free.is_null() {
                from_free
            } else {
                self.alloc_bump(layout)
            }
        };
        self.release();
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.acquire();
        let size = (layout.size() + MIN_ALIGN - 1) & !(MIN_ALIGN - 1);
        let node = ptr as *mut FreeNode;
        (*node).size = size.max(core::mem::size_of::<FreeNode>());

        // Insert into free list (sorted by address, coalesce adjacent blocks)
        let head = &mut *self.free_list.get();
        let mut prev: *mut *mut FreeNode = head as *mut _;
        let mut cur  = *head;

        while !cur.is_null() && (cur as usize) < (ptr as usize) {
            prev = &mut (*cur).next as *mut _;
            cur  = (*cur).next;
        }

        (*node).next = cur;
        *prev = node;

        // Coalesce with next block if adjacent
        if !cur.is_null() {
            let node_end = ptr as usize + (*node).size;
            if node_end == cur as usize {
                (*node).size += (*cur).size;
                (*node).next  = (*cur).next;
            }
        }

        self.release();
    }
}

// ── Global Allocator ──────────────────────────────────────────────────────────
#[global_allocator]
pub static KERNEL_HEAP: KernelHeap = KernelHeap::new();

/// Call once during boot, after paging is active and frame allocator is ready.
pub fn init() {
    KERNEL_HEAP.init();
}

/// Return total heap size.
pub fn total() -> usize { HEAP_SIZE }

/// Approximate used bytes (bump pointer offset).
pub fn used() -> usize {
    unsafe { *KERNEL_HEAP.bump_ptr.get() - HEAP_START }
}
