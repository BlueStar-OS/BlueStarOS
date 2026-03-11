// AArch64 任务切换 - 临时占位符
// 目前只是为了让编译通过

.section .text.switch
.global __switch

// 临时实现：空函数，直接返回
__switch:
    // TODO: 实现 AArch64 任务切换
    // 保存 callee-saved 寄存器 (x19-x28, x29/fp, x30/lr, sp)
    // 恢复新任务的寄存器
    ret
