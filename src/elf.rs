/// Jinn OS — ELF64 Parser & Loader Seguro
///
/// Implementa a validação estrita e o carregamento de executáveis ELF 64-bit
/// para o espaço de endereçamento isolado do usuário (Ring 3), mapeando
/// exclusivamente os segmentos PT_LOAD com permissões restritas (W^X).

extern crate alloc;

use crate::memory::paging;

// ── Constantes do Formato ELF64 ───────────────────────────────────────────────

const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1; // Little Endian
const EV_CURRENT: u32 = 1;
const ET_EXEC: u16 = 2; // Executável estático
const EM_X86_64: u16 = 62; // AMD x86-64

pub const PT_LOAD: u32 = 1;

// Segment Flags
pub const PF_X: u32 = 1 << 0; // Executável
pub const PF_W: u32 = 1 << 1; // Gravável
pub const PF_R: u32 = 1 << 2; // Legível

/// Limites do espaço de endereçamento canônico de usuário em x86-64
pub const USER_SPACE_MIN: u64 = 0x0000_0000_0001_0000; // Guarda contra NULL pointer (64 KiB)
pub const USER_SPACE_MAX: u64 = 0x0000_8000_0000_0000; // Metade canônica inferior (48-bit)
const MAX_PROGRAM_HEADERS: usize = 64;

// ── Estruturas de Cabeçalho ELF64 ─────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
struct Elf64_Ehdr {
    e_ident:     [u8; 16],
    e_type:      u16,
    e_machine:   u16,
    e_version:   u32,
    e_entry:     u64,
    e_phoff:     u64,
    e_shoff:     u64,
    e_flags:     u32,
    e_ehsize:    u16,
    e_phentsize: u16,
    e_phnum:     u16,
    e_shentsize: u16,
    e_shnum:     u16,
    e_shstrndx:  u16,
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
struct Elf64_Phdr {
    p_type:   u32,
    p_flags:  u32,
    p_offset: u64,
    p_vaddr:  u64,
    p_paddr:  u64,
    p_filesz: u64,
    p_memsz:  u64,
    p_align:  u64,
}

// ── Erros de Carregamento ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfError {
    BufferTooSmall,
    InvalidMagic,
    Not64Bit,
    NotLittleEndian,
    InvalidVersion,
    NotExecutable,
    WrongArchitecture,
    InvalidHeaderSize,
    InvalidProgramHeaders,
    InvalidVirtualAddress,
    SegmentTruncated,
    FileSizeExceedsMemSize,
    ArithmeticOverflow,
    OutOfMemory,
    VfsError,
    SecurityPolicyViolation,
}

/// Metadados verificados de um executável ELF64 válido
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElfInfo {
    pub entry_point: u64,
    pub phnum: usize,
    pub load_segments: usize,
}

// ── API Pública ───────────────────────────────────────────────────────────────

/// Valida estritamente a conformidade de um binário ELF64 para execução em Ring 3.
/// Não realiza alocação de memória nem depende de tabelas de páginas ativas.
pub fn validate_elf(elf_data: &[u8]) -> Result<ElfInfo, ElfError> {
    if elf_data.len() < core::mem::size_of::<Elf64_Ehdr>() {
        return Err(ElfError::BufferTooSmall);
    }

    let ehdr_ptr = elf_data.as_ptr() as *const Elf64_Ehdr;
    let ehdr = unsafe { &*ehdr_ptr };

    // 1. Validação estrita do cabeçalho principal
    if ehdr.e_ident[0..4] != ELF_MAGIC {
        return Err(ElfError::InvalidMagic);
    }
    if ehdr.e_ident[4] != ELFCLASS64 {
        return Err(ElfError::Not64Bit);
    }
    if ehdr.e_ident[5] != ELFDATA2LSB {
        return Err(ElfError::NotLittleEndian);
    }
    if ehdr.e_ident[6] != 1 || ehdr.e_version != EV_CURRENT {
        return Err(ElfError::InvalidVersion);
    }
    if ehdr.e_type != ET_EXEC {
        return Err(ElfError::NotExecutable);
    }
    if ehdr.e_machine != EM_X86_64 {
        return Err(ElfError::WrongArchitecture);
    }
    if (ehdr.e_ehsize as usize) < core::mem::size_of::<Elf64_Ehdr>() {
        return Err(ElfError::InvalidHeaderSize);
    }
    if (ehdr.e_phentsize as usize) < core::mem::size_of::<Elf64_Phdr>() {
        return Err(ElfError::InvalidHeaderSize);
    }

    // 2. Validação do ponto de entrada (deve estar estritamente no espaço canônico de usuário)
    if ehdr.e_entry < USER_SPACE_MIN || ehdr.e_entry >= USER_SPACE_MAX {
        return Err(ElfError::InvalidVirtualAddress);
    }

    // 3. Validação dos Program Headers
    let phoff = ehdr.e_phoff as usize;
    let phnum = ehdr.e_phnum as usize;
    let phentsize = ehdr.e_phentsize as usize;

    if phnum == 0 || phnum > MAX_PROGRAM_HEADERS {
        return Err(ElfError::InvalidProgramHeaders);
    }

    let ph_total_bytes = phnum.checked_mul(phentsize).ok_or(ElfError::ArithmeticOverflow)?;
    let ph_end = phoff.checked_add(ph_total_bytes).ok_or(ElfError::ArithmeticOverflow)?;
    if ph_end > elf_data.len() {
        return Err(ElfError::SegmentTruncated);
    }

    let mut load_segments_count = 0;

    // 4. Verificação estrita de cada segmento PT_LOAD
    for i in 0..phnum {
        let phdr_offset = phoff + (i * phentsize);
        let phdr_ptr = unsafe {
            elf_data.as_ptr().add(phdr_offset) as *const Elf64_Phdr
        };
        let phdr = unsafe { &*phdr_ptr };

        if phdr.p_type == PT_LOAD {
            if phdr.p_memsz == 0 {
                continue;
            }
            load_segments_count += 1;

            // Validações de tamanho do segmento
            if phdr.p_filesz > phdr.p_memsz {
                return Err(ElfError::FileSizeExceedsMemSize);
            }

            let file_end = phdr.p_offset.checked_add(phdr.p_filesz).ok_or(ElfError::ArithmeticOverflow)?;
            if file_end > elf_data.len() as u64 {
                return Err(ElfError::SegmentTruncated);
            }

            let mem_end = phdr.p_vaddr.checked_add(phdr.p_memsz).ok_or(ElfError::ArithmeticOverflow)?;
            if phdr.p_vaddr < USER_SPACE_MIN || mem_end > USER_SPACE_MAX {
                return Err(ElfError::InvalidVirtualAddress);
            }

            // Política de Segurança W^X: segmento nunca pode ser W (gravável) e X (executável) simultaneamente
            let is_writable = (phdr.p_flags & PF_W) != 0;
            let is_exec = (phdr.p_flags & PF_X) != 0;
            if is_writable && is_exec {
                return Err(ElfError::SecurityPolicyViolation);
            }
        }
    }

    if load_segments_count == 0 {
        return Err(ElfError::InvalidProgramHeaders);
    }

    Ok(ElfInfo {
        entry_point: ehdr.e_entry,
        phnum,
        load_segments: load_segments_count,
    })
}

/// Analisa e carrega um executável ELF64 em memória na tabela de páginas (`pml4_phys`).
/// Retorna o endereço virtual do ponto de entrada (`e_entry`).
pub fn load_elf(elf_data: &[u8], pml4_phys: u64) -> Result<u64, ElfError> {
    // 1. Validação estrita inicial
    let info = validate_elf(elf_data)?;

    let ehdr_ptr = elf_data.as_ptr() as *const Elf64_Ehdr;
    let ehdr = unsafe { &*ehdr_ptr };
    let phoff = ehdr.e_phoff as usize;
    let phnum = ehdr.e_phnum as usize;
    let phentsize = ehdr.e_phentsize as usize;

    let pml4 = paging::phys_to_virt(pml4_phys) as *mut paging::PageTable;

    // 2. Carregamento e mapeamento físico dos segmentos PT_LOAD
    for i in 0..phnum {
        let phdr_offset = phoff + (i * phentsize);
        let phdr_ptr = unsafe {
            elf_data.as_ptr().add(phdr_offset) as *const Elf64_Phdr
        };
        let phdr = unsafe { &*phdr_ptr };

        if phdr.p_type == PT_LOAD {
            if phdr.p_memsz == 0 {
                continue;
            }

            let mem_end = match phdr.p_vaddr.checked_add(phdr.p_memsz) {
                Some(e) => e,
                None => return Err(ElfError::ArithmeticOverflow),
            };

            // Define permissões de página (Política W^X confirmada)
            let mut page_flags = paging::PRESENT | paging::USER;
            let is_writable = (phdr.p_flags & PF_W) != 0;
            let is_exec = (phdr.p_flags & PF_X) != 0;

            if is_writable {
                page_flags |= paging::WRITABLE;
            }
            if !is_exec {
                page_flags |= paging::NO_EXEC;
            }

            // Alinhamento aos limites de página (4 KiB)
            let start_page = phdr.p_vaddr & !0xFFF;
            let end_page = (mem_end.checked_add(0xFFF).ok_or(ElfError::ArithmeticOverflow)?) & !0xFFF;
            let num_pages = (end_page - start_page) / 4096;

            for p in 0..num_pages {
                let current_vaddr = start_page + (p * 4096);

                // Verifica se a página já foi mapeada por outro segmento
                let (phys_frame, is_new_page) = unsafe {
                    if let Some((existing_phys, _)) = paging::lookup_page(pml4, current_vaddr) {
                        (existing_phys, false)
                    } else {
                        let new_frame = crate::memory::alloc_frame().ok_or(ElfError::OutOfMemory)?;
                        paging::map_page(pml4, current_vaddr, new_frame, page_flags);
                        (new_frame, true)
                    }
                };

                let frame_virt = paging::phys_to_virt(phys_frame) as *mut u8;

                // Se a página é nova, zera seu conteúdo completamente (BSS e segurança)
                if is_new_page {
                    unsafe {
                        core::ptr::write_bytes(frame_virt, 0, 4096);
                    }
                }

                // Determina quais bytes do arquivo pertencem a esta página
                let page_offset = if current_vaddr < phdr.p_vaddr {
                    phdr.p_vaddr - current_vaddr
                } else {
                    0
                };

                let file_data_start = if current_vaddr < phdr.p_vaddr {
                    0
                } else {
                    current_vaddr - phdr.p_vaddr
                };

                if file_data_start < phdr.p_filesz {
                    let copy_len = core::cmp::min(
                        4096 - page_offset as usize,
                        (phdr.p_filesz - file_data_start) as usize,
                    );

                    let src = unsafe {
                        elf_data.as_ptr().add((phdr.p_offset + file_data_start) as usize)
                    };
                    let dst = unsafe { frame_virt.add(page_offset as usize) };

                    unsafe {
                        core::ptr::copy_nonoverlapping(src, dst, copy_len);
                    }
                }
            }
        }
    }

    Ok(info.entry_point)
}

/// Carrega um executável ELF64 a partir do Virtual Filesystem (VFS) para o PML4 do processo.
///
/// Suporta arquivos em partições FAT32 (`/disk/...`) e Ext2 (`/ext2/...`).
pub fn load_elf_from_vfs(path: &str, pml4_phys: u64) -> Result<u64, ElfError> {
    let stat = crate::vfs::stat(path).map_err(|_| ElfError::VfsError)?;
    if stat.node_type != crate::vfs::NodeType::File {
        return Err(ElfError::VfsError);
    }

    let size = stat.size as usize;
    if size < core::mem::size_of::<Elf64_Ehdr>() || size > 16 * 1024 * 1024 {
        return Err(ElfError::BufferTooSmall);
    }

    // Aloca buffer temporário no heap do kernel para leitura do binário
    let mut elf_buf = alloc::vec![0u8; size];
    let n = crate::vfs::read_file(path, &mut elf_buf).map_err(|_| ElfError::VfsError)?;
    if n != size {
        return Err(ElfError::SegmentTruncated);
    }

    load_elf(&elf_buf, pml4_phys)
}
