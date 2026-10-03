pub mod paging;
pub mod heap;
pub mod alloc;
pub mod buddy;

pub use alloc::{alloc_frame, free_frame, total_bytes, used_bytes, init, SpinLock};
