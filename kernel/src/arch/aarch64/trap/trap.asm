// AArch64 trap trampoline.
// Mapped to TRAP_BOTTOM_ADDR in both kernel and user page tables.
.section .text.traper
.global __kernel_trap
.global __kernel_refume
.global kernel_traped_forbid
.global __aarch64_vector
.global __kernel_irq_entry

.equ TRAP_X0,         0
.equ TRAP_X2,        16
.equ TRAP_X4,        32
.equ TRAP_X6,        48
.equ TRAP_X8,        64
.equ TRAP_X10,       80
.equ TRAP_X12,       96
.equ TRAP_X14,      112
.equ TRAP_X16,      128
.equ TRAP_X18,      144
.equ TRAP_X20,      160
.equ TRAP_X22,      176
.equ TRAP_X24,      192
.equ TRAP_X26,      208
.equ TRAP_X28,      224
.equ TRAP_X30,      240
.equ TRAP_SP_EL0,   248
.equ TRAP_ELR_EL1,  256
.equ TRAP_SPSR_EL1, 264
.equ TRAP_TTBR_EL1, 272
.equ TRAP_KERNEL_SP, 280
.equ TRAP_HANDLER,  288

.macro SAVE_GPRS base
    stp x0, x1, [\base, #TRAP_X0]
    stp x2, x3, [\base, #TRAP_X2]
    stp x4, x5, [\base, #TRAP_X4]
    stp x6, x7, [\base, #TRAP_X6]
    stp x8, x9, [\base, #TRAP_X8]
    stp x10, x11, [\base, #TRAP_X10]
    stp x12, x13, [\base, #TRAP_X12]
    stp x14, x15, [\base, #TRAP_X14]
    stp x16, x17, [\base, #TRAP_X16]
    stp x18, x19, [\base, #TRAP_X18]
    stp x20, x21, [\base, #TRAP_X20]
    stp x22, x23, [\base, #TRAP_X22]
    stp x24, x25, [\base, #TRAP_X24]
    stp x26, x27, [\base, #TRAP_X26]
    stp x28, x29, [\base, #TRAP_X28]
    str x30, [\base, #TRAP_X30]
.endm

.macro RESTORE_GPRS base
    ldp x0, x1, [\base, #TRAP_X0]
    ldp x2, x3, [\base, #TRAP_X2]
    ldp x4, x5, [\base, #TRAP_X4]
    ldp x6, x7, [\base, #TRAP_X6]
    ldp x8, x9, [\base, #TRAP_X8]
    ldp x10, x11, [\base, #TRAP_X10]
    ldp x12, x13, [\base, #TRAP_X12]
    ldp x14, x15, [\base, #TRAP_X14]
    ldp x16, x17, [\base, #TRAP_X16]
    ldp x18, x19, [\base, #TRAP_X18]
    ldp x20, x21, [\base, #TRAP_X20]
    ldp x22, x23, [\base, #TRAP_X22]
    ldp x24, x25, [\base, #TRAP_X24]
    ldp x26, x27, [\base, #TRAP_X26]
    ldp x28, x29, [\base, #TRAP_X28]
    ldr x30, [\base, #TRAP_X30]
.endm

.macro SAVE_EL1_STATE base, t0, t1, t2
    mrs \t0, sp_el0
    mrs \t1, elr_el1
    mrs \t2, spsr_el1
    stp \t0, \t1, [\base, #TRAP_SP_EL0]
    str \t2, [\base, #TRAP_SPSR_EL1]
.endm

.macro RESTORE_EL1_STATE base, t0, t1, t2
    ldp \t0, \t1, [\base, #TRAP_SP_EL0]
    ldr \t2, [\base, #TRAP_SPSR_EL1]
    msr sp_el0, \t0
    msr elr_el1, \t1
    msr spsr_el1, \t2
.endm

.macro COPY_GPRS src, dst, t0, t1
    ldp \t0, \t1, [\src, #TRAP_X0]
    stp \t0, \t1, [\dst, #TRAP_X0]
    ldp \t0, \t1, [\src, #TRAP_X2]
    stp \t0, \t1, [\dst, #TRAP_X2]
    ldp \t0, \t1, [\src, #TRAP_X4]
    stp \t0, \t1, [\dst, #TRAP_X4]
    ldp \t0, \t1, [\src, #TRAP_X6]
    stp \t0, \t1, [\dst, #TRAP_X6]
    ldp \t0, \t1, [\src, #TRAP_X8]
    stp \t0, \t1, [\dst, #TRAP_X8]
    ldp \t0, \t1, [\src, #TRAP_X10]
    stp \t0, \t1, [\dst, #TRAP_X10]
    ldp \t0, \t1, [\src, #TRAP_X12]
    stp \t0, \t1, [\dst, #TRAP_X12]
    ldp \t0, \t1, [\src, #TRAP_X14]
    stp \t0, \t1, [\dst, #TRAP_X14]
    ldp \t0, \t1, [\src, #TRAP_X16]
    stp \t0, \t1, [\dst, #TRAP_X16]
    ldp \t0, \t1, [\src, #TRAP_X18]
    stp \t0, \t1, [\dst, #TRAP_X18]
    ldp \t0, \t1, [\src, #TRAP_X20]
    stp \t0, \t1, [\dst, #TRAP_X20]
    ldp \t0, \t1, [\src, #TRAP_X22]
    stp \t0, \t1, [\dst, #TRAP_X22]
    ldp \t0, \t1, [\src, #TRAP_X24]
    stp \t0, \t1, [\dst, #TRAP_X24]
    ldp \t0, \t1, [\src, #TRAP_X26]
    stp \t0, \t1, [\dst, #TRAP_X26]
    ldp \t0, \t1, [\src, #TRAP_X28]
    stp \t0, \t1, [\dst, #TRAP_X28]
    ldp \t0, \t1, [\src, #TRAP_X30]
    stp \t0, \t1, [\dst, #TRAP_X30]
.endm

.macro COPY_EL1_STATE src, dst, t0, t1
    ldp \t0, \t1, [\src, #TRAP_SP_EL0]
    stp \t0, \t1, [\dst, #TRAP_SP_EL0]
    ldr \t0, [\src, #TRAP_SPSR_EL1]
    str \t0, [\dst, #TRAP_SPSR_EL1]
.endm

// 2 KiB alignment for the AArch64 vector table.
.align 11
__aarch64_vector:
    // Current EL with SP_EL0
    .align 7
    b sync_el1_sp0
    .align 7
    b irq_el1_sp0
    .align 7
    b fiq_el1_sp0
    .align 7
    b serror_el1_sp0

    // Current EL with SP_ELx
    .align 7
    b __kernel_trap
    .align 7
    b __kernel_irq_entry
    .align 7
    b fiq_el1_spx
    .align 7
    b serror_el1_spx

    // Lower EL using AArch64
    .align 7
    b __user_sync_trap
    .align 7
    b __user_irq_trap
    .align 7
    b fiq_el0_64
    .align 7
    b serror_el0_64

    // Lower EL using AArch32
    .align 7
    b sync_el0_32
    .align 7
    b irq_el0_32
    .align 7
    b fiq_el0_32
    .align 7
    b serror_el0_32

// TrapContext layout (see trap/mod.rs)
//   x[31]       @ 0
//   sp_el0      @ 248
//   elr_el1     @ 256
//   spsr_el1    @ 264
//   ttbr_el1    @ 272   // kernel TTBR
//   kernel_sp   @ 280
//   trap_handler@ 288

.align 7
__kernel_trap:
    // Kernel synchronous trap: save the current EL1 frame on the current
    // kernel stack, then branch into the Rust kernel trap handler.
    sub sp, sp, #304

    SAVE_GPRS sp
    SAVE_EL1_STATE sp, x0, x1, x2

    mov x0, sp
    ldr x1, =kernel_trap_handler
    br x1

.align 7
__user_sync_trap:
    // EL0 -> EL1 arrives with:
    //   TTBR0_EL1 = user page table
    //   TTBR1_EL1 = kernel page table
    //   SP        = current task kernel_sp
    //
    // Save the full user frame to the kernel stack first. After that we are
    // free to reuse x0-x17 as scratch while copying into TrapContext.
    sub sp, sp, #272

    SAVE_GPRS sp
    SAVE_EL1_STATE sp, x0, x1, x2

    // Rust kernel text/data live in the kernel TTBR0 mapping, so restore
    // TTBR0_EL1 from TTBR1_EL1 before jumping to the Rust trap handler.
    mrs x10, ttbr1_el1
    msr ttbr0_el1, x10
    isb
    tlbi vmalle1is
    dsb ish
    isb

    // tpidr_el1 holds the kernel-accessible TrapContext* of the current task.
    // Copy the transient stack frame into that persistent context.
    mrs x9, tpidr_el1

    COPY_GPRS sp, x9, x0, x1
    COPY_EL1_STATE sp, x9, x0, x1

    ldr x10, =sync_el0_64
    br x10

.align 7
__user_irq_trap:
    // Same save/copy path as synchronous EL0 exceptions.
    sub sp, sp, #272

    SAVE_GPRS sp
    SAVE_EL1_STATE sp, x0, x1, x2

    mrs x10, ttbr1_el1
    msr ttbr0_el1, x10
    isb
    tlbi vmalle1is
    dsb ish
    isb

    mrs x9, tpidr_el1

    COPY_GPRS sp, x9, x0, x1
    COPY_EL1_STATE sp, x9, x0, x1

    ldr x10, =irq_el0_64
    br x10

.align 7
__kernel_refume:
    // x0 = TrapContext* (kernel direct-map pointer)
    // x1 = user TTBR0_EL1
    //
    // Return to EL0 with:
    //   TTBR0_EL1 = user page table
    //   TTBR1_EL1 = kernel page table
    //   SP        = task kernel_sp
    //   TPIDR_EL1 = TrapContext*
    //
    // Keep x18 as the final scratch pointer and restore its user value from a
    // small scratch area on the task kernel stack right before eret.
    msr tpidr_el1, x0

    ldr x9, [x0, #TRAP_KERNEL_SP] // kernel_sp
    mov sp, x9
    sub sp, sp, #80

    str x1, [sp, #0]           // user TTBR0_EL1
    ldp x9, x10, [x0, #TRAP_X0]
    stp x9, x10, [sp, #8]      // user x0-x1
    ldp x9, x10, [x0, #TRAP_X2]
    stp x9, x10, [sp, #24]     // user x2-x3
    ldp x9, x10, [x0, #TRAP_X4]
    stp x9, x10, [sp, #40]     // user x4-x5
    ldp x9, x10, [x0, #TRAP_X6]
    stp x9, x10, [sp, #56]     // user x6-x7
    ldr x9, [x0, #TRAP_X18]
    str x9, [sp, #72]          // user x18, restored last

    RESTORE_EL1_STATE x0, x9, x10, x11

    mov x18, x0

    ldp x8, x9, [x18, #TRAP_X8]
    ldp x10, x11, [x18, #TRAP_X10]
    ldp x12, x13, [x18, #TRAP_X12]
    ldp x14, x15, [x18, #TRAP_X14]
    ldp x16, x17, [x18, #TRAP_X16]
    ldp x19, x20, [x18, #152]
    ldp x21, x22, [x18, #168]
    ldp x23, x24, [x18, #184]
    ldp x25, x26, [x18, #200]
    ldp x27, x28, [x18, #216]
    ldp x29, x30, [x18, #232]

    ldr x9, [x18, #TRAP_TTBR_EL1] // kernel TTBR
    ldr x10, [sp, #0]          // user TTBR0_EL1
    msr ttbr1_el1, x9
    msr ttbr0_el1, x10
    isb
    tlbi vmalle1is
    dsb ish
    isb

    ldr x18, [sp, #72]
    ldp x0, x1, [sp, #8]
    ldp x2, x3, [sp, #24]
    ldp x4, x5, [sp, #40]
    ldp x6, x7, [sp, #56]
    add sp, sp, #80
    eret

.align 7
kernel_traped_forbid:
    b kernel_traped_forbid

.align 7
__kernel_irq_entry:
    // Kernel IRQ can interrupt ordinary EL1 code, or the final window of
    // __kernel_refume where TTBR0_EL1 temporarily points at the user table.
    // Save the interrupted EL1 frame and the current TTBR pair, switch
    // TTBR0_EL1 back to the kernel table for Rust, then restore everything
    // and eret back to the interrupted kernel PC.
    sub sp, sp, #288

    SAVE_GPRS sp
    SAVE_EL1_STATE sp, x0, x1, x2

    mrs x0, ttbr0_el1
    mrs x1, ttbr1_el1
    stp x0, x1, [sp, #TRAP_TTBR_EL1]

    // Raw PL011 probe. A register can hold the full address, but a single
    // instruction usually cannot encode an arbitrary 64-bit immediate.
    // 0x0900_0000 is special: one MOVZ is enough because only bits[31:16]
    // are non-zero.
    //movz x9, #0x0900, lsl #16
    // mov w10, #0x4b                // 'K'
    // str w10, [x9]

    // Force TTBR0_EL1 back to the kernel mapping before branching to Rust.
    msr ttbr0_el1, x1
    isb
    tlbi vmalle1is
    dsb ish
    isb

    // This trampoline runs from the high trap alias, so don't use BL to jump
    // into ordinary kernel text. Load the absolute symbol address instead.
    ldr x16, =kernel_irq_handler
    blr x16

    // Restore the interrupted TTBR pair before returning to the EL1 context.
    ldp x0, x1, [sp, #TRAP_TTBR_EL1]
    msr ttbr1_el1, x1
    msr ttbr0_el1, x0
    isb
    tlbi vmalle1is
    dsb ish
    isb

    RESTORE_EL1_STATE sp, x0, x1, x2
    RESTORE_GPRS sp
    add sp, sp, #288
    eret
