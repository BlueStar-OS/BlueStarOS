//! UDP 发包路径。
//!
//! 负责:
//! 1. 构造 UDP 头
//! 2. 将 UDP 头压入 payload 前部
//! 3. 计算 UDP 校验和 (含 IPv4 伪头)
//! 4. 调用 IPv4 发包路径提交给网卡
//!
//! 参考 Linux:
//! - `/home/inkbottle/桌面/linux-5.4.29/net/ipv4/udp.c:830-887`

use log::debug;

use crate::driver::network::e1000::netbuffer::NetBuffer;
use crate::driver::network::e1000::packet::ipv4::config::MY_IPV4;
use crate::driver::network::e1000::packet::ipv4::send_ipv4_packet;
use crate::driver::network::e1000::packet::protocol::Ipv4Protocol;

use super::header::{DestPort, SourcePort, UdpHeader};
use super::helper::udp_checksum;

/// 发送 UDP 报文。
///
/// `payload` 只包含 UDP 数据区 (不含 UDP 头)。
/// 本函数会:
///   1. 在 `payload` 前压入 `UdpHeader`
///   2. 用伪头 + UDP 头 + 数据计算校验和并回填
///   3. 调用 `send_ipv4_packet` 补三层/二层头后发出去
///
/// 数据流:
/// `payload(data only)` -> `push UdpHeader` -> `fill checksum` -> `send_ipv4_packet()`
pub fn send_udp_packet(target_ip: [u8; 4], src_port: u16, dst_port: u16, payload: NetBuffer) {
    let payload_len = payload.data_len() as u16;
    let udp_total_len = 8 + payload_len;

    let udp_hdr = UdpHeader::new(
        SourcePort::new(src_port),
        DestPort::new(dst_port),
        payload_len,
    );

    finalize_udp_and_send(target_ip, udp_total_len, payload, udp_hdr);
}

/// 将 UDP 头压入数据区, 计算校验和并发送。
fn finalize_udp_and_send(
    target_ip: [u8; 4],
    udp_total_len: u16,
    mut payload: NetBuffer,
    udp_hdr: UdpHeader,
) {
    // 压入 UDP 头到 payload 前部
    payload.new_agreement_head(&udp_hdr);

    // 计算 UDP 校验和 (含 IPv4 伪头)
    let src_ip = MY_IPV4.octets();
    let checksum = udp_checksum(src_ip, target_ip, udp_total_len, payload.data_slice());

    // 回填校验和到 UDP 头的 checksum 字段 (偏移 6..8)
    let checksum_be = checksum.host().to_be_bytes();
    let header_bytes = payload.data_slice_mut();
    header_bytes[6..8].copy_from_slice(&checksum_be);

    debug!(
        "Send UDP: {:?} -> {:?} total_len={} checksum={:#06x}",
        udp_hdr.source_port,
        udp_hdr.dest_port,
        udp_total_len,
        checksum.host(),
    );

    // 交给 IPv4 层补三层/二层头
    send_ipv4_packet(target_ip, Ipv4Protocol::UDP, payload);
}
