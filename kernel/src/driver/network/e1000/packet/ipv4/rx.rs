//! IPv4 收包入口。
//!
//! 本文件只负责三层 IPv4 基础校验与协议分发框架，不直接实现 UDP/TCP/ICMP
//! 具体处理逻辑。
//!
//! 参考:
//! - Linux IPv4 头布局: `/home/inkbottle/桌面/linux-5.4.29/include/uapi/linux/ip.h:86-106`
//! - Linux 收包入口: `/home/inkbottle/桌面/linux-5.4.29/net/ipv4/ip_input.c:514-523`
//! - Linux 基础校验: `/home/inkbottle/桌面/linux-5.4.29/net/ipv4/ip_input.c:460-505`
//! - Linux 上送协议分发: `/home/inkbottle/桌面/linux-5.4.29/net/ipv4/ip_input.c:178-223`

use log::{debug, info, warn};

use crate::driver::network::e1000::agreenment::{
    EthHead, IPv4Header, Ipv4Protocol, IPV4_HEADER_LEN, IPV4_VERSION,
};
use crate::driver::network::e1000::netbuffer::NetBuffer;
use crate::driver::network::e1000::packet::icmp::icmp_receive;
use crate::driver::network::e1000::packet::udp::udp_receive;

/// 处理一个已经剥离 Ethernet 头部的 IPv4 包。
pub fn ipv4_receive(mut payload: NetBuffer, eth_hdr: EthHead) {
    if payload.data_len() < core::mem::size_of::<IPv4Header>() {
        warn!("IPv4 包长度不足 {} 字节，直接丢弃", IPV4_HEADER_LEN);
        return;
    }

    let Some(ip_hdr) = payload.as_ipv4_header() else {
        warn!("IPv4 头解析失败，直接丢弃");
        return;
    };
    let header_len = ip_hdr.version_ihl.ihl_bytes() as usize;
    let version = ip_hdr.version_ihl.version();
    let total_length = ip_hdr.total_length.host() as usize;
    let protocol = ip_hdr.protocol;

    let ip_checksum = ip_hdr.checksum.host();
    let ip_checksum_host = ip_hdr.checksum_cloc().host();
    if ip_checksum_host != 0 {
        warn!(
            "收到 IPv4 报文但头校验和错误 packet_checksum:{:#x} verify_result:{:#x}，丢弃",
            ip_checksum, ip_checksum_host
        );
        return;
    }

    if version != IPV4_VERSION {
        warn!("收到非 IPv4 报文 version={}，丢弃", version);
        return;
    }
    if header_len < IPV4_HEADER_LEN as usize {
        warn!("IPv4 IHL 非法: {} 字节", header_len);
        return;
    }
    if payload.data_len() < header_len {
        warn!(
            "IPv4 包长度 {} 小于首部长度 {}，丢弃",
            payload.data_len(),
            header_len
        );
        return;
    }
    if total_length < header_len || payload.data_len() < total_length {
        warn!(
            "IPv4 total_length={} 与实际接收长度 {} 不一致，丢弃",
            total_length,
            payload.data_len()
        );
        return;
    }

    info!("{:?}", ip_hdr);

    // 保存一份 IP 头副本，供上层协议使用（pull 后原引用失效）。
    let ip_hdr_copy = *ip_hdr;
    payload.pull(header_len);

    match protocol {
        Ipv4Protocol::UDP => {
            debug!("IPv4 上层协议为 UDP，负载 {} 字节", payload.data_len());
            udp_receive(payload, ip_hdr_copy);
        }
        Ipv4Protocol::TCP => {
            debug!(
                "IPv4 上层协议为 TCP，负载 {} 字节，当前仅保留分发框架",
                payload.data_len()
            );
        }
        Ipv4Protocol::ICMP => {
            debug!("IPv4 上层协议为 ICMP，负载 {} 字节", payload.data_len());
            icmp_receive(payload, eth_hdr, ip_hdr_copy);
        }
        other => {
            warn!(
                "IPv4 上层协议号 {} 暂未支持，负载 {} 字节",
                other.host(),
                payload.data_len()
            );
        }
    }
}
