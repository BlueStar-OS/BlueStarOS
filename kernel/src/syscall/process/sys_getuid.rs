//! sys_getuid — 获取实际用户 ID。
//!
//! ## 作用
//! 返回当前进程 credentials 中的 real UID。
//!
//! ## 参数
//! 无参数。
//!
//! ## 注意事项
//! BlueStarOS 当前没有 `cred`/UID/GID 权限模型，临时按单用户 root 系统返回 0。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: `kernel/sys.c:1027`。
//!
//! ## 实现情况
//! Stub 降级实现；TODO: 补齐 Linux `struct cred` 等价基础设施后改为读取 real UID。

/// sys_getuid() -> 实际用户 ID。
///
/// 作用: 返回当前进程 real UID。
/// 输入: 无。
/// 输出: 当前固定返回 0。
/// 副作用: 无。
pub fn sys_getuid() -> isize {
    // TODO: 缺失 Linux 强语义: per-task credentials / namespace-aware kuid_t。
    0
}
