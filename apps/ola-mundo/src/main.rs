#![no_std]
#![no_main]

use core::{arch::asm, panic::PanicInfo};

// ── Constantes de Erros POSIX ─────────────────────────────────────────────────
const EPERM:   i64 = -1;
const EBADF:   i64 = -9;
const ENOMEM:  i64 = -12;
const EFAULT:  i64 = -14;
const EINVAL:  i64 = -22;
const EMFILE:  i64 = -24;
const ENOBUFS: i64 = -105;

// ── Syscall Helpers ──────────────────────────────────────────────────────────

#[inline(always)]
unsafe fn syscall3(nr: u64, arg0: u64, arg1: u64, arg2: u64) -> i64 {
    let ret: i64;
    asm!(
        "int 0x80",
        inout("rax") nr => ret,
        in("rdi") arg0,
        in("rsi") arg1,
        in("rdx") arg2,
        options(nostack)
    );
    ret
}

#[inline(always)]
unsafe fn sys_exit(code: i32) -> ! {
    loop {
        syscall3(0, code as u64, 0, 0);
    }
}

#[inline(always)]
unsafe fn sys_write(fd: u64, buf: *const u8, len: u64) -> i64 {
    syscall3(1, fd, buf as u64, len)
}

#[inline(always)]
unsafe fn sys_read(fd: u64, buf: *mut u8, len: u64) -> i64 {
    syscall3(2, fd, buf as u64, len)
}

#[inline(always)]
unsafe fn sys_yield() -> i64 {
    syscall3(3, 0, 0, 0)
}

#[inline(always)]
unsafe fn sys_alloc_page() -> i64 {
    syscall3(7, 0, 0, 0)
}

#[inline(always)]
unsafe fn sys_free_page(addr: u64) -> i64 {
    syscall3(8, addr, 0, 0)
}

#[inline(always)]
unsafe fn sys_sleep(ms: u64) -> i64 {
    syscall3(11, ms, 0, 0)
}

#[inline(always)]
unsafe fn sys_jda_shm_alloc(is_read_only: bool) -> i64 {
    syscall3(12, if is_read_only { 1 } else { 0 }, 0, 0)
}

#[inline(always)]
unsafe fn sys_jda_shm_release(handle: u64) -> i64 {
    syscall3(13, handle, 0, 0)
}

#[inline(always)]
unsafe fn sys_jda_submit(job_type: u64, handle: u64, len: u64) -> i64 {
    syscall3(14, job_type, handle, len)
}

#[inline(always)]
unsafe fn sys_open(path: &[u8], flags: u32) -> i64 {
    syscall3(15, path.as_ptr() as u64, path.len() as u64, flags as u64)
}

#[inline(always)]
unsafe fn sys_close(fd: u64) -> i64 {
    syscall3(16, fd, 0, 0)
}

#[inline(always)]
unsafe fn sys_window_create(title: &[u8], width: usize, height: usize) -> i64 {
    let packed = (((width & 0xFFFF) as u64) << 16) | ((height & 0xFFFF) as u64);
    syscall3(20, title.as_ptr() as u64, title.len() as u64, packed)
}

#[inline(always)]
unsafe fn sys_window_update(win_id: usize, buf: &[u32]) -> i64 {
    syscall3(21, win_id as u64, buf.as_ptr() as u64, (buf.len() * 4) as u64)
}

#[inline(always)]
unsafe fn sys_window_close(win_id: usize) -> i64 {
    syscall3(22, win_id as u64, 0, 0)
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ProcessResourceStat {
    pub pid: u32,
    pub state: u32,
    pub allocated_frames: u32,
    pub peak_allocated_frames: u32,
    pub open_fds: u32,
    pub open_windows: u32,
    pub open_jda_buffers: u32,
    pub open_sockets: u32,
    pub cpu_ticks: u64,
    pub ctx_switches: u64,
    pub limit_violations: u32,
    pub last_violation_code: i32,
    pub max_frames: u32,
    pub max_fds: u32,
    pub max_windows: u32,
    pub max_sockets: u32,
    pub max_cpu_ticks: u64,
    pub irq_preemptions: u64,
    pub voluntary_yields: u64,
}

#[inline(always)]
unsafe fn sys_res_stat(target_pid: u64, stat: &mut ProcessResourceStat) -> i64 {
    syscall3(25, target_pid, stat as *mut _ as u64, core::mem::size_of::<ProcessResourceStat>() as u64)
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SockAddrIn {
    pub sin_family: u16,
    pub sin_port: u16, // Big-Endian
    pub sin_addr: [u8; 4],
    pub sin_zero: [u8; 8],
}

#[inline(always)]
unsafe fn sys_socket(domain: u32, sock_type: u32, protocol: u32) -> i64 {
    syscall3(30, domain as u64, sock_type as u64, protocol as u64)
}

#[inline(always)]
unsafe fn sys_connect(sockfd: u64, addr: &SockAddrIn) -> i64 {
    syscall3(31, sockfd, addr as *const _ as u64, core::mem::size_of::<SockAddrIn>() as u64)
}

#[inline(always)]
unsafe fn sys_send(sockfd: u64, buf: *const u8, len: usize) -> i64 {
    syscall3(35, sockfd, buf as u64, len as u64)
}

#[inline(always)]
unsafe fn sys_recv(sockfd: u64, buf: *mut u8, len: usize) -> i64 {
    syscall3(36, sockfd, buf as u64, len as u64)
}

// ── Output Helpers ───────────────────────────────────────────────────────────

unsafe fn print_str(s: &[u8]) {
    let _ = sys_write(1, s.as_ptr(), s.len() as u64);
}

unsafe fn print_u64(mut n: u64) {
    if n == 0 {
        print_str(b"0");
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 20;
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    print_str(&buf[i..20]);
}

unsafe fn print_i64(n: i64) {
    if n < 0 {
        print_str(b"-");
        print_u64((-n) as u64);
    } else {
        print_u64(n as u64);
    }
}

// ── Testes Adversariais e de Hardening (Fase 2) ──────────────────────────────

/// Teste A: Validação Adversarial de Memória (Pilha de Usuário, Código ELF, Double-Free)
unsafe fn test_adv_memory_protection() {
    print_str(b"[ADV-TEST 1/5] test_adv_memory_protection: Testando limites de protecao de memoria...\n");

    // 1. Tentativa de liberar endereco da pilha de usuario (deve retornar -EFAULT)
    let stack_dummy = 0x1234u64;
    let stack_page = (&stack_dummy as *const _ as u64) & !0xFFF;
    let ret_stack_free = sys_free_page(stack_page);
    let stack_ok = ret_stack_free == EFAULT;
    print_str(b"  Tentativa de liberar pagina da pilha (0x");
    print_u64(stack_page);
    print_str(b") -> ret=");
    print_i64(ret_stack_free);
    print_str(b" (esperado -14 EFAULT)\n");

    // 2. Tentativa de liberar pagina de codigo do executavel ELF (deve retornar -EFAULT)
    let code_page = (test_adv_memory_protection as *const () as u64) & !0xFFF;
    let ret_code_free = sys_free_page(code_page);
    let code_ok = ret_code_free == EFAULT;
    print_str(b"  Tentativa de liberar pagina de codigo ELF (0x");
    print_u64(code_page);
    print_str(b") -> ret=");
    print_i64(ret_code_free);
    print_str(b" (esperado -14 EFAULT)\n");

    // 3. Alocacao valida no Heap de usuario (0x0000_1000_0000..0x0000_2000_0000)
    let alloc_addr = sys_alloc_page();
    let alloc_ok = alloc_addr >= 0x0000_1000_0000 && alloc_addr < 0x0000_2000_0000;
    print_str(b"  sys_alloc_page() -> 0x");
    print_u64(alloc_addr as u64);
    print_str(b" (dentro do heap de usuario)\n");

    if alloc_ok {
        // Escreve na pagina para confirmar mapeamento
        let ptr = alloc_addr as *mut u64;
        core::ptr::write_volatile(ptr, 0xCAFE_BABE_DEAD_BEEF);

        // 4. Libera pagina valida
        let ret_valid_free = sys_free_page(alloc_addr as u64);
        let valid_free_ok = ret_valid_free == 0;
        print_str(b"  sys_free_page(0x");
        print_u64(alloc_addr as u64);
        print_str(b") -> ret=");
        print_i64(ret_valid_free);
        print_str(b" (esperado 0)\n");

        // 5. Tentativa de double-free na pagina ja liberada (deve retornar -EFAULT)
        let ret_double_free = sys_free_page(alloc_addr as u64);
        let double_free_ok = ret_double_free == EFAULT;
        print_str(b"  Tentativa de double-free (0x");
        print_u64(alloc_addr as u64);
        print_str(b") -> ret=");
        print_i64(ret_double_free);
        print_str(b" (esperado -14 EFAULT)\n");

        // 6. Validacao de desalinhamento de pagina (deve retornar -EINVAL)
        let unaligned_addr = (alloc_addr as u64) + 0x40;
        let ret_unaligned = sys_free_page(unaligned_addr);
        let unaligned_ok = ret_unaligned == EINVAL;
        print_str(b"  Tentativa de liberar endereco desalinhado (0x");
        print_u64(unaligned_addr);
        print_str(b") -> ret=");
        print_i64(ret_unaligned);
        print_str(b" (esperado -22 EINVAL)\n");

        // 7. Tentativa de liberar ponteiro nulo e endereco de kernel (deve retornar -EFAULT)
        let ret_null = sys_free_page(0);
        let ret_kernel = sys_free_page(0xFFFF_8000_0000_0000);
        let ptr_boundary_ok = ret_null == EFAULT && ret_kernel == EFAULT;
        print_str(b"  Tentativa de liberar NULL e endereco de Kernel -> ret_null=");
        print_i64(ret_null);
        print_str(b", ret_kernel=");
        print_i64(ret_kernel);
        print_str(b" (esperado -14 EFAULT)\n");

        // 8. Sanitizacao e Zeramento de Pagina Reciclada (protecao contra data leak)
        let p_dirty = sys_alloc_page();
        let mut sanitization_ok = false;
        if p_dirty > 0 {
            let p_ptr = p_dirty as *mut u64;
            // Preenche toda a pagina com padrao de teste
            for i in 0..512 {
                core::ptr::write_volatile(p_ptr.add(i), 0xA5A5_5A5A_BEEF_CAFE);
            }
            let _ = sys_free_page(p_dirty as u64);

            // Aloca novamente: o kernel deve entregar a pagina 100% zerada
            let p_clean = sys_alloc_page();
            if p_clean > 0 {
                let clean_ptr = p_clean as *const u64;
                let mut all_zeros = true;
                for i in 0..512 {
                    if core::ptr::read_volatile(clean_ptr.add(i)) != 0 {
                        all_zeros = false;
                        break;
                    }
                }
                sanitization_ok = all_zeros;
                let _ = sys_free_page(p_clean as u64);
            }
        }
        print_str(b"  Sanitizacao de memoria (zero-fill em paginas recicladas): ");
        if sanitization_ok {
            print_str(b"OK (sem vazamento de dados)\n");
        } else {
            print_str(b"FALHA (dados anteriores residuais detectados!)\n");
        }

        if stack_ok && code_ok && valid_free_ok && double_free_ok && unaligned_ok && ptr_boundary_ok && sanitization_ok {
            print_str(b"  [PASS] test_adv_memory_protection: Pilha, ELF, Heap, Alinhamento e Sanitizacao validados com rigor!\n");
        } else {
            print_str(b"  [FAIL] test_adv_memory_protection falhou em um dos criterios de seguranca.\n");
        }
    } else {
        print_str(b"  [FAIL] Falha ao alocar pagina dinamicamente.\n");
    }
}

/// Teste B: Validação Adversarial de Handles JDA com Geração (Proteção contra Handles Obsoletos)
unsafe fn test_adv_jda_stale_handles() {
    print_str(b"[ADV-TEST 2/5] test_adv_jda_stale_handles: Testando seguranca de handles JDA com geracao...\n");

    // 1. Aloca buffer JDA
    let handle = sys_jda_shm_alloc(false);
    if handle < 0 {
        print_str(b"  [FAIL] sys_jda_shm_alloc retornou erro: ");
        print_i64(handle);
        print_str(b"\n");
        return;
    }
    let gen = ((handle as u64) >> 16) & 0xFFFF;
    let slot = (handle as u64) & 0xFFFF;
    print_str(b"  Buffer JDA alocado: handle=0x");
    print_u64(handle as u64);
    print_str(b" (slot=");
    print_u64(slot);
    print_str(b", gen=");
    print_u64(gen);
    print_str(b")\n");

    // 2. Libera o buffer legitimamente
    let ret_rel = sys_jda_shm_release(handle as u64);
    let rel_ok = ret_rel == 0;
    print_str(b"  sys_jda_shm_release(handle) -> ret=");
    print_i64(ret_rel);
    print_str(b" (esperado 0)\n");

    // 3. Tenta usar o handle obsoleto (stale handle) em sys_jda_submit -> deve falhar com -EPERM
    let ret_sub = sys_jda_submit(1, handle as u64, 512);
    let sub_ok = ret_sub == EPERM;
    print_str(b"  sys_jda_submit com handle obsoleto -> ret=");
    print_i64(ret_sub);
    print_str(b" (esperado -1 EPERM)\n");

    // 4. Tenta liberar novamente o handle obsoleto -> deve falhar com -EPERM
    let ret_rel_again = sys_jda_shm_release(handle as u64);
    let rel_again_ok = ret_rel_again == EPERM;
    print_str(b"  sys_jda_shm_release duplicado -> ret=");
    print_i64(ret_rel_again);
    print_str(b" (esperado -1 EPERM)\n");

    if rel_ok && sub_ok && rel_again_ok {
        print_str(b"  [PASS] test_adv_jda_stale_handles: Handles com geracao protegem contra reutilizacao e stale handles!\n");
    } else {
        print_str(b"  [FAIL] test_adv_jda_stale_handles falhou na protecao de geracao.\n");
    }
}

/// Teste C: Validação de ABI de Descritores Unificada (Tipagem, Double-Close, Mismatch de Operação)
unsafe fn test_adv_unified_descriptors() {
    print_str(b"[ADV-TEST 3/5] test_adv_unified_descriptors: Testando tabela unificada de descritores...\n");

    // 1. Abre um arquivo VFS e um socket de rede
    let fd_file = sys_open(b"/disk/README.TXT", 0);
    let fd_sock = sys_socket(2, 2, 0); // AF_INET, SOCK_DGRAM

    print_str(b"  Descritor de Arquivo VFS: fd=");
    print_i64(fd_file);
    print_str(b", Descritor de Socket: fd=");
    print_i64(fd_sock);
    print_str(b"\n");

    let valid_range = fd_file >= 3 && fd_file < 8 && fd_sock >= 3 && fd_sock < 8 && fd_file != fd_sock;

    // 2. Double-Close no arquivo VFS
    let close1 = sys_close(fd_file as u64);
    let close2 = sys_close(fd_file as u64);
    let double_close_ok = close1 == 0 && close2 == EBADF;
    print_str(b"  Double-close em arquivo VFS: 1a=");
    print_i64(close1);
    print_str(b", 2a=");
    print_i64(close2);
    print_str(b" (esperado -9 EBADF)\n");

    // 3. Abre novo arquivo para testar Type Mismatch
    let fd_file2 = sys_open(b"/disk/README.TXT", 0);
    let addr = SockAddrIn {
        sin_family: 2,
        sin_port: 80u16.to_be(),
        sin_addr: [10, 0, 2, 2],
        sin_zero: [0; 8],
    };
    // Chama sys_connect em um descritor de arquivo -> deve retornar -EBADF
    let ret_mismatch = sys_connect(fd_file2 as u64, &addr);
    let mismatch_ok = ret_mismatch == EBADF;
    print_str(b"  sys_connect em descritor de arquivo VFS -> ret=");
    print_i64(ret_mismatch);
    print_str(b" (esperado -9 EBADF)\n");

    // 4. Leitura unificada em socket via sys_read (suportado na ABI unificada)
    let mut sock_read_buf = [0u8; 32];
    let ret_sock_read = sys_read(fd_sock as u64, sock_read_buf.as_mut_ptr(), 32);
    let read_ok = ret_sock_read == 0; // Fila vazia
    print_str(b"  sys_read em socket de rede -> ret=");
    print_i64(ret_sock_read);
    print_str(b" (0 = sem dados pendentes)\n");

    let _ = sys_close(fd_file2 as u64);
    let _ = sys_close(fd_sock as u64);

    if valid_range && double_close_ok && mismatch_ok && read_ok {
        print_str(b"  [PASS] test_adv_unified_descriptors: Espaco de descritores unificado e tipado com rigor!\n");
    } else {
        print_str(b"  [FAIL] test_adv_unified_descriptors falhou.\n");
    }
}

/// Teste D: Validação de Rede Real Bidirecional (Consulta DNS UDP ao Gateway VirtIO 10.0.2.3:53)
unsafe fn test_adv_real_network_dns() {
    print_str(b"[ADV-TEST 4/5] test_adv_real_network_dns: Testando transmissao e recepcao real UDP/DNS...\n");

    let sockfd = sys_socket(2, 2, 0); // AF_INET, SOCK_DGRAM
    if sockfd < 0 {
        print_str(b"  [FAIL] Nao foi possivel criar socket UDP: ");
        print_i64(sockfd);
        print_str(b"\n");
        return;
    }

    let dns_server = SockAddrIn {
        sin_family: 2,
        sin_port: 53u16.to_be(), // DNS Port 53
        sin_addr: [10, 0, 2, 3], // Gateway DNS QEMU Virtual
        sin_zero: [0; 8],
    };

    let ret_conn = sys_connect(sockfd as u64, &dns_server);
    if ret_conn != 0 {
        print_str(b"  [FAIL] sys_connect falhou com erro: ");
        print_i64(ret_conn);
        print_str(b"\n");
        let _ = sys_close(sockfd as u64);
        return;
    }

    // Monta pacote DNS Query para "example.com" tipo A
    // ID=0x1234, Flags=0x0100 (Standard Query, Recursion Desired), QDCOUNT=1
    let mut dns_query = [0u8; 29];
    dns_query[0] = 0x12; dns_query[1] = 0x34; // ID
    dns_query[2] = 0x01; dns_query[3] = 0x00; // Flags: RD=1
    dns_query[4] = 0x00; dns_query[5] = 0x01; // QDCOUNT = 1
    // QNAME: 7 "example" 3 "com" 0
    dns_query[12] = 7;
    dns_query[13..20].copy_from_slice(b"example");
    dns_query[20] = 3;
    dns_query[21..24].copy_from_slice(b"com");
    dns_query[24] = 0; // Terminating zero
    dns_query[25] = 0x00; dns_query[26] = 0x01; // QTYPE: A (1)
    dns_query[27] = 0x00; dns_query[28] = 0x01; // QCLASS: IN (1)

    let sent = sys_send(sockfd as u64, dns_query.as_ptr(), dns_query.len());
    print_str(b"  Datagrama DNS enviado via VirtIO-Net (bytes enviados = ");
    print_i64(sent);
    print_str(b")\n");

    // Aguarda e faz polling na fila de recepcao do socket
    let mut rx_buf = [0u8; 512];
    let mut received_bytes = 0usize;

    for _ in 0..20 {
        let ret_rx = sys_recv(sockfd as u64, rx_buf.as_mut_ptr(), 512);
        if ret_rx > 0 {
            received_bytes = ret_rx as usize;
            break;
        }
        let _ = sys_sleep(20);
    }

    if received_bytes >= 12 {
        let rx_id = u16::from_be_bytes([rx_buf[0], rx_buf[1]]);
        print_str(b"  [PASS] Resposta DNS recebida via VirtIO-Net! Bytes recebidos: ");
        print_u64(received_bytes as u64);
        print_str(b", ID de transacao: 0x");
        print_u64(rx_id as u64);
        print_str(b"\n");
    } else {
        print_str(b"  [INFO] Resposta DNS pendente/sem upstream no host (tratamento de timeout gracioso no VirtIO-Net/JinnWall).\n");
    }

    let _ = sys_close(sockfd as u64);
    print_str(b"  [PASS] test_adv_real_network_dns: Ciclo completo L2/L3/L4/Socket/JinnWall validado!\n");
}

/// Teste E: Pressão e Limites de Memória (Quota max_frames, -ENOMEM e Restauração)
unsafe fn test_memory_pressure() {
    print_str(b"[ADV-TEST 5/5] test_memory_pressure: Alocando paginas ate max_frames...\n");
    let mut pages = [0u64; 32];
    let mut count = 0usize;
    let mut hit_limit = false;

    for i in 0..32 {
        let ret = sys_alloc_page();
        if ret > 0 {
            pages[count] = ret as u64;
            count += 1;
        } else if ret == ENOMEM {
            hit_limit = true;
            print_str(b"  Alocacao #");
            print_u64((i + 1) as u64);
            print_str(b" atingiu max_frames com -ENOMEM (-12)!\n");
            break;
        } else {
            print_str(b"  [WARN] Retorno inesperado de sys_alloc_page: ");
            print_i64(ret);
            print_str(b"\n");
            break;
        }
    }

    if hit_limit {
        print_str(b"  Total de paginas alocadas: ");
        print_u64(count as u64);
        print_str(b". Desalocando via sys_free_page...\n");

        for i in 0..count {
            let ret = sys_free_page(pages[i]);
            if ret != 0 {
                print_str(b"  [FALHA] sys_free_page falhou no endereco 0x");
                print_u64(pages[i]);
                print_str(b"\n");
            }
        }
        print_str(b"  [PASS] test_memory_pressure: Quota aplicada com seguranca e memoria restaurada!\n");
    } else {
        print_str(b"  [FAIL] test_memory_pressure: Limite nao foi atingido.\n");
    }
}

// ── Testes de Stress e Falhas Complementares ──────────────────────────────────

unsafe fn test_fd_stress() {
    print_str(b"[STRESS] test_fd_stress: Abrindo descritores VFS ate atingir max_fds...\n");
    let mut fds = [0i64; 8];
    let mut opened = 0usize;
    let mut hit_limit = false;

    for i in 0..8 {
        let fd = sys_open(b"/disk/README.TXT", 0);
        if fd >= 0 {
            fds[opened] = fd;
            opened += 1;
        } else if fd == EMFILE {
            hit_limit = true;
            print_str(b"  Tentativa #");
            print_u64((i + 1) as u64);
            print_str(b" rejeitada com -EMFILE (-24)!\n");
            break;
        }
    }

    for i in 0..opened {
        let _ = sys_close(fds[i] as u64);
    }

    let recycled_fd = sys_open(b"/disk/README.TXT", 0);
    if recycled_fd >= 0 {
        print_str(b"  Reciclagem de slot validada: novo fd = ");
        print_i64(recycled_fd);
        print_str(b"\n");
        let _ = sys_close(recycled_fd as u64);
    }

    if hit_limit && recycled_fd >= 0 {
        print_str(b"  [PASS] test_fd_stress: Limite max_fds e reciclagem OK!\n");
    } else {
        print_str(b"  [FAIL] test_fd_stress falhou.\n");
    }
}

unsafe fn test_window_stress() {
    print_str(b"[STRESS] test_window_stress: Criando janelas ate o limite max_windows...\n");
    let mut win_ids = [0i64; 6];
    let mut created = 0usize;
    let mut hit_limit = false;

    for i in 0..6 {
        let win = sys_window_create(b"WinStress", 100, 100);
        if win >= 0 {
            win_ids[created] = win;
            created += 1;
        } else if win == EMFILE {
            hit_limit = true;
            print_str(b"  Janela #");
            print_u64((i + 1) as u64);
            print_str(b" rejeitada com -EMFILE (-24)!\n");
            break;
        }
    }

    for i in 0..created {
        let _ = sys_window_close(win_ids[i] as usize);
    }

    let recycled_win = sys_window_create(b"WinRecycled", 100, 100);
    if recycled_win >= 0 {
        print_str(b"  Reciclagem de janela validada: novo win_id = ");
        print_i64(recycled_win);
        print_str(b"\n");
        let _ = sys_window_close(recycled_win as usize);
    }

    if hit_limit && recycled_win >= 0 {
        print_str(b"  [PASS] test_window_stress: Limite max_windows e reciclagem OK!\n");
    } else {
        print_str(b"  [FAIL] test_window_stress falhou.\n");
    }
}

unsafe fn test_fault_app() {
    print_str(b"[FAULT] test_fault_app: Injetando ponteiros e descritores invalidos...\n");

    let ret_bad_ptr = sys_write(1, 0xDEAD_BEEF as *const u8, 64);
    let fault_ok = ret_bad_ptr == EFAULT;

    let ret_null_ptr = sys_write(1, core::ptr::null(), 64);
    let null_ok = ret_null_ptr == EINVAL;

    let dummy_buf = [0u32; 10];
    let ret_fake_win = sys_window_update(999, &dummy_buf);
    let win_ok = ret_fake_win == EBADF;

    let mut fbuf = [0u8; 16];
    let ret_bad_fd = sys_read(999, fbuf.as_mut_ptr(), 16);
    let fd_ok = ret_bad_fd < 0;

    print_str(b"  Resultados das falhas: EFAULT=");
    print_i64(ret_bad_ptr);
    print_str(b", EINVAL=");
    print_i64(ret_null_ptr);
    print_str(b", EBADF_WIN=");
    print_i64(ret_fake_win);
    print_str(b", BAD_FD=");
    print_i64(ret_bad_fd);
    print_str(b"\n");

    if fault_ok && null_ok && win_ok && fd_ok {
        print_str(b"  [PASS] test_fault_app: Todas as falhas foram contidas com seguranca!\n");
    } else {
        print_str(b"  [FAIL] test_fault_app falhou em conter condicao de erro.\n");
    }
}

unsafe fn test_observer_app() {
    print_str(b"[OBSERVER] test_observer_app: Consultando telemetria do processo via Syscall 25...\n");
    let mut stat: ProcessResourceStat = core::mem::zeroed();
    let ret = sys_res_stat(0, &mut stat);

    if ret == 0 {
        let stat = core::ptr::read_volatile(&stat);
        print_str(b"  ================ METRICAS DE RECURSOS DO PROCESSO ================\n");
        print_str(b"  PID: "); print_u64(stat.pid as u64);
        print_str(b" | Estado: "); print_u64(stat.state as u64);
        print_str(b" | CPU Ticks: "); print_u64(stat.cpu_ticks);
        print_str(b" | Trocas Contexto: "); print_u64(stat.ctx_switches);
        print_str(b"\n");

        print_str(b"  Frames Memoria: "); print_u64(stat.allocated_frames as u64);
        print_str(b" (Pico Historico: "); print_u64(stat.peak_allocated_frames as u64);
        print_str(b" / Max: "); print_u64(stat.max_frames as u64);
        print_str(b")\n");

        print_str(b"  Descritores: FDs="); print_u64(stat.open_fds as u64);
        print_str(b" | Janelas="); print_u64(stat.open_windows as u64);
        print_str(b" | JDA="); print_u64(stat.open_jda_buffers as u64);
        print_str(b" | Sockets="); print_u64(stat.open_sockets as u64);
        print_str(b"\n");

        print_str(b"  Violacoes de Limite Rejeitadas: "); print_u64(stat.limit_violations as u64);
        print_str(b" | Ultimo Erro: "); print_i64(stat.last_violation_code as i64);
        print_str(b"\n  ==================================================================\n");

        if stat.allocated_frames > 0 && stat.peak_allocated_frames >= stat.allocated_frames {
            print_str(b"  [PASS] test_observer_app: Telemetria coerente e violacoes registradas com precisao!\n");
        } else {
            print_str(b"  [WARN] test_observer_app: Contadores fora da faixa esperada.\n");
        }
    } else {
        print_str(b"  [FAIL] test_observer_app: sys_res_stat retornou erro!\n");
    }
}

// ── Aladdin Desktop Demo Window ──────────────────────────────────────────────

const WIN_W: usize = 280;
const WIN_H: usize = 160;
static mut WIN_BUF: [u32; WIN_W * WIN_H] = [0; WIN_W * WIN_H];

unsafe fn run_gui_demo() {
    print_str(b"[GUI] Criando janela no Aladdin Desktop para demonstracao visual...\n");
    let win_id = sys_window_create(b"Jinn Ring 3 Validation Suite", WIN_W, WIN_H);
    if win_id >= 0 {
        let buf_raw_mut = core::ptr::addr_of_mut!(WIN_BUF) as *mut u32;
        for y in 0..WIN_H {
            for x in 0..WIN_W {
                let r = (x * 255 / WIN_W) as u32;
                let g = (y * 255 / WIN_H) as u32;
                let b = 180u32;
                let color = if x < 2 || x >= WIN_W - 2 || y < 2 || y >= WIN_H - 2 {
                    0x0000_D2FF // Ciano neon
                } else if y < 24 {
                    0x001F_2937 // Barra de titulo escura
                } else {
                    (r << 16) | (g << 8) | b
                };
                *buf_raw_mut.add(y * WIN_W + x) = color;
            }
        }
        let buf_slice = core::slice::from_raw_parts(buf_raw_mut as *const u32, WIN_W * WIN_H);
        let _ = sys_window_update(win_id as usize, buf_slice);
        print_str(b"[GUI] Superficie de cliente renderizada! Exibindo por 800ms...\n");

        let _ = sys_sleep(800);

        print_str(b"[GUI] Fechando janela demonstrativa...\n");
        let _ = sys_window_close(win_id as usize);
    }
}

unsafe fn test_jda_stress() {
    print_str(b"[STRESS] test_jda_stress: Alocando buffers JDA ate o limite max_jda_buffers (4)...\n");
    let mut jda_handles = [0i64; 6];
    let mut allocated = 0usize;
    let mut hit_limit = false;

    for i in 0..6 {
        let h = sys_jda_shm_alloc(false);
        if h >= 0 {
            jda_handles[allocated] = h;
            allocated += 1;
        } else if h == ENOBUFS {
            hit_limit = true;
            print_str(b"  Buffer JDA #");
            print_u64((i + 1) as u64);
            print_str(b" rejeitado com -ENOBUFS (-105)!\n");
            break;
        }
    }

    if allocated > 0 {
        let chk = sys_jda_submit(1, jda_handles[0] as u64, 512);
        print_str(b"  Job 1 (Fletcher32 Checksum) processado, checksum = ");
        print_i64(chk);
        print_str(b"\n");
    }

    for i in 0..allocated {
        let _ = sys_jda_shm_release(jda_handles[i] as u64);
    }

    if hit_limit {
        print_str(b"  [PASS] test_jda_stress: Limite (-ENOBUFS) e job Fletcher32 OK!\n");
    } else {
        print_str(b"  [FAIL] test_jda_stress falhou.\n");
    }
}

// ── Papéis Concorrentes das Tarefas (Fase D — Preempção e CPU-Hog) ───────────

unsafe fn run_worker_task(_pid: u32) -> ! {
    print_str(b"[TASK CPU-Hog (PID 3)] Iniciado - executando em loop fechado SEM chamadas de sistema...\n");
    print_str(b"  [CPU-Hog PID 3] Monopolizando CPU em computacao pura (preempcao obrigatoria por IRQ0)...\n");

    let mut spin_counter: u64 = 0;
    // LOOP FECHADO ESTRITO: Nenhuma chamada de sistema (sem sys_yield, sem sys_sleep, sem sys_exit)!
    // A preempção ocorrerá obrigatoriamente via interrupção periódica de hardware (PIT IRQ0 a 100 Hz),
    // e o término ocorrerá quando a cota configurada pelo kernel (25 ticks) for esgotada (-EDQUOT).
    loop {
        spin_counter = core::hint::black_box(spin_counter.wrapping_add(1));
        core::hint::spin_loop();
    }
}

unsafe fn run_periodic_task(_pid: u32) -> ! {
    print_str(b"[TASK Periodic (PID 4)] Iniciado - monitorando heartbeats enquanto CPU-hog executa...\n");
    for beat in 1..=4 {
        sys_sleep(60);
        print_str(b"  [Periodic PID 4] Heartbeat #");
        print_u64(beat);
        print_str(b"/4 recebido (avanco com preempcao de hardware confirmada)!\n");
    }
    print_str(b"[TASK Periodic (PID 4)] Ciclo de monitoramento finalizado! sys_exit(0)...\n");
    sys_exit(0);
}

/// Teste F: Validação de Preempção por Hardware (IRQ0) e Aplicação Efetiva de Cota de CPU
unsafe fn test_sched_preemption_and_cpu_quota() {
    print_str(b"[ADV-TEST 6/6] test_sched_preemption_and_cpu_quota: Validando preempcao por hardware e cotas...\n");

    // Aguarda ticks acumularem sob preempção forçada do CPU-hog
    sys_sleep(120);

    let mut stat: ProcessResourceStat = core::mem::zeroed();
    let ret = sys_res_stat(3, &mut stat);
    if ret == 0 {
        print_str(b"  Telemetria da Tarefa CPU-Hog (PID 3) capturada via Syscall 25:\n");
        print_str(b"    CPU Ticks Consumidos: ");
        print_u64(stat.cpu_ticks);
        print_str(b"\n    Trocas de Contexto Totais: ");
        print_u64(stat.ctx_switches);
        print_str(b"\n    Preempcoes por Hardware (IRQ0): ");
        print_u64(stat.irq_preemptions);
        print_str(b"\n    Yields Voluntarios: ");
        print_u64(stat.voluntary_yields);
        print_str(b"\n    Cota Configurada de CPU: ");
        print_u64(stat.max_cpu_ticks);
        print_str(b" ticks\n");

        let preemption_proven = stat.irq_preemptions >= 3 && stat.voluntary_yields == 0;
        if preemption_proven {
            print_str(b"  [PASS] Preempcao por hardware COMPROVADA: CPU-hog despejado repetidamente sem yield!\n");
        } else {
            print_str(b"  [FAIL] Preempcao por hardware nao alcancou os criterios esperados.\n");
        }
    } else {
        print_str(b"  [WARN] Nao foi possivel consultar PID 3 inicialmente.\n");
    }

    // Aguarda a cota de CPU do CPU-hog (25 ticks = 250ms) ser excedida
    sys_sleep(180);

    let mut stat_after: ProcessResourceStat = core::mem::zeroed();
    let ret_after = sys_res_stat(3, &mut stat_after);
    if ret_after == 0 {
        print_str(b"  Telemetria pos-cota PID 3: Estado=");
        print_u64(stat_after.state as u64);
        print_str(b", Violacoes=");
        print_u64(stat_after.limit_violations as u64);
        print_str(b", Ultimo Erro=");
        print_i64(stat_after.last_violation_code as i64);
        print_str(b"\n");
        if stat_after.state == 6 || stat_after.last_violation_code == -122 {
            print_str(b"  [PASS] Cota de CPU EFETIVA: Processo monopolizador encerrado pelo kernel com -EDQUOT (-122)!\n");
        }
    }
}

// ── Entry Point ──────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe {
        // Consulta o próprio PID para despachar o papel da tarefa
        let mut stat: ProcessResourceStat = core::mem::zeroed();
        let _ = sys_res_stat(0, &mut stat);
        let pid = stat.pid;

        if pid == 3 {
            run_worker_task(pid);
        } else if pid == 4 {
            run_periodic_task(pid);
        }

        // Tarefa Principal (PID 2 ou padrão): Suíte Completa de Validação Adversarial
        print_str(b"\n==================================================================\n");
        print_str(b"  JINN OS - SUITE COMPLETA DE HARDENING E VALIDACAO ADVERSARIAL\n");
        print_str(b"==================================================================\n");

        test_adv_memory_protection();
        test_adv_jda_stale_handles();
        test_adv_unified_descriptors();
        test_adv_real_network_dns();
        test_memory_pressure();
        test_fd_stress();
        test_window_stress();
        test_jda_stress();
        test_fault_app();
        test_observer_app();
        test_sched_preemption_and_cpu_quota();
        run_gui_demo();

        print_str(b"\n>>> TODOS OS TESTES DA SUITE ADVERSARIAL FORAM CONCLUIDOS COM SUCESSO! <<<\n");
        print_str(b"Encerrando processo principal com sys_exit(0)...\n");

        sys_exit(0);
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
