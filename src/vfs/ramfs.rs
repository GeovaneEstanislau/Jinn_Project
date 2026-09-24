/// Jinn OS — RAM Filesystem (ramfs)
///
/// An in-memory filesystem backed by the kernel heap.
/// Used for `/dev/stdin`, `/dev/stdout`, `/dev/null`, and temporary files.
///
/// Design:
///   - Fixed number of inodes (MAX_INODES).
///   - Each inode owns a 4 KiB inline data buffer (no disk I/O).
///   - Directory entries are stored as a flat array of (name, inode) pairs.
///   - All operations are O(n) over inodes/entries — acceptable for kernel /dev.

use super::{FileSystem, NodeType, Stat, VfsError, VfsResult, O_WRONLY, O_RDWR, O_CREAT, O_TRUNC};
use crate::memory::SpinLock;

const MAX_INODES:     usize = 64;
const MAX_NAME:       usize = 64;
const INLINE_BUF:     usize = 4096;
const MAX_DIR_ENTRIES:usize = 64;

// ── Inode ─────────────────────────────────────────────────────────────────────
struct Inode {
    used:      bool,
    node_type: NodeType,
    size:      usize,
    data:      [u8; INLINE_BUF],
}

impl Inode {
    const fn empty() -> Self {
        Self { used: false, node_type: NodeType::File, size: 0, data: [0u8; INLINE_BUF] }
    }
}

// ── Directory Entry ───────────────────────────────────────────────────────────
struct DirEntry {
    used:   bool,
    name:   [u8; MAX_NAME],
    name_len: usize,
    inode:  usize,
}

impl DirEntry {
    const fn empty() -> Self {
        Self { used: false, name: [0u8; MAX_NAME], name_len: 0, inode: 0 }
    }
}

// ── RamFs internals ───────────────────────────────────────────────────────────
struct Inner {
    inodes:  [Inode; MAX_INODES],
    entries: [DirEntry; MAX_DIR_ENTRIES],
}

impl Inner {
    const fn new() -> Self {
        const INODE: Inode    = Inode::empty();
        const DENT:  DirEntry = DirEntry::empty();
        Self { inodes: [INODE; MAX_INODES], entries: [DENT; MAX_DIR_ENTRIES] }
    }

    fn alloc_inode(&mut self, node_type: NodeType) -> Option<usize> {
        for (i, inode) in self.inodes.iter_mut().enumerate() {
            if !inode.used {
                inode.used = true;
                inode.node_type = node_type;
                inode.size = 0;
                inode.data.fill(0);
                return Some(i);
            }
        }
        None
    }

    fn find_entry(&self, name: &str) -> Option<usize> {
        let nb = name.as_bytes();
        for e in self.entries.iter() {
            if e.used && e.name_len == nb.len() && &e.name[..e.name_len] == nb {
                return Some(e.inode);
            }
        }
        None
    }

    fn add_entry(&mut self, name: &str, inode: usize) -> VfsResult<()> {
        let nb = name.as_bytes();
        for slot in self.entries.iter_mut() {
            if !slot.used {
                slot.used = true;
                slot.name_len = nb.len().min(MAX_NAME);
                slot.name[..slot.name_len].copy_from_slice(&nb[..slot.name_len]);
                slot.inode = inode;
                return Ok(());
            }
        }
        Err(VfsError::NoSpace)
    }

    fn remove_entry(&mut self, name: &str) -> VfsResult<()> {
        let nb = name.as_bytes();
        for e in self.entries.iter_mut() {
            if e.used && e.name_len == nb.len() && &e.name[..e.name_len] == nb {
                e.used = false;
                self.inodes[e.inode].used = false;
                return Ok(());
            }
        }
        Err(VfsError::NotFound)
    }
}

// ── Public RamFs ──────────────────────────────────────────────────────────────
pub struct RamFs {
    inner: SpinLock<Inner>,
}

impl RamFs {
    pub const fn new() -> Self {
        Self { inner: SpinLock::new(Inner::new()) }
    }

    /// Pre-create a file (useful for `/dev/stdin` etc. at boot).
    pub fn mknod(&self, name: &str, node_type: NodeType) -> VfsResult<()> {
        let mut inner = self.inner.lock();
        let inode = inner.alloc_inode(node_type).ok_or(VfsError::NoSpace)?;
        inner.add_entry(name, inode)
    }
}

// ── /dev/null writer (sink) ───────────────────────────────────────────────────
const NULL_INODE: u64 = 0xDEAD_0000;

impl FileSystem for RamFs {
    fn name(&self) -> &'static str { "ramfs" }

    fn lookup(&self, path: &str) -> VfsResult<Stat> {
        let inner = self.inner.lock();
        let name = path.trim_start_matches('/');
        let idx  = inner.find_entry(name).ok_or(VfsError::NotFound)?;
        let inode = &inner.inodes[idx];
        Ok(Stat {
            node_type: inode.node_type,
            size:      inode.size as u64,
            inode:     idx as u64,
        })
    }

    fn open(&self, path: &str, flags: u32) -> VfsResult<u64> {
        let mut inner = self.inner.lock();
        let name = path.trim_start_matches('/');

        // Special: /dev/null
        if name == "null" { return Ok(NULL_INODE); }

        let idx = if let Some(i) = inner.find_entry(name) {
            if flags & O_TRUNC != 0 {
                inner.inodes[i].size = 0;
                inner.inodes[i].data.fill(0);
            }
            i
        } else if flags & O_CREAT != 0 {
            let i = inner.alloc_inode(NodeType::File).ok_or(VfsError::NoSpace)?;
            inner.add_entry(name, i)?;
            i
        } else {
            return Err(VfsError::NotFound);
        };
        Ok(idx as u64)
    }

    fn close(&self, _handle: u64) -> VfsResult<()> { Ok(()) }

    fn read(&self, handle: u64, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        if handle == NULL_INODE { return Ok(0); }
        let inner = self.inner.lock();
        let inode = &inner.inodes[handle as usize];
        let start = (offset as usize).min(inode.size);
        let avail = inode.size - start;
        let n = avail.min(buf.len());
        buf[..n].copy_from_slice(&inode.data[start..start + n]);
        Ok(n)
    }

    fn write(&self, handle: u64, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        if handle == NULL_INODE { return Ok(buf.len()); }
        let mut inner = self.inner.lock();
        let inode = &mut inner.inodes[handle as usize];
        let start = offset as usize;
        let end   = (start + buf.len()).min(INLINE_BUF);
        let n     = end - start;
        inode.data[start..end].copy_from_slice(&buf[..n]);
        if end > inode.size { inode.size = end; }
        Ok(n)
    }

    fn create(&self, path: &str) -> VfsResult<()> {
        let mut inner = self.inner.lock();
        let name = path.trim_start_matches('/');
        let inode = inner.alloc_inode(NodeType::File).ok_or(VfsError::NoSpace)?;
        inner.add_entry(name, inode)
    }

    fn remove(&self, path: &str) -> VfsResult<()> {
        let mut inner = self.inner.lock();
        let name = path.trim_start_matches('/');
        inner.remove_entry(name)
    }

    fn readdir(&self, _path: &str, cb: &mut dyn FnMut(&str, NodeType)) {
        let inner = self.inner.lock();
        for e in inner.entries.iter() {
            if e.used {
                if let Ok(name) = core::str::from_utf8(&e.name[..e.name_len]) {
                    cb(name, inner.inodes[e.inode].node_type);
                }
            }
        }
    }
}

/// Global `/dev` filesystem instance.
pub static DEV_FS: RamFs = RamFs::new();

/// Initialize `/dev` with standard device nodes.
pub fn init_dev() {
    DEV_FS.mknod("null",   NodeType::CharDevice).ok();
    DEV_FS.mknod("stdin",  NodeType::CharDevice).ok();
    DEV_FS.mknod("stdout", NodeType::CharDevice).ok();
    DEV_FS.mknod("stderr", NodeType::CharDevice).ok();
}
