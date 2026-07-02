//! 网络系统调用模块。
//!
//! 提供 POSIX socket API 的内核实现:
//! - `sys_socket`: 创建 socket，返回 fd
//! - `sys_bind`: 绑定本地端口
//! - `sys_connect`: 设置默认对端地址（当前作为 TODO syscall 保留）
//! - `sys_sendto`: 发送 UDP 数据报
//! - `sys_recvfrom`: 接收 UDP 数据报 (阻塞)
//! - `sys_close` 间接调用 socket 的 `File::close`，释放端口绑定
//!
//! ## 当前职责边界
//!
//! syscall 层只负责 Linux ABI、用户地址校验、fd 查找和 errno 返回；
//! socket 状态由 `network::udpsock` 维护，协议封包和 DMA 发送由 e1000
//! packet/queue 层负责。不要在 syscall 中直接操作描述符环或协议头。
//!
//! ## 错误码对齐
//!
//! 所有错误码均对齐 Linux errno.h，通过 `BlueErr::as_isize()` 返回负数。
//! 如果下层返回的是 VFS 错误，本模块需要在 syscall 边界转换成最接近的
//! `BlueErr`，避免用户态看到 VFS 内部错误语义。

pub mod sys_accept;
pub mod sys_accept4;
pub mod sys_bind;
pub mod sys_connect;
pub mod sys_getsockopt;
pub mod sys_listen;
pub mod sys_recvfrom;
pub mod sys_sendto;
pub mod sys_setsockopt;
pub mod sys_shutdown;
pub mod sys_socket;
pub mod sys_socketpair;
