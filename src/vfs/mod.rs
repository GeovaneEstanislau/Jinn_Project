/// Jinn OS — Virtual Filesystem (VFS) Layer
///
/// Provides a unified interface over multiple filesystem implementations.
/// This module defines the core traits and types; concrete implementations
/// live in submodules (e.g., `ramfs`).
///
/// Mount table: up to MAX_MOUNTS filesystems can be mounted at once.
/// Path resolution is prefix-based: the longest matching mount point wins.

pub mod ramfs;

use crate::memory::SpinLock;

pub const MAX_MOUNTS:    usize = 8;
pub const MAX_PATH_LEN:  usize = 256;
pub const MAX_FD:        usize = 32;   // file descriptors per process
pub const BLOCK_SIZE:    usize = 512;

// ── VFS Error Type ────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsError {
    NotFound,
    PermissionDenied,
    NotADirectory,
    IsADirectory,
    AlreadyExists,
    InvalidPath,
    NoSpace,
    NotSupported,
    IoError,
}

pub type VfsResult<T> = Result<T, VfsError>;

// ── Open Flags ────────────────────────────────────────────────────────────────
pub const O_RDONLY: u32 = 0x0;
pub const O_WRONLY: u32 = 0x1;
pub const O_RDWR:   u32 = 0x2;
pub const O_CREAT:  u32 = 0x40;
pub const O_TRUNC:  u32 = 0x200;
pub const O_APPEND: u32 = 0x400;

// ── Node Types ────────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    File,
    Directory,
    CharDevice,
    BlockDevice,
    Symlink,
}

// ── VFS Node Metadata ─────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy)]
pub struct Stat {
    pub node_type: NodeType,
    pub size:      u64,
    pub inode:     u64,
}

// ── Filesystem Trait ──────────────────────────────────────────────────────────
/// Any filesystem that can be mounted must implement this trait.
pub trait FileSystem: Send + Sync {
    /// Return the filesystem name (e.g., "ramfs", "ext2").
    fn name(&self) -> &'static str;

    /// Look up a node by path (relative to the mount point).
    fn lookup(&self, path: &str) -> VfsResult<Stat>;

    /// Open a file; returns an opaque handle index.
    fn open(&self, path: &str, flags: u32) -> VfsResult<u64>;

    /// Close a handle.
    fn close(&self, handle: u64) -> VfsResult<()>;

    /// Read up to `buf.len()` bytes from `handle` at `offset`.
    fn read(&self, handle: u64, offset: u64, buf: &mut [u8]) -> VfsResult<usize>;

    /// Write `buf` into `handle` at `offset`.
    fn write(&self, handle: u64, offset: u64, buf: &[u8]) -> VfsResult<usize>;

    /// Create a file at `path`.
    fn create(&self, path: &str) -> VfsResult<()>;

    /// Remove a file at `path`.
    fn remove(&self, path: &str) -> VfsResult<()>;

    /// List directory entries. Calls `cb` for each entry name.
    fn readdir(&self, path: &str, cb: &mut dyn FnMut(&str, NodeType));
}

// ── Mount Table ───────────────────────────────────────────────────────────────
struct MountEntry {
    prefix: [u8; MAX_PATH_LEN],
    prefix_len: usize,
    fs: &'static dyn FileSystem,
}

struct MountTable {
    entries: [Option<MountEntry>; MAX_MOUNTS],
    count: usize,
}

impl MountTable {
    const fn new() -> Self {
        const NONE: Option<MountEntry> = None;
        Self { entries: [NONE; MAX_MOUNTS], count: 0 }
    }

    fn mount(&mut self, prefix: &str, fs: &'static dyn FileSystem) -> VfsResult<()> {
        if self.count >= MAX_MOUNTS { return Err(VfsError::NoSpace); }
        let mut entry = MountEntry {
            prefix: [0u8; MAX_PATH_LEN],
            prefix_len: prefix.len().min(MAX_PATH_LEN),
            fs,
        };
        entry.prefix[..entry.prefix_len].copy_from_slice(&prefix.as_bytes()[..entry.prefix_len]);
        self.entries[self.count] = Some(entry);
        self.count += 1;
        Ok(())
    }

    /// Find the filesystem whose mount prefix is the longest match for `path`.
    fn resolve<'a>(&self, path: &'a str) -> Option<(&'static dyn FileSystem, &'a str)> {
        let mut best_len = 0;
        let mut best_fs: Option<&'static dyn FileSystem> = None;

        for e in self.entries.iter().flatten() {
            let prefix = core::str::from_utf8(&e.prefix[..e.prefix_len]).unwrap_or("");
            if path.starts_with(prefix) && e.prefix_len > best_len {
                best_len = e.prefix_len;
                best_fs  = Some(e.fs);
            }
        }

        best_fs.map(|fs| {
            let rel = path.get(best_len..).unwrap_or("/");
            let rel = if rel.is_empty() { "/" } else { rel };
            (fs, rel)
        })
    }
}

static MOUNTS: SpinLock<MountTable> = SpinLock::new(MountTable::new());

// ── Public API ────────────────────────────────────────────────────────────────

/// Mount `fs` at `prefix` (e.g., `"/dev"`, `"/"`).
pub fn mount(prefix: &str, fs: &'static dyn FileSystem) -> VfsResult<()> {
    MOUNTS.lock().mount(prefix, fs)
}

pub fn stat(path: &str) -> VfsResult<Stat> {
    let table = MOUNTS.lock();
    let (fs, rel) = table.resolve(path).ok_or(VfsError::NotFound)?;
    fs.lookup(rel)
}

pub fn open(path: &str, flags: u32) -> VfsResult<u64> {
    let table = MOUNTS.lock();
    let (fs, rel) = table.resolve(path).ok_or(VfsError::NotFound)?;
    fs.open(rel, flags)
}

pub fn close(path: &str, handle: u64) -> VfsResult<()> {
    let table = MOUNTS.lock();
    let (fs, _) = table.resolve(path).ok_or(VfsError::NotFound)?;
    fs.close(handle)
}

pub fn read(path: &str, handle: u64, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
    let table = MOUNTS.lock();
    let (fs, _) = table.resolve(path).ok_or(VfsError::NotFound)?;
    fs.read(handle, offset, buf)
}

pub fn write(path: &str, handle: u64, offset: u64, buf: &[u8]) -> VfsResult<usize> {
    let table = MOUNTS.lock();
    let (fs, _) = table.resolve(path).ok_or(VfsError::NotFound)?;
    fs.write(handle, offset, buf)
}
