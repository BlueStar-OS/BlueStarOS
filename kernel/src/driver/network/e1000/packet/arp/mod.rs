//! ARP 子模块。
//!
//! - `cache.rs`: ARP 缓存表
//! - `packet.rs`: ARP 线速报文头
//! - `rx.rs`: 以太网剥头后的 ARP 收包逻辑

pub mod cache;
pub mod packet;
pub mod rx;

pub use cache::{ArpTable, ARP_TABLE};
pub use packet::{ArpHeader, ARP_OPCODE_REPLY, ARP_OPCODE_REQUEST};
pub use rx::{arp_receive, raw_ethdat_recivea, send_arp_packet, trigger_alignment_fault};
