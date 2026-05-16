//! ICMP 收包入口。
//!
//! 参考 Linux:
//! - `/home/inkbottle/桌面/linux-5.4.29/net/ipv4/icmp.c:996-1040`
//! - `/home/inkbottle/桌面/linux-5.4.29/net/ipv4/icmp.c:1115-1148`

use log::{debug, info, warn};

use crate::driver::network::e1000::agreenment::EthHead;
use crate::driver::network::e1000::ipv4::IPv4Header;
use crate::driver::network::e1000::netbuffer::NetBuffer;
use crate::driver::network::e1000::packet::icmp::header::{IcmpHeader, IcmpKind, ICMP_HEADER_LEN};
use crate::driver::network::e1000::packet::icmp::helper::icmp_checksum;
use crate::driver::network::e1000::packet::icmp::send_icmp_echo_reply;
use crate::driver::network::e1000::packet::ipv4::config::MY_IPV4;

/// 处理一个已经剥离 IPv4 头的 ICMP 报文。
pub fn icmp_receive(mut payload: NetBuffer,ethheader:EthHead,iphead:IPv4Header) {
    if payload.data_len() < ICMP_HEADER_LEN {
        warn!("ICMP 包长度不足 {} 字节，直接丢弃", ICMP_HEADER_LEN);
        return;
    }

    if icmp_checksum(payload.data_slice()).host() != 0 {
        warn!("ICMP 校验和错误，直接丢弃");
        return;
    }

    // SAFETY: 已检查至少包含 8 字节固定 ICMP 头，且 `IcmpHeader` 为 packed。
    let icmp_hdr = unsafe { &*(payload.data_slice().as_ptr() as *const IcmpHeader) };
    let icmp_kind = icmp_hdr.kind();

    info!("{:?} payload={}B", icmp_hdr, payload.data_len());

    match icmp_kind {
        IcmpKind::EchoRequest => {
            debug!(
                "ICMP Echo Request ident={} seq={}",
                icmp_hdr.echo_ident(),
                icmp_hdr.echo_sequence()
            );

            // 剥离 ICMP 头，只保留 data 区，交给 tx 层构造 Reply 头并回填校验和。
            payload.pull(ICMP_HEADER_LEN);
            send_icmp_echo_reply(
                iphead.source_addr.octets(),
                icmp_hdr.echo_ident(),
                icmp_hdr.echo_sequence(),
                payload,
            );
        }
        IcmpKind::EchoReply => {
            debug!(
                "ICMP Echo Reply ident={} seq={}",
                icmp_hdr.echo_ident(),
                icmp_hdr.echo_sequence()
            );
        }
        IcmpKind::DestUnreach(code) => {
            debug!("ICMP Destination Unreachable {:?}，当前仅保留分发框架", code);
        }
        IcmpKind::TimeExceeded(code) => {
            debug!("ICMP Time Exceeded {:?}，当前仅保留分发框架", code);
        }
        IcmpKind::ParameterProblem(code) => {
            debug!("ICMP Parameter Problem {:?}，当前仅保留分发框架", code);
        }
        IcmpKind::Other { icmp_type, code } => {
            warn!("未处理的 ICMP type={} code={}", icmp_type, code);
        }
    }
}
