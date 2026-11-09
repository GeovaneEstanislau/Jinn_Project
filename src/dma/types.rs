//! Jinn OS — Kernel DMA Manager (KDM)
//! Tipos Fundamentais, Códigos de Erro e Constantes de Barramento (Fase 1)

/// Limite de teto de endereçamento físico de 32 bits (4 GiB).
/// Obrigatório para compatibilidade com dispositivos PCI legados (VirtIO legacy, Intel e1000, AMD PCnet).
pub const DMA_32BIT_MAX_ADDR: u64 = 0xFFFF_FFFF;

/// Tamanho padrão da página física no x86_64 (4 KiB)
pub const PAGE_SIZE: usize = 4096;

/// Capacidade máxima global de buffers DMA gerenciados simultaneamente
pub const MAX_DMA_BUFFERS: usize = 64;

/// Capacidade máxima de pools de DMA simultâneos (por dispositivo/processo)
pub const MAX_DMA_POOLS: usize = 16;

/// Limite máximo de páginas mantidas em quarentena permanente
pub const MAX_QUARANTINED_FRAMES: usize = 128;

/// Cota padrão máxima de páginas DMA alocáveis por processo de driver (128 KiB = 32 páginas)
pub const DEFAULT_MAX_DMA_PAGES_PER_PROCESS: usize = 32;

/// Direção do fluxo de dados DMA do ponto de vista do dispositivo de hardware
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmaDirection {
    /// O dispositivo de rede lê da memória do host (Transmissão - TX)
    DeviceReadsMemory,
    /// O dispositivo de rede escreve na memória do host (Recepção - RX)
    DeviceWritesMemory,
    /// Bidirecional (reservado para anéis de controle específicos)
    Bidirectional,
}

/// Backend de isolamento de DMA ativo para o dispositivo
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmaBackend {
    /// Isolamento por domínios de hardware via IOMMU (Intel VT-d / AMD-Vi)
    Iommu,
    /// Isolamento mediado por buffers intermediários pertencentes ao Kernel (Ring 0)
    KernelBounce,
}

/// Códigos de erro formais do subsistema Kernel DMA Manager
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmaError {
    InvalidSize,
    InvalidAlignment,
    InvalidHandle,
    InvalidToken,
    PermissionDenied,
    OutOfMemory,
    OutOfBounds,
    InvalidDirection,
    InvalidState,
    DeviceBusy,
    DeviceUnavailable,
    MappingFailed,
    Timeout,
    DeviceResetFailed,
    UnsupportedPlatform,
    ArithmeticOverflow,
    QuarantinedMemory,
}

impl DmaError {
    /// Tradução para códigos de erro padrão compatíveis com as convenções POSIX do Jinn Microkernel
    pub const fn to_errno(&self) -> isize {
        match self {
            Self::InvalidSize | Self::InvalidAlignment => -22,      // EINVAL
            Self::InvalidHandle | Self::InvalidToken => -3,          // ESRCH (Handle/Token inexistente ou obsoleto)
            Self::PermissionDenied => -1,                           // EPERM
            Self::OutOfMemory => -12,                               // ENOMEM
            Self::OutOfBounds | Self::ArithmeticOverflow => -14,     // EFAULT
            Self::InvalidDirection | Self::InvalidState => -22,      // EINVAL
            Self::DeviceBusy => -16,                                // EBUSY
            Self::DeviceUnavailable => -19,                         // ENODEV
            Self::MappingFailed => -14,                             // EFAULT
            Self::Timeout => -110,                                  // ETIMEDOUT
            Self::DeviceResetFailed => -5,                          // EIO
            Self::UnsupportedPlatform => -38,                       // ENOSYS
            Self::QuarantinedMemory => -16,                         // EBUSY (Recurso bloqueado em quarentena)
        }
    }
}

