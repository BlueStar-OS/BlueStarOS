//! sys_getpid — 返回当前进程 pid。
//!
//! ## 作用
//! 返回当前进程 pid。
//!
//! ## 参数
//! 无参数。
//!
//! ## 注意事项
//! 直接读取当前 TCB pid。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: kernel/sys.c:999
//!
//! ## 实现情况
//! 已实现。

use crate::syscall::syscall::*;

pub fn sys_getpid() -> isize {
    TASK_MANAER
        .task_que_inner
        .lock(|inner| inner.task_queen[inner.current].lock(|tcb| tcb.pid.0 as isize))
}
