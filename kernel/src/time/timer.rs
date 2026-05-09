//! 内核定时器模块：基于 RISC-V mtime / aarch64 Generic Timer 的时钟管理。
//!
//! 硬件提供一个单调递增的计数器（tick），频率由 `CPU_CIRCLE` 定义（主频，单位 Hz）。
//! 本模块负责：
//! - 读取当前 tick 数，转换为毫秒/纳秒
//! - 设置下一次时钟中断（mtimecmp / aarch64 timer compare）
//! - 内核忙等 sleep（当前有 bug，不推荐使用）
//!
//! 时钟中断频率由 `TIME_FREQUENT` 定义（单位 Hz），每个 tick 的周期 = `CPU_CIRCLE / TIME_FREQUENT`。

// 毫秒常量，用于时间单位转换
const MSEC: usize = 1000;
const MNS: usize = 1_000_000_000;
use crate::arch::time::*;
use crate::config::{CPU_CIRCLE, TIME_FREQUENT};
use crate::set_next_timetriger;

/// POSIX 风格的时间值结构体：秒 + 微秒
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// 读取当前硬件 tick 计数值（mtime / Generic Timer 计数寄存器）
pub fn get_time_tick() -> usize {
    read_time()
}

/// tick → 毫秒转换。
/// 先乘再除防止整数截断导致精度丢失。
pub fn get_time_ms() -> usize {
    (read_time() * MSEC) / CPU_CIRCLE
}

/// tick → 微秒转换。
pub fn get_time_us() -> usize {
    (read_time() * 1_000_000) / CPU_CIRCLE
}

/// tick → 纳秒转换。
pub fn get_time_ns() -> usize {
    (read_time() * MNS) / CPU_CIRCLE
}

/// 设置下一次时钟中断。
///
/// 计算方式：当前 tick + 每个中断间隔的 tick 数，写入 mtimecmp / Generic Timer compare。
/// 不在此函数内做额外耗时检查，因为定时器中断处理路径要求极低延迟。
/// 即使因调用延迟导致计算值 < 当前 mtime，硬件会立即触发一次中断，
/// 等价于提前触发，无副作用。
pub fn set_next_timeInterupt() {
    let next_time = get_time_tick() + CPU_CIRCLE / TIME_FREQUENT;
    set_next_timetriger(next_time);
}

/// 内核忙等 sleep（阻塞式，传入毫秒数）。
///
/// 通过轮询 mtime 实现延迟，不释放 CPU。
/// **当前有 bug，不推荐使用。**
pub fn kernel_sleep(time_ms: usize) {
    let target = read_time() + CPU_CIRCLE / MSEC * time_ms;
    while read_time() <= target {
        core::hint::spin_loop();
    }
}
