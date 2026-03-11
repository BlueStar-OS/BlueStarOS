// 引入trap
use crate::global_asm;
use core::arch::asm;
global_asm!(include_str!("./trap.asm"));



mod traphandler;


extern "C"{
    pub fn __aarch64_vector();
}

// Aarch64 task context
pub struct TrapContext {
    // 通用寄存器 x0-x30
    pub x: [u64; 31],
    
    // 栈指针（用户态）
    pub sp_el0: u64,
    
    // 程序计数器（异常返回地址）
    pub elr_el1: u64,
    
    // 程序状态寄存器
    pub spsr_el1: u64,
    
    // 页表基地址寄存器
    pub ttbr0_el1: u64,  // 用户空间页表
    
    // 内核信息（类似RISC-V）
    pub kernel_sp: u64,
    pub kernel_ttbr1: u64,  // 内核空间页表
    pub trap_handler: u64,
}

/// 设置内核陷阱处理程序向量
pub fn set_kernel_trap_handler() {
    // 设置aarch64 异常向量表地址
    unsafe {
         let vector_table = __aarch64_vector as usize;
         asm!("msr vbar_el1, {}", in(reg) vector_table);
    }
}

/// 设置内核禁止陷阱处理
pub fn set_kernel_forbid() {
    // TODO: AArch64 实现
    unsafe {
        // 临时空实现
    }
}