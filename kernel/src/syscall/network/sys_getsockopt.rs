//! sys_getsockopt — 读取 socket 选项。
//!
//! ## 作用
//! 按 `level + optname` 读取 socket 内部选项状态，并写回用户缓冲区。
//!
//! ## 参数
//! `sockfd` 是 socket fd；`level` 是选项层级；`optname` 是选项名；`optval` 指向用户写回缓冲区；`optlen` 指向用户长度字段。
//!
//! ## 注意事项
//! 当前 socket option 状态表尚未实现；不能静默返回默认值，否则会破坏用户态协议栈探测逻辑。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: `net/socket.c:2454`。
//!
//! ## 实现情况
//! 未实现；后续需要与 `sys_setsockopt` 共用 socket option 存储与用户拷贝路径。

/// sys_getsockopt(sockfd, level, optname, optval, optlen) -> 0 或 -errno。
///
/// 作用: 获取 socket 选项。
/// 输入: `sockfd` 选择 socket；`level/optname` 选择选项；`optval/optlen` 为用户写回位置。
/// 输出: 成功返回 0；失败返回负 errno。
/// 副作用: 完整实现时会写回用户 `optval`，并更新用户 `optlen` 指向的实际长度。
pub fn sys_getsockopt(
    sockfd: usize,
    level: usize,
    optname: usize,
    optval: usize,
    optlen: usize,
) -> isize {
    let _ = (sockfd, level, optname, optval, optlen);
    // TODO: 缺失 Linux 强语义: socket option 状态读取、用户 optlen 读写、
    // 协议层选项分发。参考 K3 Linux 6.18.3 net/socket.c:2454。
    unimplemented!("sys_getsockopt: user TODO")
}
