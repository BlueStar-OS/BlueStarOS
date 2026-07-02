//! sys_exit_group — 退出整个进程组。
//!
//! ## 作用
//! 退出整个进程组。
//!
//! ## 参数
//! `exit_code` 退出码。
//!
//! ## 注意事项
//! 当前复用 sys_exit；线程组完整语义依赖未来线程模型。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: kernel/exit.c:1116
//!
//! ## 实现情况
//! 降级实现。

use crate::syscall::sys_exit::sys_exit;
use crate::syscall::syscall::*;

pub fn sys_exit_group(exit_code: usize) -> isize {
    sys_exit(exit_code);
    BlueErr::ENOSYS.as_isize()
}
