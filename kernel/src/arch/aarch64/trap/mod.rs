// 引入trap
use crate::global_asm;
use core::arch::asm;
global_asm!(include_str!("./trap.asm"));
use core::panicking::panic;
mod traphandler;

// 导出平台相关符号函数
extern "C" {
    pub fn __aarch64_vector();
}

// Aarch64 task context
#[repr(C)]
pub struct TrapContext {
    // 通用寄存器 x0-x30
    pub x: [usize; 31],

    // 栈指针（用户态）
    pub sp_el0: usize,

    // 程序计数器（异常返回地址）
    pub elr_el1: usize,

    // 程序状态寄存器
    pub spsr_el1: usize,

    // 页表基地址寄存器
    pub ttbr0_el1: usize,  // 用户空间页表

    // 内核信息（类似RISC-V）
    pub kernel_sp: usize,
    pub kernel_ttbr1: usize,  // 内核空间页表
    pub trap_handler: usize,
}

#[no_mangle]
pub fn no_return_start()->!{
    panic("Start Function you ret ,WTF????");
}

impl TrapContext {
    /// 初始化应用程序的陷阱上下文
    pub fn init_app_trap_context(
        entry: usize,
        kernel_satp: usize,
        trap_handler: usize,
        kernel_sp: usize,
        user_sp: usize,
    ) -> Self {
        use log::debug;

        // SPSR_EL1: 设置返回到EL0 (用户态)
        // bit [3:0] = 0b0000 (EL0t - EL0 with SP_EL0)
        // bit [6] = 0 (FIQ not masked)
        // bit [7] = 0 (IRQ not masked)
        // bit [8] = 0 (SError not masked)
        // bit [9] = 0 (Debug exceptions not masked)
        let spsr: usize = 0x0;

        debug!("SPSR_EL1: {:#X}", spsr);

        let mut x = [0usize; 31];
        // x30 (LR) 设置为 no_return_start (如果需要)
        x[30] = no_return_start as usize;

        TrapContext {
            x,
            sp_el0: user_sp,
            elr_el1: entry,
            spsr_el1: spsr,
            ttbr0_el1: kernel_satp,
            kernel_sp: kernel_sp,
            kernel_ttbr1: kernel_satp,
            trap_handler: trap_handler ,
        }
    }
}




/// 设置内核陷阱处理程序向量
pub fn set_kernel_trap_handler() {
    use crate::config::TRAP_BOTTOM_ADDR;
    // VBAR_EL1 必须指向高地址 trampoline，因为：
    // 用户态 trap 时 TTBR0=用户页表（没有内核代码），
    // 但 TTBR1 有 TRAP_BOTTOM_ADDR 映射（内核和用户页表都有）
    unsafe {
         let vector_table = TRAP_BOTTOM_ADDR;
         asm!("msr vbar_el1, {}", in(reg) vector_table);
    }
}

/// 设置内核禁止陷阱处理
pub fn set_kernel_forbid() {
    // TODO: AArch64 实现
    unsafe {
        // 临时空实现
    }
    panic!("kernel trap");
}