// AArch64 trap handler - 临时占位符
// 目前只是为了让编译通过，实际功能未实现

.section .text.traper
.global __kernel_trap
.global __kernel_refume
.global kernel_traped_forbid
.global __aarch64_vector


//aarch 2048位对齐
.align 11
__aarch64_vector:
    // 当前EL，SP_EL0（使用EL0的栈指针）
    .align 7   // 每个入口128字节
    b sync_el1_sp0      // 同步异常
    .align 7
    b irq_el1_sp0       // IRQ中断
    .align 7
    b fiq_el1_sp0       // FIQ中断
    .align 7
    b serror_el1_sp0    // SError
    // 当前EL，SP_ELx（使用当前EL的栈指针）
    .align 7
    b __kernel_trap      // 同步异常
    .align 7
    b irq_el1_spx       // IRQ中断
    .align 7
    b fiq_el1_spx       // FIQ中断
    .align 7
    b serror_el1_spx    // SError

    // 低EL，AArch64
    .align 7
    b __kernel_trap       // 同步异常（用户态）
    .align 7
    b irq_el0_64        // IRQ中断
    .align 7
    b fiq_el0_64        // FIQ中断
    .align 7
    b serror_el0_64     // SError

    // 低EL，AArch32
    .align 7
    b sync_el0_32       // 同步异常（32位用户态）
    .align 7
    b irq_el0_32        // IRQ中断
    .align 7
    b fiq_el0_32        // FIQ中断
    .align 7
    b serror_el0_32     // SError




######################################################
# TrapContext 结构（参考 trap/mod.rs）
#     pub x: [u64; 31],        // 0-248 (31*8)
#     pub sp_el0: u64,         // 248
#     pub elr_el1: u64,        // 256
#     pub spsr_el1: u64,       // 264
#     pub ttbr0_el1: u64,      // 272
#     pub kernel_sp: u64,      // 280
#     pub kernel_ttbr1: u64,   // 288
#     pub trap_handler: u64,   // 296
######################################################

.align 7
__kernel_trap:
    // 1. 切换栈指针：sp ↔ tpidr_el1
    // tpidr_el1保存TrapContext地址，sp是用户栈
    // 注意：不能直接 msr/mrs sp，需要通过通用寄存器中转
    mov x0, sp              // 保存用户sp到x0
    mrs x1, tpidr_el1       // 读取TrapContext地址到x1
    msr tpidr_el1, x0       // 保存用户sp到tpidr_el1
    mov sp, x1              // 设置sp为TrapContext地址

    // 现在sp指向TrapContext，tpidr_el1保存用户sp

    // 2. 保存通用寄存器 x0-x30
    // 注意：x0和x1已经被修改，需要从tpidr_el1和sp恢复原值后再保存
    mrs x0, tpidr_el1       // 恢复原始用户sp到x0（临时）
    stp x0, x1, [sp, #0]    // 保存x0(用户sp), x1

    stp x2, x3, [sp, #16]
    stp x4, x5, [sp, #32]
    stp x6, x7, [sp, #48]
    stp x8, x9, [sp, #64]
    stp x10, x11, [sp, #80]
    stp x12, x13, [sp, #96]
    stp x14, x15, [sp, #112]
    stp x16, x17, [sp, #128]
    stp x18, x19, [sp, #144]
    stp x20, x21, [sp, #160]
    stp x22, x23, [sp, #176]
    stp x24, x25, [sp, #192]
    stp x26, x27, [sp, #208]
    stp x28, x29, [sp, #224]
    str x30, [sp, #240]

    // 3. 保存特殊寄存器
    mrs x0, tpidr_el1       // 用户sp
    mrs x1, elr_el1         // 异常返回地址
    mrs x2, spsr_el1        // 程序状态

    stp x0, x1, [sp, #248]  // 保存sp_el0和elr_el1
    str x2, [sp, #264]      // 保存spsr_el1

    // 4. 加载内核信息
    ldr x0, [sp, #280]      // kernel_sp
    ldr x1, [sp, #288]      // kernel_ttbr1
    ldr x2, [sp, #296]      // trap_handler

    // 5. 切换到内核页表
    msr ttbr1_el1, x1
    isb
    tlbi vmalle1is          // 刷新TLB
    dsb ish
    isb

    // 6. 切换到内核栈
    mov sp, x0

    // 7. 跳转到C处理函数
    br x2


.align 7
__kernel_refume:
    // 参数：x0 = TrapContext地址, x1 = 用户页表(ttbr0_el1)

    // 1. 切换到用户页表
    msr ttbr0_el1, x1
    isb
    tlbi vmalle1is
    dsb ish
    isb

    // 2. 让tpidr_el1指向TrapContext
    msr tpidr_el1, x0

    // 3. sp指向TrapContext
    mov sp, x0

    // 4. 恢复特殊寄存器
    ldp x0, x1, [sp, #248]  // sp_el0, elr_el1
    ldr x2, [sp, #264]      // spsr_el1

    msr sp_el0, x0
    msr elr_el1, x1
    msr spsr_el1, x2

    // 5. 恢复通用寄存器 x0-x30
    ldp x0, x1, [sp, #0]
    ldp x2, x3, [sp, #16]
    ldp x4, x5, [sp, #32]
    ldp x6, x7, [sp, #48]
    ldp x8, x9, [sp, #64]
    ldp x10, x11, [sp, #80]
    ldp x12, x13, [sp, #96]
    ldp x14, x15, [sp, #112]
    ldp x16, x17, [sp, #128]
    ldp x18, x19, [sp, #144]
    ldp x20, x21, [sp, #160]
    ldp x22, x23, [sp, #176]
    ldp x24, x25, [sp, #192]
    ldp x26, x27, [sp, #208]
    ldp x28, x29, [sp, #224]
    ldr x30, [sp, #240]

    // 6. 恢复用户栈指针（从sp_el0）
    // 注意：不能直接 mrs sp, sp_el0，需要通过通用寄存器
    mrs x0, sp_el0
    mov sp, x0

    // 7. 返回用户态
    eret





.align 7
kernel_traped_forbid:
    // TODO: 实现 AArch64 kernel trap forbid
    b kernel_traped_forbid