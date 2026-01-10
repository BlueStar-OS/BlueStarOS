######################################################
# trapcontext 里面保存的是程序在·用户·态陷入内核的最后状态  
#    ///32个寄存器完全保存
#     pub x:[usize;32],
#     ///陷入状态
#     pub sstatus:Sstatus, //32*8(sp)
#     ///返回地址
#     pub sepc_entry_point:usize,//33*8(sp)
#     ///内核地址空间satp
#     pub kernel_satp:usize,//34*8(sp)
#     ///内核栈指针
#     pub kernel_sp:usize,//35*8(sp)
#     ///陷阱处理程序
#     pub trap_handler:usize,//36*8(sp)
######################################################

#针对内核trap处理，先不使用trapcontext的结构体
#启用高级宏
.altmacro
.macro SAVE_GP n
    sd x\n , \n*8(sp)
.endm

.macro REFUME_GP n
    ld x\n,\n*8(sp)
.endm
.section .text.traper
    .global __kernel_trap
    .global __kernel_refume

#x权限硬性要求对齐，确保 __kernel_trap 在段的开始处
.align 4
__kernel_trap:
#切换到内核陷入栈,函数栈暂存
csrrw sp,sscratch,sp #sp现在是trapcontext指针 sscratch 是usersp             
sd x1,1*8(sp) #ra
sd x3,3*8(sp) #gp
.set n,4
.rept 28
    SAVE_GP %n
    .set n,n+1
.endr
#保存csr寄存器
csrr t0,sstatus
csrr t1,sepc

sd t0,32*8(sp)
sd t1,33*8(sp)

csrr t2,sscratch
sd t2,2*8(sp)#保存usersp
#kernel satp
ld t0,34*8(sp)
#traphand;er
ld t1,36*8(sp)
#app kernel sp
ld sp,35*8(sp)

csrw satp,t0
sfence.vma
jr t1#跳转到traphandler




#x权限硬性要求对齐
.align 4
__kernel_refume: #a0 trap_context_addr a1:user satp

#切换为用户地址空间
csrw satp ,a1
sfence.vma #刷新页表

#让sscratch指向所有任务通用的trapcontext
csrw sscratch,a0 

#指向trapcontext
mv sp,a0

#先恢复csr寄存器
ld t0,32*8(sp)#sstatus
ld t1,33*8(sp)#sepc

csrw sstatus,t0
csrw sepc,t1

ld x1,1*8(sp) #x1 a0
ld x3,3*8(sp)
.set n,4
.rept 28
    REFUME_GP %n
    .set n,n+1
.endr

# sp = x2
ld x2,2*8(sp) #最后恢复sp为user普通栈指针
sret
#回到触发异常的那条指令
