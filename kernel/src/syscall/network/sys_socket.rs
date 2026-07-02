//! sys_socket — 创建一个内核 Socket 结构体，返回一个 fd。
//!
//! ## 作用
//! 创建 socket 对象并安装到当前进程 fd 表，返回新的文件描述符。
//!
//! ## 参数
//! `family` 为协议族；`type_flags` 为 socket 类型及 `SOCK_CLOEXEC`/`SOCK_NONBLOCK` 标志；`protocol` 为协议号。
//!
//! ## 注意事项
//! 当前只支持 AF_INET + SOCK_DGRAM 的 UDP 路径，未实现 Linux socket layer 的协议族注册表。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: net/socket.c:1759。
//!
//! ## 实现情况
//! 已实现 UDP socket 分配和 fd 安装；TODO: 补齐 TCP、AF_UNIX、协议选择、CLOEXEC/NONBLOCK 状态和 `sock_alloc_file` 等价语义。
//!
//! 对标 Linux:
//! - `__sys_socket` (net/socket.c:1491)
//! - `sock_create` → `__sock_create` → `sock_alloc` + `inet_create`
//! - `sock_map_fd` → `get_unused_fd_flags` + `sock_alloc_file` + `fd_install`
//!
//! ## 输入
//!
//! | 寄存器 | 参数 | 含义 | 示例 |
//! |--------|------|------|------|
//! | a0 | family | 协议族 | AF_INET = 2 |
//! | a1 | type | 套接字类型 (低位) + 标志 (高位) | SOCK_DGRAM = 2 |
//! | a2 | protocol | 具体协议 | 0 (自动选 UDP) |
//!
//! ## 输出
//!
//! - 成功: 返回 fd (≥0)
//! - 失败: 返回负数 errno
//!
//! ## 错误码 (对齐 Linux errno.h)
//!
//! | errno | 值 | 触发条件 |
//! |-------|------|----------|
//! | EAFNOSUPPORT | 97 | family 不支持 (非 AF_INET) |
//! | EPROTONOSUPPORT | 93 | protocol 不支持 |
//! | ESOCKTNOSUPPORT | 94 | type 不支持 (非 SOCK_DGRAM/SOCK_STREAM) |
//! | ENFILE | 23 | 系统级 fd 表已满 |
//! | ENOBUFS | 105 | 内核内存不足 |
//! | EINVAL | 22 | 无效参数 (非法 flags 位) |
//!
//! ## 状态改变
//!
//! - 分配一个新的 `UdpSock` (或 `TcpSock`) 对象
//! - 占用进程 fd 表的一个条目
//! - socket 状态: `SS_UNCONNECTED`, bind_port = None
//!
//! ## 副作用
//!
//! - 无网络副作用: 不发包，不监听，不绑定端口

use alloc::sync::Arc;
use log::error;

use crate::{error::BlueErr, fs::vfs::File, network::udpsock::UdpSock, task::TASK_MANAER};

/// Linux socket 协议族
pub const AF_INET: usize = 2;

/// Linux socket 类型
pub const SOCK_STREAM: usize = 1;
pub const SOCK_DGRAM: usize = 2;

/// type 字段高位标志
pub const SOCK_TYPE_MASK: usize = 0xf;
pub const SOCK_CLOEXEC: usize = 0o2000000;
pub const SOCK_NONBLOCK: usize = 0o4000;

/// sys_socket(family, type, protocol) -> fd
///
/// 对标 Linux `__sys_socket` (net/socket.c:1491-1516)
pub fn sys_socket(family: usize, type_flags: usize, protocol: usize) -> isize {
    let flags = type_flags & !SOCK_TYPE_MASK;
    let sock_type = type_flags & SOCK_TYPE_MASK;

    // 验证 flags 只包含 SOCK_CLOEXEC | SOCK_NONBLOCK
    if flags & !(SOCK_CLOEXEC | SOCK_NONBLOCK) != 0 {
        error!("sys_socket: invalid flags=0x{:x}", flags);
        return BlueErr::EINVAL.as_isize();
    }

    match family {
        AF_INET => {}
        _ => {
            error!("sys_socket: unsupported family={}", family);
            return BlueErr::EAFNOSUPPORT.as_isize();
        }
    }

    match sock_type {
        SOCK_STREAM => { /* TODO TCP — 暂不实现 */ }
        SOCK_DGRAM => { /* TODO UDP */ }
        _ => {
            error!("sys_socket: unsupported sock_type={}", sock_type);
            return BlueErr::ESOCKTNOSUPPORT.as_isize();
        }
    }

    if protocol != 0 && protocol != 17 {
        // 17 = IPPROTO_UDP
        error!("sys_socket: unsupported protocol={}", protocol);
        return BlueErr::EPROTONOSUPPORT.as_isize();
    }

    let sock: Arc<dyn File> = match sock_type {
        SOCK_DGRAM => Arc::new(UdpSock::new()),
        _ => unreachable!(),
    };

    //  3. 分配 fd 并安装到进程 fd 表
    let fd_num = TASK_MANAER.alloc_fd_for_current(sock);
    fd_num as isize
}
