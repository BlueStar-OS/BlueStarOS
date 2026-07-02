//! sys_yield — 主动让出 CPU。
//!
//! ## 作用
//! 主动让出 CPU。
//!
//! ## 参数
//! 无参数。
//!
//! ## 注意事项
//! 直接调用调度器切换。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: kernel/sched/syscalls.c:1383
//!
//! ## 实现情况
//! 已实现。

use crate::syscall::syscall::*;

pub fn sys_yield() -> isize {
    TASK_MANAER.suspend_and_run_task();
    0
}
