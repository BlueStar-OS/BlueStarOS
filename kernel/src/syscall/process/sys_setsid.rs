//! sys_setsid — 创建新会话。
//!
//! ## 作用
//! 让当前进程成为新 session leader 与新 process group leader。
//!
//! ## 参数
//! 无参数。
//!
//! ## 注意事项
//! 当前 BlueStarOS 尚未实现 session/控制终端/PGID 语义，因此不能静默返回伪 SID。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: `kernel/sys.c:1303`。
//!
//! ## 实现情况
//! 未实现；TODO: 补齐当前进程非组长校验、SID/PGID 更新、控制终端脱离语义后实现。

/// sys_setsid() -> 新 SID 或 -errno。
///
/// 作用: 创建新会话。
/// 输入: 无。
/// 输出: 成功返回新 SID，失败返回负 errno。
/// 副作用: 完整实现时会修改当前进程 SID/PGID 并脱离控制终端。
pub fn sys_setsid() -> isize {
    // TODO: 缺失 Linux 强语义: session leader/PGID 字段和 controlling tty。
    // 参考 K3 Linux 6.18.3 kernel/sys.c:1303。
    unimplemented!("sys_setsid: user TODO")
}
