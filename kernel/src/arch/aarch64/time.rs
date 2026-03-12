// AArch64 时间相关函数
use core::arch::asm;

/// 读取当前时间计数器
/// AArch64使用CNTPCT_EL0 (Physical Count register)
#[inline(always)]
pub fn read_time() -> usize {
    let count: u64;
    unsafe {
        asm!("mrs {}, cntpct_el0", out(reg) count);
    }
    count as usize
}

/// 读取计数器频率
/// AArch64使用CNTFRQ_EL0 (Counter Frequency register)
#[inline(always)]
pub fn read_time_freq() -> usize {
    let freq: u64;
    unsafe {
        asm!("mrs {}, cntfrq_el0", out(reg) freq);
    }
    freq as usize
}
