//! sys_set_tid_address — 登记 clear_child_tid 地址并返回当前 tid。
//!
//! ## 作用
//! 登记 clear_child_tid 地址并返回当前 tid。
//!
//! ## 参数
//! `tidptr` 用户态 clear_child_tid 地址。
//!
//! ## 注意事项
//! 当前未保存 tidptr，也未在退出时 futex 唤醒。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: kernel/fork.c:1741
//!
//! ## 实现情况
//! Stub 降级实现。

use crate::syscall::sys_getpid::sys_getpid;

pub fn sys_set_tid_address(_tidptr: usize) -> isize {
    // simple implent
    sys_getpid()
}
