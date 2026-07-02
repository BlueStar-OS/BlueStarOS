//! sys_getgid — 获取实际组 ID。
//!
//! ## 作用
//! 返回当前进程 credentials 中的 real GID。
//!
//! ## 参数
//! 无参数。
//!
//! ## 注意事项
//! 当前无 GID/补充组权限模型，临时按 root 组返回 0。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: `kernel/sys.c:1039`。
//!
//! ## 实现情况
//! Stub 降级实现；TODO: 补齐 `cred.gid` 后返回实际组 ID。

/// sys_getgid() -> 实际组 ID。
///
/// 作用: 返回当前进程 real GID。
/// 输入: 无。
/// 输出: 当前固定返回 0。
/// 副作用: 无。
pub fn sys_getgid() -> isize {
    // TODO: 缺失 Linux 强语义: per-task credentials / namespace-aware kgid_t。
    0
}
