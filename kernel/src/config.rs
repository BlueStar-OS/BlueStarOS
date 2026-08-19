extern "C" {
    pub fn kernel_stack_lower_bound();
    pub fn kernel_stack_top();

    // 内核启动栈保护页
    pub fn kernel_stack_protect_start();
    pub fn kernel_stack_protect_end();

    pub fn kernel_trap_stack_top();
    pub fn kernel_trap_stack_bottom();

    // 内核运行栈保护页
    pub fn kernel_trap_stack_protect_start();
    pub fn kernel_trap_stack_protect_end();

    // 内核 self-trap 栈
    pub fn kernel_kernel_trap_bottom();
    pub fn kernel_kernel_trap_top();

    // stack bss 结束后的 bss 开始地址
    pub fn estack();

    pub fn ekernel();
    pub fn skernel();
    pub fn stext();
    pub fn etext();
    pub fn srodata();
    pub fn erodata();
    pub fn sdata();
    pub fn edata();
    pub fn sbss();
    pub fn ebss();
    /// 内核陷阱地址
    pub fn __kernel_trap();
    /// 内核陷阱恢复地址
    pub fn __kernel_refume();
    /// 内核陷入拒绝函数
    pub fn kernel_traped_forbid();
    /// 内核陷阱的物理起始地址
    pub fn straper();
    /// 用户程序专用陷阱物理起始地址
    pub fn utraper();
    /// secondary hart 入口地址 (entry.asm)
    pub fn _blue_secondary_start();
}

/// MB 的简单封装
pub const MB: usize = 1024 * 1024;
pub const PAGE_SIZE: usize = 4096;

pub const KERNEL_HEAP_SIZE: usize = 64 * MB;
pub const KERNEL_STACK_SIZE: usize = PAGE_SIZE * 64;
pub const USER_STACK_SIZE: usize = 64;
pub static mut KERNEL_HEADP: [u8; KERNEL_HEAP_SIZE] = [0; KERNEL_HEAP_SIZE];
pub const PAGE_SIZE_BITS: usize = 12;

pub const CPU_CIRCLE: usize = 12_500_000;

/// 使用虚拟高地址并且刚好留够一个页面，代表开始的第一个地址
pub const TRAP_BOTTOM_ADDR: usize = usize::MAX - PAGE_SIZE + 1;
/// 每个 app 的 trap context（高地址）
pub const TRAP_CONTEXT_ADDR: usize = TRAP_BOTTOM_ADDR - PAGE_SIZE;
/// 用户 start 函数在用户地址空间的起始映射地址
pub const USERLIB_START_RETURN_HIGNADDR: usize = TRAP_CONTEXT_ADDR - PAGE_SIZE;
pub const HIGNADDRESS_MASK: usize = 0xFFFFFFE000000000;
/// 每秒时钟中断次数
pub const TIME_FREQUENT: usize = 100;
/// 扇区大小
pub const SECTOR_SIZE: usize = 512;

/// 任务初始 ticket（优先级）
pub const TASK_TICKET: usize = 100;
/// Stride 调度基准大数
pub const BIG_INT: usize = 1_000_000;

/// 文件系统每个 Block 大小
pub const BLOCKSIZE: usize = 4096;

/// 系统架构字符串
#[cfg(target_arch = "aarch64")]
pub const ARCHITECTURE: &str = "aarch64";
#[cfg(target_arch = "riscv64")]
pub const ARCHITECTURE: &str = "riscv64";

use crate::{sync::UPSafeCell, MapSet};
use lazy_static::lazy_static;
lazy_static! {
    pub static ref KERNEL_SPACE: UPSafeCell<MapSet> =
        UPSafeCell::new(MapSet::new_kernel());
}
