//! UDP 子模块。
//!
//! - `header.rs`: UDP 头定义 (端口语义化类型)
//! - `helper.rs`: UDP 校验和 (含 IPv4 伪头)
//! - `rx.rs`: UDP 收包入口
//! - `tx.rs`: UDP 发包路径

pub mod header;
pub mod helper;
pub mod rx;
pub mod tx;

pub use header::{DestPort, SourcePort, UdpHeader, UdpPrseHeader, UDP_HEADER_LEN};
pub use helper::{udp_checksum, udp_verify_checksum};
pub use rx::udp_receive;
pub use tx::send_udp_packet;
