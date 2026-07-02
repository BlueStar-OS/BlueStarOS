//! sys_close — 关闭 fd。
//!
//! ## 作用
//! 关闭 fd。
//!
//! ## 参数
//! `fd` 文件描述符。
//!
//! ## 注意事项
//! 由任务 fd 表释放引用。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/open.c:1574
//!
//! ## 实现情况
//! 已实现。

use crate::syscall::syscall::*;

pub fn sys_close(fd: usize) -> isize {
    let ret = TASK_MANAER.close_current_fd(fd);
    if ret < 0 {
        warn!("sys_close: invalid fd={}", fd);
    }
    ret
}
