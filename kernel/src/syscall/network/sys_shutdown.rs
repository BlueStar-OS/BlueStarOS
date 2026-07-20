//! sys_shutdown — 关闭 socket 连接的一部分 (POSIX shutdown)。
//!
//! ## 作用
//! 按 `how` 关闭 socket 的读端、写端或读写两端。
//!
//! ## 参数
//! `sockfd` 为 socket 文件描述符；`how` 为 `SHUT_RD`、`SHUT_WR` 或 `SHUT_RDWR`。
//!
//! ## 注意事项
//! 当前仍是占位实现；Linux 的 shutdown 会影响同一 socket 的所有 fd 引用，而不是只关闭某个 fd。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: net/socket.c:2489。
//!
//! ## 实现情况
//! 未实现，保留显式 TODO/占位；TODO: 依赖 socket 共享状态、TCP FIN/RST 状态机和读写半关闭标志。
//!
//! 对标 Linux:
//! - `__NR_shutdown` = 210 (include/uapi/asm-generic/unistd.h)
//! - 内核入口: `__sys_shutdown` (net/socket.c:2044)
//!
//! ## Linux riscv64 ABI
//!
//! | 寄存器 | 参数 | 含义 |
//! |--------|------|------|
//! | a0 | sockfd | socket fd |
//! | a1 | how | 关闭方式 (SHUT_RD / SHUT_WR / SHUT_RDWR) |
//!
//! ## 返回值
//!
//! - `= 0`: 成功
//! - `< 0`: 错误 (负 errno)
//!
//! ## errno
//!
//! | errno | 值 | 触发条件 |
//! |-------|------|----------|
//! | EBADF | 9 | sockfd 无效 |
//! | EINVAL | 22 | how 无效 |
//! | ENOTSOCK | 88 | sockfd 不是 socket |
//! | ENOTCONN | 107 | socket 未连接 |
//!
//! ## how 常量 (include/uapi/linux/net.h)
//!
//! | 常量 | 值 | 含义 |
//! |------|------|------|
//! | SHUT_RD | 0 | 关闭读端 (不再接收数据) |
//! | SHUT_WR | 1 | 关闭写端 (发送 FIN，不再发送数据) |
//! | SHUT_RDWR | 2 | 关闭读写 (等价于 SHUT_RD + SHUT_WR) |
//!
//! ## 与 close() 的区别
//!
//! | 操作 | 影响 | 引用计数 |
//! |------|------|----------|
//! | close(fd) | 关闭 fd，引用计数减一。到零时释放 socket | 减一 |
//! | shutdown(fd, SHUT_WR) | 发送 FIN，对端收到 EOF | 不影响 fd 引用计数 |
//! | shutdown(fd, SHUT_RD) | 丢弃接收缓冲区数据 | 不影响 |
//!
//! ## TCP 连接终止 (shutdown SHUT_WR)
//!
//! ```text
//! 主动关闭方                    被动关闭方
//!   │── FIN ──────────────────►│
//!   │◄── ACK ──────────────────│
//!   │◄── FIN ──────────────────│
//!   │── ACK ──────────────────►│
//! ```
//!
//! `shutdown(SHUT_WR)` 发送 FIN，进入半关闭 (CLOSE_WAIT / FIN_WAIT_1)。
//! 对端 read() 返回 0 (EOF)。
//!
//! ## fork 后的 shutdown 语义
//!
//! fork 后多个 fd 指向同一 socket:
//! - close() 只减少引用计数
//! - shutdown() 影响所有引用该 socket 的 fd
//! - 典型: 父进程 shutdown(SHUT_WR) 通知子进程 EOF
//!
//! 参考: POSIX.1-2017, shutdown(3p)
//! 参考: net/socket.c:__sys_shutdown


/// 关闭方式常量。
pub const SHUT_RD: usize = 0;
pub const SHUT_WR: usize = 1;
pub const SHUT_RDWR: usize = 2;

/// sys_shutdown(sockfd, how) -> 0 或 -errno
///
/// 关闭 socket 的读端、写端或两端。
///
/// TODO: 用户自行实现
pub fn sys_shutdown(_sockfd: usize, _how: usize) -> isize {
    // TODO: 实现步骤
    // 1. 获取 sockfd 对应的 socket
    // 2. match how {
    //     SHUT_RD => 丢弃接收缓冲区，后续 read 返回 EOF
    //     SHUT_WR => 发送 FIN (TCP)，后续 write 返回 -EPIPE
    //     SHUT_RDWR => SHUT_RD + SHUT_WR
    //   }
    // 3. TCP: 触发连接终止握手
    // 4. UDP: 标记为不可用 (无实际网络操作)
    // 5. 返回 0

    unimplemented!("sys_shutdown: user TODO")
}
