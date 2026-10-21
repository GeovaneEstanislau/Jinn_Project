/// Jinn OS — Virtual Filesystem (VFS) Layer
///
/// Provides a unified interface over multiple filesystem implementations.
/// This module defines the core traits and types; concrete implementations
/// live in submodules (e.g., `ramfs`).
///
/// Mount table: up to MAX_MOUNTS filesystems can be mounted at once.
/// Path resolution is prefix-based: the longest matching mount point wins.

pub mod ext2;
pub mod fat32;
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
    Busy,
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

// ── Capabilities do Sistema de Arquivos ──────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FsCapabilities {
    pub read_only: bool,
    pub supports_create: bool,
    pub supports_remove: bool,
    pub supports_write: bool,
    pub supports_directories: bool,
    pub supports_journaling: bool,
    pub supports_permissions: bool,
    pub max_file_size: u64,
    pub name: &'static str,
}

impl FsCapabilities {
    pub const fn read_only(name: &'static str, max_file_size: u64) -> Self {
        Self {
            read_only: true,
            supports_create: false,
            supports_remove: false,
            supports_write: false,
            supports_directories: true,
            supports_journaling: false,
            supports_permissions: true,
            max_file_size,
            name,
        }
    }

    pub const fn read_write(name: &'static str, max_file_size: u64) -> Self {
        Self {
            read_only: false,
            supports_create: true,
            supports_remove: true,
            supports_write: true,
            supports_directories: true,
            supports_journaling: false,
            supports_permissions: false,
            max_file_size,
            name,
        }
    }
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

    /// Consulta formal das capacidades operacionais suportadas por este filesystem.
    fn capabilities(&self) -> FsCapabilities;

    /// Sincroniza metadados e blocos modificados pendentes para a mídia física subjacente.
    fn sync(&self) -> VfsResult<()>;

    /// Notifica o filesystem sobre sua desmontagem iminente, liberando descritores e caches.
    fn unmount(&self) -> VfsResult<()>;

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
#[derive(Clone, Copy)]
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

    fn unmount(&mut self, prefix: &str) -> VfsResult<()> {
        let mut found_idx = None;
        for (i, e) in self.entries.iter().enumerate().take(self.count) {
            if let Some(entry) = e {
                let e_prefix = core::str::from_utf8(&entry.prefix[..entry.prefix_len]).unwrap_or("");
                if e_prefix == prefix {
                    found_idx = Some((i, entry.fs));
                    break;
                }
            }
        }

        if let Some((idx, fs)) = found_idx {
            // Sincroniza e desmonta o filesystem antes de remover da tabela de montagem
            fs.sync()?;
            fs.unmount()?;

            // Compacta a tabela de montagem
            for i in idx..(self.count - 1) {
                self.entries[i] = self.entries[i + 1];
            }
            self.entries[self.count - 1] = None;
            self.count -= 1;
            Ok(())
        } else {
            Err(VfsError::NotFound)
        }
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

/// Desmonta o sistema de arquivos montado no prefixo especificado (ex: `"/ext2"` ou `"/disk"`).
pub fn unmount(prefix: &str) -> VfsResult<()> {
    MOUNTS.lock().unmount(prefix)
}

/// Sincroniza todos os sistemas de arquivos atualmente montados no VFS.
pub fn sync_all() -> VfsResult<()> {
    let table = MOUNTS.lock();
    for entry_opt in table.entries.iter().take(table.count) {
        if let Some(ref entry) = entry_opt {
            let _ = entry.fs.sync();
        }
    }
    // Também sincroniza a cache de blocos física subjacente
    let _ = crate::drivers::block::flush_primary_cache();
    Ok(())
}

pub fn stat(path: &str) -> VfsResult<Stat> {
    let table = MOUNTS.lock();
    let (fs, rel) = table.resolve(path).ok_or(VfsError::NotFound)?;
    fs.lookup(rel)
}

pub fn open(path: &str, flags: u32) -> VfsResult<u64> {
    let table = MOUNTS.lock();
    let (fs, rel) = table.resolve(path).ok_or(VfsError::NotFound)?;

    // Proteção de Integridade: se o FS for Read-Only, rejeita qualquer flag de escrita precocemente
    let is_write_intent = (flags & (O_WRONLY | O_RDWR | O_CREAT | O_TRUNC | O_APPEND)) != 0;
    if is_write_intent && fs.capabilities().read_only {
        return Err(VfsError::PermissionDenied);
    }

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

pub fn readdir<F>(path: &str, mut cb: F) -> VfsResult<()>
where
    F: FnMut(&str, NodeType),
{
    let table = MOUNTS.lock();
    let (fs, rel) = table.resolve(path).ok_or(VfsError::NotFound)?;
    fs.readdir(rel, &mut cb);
    Ok(())
}

pub fn list_mounts<F>(mut cb: F)
where
    F: FnMut(&str, &'static str),
{
    let table = MOUNTS.lock();
    for e in table.entries.iter().flatten() {
        if let Ok(prefix) = core::str::from_utf8(&e.prefix[..e.prefix_len]) {
            cb(prefix, e.fs.name());
        }
    }
}

/// Consulta as capacidades formais e limitações do sistema de arquivos para o caminho especificado.
pub fn query_capabilities(path: &str) -> VfsResult<FsCapabilities> {
    let table = MOUNTS.lock();
    let (fs, _) = table.resolve(path).ok_or(VfsError::NotFound)?;
    Ok(fs.capabilities())
}

/// Lê todo o conteúdo de um arquivo do VFS para o buffer fornecido.
/// Retorna o número total de bytes lidos.
pub fn read_file(path: &str, buf: &mut [u8]) -> VfsResult<usize> {
    let stat = stat(path)?;
    if stat.node_type != NodeType::File {
        return Err(VfsError::IsADirectory);
    }
    let handle = open(path, O_RDONLY)?;
    let mut total = 0;
    while total < buf.len() {
        let n = read(path, handle, total as u64, &mut buf[total..])?;
        if n == 0 {
            break;
        }
        total += n;
    }
    let _ = close(path, handle);
    Ok(total)
}

/// Escreve dados em um arquivo existente no VFS a partir do offset 0.
/// Retorna o número total de bytes gravados no filesystem.
pub fn write_file(path: &str, buf: &[u8]) -> VfsResult<usize> {
    let stat = stat(path)?;
    if stat.node_type != NodeType::File {
        return Err(VfsError::IsADirectory);
    }
    let handle = open(path, O_WRONLY)?;
    let mut total = 0;
    while total < buf.len() {
        let n = write(path, handle, total as u64, &buf[total..])?;
        if n == 0 {
            break;
        }
        total += n;
    }
    let _ = close(path, handle);
    Ok(total)
}

/// Cria um novo arquivo no VFS.
pub fn create(path: &str) -> VfsResult<()> {
    let table = MOUNTS.lock();
    let (fs, rel) = table.resolve(path).ok_or(VfsError::NotFound)?;
    fs.create(rel)
}

/// Remove um arquivo existente do VFS.
pub fn remove(path: &str) -> VfsResult<()> {
    let table = MOUNTS.lock();
    let (fs, rel) = table.resolve(path).ok_or(VfsError::NotFound)?;
    fs.remove(rel)
}
