//! sys_connect — 发起 TCP 连接 (POSIX connect)。
//!
//! ## 作用
//! 为 socket 建立默认对端；TCP 语义下应发起连接握手，UDP 语义下记录默认目标地址。
//!
//! ## 参数
//! `sockfd` 为 socket 文件描述符；`addr` 为用户态目标地址指针；`addrsz` 为地址结构长度。
//!
//! ## 注意事项
//! 当前仍是占位实现；不能静默假装连接成功，否则会破坏 POSIX socket 状态机和应用 ABI。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: net/socket.c:2124。
//!
//! ## 实现情况
//! 未实现，保留显式 TODO/占位；TODO: 依赖 TCP 状态机、非阻塞 EINPROGRESS、socket 等待队列与错误回传。
//!
//! 对标 Linux:
//! - `__NR_connect` = 203 (include/uapi/asm-generic/unistd.h)
//! - 内核入口: `__sys_connect` (net/socket.c:1635)
//!
//! ## Linux riscv64 ABI
//!
//! | 寄存器 | 参数 | 含义 |
//! |--------|------|------|
//! | a0 | sockfd | socket 文件描述符 |
//! | a1 | addr | 目标地址指针 (`struct sockaddr_in *`) |
//! | a2 | addrsz | 地址结构体大小 (字节) |
//!
//! ## 返回值
//!
//! - `= 0`: 连接成功建立
//! - `< 0`: 错误 (负 errno)
//!
//! ## errno
//!
//! | errno | 值 | 触发条件 |
//! |-------|------|----------|
//! | EADDRINUSE | 98 | 本地地址已被占用 |
//! | ECONNREFUSED | 111 | 远端拒绝连接 (TCP RST) |
//! | EINPROGRESS | 115 | 非阻塞 socket，连接进行中 |
//! | EINTR | 4 | 被信号中断 |
//! | ENETUNREACH | 101 | 网络不可达 |
//! | ETIMEDOUT | 110 | 连接超时 |
//!
//! ## TCP 三次握手 (connect 发起)
//!
//! ```text
//! Client                          Server
//!   │── SYN ──────────────────►│
//!   │◄── SYN-ACK ──────────────│
//!   │── ACK ──────────────────►│
//!   │                           │
//!   ╘═ ESTABLISHED ═════════════╛
//! ```
//!
//! ## 阻塞 vs 非阻塞
//!
//! | socket 模式 | connect 行为 |
//! |-------------|-------------|
//! | O_BLOCK | 阻塞直到三次握手完成或超时 |
//! | O_NONBLOCK | 立即返回 -EINPROGRESS，用 poll/select 检测完成 |
//!
//! ## sockaddr_in 布局 (include/uapi/linux/in.h)
//!
//! ```text
//! struct sockaddr_in {
//!     sa_family_t sin_family;  // AF_INET (2)
//!     __be16      sin_port;    // 端口号 (网络字节序)
//!     struct in_addr sin_addr; // IP 地址 (网络字节序)
//!     char        sin_addr[8]; // 填充对齐
//! };
//! ```
//!
//! ## UDP connect 的特殊语义
//!
//! UDP socket 调用 connect:
//! - 不发送任何包
//! - 记录默认目标地址 (后续 sendto 可省略地址参数)
//! - 只接收来自该地址的数据报
//!
//! 参考: POSIX.1-2017, connect(3p)
//! 参考: net/socket.c:__sys_connect

use crate::error::BlueErr;

/// sys_connect(sockfd, addr, addrsz) -> 0 或 -errno
///
/// 向目标地址发起连接 (TCP: 三次握手; UDP: 记录默认地址)。
///
/// TODO: 用户自行实现
pub fn sys_connect(sockfd: usize, addr: usize, addrsz: usize) -> isize {
    // TODO: 实现步骤
    // 1. 获取 sockfd 对应的 socket
    // 2. 从用户空间拷贝 sockaddr_in (验证 addrsz)
    // 3. 提取目标 IP 和端口
    // 4. TCP: 发送 SYN，等待三次握手完成
    // 5. UDP: 记录默认目标地址，返回 0
    // 6. 非阻塞: 立即返回 -EINPROGRESS

    unimplemented!("sys_connect: user TODO")
}
