#![allow(dead_code)]

pub const PAGE_SIZE: usize = 4096;
const MAX_FRAMES: usize = 1024 * 1024;
const BITMAP_WORDS: usize = MAX_FRAMES / 64;
const RESERVED_LOW_MEMORY: u64 = 1024 * 1024;

pub struct BuddyAllocator {
    allocated: [u64; BITMAP_WORDS],
    usable: [u64; BITMAP_WORDS],
    pub total_frames: usize,
    pub free_frames: usize,
    pub used_frames: usize,
    frame_limit: usize,
}

impl BuddyAllocator {
    pub const fn new() -> Self {
        Self {
            allocated: [u64::MAX; BITMAP_WORDS],
            usable: [0; BITMAP_WORDS],
            total_frames: 0,
            free_frames: 0,
            used_frames: 0,
            frame_limit: 0,
        }
    }

    pub fn init(&mut self) {
        self.allocated = [u64::MAX; BITMAP_WORDS];
        self.usable = [0; BITMAP_WORDS];
        self.total_frames = 0;
        self.free_frames = 0;
        self.used_frames = 0;
        self.frame_limit = 0;

        unsafe {
            let response = core::ptr::read_volatile(&crate::limine::LIMINE_MEMMAP_REQUEST.response);
            if response.is_null() {
                return;
            }

            let count = core::ptr::read_volatile(&(*response).entry_count);
            let entries = core::ptr::read_volatile(&(*response).entries);
            if entries.is_null() {
                return;
            }

            for index in 0..count {
                let entry_ptr = core::ptr::read_volatile(entries.add(index as usize));
                if entry_ptr.is_null() {
                    continue;
                }

                let entry = core::ptr::read_volatile(entry_ptr);
                if entry.typ != crate::limine::LIMINE_MEMMAP_USABLE || entry.length == 0 {
                    continue;
                }

                let start = entry.base.max(RESERVED_LOW_MEMORY).saturating_add(PAGE_SIZE as u64 - 1)
                    / PAGE_SIZE as u64;
                let end = entry.base.saturating_add(entry.length) / PAGE_SIZE as u64;
                let first_frame = start as usize;
                let end_frame = (end as usize).min(MAX_FRAMES);

                if first_frame >= end_frame {
                    continue;
                }

                self.frame_limit = self.frame_limit.max(end_frame);
                for frame in first_frame..end_frame {
                    Self::set_bit(&mut self.usable, frame, true);
                    Self::set_bit(&mut self.allocated, frame, false);
                    self.total_frames += 1;
                    self.free_frames += 1;
                }
            }
        }
    }

    pub fn alloc(&mut self, order: usize) -> Option<u64> {
        if order != 0 || self.free_frames == 0 {
            return None;
        }

        for frame in 0..self.frame_limit {
            if self.get_bit(&self.usable, frame) && !self.get_bit(&self.allocated, frame) {
                Self::set_bit(&mut self.allocated, frame, true);
                self.free_frames -= 1;
                self.used_frames += 1;
                return Some((frame * PAGE_SIZE) as u64);
            }
        }

        None
    }

    pub fn free(&mut self, paddr: u64, order: usize) {
        if order != 0 || paddr % PAGE_SIZE as u64 != 0 {
            return;
        }

        let frame = (paddr / PAGE_SIZE as u64) as usize;
        if frame >= self.frame_limit
            || !self.get_bit(&self.usable, frame)
            || !self.get_bit(&self.allocated, frame)
        {
            return;
        }

        Self::set_bit(&mut self.allocated, frame, false);
        self.free_frames += 1;
        self.used_frames -= 1;
    }

    fn get_bit(&self, bitmap: &[u64; BITMAP_WORDS], frame: usize) -> bool {
        bitmap[frame / 64] & (1u64 << (frame % 64)) != 0
    }

    fn set_bit(bitmap: &mut [u64; BITMAP_WORDS], frame: usize, value: bool) {
        let mask = 1u64 << (frame % 64);
        let word = &mut bitmap[frame / 64];
        if value {
            *word |= mask;
        } else {
            *word &= !mask;
        }
    }
}
