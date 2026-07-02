//! sys_setsockopt — 设置 socket 选项。
//!
//! ## 作用
//! 按 `level + optname` 从用户空间读取选项值，并写入 socket 内部状态。
//!
//! ## 参数
//! `sockfd` 是 socket fd；`level` 是选项层级；`optname` 是选项名；`optval` 指向用户选项值；`optlen` 是选项值字节数。
//!
//! ## 注意事项
//! 当前 socket option 状态表尚未实现；必须保留显式 TODO，避免用户态误以为选项已经生效。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: `net/socket.c:2388`。
//!
//! ## 实现情况
//! 未实现；后续需要按 SOL_SOCKET/IPPROTO_TCP/IPPROTO_UDP 分层落到 socket 状态。

/// SOL_SOCKET 层级。
///
/// 参考: K3 Linux 6.18.3 `include/uapi/asm-generic/socket.h`。
pub const SOL_SOCKET: usize = 1;
pub const SO_REUSEADDR: usize = 2;
pub const SO_TYPE: usize = 3;
pub const SO_ERROR: usize = 4;
pub const SO_BROADCAST: usize = 6;
pub const SO_SNDBUF: usize = 7;
pub const SO_RCVBUF: usize = 8;
pub const SO_KEEPALIVE: usize = 9;
pub const SO_LINGER: usize = 13;

/// TCP 层级与常用 TCP socket option。
///
/// 参考: K3 Linux 6.18.3 `include/uapi/linux/in.h` 与 `include/uapi/linux/tcp.h`。
pub const IPPROTO_TCP: usize = 6;
pub const TCP_NODELAY: usize = 1;
pub const TCP_MAXSEG: usize = 2;
pub const TCP_KEEPIDLE: usize = 4;
pub const TCP_KEEPINTVL: usize = 5;
pub const TCP_KEEPCNT: usize = 6;

/// sys_setsockopt(sockfd, level, optname, optval, optlen) -> 0 或 -errno。
///
/// 作用: 设置 socket 选项。
/// 输入: `sockfd` 选择 socket；`level/optname` 选择选项；`optval/optlen` 给出用户缓冲区。
/// 输出: 成功返回 0；失败返回负 errno。
/// 副作用: 完整实现时会修改 socket 层/协议层选项状态。
pub fn sys_setsockopt(
    sockfd: usize,
    level: usize,
    optname: usize,
    optval: usize,
    optlen: usize,
) -> isize {
    let _ = (sockfd, level, optname, optval, optlen);
    // TODO: 缺失 Linux 强语义: socket option 状态存储、用户 optval 类型解析、
    // 协议层选项分发。参考 K3 Linux 6.18.3 net/socket.c:2388。
    unimplemented!("sys_setsockopt: user TODO")
}
