//! sys_mkdir — 兼容旧 mkdir，复用 mkdirat。
//!
//! ## 作用
//! 兼容旧 mkdir，复用 mkdirat。
//!
//! ## 参数
//! `path_ptr` 路径。
//!
//! ## 注意事项
//! 内部固定 AT_FDCWD 和 mode=0。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/namei.c:4501 (mkdir 通过 mkdirat 语义承载)
//!
//! ## 实现情况
//! 降级实现。

use crate::syscall::sys_mkdirat::sys_mkdirat;
use crate::syscall::syscall::*;

pub fn sys_mkdir(path_ptr: usize) -> isize {
    sys_mkdirat(-100, path_ptr, 0)
}
