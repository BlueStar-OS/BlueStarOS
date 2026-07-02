//! sys_munmap — 解除用户地址区间映射。
//!
//! ## 作用
//! 解除用户地址区间映射。
//!
//! ## 参数
//! `start` 起始地址；`size` 长度。
//!
//! ## 注意事项
//! 依赖当前任务 memory_set unmap_range。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: mm/mmap.c:1077
//!
//! ## 实现情况
//! 已实现基础路径。

use crate::syscall::syscall::*;

pub fn sys_munmap(start: usize, size: usize) -> isize {
    TASK_MANAER.task_que_inner.lock(|inner| {
        let current = inner.current;
        inner.task_queen[current].lock(|memset| memset.memory_set.unmap_range(VirAddr(start), size))
    })
}
