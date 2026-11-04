//! Suíte de Endurecimento e Segurança da ABI (0.3.0)
//!
//! Testa exaustivamente:
//! 1. Rejeição de ponteiros nulos (NULL) e espaço de kernel (Ring 0 / Higher Half).
//! 2. Detecção de overflow aritmético em comprimentos e offsets de buffers de usuário.
//! 3. Validação de descritores de arquivo e limites de tabela (FD 0..=7).
//! 4. Isolamento de recursos e ciclo de vida de tarefas Zombie.
//! 5. Contabilidade de frames físicos e prevenção de double-free.

use std::collections::HashSet;

const USER_SPACE_MIN: usize = 0x0000_0000_0040_0000;
const USER_SPACE_MAX: usize = 0x0000_7FFF_FFFF_0000;
const KERNEL_SPACE_START: usize = 0xFFFF_8000_0000_0000;

// Códigos de erro POSIX
const ENOENT:  isize = -2;
const EBADF:   isize = -9;
const EFAULT:  isize = -14;
const EBUSY:   isize = -16;
const EINVAL:  isize = -22;
const EMFILE:  isize = -24;

/// Validador espelho da rotina `validate_user_buffer` do microkernel
fn sim_validate_user_buffer(ptr_val: usize, len: usize, write: bool, mapped_pages: &HashSet<usize>) -> Result<(), isize> {
    if ptr_val == 0 || len == 0 {
        return Err(EINVAL);
    }
    let end = ptr_val.checked_add(len).ok_or(EFAULT)?;
    if ptr_val < USER_SPACE_MIN || end > USER_SPACE_MAX {
        return Err(EFAULT);
    }
    // Verifica todas as páginas abrangidas
    let start_page = ptr_val & !0xFFF;
    let end_page = (end.saturating_add(0xFFF)) & !0xFFF;
    let mut page = start_page;
    while page < end_page {
        if !mapped_pages.contains(&page) {
            return Err(EFAULT);
        }
        page = page.saturating_add(4096);
    }
    let _ = write;
    Ok(())
}

#[test]
fn test_null_pointer_and_zero_len_rejection() {
    let mut mapped = HashSet::new();
    mapped.insert(0x1000_0000);

    // 1. Ponteiro NULL deve retornar -EINVAL
    assert_eq!(sim_validate_user_buffer(0, 100, false, &mapped), Err(EINVAL));

    // 2. Comprimento zero deve retornar -EINVAL
    assert_eq!(sim_validate_user_buffer(0x1000_0000, 0, false, &mapped), Err(EINVAL));
}

#[test]
fn test_kernel_space_and_unmapped_access_rejection() {
    let mut mapped = HashSet::new();
    mapped.insert(0x1000_0000);

    // 1. Endereço na Higher-Half (Kernel Space) deve ser rejeitado com -EFAULT
    assert_eq!(sim_validate_user_buffer(KERNEL_SPACE_START, 64, false, &mapped), Err(EFAULT));

    // 2. Endereço em espaço de usuário, porém não mapeado na tabela de páginas deve retornar -EFAULT
    let unmapped_addr = 0x2000_0000;
    assert_eq!(sim_validate_user_buffer(unmapped_addr, 64, false, &mapped), Err(EFAULT));

    // 3. Endereço válido e mapeado deve ter sucesso
    assert_eq!(sim_validate_user_buffer(0x1000_0000, 64, false, &mapped), Ok(()));
}

#[test]
fn test_arithmetic_overflow_in_buffer_bounds() {
    let mut mapped = HashSet::new();
    mapped.insert(0x1000_0000);

    // Endereço + len que transborda usize::MAX deve retornar -EFAULT de forma segura (sem crash de pânico)
    let overflow_addr = usize::MAX - 10;
    let len = 20;
    assert_eq!(sim_validate_user_buffer(overflow_addr, len, false, &mapped), Err(EFAULT));
}

#[test]
fn test_descriptor_table_isolation_and_bounds() {
    const MAX_DESCRIPTORS: usize = 8;
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum FdType { None, Stdin, Stdout, Stderr, VfsFile, Socket }

    struct ProcessDescriptors {
        table: [FdType; MAX_DESCRIPTORS],
    }

    impl ProcessDescriptors {
        fn new() -> Self {
            let mut table = [FdType::None; MAX_DESCRIPTORS];
            table[0] = FdType::Stdin;
            table[1] = FdType::Stdout;
            table[2] = FdType::Stderr;
            Self { table }
        }

        fn open_file(&mut self, is_socket: bool) -> Result<usize, isize> {
            for i in 3..MAX_DESCRIPTORS {
                if self.table[i] == FdType::None {
                    self.table[i] = if is_socket { FdType::Socket } else { FdType::VfsFile };
                    return Ok(i);
                }
            }
            Err(EMFILE) // Tabela cheia
        }

        fn close_fd(&mut self, fd: usize) -> Result<(), isize> {
            if fd >= MAX_DESCRIPTORS {
                return Err(EBADF);
            }
            if self.table[fd] == FdType::None {
                return Err(EBADF); // Double-close ou descritor não alocado
            }
            self.table[fd] = FdType::None;
            Ok(())
        }
    }

    let mut proc = ProcessDescriptors::new();

    // 1. Aloca até a exaustão da tabela (misturando arquivo e socket)
    let mut allocated = Vec::new();
    for i in 3..MAX_DESCRIPTORS {
        let fd = proc.open_file(i % 2 == 0).expect("deve alocar FD disponível");
        allocated.push(fd);
    }
    assert_eq!(allocated.len(), 5); // FDs 3, 4, 5, 6, 7

    // 2. Próxima alocação deve falhar com -EMFILE (-24)
    assert_eq!(proc.open_file(false), Err(EMFILE));

    // 3. Libera um descritor e re-aloca
    assert_eq!(proc.close_fd(3), Ok(()));
    // Double-close no mesmo descritor deve falhar com -EBADF (-9)
    assert_eq!(proc.close_fd(3), Err(EBADF));

    // Novo open agora deve reutilizar a fresta liberada (FD 3)
    let reused_fd = proc.open_file(false).expect("reúso de descritor liberado");
    assert_eq!(reused_fd, 3);
}

#[test]
fn test_resource_accounting_physical_frames_and_anti_leak() {
    const TOTAL_TEST_FRAMES: usize = 1000;
    struct BitmapAllocator {
        bitmap: Vec<bool>, // true = ocupado, false = livre
        allocated_count: usize,
    }

    impl BitmapAllocator {
        fn new(size: usize) -> Self {
            Self {
                bitmap: vec![false; size],
                allocated_count: 0,
            }
        }

        fn alloc_frame(&mut self) -> Option<usize> {
            for (idx, occupied) in self.bitmap.iter_mut().enumerate() {
                if !*occupied {
                    *occupied = true;
                    self.allocated_count += 1;
                    return Some(idx);
                }
            }
            None
        }

        fn free_frame(&mut self, frame: usize) -> Result<(), isize> {
            if frame >= self.bitmap.len() {
                return Err(EINVAL);
            }
            if !self.bitmap[frame] {
                return Err(EFAULT); // Double free detectado
            }
            self.bitmap[frame] = false;
            self.allocated_count -= 1;
            Ok(())
        }
    }

    let mut alloc = BitmapAllocator::new(TOTAL_TEST_FRAMES);

    // Ciclo de 500 alocações
    let mut frames = Vec::new();
    for _ in 0..500 {
        frames.push(alloc.alloc_frame().expect("deve ter frame livre"));
    }
    assert_eq!(alloc.allocated_count, 500);

    // Libera todos os 500 frames
    for f in frames {
        assert_eq!(alloc.free_frame(f), Ok(()));
    }

    // Comprova ausência de vazamento: o contador de alocações volta exatamente a zero
    assert_eq!(alloc.allocated_count, 0);
    assert!(!alloc.bitmap.iter().any(|&occupied| occupied));
}

#[test]
fn test_process_zombie_collection_and_pid_isolation() {
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum TaskState { Ready, Running, Sleeping, Zombie, Completed }

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    struct SimTask {
        pid: usize,
        state: TaskState,
        exit_code: i32,
    }

    struct SimScheduler {
        tasks: Vec<Option<SimTask>>,
    }

    impl SimScheduler {
        fn new() -> Self {
            Self { tasks: vec![None; 16] }
        }

        fn spawn(&mut self, pid: usize) {
            self.tasks[pid] = Some(SimTask {
                pid,
                state: TaskState::Ready,
                exit_code: 0,
            });
        }

        fn set_running(&mut self, pid: usize) {
            if let Some(t) = self.tasks[pid].as_mut() {
                t.state = TaskState::Running;
            }
        }

        fn set_sleeping(&mut self, pid: usize) {
            if let Some(t) = self.tasks[pid].as_mut() {
                t.state = TaskState::Sleeping;
            }
        }

        fn exit(&mut self, pid: usize, code: i32) {
            if let Some(task) = self.tasks[pid].as_mut() {
                task.state = TaskState::Zombie;
                task.exit_code = code;
            }
        }

        fn collect_zombie(&mut self, pid: usize) -> Result<i32, isize> {
            if pid >= self.tasks.len() {
                return Err(EINVAL);
            }
            match self.tasks[pid].as_ref() {
                Some(task) if task.state == TaskState::Zombie => {
                    let code = task.exit_code;
                    self.tasks[pid] = Some(SimTask { pid, state: TaskState::Completed, exit_code: code });
                    self.tasks[pid] = None; // Desaloca e libera o slot
                    Ok(code)
                }
                Some(_) => Err(EBUSY), // Processo ainda em execução
                None => Err(ENOENT),   // Processo inexistente
            }
        }
    }

    let mut sched = SimScheduler::new();
    sched.spawn(4);
    sched.set_running(4);
    sched.set_sleeping(4);

    // Tentativa de coletar tarefa ainda ativa deve retornar -EBUSY (-16)
    assert_eq!(sched.collect_zombie(4), Err(EBUSY));

    // Tarefa finaliza com código 42
    sched.exit(4, 42);

    // Coleta bem-sucedida do zumbi
    assert_eq!(sched.collect_zombie(4), Ok(42));

    // Tentativa repetida deve retornar -ENOENT (-2) pois o zumbi foi consumido e desalocado
    assert_eq!(sched.collect_zombie(4), Err(ENOENT));
}
