#![no_std]
#![no_main]

use core::{arch::asm, panic::PanicInfo};

#[inline(always)]
unsafe fn syscall3(nr: u64, arg0: u64, arg1: u64, arg2: u64) -> i64 {
    let ret: i64;
    asm!(
        "int 0x80",
        inout("rax") nr => ret,
        in("rdi") arg0,
        in("rsi") arg1,
        in("rdx") arg2,
        options(nostack, nomem)
    );
    ret
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
unsafe fn sys_open(path: &[u8], flags: u32) -> i64 {
    syscall3(15, path.as_ptr() as u64, path.len() as u64, flags as u64)
}

#[inline(always)]
unsafe fn sys_close(fd: u64) -> i64 {
    syscall3(16, fd, 0, 0)
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
unsafe fn sys_exit(code: i32) -> ! {
    loop {
        syscall3(0, code as u64, 0, 0);
    }
}

const WIN_W: usize = 280;
const WIN_H: usize = 160;
static mut WIN_BUF: [u32; WIN_W * WIN_H] = [0; WIN_W * WIN_H];

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let msg1 = b"[U3] ola-mundo ativo em Ring 3!\n";
    let msg_sec = b"[U3] Teste seguranca: validando rejeicao de ponteiro invalido...\n";
    let msg_sec_ok = b"[U3] Syscall 1 rejeitou ponteiro 0xDEADBEEF com -EFAULT (-14)!\n";
    let msg_file = b"[U3] Testando VFS via Syscall 15 open / 2 read (/disk/README.TXT)...\n";
    let msg2 = b"[U3] Solicitando buffer JDA Zero-Copy (Syscall 12 alloc)...\n";
    let msg3 = b"[U3] Submetendo Job JDA Zero-Copy (Syscall 14: Fletcher32 Checksum)...\n";
    let msg4 = b"[U3] Job JDA processado com sucesso! Liberando (Syscall 13 release)...\n";
    let msg5 = b"[U3] Pipeline JDA Zero-Copy validado!\n";
    let msg_win = b"[U3] Criando janela no Aladdin Desktop (Syscall 20)...\n";
    let msg_win_ok = b"[U3] Janela criada com sucesso! Desenhando superficie de cliente...\n";
    let msg_win_up = b"[U3] Buffer de pixels enviado (Syscall 21). Exibindo por 1000ms...\n";
    let msg_win_cl = b"[U3] Fechando janela (Syscall 22)...\n";
    let msg6 = b"[U3] Concluido com sucesso! Encerrando processo.\n";
    unsafe {
        let _ = sys_write(1, msg1.as_ptr(), msg1.len() as u64);

        // Teste de Segurança: passagem de ponteiro não mapeado (0xDEADBEEF) para o kernel
        let _ = sys_write(1, msg_sec.as_ptr(), msg_sec.len() as u64);
        let ret_bad = sys_write(1, 0xDEAD_BEEF as *const u8, 100);
        if ret_bad == -14 { // -EFAULT
            let _ = sys_write(1, msg_sec_ok.as_ptr(), msg_sec_ok.len() as u64);
        }

        // Teste de VFS em Ring 3
        let _ = sys_write(1, msg_file.as_ptr(), msg_file.len() as u64);
        let fd = sys_open(b"/disk/README.TXT", 0);
        if fd >= 0 {
            let mut fbuf = [0u8; 64];
            let n = sys_read(fd as u64, fbuf.as_mut_ptr(), 64);
            if n > 0 {
                let msg_ok = b"[U3] Leitura VFS bem-sucedida! Conteudo: ";
                let _ = sys_write(1, msg_ok.as_ptr(), msg_ok.len() as u64);
                let _ = sys_write(1, fbuf.as_ptr(), n as u64);
                let _ = sys_write(1, b"\n".as_ptr(), 1);
            }
            let _ = sys_close(fd as u64);
        }

        let _ = sys_write(1, msg2.as_ptr(), msg2.len() as u64);

        let handle = sys_jda_shm_alloc(false);
        if handle >= 0 {
            let _ = sys_write(1, msg3.as_ptr(), msg3.len() as u64);
            let checksum = sys_jda_submit(1, handle as u64, 512);
            if checksum >= 0 {
                let _ = sys_write(1, msg4.as_ptr(), msg4.len() as u64);
            }
            let _ = sys_jda_shm_release(handle as u64);
            let _ = sys_write(1, msg5.as_ptr(), msg5.len() as u64);
        }

        // Teste de Contabilidade e Métricas de Recursos (Syscall 25)
        let mut stat: ProcessResourceStat = core::mem::zeroed();
        let ret_stat = sys_res_stat(0, &mut stat);
        if ret_stat == 0 {
            let msg_res_ok = b"[U3] Syscall 25 res_stat OK! Frames alocados: ";
            let _ = sys_write(1, msg_res_ok.as_ptr(), msg_res_ok.len() as u64);
            let frames_char = (b'0' + ((stat.allocated_frames % 10) as u8));
            let _ = sys_write(1, &frames_char as *const u8, 1);
            let _ = sys_write(1, b"\n".as_ptr(), 1);
        } else {
            let msg_res_err = b"[U3] Syscall 25 falhou com erro!\n";
            let _ = sys_write(1, msg_res_err.as_ptr(), msg_res_err.len() as u64);
        }

        // Teste de Limites de Recursos: alocar buffers JDA ate atingir o limite max_jda_buffers (4)
        let mut jda_handles = [0i64; 5];
        for i in 0..4 {
            jda_handles[i] = sys_jda_shm_alloc(false);
        }
        // A 5a tentativa deve falhar com -ENOBUFS (-105)
        let overflow_jda = sys_jda_shm_alloc(false);
        if overflow_jda == -105 {
            let msg_lim_ok = b"[U3] Limite de buffers JDA (4) atingido com sucesso: kernel retornou -ENOBUFS (-105)!\n";
            let _ = sys_write(1, msg_lim_ok.as_ptr(), msg_lim_ok.len() as u64);
        }
        // Libera os 4 handles alocados
        for i in 0..4 {
            if jda_handles[i] >= 0 {
                let _ = sys_jda_shm_release(jda_handles[i] as u64);
            }
        }

        // Teste de Sockets e Rede em Ring 3 (Syscalls 30, 31, 35)
        let sockfd = sys_socket(2, 2, 0); // AF_INET (2), SOCK_DGRAM (2)
        if sockfd >= 0 {
            let msg_sock_ok = b"[U3] Syscall 30 socket criada com sucesso!\n";
            let _ = sys_write(1, msg_sock_ok.as_ptr(), msg_sock_ok.len() as u64);

            let server_addr = SockAddrIn {
                sin_family: 2,
                sin_port: 80u16.to_be(),
                sin_addr: [10, 0, 2, 2], // Gateway / Host QEMU
                sin_zero: [0; 8],
            };
            let ret_conn = sys_connect(sockfd as u64, &server_addr);
            if ret_conn == 0 {
                let msg_conn_ok = b"[U3] Syscall 31 connect bem-sucedido para 10.0.2.2:80!\n";
                let _ = sys_write(1, msg_conn_ok.as_ptr(), msg_conn_ok.len() as u64);

                let req = b"PING / HTTP/1.0\r\n\r\n";
                let sent = sys_send(sockfd as u64, req.as_ptr(), req.len());
                if sent > 0 {
                    let msg_send_ok = b"[U3] Syscall 35 send transmitiu datagrama pela VirtIO-Net!\n";
                    let _ = sys_write(1, msg_send_ok.as_ptr(), msg_send_ok.len() as u64);
                }
            }
        }

        // Teste de Janela Gráfica no Aladdin Desktop via Syscalls 20, 21, 22
        let ret_fake_win = sys_window_update(99, core::slice::from_raw_parts(core::ptr::addr_of!(WIN_BUF) as *const u32, WIN_W * WIN_H));
        if ret_fake_win == -9 { // -EBADF
            let msg_win_ebadf = b"[U3] Syscall 21 rejeitou ID de janela inexistente com -EBADF (-9)!\n";
            let _ = sys_write(1, msg_win_ebadf.as_ptr(), msg_win_ebadf.len() as u64);
        }

        let _ = sys_write(1, msg_win.as_ptr(), msg_win.len() as u64);
        let win_id = sys_window_create(b"Janela Ring 3 Demo", WIN_W, WIN_H);
        if win_id >= 0 {
            let _ = sys_write(1, msg_win_ok.as_ptr(), msg_win_ok.len() as u64);

            let buf_raw_mut = core::ptr::addr_of_mut!(WIN_BUF) as *mut u32;
            for y in 0..WIN_H {
                for x in 0..WIN_W {
                    let r = (x * 255 / WIN_W) as u32;
                    let g = (y * 255 / WIN_H) as u32;
                    let b = 180u32;
                    let color = if x < 2 || x >= WIN_W - 2 || y < 2 || y >= WIN_H - 2 {
                        0x0000_D2FF
                    } else {
                        (r << 16) | (g << 8) | b
                    };
                    *buf_raw_mut.add(y * WIN_W + x) = color;
                }
            }
            let buf_slice = core::slice::from_raw_parts(buf_raw_mut as *const u32, WIN_W * WIN_H);
            let _ = sys_window_update(win_id as usize, buf_slice);
            let _ = sys_write(1, msg_win_up.as_ptr(), msg_win_up.len() as u64);

            let _ = sys_sleep(1000);

            let _ = sys_write(1, msg_win_cl.as_ptr(), msg_win_cl.len() as u64);
            let _ = sys_window_close(win_id as usize);
        }

        let _ = sys_sleep(200);
        let _ = sys_write(1, msg6.as_ptr(), msg6.len() as u64);
        sys_exit(0);
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
