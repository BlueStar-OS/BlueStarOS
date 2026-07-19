//! 内核网络抽象层。
//!
//! 本层位于 syscall 与具体网卡驱动之间：
//! - `udpsock` 保存 socket 状态，并通过 VFS `File` trait 暴露读写接口；
//! - `porttable` 维护本地端口到 socket 的分发表；
//! - e1000 驱动只负责收发以太网帧，协议分发完成后再回到本层。
//!
//! 当前只实现 IPv4/UDP 的最小闭环。新增 TCP 或更多协议时，应优先在本层
//! 增加协议无关的 socket 状态，而不是把 syscall 语义下沉到驱动。

use crate::driver::network::e1000::agreenment::Net16;

pub mod porttable;
pub mod udpsock;

/// UDP/TCP 端口号的网络层封装。
///
/// 内部保持 `Net16`，避免端口在 syscall、协议头和驱动路径中反复手写
/// `to_be` / `from_be`。需要展示或比较主机序端口时使用 `Net16::host()`。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct NetPort(pub Net16);
