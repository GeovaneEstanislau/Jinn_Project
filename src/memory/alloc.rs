#![allow(dead_code)]

use core::alloc::{GlobalAlloc, Layout};
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

// ── Physical Page Frame Allocator (Bitmap) ───────────────────────────────────
pub struct PageFrameAllocator {
    bitmap: [u64; BITMAP_WORDS],
    total_frames: usize,
    free_frames: usize,
    used_frames: usize,
    initialized: bool,
}

impl PageFrameAllocator {
    pub const fn new() -> Self {
        PageFrameAllocator {
            bitmap: [!0u64; BITMAP_WORDS], // 1 = allocated/reserved, 0 = free
            total_frames: 0,
            free_frames: 0,
            used_frames: 0,
            initialized: false,
        }
    }

    pub fn init(&mut self) {
        // 1. Initially mark all frames as reserved
        for word in self.bitmap.iter_mut() {
            *word = !0u64;
        }

        self.total_frames = 0;
        self.free_frames = 0;
        self.used_frames = 0;

        // 2. Query Limine Memory Map and free usable regions
        let entry_count = crate::limine::memmap_entries_count();
        for i in 0..entry_count {
            if let Some(entry) = crate::limine::get_memmap_entry(i) {
                let start_frame = (entry.base as usize) / PAGE_SIZE;
                let frame_count = (entry.length as usize) / PAGE_SIZE;
                let end_frame = (start_frame + frame_count).min(MAX_FRAMES);

                if entry.typ == crate::limine::LIMINE_MEMMAP_USABLE {
                    for f in start_frame..end_frame {
                        self.set_free(f);
                        self.free_frames += 1;
                        self.total_frames += 1;
                    }
                } else {
                    self.total_frames += frame_count.min(MAX_FRAMES.saturating_sub(start_frame));
                    self.used_frames += frame_count.min(MAX_FRAMES.saturating_sub(start_frame));
                }
            }
        }

        // 3. Reserve lower 1 MiB (first 256 frames) to protect BIOS/IVT/BDA/VGA
        for f in 0..256.min(MAX_FRAMES) {
            if self.is_free(f) {
                self.set_used(f);
                self.free_frames = self.free_frames.saturating_sub(1);
                self.used_frames += 1;
            }
        }

        // 4. Reserve kernel physical memory area
        if let Some((kphys, _)) = crate::limine::kernel_address() {
            let kernel_start_frame = (kphys as usize) / PAGE_SIZE;
            let kernel_frame_count = (16 * 1024 * 1024) / PAGE_SIZE; // 16 MB reservation
            for f in kernel_start_frame..(kernel_start_frame + kernel_frame_count).min(MAX_FRAMES) {
                if self.is_free(f) {
                    self.set_used(f);
                    self.free_frames = self.free_frames.saturating_sub(1);
                    self.used_frames += 1;
                }
            }
        }

        self.initialized = true;
    }

    #[inline]
    fn is_free(&self, frame: usize) -> bool {
        if frame >= MAX_FRAMES {
            return false;
        }
        let word = frame / 64;
        let bit = frame % 64;
        (self.bitmap[word] & (1 << bit)) == 0
    }

    #[inline]
    fn set_free(&mut self, frame: usize) {
        if frame < MAX_FRAMES {
            let word = frame / 64;
            let bit = frame % 64;
            self.bitmap[word] &= !(1 << bit);
        }
    }

    #[inline]
    fn set_used(&mut self, frame: usize) {
        if frame < MAX_FRAMES {
            let word = frame / 64;
            let bit = frame % 64;
            self.bitmap[word] |= 1 << bit;
        }
    }

    pub fn alloc_frame(&mut self) -> Option<u64> {
        for word_idx in 0..BITMAP_WORDS {
            let word = self.bitmap[word_idx];
            if word != !0u64 {
                let bit_idx = (!word).trailing_zeros() as usize;
                let frame_idx = word_idx * 64 + bit_idx;
                if frame_idx < MAX_FRAMES {
                    self.bitmap[word_idx] |= 1 << bit_idx;
                    self.free_frames = self.free_frames.saturating_sub(1);
                    self.used_frames += 1;
                    return Some((frame_idx * PAGE_SIZE) as u64);
                }
            }
        }
        None
    }

    pub fn free_frame(&mut self, paddr: u64) {
        let frame_idx = (paddr as usize) / PAGE_SIZE;
        if frame_idx < MAX_FRAMES && !self.is_free(frame_idx) {
            self.set_free(frame_idx);
            self.free_frames += 1;
            self.used_frames = self.used_frames.saturating_sub(1);
        }
    }
}

static FRAME_ALLOCATOR: SpinLock<PageFrameAllocator> = SpinLock::new(PageFrameAllocator::new());

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
            if let Some(paddr) = fa.alloc_frame() {
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
    FRAME_ALLOCATOR.lock().alloc_frame()
}

pub fn free_frame(paddr: u64) {
    FRAME_ALLOCATOR.lock().free_frame(paddr)
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
