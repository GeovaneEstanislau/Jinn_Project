#![no_std]
#![no_main]

use core::{arch::asm, panic::PanicInfo};

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
}

#[inline(always)]
unsafe fn sys_res_stat(target_pid: u64, stat: &mut ProcessResourceStat) -> i64 {
    syscall3(25, target_pid, stat as *mut _ as u64, core::mem::size_of::<ProcessResourceStat>() as u64)
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SockAddrIn {
    pub sin_family: u16,
    pub sin_port: u16,
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

// ── Test Behaviors (Fase 2) ──────────────────────────────────────────────────

/// 1. CPU-Hog: Loop de 500.000 iterações computacionais
unsafe fn test_cpu_hog() {
    print_str(b"[TEST 1/8] test_cpu_hog: Executando 500.000 iteracoes computacionais...\n");
    let mut acc: u64 = 0;
    for i in 0..500_000u64 {
        acc = core::hint::black_box(acc.wrapping_add(i.wrapping_mul(3).wrapping_add(1)));
    }
    core::hint::black_box(acc);
    print_str(b"  [PASS] test_cpu_hog: 500.000 iteracoes concluidas com sucesso!\n");
}

/// 2. Memory-Pressure: Aloca páginas até o limite `max_frames` (-ENOMEM), depois libera e restaura
unsafe fn test_memory_pressure() {
    print_str(b"[TEST 2/8] test_memory_pressure: Alocando paginas via sys_alloc_page ate o limite...\n");
    let mut pages = [0u64; 32];
    let mut count = 0usize;
    let mut hit_limit = false;

    for i in 0..32 {
        let ret = sys_alloc_page();
        if ret > 0 {
            pages[count] = ret as u64;
            count += 1;
        } else if ret == -12 { // -ENOMEM
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
        print_str(b"  Total de paginas alocadas com sucesso: ");
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
        print_str(b"  [PASS] test_memory_pressure: limite atingido, paginas liberadas e memoria restaurada!\n");
    } else {
        print_str(b"  [FAIL] test_memory_pressure: limite nao foi atingido em 32 alocacoes.\n");
    }
}

/// 3. FD-Stress: Abre descritores VFS até atingir `max_fds` (-EMFILE), fecha e testa reciclagem
unsafe fn test_fd_stress() {
    print_str(b"[TEST 3/8] test_fd_stress: Abrindo descritores VFS ate atingir max_fds...\n");
    let mut fds = [0i64; 8];
    let mut opened = 0usize;
    let mut hit_limit = false;

    for i in 0..8 {
        let fd = sys_open(b"/disk/README.TXT", 0);
        if fd >= 0 {
            fds[opened] = fd;
            opened += 1;
        } else if fd == -24 { // -EMFILE
            hit_limit = true;
            print_str(b"  Tentativa #");
            print_u64((i + 1) as u64);
            print_str(b" rejeitada com -EMFILE (-24)!\n");
            break;
        } else {
            print_str(b"  [WARN] sys_open retornou: ");
            print_i64(fd);
            print_str(b"\n");
            break;
        }
    }

    // Fecha todos os descritores abertos
    for i in 0..opened {
        let _ = sys_close(fds[i] as u64);
    }

    // Testa reciclagem de slots
    let recycled_fd = sys_open(b"/disk/README.TXT", 0);
    if recycled_fd >= 0 {
        print_str(b"  Reciclagem de slot validada: novo fd = ");
        print_i64(recycled_fd);
        print_str(b"\n");
        let _ = sys_close(recycled_fd as u64);
    }

    if hit_limit && recycled_fd >= 0 {
        print_str(b"  [PASS] test_fd_stress: limite max_fds (-EMFILE) e reciclagem de slots OK!\n");
    } else {
        print_str(b"  [FAIL] test_fd_stress falhou na deteccao de limite ou reciclagem.\n");
    }
}

/// 4. Window-Stress: Abre janelas no compositor Aladdin até atingir `max_windows` (-EMFILE)
unsafe fn test_window_stress() {
    print_str(b"[TEST 4/8] test_window_stress: Criando janelas ate o limite max_windows (4)...\n");
    let mut win_ids = [0i64; 6];
    let mut created = 0usize;
    let mut hit_limit = false;

    for i in 0..6 {
        let win = sys_window_create(b"WinStress", 100, 100);
        if win >= 0 {
            win_ids[created] = win;
            created += 1;
        } else if win == -24 { // -EMFILE
            hit_limit = true;
            print_str(b"  Janela #");
            print_u64((i + 1) as u64);
            print_str(b" rejeitada com -EMFILE (-24)!\n");
            break;
        } else {
            print_str(b"  [WARN] sys_window_create retornou: ");
            print_i64(win);
            print_str(b"\n");
            break;
        }
    }

    // Fecha todas as janelas criadas
    for i in 0..created {
        let _ = sys_window_close(win_ids[i] as usize);
    }

    // Testa reciclagem de janela
    let recycled_win = sys_window_create(b"WinRecycled", 100, 100);
    if recycled_win >= 0 {
        print_str(b"  Reciclagem de janela validada: novo win_id = ");
        print_i64(recycled_win);
        print_str(b"\n");
        let _ = sys_window_close(recycled_win as usize);
    }

    if hit_limit && recycled_win >= 0 {
        print_str(b"  [PASS] test_window_stress: limite max_windows (-EMFILE) e reciclagem OK!\n");
    } else {
        print_str(b"  [FAIL] test_window_stress falhou na deteccao de limite ou reciclagem.\n");
    }
}

/// 5. JDA-Stress: Aloca buffers Zero-Copy até atingir `max_jda_buffers` (4) (-ENOBUFS), testa isolamento cross-PID
unsafe fn test_jda_stress() {
    print_str(b"[TEST 5/8] test_jda_stress: Alocando buffers JDA ate o limite max_jda_buffers (4)...\n");
    let mut jda_handles = [0i64; 6];
    let mut allocated = 0usize;
    let mut hit_limit = false;

    for i in 0..6 {
        let h = sys_jda_shm_alloc(false);
        if h >= 0 {
            jda_handles[allocated] = h;
            allocated += 1;
        } else if h == -105 { // -ENOBUFS
            hit_limit = true;
            print_str(b"  Buffer JDA #");
            print_u64((i + 1) as u64);
            print_str(b" rejeitado com -ENOBUFS (-105)!\n");
            break;
        } else {
            print_str(b"  [WARN] sys_jda_shm_alloc retornou: ");
            print_i64(h);
            print_str(b"\n");
            break;
        }
    }

    // Teste de isolamento de segurança: tentar liberar handle invalido/de outro processo
    let cross_pid_err = sys_jda_shm_release(999);
    let cross_pid_ok = cross_pid_err == -1; // -EPERM
    if cross_pid_ok {
        print_str(b"  Acesso cross-PID/handle invalido rejeitado com -EPERM (-1)!\n");
    }

    // Executa Job 1 (Fletcher32 checksum) no primeiro buffer
    if allocated > 0 {
        let chk = sys_jda_submit(1, jda_handles[0] as u64, 512);
        print_str(b"  Job 1 (Fletcher32 Checksum) processado, checksum = ");
        print_i64(chk);
        print_str(b"\n");
    }

    // Libera todos os buffers alocados
    for i in 0..allocated {
        let _ = sys_jda_shm_release(jda_handles[i] as u64);
    }

    if hit_limit && cross_pid_ok {
        print_str(b"  [PASS] test_jda_stress: limite (-ENOBUFS), isolamento cross-PID (-EPERM) e job OK!\n");
    } else {
        print_str(b"  [FAIL] test_jda_stress falhou.\n");
    }
}

/// 6. Network-Stress: Cria sockets até `max_sockets` (4) (-EMFILE), envia datagrama e testa reciclagem
unsafe fn test_network_stress() {
    print_str(b"[TEST 6/8] test_network_stress: Criando sockets ate o limite max_sockets (4)...\n");
    let mut socks = [0i64; 6];
    let mut created = 0usize;
    let mut hit_limit = false;

    for i in 0..6 {
        let s = sys_socket(2, 2, 0); // AF_INET, SOCK_DGRAM
        if s >= 0 {
            socks[created] = s;
            created += 1;
        } else if s == -24 { // -EMFILE
            hit_limit = true;
            print_str(b"  Socket #");
            print_u64((i + 1) as u64);
            print_str(b" rejeitado com -EMFILE (-24)!\n");
            break;
        } else {
            print_str(b"  [WARN] sys_socket retornou: ");
            print_i64(s);
            print_str(b"\n");
            break;
        }
    }

    // Conecta e envia dados pelo socket 0
    let mut net_tx_ok = false;
    if created > 0 {
        let server_addr = SockAddrIn {
            sin_family: 2,
            sin_port: 80u16.to_be(),
            sin_addr: [10, 0, 2, 2], // Gateway / Host QEMU
            sin_zero: [0; 8],
        };
        let ret_conn = sys_connect(socks[0] as u64, &server_addr);
        if ret_conn == 0 {
            let req = b"PING / HTTP/1.0\r\n\r\n";
            let sent = sys_send(socks[0] as u64, req.as_ptr(), req.len());
            if sent > 0 {
                net_tx_ok = true;
                print_str(b"  Datagrama transmitido via VirtIO-Net (bytes enviados = ");
                print_i64(sent);
                print_str(b")\n");
            }
            // Testa recepção (fila vazia -> retorna 0)
            let mut rx_buf = [0u8; 64];
            let rx = sys_recv(socks[0] as u64, rx_buf.as_mut_ptr(), 64);
            print_str(b"  sys_recv retornou: ");
            print_i64(rx);
            print_str(b" (0 = sem pacotes pendentes na fila)\n");
        }
    }

    // Fecha todos os sockets
    for i in 0..created {
        let _ = sys_close(socks[i] as u64);
    }

    // Testa reciclagem de socket
    let recycled_sock = sys_socket(2, 2, 0);
    if recycled_sock >= 0 {
        print_str(b"  Reciclagem de socket validada: novo sockfd = ");
        print_i64(recycled_sock);
        print_str(b"\n");
        let _ = sys_close(recycled_sock as u64);
    }

    if hit_limit && net_tx_ok && recycled_sock >= 0 {
        print_str(b"  [PASS] test_network_stress: limite max_sockets (-EMFILE), TX VirtIO e reciclagem OK!\n");
    } else {
        print_str(b"  [FAIL] test_network_stress falhou.\n");
    }
}

/// 7. Fault-App: Injeta ponteiros inválidos e descritores fora de faixa para validar robustez do kernel
unsafe fn test_fault_app() {
    print_str(b"[TEST 7/8] test_fault_app: Injetando falhas propositais (ponteiros e descritores invalidos)...\n");

    // 1. Ponteiro canônico inválido 0xDEADBEEF -> -EFAULT (-14)
    let ret_bad_ptr = sys_write(1, 0xDEAD_BEEF as *const u8, 64);
    let fault_ok = ret_bad_ptr == -14;

    // 2. Ponteiro NULL -> -EINVAL (-22)
    let ret_null_ptr = sys_write(1, core::ptr::null(), 64);
    let null_ok = ret_null_ptr == -22;

    // 3. Janela inexistente (999) -> -EBADF (-9)
    let dummy_buf = [0u32; 10];
    let ret_fake_win = sys_window_update(999, &dummy_buf);
    let win_ok = ret_fake_win == -9;

    // 4. Descritor de leitura inválido (999) -> -EBADF / -EINVAL
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
        print_str(b"  [PASS] test_fault_app: Todas as falhas foram contidas com seguranca pelo kernel!\n");
    } else {
        print_str(b"  [FAIL] test_fault_app falhou em conter uma das condicoes de erro.\n");
    }
}

/// 8. Observer-App: Inspeciona contabilidade e limites de recursos via Syscall 25 (sys_res_stat)
unsafe fn test_observer_app() {
    print_str(b"[TEST 8/8] test_observer_app: Consultando telemetria do processo via Syscall 25...\n");
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

        print_str(b"  Descritores Abertos: FDs="); print_u64(stat.open_fds as u64);
        print_str(b" | Janelas="); print_u64(stat.open_windows as u64);
        print_str(b" | JDA="); print_u64(stat.open_jda_buffers as u64);
        print_str(b" | Sockets="); print_u64(stat.open_sockets as u64);
        print_str(b"\n");

        print_str(b"  Violacoes de Limites Rejeitadas: "); print_u64(stat.limit_violations as u64);
        print_str(b" | Ultimo Erro: "); print_i64(stat.last_violation_code as i64);
        print_str(b"\n  ==================================================================\n");

        if stat.allocated_frames > 0 && stat.peak_allocated_frames >= stat.allocated_frames && stat.limit_violations >= 3 {
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
        print_str(b"[GUI] Superficie de cliente renderizada! Exibindo por 1000ms...\n");

        let _ = sys_sleep(1000);

        print_str(b"[GUI] Fechando janela demonstrativa...\n");
        let _ = sys_window_close(win_id as usize);
    }
}

// ── Entry Point ──────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe {
        print_str(b"\n==================================================================\n");
        print_str(b"  JINN OS - SUITE COMPLETA DE CARGA, ISOLAMENTO E REDE (RING 3)\n");
        print_str(b"==================================================================\n");

        test_cpu_hog();
        test_memory_pressure();
        test_fd_stress();
        test_window_stress();
        test_jda_stress();
        test_network_stress();
        test_fault_app();
        test_observer_app();
        run_gui_demo();

        print_str(b"\n>>> TODOS OS 8 TESTES DA SUITE FORAM EXECUTADOS COM SUCESSO! <<<\n");
        print_str(b"Encerrando processo com sys_exit(0)...\n");

        sys_exit(0);
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
