//! Suíte de Testes de Host para o ELF64 Parser e Validador de Segurança do Jinn OS
//!
//! Testa exaustivamente todos os casos positivos e negativos exigidos pelo
//! Contrato de Execução Segura de Processos do Jinn OS 1.2.

const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const EV_CURRENT: u32 = 1;
const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 62;

pub const PT_LOAD: u32 = 1;
pub const PF_X: u32 = 1 << 0;
pub const PF_W: u32 = 1 << 1;
pub const PF_R: u32 = 1 << 2;

pub const USER_SPACE_MIN: u64 = 0x0000_0000_0001_0000; // 64 KiB
pub const USER_SPACE_MAX: u64 = 0x0000_8000_0000_0000; // 128 TiB
const MAX_PROGRAM_HEADERS: usize = 64;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElfInfo {
    pub entry_point: u64,
    pub phnum: usize,
    pub load_segments: usize,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
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

#[repr(C, packed)]
#[derive(Clone, Copy)]
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

fn validate_elf(elf_data: &[u8]) -> Result<ElfInfo, ElfError> {
    if elf_data.len() < core::mem::size_of::<Elf64_Ehdr>() {
        return Err(ElfError::BufferTooSmall);
    }

    let ehdr_ptr = elf_data.as_ptr() as *const Elf64_Ehdr;
    let ehdr = unsafe { &*ehdr_ptr };

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

    if ehdr.e_entry < USER_SPACE_MIN || ehdr.e_entry >= USER_SPACE_MAX {
        return Err(ElfError::InvalidVirtualAddress);
    }

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
    let mut load_segs = [(0u64, 0u64, false); MAX_PROGRAM_HEADERS];

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

            if phdr.p_align > 1 {
                if !phdr.p_align.is_power_of_two() {
                    return Err(ElfError::AlignmentError);
                }
                if (phdr.p_vaddr % phdr.p_align) != (phdr.p_offset % phdr.p_align) {
                    return Err(ElfError::AlignmentError);
                }
            }

            let is_writable = (phdr.p_flags & PF_W) != 0;
            let is_exec = (phdr.p_flags & PF_X) != 0;
            if is_writable && is_exec {
                return Err(ElfError::SecurityPolicyViolation);
            }

            if load_segments_count < MAX_PROGRAM_HEADERS {
                load_segs[load_segments_count] = (phdr.p_vaddr, mem_end, is_exec);
                load_segments_count += 1;
            } else {
                return Err(ElfError::InvalidProgramHeaders);
            }
        }
    }

    if load_segments_count == 0 {
        return Err(ElfError::InvalidProgramHeaders);
    }

    for i in 0..load_segments_count {
        let (start_a, end_a, _) = load_segs[i];
        for j in (i + 1)..load_segments_count {
            let (start_b, end_b, _) = load_segs[j];
            if start_a < end_b && start_b < end_a {
                return Err(ElfError::OverlappingSegments);
            }
        }
    }

    let mut entry_in_exec = false;
    for &(start, end, is_exec) in &load_segs[..load_segments_count] {
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

fn create_test_elf(ph_flags: u32, p_align: u64, filesz: u64, memsz: u64) -> (Vec<u8>, usize) {
    let ehdr_size = core::mem::size_of::<Elf64_Ehdr>();
    let phdr_size = core::mem::size_of::<Elf64_Phdr>();
    let offset = (ehdr_size + phdr_size) as u64;
    let mut buf = vec![0u8; (offset + filesz) as usize];

    let ehdr = Elf64_Ehdr {
        e_ident: [0x7F, b'E', b'L', b'F', 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        e_type: ET_EXEC,
        e_machine: EM_X86_64,
        e_version: 1,
        e_entry: USER_SPACE_MIN + (offset % if p_align > 1 { p_align } else { 1 }),
        e_phoff: ehdr_size as u64,
        e_shoff: 0,
        e_flags: 0,
        e_ehsize: ehdr_size as u16,
        e_phentsize: phdr_size as u16,
        e_phnum: 1,
        e_shentsize: 0,
        e_shnum: 0,
        e_shstrndx: 0,
    };

    let p_vaddr = if p_align > 1 {
        let base = USER_SPACE_MIN;
        (base & !(p_align - 1)) + (offset % p_align)
    } else {
        USER_SPACE_MIN + 0x1000
    };

    let mut ehdr_fixed = ehdr;
    ehdr_fixed.e_entry = p_vaddr;

    let phdr = Elf64_Phdr {
        p_type: PT_LOAD,
        p_flags: ph_flags,
        p_offset: offset,
        p_vaddr,
        p_paddr: p_vaddr,
        p_filesz: filesz,
        p_memsz: memsz,
        p_align,
    };

    unsafe {
        let ehdr_dst = buf.as_mut_ptr() as *mut Elf64_Ehdr;
        core::ptr::write_unaligned(ehdr_dst, ehdr_fixed);

        let phdr_dst = buf.as_mut_ptr().add(ehdr_size) as *mut Elf64_Phdr;
        core::ptr::write_unaligned(phdr_dst, phdr);
    }

    let len = buf.len();
    (buf, len)
}

#[test]
fn test_elf_minimal_valid() {
    let (buf, len) = create_test_elf(PF_R | PF_X, 0, 64, 64);
    let info = validate_elf(&buf[..len]).expect("ELF mínimo deve ser aceito");
    assert_eq!(info.load_segments, 1);
}

#[test]
fn test_elf_with_bss_expansion() {
    // memsz > filesz (região BSS inicializada em zero)
    let (buf, len) = create_test_elf(PF_R | PF_X, 0, 64, 4096);
    let info = validate_elf(&buf[..len]).expect("ELF com expansão BSS deve ser aceito");
    assert_eq!(info.load_segments, 1);
}

#[test]
fn test_elf_reject_wx_violation() {
    // Violação W^X: simultaneamente W (gravável) e X (executável)
    let (buf, len) = create_test_elf(PF_W | PF_X, 0, 64, 64);
    let res = validate_elf(&buf[..len]);
    assert_eq!(res, Err(ElfError::SecurityPolicyViolation));
}

#[test]
fn test_elf_reject_truncated_header() {
    let tiny = [0x7F, b'E', b'L', b'F', 2, 1];
    assert_eq!(validate_elf(&tiny), Err(ElfError::BufferTooSmall));
}

#[test]
fn test_elf_reject_invalid_magic() {
    let (mut buf, len) = create_test_elf(PF_R | PF_X, 0, 64, 64);
    buf[0] = 0x00;
    assert_eq!(validate_elf(&buf[..len]), Err(ElfError::InvalidMagic));
}

#[test]
fn test_elf_reject_class32() {
    let (mut buf, len) = create_test_elf(PF_R | PF_X, 0, 64, 64);
    buf[4] = 1; // 32-bit
    assert_eq!(validate_elf(&buf[..len]), Err(ElfError::Not64Bit));
}

#[test]
fn test_elf_reject_big_endian() {
    let (mut buf, len) = create_test_elf(PF_R | PF_X, 0, 64, 64);
    buf[5] = 2; // Big-endian
    assert_eq!(validate_elf(&buf[..len]), Err(ElfError::NotLittleEndian));
}

#[test]
fn test_elf_reject_wrong_arch() {
    let (mut buf, len) = create_test_elf(PF_R | PF_X, 0, 64, 64);
    unsafe {
        let ehdr = buf.as_mut_ptr() as *mut Elf64_Ehdr;
        (*ehdr).e_machine = 3; // EM_386
    }
    assert_eq!(validate_elf(&buf[..len]), Err(ElfError::WrongArchitecture));
}

#[test]
fn test_elf_reject_kernel_entry_point() {
    let (mut buf, len) = create_test_elf(PF_R | PF_X, 0, 64, 64);
    unsafe {
        let ehdr = buf.as_mut_ptr() as *mut Elf64_Ehdr;
        (*ehdr).e_entry = 0xFFFF_8000_0000_0000;
    }
    assert_eq!(validate_elf(&buf[..len]), Err(ElfError::InvalidVirtualAddress));
}

#[test]
fn test_elf_reject_null_entry_point() {
    let (mut buf, len) = create_test_elf(PF_R | PF_X, 0, 64, 64);
    unsafe {
        let ehdr = buf.as_mut_ptr() as *mut Elf64_Ehdr;
        (*ehdr).e_entry = 0x0;
    }
    assert_eq!(validate_elf(&buf[..len]), Err(ElfError::InvalidVirtualAddress));
}

#[test]
fn test_elf_reject_filesz_greater_than_memsz() {
    let (mut buf, len) = create_test_elf(PF_R | PF_X, 0, 64, 64);
    let ehdr_size = core::mem::size_of::<Elf64_Ehdr>();
    unsafe {
        let phdr = buf.as_mut_ptr().add(ehdr_size) as *mut Elf64_Phdr;
        (*phdr).p_filesz = 500;
        (*phdr).p_memsz = 100;
    }
    assert_eq!(validate_elf(&buf[..len]), Err(ElfError::FileSizeExceedsMemSize));
}

#[test]
fn test_elf_reject_entry_outside_executable_segment() {
    let (mut buf, len) = create_test_elf(PF_R | PF_X, 0, 64, 64);
    unsafe {
        let ehdr = buf.as_mut_ptr() as *mut Elf64_Ehdr;
        // Aponta para outro endereço no espaço de usuário que não pertence a nenhum segmento
        (*ehdr).e_entry = USER_SPACE_MIN + 0x50000;
    }
    assert_eq!(validate_elf(&buf[..len]), Err(ElfError::EntryNotInExecutableSegment));
}

#[test]
fn test_elf_reject_overlapping_segments() {
    let ehdr_size = core::mem::size_of::<Elf64_Ehdr>();
    let phdr_size = core::mem::size_of::<Elf64_Phdr>();
    let total_len = ehdr_size + phdr_size * 2 + 128;
    let mut buf = vec![0u8; total_len];

    let ehdr = Elf64_Ehdr {
        e_ident: [0x7F, b'E', b'L', b'F', 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        e_type: ET_EXEC,
        e_machine: EM_X86_64,
        e_version: 1,
        e_entry: USER_SPACE_MIN + 0x1000,
        e_phoff: ehdr_size as u64,
        e_shoff: 0,
        e_flags: 0,
        e_ehsize: ehdr_size as u16,
        e_phentsize: phdr_size as u16,
        e_phnum: 2,
        e_shentsize: 0,
        e_shnum: 0,
        e_shstrndx: 0,
    };

    // Segmento 1: [0x10000..0x12000)
    let phdr1 = Elf64_Phdr {
        p_type: PT_LOAD,
        p_flags: PF_R | PF_X,
        p_offset: (ehdr_size + phdr_size * 2) as u64,
        p_vaddr: USER_SPACE_MIN + 0x1000,
        p_paddr: USER_SPACE_MIN + 0x1000,
        p_filesz: 64,
        p_memsz: 64,
        p_align: 0,
    };

    // Segmento 2: [0x10020..0x11020) -> sobreposição com o segmento 1!
    let phdr2 = Elf64_Phdr {
        p_type: PT_LOAD,
        p_flags: PF_R,
        p_offset: (ehdr_size + phdr_size * 2 + 64) as u64,
        p_vaddr: USER_SPACE_MIN + 0x1020, // Sobrepõe!
        p_paddr: USER_SPACE_MIN + 0x1020,
        p_filesz: 64,
        p_memsz: 64,
        p_align: 0,
    };

    unsafe {
        let ehdr_dst = buf.as_mut_ptr() as *mut Elf64_Ehdr;
        core::ptr::write_unaligned(ehdr_dst, ehdr);
        let phdr1_dst = buf.as_mut_ptr().add(ehdr_size) as *mut Elf64_Phdr;
        core::ptr::write_unaligned(phdr1_dst, phdr1);
        let phdr2_dst = buf.as_mut_ptr().add(ehdr_size + phdr_size) as *mut Elf64_Phdr;
        core::ptr::write_unaligned(phdr2_dst, phdr2);
    }

    assert_eq!(validate_elf(&buf), Err(ElfError::OverlappingSegments));
}

#[test]
fn test_elf_reject_misaligned_segment() {
    let (mut buf, len) = create_test_elf(PF_R | PF_X, 0, 32, 32);
    let ehdr_size = core::mem::size_of::<Elf64_Ehdr>();
    unsafe {
        let phdr = buf.as_mut_ptr().add(ehdr_size) as *mut Elf64_Phdr;
        (*phdr).p_align = 4096;
        (*phdr).p_vaddr = USER_SPACE_MIN + 0x1000; // vaddr % 4096 == 0
        (*phdr).p_offset = (ehdr_size + core::mem::size_of::<Elf64_Phdr>()) as u64; // offset % 4096 == 120 != 0
        (*phdr).p_filesz = 32;
        (*phdr).p_memsz = 32;
    }
    assert_eq!(validate_elf(&buf[..len]), Err(ElfError::AlignmentError));
}
