/// Jinn OS — ELF64 Parser & Loader
///
/// Implementa o carregamento de executáveis formato ELF 64-bit para a
/// memória do usuário (Ring 3), mapeando corretamente os segmentos na
/// tabela de páginas do processo.

use crate::memory::paging;

// ── Constants ─────────────────────────────────────────────────────────────────

const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1; // Little Endian
const ET_EXEC: u16 = 2; // Executable file
const EM_X86_64: u16 = 62; // AMD x86-64

const PT_LOAD: u32 = 1;

// Segment Flags
pub const PF_X: u32 = 1 << 0; // Executable
pub const PF_W: u32 = 1 << 1; // Writable
pub const PF_R: u32 = 1 << 2; // Readable

// ── ELF Structures ────────────────────────────────────────────────────────────

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

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ElfError {
    BufferTooSmall,
    InvalidMagic,
    Not64Bit,
    NotLittleEndian,
    NotExecutable,
    WrongArchitecture,
    OutOfMemory,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Analisa e carrega um arquivo ELF na tabela de páginas (`pml4_phys`) especificada.
/// Retorna o endereço virtual do ponto de entrada (`e_entry`).
///
/// # Parâmetros
/// - `elf_data`: Buffer contendo os bytes crus do arquivo ELF lido da memória/disco.
/// - `pml4_phys`: Endereço físico da tabela de páginas raiz (CR3) onde o
///                binário será mapeado (isolamento de memória).
pub fn load_elf(elf_data: &[u8], pml4_phys: u64) -> Result<u64, ElfError> {
    if elf_data.len() < core::mem::size_of::<Elf64_Ehdr>() {
        return Err(ElfError::BufferTooSmall);
    }

    // Lê o cabeçalho principal
    let ehdr_ptr = elf_data.as_ptr() as *const Elf64_Ehdr;
    let ehdr = unsafe { &*ehdr_ptr };

    // 1. Validações estritas do formato
    if ehdr.e_ident[0..4] != ELF_MAGIC {
        return Err(ElfError::InvalidMagic);
    }
    if ehdr.e_ident[4] != ELFCLASS64 {
        return Err(ElfError::Not64Bit);
    }
    if ehdr.e_ident[5] != ELFDATA2LSB {
        return Err(ElfError::NotLittleEndian);
    }
    if ehdr.e_type != ET_EXEC {
        return Err(ElfError::NotExecutable);
    }
    if ehdr.e_machine != EM_X86_64 {
        return Err(ElfError::WrongArchitecture);
    }

    // 2. Iterar pelos Program Headers (segmentos)
    let phoff = ehdr.e_phoff as usize;
    let phnum = ehdr.e_phnum as usize;
    let phentsize = ehdr.e_phentsize as usize;

    if elf_data.len() < phoff + (phnum * phentsize) {
        return Err(ElfError::BufferTooSmall);
    }

    let pml4 = paging::phys_to_virt(pml4_phys) as *mut paging::PageTable;

    for i in 0..phnum {
        let phdr_ptr = unsafe {
            elf_data.as_ptr().add(phoff + (i * phentsize)) as *const Elf64_Phdr
        };
        let phdr = unsafe { &*phdr_ptr };

        if phdr.p_type == PT_LOAD {
            if phdr.p_memsz == 0 {
                continue;
            }

            // Descobrir as permissões do segmento
            let mut page_flags = paging::PRESENT | paging::USER;
            if (phdr.p_flags & PF_W) != 0 {
                page_flags |= paging::WRITABLE;
            }
            if (phdr.p_flags & PF_X) == 0 {
                page_flags |= paging::NO_EXEC;
            }

            // O endereço virtual base do segmento
            let vaddr = phdr.p_vaddr;
            let memsz = phdr.p_memsz;
            let filesz = phdr.p_filesz;
            let offset = phdr.p_offset;

            // Precisamos alinhar aos limites de página (4 KiB)
            let start_page = vaddr & !0xFFF;
            let end_page = (vaddr + memsz + 0xFFF) & !0xFFF;
            let num_pages = (end_page - start_page) / 4096;

            // Alocar frames físicos, mapeá-los, e copiar os dados
            for p in 0..num_pages {
                let current_vaddr = start_page + (p * 4096);

                let phys_frame = crate::memory::alloc_frame()
                    .ok_or(ElfError::OutOfMemory)?;

                // Mapear na tabela do usuário
                unsafe {
                    paging::map_page(pml4, current_vaddr, phys_frame, page_flags);
                }

                // Vamos preencher o frame com dados ou zeros.
                // Como não estamos rodando nesse PML4 agora (estamos no PML4 do kernel),
                // acessamos o frame físico recém-alocado via HHDM (espaço do kernel).
                let frame_virt = paging::phys_to_virt(phys_frame) as *mut u8;
                
                // Zero a página inteira por padrão (para BSS ou preenchimento)
                unsafe {
                    core::ptr::write_bytes(frame_virt, 0, 4096);
                }

                // Se houver dados do arquivo que pertencem a essa página, copiar.
                let page_offset = if current_vaddr < vaddr {
                    vaddr - current_vaddr // Página inclui dados antes do vaddr (alinhamento)
                } else {
                    0
                };

                let file_data_start = if current_vaddr < vaddr {
                    0
                } else {
                    current_vaddr - vaddr
                };

                if file_data_start < filesz {
                    let copy_len = core::cmp::min(
                        4096 - page_offset as usize,
                        (filesz - file_data_start) as usize
                    );

                    let src = unsafe {
                        elf_data.as_ptr().add(offset as usize + file_data_start as usize)
                    };
                    let dst = unsafe { frame_virt.add(page_offset as usize) };

                    unsafe {
                        core::ptr::copy_nonoverlapping(src, dst, copy_len);
                    }
                }
            }
        }
    }

    Ok(ehdr.e_entry)
}
