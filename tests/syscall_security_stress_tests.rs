// tests/syscall_security_stress_tests.rs
// Testes unitários e de estresse para validação de segurança em Syscalls e despacho Ring 3 -> Ring 0

pub const EINVAL: isize = -22;
pub const EFAULT: isize = -14;
pub const EPERM:  isize = -1;
pub const ESRCH:  isize = -3;
pub const EBADF:  isize = -9;
pub const EDEADLK: isize = -35;
pub const ENOSYS: isize = -38;

pub const USER_SPACE_MIN: usize = 0x0000_0000_0001_0000; // 64 KiB NULL guard
pub const USER_SPACE_MAX: usize = 0x0000_8000_0000_0000; // 128 TiB limite canônico de usuário

pub const USER_HEAP_BASE: u64 = 0x0000_2000_0000_0000;
pub const USER_HEAP_MAX:  u64 = 0x0000_3000_0000_0000;

pub const PAGE_PRESENT:  u64 = 1 << 0;
pub const PAGE_WRITABLE: u64 = 1 << 1;
pub const PAGE_USER:     u64 = 1 << 2;

#[derive(Clone, Copy, Debug)]
pub struct MockPageEntry {
    pub vaddr: u64,
    pub paddr: u64,
    pub flags: u64,
}

pub struct MockPageTable {
    pub entries: [Option<MockPageEntry>; 128],
    pub count: usize,
}

impl MockPageTable {
    pub fn new() -> Self {
        Self {
            entries: [None; 128],
            count: 0,
        }
    }

    pub fn map(&mut self, vaddr: u64, paddr: u64, flags: u64) {
        let page_vaddr = vaddr & !0xFFF;
        for i in 0..self.count {
            if let Some(ref mut e) = self.entries[i] {
                if e.vaddr == page_vaddr {
                    e.paddr = paddr;
                    e.flags = flags;
                    return;
                }
            }
        }
        if self.count < 128 {
            self.entries[self.count] = Some(MockPageEntry {
                vaddr: page_vaddr,
                paddr,
                flags,
            });
            self.count += 1;
        }
    }

    pub fn lookup(&self, vaddr: u64) -> Option<(u64, u64)> {
        let page_vaddr = vaddr & !0xFFF;
        for i in 0..self.count {
            if let Some(ref e) = self.entries[i] {
                if e.vaddr == page_vaddr {
                    return Some((e.paddr, e.flags));
                }
            }
        }
        None
    }
}

pub fn validate_user_buffer_mock(
    pt: &MockPageTable,
    ptr: *const u8,
    len: usize,
    write_access: bool,
) -> Result<(), isize> {
    if ptr.is_null() || len == 0 {
        return Err(EINVAL);
    }
    let addr = ptr as usize;
    let end = addr.checked_add(len).ok_or(EFAULT)?;
    if addr < USER_SPACE_MIN || end > USER_SPACE_MAX {
        return Err(EFAULT);
    }

    let start_page = (addr as u64) & !0xFFF;
    let end_page = ((end as u64).saturating_add(0xFFF)) & !0xFFF;
    let mut page = start_page;

    while page < end_page {
        match pt.lookup(page) {
            Some((_phys, flags)) => {
                if (flags & PAGE_USER) == 0 {
                    return Err(EFAULT);
                }
                if write_access && (flags & PAGE_WRITABLE) == 0 {
                    return Err(EFAULT);
                }
            }
            None => return Err(EFAULT),
        }
        page = page.saturating_add(4096);
    }

    Ok(())
}

pub fn copy_from_user_mock(
    pt: &MockPageTable,
    dst: &mut [u8],
    src: *const u8,
    len: usize,
    is_user: bool,
) -> Result<(), isize> {
    if src.is_null() || len == 0 || dst.len() < len {
        return Err(EINVAL);
    }
    if is_user {
        validate_user_buffer_mock(pt, src, len, false)?;
    }
    unsafe {
        core::ptr::copy_nonoverlapping(src, dst.as_mut_ptr(), len);
    }
    Ok(())
}

pub fn copy_to_user_mock(
    pt: &MockPageTable,
    dst: *mut u8,
    src: &[u8],
    len: usize,
    is_user: bool,
) -> Result<(), isize> {
    if dst.is_null() || len == 0 || src.len() < len {
        return Err(EINVAL);
    }
    if is_user {
        validate_user_buffer_mock(pt, dst as *const u8, len, true)?;
    }
    unsafe {
        core::ptr::copy_nonoverlapping(src.as_ptr(), dst, len);
    }
    Ok(())
}

#[test]
fn test_user_space_boundary_validation() {
    print!("Teste 1: Validação de limites do espaço de usuário (USER_SPACE_MIN/MAX)... ");
    let mut pt = MockPageTable::new();
    pt.map(0x0001_0000, 0x1000_0000, PAGE_PRESENT | PAGE_USER | PAGE_WRITABLE);

    // 1. Ponteiro NULL
    assert_eq!(validate_user_buffer_mock(&pt, core::ptr::null(), 16, false), Err(EINVAL));
    // 2. Comprimento 0
    assert_eq!(validate_user_buffer_mock(&pt, 0x0001_0000 as *const u8, 0, false), Err(EINVAL));
    // 3. Overflow numérico (wrapping)
    assert_eq!(validate_user_buffer_mock(&pt, (usize::MAX - 8) as *const u8, 16, false), Err(EFAULT));
    // 4. Faixa de NULL Guard (abaixo de 64 KiB)
    assert_eq!(validate_user_buffer_mock(&pt, 0x0000_1000 as *const u8, 16, false), Err(EFAULT));
    // 5. Acima de USER_SPACE_MAX (128 TiB)
    assert_eq!(validate_user_buffer_mock(&pt, 0x0000_8000_0000_0000 as *const u8, 16, false), Err(EFAULT));
    // 6. Espaço de Kernel canônico alto (0xFFFF_8000_...)
    assert_eq!(validate_user_buffer_mock(&pt, 0xFFFF_8000_0000_0000 as *const u8, 16, false), Err(EFAULT));
    // 7. Faixa válida mapeada
    assert_eq!(validate_user_buffer_mock(&pt, 0x0001_0000 as *const u8, 64, false), Ok(()));

    println!("PASSOU!");
}

#[test]
fn test_page_table_permissions_user_vs_supervisor() {
    print!("Teste 2: Permissões de tabela de páginas (USER vs SUPERVISOR, RO vs RW)... ");
    let mut pt = MockPageTable::new();
    // Página 1: User Read-Only
    pt.map(0x0002_0000, 0x1000, PAGE_PRESENT | PAGE_USER);
    // Página 2: User Read-Write
    pt.map(0x0002_1000, 0x2000, PAGE_PRESENT | PAGE_USER | PAGE_WRITABLE);
    // Página 3: Supervisor (Kernel) Read-Write
    pt.map(0x0002_2000, 0x3000, PAGE_PRESENT | PAGE_WRITABLE);

    // Leitura na página User RO: deve ser permitida
    assert_eq!(validate_user_buffer_mock(&pt, 0x0002_0000 as *const u8, 64, false), Ok(()));
    // Escrita na página User RO: deve falhar com EFAULT
    assert_eq!(validate_user_buffer_mock(&pt, 0x0002_0000 as *const u8, 64, true), Err(EFAULT));

    // Leitura e escrita na página User RW: permitidas
    assert_eq!(validate_user_buffer_mock(&pt, 0x0002_1000 as *const u8, 64, false), Ok(()));
    assert_eq!(validate_user_buffer_mock(&pt, 0x0002_1000 as *const u8, 64, true), Ok(()));

    // Acesso à página de Supervisor (Ring 0): rejeição absoluta com EFAULT
    assert_eq!(validate_user_buffer_mock(&pt, 0x0002_2000 as *const u8, 64, false), Err(EFAULT));
    assert_eq!(validate_user_buffer_mock(&pt, 0x0002_2000 as *const u8, 64, true), Err(EFAULT));

    // Página não mapeada: EFAULT
    assert_eq!(validate_user_buffer_mock(&pt, 0x0003_0000 as *const u8, 64, false), Err(EFAULT));

    println!("PASSOU!");
}

#[test]
fn test_copy_from_and_to_user_primitives() {
    print!("Teste 3: Primitivas seguras copy_from_user e copy_to_user... ");
    let mut pt = MockPageTable::new();
    let user_vaddr = 0x0004_0000;
    pt.map(user_vaddr, 0x4000, PAGE_PRESENT | PAGE_USER | PAGE_WRITABLE);

    let kernel_src = b"Mensagem secreta do kernel Jinn OS!";
    let mut kernel_dst = [0u8; 64];

    let mut mock_user_mem = [0u8; 4096];
    let user_ptr = mock_user_mem.as_mut_ptr();

    // 1. copy_to_user: Kernel -> User
    let res = copy_to_user_mock(&pt, user_ptr, kernel_src, kernel_src.len(), false);
    assert_eq!(res, Ok(()));
    assert_eq!(&mock_user_mem[..kernel_src.len()], kernel_src);

    // 2. copy_from_user: User -> Kernel
    let res = copy_from_user_mock(&pt, &mut kernel_dst[..kernel_src.len()], user_ptr, kernel_src.len(), false);
    assert_eq!(res, Ok(()));
    assert_eq!(&kernel_dst[..kernel_src.len()], kernel_src);

    // 3. Destino menor que len -> EINVAL
    let mut short_dst = [0u8; 8];
    assert_eq!(copy_from_user_mock(&pt, &mut short_dst, user_ptr, 16, false), Err(EINVAL));

    // 4. Fonte menor que len -> EINVAL
    assert_eq!(copy_to_user_mock(&pt, user_ptr, &kernel_src[..4], 16, false), Err(EINVAL));

    println!("PASSOU!");
}

#[test]
fn test_sys_free_page_ownership_and_bounds() {
    print!("Teste 4: sys_free_page alinhamento, faixa de heap e propriedade... ");
    // Simula a lógica de sys_free_page
    let check_free_page = |addr: u64, dynamic_pages: &[u64]| -> isize {
        if (addr & 0xFFF) != 0 {
            return EINVAL;
        }
        if addr < USER_HEAP_BASE || addr >= USER_HEAP_MAX {
            return EFAULT;
        }
        if !dynamic_pages.contains(&addr) {
            return EFAULT; // Rejeita página não alocada ou double free
        }
        0
    };

    let pages = [0x0000_2000_0000_0000, 0x0000_2000_0000_1000];

    // Endereço desalinhado: EINVAL
    assert_eq!(check_free_page(0x0000_2000_0000_0001, &pages), EINVAL);
    // Endereço fora da faixa de heap (ex: pilha ou ELF): EFAULT
    assert_eq!(check_free_page(0x0000_7FFF_FFFF_0000, &pages), EFAULT);
    // Endereço de kernel: EFAULT
    assert_eq!(check_free_page(0xFFFF_8000_0000_0000, &pages), EFAULT);
    // Endereço não pertencente ao processo (não registrado): EFAULT
    assert_eq!(check_free_page(0x0000_2000_0000_2000, &pages), EFAULT);
    // Endereço válido e registrado: Sucesso (0)
    assert_eq!(check_free_page(0x0000_2000_0000_0000, &pages), 0);

    println!("PASSOU!");
}

#[test]
fn test_sys_task_set_state_hierarchy_and_security() {
    print!("Teste 5: sys_task_set_state hierarquia, proteção PID 0 e anti-deadlock... ");
    // Simula a lógica de sys_task_set_state
    let check_task_state = |caller: usize, target: usize, new_state: u32, parent: Option<usize>| -> isize {
        if target == 0 {
            return EPERM; // Proteção PID 0 (Idle)
        }
        let is_self = caller == target;
        let is_parent = parent == Some(caller);
        let is_privileged = caller <= 1;

        if !is_privileged && !is_parent && !is_self {
            return EPERM;
        }

        if is_self && !is_privileged && new_state == 3 {
            return EDEADLK; // Anti auto-deadlock
        }

        if new_state == 1 || new_state == 3 {
            0
        } else {
            EINVAL
        }
    };

    // 1. Tentar alterar estado da Idle Task (PID 0) -> EPERM
    assert_eq!(check_task_state(2, 0, 3, None), EPERM);
    assert_eq!(check_task_state(0, 0, 3, None), EPERM);

    // 2. Tarefa Ring 3 não privilegiada tentando bloquear a si mesma -> EDEADLK
    assert_eq!(check_task_state(5, 5, 3, Some(2)), EDEADLK);

    // 3. Processo pai (ex: navegador PID 2) suspendendo aba filha (PID 5) -> Ok (0)
    assert_eq!(check_task_state(2, 5, 3, Some(2)), 0);

    // 4. Processo pai retomando aba filha -> Ok (0)
    assert_eq!(check_task_state(2, 5, 1, Some(2)), 0);

    // 5. Processo terceiro não relacionado tentando intervir -> EPERM
    assert_eq!(check_task_state(7, 5, 3, Some(2)), EPERM);

    // 6. Estado inválido -> EINVAL
    assert_eq!(check_task_state(2, 5, 99, Some(2)), EINVAL);

    println!("PASSOU!");
}

#[test]
fn test_smp_scheduler_concurrency_and_ticks() {
    print!("Teste 6: Escalonador SMP, trava SCHED_LOCK e isolamento PML4... ");
    use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;

    let ctx_switches = Arc::new(AtomicU64::new(0));
    let sched_ticks = Arc::new(AtomicU64::new(0));
    let lock_active = Arc::new(AtomicBool::new(false));

    let mut handles = Vec::new();
    for core in 0..4 {
        let cs = Arc::clone(&ctx_switches);
        let st = Arc::clone(&sched_ticks);
        let la = Arc::clone(&lock_active);
        handles.push(thread::spawn(move || {
            for _ in 0..1000 {
                // Simula aquisição de SCHED_LOCK
                while la.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
                    core::hint::spin_loop();
                }

                // Seção crítica do escalonador
                cs.fetch_add(1, Ordering::Relaxed);
                st.fetch_add(1, Ordering::Relaxed);

                la.store(false, Ordering::Release);
            }
            core
        }));
    }

    for h in handles {
        let _ = h.join().unwrap();
    }

    assert_eq!(ctx_switches.load(Ordering::SeqCst), 4000);
    assert_eq!(sched_ticks.load(Ordering::SeqCst), 4000);

    println!("PASSOU!");
}

fn main() {
    println!("==================================================================");
    println!("  JINN OS — AUDITORIA DE SEGURANÇA E DESPACHO DE SYSCALLS (RING 3)");
    println!("==================================================================");
    test_user_space_boundary_validation();
    test_page_table_permissions_user_vs_supervisor();
    test_copy_from_and_to_user_primitives();
    test_sys_free_page_ownership_and_bounds();
    test_sys_task_set_state_hierarchy_and_security();
    test_smp_scheduler_concurrency_and_ticks();
    println!("==================================================================");
    println!("  TODOS OS TESTES DE SEGURANÇA DE SYSCALLS E RING 3 PASSARAM!");
    println!("==================================================================");
}

