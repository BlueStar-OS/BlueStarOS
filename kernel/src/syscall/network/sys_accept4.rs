//! sys_accept4 — 接受 TCP 连接并可设置新 fd 标志。
//!
//! ## 作用
//! 从监听 socket 的已完成连接队列取出一个连接，并按 `flags` 设置返回 fd 的属性。
//!
//! ## 参数
//! `sockfd` 是监听 socket fd；`addr` 是可选对端地址写回指针；`addrlen` 是可选地址长度读写指针；`flags` 支持 `SOCK_CLOEXEC` / `SOCK_NONBLOCK`。
//!
//! ## 注意事项
//! 当前 BlueStarOS 还没有 TCP accept 队列、socket 阻塞等待、CLOEXEC/NONBLOCK fd 属性完整基础设施，因此必须显式保留 TODO，不能静默伪装成功。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: `net/socket.c:2061`。
//!
//! ## 实现情况
//! 未实现；调用会触发 TODO panic，后续应改为返回精确 errno 或完整实现 TCP accept。

/// sys_accept4(sockfd, addr, addrlen, flags) -> 新连接 fd 或 -errno。
///
/// 作用: 接受一个已完成 TCP 连接。
/// 输入: `sockfd` 为监听 fd；`addr/addrlen` 为可空用户地址；`flags` 为 accept4 标志。
/// 输出: 成功返回新 fd；失败返回负 errno。
/// 副作用: 完整实现时会消耗 listen socket 的 accept 队列项并分配新 fd。
pub fn sys_accept4(sockfd: usize, addr: usize, addrlen: usize, flags: usize) -> isize {
    let _ = (sockfd, addr, addrlen, flags);
    // TODO: 缺失 Linux 强语义: TCP listen/accept 队列、socket 阻塞/非阻塞等待、
    // fd close-on-exec 与 nonblock 标志传播。参考 K3 Linux 6.18.3 net/socket.c:2061。
    unimplemented!("sys_accept4: user TODO")
}
