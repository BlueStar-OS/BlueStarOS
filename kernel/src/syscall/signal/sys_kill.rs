//! sys_kill — 向进程或进程组发送信号。
//!
//! ## 作用
//! 按 Linux `kill(2)` 的 pid 规则向目标进程或进程组投递信号。
//!
//! ## 参数
//! `pid` 指定进程/进程组选择规则；`sig` 为待发送信号，0 表示只做存在性和权限检查。
//!
//! ## 注意事项
//! 当前 BlueStarOS 尚未实现 pending signal、signal disposition、权限检查和进程组投递语义。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: `kernel/signal.c:3949`。
//!
//! ## 实现情况
//! 未实现；TODO: 补齐 signal 子系统后实现真实投递。

/// sys_kill(pid, sig) -> 0 或 -errno。
///
/// 作用: 向进程或进程组发送信号。
/// 输入: `pid` 选择目标，`sig` 为信号编号。
/// 输出: 成功返回 0，失败返回负 errno。
/// 副作用: 完整实现时会修改目标任务 pending signal 集并触发唤醒/中断返回处理。
pub fn sys_kill(pid: usize, sig: usize) -> isize {
    let _ = (pid, sig);
    // TODO: 缺失 Linux 强语义: signal pending 集、sigaction/sigprocmask、
    // 凭据权限检查和进程组遍历。参考 K3 Linux 6.18.3 kernel/signal.c:3949。
    unimplemented!("sys_kill: user TODO")
}
