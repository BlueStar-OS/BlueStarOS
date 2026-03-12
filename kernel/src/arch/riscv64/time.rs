// RISC-V 时间相关函数
use riscv::register::time;

/// 读取当前时间计数器
#[inline(always)]
pub fn read_time() -> usize {
    time::read()
}
