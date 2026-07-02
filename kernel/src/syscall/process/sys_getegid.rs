//! sys_getegid — 获取有效组 ID。
//!
//! ## 作用
//! 返回当前进程 credentials 中用于权限检查的 effective GID。
//!
//! ## 参数
//! 无参数。
//!
//! ## 注意事项
//! 当前无 GID/补充组权限模型，临时按 root 组返回 0。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: `kernel/sys.c:1045`。
//!
//! ## 实现情况
//! Stub 降级实现；TODO: 补齐 `cred.egid` 后返回有效组 ID。

/// sys_getegid() -> 有效组 ID。
///
/// 作用: 返回当前进程 effective GID。
/// 输入: 无。
/// 输出: 当前固定返回 0。
/// 副作用: 无。
pub fn sys_getegid() -> isize {
    // TODO: 缺失 Linux 强语义: per-task credentials / setgid 语义。
    0
}
