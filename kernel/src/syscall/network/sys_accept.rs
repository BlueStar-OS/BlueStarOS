//! sys_accept — 接受 TCP 连接。
//!
//! ## 作用
//! 从监听 socket 的已完成连接队列取出一个连接，返回新的已连接 socket fd。
//!
//! ## 参数
//! `sockfd` 是监听 socket fd；`addr` 是可选对端地址写回指针；`addrlen` 是可选地址长度读写指针。
//!
//! ## 注意事项
//! 当前 TCP listen/accept 队列尚未实现，本 syscall 仅保留 Linux ABI 入口并降级到 `sys_accept4(flags=0)`。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: `net/socket.c:2067`。
//!
//! ## 实现情况
//! Stub 降级实现；真实 TCP accept 依赖 TCP socket 状态机、accept 队列、阻塞/唤醒基础设施。

use crate::syscall::network::sys_accept4::sys_accept4;

/// sys_accept(sockfd, addr, addrlen) -> 新连接 fd 或 -errno。
///
/// 作用: 兼容 Linux `accept(2)`，语义等价于 `accept4(..., flags=0)`。
/// 输入: `sockfd` 为监听 fd，`addr/addrlen` 为可空用户写回地址。
/// 输出: 成功返回新 fd；失败返回负 errno。
/// 副作用: 当前无真实连接出队副作用，因为 TCP accept 尚未实现。
pub fn sys_accept(sockfd: usize, addr: usize, addrlen: usize) -> isize {
    sys_accept4(sockfd, addr, addrlen, 0)
}
