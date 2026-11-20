//! Aladdin Desktop SDK — IPC & Shared Memory (JDA) Primitives

use crate::syscall::{
    syscall1, syscall2, syscall3,
    SYS_IPC_SEND, SYS_IPC_RECV,
    SYS_JDA_SHM_ALLOC, SYS_JDA_SHM_RELEASE, SYS_JDA_SHM_SHARE, SYS_JDA_SHM_GET_VADDR,
};

/// IPC Message payload layout matching the Jinn microkernel (80 bytes total).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Message {
    /// PID of the sender (0 = kernel)
    pub pid_from: u32,
    /// PID of the intended recipient
    pub pid_to: u32,
    /// Application-defined message type tag
    pub tag: u32,
    pub _pad: u32,
    /// Up to 64 bytes of inline payload
    pub payload: [u8; 64],
}

impl Message {
    pub const fn zero() -> Self {
        Self {
            pid_from: 0,
            pid_to: 0,
            tag: 0,
            _pad: 0,
            payload: [0u8; 64],
        }
    }

    pub fn new(to: u32, tag: u32, payload: &[u8]) -> Self {
        let mut msg = Self::zero();
        msg.pid_to = to;
        msg.tag = tag;
        let copy_len = payload.len().min(64);
        msg.payload[..copy_len].copy_from_slice(&payload[..copy_len]);
        msg
    }
}

/// Send an IPC message to another task.
/// Returns Ok(()) on success, or Err(errno) on failure.
pub fn ipc_send(to_pid: u32, tag: u32, payload: &[u8; 64]) -> Result<(), i64> {
    let ret = unsafe {
        syscall3(
            SYS_IPC_SEND,
            to_pid as u64,
            tag as u64,
            payload.as_ptr() as u64,
        )
    };
    if ret == 0 {
        Ok(())
    } else {
        Err(ret)
    }
}

/// Receive a pending IPC message. Non-blocking.
/// Returns Ok(true) if a message was received, Ok(false) if none pending, or Err(errno).
pub fn ipc_recv(msg: &mut Message) -> Result<bool, i64> {
    let ret = unsafe {
        syscall2(
            SYS_IPC_RECV,
            msg as *mut Message as u64,
            core::mem::size_of::<Message>() as u64,
        )
    };
    if ret > 0 {
        Ok(true)
    } else if ret == 0 {
        Ok(false)
    } else {
        Err(ret)
    }
}

/// Allocate a 4 KiB shared memory frame in JDA.
/// Returns the shared memory handle on success.
pub fn shm_alloc(is_read_only: bool) -> Result<usize, i64> {
    let ret = unsafe { syscall1(SYS_JDA_SHM_ALLOC, if is_read_only { 1 } else { 0 }) };
    if ret >= 0 {
        Ok(ret as usize)
    } else {
        Err(ret)
    }
}

/// Release a previously allocated shared memory frame handle.
pub fn shm_release(handle: usize) -> Result<(), i64> {
    let ret = unsafe { syscall1(SYS_JDA_SHM_RELEASE, handle as u64) };
    if ret == 0 {
        Ok(())
    } else {
        Err(ret)
    }
}

/// Share an allocated shared memory handle with a target process PID.
pub fn shm_share(handle: usize, target_pid: usize, make_owner_ro: bool) -> Result<usize, i64> {
    let ret = unsafe {
        syscall3(
            SYS_JDA_SHM_SHARE,
            handle as u64,
            target_pid as u64,
            if make_owner_ro { 1 } else { 0 },
        )
    };
    if ret >= 0 {
        Ok(ret as usize)
    } else {
        Err(ret)
    }
}

/// Retrieve the Ring 3 virtual address for a shared memory handle mapped in the current process.
pub fn shm_get_vaddr(handle: usize) -> Result<*mut u8, i64> {
    let ret = unsafe { syscall1(SYS_JDA_SHM_GET_VADDR, handle as u64) };
    if ret > 0 {
        Ok(ret as *mut u8)
    } else {
        Err(ret)
    }
}
