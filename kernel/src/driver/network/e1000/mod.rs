//! e1000 网卡驱动顶层模块。
//!
//! 本文件只负责模块编排与兼容导出，不再混放：
//! - 硬件寄存器/全局状态
//! - PCI probe / 中断路径
//! - 协议头定义
//! - RX/TX ring 实现
//!
//! ## 分层约定
//!
//! - `hw`: 设备全局状态、probe、中断屏蔽/确认；
//! - `packet`: 以太网/ARP/IPv4/ICMP/UDP 头部解析与封包；
//! - `queue`: RX/TX 描述符环生命周期和 DMA 内存屏障。
//!
//! 上层 socket 不应直接操作 `queue`，只通过 packet 层发送或由 IRQ 收包分发。
//! 本模块保留旧路径兼容导出，避免一次重构牵动现有调用点。

pub mod hw;
pub mod packet;
pub mod queue;

pub use hw::device::{E1000, E1000_DEV, E1000_RX_INTR_PENDING};
pub use hw::irq::{e1000_intr_handler, e1000_irq_disable, e1000_irq_enable};
pub use hw::probe::{probe_registered_e1000, E1000PcieDeviceTarget};

/// 兼容旧路径：`e1000::agreenment::*`
pub mod agreenment {
    pub use super::packet::protocol::*;
}

/// 兼容旧路径：`e1000::arp::*`
pub mod arp {
    pub use super::packet::arp::*;
}

/// 兼容旧路径：`e1000::icmp::*`
pub mod icmp {
    pub use super::packet::icmp::*;
}

/// 兼容旧路径：`e1000::ipv4::*`
pub mod ipv4 {
    pub use super::packet::ipv4::*;
}

/// 兼容旧路径：`e1000::rx_ringbuffer::*`
pub mod rx_ringbuffer {
    pub use super::queue::rx::*;
}

/// 兼容旧路径：`e1000::tx_ringbuffer::*`
pub mod tx_ringbuffer {
    pub use super::queue::tx::*;
}

/// `NetBuffer` 只在驱动内部使用，继续保持私有模块名。
mod netbuffer {
    pub use super::packet::buffer::*;
}
