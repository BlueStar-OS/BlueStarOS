//! sys_getppid — 返回父进程 pid。
//!
//! ## 作用
//! 返回父进程 pid。
//!
//! ## 参数
//! 无参数。
//!
//! ## 注意事项
//! 无父进程时返回 0。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: kernel/sys.c:1016
//!
//! ## 实现情况
//! 已实现基础路径。

use crate::syscall::syscall::*;

pub fn sys_getppid() -> isize {
    let current_task = TASK_MANAER
        .task_que_inner
        .lock(|inner| inner.task_queen[inner.current].clone());
    current_task.lock(|tcb| {
        if let Some(parent) = tcb.parent.as_ref().and_then(|w| w.upgrade()) {
            parent.lock(|p| p.pid.0 as isize)
        } else {
            0
        }
    })
}
