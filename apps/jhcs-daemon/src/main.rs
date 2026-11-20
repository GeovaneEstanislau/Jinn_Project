//! Jinn Heterogeneous Compute Service (JHCS) — Ring 3 Daemon
//!
//! Provides cooperative heterogeneous compute services (CPU Scalar & SIMD SSE2)
//! to user-space processes (Aladdin Desktop, audio subsystem, client applications)
//! over Jinn OS IPC and Zero-Copy Shared Memory (JDA Shmem).
//!
//! Operates under strict Zero-Idle policy: consumes 0% CPU cycles when no
//! computation requests are queued.

#![no_std]
#![no_main]

extern crate alloc;

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use aladdin_sdk::console::{print, print_num, println};
use aladdin_sdk::ipc::{ipc_recv, ipc_send, shm_get_vaddr, Message};
use aladdin_sdk::process::{exit, getpid};
use aladdin_sdk::time::sleep;

use jhcs_broker::dispatcher::ComputeBroker;
use jhcs_broker::policy::DispatchPolicy;
use jhcs_core::buffer::BufferSet;
use jhcs_core::capabilities::{BackendKind, ComputeRequirements, ParallelismClass};
use jhcs_core::task::{ComputeTask, TaskId, OP_MATMUL, OP_REDUCE_SUM, OP_VECTOR_ADD};
use jhcs_platform_jinn::JinnPlatformAdapter;

// ── Allocator de Heap em Espaço de Usuário (.bss) ─────────────────────────────
// 4 MiB alocados na seção .bss da tarefa (0 bytes na imagem ELF em disco).
// Suporta alinhamento de até 64 bytes para operações SIMD.
const HEAP_SIZE: usize = 4 * 1024 * 1024;
static mut HEAP_MEMORY: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];

const MIN_ALIGN: usize = 64;

#[repr(C, align(64))]
struct StaticScratchBuffer {
    buf_a: [f32; 1024],
    buf_b: [f32; 1024],
    buf_c: [f32; 1024],
}

static mut STATIC_SCRATCH: StaticScratchBuffer = StaticScratchBuffer {
    buf_a: [1.0f32; 1024],
    buf_b: [2.0f32; 1024],
    buf_c: [0.0f32; 1024],
};

#[repr(C)]
struct FreeNode {
    size: usize,
    next: *mut FreeNode,
}

struct SimpleHeapAllocator {
    offset: AtomicUsize,
    free_list: core::cell::UnsafeCell<*mut FreeNode>,
    lock: AtomicBool,
}

unsafe impl Sync for SimpleHeapAllocator {}

impl SimpleHeapAllocator {
    const fn new() -> Self {
        Self {
            offset: AtomicUsize::new(0),
            free_list: core::cell::UnsafeCell::new(core::ptr::null_mut()),
            lock: AtomicBool::new(false),
        }
    }

    fn acquire(&self) {
        while self
            .lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
    }

    fn release(&self) {
        self.lock.store(false, Ordering::Release);
    }

    unsafe fn try_alloc_free_list(&self, layout: Layout) -> *mut u8 {
        let align = layout.align().max(MIN_ALIGN);
        let size = (layout.size() + MIN_ALIGN - 1) & !(MIN_ALIGN - 1);

        let head = &mut *self.free_list.get();
        let mut prev: *mut *mut FreeNode = head as *mut _;
        let mut cur = *head;

        while !cur.is_null() {
            let node_size = (*cur).size;
            let addr = cur as usize;
            let aligned_start = (addr + align - 1) & !(align - 1);
            let wasted = aligned_start - addr;

            if node_size >= wasted + size {
                let remaining = node_size - wasted - size;
                *prev = (*cur).next;

                if remaining >= core::mem::size_of::<FreeNode>() {
                    let leftover = (aligned_start + size) as *mut FreeNode;
                    (*leftover).size = remaining;
                    (*leftover).next = *prev;
                    *prev = leftover;
                }

                return aligned_start as *mut u8;
            }

            prev = &mut (*cur).next as *mut _;
            cur = (*cur).next;
        }

        core::ptr::null_mut()
    }

    unsafe fn alloc_bump(&self, layout: Layout) -> *mut u8 {
        let align = layout.align().max(MIN_ALIGN);
        let size = layout.size();
        let current = self.offset.load(Ordering::Relaxed);
        let aligned = (current + align - 1) & !(align - 1);
        let next = aligned + size;

        if next > HEAP_SIZE {
            println("[jhcs-daemon] [OOM] Heap esgotado!");
            return core::ptr::null_mut();
        }

        self.offset.store(next, Ordering::Relaxed);
        let base = core::ptr::addr_of_mut!(HEAP_MEMORY) as *mut u8;
        base.add(aligned)
    }
}

unsafe impl GlobalAlloc for SimpleHeapAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.acquire();
        let ptr = {
            let from_free = self.try_alloc_free_list(layout);
            if !from_free.is_null() {
                from_free
            } else {
                self.alloc_bump(layout)
            }
        };
        self.release();
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            return;
        }
        self.acquire();
        let size = (layout.size() + MIN_ALIGN - 1) & !(MIN_ALIGN - 1);
        let node = ptr as *mut FreeNode;
        (*node).size = size.max(core::mem::size_of::<FreeNode>());

        let head = &mut *self.free_list.get();
        (*node).next = *head;
        *head = node;

        self.release();
    }
}

#[global_allocator]
static ALLOCATOR: SimpleHeapAllocator = SimpleHeapAllocator::new();

// ── Protocolo IPC JHCS ────────────────────────────────────────────────────────

pub const TAG_JHCS_REQ: u32 = 0x4A484353;  // 'JHCS'
pub const TAG_JHCS_RESP: u32 = 0x4A484352; // 'JHCR'

pub const REQ_OP_MATMUL: u32 = 1;
pub const REQ_OP_VECTOR_ADD: u32 = 2;
pub const REQ_OP_REDUCE_SUM: u32 = 3;
pub const REQ_OP_PING_TELEMETRY: u32 = 4;
pub const REQ_OP_SHUTDOWN: u32 = 0xFF;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct JhcsIpcRequest {
    pub magic: u32,             // 0x4A484353
    pub op: u32,                // REQ_OP_*
    pub shm_handle: u32,        // Handle JDA Shmem ou 0 para inline
    pub m: u32,                 // Dimensão M / comprimento do vetor
    pub k: u32,                 // Dimensão K
    pub n: u32,                 // Dimensão N
    pub flags: u32,             // Flags de política (ex: prefer SIMD)
    pub inline_data: [f32; 9], // Parâmetros/dados curtos inline (36 bytes)
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct JhcsIpcResponse {
    pub magic: u32,              // 0x4A484352
    pub status: i32,             // 0 = Sucesso, negativo = Erro
    pub backend: u32,            // 1 = CpuScalar, 2 = CpuSimd, 3 = Gpu
    pub _align_pad: u32,         // Alinhamento para u64
    pub exec_cycles: u64,        // Duração aproximada em ciclos
    pub result_val: f32,         // Resultado escalar / checksum
    pub processed_elements: u32, // Quantidade de elementos computados
    pub _pad: [u8; 32],          // Totaliza exatamente 64 bytes
}

const _: () = assert!(core::mem::size_of::<JhcsIpcRequest>() == 64);
const _: () = assert!(core::mem::size_of::<JhcsIpcResponse>() == 64);

// ── Ponto de Entrada do Daemon ────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn _start() -> ! {
    println("[jhcs-daemon] ==================================================");
    println("[jhcs-daemon] Jinn Heterogeneous Compute Service (JHCS) Daemon");
    print("[jhcs-daemon] PID do Daemon Ring 3: ");
    print_num(getpid());
    println("");

    // 1. Inicializa o Adapter de Plataforma nativo do Jinn OS e o Broker
    let _platform = JinnPlatformAdapter::new();
    let mut broker = ComputeBroker::new().with_policy(DispatchPolicy::ZeroGpuOptimized);

    println("[jhcs-daemon] ComputeBroker inicializado com sucesso.");
    println("[jhcs-daemon] Politica de Despacho: ZeroGpuOptimized (SIMD / Custo-Zero Ativo)");
    println("[jhcs-daemon] Suporte Vetorial: SSE2 Ativo (128-bit seguro via fxsave64)");
    println("[jhcs-daemon] Modo Zero-Idle Ativado: Aguardando requisicoes IPC...");

    let mut msg = Message::zero();
    let mut tasks_served: u64 = 0;

    // 2. Loop de Eventos IPC com Política Zero-Idle Estrita
    loop {
        match ipc_recv(&mut msg) {
            Ok(true) => {
                if msg.tag == TAG_JHCS_REQ {
                    tasks_served = tasks_served.wrapping_add(1);
                    handle_ipc_request(&mut broker, &msg, tasks_served);
                }
            }
            Ok(false) => {
                // Fila vazia: cede quantum e entra em estado de baixo consumo.
                // sys_sleep(10) transita a tarefa para Sleeping, garantindo 0% de CPU ociosa.
                sleep(10);
            }
            Err(e) => {
                print("[jhcs-daemon] Erro na recepcao de IPC: ");
                print_num((-e) as u64);
                println("");
                sleep(20);
            }
        }
    }
}

fn handle_ipc_request(broker: &mut ComputeBroker, msg: &Message, req_idx: u64) {
    if core::mem::size_of::<JhcsIpcRequest>() > msg.payload.len() {
        return;
    }

    let req = unsafe { core::ptr::read_unaligned(msg.payload.as_ptr() as *const JhcsIpcRequest) };
    if req.magic != TAG_JHCS_REQ {
        return;
    }

    let start_tsc = unsafe { core::arch::x86_64::_rdtsc() };

    let mut resp = JhcsIpcResponse {
        magic: TAG_JHCS_RESP,
        status: 0,
        backend: 2, // Simd
        _align_pad: 0,
        exec_cycles: 0,
        result_val: 0.0,
        processed_elements: 0,
        _pad: [0u8; 32],
    };

    match req.op {
        REQ_OP_PING_TELEMETRY => {
            resp.status = 0;
            resp.backend = if broker.simd_backend().has_sse() { 2 } else { 1 };
            resp.result_val = req_idx as f32;
            resp.processed_elements = 0;
            print("[jhcs-daemon] [TELEMETRY] Ping atendido para PID ");
            print_num(msg.pid_from as u64);
            println("");
        }

        REQ_OP_MATMUL => {
            let m = req.m as usize;
            let k = req.k as usize;
            let n = req.n as usize;

            if req.shm_handle != 0 {
                // Operação Zero-Copy com Shmem
                if let Ok(vaddr) = shm_get_vaddr(req.shm_handle as usize) {
                    let total_a = m * k;
                    let total_b = k * n;
                    let total_c = m * n;

                    let ptr_a = vaddr as *const f32;
                    let ptr_b = unsafe { ptr_a.add(total_a) };
                    let ptr_c = unsafe { (vaddr as *mut f32).add(total_a + total_b) };

                    let a = unsafe { core::slice::from_raw_parts(ptr_a, total_a) };
                    let b = unsafe { core::slice::from_raw_parts(ptr_b, total_b) };
                    let c = unsafe { core::slice::from_raw_parts_mut(ptr_c, total_c) };

                    let task = ComputeTask::new(
                        TaskId::new(req_idx),
                        OP_MATMUL,
                        BufferSet::new(),
                        BufferSet::new(),
                    )
                    .with_requirements(ComputeRequirements {
                        min_memory_bytes: (total_a + total_b + total_c) * 4,
                        parallelism: ParallelismClass::Vectorized128,
                        precision: jhcs_core::buffer::Precision::Float32,
                        allow_fallback: true,
                    });

                    let outcome = broker.execute_matmul(&task, a, b, c, m, k, n);
                    resp.backend = match outcome.executed_by {
                        BackendKind::CpuSimd => 2,
                        BackendKind::CpuScalar => 1,
                        _ => 0,
                    };
                    resp.processed_elements = total_c as u32;
                    // Checksum dos cantos
                    resp.result_val = if !c.is_empty() { c[0] + c[c.len() - 1] } else { 0.0 };
                } else {
                    resp.status = -1; // EFAULT
                }
            } else {
                // Operação com buffer local Ring 3 (Static Scratch Buffer .bss, zero heap allocations)
                let m = m.max(2).min(16);
                let k = k.max(2).min(16);
                let n = n.max(2).min(16);
                let total_a = (m * k).min(1024);
                let total_b = (k * n).min(1024);
                let total_c = (m * n).min(1024);

                let scratch = unsafe { &mut *core::ptr::addr_of_mut!(STATIC_SCRATCH) };
                for val in scratch.buf_a[..total_a].iter_mut() { *val = 1.0f32; }
                for val in scratch.buf_b[..total_b].iter_mut() { *val = 2.0f32; }
                for val in scratch.buf_c[..total_c].iter_mut() { *val = 0.0f32; }

                let task = ComputeTask::new(
                    TaskId::new(req_idx),
                    OP_MATMUL,
                    BufferSet::new(),
                    BufferSet::new(),
                )
                .with_requirements(ComputeRequirements {
                    min_memory_bytes: 0,
                    parallelism: ParallelismClass::Vectorized128,
                    precision: jhcs_core::buffer::Precision::Float32,
                    allow_fallback: true,
                });

                let outcome = broker.execute_matmul(&task, &scratch.buf_a[..total_a], &scratch.buf_b[..total_b], &mut scratch.buf_c[..total_c], m, k, n);
                resp.backend = match outcome.executed_by {
                    BackendKind::CpuSimd => 2,
                    BackendKind::CpuScalar => 1,
                    _ => 0,
                };
                resp.processed_elements = total_c as u32;
                resp.result_val = scratch.buf_c[0];
            }

            print("[jhcs-daemon] [MATMUL] Executado ");
            print_num(m as u64);
            print("x");
            print_num(n as u64);
            print(" elementos=");
            print_num(resp.processed_elements as u64);
            print(" backend=");
            print_num(resp.backend as u64);
            println("");
        }

        REQ_OP_VECTOR_ADD => {
            let len = (req.m as usize).max(4).min(1024);
            let scratch = unsafe { &mut *core::ptr::addr_of_mut!(STATIC_SCRATCH) };
            for val in scratch.buf_a[..len].iter_mut() { *val = req.inline_data[0]; }
            for val in scratch.buf_b[..len].iter_mut() { *val = req.inline_data[1]; }
            for val in scratch.buf_c[..len].iter_mut() { *val = 0.0f32; }

            let task = ComputeTask::new(
                TaskId::new(req_idx),
                OP_VECTOR_ADD,
                BufferSet::new(),
                BufferSet::new(),
            )
            .with_requirements(ComputeRequirements {
                min_memory_bytes: 0,
                parallelism: ParallelismClass::Vectorized128,
                precision: jhcs_core::buffer::Precision::Float32,
                allow_fallback: true,
            });

            let outcome = broker.execute_vector_add(&task, &scratch.buf_a[..len], &scratch.buf_b[..len], &mut scratch.buf_c[..len]);
            resp.backend = match outcome.executed_by {
                BackendKind::CpuSimd => 2,
                _ => 1,
            };
            resp.processed_elements = len as u32;
            resp.result_val = scratch.buf_c[0];

            print("[jhcs-daemon] [VECTOR_ADD] Executado len=");
            print_num(len as u64);
            print(" c[0]=");
            print_num(resp.result_val as u64);
            println("");
        }

        REQ_OP_REDUCE_SUM => {
            let len = (req.m as usize).min(9);
            let slice = &req.inline_data[..len];

            let task = ComputeTask::new(
                TaskId::new(req_idx),
                OP_REDUCE_SUM,
                BufferSet::new(),
                BufferSet::new(),
            );

            let mut sum = 0.0f32;
            let outcome = broker.execute_reduce_sum(&task, slice, &mut sum);
            resp.backend = match outcome.executed_by {
                BackendKind::CpuSimd => 2,
                _ => 1,
            };
            resp.processed_elements = len as u32;
            resp.result_val = sum;

            print("[jhcs-daemon] [REDUCE_SUM] Soma=");
            print_num(sum as u64);
            println("");
        }

        REQ_OP_SHUTDOWN => {
            println("[jhcs-daemon] [SHUTDOWN] Requisicao de encerramento recebida.");
            exit(0);
        }

        _ => {
            resp.status = -22; // EINVAL
        }
    }

    let end_tsc = unsafe { core::arch::x86_64::_rdtsc() };
    resp.exec_cycles = end_tsc.saturating_sub(start_tsc);

    let payload_bytes = unsafe {
        core::slice::from_raw_parts(
            &resp as *const JhcsIpcResponse as *const u8,
            core::mem::size_of::<JhcsIpcResponse>(),
        )
    };

    let mut send_buf = [0u8; 64];
    send_buf.copy_from_slice(payload_bytes);

    let _ = ipc_send(msg.pid_from, TAG_JHCS_RESP, &send_buf);
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    print("[jhcs-daemon] [PANIC] ");
    if let Some(loc) = info.location() {
        print(loc.file());
        print(":");
        print_num(loc.line() as u64);
    }
    println("");
    exit(101);
}
