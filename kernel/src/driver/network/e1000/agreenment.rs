//! 兼容旧路径的协议导出层。
//!
//! 实际定义已经拆分到 `packet/` 各协议子模块：
//! - `packet/ethernet/`
//! - `packet/ipv4/header.rs`
//! - `packet/udp/`
//! - `packet/net_endian.rs`

pub use crate::driver::network::e1000::packet::protocol::*;
