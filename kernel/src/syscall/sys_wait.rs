//! sys_wait — 等待任意子进程，兼容旧 wait。
//!
//! ## 作用
//! 等待任意子进程，兼容旧 wait。
//!
//! ## 参数
//! `exit_code_ptr` wstatus 写回地址。
//!
//! ## 注意事项
//! 复用 wait4(pid=-1, options=WNOHANG)。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: kernel/exit.c:1894 (wait4)
//!
//! ## 实现情况
//! 降级实现。

use crate::syscall::sys_wait4::sys_wait4;
use crate::syscall::syscall::*;

pub fn sys_wait(exit_code_ptr: usize) -> isize {
    // wait4(pid=-1, wstatus, options=1)  WNOHANG == 1
    sys_wait4(-1, exit_code_ptr, 1)
}
