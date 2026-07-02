//! sys_getpgid — 获取进程组 ID。
//!
//! ## 作用
//! 返回指定进程所属的进程组 ID。
//!
//! ## 参数
//! `pid` 为目标进程 ID，0 表示当前进程。
//!
//! ## 注意事项
//! 当前 BlueStarOS 尚未实现 PGID 字段与 PID 命名空间查找语义。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: `kernel/sys.c:1215`。
//!
//! ## 实现情况
//! 未实现；TODO: 补齐 task PGID/session 基础设施后返回真实 PGID。

/// sys_getpgid(pid) -> PGID 或 -errno。
///
/// 作用: 获取指定进程的进程组 ID。
/// 输入: `pid` 指定目标进程，0 表示当前进程。
/// 输出: 成功返回 PGID，失败返回负 errno。
/// 副作用: 无。
pub fn sys_getpgid(pid: usize) -> isize {
    let _ = pid;
    // TODO: 缺失 Linux 强语义: PID 查找、PGID 字段、权限/可见性规则。
    // 参考 K3 Linux 6.18.3 kernel/sys.c:1215。
    unimplemented!("sys_getpgid: user TODO")
}
