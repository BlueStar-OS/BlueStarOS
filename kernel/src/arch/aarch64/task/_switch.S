// AArch64 任务切换
// 参考 Linux kernel arch/arm64/kernel/entry.S cpu_switch_to
// 和 ARM Procedure Call Standard (AAPCS64)
//
// AArch64 calling convention:
// - Callee-saved registers: x19-x28, x29(fp), x30(lr), sp
// - Caller-saved registers: x0-x18
//
// TaskContext 结构 (见 mod.rs):
//   x19-x28 (10 registers)
//   x29 (fp)
//   x30 (lr)
//   sp
//   总共 13 个 8字节寄存器 = 104 字节

.section .text.switch
.global __switch

// __switch(current_task_cx_ptr: *const TaskContext, next_task_cx_ptr: *const TaskContext)
// x0 = current_task_cx_ptr (保存当前任务上下文的地址)
// x1 = next_task_cx_ptr (恢复下一个任务上下文的地址)
__switch:
    // 保存当前任务的 callee-saved 寄存器到 x0 指向的 TaskContext
    stp x19, x20, [x0, #0]      // 保存 x19, x20
    stp x21, x22, [x0, #16]     // 保存 x21, x22
    stp x23, x24, [x0, #32]     // 保存 x23, x24
    stp x25, x26, [x0, #48]     // 保存 x25, x26
    stp x27, x28, [x0, #64]     // 保存 x27, x28
    stp x29, x30, [x0, #80]     // 保存 x29(fp), x30(lr)
    mov x9, sp                   // 读取当前 sp
    str x9, [x0, #96]            // 保存 sp

    // 恢复下一个任务的 callee-saved 寄存器从 x1 指向的 TaskContext
    ldp x19, x20, [x1, #0]      // 恢复 x19, x20
    ldp x21, x22, [x1, #16]     // 恢复 x21, x22
    ldp x23, x24, [x1, #32]     // 恢复 x23, x24
    ldp x25, x26, [x1, #48]     // 恢复 x25, x26
    ldp x27, x28, [x1, #64]     // 恢复 x27, x28
    ldp x29, x30, [x1, #80]     // 恢复 x29(fp), x30(lr)
    ldr x9, [x1, #96]            // 恢复 sp
    mov sp, x9

    // 返回到 x30(lr) 指向的地址
    ret
