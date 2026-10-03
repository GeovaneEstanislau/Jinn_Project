#![allow(dead_code)]

use core::alloc::Layout;
use core::cell::UnsafeCell;
use core::ptr::null_mut;
use core::sync::atomic::{AtomicBool, Ordering};

pub const PAGE_SIZE: usize = 4096;
const MAX_FRAMES: usize = 1024 * 1024; // 1,048,576 frames = 4 GiB of physical memory
const BITMAP_WORDS: usize = MAX_FRAMES / 64; // 16,384 u64 words = 128 KiB

// ── Simple Zero-Dependency SpinLock ──────────────────────────────────────────
pub struct SpinLock<T> {
    lock: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for SpinLock<T> {}
unsafe impl<T: Send> Send for SpinLock<T> {}

impl<T> SpinLock<T> {
    pub const fn new(data: T) -> Self {
        SpinLock {
            lock: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        while self.lock.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        SpinLockGuard { lock: self }
    }
}

pub struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
}

impl<'a, T> core::ops::Deref for SpinLockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<'a, T> core::ops::DerefMut for SpinLockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<'a, T> Drop for SpinLockGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.lock.store(false, Ordering::Release);
    }
}

// ── Physical Page Frame Allocator (Buddy) ────────────────────────────────────
use crate::memory::buddy::BuddyAllocator;

static FRAME_ALLOCATOR: SpinLock<BuddyAllocator> = SpinLock::new(BuddyAllocator::new());


// ── Kernel Dynamic Heap Allocator (Free List) ────────────────────────────────
#[repr(C)]
struct FreeBlock {
    size: usize,
    next: Option<*mut FreeBlock>,
}

pub struct KernelHeap {
    head: Option<*mut FreeBlock>,
    allocated_bytes: usize,
    total_heap_bytes: usize,
}

unsafe impl Send for KernelHeap {}

impl KernelHeap {
    pub const fn new() -> Self {
        KernelHeap {
            head: None,
            allocated_bytes: 0,
            total_heap_bytes: 0,
        }
    }

    pub fn init(&mut self) {
        // Expand with initial 16 frames = 64 KiB
        self.expand(16);
    }

    fn expand(&mut self, frames_count: usize) -> bool {
        let hhdm = crate::limine::hhdm_offset();
        let mut fa = FRAME_ALLOCATOR.lock();

        for _ in 0..frames_count {
            if let Some(paddr) = fa.alloc(0) {
                let vaddr = (paddr + hhdm) as *mut u8;
                self.add_free_region(vaddr, PAGE_SIZE);
                self.total_heap_bytes += PAGE_SIZE;
            } else {
                return false;
            }
        }
        true
    }

    fn add_free_region(&mut self, addr: *mut u8, size: usize) {
        if size < core::mem::size_of::<FreeBlock>() {
            return;
        }

        let new_block = addr as *mut FreeBlock;
        unsafe {
            (*new_block).size = size;
            (*new_block).next = self.head;
        }
        self.head = Some(new_block);
    }

    pub fn allocate(&mut self, layout: Layout) -> *mut u8 {
        let size = layout.size().max(core::mem::size_of::<FreeBlock>());
        let align = layout.align().max(core::mem::align_of::<FreeBlock>());
        let needed_size = size + align;

        // Try allocating from current free list
        if let Some(ptr) = self.find_and_alloc(needed_size, align) {
            self.allocated_bytes += size;
            return ptr;
        }

        // Out of free heap: expand heap by allocating more frames
        let frames_needed = (needed_size + PAGE_SIZE - 1) / PAGE_SIZE + 4;
        if self.expand(frames_needed) {
            if let Some(ptr) = self.find_and_alloc(needed_size, align) {
                self.allocated_bytes += size;
                return ptr;
            }
        }

        null_mut()
    }

    fn find_and_alloc(&mut self, size: usize, align: usize) -> Option<*mut u8> {
        let mut prev: Option<*mut FreeBlock> = None;
        let mut current = self.head;

        while let Some(curr_ptr) = current {
            let block = unsafe { &mut *curr_ptr };
            let raw_addr = curr_ptr as usize;
            let aligned_addr = (raw_addr + align - 1) & !(align - 1);
            let overhead = aligned_addr - raw_addr;

            if block.size >= size + overhead {
                // Remove or split block
                let remaining = block.size - (size + overhead);
                if remaining >= core::mem::size_of::<FreeBlock>() + 16 {
                    let next_block = (aligned_addr + size) as *mut FreeBlock;
                    unsafe {
                        (*next_block).size = remaining;
                        (*next_block).next = block.next;
                    }
                    if let Some(prev_ptr) = prev {
                        unsafe { (*prev_ptr).next = Some(next_block) };
                    } else {
                        self.head = Some(next_block);
                    }
                } else {
                    if let Some(prev_ptr) = prev {
                        unsafe { (*prev_ptr).next = block.next };
                    } else {
                        self.head = block.next;
                    }
                }
                return Some(aligned_addr as *mut u8);
            }

            prev = current;
            current = block.next;
        }
        None
    }

    pub fn deallocate(&mut self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            return;
        }
        let size = layout.size().max(core::mem::size_of::<FreeBlock>());
        self.allocated_bytes = self.allocated_bytes.saturating_sub(size);
        self.add_free_region(ptr, size);
    }
}

// ── Public Memory Subsystem Interface ────────────────────────────────────────

pub fn init() {
    let mut alloc = FRAME_ALLOCATOR.lock();
    alloc.init();
}

pub fn alloc_frame() -> Option<u64> {
    FRAME_ALLOCATOR.lock().alloc(0)
}

pub fn free_frame(paddr: u64) {
    FRAME_ALLOCATOR.lock().free(paddr, 0)
}

pub fn total_frames() -> usize {
    FRAME_ALLOCATOR.lock().total_frames
}

pub fn free_frames() -> usize {
    FRAME_ALLOCATOR.lock().free_frames
}

pub fn used_frames() -> usize {
    FRAME_ALLOCATOR.lock().used_frames
}

pub fn used_bytes() -> usize {
    FRAME_ALLOCATOR.lock().used_frames * 4096
}

pub fn total_bytes() -> usize {
    FRAME_ALLOCATOR.lock().total_frames * 4096
}
