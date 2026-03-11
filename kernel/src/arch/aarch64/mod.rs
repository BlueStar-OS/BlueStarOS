// AArch64 架构相关实现


pub mod task;
pub mod memory;
pub mod trap;


use core::arch::{global_asm, asm};
use crate::arch::task::TaskContext;


// 引入入口
global_asm!(include_str!("./entry.asm"));


extern "C" {
    pub fn __kernel_trap();
    pub fn __kernel_refume();
    pub fn kernel_traped_forbid();
    pub fn __switch(need_swapout:*const TaskContext,need_swapin:*const TaskContext);//任务切换汇编函数
}




pub use trap::*;
