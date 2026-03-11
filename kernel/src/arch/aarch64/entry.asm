    .section .text.entry
    .globl _blue_start
_blue_start:
    // ARM64 Linux Image Header (64 bytes total)
    // Offset 0x00: code0 - branch to kernel start
    b       _start_kernel

    // Offset 0x04: code1 - reserved
    .long   0

    // Offset 0x08: text_offset - load offset from RAM start
    .quad   0

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
    ldr x0, =kernel_stack_top
    mov sp, x0

    ldr x0, =kernel_trap_stack_top
    msr tpidr_el1, x0

    // 屏蔽 IRQ 中断
    msr daifset, #2

    bl blue_main






.section .bss.stack
kernel_stack_protect_start:
    .space 4096
    .global kernel_stack_protect_end
kernel_stack_protect_end:

    .globl kernel_stack_lower_bound
kernel_stack_lower_bound:
    .space 4096 * 64
    .globl kernel_stack_top
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