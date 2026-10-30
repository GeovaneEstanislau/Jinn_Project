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
    OverlappingSegments,
    EntryNotInExecutableSegment,
    FileNotFound,
    NotAFile,
    FileTooLarge,
    AlignmentError,
    ArgumentTooLong,
    TooManyArguments,
}

impl ElfError {
    pub fn to_errno(self) -> isize {
        match self {
            ElfError::FileNotFound => -2,  // ENOENT
            ElfError::NotAFile => -13,     // EACCES
            ElfError::OutOfMemory => -12,  // ENOMEM
            ElfError::FileTooLarge => -27, // EFBIG
            ElfError::ArgumentTooLong | ElfError::TooManyArguments => -7, // E2BIG
            ElfError::VfsError => -5,      // EIO
            _ => -8,                       // ENOEXEC (exec format error)
        }
    }
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
    let mut load_segs = [(0u64, 0u64, false, false); MAX_PROGRAM_HEADERS];

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

            // Validação de alinhamento do segmento
            if phdr.p_align > 1 {
                if !phdr.p_align.is_power_of_two() {
                    return Err(ElfError::AlignmentError);
                }
                if (phdr.p_vaddr % phdr.p_align) != (phdr.p_offset % phdr.p_align) {
                    return Err(ElfError::AlignmentError);
                }
            }

            // Política de Segurança W^X: segmento nunca pode ser W (gravável) e X (executável) simultaneamente
            let is_writable = (phdr.p_flags & PF_W) != 0;
            let is_exec = (phdr.p_flags & PF_X) != 0;
            if is_writable && is_exec {
                return Err(ElfError::SecurityPolicyViolation);
            }

            if load_segments_count < MAX_PROGRAM_HEADERS {
                load_segs[load_segments_count] = (phdr.p_vaddr, mem_end, is_writable, is_exec);
                load_segments_count += 1;
            } else {
                return Err(ElfError::InvalidProgramHeaders);
            }
        }
    }

    if load_segments_count == 0 {
        return Err(ElfError::InvalidProgramHeaders);
    }

    // 5. Validação de Não-Sobreposição e Política W^X entre Segmentos PT_LOAD
    for i in 0..load_segments_count {
        let (start_a, end_a, is_writable_a, is_exec_a) = load_segs[i];
        for j in (i + 1)..load_segments_count {
            let (start_b, end_b, is_writable_b, is_exec_b) = load_segs[j];

            // 5a. Verificação de sobreposição de faixas virtuais
            if start_a < end_b && start_b < end_a {
                return Err(ElfError::OverlappingSegments);
            }

            // 5b. Verificação de compartilhamento de páginas de 4 KiB com permissões conflitantes (W^X)
            let page_start_a = start_a & !0xFFF;
            let page_end_a = match end_a.checked_add(0xFFF) {
                Some(e) => e & !0xFFF,
                None => return Err(ElfError::ArithmeticOverflow),
            };
            let page_start_b = start_b & !0xFFF;
            let page_end_b = match end_b.checked_add(0xFFF) {
                Some(e) => e & !0xFFF,
                None => return Err(ElfError::ArithmeticOverflow),
            };

            if page_start_a < page_end_b && page_start_b < page_end_a {
                // Os segmentos compartilham ao menos uma página de 4 KiB
                // Rejeita terminantemente se um segmento for gravável e o outro for executável
                if (is_writable_a && is_exec_b) || (is_exec_a && is_writable_b) {
                    return Err(ElfError::SecurityPolicyViolation);
                }
            }
        }
    }

    // 6. Validação de que o ponto de entrada reside em segmento executável válido
    let mut entry_in_exec = false;
    for &(start, end, _, is_exec) in &load_segs[..load_segments_count] {
        if is_exec && ehdr.e_entry >= start && ehdr.e_entry < end {
            entry_in_exec = true;
            break;
        }
    }
    if !entry_in_exec {
        return Err(ElfError::EntryNotInExecutableSegment);
    }

    Ok(ElfInfo {
        entry_point: ehdr.e_entry,
        phnum,
        load_segments: load_segments_count,
    })
}

/// Analisa e carrega um executável ELF64 em memória na tabela de páginas (`pml4_phys`).
/// Retorna uma tupla com o endereço virtual do ponto de entrada (`e_entry`) e a contagem de frames físicos alocados.
pub fn load_elf(elf_data: &[u8], pml4_phys: u64) -> Result<(u64, usize), ElfError> {
    // 1. Validação estrita inicial
    let info = validate_elf(elf_data)?;

    let ehdr_ptr = elf_data.as_ptr() as *const Elf64_Ehdr;
    let ehdr = unsafe { &*ehdr_ptr };
    let phoff = ehdr.e_phoff as usize;
    let phnum = ehdr.e_phnum as usize;
    let phentsize = ehdr.e_phentsize as usize;

    let pml4 = paging::phys_to_virt(pml4_phys) as *mut paging::PageTable;
    let mut total_allocated_frames = 0usize;

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
                    if let Some((existing_phys, existing_flags)) = paging::lookup_page(pml4, current_vaddr) {
                        let mut merged_flags = existing_flags;
                        if is_exec {
                            merged_flags &= !paging::NO_EXEC;
                        }
                        if is_writable {
                            merged_flags |= paging::WRITABLE;
                        }
                        // Defesa em profundidade W^X: uma página nunca pode ser gravável e executável
                        if (merged_flags & paging::WRITABLE != 0) && (merged_flags & paging::NO_EXEC == 0) {
                            return Err(ElfError::SecurityPolicyViolation);
                        }
                        if merged_flags != existing_flags {
                            paging::set_page_flags(pml4, current_vaddr, merged_flags);
                        }
                        (existing_phys, false)
                    } else {
                        let new_frame = crate::memory::alloc_frame().ok_or(ElfError::OutOfMemory)?;
                        paging::map_page(pml4, current_vaddr, new_frame, page_flags);
                        (new_frame, true)
                    }
                };

                let frame_virt = paging::phys_to_virt(phys_frame) as *mut u8;

                // Se a página é nova, inicializa seu conteúdo:
                // Páginas executáveis são preenchidas com 0xCC (INT3 trap) para evitar execução
                // descontrolada em áreas de padding. Páginas de dados são zeradas para BSS.
                if is_new_page {
                    total_allocated_frames += 1;
                    let fill_byte = if is_exec { 0xCC } else { 0x00 };
                    unsafe {
                        core::ptr::write_bytes(frame_virt, fill_byte, 4096);
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

    Ok((info.entry_point, total_allocated_frames))
}

/// Carrega um executável ELF64 a partir do Virtual Filesystem (VFS) para o PML4 do processo.
///
/// Suporta arquivos em partições FAT32 (`/disk/...`) e Ext2 (`/ext2/...`).
pub fn load_elf_from_vfs(path: &str, pml4_phys: u64) -> Result<(u64, usize), ElfError> {
    let stat = match crate::vfs::stat(path) {
        Ok(s) => s,
        Err(crate::vfs::VfsError::NotFound) => return Err(ElfError::FileNotFound),
        Err(_) => return Err(ElfError::VfsError),
    };
    if stat.node_type != crate::vfs::NodeType::File {
        return Err(ElfError::NotAFile);
    }

    let size = stat.size as usize;
    if size < core::mem::size_of::<Elf64_Ehdr>() {
        return Err(ElfError::BufferTooSmall);
    }
    if size > 16 * 1024 * 1024 {
        return Err(ElfError::FileTooLarge);
    }

    // Aloca buffer temporário no heap do kernel para leitura do binário
    let mut elf_buf = alloc::vec![0u8; size];
    let n = crate::vfs::read_file(path, &mut elf_buf).map_err(|e| match e {
        crate::vfs::VfsError::NotFound => ElfError::FileNotFound,
        _ => ElfError::VfsError,
    })?;
    if n != size {
        return Err(ElfError::SegmentTruncated);
    }

    load_elf(&elf_buf, pml4_phys)
}

/// Aloca e inicializa a pilha de usuário com argumentos (`argv`) e variáveis de ambiente (`envp`).
///
/// Implementa a convenção x86-64 System V ABI:
/// - Strings copiadas no topo da página de pilha.
/// - Tabela com ponteiros `envp`, terminador NULL, ponteiros `argv`, terminador NULL, e `argc`.
/// - Alinhamento do RSP a 16 bytes.
///
/// Retorna `(user_rsp, argc, argv_ptr, envp_ptr, frames_allocated)`.
pub fn setup_user_stack_with_args(
    pml4_phys: u64,
    task_id: usize,
    argv: &[&str],
    envp: &[&str],
) -> Result<(usize, usize, usize, usize, usize), ElfError> {
    const USTACK_SIZE: usize = 262144; // 256 KiB
    const USER_STACK_BASE: u64 = 0x0000_7FFF_F000_0000;

    if argv.len() > 32 || envp.len() > 32 {
        return Err(ElfError::TooManyArguments);
    }

    let stack_top_virt  = USER_STACK_BASE - (task_id as u64 * USTACK_SIZE as u64);
    let stack_base_virt = stack_top_virt - USTACK_SIZE as u64;

    let pml4 = paging::phys_to_virt(pml4_phys) as *mut paging::PageTable;
    let pages = USTACK_SIZE / 4096; // 64 pages

    let mut frames = alloc::vec::Vec::with_capacity(pages);
    for i in 0..pages {
        let virt = stack_base_virt + (i as u64 * 4096);
        let phys = match crate::memory::alloc_frame() {
            Some(f) => f,
            None => {
                return Err(ElfError::OutOfMemory);
            }
        };
        frames.push(phys);
        unsafe {
            paging::map_page(pml4, virt, phys, paging::USER_RW);
            // Zera o frame para segurança total contra vazamento de dados
            let frame_virt = paging::phys_to_virt(phys) as *mut u8;
            core::ptr::write_bytes(frame_virt, 0, 4096);
        }
    }

    // A página de topo é frames[pages - 1]
    let top_frame_phys = frames[pages - 1];
    let top_frame_virt = paging::phys_to_virt(top_frame_phys) as *mut u8;

    let mut page_offset = 4096usize;

    // 1. Escreve strings de envp em ordem reversa
    let mut envp_ptrs = alloc::vec::Vec::with_capacity(envp.len());
    for s in envp.iter().rev() {
        let bytes = s.as_bytes();
        let len = bytes.len() + 1; // +1 para \0
        if page_offset < len + 256 {
            return Err(ElfError::ArgumentTooLong);
        }
        page_offset -= len;
        unsafe {
            let dst = top_frame_virt.add(page_offset);
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len());
            *dst.add(bytes.len()) = 0;
        }
        let user_vaddr = (stack_top_virt - 4096) + page_offset as u64;
        envp_ptrs.push(user_vaddr);
    }
    envp_ptrs.reverse();

    // 2. Escreve strings de argv em ordem reversa
    let mut argv_ptrs = alloc::vec::Vec::with_capacity(argv.len());
    for s in argv.iter().rev() {
        let bytes = s.as_bytes();
        let len = bytes.len() + 1; // +1 para \0
        if page_offset < len + 256 {
            return Err(ElfError::ArgumentTooLong);
        }
        page_offset -= len;
        unsafe {
            let dst = top_frame_virt.add(page_offset);
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len());
            *dst.add(bytes.len()) = 0;
        }
        let user_vaddr = (stack_top_virt - 4096) + page_offset as u64;
        argv_ptrs.push(user_vaddr);
    }
    argv_ptrs.reverse();

    // 3. Alinha page_offset para 8 bytes
    page_offset &= !7;

    // 4. Monta tabela de ponteiros na pilha (System V ABI)
    // Total de palavras: argc (1) + argv ptrs (len) + NULL (1) + envp ptrs (len) + NULL (1)
    let total_words = 1 + argv_ptrs.len() + 1 + envp_ptrs.len() + 1;
    let table_bytes = total_words * 8;
    if page_offset < table_bytes + 64 {
        return Err(ElfError::TooManyArguments);
    }

    // Alinha previamente para que, apos subtrair table_bytes, page_offset seja multiplo de 16
    let target_offset = page_offset - table_bytes;
    if (target_offset % 16) != 0 {
        page_offset -= 8; // padding de alinhamento
    }

    // envp NULL terminator
    page_offset -= 8;
    unsafe { (top_frame_virt.add(page_offset) as *mut u64).write(0); }

    // envp pointers
    for &ptr in envp_ptrs.iter().rev() {
        page_offset -= 8;
        unsafe { (top_frame_virt.add(page_offset) as *mut u64).write(ptr); }
    }
    let envp_table_vaddr = if envp_ptrs.is_empty() {
        (stack_top_virt - 4096) + page_offset as u64
    } else {
        (stack_top_virt - 4096) + page_offset as u64
    };

    // argv NULL terminator
    page_offset -= 8;
    unsafe { (top_frame_virt.add(page_offset) as *mut u64).write(0); }

    // argv pointers
    for &ptr in argv_ptrs.iter().rev() {
        page_offset -= 8;
        unsafe { (top_frame_virt.add(page_offset) as *mut u64).write(ptr); }
    }
    let argv_table_vaddr = (stack_top_virt - 4096) + page_offset as u64;

    // argc
    page_offset -= 8;
    let argc = argv.len();
    unsafe { (top_frame_virt.add(page_offset) as *mut u64).write(argc as u64); }

    // O topo da pilha agora aponta diretamente para argc e esta perfeitamente alinhado a 16 bytes
    let final_sp_vaddr = (stack_top_virt - 4096) + page_offset as u64;

    Ok((
        final_sp_vaddr as usize,
        argc,
        argv_table_vaddr as usize,
        envp_table_vaddr as usize,
        pages,
    ))
}
