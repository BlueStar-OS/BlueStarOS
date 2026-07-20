//! sys_socketpair — 创建一对相互连接的 socket (POSIX socketpair)。
//!
//! ## 作用
//! 创建一对相互连接的本地 socket，用于进程内或父子进程 IPC。
//!
//! ## 参数
//! `domain` 为协议族；`type_` 为 socket 类型和标志；`protocol` 为协议号；`sv` 为用户态 `[fd; 2]` 输出地址。
//!
//! ## 注意事项
//! 当前仍是占位实现；Linux 主要支持 AF_UNIX socketpair，要求两个 fd 共享一组内核缓冲/唤醒状态。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: net/socket.c:1860。
//!
//! ## 实现情况
//! 未实现，保留显式 TODO/占位；TODO: 依赖 AF_UNIX/socketpair 缓冲区、fd 原子安装和错误回滚。
//!
//! 对标 Linux:
//! - `__NR_socketpair` = 199 (include/uapi/asm-generic/unistd.h)
//! - 内核入口: `__sys_socketpair` (net/socket.c:1484)
//!
//! ## Linux riscv64 ABI
//!
//! | 寄存器 | 参数 | 含义 |
//! |--------|------|------|
//! | a0 | domain | 协议族 (仅 AF_UNIX 支持) |
//! | a1 | type | socket 类型 (SOCK_STREAM / SOCK_DGRAM) |
//! | a2 | protocol | 协议 (通常为 0) |
//! | a3 | sv | 输出: 两个 fd 的数组指针 `[2]` |
//!
//! ## 返回值
//!
//! - `= 0`: 成功，sv[0] 和 sv[1] 已填充
//! - `< 0`: 错误 (负 errno)
//!
//! ## errno
//!
//! | errno | 值 | 触发条件 |
//! |-------|------|----------|
//! | EAFNOSUPPORT | 97 | domain 不是 AF_UNIX |
//! | EINVAL | 22 | type 或 protocol 无效 |
//! | EMFILE | 24 | 进程 fd 表满 |
//! | ENFILE | 23 | 系统 fd 表满 |
//! | ENOPROTOOPT | 92 | protocol 不支持 |
//! | EOPNOTSUPP | 95 | 该 domain 不支持 socketpair |
//!
//! ## type 标志位
//!
//! | 常量 | 值 | 含义 |
//! |------|------|------|
//! | SOCK_STREAM | 1 | 可靠双向字节流 (TCP 语义) |
//! | SOCK_DGRAM | 2 | 数据报 (UDP 语义) |
//! | SOCK_NONBLOCK | 04000 | 非阻塞模式 |
//! | SOCK_CLOEXEC | 02000000 | close-on-exec |
//!
//! ## 语义
//!
//! ```text
//! socketpair(AF_UNIX, SOCK_STREAM, 0, sv)
//!
//! 进程 A (sv[0])  ◄═══════►  进程 B (sv[1])
//!   write(sv[0], ...)  →  read(sv[1], ...)
//!   write(sv[1], ...)  →  read(sv[0], ...)
//! ```
//!
//! - 两个 fd 之间有内核内部缓冲区连接
//! - `fork()` 后父子进程各持一端，实现 IPC
//! - 不需要 bind / listen / accept
//! - 关闭一端后另一端读到 EOF
//!
//! ## AF_UNIX vs AF_INET
//!
//! socketpair 仅支持 AF_UNIX (本地 IPC)。
//! AF_INET 需要 socket + bind + listen + accept 或 connect。
//!
//! ## fork + socketpair 的经典用法
//!
//! ```text
//! socketpair(AF_UNIX, SOCK_STREAM, 0, sv)
//! if fork() == 0:
//!     close(sv[0])
//!     // 子进程使用 sv[1] 与父进程通信
//! else:
//!     close(sv[1])
//!     // 父进程使用 sv[0] 与子进程通信
//! ```
//!
//! 参考: POSIX.1-2017, socketpair(3p)
//! 参考: net/socket.c:__sys_socketpair


/// AF_UNIX 协议族。
pub const AF_UNIX: usize = 1;

/// sys_socketpair(domain, type, protocol, sv) -> 0 或 -errno
///
/// 创建一对相互连接的 socket，用于进程间通信。
///
/// TODO: 用户自行实现
pub fn sys_socketpair(_domain: usize, _type_: usize, _protocol: usize, _sv: usize) -> isize {
    // TODO: 实现步骤
    // 1. 验证 domain == AF_UNIX
    // 2. 创建两个 pipe 或内部 socket 实例
    // 3. 分配两个 fd，分别指向 socket 两端
    // 4. 写入 [fd0, fd1] 到用户空间 sv
    // 5. 设置 SOCK_CLOEXEC / SOCK_NONBLOCK (从 type 中提取)
    // 6. 返回 0

    unimplemented!("sys_socketpair: user TODO")
}
