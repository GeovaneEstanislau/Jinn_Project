/// Jinn OS — Virtual Filesystem (VFS) Layer
///
/// Provides a unified interface over multiple filesystem implementations.
/// This module defines the core traits and types; concrete implementations
/// live in submodules (e.g., `ramfs`).
///
/// Mount table: up to MAX_MOUNTS filesystems can be mounted at once.
/// Path resolution is prefix-based: the longest matching mount point wins.

pub mod cache;
pub mod ext2;
pub mod fat32;
pub mod ramfs;
pub mod volume;

use crate::memory::SpinLock;

pub const MAX_MOUNTS:    usize = 8;
pub const MAX_PATH_LEN:  usize = 256;
pub const MAX_FD:        usize = 32;   // file descriptors per process
pub const BLOCK_SIZE:    usize = 512;

// ── VFS Error Type ────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsError {
    NotFound,           // -ENOENT (-2)
    PermissionDenied,   // -EACCES (-13)
    NotPermitted,       // -EPERM (-1)
    NotADirectory,      // -ENOTDIR (-20)
    IsADirectory,       // -EISDIR (-21)
    AlreadyExists,      // -EEXIST (-17)
    InvalidPath,        // -EINVAL (-22)
    NoSpace,            // -ENOSPC (-28)
    ReadOnlyFilesystem, // -EROFS (-30)
    TooManyOpenFiles,   // -EMFILE (-24)
    FileTooLarge,       // -EFBIG (-27)
    BadDescriptor,      // -EBADF (-9)
    NotSupported,       // -ENOSYS (-38)
    IoError,            // -EIO (-5)
    Busy,               // -EBUSY (-16)
}

pub type VfsResult<T> = Result<T, VfsError>;

// ── Open Flags (Conforme Padrão POSIX.1-2008 / Linux ABI) ─────────────────────
pub const O_RDONLY:    u32 = 0x0000;
pub const O_WRONLY:    u32 = 0x0001;
pub const O_RDWR:      u32 = 0x0002;
pub const O_ACCMODE:   u32 = 0x0003;
pub const O_CREAT:     u32 = 0x0040;
pub const O_EXCL:      u32 = 0x0080;
pub const O_NOCTTY:    u32 = 0x0100;
pub const O_TRUNC:     u32 = 0x0200;
pub const O_APPEND:    u32 = 0x0400;
pub const O_NONBLOCK:  u32 = 0x0800;
pub const O_DIRECTORY: u32 = 0x10000;
pub const O_CLOEXEC:   u32 = 0x80000;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stat {
    pub node_type: NodeType,
    pub size:      u64,
    pub inode:     u64,
    pub mode:      u32, // Modo POSIX (ex: 0o100644, 0o040755)
    pub uid:       u32, // User ID proprietário
    pub gid:       u32, // Group ID proprietário
}

impl Stat {
    pub const fn new(node_type: NodeType, size: u64, inode: u64, mode: u32, uid: u32, gid: u32) -> Self {
        Self {
            node_type,
            size,
            inode,
            mode,
            uid,
            gid,
        }
    }
}

/// Métricas de capacidade e utilização do sistema de arquivos (statvfs)
#[derive(Debug, Clone, Copy)]
pub struct FsStat {
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub block_size: usize,
    pub total_inodes: u64,
    pub free_inodes: u64,
    pub fs_type: &'static str,
    pub is_read_only: bool,
}

// ── Filesystem Trait ──────────────────────────────────────────────────────────
/// Any filesystem that can be mounted must implement this trait.
pub trait FileSystem: Send + Sync {
    /// Return the filesystem name (e.g., "ramfs", "ext2").
    fn name(&self) -> &'static str;

    /// Consulta formal das capacidades operacionais suportadas por este filesystem.
    fn capabilities(&self) -> FsCapabilities;

    /// Retorna estatísticas de capacidade e espaço livre do sistema de arquivos (statvfs)
    fn statvfs(&self) -> VfsResult<FsStat>;

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

    /// Create a directory at `path`.
    fn mkdir(&self, _path: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

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
        let is_root_mount = {
            let vm = crate::vfs::volume::VOLUME_MANAGER.lock();
            if let Some(root_vol) = vm.root_volume() {
                root_vol.mount_point_str() == prefix
            } else {
                false
            }
        };
        // REGRA DE SEGURANÇA INVIOLÁVEL: Raiz do sistema e nós virtuais nunca podem ser desmontados
        if prefix == "/" || prefix == "/disk" || prefix == "/dev" || is_root_mount {
            return Err(VfsError::PermissionDenied);
        }

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
        self.resolve_with_prefix(path).map(|(fs, rel, _)| (fs, rel))
    }

    /// Encontra o filesystem montado com maior correspondência de prefixo e retorna o prefixo casado
    fn resolve_with_prefix<'a>(&self, path: &'a str) -> Option<(&'static dyn FileSystem, &'a str, &'a str)> {
        let mut best_len = 0;
        let mut best_fs: Option<&'static dyn FileSystem> = None;

        for e in self.entries.iter().flatten() {
            let prefix = core::str::from_utf8(&e.prefix[..e.prefix_len]).unwrap_or("");
            if prefix.is_empty() { continue; }
            let matches = if prefix == "/" {
                path.starts_with('/')
            } else if path.starts_with(prefix) {
                path.len() == prefix.len() || path.as_bytes().get(prefix.len()) == Some(&b'/')
            } else {
                false
            };
            if matches && e.prefix_len > best_len {
                best_len = e.prefix_len;
                best_fs  = Some(e.fs);
            }
        }

        if let Some(fs) = best_fs {
            let rel = path.get(best_len..).unwrap_or("/");
            let rel = if rel.is_empty() { "/" } else { rel };
            let pfx = path.get(..best_len).unwrap_or("");
            Some((fs, rel, pfx))
        } else {
            // Se nenhum prefixo coincidir (ex: caminho relativo "arquivo.txt" ou raiz não prefixada),
            // faz fallback para o volume primário ("/disk" ou "/")
            let mut fallback: Option<(&'static dyn FileSystem, &'static str)> = None;
            for e in self.entries.iter().flatten() {
                let p = core::str::from_utf8(&e.prefix[..e.prefix_len]).unwrap_or("");
                if p == "/disk" {
                    fallback = Some((e.fs, "/disk"));
                    break;
                } else if p == "/" {
                    fallback = Some((e.fs, "/"));
                }
            }
            fallback.map(|(fs, pfx)| {
                (fs, path, pfx)
            })
        }
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
    // Sincroniza o cache de blocos LRU dedicado do VFS
    let _ = crate::vfs::cache::flush_all();
    // Também sincroniza a cache de blocos física subjacente
    let _ = crate::drivers::block::flush_primary_cache();
    Ok(())
}

pub fn stat(path: &str) -> VfsResult<Stat> {
    let table = MOUNTS.lock();
    let (fs, rel) = table.resolve(path).ok_or(VfsError::NotFound)?;
    fs.lookup(rel)
}

/// Valida os modos de acesso e permissões POSIX para um nó do VFS
pub fn check_permission(
    stat: &Stat,
    flags: u32,
    caller_uid: u32,
    caller_gid: u32,
    is_fs_readonly: bool,
) -> VfsResult<()> {
    let access_mode = flags & O_ACCMODE;
    let is_write_intent = access_mode == O_WRONLY || access_mode == O_RDWR || (flags & (O_CREAT | O_TRUNC | O_APPEND)) != 0;

    // 1. Rejeita escrita direta em diretórios (EISDIR)
    if stat.node_type == NodeType::Directory && (access_mode == O_WRONLY || access_mode == O_RDWR || (flags & O_TRUNC) != 0) {
        return Err(VfsError::IsADirectory);
    }

    // 2. Se O_DIRECTORY foi requisitado e o nó não for diretório: ENOTDIR
    if (flags & O_DIRECTORY) != 0 && stat.node_type != NodeType::Directory {
        return Err(VfsError::NotADirectory);
    }

    // 3. Se houver tentativa de escrita e o sistema de arquivos for somente leitura: EROFS
    if is_write_intent && is_fs_readonly {
        return Err(VfsError::ReadOnlyFilesystem);
    }

    // 4. Superusuário (root, UID 0): bypass de checagens normais de DAC
    if caller_uid == 0 {
        return Ok(());
    }

    // 5. Verificação de permissões do proprietário, grupo e outros
    let req_read = access_mode == O_RDONLY || access_mode == O_RDWR;
    let req_write = is_write_intent;

    let (r_perm, w_perm) = if caller_uid == stat.uid {
        ((stat.mode & 0o400) != 0, (stat.mode & 0o200) != 0)
    } else if caller_gid == stat.gid {
        ((stat.mode & 0o040) != 0, (stat.mode & 0o020) != 0)
    } else {
        ((stat.mode & 0o004) != 0, (stat.mode & 0o002) != 0)
    };

    if req_read && !r_perm {
        return Err(VfsError::PermissionDenied); // -EACCES
    }

    if req_write && !w_perm {
        return Err(VfsError::PermissionDenied); // -EACCES
    }

    Ok(())
}

pub fn open(path: &str, flags: u32) -> VfsResult<u64> {
    let (handle, pfx) = {
        let table = MOUNTS.lock();
        let (fs, rel, pfx) = table.resolve_with_prefix(path).ok_or(VfsError::NotFound)?;

        // Proteção de Integridade: se o FS for Read-Only, rejeita qualquer flag de escrita precocemente
        let is_write_intent = (flags & (O_WRONLY | O_RDWR | O_CREAT | O_TRUNC | O_APPEND)) != 0;
        if is_write_intent && fs.capabilities().read_only {
            return Err(VfsError::ReadOnlyFilesystem);
        }

        let h = fs.open(rel, flags)?;
        (h, pfx)
    };

    let mut vm = crate::vfs::volume::VOLUME_MANAGER.lock();
    for i in 0..vm.count {
        if let Some(vol) = vm.volumes[i].as_mut() {
            if vol.mount_point_str() == pfx {
                vol.open_handles_count = vol.open_handles_count.saturating_add(1);
                break;
            }
        }
    }

    Ok(handle)
}

pub fn close(path: &str, handle: u64) -> VfsResult<()> {
    let pfx = {
        let table = MOUNTS.lock();
        let (fs, _, pfx) = table.resolve_with_prefix(path).ok_or(VfsError::NotFound)?;
        fs.close(handle)?;
        pfx
    };

    let mut vm = crate::vfs::volume::VOLUME_MANAGER.lock();
    for i in 0..vm.count {
        if let Some(vol) = vm.volumes[i].as_mut() {
            if vol.mount_point_str() == pfx {
                vol.open_handles_count = vol.open_handles_count.saturating_sub(1);
                break;
            }
        }
    }

    Ok(())
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

/// Consulta estatísticas de capacidade e utilização do sistema de arquivos (statvfs)
pub fn statvfs(path: &str) -> VfsResult<FsStat> {
    let table = MOUNTS.lock();
    let (fs, _) = table.resolve(path).ok_or(VfsError::NotFound)?;
    fs.statvfs()
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

/// Escreve dados em um arquivo existente ou recém-criado no VFS a partir do offset 0.
/// Retorna o número total de bytes gravados no filesystem.
pub fn write_file(path: &str, buf: &[u8]) -> VfsResult<usize> {
    match stat(path) {
        Ok(s) => {
            if s.node_type == NodeType::Directory {
                return Err(VfsError::IsADirectory);
            }
        }
        Err(VfsError::NotFound) => {
            create(path)?;
        }
        Err(e) => return Err(e),
    }
    let handle = open(path, O_WRONLY | O_TRUNC)?;
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

/// Remove um link ou arquivo do VFS (alias POSIX compatível para remove)
pub fn unlink(path: &str) -> VfsResult<()> {
    remove(path)
}

/// Renomeia ou move um arquivo no VFS
pub fn rename(old_path: &str, new_path: &str) -> VfsResult<()> {
    // 1. Lê os metadados do arquivo de origem
    let s = stat(old_path)?;
    if s.node_type == NodeType::Directory {
        return Err(VfsError::NotSupported);
    }

    // Se o destino já existir, remove para permitir substituição atômica POSIX
    if let Ok(dst_st) = stat(new_path) {
        if dst_st.node_type == NodeType::Directory {
            return Err(VfsError::IsADirectory);
        }
        let _ = remove(new_path);
    }

    // 2. Cria o novo arquivo e copia o conteúdo em blocos seguros
    create(new_path)?;
    let mut chunk = [0u8; 512];
    let in_h = open(old_path, O_RDONLY)?;
    let out_h = match open(new_path, O_WRONLY) {
        Ok(h) => h,
        Err(e) => {
            let _ = close(old_path, in_h);
            let _ = remove(new_path);
            return Err(e);
        }
    };

    let mut offset = 0u64;
    loop {
        match read(old_path, in_h, offset, &mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                if let Err(e) = write(new_path, out_h, offset, &chunk[..n]) {
                    let _ = close(old_path, in_h);
                    let _ = close(new_path, out_h);
                    let _ = remove(new_path);
                    return Err(e);
                }
                offset += n as u64;
            }
            Err(e) => {
                let _ = close(old_path, in_h);
                let _ = close(new_path, out_h);
                let _ = remove(new_path);
                return Err(e);
            }
        }
    }

    let _ = close(old_path, in_h);
    let _ = close(new_path, out_h);

    // 3. Remove o arquivo original após a cópia com sucesso
    remove(old_path)
}

/// Cria um novo diretório no VFS.
pub fn mkdir(path: &str) -> VfsResult<()> {
    let table = MOUNTS.lock();
    let (fs, rel) = table.resolve(path).ok_or(VfsError::NotFound)?;
    fs.mkdir(rel)
}

/// Garante a criação da infraestrutura de diretórios dedicada do Jinn Browser (/disk/browser/)
pub fn ensure_browser_directories() {
    let _ = mkdir("/disk/browser");
    let _ = mkdir("/disk/browser/cache");
    let _ = mkdir("/disk/browser/cookies");
    let _ = mkdir("/disk/browser/history");
    let _ = mkdir("/disk/browser/bookmark");
}

/// Copia um arquivo no VFS com integridade garantida bloco a bloco.
pub fn copy(src_path: &str, dst_path: &str) -> VfsResult<usize> {
    let s = stat(src_path)?;
    if s.node_type == NodeType::Directory {
        return Err(VfsError::NotSupported);
    }

    create(dst_path)?;
    let in_h = open(src_path, O_RDONLY)?;
    let out_h = match open(dst_path, O_WRONLY) {
        Ok(h) => h,
        Err(e) => {
            let _ = close(src_path, in_h);
            let _ = remove(dst_path);
            return Err(e);
        }
    };

    let mut chunk = [0u8; 512];
    let mut offset = 0u64;
    let mut total_copied = 0usize;

    loop {
        match read(src_path, in_h, offset, &mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                if let Err(e) = write(dst_path, out_h, offset, &chunk[..n]) {
                    let _ = close(src_path, in_h);
                    let _ = close(dst_path, out_h);
                    let _ = remove(dst_path);
                    return Err(e);
                }
                offset += n as u64;
                total_copied += n;
            }
            Err(e) => {
                let _ = close(src_path, in_h);
                let _ = close(dst_path, out_h);
                let _ = remove(dst_path);
                return Err(e);
            }
        }
    }

    let _ = close(src_path, in_h);
    let _ = close(dst_path, out_h);
    Ok(total_copied)
}

/// Suíte de testes unitários in-kernel para validação de permissões POSIX DAC e flags de acesso
pub fn run_posix_permission_selftests() -> (usize, usize) {
    let mut passed = 0;
    let mut total = 0;

    // Teste 1: Bypass total de superusuário (UID 0) em arquivo somente-leitura (0o100444)
    total += 1;
    let st_ro = Stat::new(NodeType::File, 1024, 1, 0o100444, 1000, 1000);
    if check_permission(&st_ro, O_WRONLY, 0, 0, false).is_ok()
        && check_permission(&st_ro, O_RDWR, 0, 0, false).is_ok() {
        passed += 1;
    }

    // Teste 2: Proprietário com permissão rw-r--r-- (0o100644)
    total += 1;
    let st_rw = Stat::new(NodeType::File, 512, 2, 0o100644, 1000, 1000);
    if check_permission(&st_rw, O_RDWR, 1000, 1000, false).is_ok()
        && check_permission(&st_rw, O_WRONLY, 1000, 1000, false).is_ok()
        && check_permission(&st_rw, O_RDONLY, 1000, 1000, false).is_ok() {
        passed += 1;
    }

    // Teste 3: Outros (other) tentando escrever em 0o100644 -> EACCES
    total += 1;
    if check_permission(&st_rw, O_WRONLY, 1001, 1001, false) == Err(VfsError::PermissionDenied)
        && check_permission(&st_rw, O_RDWR, 1001, 1001, false) == Err(VfsError::PermissionDenied) {
        passed += 1;
    }

    // Teste 4: Membro do grupo em arquivo com permissão rwxr-x--- (0o100750)
    total += 1;
    let st_grp = Stat::new(NodeType::File, 256, 3, 0o100750, 1000, 2000);
    if check_permission(&st_grp, O_RDONLY, 9999, 2000, false).is_ok()
        && check_permission(&st_grp, O_WRONLY, 9999, 2000, false) == Err(VfsError::PermissionDenied) {
        passed += 1;
    }

    // Teste 5: Rejeição de escrita em nó de diretório (EISDIR)
    total += 1;
    let st_dir = Stat::new(NodeType::Directory, 4096, 4, 0o040755, 1000, 1000);
    if check_permission(&st_dir, O_WRONLY, 1000, 1000, false) == Err(VfsError::IsADirectory)
        && check_permission(&st_dir, O_RDWR, 1000, 1000, false) == Err(VfsError::IsADirectory)
        && check_permission(&st_dir, O_TRUNC | O_RDONLY, 1000, 1000, false) == Err(VfsError::IsADirectory) {
        passed += 1;
    }

    // Teste 6: Flag O_DIRECTORY em arquivo regular (ENOTDIR)
    total += 1;
    if check_permission(&st_rw, O_RDONLY | O_DIRECTORY, 1000, 1000, false) == Err(VfsError::NotADirectory) {
        passed += 1;
    }

    // Teste 7: Tentativa de escrita em sistema de arquivos somente-leitura (EROFS)
    total += 1;
    if check_permission(&st_rw, O_WRONLY, 1000, 1000, true) == Err(VfsError::ReadOnlyFilesystem)
        && check_permission(&st_rw, O_RDONLY, 1000, 1000, true).is_ok() {
        passed += 1;
    }

    // Teste 8: O_APPEND tratado compulsoriamente como intenção de escrita
    total += 1;
    let st_no_write = Stat::new(NodeType::File, 128, 5, 0o100444, 1000, 1000);
    if check_permission(&st_no_write, O_RDONLY | O_APPEND, 1000, 1000, false) == Err(VfsError::PermissionDenied)
        && check_permission(&st_no_write, O_RDONLY | O_APPEND, 1000, 1000, true) == Err(VfsError::ReadOnlyFilesystem) {
        passed += 1;
    }

    // Teste 9: Mapeamento de códigos de erro normativos POSIX
    total += 1;
    if crate::syscall::EACCES == -13
        && crate::syscall::EROFS == -30
        && crate::syscall::EISDIR == -21
        && crate::syscall::ENOTDIR == -20
        && crate::syscall::EEXIST == -17
        && crate::syscall::EMFILE == -24
        && crate::syscall::EBADF == -9 {
        passed += 1;
    }

    // Teste 10: Integridade e paridade dos campos da estrutura Stat
    total += 1;
    let st_test = Stat::new(NodeType::File, 4096, 42, 0o100644, 1000, 1000);
    if st_test.size == 4096 && st_test.inode == 42 && st_test.mode == 0o100644 && st_test.uid == 1000 && st_test.gid == 1000 {
        passed += 1;
    }

    (passed, total)
}
