//! sys_listen — 标记 socket 为被动监听状态 (POSIX listen)。
//!
//! ## 作用
//! 将已绑定的流式 socket 切换为监听状态，后续由 accept/accept4 取出已完成连接。
//!
//! ## 参数
//! `sockfd` 为 socket 文件描述符；`backlog` 为完成连接队列长度提示。
//!
//! ## 注意事项
//! 当前仍是占位实现；Linux 会对 backlog 做 somaxconn 截断，并维护半连接/完成连接队列。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: net/socket.c:1946。
//!
//! ## 实现情况
//! 未实现，保留显式 TODO/占位；TODO: 依赖 TCP listen 状态、accept 队列、SYN 处理和 socket 唤醒机制。
//!
//! 对标 Linux:
//! - `__NR_listen` = 201 (include/uapi/asm-generic/unistd.h)
//! - 内核入口: `__sys_listen` (net/socket.c:1533)
//!
//! ## Linux riscv64 ABI
//!
//! | 寄存器 | 参数 | 含义 |
//! |--------|------|------|
//! | a0 | sockfd | socket 文件描述符 |
//! | a1 | backlog | 连接等待队列最大长度 |
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
//! | EADDRINUSE | 98 | 另一个 socket 已在监听同一端口 |
//! | EBADF | 9 | sockfd 无效 |
//! | EINVAL | 22 | socket 未 bind 或已连接 |
//! | ENOTSOCK | 88 | sockfd 不是 socket |
//! | EOPNOTSUPP | 95 | socket 类型不支持 listen (如 UDP) |
//!
//! ## backlog 语义
//!
//! | 值 | 含义 |
//! |------|------|
//! | > 0 | 内核截断到系统上限 (`net.core.somaxconn`，默认 128) |
//! | = 0 | 行为依赖实现，通常为 1 |
//!
//! ## TCP 状态机 (LISTEN 状态)
//!
//! ```text
//! CLOSED → [bind] → BOUND → [listen] → LISTEN → [accept] → ESTABLISHED
//! ```
//!
//! listen 后 socket 进入 LISTEN 状态:
//! - 接收 SYN → 回复 SYN-ACK → 连接进入 accept 队列
//! - accept 队列满时: 根据 `tcp_abort_on_overflow` 丢弃或回复 RST
//!
//! ## 与 bind/accept 的关系
//!
//! ```text
//! server = socket(AF_INET, SOCK_STREAM, 0)
//! bind(server, addr, len)
//! listen(server, backlog)     ← 本系统调用
//! client_fd = accept(server, ...)  ← 阻塞等待连接
//! ```
//!
//! 参考: POSIX.1-2017, listen(3p)
//! 参考: net/socket.c:__sys_listen


/// sys_listen(sockfd, backlog) -> 0 或 -errno
///
/// 将 socket 标记为被动监听状态，准备接受连接。
///
/// TODO: 用户自行实现
pub fn sys_listen(_sockfd: usize, _backlog: usize) -> isize {
    // TODO: 实现步骤
    // 1. 通过 TASK_MANAER 获取 fd 对应的 Arc<dyn File>
    // 2. downcast 到具体 socket 类型 (TcpSock?)
    // 3. 检查 socket 已 bind
    // 4. 设置 socket 状态为 LISTEN
    // 5. 分配 accept 队列 (backlog 大小)
    // 6. 返回 0

    unimplemented!("sys_listen: user TODO")
}
