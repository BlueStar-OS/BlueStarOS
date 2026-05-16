//! ICMP 发包框架。
//!
//! 负责：
//! 1. 构造 ICMP 头
//! 2. 将 ICMP 头压入 payload 前部
//! 3. 对整个 ICMP 报文计算校验和
//! 4. 调用 IPv4 发包路径提交给网卡

use crate::driver::network::e1000::icmp::icmp_checksum;
use crate::driver::network::e1000::netbuffer::NetBuffer;
use crate::driver::network::e1000::packet::icmp::header::{IcmpHeader, IcmpKind};
use crate::driver::network::e1000::packet::ipv4::send_ipv4_packet;
use crate::driver::network::e1000::packet::protocol::Ipv4Protocol;
use log::info;

/// 将 ICMP 头压入数据区并对整个 ICMP 报文回填校验和。
fn finalize_icmp_and_send(target_ip: [u8; 4], mut payload: NetBuffer, icmp_hdr: IcmpHeader) {
    payload.new_agreement_head(&icmp_hdr);

    let checksum = icmp_checksum(payload.data_slice());
    let checksum_be = checksum.host().to_be_bytes();
    let header_bytes = payload.data_slice_mut();
    header_bytes[2..4].copy_from_slice(&checksum_be);

    let final_icmp = payload.data_slice();
    info!(
        "Send ICMP packet: dst={}.{}.{}.{} total_len={} bytes={:02x?}",
        target_ip[0],
        target_ip[1],
        target_ip[2],
        target_ip[3],
        final_icmp.len(),
        final_icmp
    );

    send_ipv4_packet(target_ip, Ipv4Protocol::ICMP, payload);
}

/// 发送一个通用 ICMP 报文。
///
/// `payload` 只包含 ICMP 头之后的“数据区”，不能带已有的 ICMP 头。
/// 本函数会在 `payload.data` 前面主动压入新的 `IcmpHeader`，随后再交给
/// IPv4 发包路径补三层/二层头。
///
/// 数据流:
/// `payload(data only)` -> `push IcmpHeader` -> `send_ipv4_packet()`
pub fn send_icmp_packet(
    target_ip: [u8; 4],
    kind: IcmpKind,
    rest_host: u32,
    payload: NetBuffer,
) {
    let icmp_hdr = IcmpHeader::new(kind, rest_host);
    finalize_icmp_and_send(target_ip, payload, icmp_hdr);
}

/// 发送 ICMP Echo Request。
///
/// `payload` 只包含 Echo 的附加数据区，不包含现成 ICMP 头。
/// `ident` 和 `sequence` 会被编码进 ICMP 固定头的 `rest` 4 字节。
pub fn send_icmp_echo_request(
    target_ip: [u8; 4],
    ident: u16,
    sequence: u16,
    payload: NetBuffer,
) {
    let icmp_hdr = IcmpHeader::new_echo(IcmpKind::EchoRequest, ident, sequence);
    finalize_icmp_and_send(target_ip, payload, icmp_hdr);
}

/// 发送 ICMP Echo Reply。
///
/// `payload` 只包含 Echo Reply 的附加数据区，不包含现成 ICMP 头。
/// 也就是说，如果从收到的 Echo Request 回包，调用方必须先剥掉原请求包
/// 的 `IcmpHeader`，把请求里的 data 部分原样传进来。
///
/// 对齐 Linux `icmp_echo()` 的处理语义：
/// `/home/inkbottle/桌面/linux-5.4.29/net/ipv4/icmp.c:917-941`
pub fn send_icmp_echo_reply(
    target_ip: [u8; 4],
    ident: u16,
    sequence: u16,
    payload: NetBuffer,
) {
    let icmp_hdr = IcmpHeader::new_echo(IcmpKind::EchoReply, ident, sequence);
    finalize_icmp_and_send(target_ip, payload, icmp_hdr);
}
