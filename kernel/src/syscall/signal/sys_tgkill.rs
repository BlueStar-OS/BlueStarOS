//! sys_tgkill — 向指定线程组中的指定线程发送信号。
//!
//! ## 作用
//! 校验目标线程属于指定线程组后投递信号，是 pthread_kill 常用底层 syscall。
//!
//! ## 参数
//! `tgid` 为线程组 ID；`tid` 为目标线程 ID；`sig` 为待发送信号。
//!
//! ## 注意事项
//! 当前 BlueStarOS 尚未实现线程组、TID/TGID 对照和完整信号投递语义。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: `kernel/signal.c:4167`。
//!
//! ## 实现情况
//! 未实现；TODO: 补齐线程组模型和 signal 子系统后实现。

/// sys_tgkill(tgid, tid, sig) -> 0 或 -errno。
///
/// 作用: 向指定线程组内的指定线程发送信号。
/// 输入: `tgid` 为线程组 ID，`tid` 为目标线程 ID，`sig` 为信号编号。
/// 输出: 成功返回 0，失败返回负 errno。
/// 副作用: 完整实现时会修改目标线程 pending signal 集。
pub fn sys_tgkill(tgid: usize, tid: usize, sig: usize) -> isize {
    let _ = (tgid, tid, sig);
    // TODO: 缺失 Linux 强语义: TGID/TID 关系校验、线程级 pending signal 和权限检查。
    // 参考 K3 Linux 6.18.3 kernel/signal.c:4167。
    unimplemented!("sys_tgkill: user TODO")
}
