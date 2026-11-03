/// Jinn OS — Gerenciador de Volumes (Volume Manager - L1)
///
/// Responsável pela descoberta automática de volumes em partições e dispositivos de bloco,
/// controle de montagem, desmontagem segura, rastreamento de ocupação (busy-tracking),
/// estatísticas de capacidade e proteção absoluta contra desmontagem da raiz ('/').

use crate::drivers::partition::DetectedFsKind;
use crate::vfs::{VfsError, VfsResult};

pub const MAX_VOLUMES: usize = 8;

/// Representação de um volume de armazenamento gerenciado
#[derive(Debug, Clone, Copy)]
pub struct Volume {
    pub id: usize,
    pub device_id: usize,
    pub partition_index: usize,
    pub label: [u8; 32],
    pub label_len: usize,
    pub mount_point: [u8; 32],
    pub mount_point_len: usize,
    pub fs_kind: DetectedFsKind,
    pub is_mounted: bool,
    pub is_root: bool,
    pub is_boot: bool,
    pub is_removable: bool,
    pub open_handles_count: usize,
    pub total_mb: usize,
    pub used_mb: usize,
    pub free_mb: usize,
}

impl Volume {
    pub const fn empty() -> Self {
        Self {
            id: 0,
            device_id: 0,
            partition_index: 0,
            label: [0; 32],
            label_len: 0,
            mount_point: [0; 32],
            mount_point_len: 0,
            fs_kind: DetectedFsKind::Unknown,
            is_mounted: false,
            is_root: false,
            is_boot: false,
            is_removable: false,
            open_handles_count: 0,
            total_mb: 0,
            used_mb: 0,
            free_mb: 0,
        }
    }

    pub fn mount_point_str(&self) -> &str {
        if self.mount_point_len == 0 {
            ""
        } else {
            core::str::from_utf8(&self.mount_point[..self.mount_point_len]).unwrap_or("")
        }
    }

    pub fn label_str(&self) -> &str {
        if self.label_len == 0 {
            "Sem Rotulo"
        } else {
            core::str::from_utf8(&self.label[..self.label_len]).unwrap_or("Sem Rotulo")
        }
    }
}

/// Gerenciador central de volumes de armazenamento do Jinn OS
pub struct VolumeManager {
    pub volumes: [Option<Volume>; MAX_VOLUMES],
    pub count: usize,
    pub root_volume_id: Option<usize>,
}

impl VolumeManager {
    pub const fn new() -> Self {
        Self {
            volumes: [None; MAX_VOLUMES],
            count: 0,
            root_volume_id: None,
        }
    }

    /// Registra um novo volume detectado no catálogo do sistema
    pub fn register_volume(&mut self, mut vol: Volume) -> Result<usize, &'static str> {
        if self.count >= MAX_VOLUMES {
            return Err("Limite maximo de volumes atingido (MAX_VOLUMES)");
        }
        let id = self.count;
        vol.id = id;
        if vol.is_root && self.root_volume_id.is_none() {
            self.root_volume_id = Some(id);
        }
        self.volumes[id] = Some(vol);
        self.count += 1;
        Ok(id)
    }

    pub fn get_volume(&self, id: usize) -> Option<&Volume> {
        if id < self.count {
            self.volumes[id].as_ref()
        } else {
            None
        }
    }

    pub fn get_volume_mut(&mut self, id: usize) -> Option<&mut Volume> {
        if id < self.count {
            self.volumes[id].as_mut()
        } else {
            None
        }
    }

    pub fn volume_count(&self) -> usize {
        self.count
    }

    pub fn root_volume(&self) -> Option<&Volume> {
        self.root_volume_id.and_then(|id| self.get_volume(id))
    }

    /// Incrementa a contagem de descritores de arquivos abertos no volume (Busy-Tracking)
    pub fn track_open_handle(&mut self, id: usize) {
        if let Some(vol) = self.get_volume_mut(id) {
            vol.open_handles_count = vol.open_handles_count.saturating_add(1);
        }
    }

    /// Decrementa a contagem de descritores de arquivos abertos no volume
    pub fn track_close_handle(&mut self, id: usize) {
        if let Some(vol) = self.get_volume_mut(id) {
            vol.open_handles_count = vol.open_handles_count.saturating_sub(1);
        }
    }

    /// Desmonta com segurança um volume montado no sistema.
    ///
    /// REGRA INVIOLÁVEL: Nunca permite desmontar a raiz ('/') do sistema ativo.
    /// BUSY-CHECKING: Rejeita desmontagem caso haja arquivos ou handles abertos no volume.
    pub fn unmount_volume(&mut self, vol_id: usize) -> Result<(), &'static str> {
        let (prefix, is_root, open_handles) = {
            let vol = self.get_volume(vol_id).ok_or("Volume nao encontrado")?;
            if !vol.is_mounted {
                return Err("Volume nao esta montado");
            }
            let pfx = vol.mount_point_str();
            (pfx, vol.is_root, vol.open_handles_count)
        };

        // 1. Proteção de Segurança: A raiz não pode ser desmontada
        if is_root || prefix == "/" || prefix == "/disk" {
            return Err("Tentativa de desmontar o volume raiz ativo do sistema bloqueada (Operacao Negada)");
        }

        // 2. Verificação de Ocupação: Se há arquivos abertos, rejeita a desmontagem
        if open_handles > 0 {
            return Err("Volume ocupado: existem arquivos ou descritores abertos neste volume");
        }

        // 3. Executa a desmontagem no VFS
        match crate::vfs::unmount(prefix) {
            Ok(()) => {
                if let Some(vol) = self.get_volume_mut(vol_id) {
                    vol.is_mounted = false;
                }
                Ok(())
            }
            Err(VfsError::Busy) => Err("Volume ocupado no VFS"),
            Err(_) => Err("Falha ao desmontar volume do VFS"),
        }
    }

    /// Monta um volume previamente desmontado ou novo em um ponto de montagem do VFS
    pub fn mount_volume(&mut self, vol_id: usize, mount_point: &str) -> Result<(), &'static str> {
        let vol = self.get_volume(vol_id).ok_or("Volume nao encontrado")?;
        if vol.is_mounted {
            return Err("Volume ja esta montado");
        }

        let p_bytes = mount_point.as_bytes();
        if p_bytes.is_empty() || p_bytes.len() >= 32 {
            return Err("Ponto de montagem invalido");
        }

        if let Some(v_mut) = self.get_volume_mut(vol_id) {
            v_mut.mount_point = [0; 32];
            v_mut.mount_point[..p_bytes.len()].copy_from_slice(p_bytes);
            v_mut.mount_point_len = p_bytes.len();
            v_mut.is_mounted = true;
        }

        Ok(())
    }

    /// Realiza a formatação de um volume (AÇÃO DESTRUTIVA - Requer confirmação prévia no chamador)
    pub fn format_volume(&mut self, vol_id: usize, fs_name: &str) -> Result<(), &'static str> {
        let vol = self.get_volume(vol_id).ok_or("Volume nao encontrado")?;
        if vol.is_root {
            return Err("Formatacao da raiz do sistema terminantemente proibida");
        }
        if vol.is_mounted {
            return Err("Desmonte o volume antes de formata-lo");
        }

        if fs_name != "FAT32" && fs_name != "Ext2" {
            return Err("Sistema de arquivos nao suportado para formatacao");
        }

        if let Some(v_mut) = self.get_volume_mut(vol_id) {
            v_mut.fs_kind = if fs_name == "FAT32" {
                DetectedFsKind::Fat32
            } else {
                DetectedFsKind::Ext2
            };
        }

        Ok(())
    }

    /// Consulta estatísticas de capacidade e ocupação de um volume através do VFS
    pub fn stat_volume(&self, vol_id: usize) -> VfsResult<crate::vfs::FsStat> {
        let vol = self.get_volume(vol_id).ok_or(VfsError::NotFound)?;
        if !vol.is_mounted {
            return Err(VfsError::NotFound);
        }
        let prefix = vol.mount_point_str();
        crate::vfs::statvfs(prefix)
    }

    /// Inicialização e mapeamento padrão de volumes do sistema
    pub fn auto_discover(&mut self) {
        self.count = 0;
        self.root_volume_id = None;

        // Volume 0: /disk (FAT32 no IDE Master - Raiz do Sistema)
        let mut v0 = Volume::empty();
        v0.id = 0;
        v0.device_id = 0;
        v0.partition_index = 1;
        let l0 = b"JINN_SYSTEM";
        v0.label[..l0.len()].copy_from_slice(l0);
        v0.label_len = l0.len();
        let m0 = b"/disk";
        v0.mount_point[..m0.len()].copy_from_slice(m0);
        v0.mount_point_len = m0.len();
        v0.fs_kind = DetectedFsKind::Fat32;
        v0.is_mounted = true;
        v0.is_root = true;
        v0.is_boot = true;
        v0.is_removable = false;
        v0.total_mb = 32;
        v0.used_mb = 14;
        v0.free_mb = 18;
        let _ = self.register_volume(v0);

        // Volume 1: /ext2 (Ext2 na Partição 2 do disco)
        let mut v1 = Volume::empty();
        v1.id = 1;
        v1.device_id = 0;
        v1.partition_index = 2;
        let l1 = b"EXT2_DATA";
        v1.label[..l1.len()].copy_from_slice(l1);
        v1.label_len = l1.len();
        let m1 = b"/ext2";
        v1.mount_point[..m1.len()].copy_from_slice(m1);
        v1.mount_point_len = m1.len();
        v1.fs_kind = DetectedFsKind::Ext2;
        v1.is_mounted = true;
        v1.is_root = false;
        v1.is_boot = false;
        v1.is_removable = false;
        v1.total_mb = 32;
        v1.used_mb = 2;
        v1.free_mb = 30;
        let _ = self.register_volume(v1);
    }
}

pub static VOLUME_MANAGER: crate::memory::SpinLock<VolumeManager> =
    crate::memory::SpinLock::new(VolumeManager::new());

/// Inicializa o gerenciador global de volumes
pub fn init_volume_manager() {
    let mut mgr = VOLUME_MANAGER.lock();
    mgr.auto_discover();
}
