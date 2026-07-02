//! sys_dup — 复制 fd 到最小可用 fd。
//!
//! ## 作用
//! 复制 fd 到最小可用 fd。
//!
//! ## 参数
//! `old_fd` 源 fd。
//!
//! ## 注意事项
//! 未实现 close-on-exec 标志传播细节。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/file.c:1457
//!
//! ## 实现情况
//! 已实现基础路径。

use crate::syscall::syscall::*;

pub fn sys_dup(old_fd: i32) -> isize {
    let current_task = TASK_MANAER
        .task_que_inner
        .lock(|inner| inner.task_queen[inner.current].clone());

    current_task.lock(|tcb| {
        if old_fd < 0 {
            return BlueErr::EBADF.as_isize();
        }
        let old_idx = old_fd as usize;
        if old_idx >= tcb.file_descriptor.len() {
            return BlueErr::EBADF.as_isize();
        }
        let Some(source_fd) = tcb.file_descriptor[old_idx].clone() else {
            return BlueErr::EBADF.as_isize();
        };

        if let Some((idx, _)) = tcb
            .file_descriptor
            .iter()
            .enumerate()
            .find(|(_, slot)| slot.is_none())
        {
            tcb.file_descriptor[idx] = Some(source_fd);
            return idx as isize;
        }

        let idx = tcb.file_descriptor.len();
        tcb.file_descriptor.push(Some(source_fd));
        idx as isize
    })
}
