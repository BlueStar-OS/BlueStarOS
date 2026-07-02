//! sys_clone — 创建新任务。
//!
//! ## 作用
//! 创建新任务。
//!
//! ## 参数
//! `flags` clone 标志；`stack` 子栈；`ptid/tls/ctid` 线程相关地址。
//!
//! ## 注意事项
//! 当前走 sys_fork，线程共享地址空间/TLS/ctid 语义未完整支持。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: kernel/fork.c:2718-2734
//!
//! ## 实现情况
//! 降级实现。

use crate::memory::CloneFlags;
use crate::syscall::sys_fork::sys_fork;
use crate::syscall::syscall::*;

pub fn sys_clone(flags: usize, stack: usize, ptid: usize, tls: usize, ctid: usize) -> isize {
    let upper = flags & !0xffusize;

    // 没传信号处理
    sys_fork(
        CloneFlags::from_bits_truncate(upper),
        stack,
        ptid,
        tls,
        ctid,
    )
}
