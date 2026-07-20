//! sys_creat — 创建或截断打开文件。
//!
//! ## 作用
//! 创建或截断打开文件。
//!
//! ## 参数
//! `path_ptr` 路径。
//!
//! ## 注意事项
//! 复用 sys_open 固定 flags。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/open.c:1463 (openat + O_CREAT)
//!
//! ## 实现情况
//! 降级实现。

use crate::syscall::sys_open::sys_open;

pub fn sys_creat(path_ptr: usize) -> isize {
    let flags_bits = (1 << 6) | (1 << 9) | 1;
    sys_open(path_ptr, flags_bits)
}
