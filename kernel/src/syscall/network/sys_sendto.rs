//! sys_sendto — 发送 UDP 数据报。
//!
//! ## 作用
//! 从用户缓冲区取出数据，并通过 socket 发送到指定 IPv4 UDP 目标。
//!
//! ## 参数
//! `fd` 为 socket 文件描述符；`buff`/`len` 描述用户发送缓冲区；`flags` 为发送标志；`addr`/`addr_len` 为可选目标地址。
//!
//! ## 注意事项
//! 当前主要覆盖 UDP/IPv4 路径，未实现 Linux `sendmsg` 的控制消息、路由缓存和完整 flags 语义。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: net/socket.c:2247。
//!
//! ## 实现情况
//! 已实现基础 UDP 发送、目标地址解析和长度限制；TODO: 补齐 MSG_* flags、跨页拷贝、路由错误与异步错误队列。
//!
//! 对标 Linux:
//! - `__sys_sendto` (net/socket.c:1921)
//! - `udp_sendmsg` (net/ipv4/udp.c:965)
//!
//! ## 输入
//!
//! | 寄存器 | 参数 | 含义 | 示例 |
//! |--------|------|------|------|
//! | a0 | fd | socket 的文件描述符 | 3 |
//! | a1 | buff | 用户态数据缓冲区地址 | 指向 "Hello" |
//! | a2 | len | 数据长度 | 5 |
//! | a3 | flags | 发送标志 | 0 |
//! | a4 | addr | 目标 sockaddr_in 指针 | &{10.0.0.1:9090} |
//! | a5 | addr_len | 地址结构体长度 | 16 |
//!
//! ## 输出
//!
//! - 成功: 实际发送的字节数
//! - 失败: 负数 errno
//!
//! ## 错误码 (对齐 Linux)
//!
//! | errno | 值 | 触发条件 |
//! |-------|------|----------|
//! | EBADF | 9 | fd 无效 |
//! | ENOTSOCK | 88 | fd 不是 socket |
//! | EDESTADDRREQ | 89 | 未指定目标地址且 socket 未 connect |
//! | EMSGSIZE | 90 | 数据报超过 65535 - 20 - 8 字节 |
//! | EAFNOSUPPORT | 97 | 目标地址族不是 AF_INET |
//! | ENOBUFS | 105 | 发送缓冲区不足 |
//!
//! ## 流程
//!
//! ```text
//! 用户态 "Hello" + 目标 {10.0.0.1:9090}
//!   ↓
//! 1. fd → Arc<dyn File>
//!   ↓
//! 2. 从用户态拷贝目标地址 (sockaddr_in) → set_target
//!   ↓
//! 3. 从用户态拷贝数据到内核 Vec<u8>
//!   ↓
//! 4. socket_write(data) → send_udp_packet() → e1000_transmit
//!   ↓
//! 5. return len
//! ```
//!
//! ## 副作用
//!
//! - 数据经 e1000 DMA 发送到网线
//! - 如果 socket 未 bind，由 write 层返回错误

use log::error;

use crate::arch::memory::*;
use crate::driver::network::e1000::ipv4::Ipv4Addr;
use crate::driver::network::e1000::packet::net_endian::Net16;
use crate::error::BlueErr;
use crate::network::udpsock::UdpSock;
use crate::network::NetPort;
use crate::task::TASK_MANAER;

/// 用户态 `struct sockaddr_in` 布局。
#[repr(C)]
struct SockAddrIn {
    sin_family: u16,
    sin_port: u16,
    sin_addr: [u8; 4],
    sin_zero: [u8; 8],
}

/// UDP 最大 payload: 65535 - 20 (IP头) - 8 (UDP头) = 65507
const UDP_MAX_PAYLOAD: usize = 65507;

/// sys_sendto(fd, buff, len, flags, addr, addr_len) -> bytes_sent 或 -errno
///
/// 对标 Linux `__sys_sendto` (net/socket.c:1921-1958)
pub fn sys_sendto(
    fd: usize,
    buff: usize,
    len: usize,
    _flags: usize,
    addr: usize,
    addr_len: usize,
) -> isize {
    // 1. fd → Arc<dyn File> → downcast 到 UdpSock
    let file = match TASK_MANAER.get_current_fd(fd) {
        Some(Some(f)) => f,
        _ => {
            error!("sys_sendto: invalid fd={}", fd);
            return BlueErr::EBADF.as_isize();
        }
    };
    let sock: &UdpSock = match file.as_any().downcast_ref::<UdpSock>() {
        Some(s) => s,
        None => {
            error!("sys_sendto: fd={} is not a UDP socket", fd);
            return BlueErr::ENOTSOCK.as_isize();
        }
    };

    // 2. 从用户态拷贝目标地址 (如果提供了 addr)
    let user_satp = TASK_MANAER.get_current_stap();
    if addr != 0 {
        if addr_len < core::mem::size_of::<SockAddrIn>() {
            error!(
                "sys_sendto: addr_len={} too small (need {})",
                addr_len,
                core::mem::size_of::<SockAddrIn>()
            );
            return BlueErr::EINVAL.as_isize();
        }

        let mut tb = PageTable::crate_table_from_satp(user_satp);
        let sa_pa = match tb.translate(VirAddr(addr)) {
            Some(pa) => pa,
            None => {
                error!("sys_sendto: translate failed for addr=0x{:x}", addr);
                return BlueErr::EFAULT.as_isize();
            }
        };
        let sa = unsafe { &*(sa_pa.0 as *const SockAddrIn) };

        // 验证地址族
        if sa.sin_family != 2 {
            // AF_INET
            error!("sys_sendto: unsupported sin_family={}", sa.sin_family);
            return BlueErr::EAFNOSUPPORT.as_isize();
        }

        let dst_port = u16::from_be(sa.sin_port);
        sock.set_target_inner(Ipv4Addr::new(sa.sin_addr), NetPort(Net16::new(dst_port)));
    }

    // 3. 检查数据长度
    if len > UDP_MAX_PAYLOAD {
        error!(
            "sys_sendto: len={} exceeds UDP_MAX_PAYLOAD={}",
            len, UDP_MAX_PAYLOAD
        );
        return BlueErr::EMSGSIZE.as_isize();
    }

    // 4. 从用户态拷贝数据到内核
    let mut tb2 = PageTable::crate_table_from_satp(user_satp);
    let data = match tb2.read_bytes_from_userspace(VirAddr(buff), len) {
        Some(d) => d,
        None => {
            error!(
                "sys_sendto: read_bytes_from_userspace failed for buff=0x{:x} len={}",
                buff, len
            );
            return BlueErr::EFAULT.as_isize();
        }
    };

    // 5. 发送。
    //
    // `socket_write` 会统一检查目标地址和本地绑定端口：
    // - 未设置目标地址：EDESTADDRREQ
    // - 未 bind 本地端口：EINVAL（当前简化实现不自动分配临时端口）
    match sock.socket_write(&data) {
        Ok(sent) => sent as isize,
        Err(e) => {
            error!("sys_sendto: socket_write failed: {:?}", e);
            e.as_isize()
        }
    }
}
