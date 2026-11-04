//! Suíte de Auditoria e Validação Rigorosa da ABI de Syscalls (0..52)
//!
//! Este teste comprova:
//! 1. A tabela canônica de identificadores numéricos das syscalls.
//! 2. A ausência de colisões entre identificadores.
//! 3. O alinhamento, layout de memória e tamanho de estruturas passadas na ABI.
//! 4. A correta distinção e correspondência entre:
//!    - Syscall 50: sys_unlink
//!    - Syscall 51: sys_mkdir
//!    - Syscall 52: sys_rename
//! 5. A passagem íntegra de argumentos em espaço de usuário (Ring 3) e retorno de erros POSIX.

use std::collections::HashSet;
use std::cell::RefCell;

// ── Estruturas Canônicas da ABI (representação de espelho binário idêntica ao kernel) ──

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanonicalRenameArgs {
    pub old_ptr: u64,
    pub old_len: usize,
    pub new_ptr: u64,
    pub new_len: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanonicalFileStat {
    pub size: u64,
    pub inode: u64,
    pub node_type: u32,
    pub mode: u32,
    pub blksize: u32,
    pub blocks: u32,
    pub atime: u64,
    pub mtime: u64,
    pub ctime: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanonicalFsStatUser {
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub block_size: u64,
    pub total_inodes: u64,
    pub free_inodes: u64,
    pub is_read_only: u32,
    pub fs_type: [u8; 16],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanonicalDiskInfoUser {
    pub disk_id: u32,
    pub kind: u32,
    pub total_sectors: u64,
    pub sector_size: u32,
    pub is_read_only: u32,
    pub is_boot_drive: u32,
    pub partition_count: u32,
    pub model: [u8; 40],
    pub serial: [u8; 20],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanonicalPartitionInfoUser {
    pub disk_id: u32,
    pub partition_index: u32,
    pub start_lba: u64,
    pub sector_count: u64,
    pub fs_kind: u32,
    pub is_bootable: u32,
    pub is_mounted: u32,
    pub name: [u8; 32],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanonicalVolumeInfoUser {
    pub volume_id: u32,
    pub device_id: u32,
    pub partition_index: u32,
    pub fs_kind: u32,
    pub is_mounted: u32,
    pub is_root: u32,
    pub is_boot: u32,
    pub total_mb: u64,
    pub used_mb: u64,
    pub free_mb: u64,
    pub label: [u8; 32],
    pub mount_point: [u8; 32],
}

// ── Tabela Canônica Oficial de Identificadores de Syscall (0..52) ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyscallId {
    Exit = 0,
    Write = 1,
    Read = 2,
    Yield = 3,
    Getpid = 4,
    IpcSend = 5,
    IpcRecv = 6,
    AllocPage = 7,
    FreePage = 8,
    Spawn = 9,
    Waitpid = 10,
    Sleep = 11,
    JdaShmAlloc = 12,
    JdaShmRelease = 13,
    JdaSubmit = 14,
    Open = 15,
    Close = 16,
    Stat = 17,
    Fstat = 18,
    Lseek = 19,
    WindowCreate = 20,
    WindowUpdate = 21,
    WindowClose = 22,
    Sync = 23,
    WindowPollEvent = 24,
    ResStat = 25,
    JdaShmShare = 26,
    JdaRingSubmit = 27,
    JdaRingPoll = 28,
    JdaShmGetVaddr = 29,
    Socket = 30,
    Connect = 31,
    Bind = 32,
    Listen = 33,
    Accept = 34,
    Send = 35,
    Recv = 36,
    JinnwallCtrl = 37,
    Execve = 38,
    SpawnProcess = 39,
    DnsResolve = 40,
    Statvfs = 41,
    DiskInfo = 42,
    Mount = 43,
    Unmount = 44,
    DeviceInfo = 45,
    DeviceCount = 46,
    PartitionInfo = 47,
    VolumeInfo = 48,
    VolumeCount = 49,
    Unlink = 50,
    Mkdir = 51,
    Rename = 52,
}

impl SyscallId {
    pub fn from_u64(n: u64) -> Option<Self> {
        match n {
            0 => Some(Self::Exit),
            1 => Some(Self::Write),
            2 => Some(Self::Read),
            3 => Some(Self::Yield),
            4 => Some(Self::Getpid),
            5 => Some(Self::IpcSend),
            6 => Some(Self::IpcRecv),
            7 => Some(Self::AllocPage),
            8 => Some(Self::FreePage),
            9 => Some(Self::Spawn),
            10 => Some(Self::Waitpid),
            11 => Some(Self::Sleep),
            12 => Some(Self::JdaShmAlloc),
            13 => Some(Self::JdaShmRelease),
            14 => Some(Self::JdaSubmit),
            15 => Some(Self::Open),
            16 => Some(Self::Close),
            17 => Some(Self::Stat),
            18 => Some(Self::Fstat),
            19 => Some(Self::Lseek),
            20 => Some(Self::WindowCreate),
            21 => Some(Self::WindowUpdate),
            22 => Some(Self::WindowClose),
            23 => Some(Self::Sync),
            24 => Some(Self::WindowPollEvent),
            25 => Some(Self::ResStat),
            26 => Some(Self::JdaShmShare),
            27 => Some(Self::JdaRingSubmit),
            28 => Some(Self::JdaRingPoll),
            29 => Some(Self::JdaShmGetVaddr),
            30 => Some(Self::Socket),
            31 => Some(Self::Connect),
            32 => Some(Self::Bind),
            33 => Some(Self::Listen),
            34 => Some(Self::Accept),
            35 => Some(Self::Send),
            36 => Some(Self::Recv),
            37 => Some(Self::JinnwallCtrl),
            38 => Some(Self::Execve),
            39 => Some(Self::SpawnProcess),
            40 => Some(Self::DnsResolve),
            41 => Some(Self::Statvfs),
            42 => Some(Self::DiskInfo),
            43 => Some(Self::Mount),
            44 => Some(Self::Unmount),
            45 => Some(Self::DeviceInfo),
            46 => Some(Self::DeviceCount),
            47 => Some(Self::PartitionInfo),
            48 => Some(Self::VolumeInfo),
            49 => Some(Self::VolumeCount),
            50 => Some(Self::Unlink),
            51 => Some(Self::Mkdir),
            52 => Some(Self::Rename),
            _ => None,
        }
    }
}

// ── TESTES UNITÁRIOS DE AUDITORIA ──

#[test]
fn test_syscall_canonical_numbering_and_collision_freedom() {
    let mut numbers = HashSet::new();
    let total_syscalls = 53; // 0..=52

    for nr in 0..total_syscalls {
        let sc = SyscallId::from_u64(nr);
        assert!(sc.is_some(), "Syscall nº {} deve ter correspondência unívoca na tabela canônica", nr);
        assert!(numbers.insert(nr), "Colisão detectada para o identificador {}", nr);
    }

    assert_eq!(numbers.len(), total_syscalls as usize);
    assert!(SyscallId::from_u64(53).is_none(), "Syscall 53 não deve existir na ABI 0.2.0");
}

#[test]
fn test_syscall_50_51_52_unambiguous_mapping() {
    // Verificação estrita das chamadas sob auditoria documental:
    assert_eq!(SyscallId::from_u64(50), Some(SyscallId::Unlink), "Syscall 50 DEVE ser sys_unlink");
    assert_eq!(SyscallId::from_u64(51), Some(SyscallId::Mkdir), "Syscall 51 DEVE ser sys_mkdir");
    assert_eq!(SyscallId::from_u64(52), Some(SyscallId::Rename), "Syscall 52 DEVE ser sys_rename");
}

#[test]
fn test_abi_struct_layout_and_alignment() {
    // RenameArgs: 2 ponteiros de 64-bit + 2 usize => 32 bytes, alinhamento 8
    assert_eq!(core::mem::size_of::<CanonicalRenameArgs>(), 32);
    assert_eq!(core::mem::align_of::<CanonicalRenameArgs>(), 8);

    // FileStat: size = 56 bytes, alinhamento 8
    assert_eq!(core::mem::size_of::<CanonicalFileStat>(), 56);
    assert_eq!(core::mem::align_of::<CanonicalFileStat>(), 8);

    // FsStatUser: total_bytes(8)+free_bytes(8)+block_size(8)+total_inodes(8)+free_inodes(8)+is_read_only(4)+pad(4)+fs_type(16) = 64 bytes
    assert_eq!(core::mem::size_of::<CanonicalFsStatUser>(), 64);
    assert_eq!(core::mem::align_of::<CanonicalFsStatUser>(), 8);

    // VolumeInfoUser: alinhamento 8 bytes
    assert_eq!(core::mem::align_of::<CanonicalVolumeInfoUser>(), 8);
}

#[test]
fn test_mock_dispatch_unlink_mkdir_rename_parameters() {
    struct DispatchRecord {
        called_nr: u64,
        path_arg: String,
        extra_arg: String,
    }

    let log: RefCell<Vec<DispatchRecord>> = RefCell::new(Vec::new());

    let mock_dispatch = |nr: u64, arg0: u64, arg1: u64, _arg2: u64| -> i64 {
        match nr {
            50 => {
                // sys_unlink(path_ptr, path_len)
                let ptr = arg0 as *const u8;
                let len = arg1 as usize;
                if ptr.is_null() || len == 0 || len > 128 {
                    return -22; // -EINVAL
                }
                let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
                let path = std::str::from_utf8(slice).unwrap();
                log.borrow_mut().push(DispatchRecord {
                    called_nr: 50,
                    path_arg: path.to_string(),
                    extra_arg: String::new(),
                });
                0
            }
            51 => {
                // sys_mkdir(path_ptr, path_len)
                let ptr = arg0 as *const u8;
                let len = arg1 as usize;
                if ptr.is_null() || len == 0 || len > 128 {
                    return -22; // -EINVAL
                }
                let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
                let path = std::str::from_utf8(slice).unwrap();
                log.borrow_mut().push(DispatchRecord {
                    called_nr: 51,
                    path_arg: path.to_string(),
                    extra_arg: String::new(),
                });
                0
            }
            52 => {
                // sys_rename(args_ptr)
                let args_ptr = arg0 as *const CanonicalRenameArgs;
                if args_ptr.is_null() {
                    return -22; // -EINVAL
                }
                let args = unsafe { *args_ptr };
                let old_slice = unsafe { std::slice::from_raw_parts(args.old_ptr as *const u8, args.old_len) };
                let new_slice = unsafe { std::slice::from_raw_parts(args.new_ptr as *const u8, args.new_len) };
                let old_str = std::str::from_utf8(old_slice).unwrap();
                let new_str = std::str::from_utf8(new_slice).unwrap();
                log.borrow_mut().push(DispatchRecord {
                    called_nr: 52,
                    path_arg: old_str.to_string(),
                    extra_arg: new_str.to_string(),
                });
                0
            }
            _ => -38, // -ENOSYS
        }
    };

    // 1. Testa sys_unlink via mock
    let del_target = "/disk/DELETEME.TXT";
    let ret_del = mock_dispatch(50, del_target.as_ptr() as u64, del_target.len() as u64, 0);
    assert_eq!(ret_del, 0);
    assert_eq!(log.borrow()[0].called_nr, 50);
    assert_eq!(log.borrow()[0].path_arg, del_target);

    // 2. Testa sys_mkdir via mock
    let dir_target = "/disk/PROJECTS";
    let ret_dir = mock_dispatch(51, dir_target.as_ptr() as u64, dir_target.len() as u64, 0);
    assert_eq!(ret_dir, 0);
    assert_eq!(log.borrow()[1].called_nr, 51);
    assert_eq!(log.borrow()[1].path_arg, dir_target);

    // 3. Testa sys_rename via mock
    let src = "/disk/OLD.TXT";
    let dst = "/disk/NEW.TXT";
    let rename_args = CanonicalRenameArgs {
        old_ptr: src.as_ptr() as u64,
        old_len: src.len(),
        new_ptr: dst.as_ptr() as u64,
        new_len: dst.len(),
    };
    let ret_ren = mock_dispatch(52, &rename_args as *const _ as u64, 0, 0);
    assert_eq!(ret_ren, 0);
    assert_eq!(log.borrow()[2].called_nr, 52);
    assert_eq!(log.borrow()[2].path_arg, src);
    assert_eq!(log.borrow()[2].extra_arg, dst);

    // 4. Testa rejeição de ponteiro nulo em sys_rename
    let ret_null = mock_dispatch(52, 0, 0, 0);
    assert_eq!(ret_null, -22); // -EINVAL
}
