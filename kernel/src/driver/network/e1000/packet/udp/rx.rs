//! UDP 收包入口。
//!
//! 负责:
//! 1. UDP 头长度校验
//! 2. UDP 校验和验证 (含 IPv4 伪头)
//! 3. 剥离 UDP 头, 上送应用层
//!
//! 参考 Linux:
//! - `/home/inkbottle/桌面/linux-5.4.29/net/ipv4/udp.c:2049-2087`

use alloc::string::String;
use log::{debug, info, warn};

use crate::driver::network::e1000::agreenment::{IPv4Header, Net16};
use crate::driver::network::e1000::netbuffer::NetBuffer;
use crate::network::porttable::PORT_TABLE;
use crate::network::udpsock::UdpSock;
use crate::network::NetPort;

use super::header::{UdpHeader, UDP_HEADER_LEN};
use super::helper::udp_verify_checksum;

/// 处理一个已经剥离 IPv4 头的 UDP 报文。
///
/// `payload` 的 data 指向 UDP 头起始位置。
/// `ip_hdr` 为上层 IPv4 头副本, 用于构造伪头校验。
pub fn udp_receive(mut payload: NetBuffer, ip_hdr: IPv4Header) {
    if payload.data_len() < UDP_HEADER_LEN {
        warn!(
            "UDP 包长度 {} 不足 {} 字节，丢弃",
            payload.data_len(),
            UDP_HEADER_LEN
        );
        return;
    }

    let udp_hdr = match payload.as_header::<UdpHeader>() {
        Some(hdr) => *hdr,
        None => {
            warn!("UDP 头解析失败，丢弃");
            return;
        }
    };

    let src_port = udp_hdr.source_port.host();
    let dst_port = udp_hdr.dest_port.host();
    let total_len = udp_hdr.total_len() as usize;

    // total_length 字段与实际数据长度交叉校验
    if total_len > payload.data_len() {
        warn!(
            "UDP length 字段 {} 大于实际数据 {}，丢弃",
            total_len,
            payload.data_len()
        );
        return;
    }

    // 校验和验证 (checksum == 0 表示发送端未计算, 允许通过)
    let src_ip = ip_hdr.source_addr.octets();
    let dst_ip = ip_hdr.dest_addr.octets();
    let udp_raw = &payload.data_slice()[..total_len];
    if !udp_verify_checksum(src_ip, dst_ip, total_len as u16, udp_raw) {
        warn!(
            "UDP 校验和错误 {:?} -> {:?}，丢弃",
            udp_hdr.source_port, udp_hdr.dest_port,
        );
        return;
    }

    info!("{:?}", udp_hdr);

    // 剥离 UDP 头, 保留纯数据区
    payload.pull(UDP_HEADER_LEN);

    debug!(
        "UDP payload: {} bytes, {:?} -> {:?}  content:{}",
        payload.data_len(),
        src_port,
        dst_port,
        String::from_utf8(payload.data_slice().to_vec()).expect("cant")
    );

    // TODO: 后续根据 dst_port 分发到具体的 socket / 应用层
    // 目前仅保留分发框架, 打印 payload 前 64 字节用于调试
    let preview_len = payload.data_len().min(64);
    debug!(
        "UDP payload preview ({}B): {:02x?}",
        preview_len,
        &payload.data_slice()[..preview_len]
    );

    // 发送给绑定该端口的 sock
    if let Some(sock_arc) = PORT_TABLE.lookup(NetPort(Net16::new(dst_port))) {
        if let Some(sock) = sock_arc.as_any().downcast_ref::<UdpSock>() {
            sock.push_rx_buf(payload);
        } else {
            warn!("port {} bound to non-UDP file", dst_port);
        }
    } else {
        warn!("find a no bind port packet!");
    }
}
