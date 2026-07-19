//! UDP Socket 实现。
//!
//! 实现 `File` trait，使 UDP socket 可以通过 VFS 的 read/write/poll 接口操作。
//!
//! ## 数据流
//!
//! ```text
//! bind:   sys_bind → PORT_TABLE.bind(port, fd) → UdpSock.bind_port = Some(port)
//! send:   sys_sendto/connect → set_target → socket_write/write → send_udp_packet
//! recv:   e1000 IRQ → UDP dst_port → PORT_TABLE.lookup → push_rx_buf
//!         sys_recvfrom/read → pop rx_queue → 拷贝到用户缓冲区
//! close:  File::close → PORT_TABLE.unbind(bind_port)
//! ```
//!
//! ## 对标 Linux
//!
//! | Linux | BlueStarOS |
//! |-------|------------|
//! | `struct udp_sock` (sk_buff receive_queue) | `UdpSock.rx_queue` |
//! | `sk->sk_data_ready` 唤醒 | `WaitQueue::wake()` |
//! | `sk_wait_data` 阻塞 | `WaitQueue::block()` |
//! | `udp_sendmsg` | `write()` → `send_udp_packet()` |
//! | `udp_recvmsg` | `read()` → `rx_queue.pop_front()` |

use core::mem::take;

use alloc::collections::vec_deque::VecDeque;
use log::debug;

use crate::{
    arch::enable_irq,
    driver::network::e1000::{
        ipv4::Ipv4Addr,
        packet::{buffer::NetBuffer, net_endian::Net16, udp::tx::send_udp_packet},
    },
    error::BlueErr,
    fs::{
        semaphore::waitqueue::WaitQueue,
        vfs::{File, PollStatus, VfsFsError},
    },
    network::{porttable::PORT_TABLE, NetPort},
    sync::UPSafeCell,
};

/// UDP 套接字。
///
/// 内部维护:
/// - `target_ip` / `target_port`: 对端地址 (由 `connect` 或 `sendto` 的 sockaddr 设置)
/// - `bind_port`: 本地绑定端口 (由 `bind` 设置)，内核据此将收到的 UDP 包分发到此 socket
/// - `rx_queue`: 接收队列，中断收包路径将剥离 L2/L3/L4 头后的纯数据 NetBuffer 入队
/// - `wait_queue`: 阻塞队列，`read` 在 rx_queue 为空时阻塞当前任务
pub struct UdpSock {
    /// 对端 IP (sendto 时设置)
    target_ip: UPSafeCell<Ipv4Addr>,
    /// 对端端口 (sendto 时设置，网络字节序)
    target_port: UPSafeCell<NetPort>,
    /// 本地绑定端口 (bind 时设置)
    bind_port: UPSafeCell<Option<NetPort>>,
    /// 接收队列 — 塞入剥离 UDP 头后的纯数据 NetBuffer
    rx_queue: UPSafeCell<VecDeque<NetBuffer>>,
    /// 等待队列 — 收到包后唤醒一个阻塞在 read 的线程
    wait_queue: WaitQueue,
}

impl Default for UdpSock {
    fn default() -> Self {
        Self::new()
    }
}

impl UdpSock {
    /// 创建一个空的 UDP socket。
    pub fn new() -> Self {
        Self {
            target_ip: UPSafeCell::new(Ipv4Addr::ZERO),
            target_port: UPSafeCell::new(NetPort(Net16::ZERO)),
            bind_port: UPSafeCell::new(None),
            rx_queue: UPSafeCell::new(VecDeque::new()),
            wait_queue: WaitQueue::new(),
        }
    }

    /// 设置本地绑定端口 (由 sys_bind 调用)。
    pub fn set_bind_port_inner(&self, port: NetPort) {
        self.bind_port.lock(|bp| *bp = Some(port));
    }

    /// 获取本地绑定端口 (主机字节序)。
    fn bind_port_inner(&self) -> Option<u16> {
        self.bind_port.lock(|bp| bp.as_ref().map(|p| p.0.host()))
    }

    /// 设置对端地址 (由 sys_sendto / sys_connect 调用)。
    pub fn set_target_inner(&self, ip: Ipv4Addr, port: NetPort) {
        self.target_ip.lock(|ti| *ti = ip);
        self.target_port.lock(|tp| *tp = port);
    }

    /// 获取对端 IP。
    fn target_ip_inner(&self) -> Ipv4Addr {
        self.target_ip.lock(|ti| *ti)
    }

    /// 获取对端端口 (主机字节序)。
    fn target_port_inner(&self) -> u16 {
        self.target_port.lock(|tp| tp.0.host())
    }

    /// 从接收队列弹出一个 UDP 数据报并拷贝到调用方缓冲区。
    ///
    /// UDP 保留消息边界：一次调用最多消费一个 `NetBuffer`。如果用户缓冲区
    /// 小于数据报长度，尾部会被丢弃，等价于 Linux 未暴露 `MSG_TRUNC` 标志时
    /// 的简化语义。
    fn pop_datagram_into(&self, buf: &mut [u8]) -> Option<usize> {
        self.rx_queue.lock(|q| q.pop_front()).map(|pkt| {
            let copy_len = core::cmp::min(buf.len(), pkt.data_len());
            buf[..copy_len].copy_from_slice(&pkt.data_slice()[..copy_len]);
            copy_len
        })
    }

    /// 发送一个 UDP payload，要求目标地址和本地绑定端口已经设置。
    fn send_payload(&self, buf: &[u8]) -> Result<usize, BlueErr> {
        let dst_ip = self.target_ip_inner();
        let dst_port = self.target_port_inner();

        if dst_ip == Ipv4Addr::ZERO || dst_port == 0 {
            return Err(BlueErr::EDESTADDRREQ);
        }

        let src_port = self.bind_port_inner().ok_or(BlueErr::EINVAL)?;

        let mut payload = NetBuffer::new();
        payload.new_data(buf);
        send_udp_packet(dst_ip.octets(), src_port, dst_port, payload);

        Ok(buf.len())
    }

    /// 中断收包路径调用: 将一个剥离 UDP 头后的 NetBuffer 入队，并唤醒一个等待者。
    ///
    /// 对标 Linux: `__udp_enqueue_schedule_skb` → `sk->sk_data_ready(sk)`
    ///
    /// TODO(IRQ-safety): 当前 e1000 IRQ handler 可能直接调用 `wake()`，它会借用
    /// `TASK_MANAER.task_que_inner`。如果中断打断了已持有该 `UPSafeCell` 的路径，
    /// 会触发双重借用 panic。后续应把唤醒动作延迟到 softirq / bottom half，
    /// 或在进入相关临界区时明确屏蔽网卡中断。
    pub fn push_rx_buf(&self, pkt: NetBuffer) {
        self.rx_queue.lock(|q| q.push_back(pkt));
        self.wait_queue.wake();
    }

    /// Socket 读: 从接收队列取一个数据报 (非阻塞接口)。
    pub fn socket_read(&self, buf: &mut [u8]) -> Result<usize, BlueErr> {
        self.pop_datagram_into(buf).ok_or(BlueErr::EAGAIN)
    }

    /// Socket 写: 发送一个数据报。
    pub fn socket_write(&self, buf: &[u8]) -> Result<usize, BlueErr> {
        self.send_payload(buf)
    }
}

// ── File trait 实现 ────────────────────────────────────────────────

impl File for UdpSock {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    /// 从 UDP socket 读取一个数据报。
    ///
    /// 对标 Linux: `udp_recvmsg` (net/ipv4/udp.c:1725)
    ///
    /// 语义: UDP 是消息边界协议，一次 read 收一个完整数据报。
    /// 如果 buf 小于数据报长度，多余部分被丢弃 (MSG_TRUNC 语义)。
    fn read(&self, buf: &mut [u8]) -> Result<usize, VfsFsError> {
        loop {
            // 开启中断来接受网卡的中断
            enable_irq();

            if let Some(copy_len) = self.pop_datagram_into(buf) {
                return Ok(copy_len);
            }

            self.wait_queue.block();
        }
    }

    /// 向 UDP socket 写入数据并发送。
    ///
    /// 对标 Linux: `udp_sendmsg` (net/ipv4/udp.c:965)
    fn write(&self, buf: &[u8]) -> Result<usize, VfsFsError> {
        self.send_payload(buf).map_err(|_| VfsFsError::Invalid)
    }

    /// 轮询 socket 状态。
    ///
    /// 对标 Linux: `udp_poll` (net/ipv4/udp.c) → `datagram_poll`
    fn poll(&self) -> PollStatus {
        let is_empty = self.rx_queue.lock(|q| q.is_empty());
        if is_empty {
            PollStatus::NONE
        } else {
            PollStatus::POLLIN
        }
    }

    fn close(&self) -> Result<bool, VfsFsError> {
        self.bind_port.lock(|lock| {
            if let Some(port) = lock {
                debug!("unbind port:{}", port.0.host());
                PORT_TABLE.unbind(take(port));
            }
        });
        Ok(true)
    }
}
