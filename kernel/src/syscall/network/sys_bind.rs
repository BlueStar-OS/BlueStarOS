//! sys_bind — 把 fd 绑定到指定的本地端口。
//!
//! ## 作用
//! 将 socket fd 绑定到用户传入的本地 `sockaddr_in`，并把端口登记到内核端口表。
//!
//! ## 参数
//! `fd` 为 socket 文件描述符；`umyaddr` 为用户态 `sockaddr_in` 指针；`addrlen` 为地址结构长度。
//!
//! ## 注意事项
//! 当前实现只支持 IPv4 UDP socket；用户指针按当前任务页表翻译，跨页 sockaddr 尚未完整处理。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: net/socket.c:1908。
//!
//! ## 实现情况
//! 已实现 IPv4 UDP 绑定、端口冲突检测和基础错误码；TODO: 补齐 Linux 的 VFS/socket 引用计数、权限命名空间与完整 sockaddr 拷贝语义。
//!
//! 对标 Linux:
//! - `__sys_bind` (net/socket.c:1633)
//! - `inet_bind` → `__inet_bind` → `udp_lib_get_port` (net/ipv4/af_inet.c:434)
//!
//! ## 输入
//!
//! | 寄存器 | 参数 | 含义 | 示例 |
//! |--------|------|------|------|
//! | a0 | fd | socket 的文件描述符 | 3 |
//! | a1 | umyaddr | 用户态 sockaddr_in 指针 | &{AF_INET, port=8080, addr=0.0.0.0} |
//! | a2 | addrlen | 地址结构体长度 | 16 |
//!
//! ## 输出
//!
//! - 成功: 0
//! - 失败: 负数 errno
//!
//! ## 错误码 (对齐 Linux)
//!
//! | errno | 值 | 触发条件 |
//! |-------|------|----------|
//! | EBADF | 9 | fd 无效 |
//! | ENOTSOCK | 88 | fd 不是 socket |
//! | EAFNOSUPPORT | 97 | sin_family != AF_INET |
//! | EINVAL | 22 | addrlen 不足 / socket 已绑定 |
//! | EACCES | 13 | 端口 < 1024 无权限 |
//! | EADDRINUSE | 98 | 端口已被占用 |
//!
//! ## 状态改变
//!
//! - `UdpSock.bind_port`: `None → Some(port)`
//! - 注册到全局端口映射表 (`PORT_TABLE`)
//!
//! ## 副作用
//!
//! 端口冲突检测: 如果端口已被另一个 socket 占用，返回 `EADDRINUSE`。

use log::error;

use crate::arch::memory::*;
use crate::driver::network::e1000::packet::net_endian::Net16;
use crate::error::BlueErr;
use crate::network::porttable::PORT_TABLE;
use crate::network::udpsock::UdpSock;
use crate::network::NetPort;
use crate::task::TASK_MANAER;

/// 用户态 `struct sockaddr_in` 布局。
///
/// 参考 Linux: include/uapi/linux/in.h
#[repr(C)]
struct SockAddrIn {
    sin_family: u16,
    sin_port: u16,
    sin_addr: [u8; 4],
    sin_zero: [u8; 8],
}

/// sys_bind(fd, umyaddr, addrlen) -> 0 或 -errno
///
/// 对标 Linux `__sys_bind` (net/socket.c:1633-1654)
///
/// 流程:
/// 1. fd → Arc\<dyn File\> (查进程 fd 表)
/// 2. 从用户态拷贝 sockaddr_in 到内核
/// 3. 验证: family == AF_INET, addrlen >= 16
/// 4. 端口冲突检测 (查全局 PORT_TABLE)
/// 5. 注册: PORT_TABLE.bind(port, fd) + sock.set_bind_port(port)
/// 6. return 0
pub fn sys_bind(fd: usize, umyaddr: usize, addrlen: usize) -> isize {
    // 1. fd → Arc<dyn File> → downcast 到 UdpSock
    let file = match TASK_MANAER.get_current_fd(fd) {
        Some(Some(f)) => f,
        _ => {
            error!("sys_bind: invalid fd={}", fd);
            return BlueErr::EBADF.as_isize();
        }
    };
    let sock: &UdpSock = match file.as_any().downcast_ref::<UdpSock>() {
        Some(s) => s,
        None => {
            error!("sys_bind: fd={} is not a UDP socket", fd);
            return BlueErr::ENOTSOCK.as_isize();
        }
    };

    // 2. 验证 addrlen
    if addrlen < core::mem::size_of::<SockAddrIn>() {
        error!(
            "sys_bind: addrlen={} too small (need {})",
            addrlen,
            core::mem::size_of::<SockAddrIn>()
        );
        return BlueErr::EINVAL.as_isize();
    }

    // 3. 从用户态拷贝 sockaddr_in
    let user_satp = TASK_MANAER.get_current_stap();
    let mut tb = PageTable::crate_table_from_satp(user_satp);
    let sa_pa = match tb.translate(VirAddr(umyaddr)) {
        Some(pa) => pa,
        None => {
            error!("sys_bind: translate failed for umyaddr=0x{:x}", umyaddr);
            return BlueErr::EFAULT.as_isize();
        }
    };
    let sa = unsafe { &*(sa_pa.0 as *const SockAddrIn) };

    // 4. 验证地址族
    if sa.sin_family != 2 {
        // AF_INET = 2
        error!("sys_bind: unsupported sin_family={}", sa.sin_family);
        return BlueErr::EAFNOSUPPORT.as_isize();
    }

    // 5. 提取端口 (网络字节序 → 主机字节序)
    let port = u16::from_be(sa.sin_port);

    // 6. 低端口权限检查 (对标 Linux: uid < 1024 需要 CAP_NET_BIND_SERVICE)
    if port < 1024 {
        error!("sys_bind: permission denied for port={}", port);
        return BlueErr::EACCES.as_isize();
    }

    // 7. 端口冲突检测 + 注册到全局端口表。
    //
    // 注意顺序：先注册全局表，再写入 socket 内部状态。若端口已被占用，
    // socket 仍保持未绑定，close 时也不会误 unbind 他人的端口。
    let net_port = NetPort(Net16::new(port));
    if !PORT_TABLE.bind(net_port, file.clone()) {
        error!("sys_bind: port {} already in use", port);
        return BlueErr::EADDRINUSE.as_isize();
    }

    // 8. 设置 socket 内部绑定端口
    sock.set_bind_port_inner(NetPort(Net16::new(port)));

    0
}
