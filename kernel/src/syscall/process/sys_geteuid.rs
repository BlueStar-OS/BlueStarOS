//! sys_geteuid — 获取有效用户 ID。
//!
//! ## 作用
//! 返回当前进程 credentials 中用于权限检查的 effective UID。
//!
//! ## 参数
//! 无参数。
//!
//! ## 注意事项
//! 当前无 setuid/SUID/credentials 模型，临时按 root 返回 0。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: `kernel/sys.c:1033`。
//!
//! ## 实现情况
//! Stub 降级实现；TODO: 补齐 `cred.euid` 后返回有效用户 ID。

/// sys_geteuid() -> 有效用户 ID。
///
/// 作用: 返回当前进程 effective UID。
/// 输入: 无。
/// 输出: 当前固定返回 0。
/// 副作用: 无。
pub fn sys_geteuid() -> isize {
    // TODO: 缺失 Linux 强语义: per-task credentials / setuid 语义。
    0
}
