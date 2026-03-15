######################################################
# 内核态 trap 处理
# 当内核态开启 SIE 后（如等待 I/O），中断会进入这里
# 与用户态 trap 不同：
#   - 不需要切换地址空间（已经在内核 satp）
#   - 不需要 csrrw sp,sscratch（sp 已经是内核栈）
#   - 直接在当前内核栈上保存 caller-saved 寄存器
######################################################

.altmacro

.section .text.traper
    .global __kernel_mode_trap
    .global __kernel_mode_trap_return

.align 4
__kernel_mode_trap:
    # 在当前内核栈上分配空间保存寄存器
    # 需要保存: ra, t0-t6, a0-a7, sepc, sstatus = 17 个寄存器
    addi sp, sp, -18*8

    sd ra,  0*8(sp)
    sd t0,  1*8(sp)
    sd t1,  2*8(sp)
    sd t2,  3*8(sp)
    sd t3,  4*8(sp)
    sd t4,  5*8(sp)
    sd t5,  6*8(sp)
    sd t6,  7*8(sp)
    sd a0,  8*8(sp)
    sd a1,  9*8(sp)
    sd a2, 10*8(sp)
    sd a3, 11*8(sp)
    sd a4, 12*8(sp)
    sd a5, 13*8(sp)
    sd a6, 14*8(sp)
    sd a7, 15*8(sp)

    # 保存 sepc 和 sstatus
    csrr t0, sepc
    csrr t1, sstatus
    sd t0, 16*8(sp)
    sd t1, 17*8(sp)


    
    # 调用 Rust handler（用 la+jalr 避免 call 的 ±1MB 范围限制）
    la t0, kernel_mode_trap_handler
    jalr ra, t0, 0

__kernel_mode_trap_return:
    # 恢复 sepc 和 sstatus
    ld t0, 16*8(sp)
    ld t1, 17*8(sp)
    csrw sepc, t0
    csrw sstatus, t1

    ld ra,  0*8(sp)
    ld t0,  1*8(sp)
    ld t1,  2*8(sp)
    ld t2,  3*8(sp)
    ld t3,  4*8(sp)
    ld t4,  5*8(sp)
    ld t5,  6*8(sp)
    ld t6,  7*8(sp)
    ld a0,  8*8(sp)
    ld a1,  9*8(sp)
    ld a2, 10*8(sp)
    ld a3, 11*8(sp)
    ld a4, 12*8(sp)
    ld a5, 13*8(sp)
    ld a6, 14*8(sp)
    ld a7, 15*8(sp)

    addi sp, sp, 18*8
    sret
