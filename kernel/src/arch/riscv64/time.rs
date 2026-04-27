// RISC-V 时间相关函数
use crate::config::CPU_CIRCLE;
use riscv::register::time;
use rsext4::Ext4Timestamp;

/// 读取当前时间计数器
#[inline(always)]
pub fn read_time() -> usize {
    time::read()
}

/// 为 ext4 元数据更新时间戳。
#[inline(always)]
pub fn ext4_current_time() -> Ext4Timestamp {
    let ticks = read_time() as u128;
    let freq = CPU_CIRCLE as u128;
    if freq == 0 {
        return Ext4Timestamp::UNIX_EPOCH;
    }

    let sec = ticks / freq;
    let nsec = ((ticks % freq) * 1_000_000_000u128) / freq;
    Ext4Timestamp::new(sec.min(i64::MAX as u128) as i64, nsec as u32)
}
