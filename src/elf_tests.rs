/// Jinn OS — Suíte de Testes Negativos de Segurança para o ELF Loader
///
/// Valida que o parser e o loader rejeitam de forma estrita e segura qualquer
/// binário malformado, truncado, hostil ou que viole as políticas de isolamento
/// e segurança (como W^X, injeção de ponteiros de kernel ou estouro de inteiros).

use crate::elf::{validate_elf, ElfError, PF_R, PF_W, PF_X, PT_LOAD, USER_SPACE_MIN};

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct Elf64_Ehdr_Raw {
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
struct Elf64_Phdr_Raw {
    p_type:   u32,
    p_flags:  u32,
    p_offset: u64,
    p_vaddr:  u64,
    p_paddr:  u64,
    p_filesz: u64,
    p_memsz:  u64,
    p_align:  u64,
}

/// Cria um cabeçalho ELF64 válido mínimo em um buffer configurável.
fn create_valid_dummy_elf() -> ([u8; 256], usize) {
    let mut buf = [0u8; 256];
    let ehdr_size = core::mem::size_of::<Elf64_Ehdr_Raw>();
    let phdr_size = core::mem::size_of::<Elf64_Phdr_Raw>();

    let ehdr = Elf64_Ehdr_Raw {
        e_ident: [
            0x7F, b'E', b'L', b'F', // Magic
            2,                      // ELFCLASS64
            1,                      // ELFDATA2LSB
            1,                      // EV_CURRENT
            0, 0, 0, 0, 0, 0, 0, 0, 0,
        ],
        e_type: 2,                  // ET_EXEC
        e_machine: 62,              // EM_X86_64
        e_version: 1,
        e_entry: USER_SPACE_MIN + 0x1000,
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

    let phdr = Elf64_Phdr_Raw {
        p_type: PT_LOAD,
        p_flags: PF_R | PF_X,       // Somente Leitura + Execução (atende W^X)
        p_offset: (ehdr_size + phdr_size) as u64,
        p_vaddr: USER_SPACE_MIN + 0x1000,
        p_paddr: USER_SPACE_MIN + 0x1000,
        p_filesz: 32,
        p_memsz: 32,
        p_align: 4096,
    };

    unsafe {
        let ehdr_dst = buf.as_mut_ptr() as *mut Elf64_Ehdr_Raw;
        core::ptr::write_unaligned(ehdr_dst, ehdr);

        let phdr_dst = buf.as_mut_ptr().add(ehdr_size) as *mut Elf64_Phdr_Raw;
        core::ptr::write_unaligned(phdr_dst, phdr);
    }

    let total_len = ehdr_size + phdr_size + 32;
    (buf, total_len)
}

/// Executa a suíte completa de testes de segurança negativos do ELF loader.
/// Retorna `(testes_passados, total_testes)`.
pub fn run_elf_security_selftests() -> (usize, usize) {
    let mut passed = 0;
    let mut total = 0;

    macro_rules! assert_test {
        ($name:expr, $expr:expr) => {
            total += 1;
            if $expr {
                passed += 1;
            } else {
                crate::serial_write(b"  [FALHA] Teste de seguranca ELF: ");
                crate::serial_write($name.as_bytes());
                crate::serial_write(b"\n");
            }
        };
    }

    // 1. Binário truncado (< sizeof Ehdr)
    let tiny = [0x7F, b'E', b'L', b'F'];
    assert_test!("ELF Truncado (< 64B)", validate_elf(&tiny) == Err(ElfError::BufferTooSmall));

    // 2. Magic bytes corrompidos
    let (mut buf, len) = create_valid_dummy_elf();
    buf[0] = 0x00;
    assert_test!("Magic Invalido", validate_elf(&buf[..len]) == Err(ElfError::InvalidMagic));

    // 3. Binário 32-bit (ELFCLASS32) rejeitado
    let (mut buf, len) = create_valid_dummy_elf();
    buf[4] = 1; // 32-bit
    assert_test!("Rejeicao de ELFCLASS32", validate_elf(&buf[..len]) == Err(ElfError::Not64Bit));

    // 4. Big-Endian rejeitado
    let (mut buf, len) = create_valid_dummy_elf();
    buf[5] = 2; // Big-endian
    assert_test!("Rejeicao de Big-Endian", validate_elf(&buf[..len]) == Err(ElfError::NotLittleEndian));

    // 5. Arquitetura incorreta (ex: i386 ao inves de x86-64)
    let (mut buf, len) = create_valid_dummy_elf();
    unsafe {
        let ehdr = buf.as_mut_ptr() as *mut Elf64_Ehdr_Raw;
        (*ehdr).e_machine = 3; // EM_386
    }
    assert_test!("Rejeicao de Arquitetura Incorreta", validate_elf(&buf[..len]) == Err(ElfError::WrongArchitecture));

    // 6. Ponto de entrada apontando para espaço de kernel (tentativa de sequestro de Ring 0)
    let (mut buf, len) = create_valid_dummy_elf();
    unsafe {
        let ehdr = buf.as_mut_ptr() as *mut Elf64_Ehdr_Raw;
        (*ehdr).e_entry = 0xFFFF_8000_0000_0000;
    }
    assert_test!("Rejeicao de Entry Point no Kernel", validate_elf(&buf[..len]) == Err(ElfError::InvalidVirtualAddress));

    // 7. Ponto de entrada nulo (NULL guard)
    let (mut buf, len) = create_valid_dummy_elf();
    unsafe {
        let ehdr = buf.as_mut_ptr() as *mut Elf64_Ehdr_Raw;
        (*ehdr).e_entry = 0x0;
    }
    assert_test!("Rejeicao de Entry Point NULL", validate_elf(&buf[..len]) == Err(ElfError::InvalidVirtualAddress));

    // 8. Program headers apontando para fora do arquivo (truncamento)
    let (mut buf, len) = create_valid_dummy_elf();
    unsafe {
        let ehdr = buf.as_mut_ptr() as *mut Elf64_Ehdr_Raw;
        (*ehdr).e_phoff = 99999;
    }
    assert_test!("Segmento Program Header Truncado", validate_elf(&buf[..len]) == Err(ElfError::SegmentTruncated));

    // 9. Segmento com tamanho em arquivo maior que na memoria (filesz > memsz)
    let (mut buf, len) = create_valid_dummy_elf();
    let ehdr_size = core::mem::size_of::<Elf64_Ehdr_Raw>();
    unsafe {
        let phdr = buf.as_mut_ptr().add(ehdr_size) as *mut Elf64_Phdr_Raw;
        (*phdr).p_filesz = 500;
        (*phdr).p_memsz = 100;
    }
    assert_test!("Rejeicao filesz > memsz", validate_elf(&buf[..len]) == Err(ElfError::FileSizeExceedsMemSize));

    // 10. Violação da política de segurança W^X (Segmento gravável e executável simultaneamente)
    let (mut buf, len) = create_valid_dummy_elf();
    unsafe {
        let phdr = buf.as_mut_ptr().add(ehdr_size) as *mut Elf64_Phdr_Raw;
        (*phdr).p_flags = PF_W | PF_X; // W+X
    }
    assert_test!("Violacao W^X Rejeitada", validate_elf(&buf[..len]) == Err(ElfError::SecurityPolicyViolation));

    // 11. Segmento mapeando endereço de kernel
    let (mut buf, len) = create_valid_dummy_elf();
    unsafe {
        let phdr = buf.as_mut_ptr().add(ehdr_size) as *mut Elf64_Phdr_Raw;
        (*phdr).p_vaddr = 0xFFFF_8000_0000_0000;
    }
    assert_test!("Rejeicao de Vaddr no Kernel", validate_elf(&buf[..len]) == Err(ElfError::InvalidVirtualAddress));

    // 12. Estouro de inteiro aritmético em limites de memória
    let (mut buf, len) = create_valid_dummy_elf();
    unsafe {
        let phdr = buf.as_mut_ptr().add(ehdr_size) as *mut Elf64_Phdr_Raw;
        (*phdr).p_vaddr = u64::MAX - 100;
        (*phdr).p_memsz = 200;
    }
    assert_test!("Prevencao de Overflow Aritmetico", validate_elf(&buf[..len]) == Err(ElfError::ArithmeticOverflow));

    // 13. Binário válido de referência deve ser aceito
    let (buf, len) = create_valid_dummy_elf();
    assert_test!("ELF Valido Aceito", validate_elf(&buf[..len]).is_ok());

    (passed, total)
}
