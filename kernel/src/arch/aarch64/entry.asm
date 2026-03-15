    .section .text.entry
    .globl _blue_start
_blue_start:
    // ARM64 Linux Image Header (64 bytes total)
    // Offset 0x00: code0 - branch to kernel start
    b       _start_kernel

    // Offset 0x04: code1 - reserved
    .long   0

    // Offset 0x08: text_offset - QEMU virt RAM starts at 0x40000000,
    // and the kernel is linked at 0x40080000.
    .quad   0x00080000

    // Offset 0x10: image_size - effective size
    .quad   _kernel_size

    // Offset 0x18: flags
    .quad   0x0a

    // Offset 0x20-0x30: reserved
    .quad   0
    .quad   0
    .quad   0

    // Offset 0x38: magic number "ARM\x64" (0x644d5241 little endian)
    .long   0x644d5241

    // Offset 0x3c: reserved
    .long   0

_start_kernel:


    //Save dtb pointer
    mov x20,x0

    // underflow el level
    mrs x0,CurrentEL
    lsr x0,x0,#2
    cmp x0, #2
    b.ne 1f

    // 配置 EL2 → EL1 降级
    mov     x0, #(1 << 31)  // HCR_EL2.RW=1 (EL1 用 AArch64)
    msr     hcr_el2, x0

    mov     x0, #0x3c5      // SPSR: EL1h (SPSel=1), DAIF 全屏蔽 外部中断全屏蔽
    msr     spsr_el2, x0

    adr     x0, 1f          // 返回地址
    msr     elr_el2, x0

    eret                     // 降到 EL1 

    1:
    
    // Save DTB pointer from x0 (passed by U-Boot following ARM64 boot protocol)
    // x0 contains the device tree blob pointer
    ldr x1, =_dtb_pointer
    str x20, [x1]

    // Initialize stack
    ldr x0, =kernel_stack_top
    mov sp, x0

    ldr x0, =kernel_trap_stack_top
    msr tpidr_el1, x0

    // 屏蔽 IRQ 中断
    msr daifset, #2

    bl blue_main


// DTB pointer storage
.section .data
.align 3
.global _dtb_pointer
_dtb_pointer:
    .quad 0






.section .bss.stack
kernel_stack_protect_start:
    .space 4096
    .global kernel_stack_protect_end
kernel_stack_protect_end:

    .globl kernel_stack_lower_bound
kernel_stack_lower_bound:
    .space 4096 * 64
    .globl kernel_stack_top
    .align 4
kernel_stack_top:
    .global kernel_stack_protect_start

.space 4096

#下面为了简化，加一个特殊的内核专用异常处理栈
.global kernel_trap_stack_bottom
.global kernel_trap_stack_top
.global kernel_trap_stack_protect_start
.global kernel_trap_stack_protect_end
.align 4

# 内核trap栈保护页
kernel_trap_stack_protect_start:
    .space 4096
kernel_trap_stack_protect_end:


# 内核陷入trap用
.global kernel_kernel_trap_bottom
.global kernel_kernel_trap_top
.align 4
kernel_kernel_trap_bottom:
.space 4096*4
kernel_kernel_trap_top:


# 内核启动到firstapp启动期间用
kernel_trap_stack_bottom:
.space 4096
kernel_trap_stack_top:


.space 4096
.global kernel_bss_end
kernel_bss_end:
#ld:从内存加载64到寄存器 la 将符号地址赋值给寄存器
