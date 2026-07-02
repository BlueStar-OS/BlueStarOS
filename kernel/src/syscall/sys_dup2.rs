//! sys_dup2 — 把 old fd 复制到指定 new fd。
//!
//! ## 作用
//! 把 old fd 复制到指定 new fd。
//!
//! ## 参数
//! `old_fd` 源 fd；`new_fd` 目标 fd。
//!
//! ## 注意事项
//! 当前关闭目标 fd 时只替换表项，依赖 Arc drop 释放资源。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/file.c:1438
//!
//! ## 实现情况
//! 已实现基础路径。

use crate::syscall::syscall::*;

pub fn sys_dup2(old_fd: i32, new_fd: i32) -> isize {
    if old_fd < 0 || new_fd < 0 {
        return BlueErr::EBADF.as_isize();
    }

    let current_task = TASK_MANAER
        .task_que_inner
        .lock(|inner| inner.task_queen[inner.current].clone());

    current_task.lock(|tcb| {
        let old_idx = old_fd as usize;
        if old_idx >= tcb.file_descriptor.len() {
            return BlueErr::EBADF.as_isize();
        }
        let Some(source_fd) = tcb.file_descriptor[old_idx].clone() else {
            return BlueErr::EBADF.as_isize();
        };

        if old_fd == new_fd {
            return new_fd as isize;
        }

        let new_idx = new_fd as usize;
        if new_idx >= tcb.file_descriptor.len() {
            tcb.file_descriptor.resize_with(new_idx + 1, || None);
        }
        tcb.file_descriptor[new_idx] = None;
        tcb.file_descriptor[new_idx] = Some(source_fd);
        new_fd as isize
    })
}
