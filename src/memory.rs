const HEAP_SIZE: usize = 16 * 1024;
static mut HEAP_MEMORY: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

#[derive(Debug)]
pub struct BumpAllocator {
    heap_start: usize,
    heap_end: usize,
    next: usize,
}

unsafe impl Sync for BumpAllocator {}

impl BumpAllocator {
    pub const fn new() -> Self {
        BumpAllocator {
            heap_start: 0,
            heap_end: 0,
            next: 0,
        }
    }

    pub fn init(&mut self) {
        let start = unsafe { HEAP_MEMORY.as_ptr() as usize };
        self.heap_start = start;
        self.heap_end = start + HEAP_SIZE;
        self.next = start;
    }

    pub fn allocate(&mut self, size: usize, align: usize) -> Option<&'static mut [u8]> {
        let aligned = align_up(self.next, align);
        let new_next = aligned.checked_add(size)?;
        if new_next > self.heap_end {
            return None;
        }

        self.next = new_next;
        let offset = aligned - self.heap_start;
        unsafe { Some(&mut HEAP_MEMORY[offset..offset + size]) }
    }
}

fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

static mut GLOBAL_ALLOCATOR: BumpAllocator = BumpAllocator::new();

pub fn init() {
    unsafe {
        GLOBAL_ALLOCATOR.init();
    }
}

pub fn allocate(size: usize, align: usize) -> Option<&'static mut [u8]> {
    unsafe { GLOBAL_ALLOCATOR.allocate(size, align) }
}

pub fn used_bytes() -> usize {
    unsafe { GLOBAL_ALLOCATOR.next - GLOBAL_ALLOCATOR.heap_start }
}

pub fn total_bytes() -> usize {
    HEAP_SIZE
}
