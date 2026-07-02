//! sys_setpgid — 设置进程组 ID。
//!
//! ## 作用
//! 将指定进程加入指定进程组，支撑 POSIX job control。
//!
//! ## 参数
//! `pid` 为目标进程 ID，0 表示当前进程；`pgid` 为目标进程组 ID，0 表示使用目标进程 PID。
//!
//! ## 注意事项
//! 当前 BlueStarOS 尚未实现 session/PGID/控制终端/exec 前后约束等 job control 强语义，不能静默成功。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: `kernel/sys.c:1114`。
//!
//! ## 实现情况
//! 未实现；TODO: 补齐 task session/pgid 字段、进程查找、同 session 校验与锁顺序后实现。

/// sys_setpgid(pid, pgid) -> 0 或 -errno。
///
/// 作用: 设置目标进程的进程组 ID。
/// 输入: `pid` 指定目标进程，`pgid` 指定目标进程组。
/// 输出: 成功返回 0，失败返回负 errno。
/// 副作用: 完整实现时会修改目标进程的 PGID。
pub fn sys_setpgid(pid: usize, pgid: usize) -> isize {
    let _ = (pid, pgid);
    // TODO: 缺失 Linux 强语义: session/PGID 模型、同 session 权限校验、
    // exec 后禁止 setpgid 规则。参考 K3 Linux 6.18.3 kernel/sys.c:1114。
    unimplemented!("sys_setpgid: user TODO")
}
