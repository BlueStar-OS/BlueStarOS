//! sys_recvfrom — 接收 UDP 数据报 (阻塞)。
//!
//! ## 作用
//! 从 socket 接收一个数据报到用户缓冲区，并可选写回发送方地址。
//!
//! ## 参数
//! `fd` 为 socket 文件描述符；`ubuf`/`size` 描述用户接收缓冲区；`flags` 为接收标志；`addr`/`addr_len` 为可选发送方地址输出。
//!
//! ## 注意事项
//! 当前发送方地址写回仍是降级路径，未完整携带每个 NetBuffer 的源地址元数据。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: net/socket.c:2305。
//!
//! ## 实现情况
//! 已实现 UDP 数据读取和 MSG_DONTWAIT 基础分支；TODO: 补齐源地址写回、跨页用户缓冲区和 Linux sk_wait_data 阻塞唤醒语义。
//!
//! 对标 Linux:
//! - `__sys_recvfrom` (net/socket.c:1982)
//! - `udp_recvmsg` (net/ipv4/udp.c:1725)
//!
//! ## 输入
//!
//! | 寄存器 | 参数 | 含义 | 示例 |
//! |--------|------|------|------|
//! | a0 | fd | socket 的文件描述符 | 3 |
//! | a1 | ubuf | 用户态接收缓冲区地址 | buf[1024] |
//! | a2 | size | 缓冲区大小 | 1024 |
//! | a3 | flags | 接收标志 (MSG_DONTWAIT 等) | 0 |
//! | a4 | addr | 输出: 发送方 sockaddr_in 指针 | &out_addr |
//! | a5 | addr_len | 输出: 地址长度指针 | &16 |
//!
//! ## 输出
//!
//! - 成功: 接收的字节数
//! - 失败: 负数 errno
//!
//! ## 错误码 (对齐 Linux)
//!
//! | errno | 值 | 触发条件 |
//! |-------|------|----------|
//! | EBADF | 9 | fd 无效 |
//! | ENOTSOCK | 88 | fd 不是 socket |
//! | EAGAIN | 11 | 非阻塞且无数据 |
//! | EFAULT | 14 | 用户缓冲区地址无效 |
//!
//! ## 流程
//!
//! ```text
//! sys_recvfrom(fd, buf, size, flags, addr, addr_len)
//!   ↓
//! 1. fd → Arc<dyn File>
//!   ↓
//! 2. 调用 read() 阻塞等待数据 (或 socket_read 非阻塞)
//!   ↓
//! 3. 拷贝数据到用户态 buf
//!   ↓
//! 4. 如果 addr != 0: 填写发送方地址 (当前填零，TODO: 从 NetBuffer 提取)
//!   ↓
//! 5. return 字节数
//! ```
//!
//! ## 状态改变
//!
//! - rx_queue: 消费一个 NetBuffer
//! - 进程: 可能从 Running → Blocking → Running (如果队列为空且为阻塞模式)
//!
//! ## 副作用
//!
//! - 阻塞模式下进程睡眠，等待中断收包路径唤醒
//! - 一次 recvfrom 收一个完整数据报 (UDP 消息边界语义)

use log::{debug, error};

use crate::arch::memory::*;
use crate::error::BlueErr;
use crate::network::udpsock::UdpSock;
use crate::task::TASK_MANAER;

/// 用户态 `struct sockaddr_in` 布局。
#[repr(C)]
struct SockAddrIn {
    sin_family: u16,
    sin_port: u16,
    sin_addr: [u8; 4],
    sin_zero: [u8; 8],
}

/// recvfrom flags
pub const MSG_DONTWAIT: usize = 0x40;

/// sys_recvfrom(fd, ubuf, size, flags, addr, addr_len) -> bytes_received 或 -errno
///
/// 对标 Linux `__sys_recvfrom` (net/socket.c:1982-2021)
pub fn sys_recvfrom(
    fd: usize,
    ubuf: usize,
    size: usize,
    flags: usize,
    addr: usize,
    _addr_len: usize,
) -> isize {
    debug!(
        "sys_recvfrom: fd={} ubuf=0x{:x} size={} flags=0x{:x} addr=0x{:x}",
        fd, ubuf, size, flags, addr
    );
    // 1. fd → Arc<dyn File> → downcast 到 UdpSock
    let file = match TASK_MANAER.get_current_fd(fd) {
        Some(Some(f)) => f,
        _ => {
            error!("sys_recvfrom: invalid fd={}", fd);
            return BlueErr::EBADF.as_isize();
        }
    };
    let sock: &UdpSock = match file.as_any().downcast_ref::<UdpSock>() {
        Some(s) => s,
        None => {
            error!("sys_recvfrom: fd={} is not a UDP socket", fd);
            return BlueErr::ENOTSOCK.as_isize();
        }
    };

    // 2. 接收数据
    let mut buf = alloc::vec![0u8; size];
    let received = if flags & MSG_DONTWAIT != 0 {
        // 非阻塞: 调用 socket_read，无数据立即返回 EAGAIN
        match sock.socket_read(&mut buf) {
            Ok(n) => n,
            Err(e) => {
                error!("sys_recvfrom: socket_read failed (non-blocking): {:?}", e);
                return e.as_isize();
            }
        }
    } else {
        // 阻塞: 调用 read，队列为空时进程睡眠
        match file.read(&mut buf) {
            Ok(n) => n,
            Err(e) => {
                error!("sys_recvfrom: read failed (blocking): {:?}", e);
                return BlueErr::EAGAIN.as_isize();
            }
        }
    };

    // 3. 拷贝数据到用户态。
    //
    // `received` 是一次 UDP 数据报被裁剪后的实际返回长度；如果用户缓冲区
    // 小于原始数据报，尾部已经在 `UdpSock` 层按 UDP 消息边界语义丢弃。
    let user_satp = TASK_MANAER.get_current_stap();
    let mut slices = PageTable::get_mut_slice_from_satp(user_satp, received, VirAddr(ubuf));
    let mut offset = 0;
    for slice in slices.iter_mut() {
        let copy_len = core::cmp::min(slice.len(), received - offset);
        slice[..copy_len].copy_from_slice(&buf[offset..offset + copy_len]);
        offset += copy_len;
        if offset >= received {
            break;
        }
    }

    // 4. 填写发送方地址 (如果 addr != 0)
    // 当前实现: 填零 (TODO: 从 NetBuffer 提取源 IP/端口)
    if addr != 0 {
        let sa = SockAddrIn {
            sin_family: 2, // AF_INET
            sin_port: 0,
            sin_addr: [0; 4],
            sin_zero: [0; 8],
        };
        let sa_bytes = unsafe {
            core::slice::from_raw_parts(
                &sa as *const SockAddrIn as *const u8,
                core::mem::size_of::<SockAddrIn>(),
            )
        };
        let _tb = PageTable::crate_table_from_satp(user_satp);
        let mut slices =
            PageTable::get_mut_slice_from_satp(user_satp, sa_bytes.len(), VirAddr(addr));
        let mut off = 0;
        for slice in slices.iter_mut() {
            let copy_len = core::cmp::min(slice.len(), sa_bytes.len() - off);
            (*slice)[..copy_len].copy_from_slice(&sa_bytes[off..off + copy_len]);
            off += copy_len;
            if off >= sa_bytes.len() {
                break;
            }
        }
    }

    received as isize
}
