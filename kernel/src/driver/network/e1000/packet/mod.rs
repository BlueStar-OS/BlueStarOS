//! e1000 协议与报文缓冲模块。
//!
//! 本层承接 RX ring 交上来的原始以太网帧，并按 L2/L3/L4 顺序逐层剥头。
//! TX 方向则由 UDP/ICMP 等子模块从 payload 反向补齐协议头，再交给 TX ring。
//! 这里保持 wire format 相关逻辑集中，避免 syscall 或 socket 层手写协议头。

use log::{info, warn};

use self::protocol::{EtherType, ETH_HLEN};
use crate::driver::network::e1000::netbuffer::{
    NetBuffer, NetBufferHeaderKind, NetBufferHeaderRef,
};

pub mod arp;
pub mod ethernet;
pub mod icmp;
pub mod ipv4;
pub mod net_endian;
pub mod protocol;
pub mod udp;

#[path = "../netbuffer.rs"]
pub mod buffer;

// 收包分发结果目前只通过日志体现。后续如果需要统计丢包原因，应在这里增加
// 轻量计数，而不是让每个协议层自己维护重复的错误分类。
/// 统一的网络收包分发入口。
///
/// 输入必须是网卡 DMA 刚交上来的原始以太网帧，即 `data` 指向 `EthHead`。
/// 该流程对齐 Linux `eth_type_trans()` 先剥离二层头、再根据协议上送到
/// `arp_receive()` / `ip_rcv()` 的总体思路。
///
/// 参考:
/// - Ethernet 剥头: `/home/inkbottle/桌面/linux-5.4.29/net/ethernet/eth.c:155-196`
/// - IPv4 收包入口: `/home/inkbottle/桌面/linux-5.4.29/net/ipv4/ip_input.c:514-523`
pub fn network_packet_resolve(mut frame: NetBuffer) {
    if frame.data_len() < ETH_HLEN {
        warn!(
            "收到残缺以太网帧，长度 {} 小于 {}",
            frame.data_len(),
            ETH_HLEN
        );
        return;
    }

    let eth_hdr = match frame.as_any(NetBufferHeaderKind::Ethernet) {
        Some(NetBufferHeaderRef::Ethernet(hdr)) => *hdr,
        None => {
            warn!("无法解析以太网头，直接丢弃");
            return;
        }
        Some(_) => unreachable!("Ethernet 视图类型不匹配"),
    };
    let ether_type = eth_hdr.ether_type;
    info!("{}  len={}", eth_hdr, frame.data_len());
    frame.pull(ETH_HLEN);

    match ether_type {
        EtherType::ARP => arp::arp_receive(frame, eth_hdr),
        EtherType::IPV4 => ipv4::ipv4_receive(frame, eth_hdr),
        other => {
            warn!("未支持的 EtherType: {:?}", other);
        }
    }
}
