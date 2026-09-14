# Assembly stubs for all CPU exceptions (0-31) and IRQs (32-47).
# Each stub pushes the interrupt vector number onto the stack, aligns the frame,
# and calls the central Rust dispatcher: interrupt_dispatch(vector: u64, frame: *mut InterruptFrame).

.intel_syntax noprefix
.section .text

# ── Helper macro: exception WITH error code ──────────────────────────────────
.macro exc_err vec
    .global exc\vec\()_stub
    exc\vec\()_stub:
        push \vec
        jmp exception_common
.endm

# ── Helper macro: exception WITHOUT error code ────────────────────────────────
.macro exc_noerr vec
    .global exc\vec\()_stub
    exc\vec\()_stub:
        push 0          # dummy error code
        push \vec
        jmp exception_common
.endm

# ── Helper macro: IRQ (no error code) ────────────────────────────────────────
.macro irq_stub vec
    .global irq\vec\()_stub
    irq\vec\()_stub:
        push 0
        push \vec
        jmp irq_common
.endm

# ── CPU Exception stubs (vectors 0–31) ───────────────────────────────────────
exc_noerr 0    # Divide By Zero
exc_noerr 1    # Debug
# Vector 2 = NMI: benigno em VMs - apenas retorna sem haltar
.global exc2_stub
exc2_stub:
    iretq
# --- end NMI stub ---
exc_noerr 3    # Breakpoint
exc_noerr 4    # Overflow
exc_noerr 5    # Bound Range Exceeded
exc_noerr 6    # Invalid Opcode
exc_noerr 7    # Device Not Available
exc_err   8    # Double Fault
exc_noerr 9    # Coprocessor Segment Overrun
exc_err   10   # Invalid TSS
exc_err   11   # Segment Not Present
exc_err   12   # Stack-Segment Fault
exc_err   13   # General Protection Fault
exc_err   14   # Page Fault
exc_noerr 15   # Reserved
exc_noerr 16   # x87 FP Exception
exc_err   17   # Alignment Check
exc_noerr 18   # Machine Check
exc_noerr 19   # SIMD FP Exception
exc_noerr 20   # Virtualization Exception
exc_noerr 21   # Control Protection Exception
exc_noerr 22   # Reserved
exc_noerr 23   # Reserved
exc_noerr 24   # Reserved
exc_noerr 25   # Reserved
exc_noerr 26   # Reserved
exc_noerr 27   # Reserved
exc_noerr 28   # Hypervisor Injection Exception
exc_noerr 29   # VMM Communication Exception
exc_err   30   # Security Exception
exc_noerr 31   # Reserved

# ── IRQ stubs (vectors 32–47) ─────────────────────────────────────────────────
irq_stub 32    # IRQ0 — Timer (PIT)
irq_stub 33    # IRQ1 — PS/2 Keyboard
irq_stub 34    # IRQ2 — Cascade
irq_stub 35    # IRQ3 — COM2
irq_stub 36    # IRQ4 — COM1
irq_stub 37    # IRQ5 — LPT2
irq_stub 38    # IRQ6 — Floppy
irq_stub 39    # IRQ7 — LPT1
irq_stub 40    # IRQ8 — CMOS RTC
irq_stub 41    # IRQ9 — Free
irq_stub 42    # IRQ10 — Free
irq_stub 43    # IRQ11 — Free
irq_stub 44    # IRQ12 — PS/2 Mouse
irq_stub 45    # IRQ13 — FPU
irq_stub 46    # IRQ14 — ATA Primary
irq_stub 47    # IRQ15 — ATA Secondary

# ── Common exception handler ──────────────────────────────────────────────────
exception_common:
    # Stack at this point: [error_code, vector, rip, cs, rflags, rsp, ss]
    mov rdi, [rsp + 8]   # vector
    mov rsi, [rsp]       # error code
    mov rdx, rsp         # frame pointer (arg3)

    mov rbp, rsp
    and rsp, -16
    call exception_dispatch
    mov rsp, rbp
    
    cli
.exc_halt:
    hlt
    jmp .exc_halt

# ── Common IRQ handler ────────────────────────────────────────────────────────
irq_common:
    # Save all caller-saved + callee-saved registers
    push rax
    push rcx
    push rdx
    push rbx
    push rbp
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15

    mov rdi, [rsp + 15*8]        # vector number
    mov rsi, rsp                  # frame pointer

    # Align stack to 16 bytes
    mov rbp, rsp
    and rsp, -16
    call irq_dispatch
    mov rsp, rbp

    # Check if a context switch is requested
    test rax, rax
    jz .no_ctx_switch
    mov rsp, rax
.no_ctx_switch:

    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rbp
    pop rbx
    pop rdx
    pop rcx
    pop rax

    # Clean up the vector + dummy error code
    add rsp, 16
    iretq

# ── Syscall stub (vetor 0x80 — int 0x80 vindo do Ring 3 ou Ring 0) ───────────
#
# Convenção de chamada do USUÁRIO (in):
#   RAX = número da syscall
#   RDI = arg0
#   RSI = arg1
#   RDX = arg2
#
# Convenção System V AMD64 ABI para syscall_dispatch(nr, arg0, arg1, arg2):
#   RDI = nr    ← RAX do usuário
#   RSI = arg0  ← RDI do usuário
#   RDX = arg1  ← RSI do usuário
#   RCX = arg2  ← RDX do usuário
#
# Retorno: RAX = valor de retorno (negativo = errno)
.global isr128_stub
isr128_stub:
    # Salva registradores caller-saved que o Rust pode usar internamente
    push r11
    push r10
    push r9
    push r8
    push rbp
    push rbx

    # Remapeia argumentos: (RAX, RDI, RSI, RDX) → (RDI, RSI, RDX, RCX)
    mov rcx, rdx       # arg2 → RCX (4º parâmetro SysV)
    mov rdx, rsi       # arg1 → RDX (3º parâmetro SysV)
    mov rsi, rdi       # arg0 → RSI (2º parâmetro SysV)
    mov rdi, rax       # nr   → RDI (1º parâmetro SysV)

    # Alinha a pilha a 16 bytes (obrigatório pela ABI antes de CALL)
    mov rbp, rsp
    and rsp, -16
    call syscall_dispatch
    mov rsp, rbp

    # Restaura registradores
    pop rbx
    pop rbp
    pop r8
    pop r9
    pop r10
    pop r11
    iretq

