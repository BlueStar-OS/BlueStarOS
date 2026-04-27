// AArch64 时间相关函数
use core::arch::asm;
use rsext4::Ext4Timestamp;

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

/// 为 ext4 元数据更新时间戳。
#[inline(always)]
pub fn ext4_current_time() -> Ext4Timestamp {
    let ticks = read_time() as u128;
    let freq = read_time_freq() as u128;
    if freq == 0 {
        return Ext4Timestamp::UNIX_EPOCH;
    }

    let sec = ticks / freq;
    let nsec = ((ticks % freq) * 1_000_000_000u128) / freq;
    Ext4Timestamp::new(sec.min(i64::MAX as u128) as i64, nsec as u32)
}
