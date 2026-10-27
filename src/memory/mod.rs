pub mod paging;
pub mod heap;
pub mod alloc;
pub mod buddy;

pub use alloc::{alloc_frame, alloc_frames, free_frame, free_frames_order, total_bytes, used_bytes, init, SpinLock};
