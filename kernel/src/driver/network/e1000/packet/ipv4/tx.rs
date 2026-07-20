//! IPv4 发包路径。
//!
//! 负责：
//! 1. 查询 ARP 缓存获取目的 MAC
//! 2. 构造 IPv4 首部
//! 3. 压入二层/三层头后提交给 TX ring

use log::{info, warn};

use crate::driver::network::e1000::{
        agreenment::{
            DestIp, DstMac, EthHead, EtherType, IPv4Header, Ipv4Addr, Ipv4Protocol, SourceIp,
            SourceMac,
        },
        arp::ARP_TABLE,
        netbuffer::NetBuffer,
        tx_ringbuffer::{e1000_transmit, E1000TxRing},
        E1000_DEV,
    };

use super::config::MY_IPV4;

/// ARP 解析重试次数上限。
const ARP_RETRY_LIMIT: usize = 3;

/// 尝试从 ARP 缓存获取目标 MAC。
///
/// 若缓存未命中，发送 ARP 广播请求并轮询等待回复。
/// 返回 `Some(mac)` 表示解析成功，`None` 表示超时。
fn resolve_target_mac(target_ip: Ipv4Addr) -> Option<DstMac> {
    if let Some(mac) = ARP_TABLE.lock(|t| t.lookup(&target_ip).copied()) {
        return Some(mac);
    }

    info!("ARP cache miss for {:?}, sending ARP request", target_ip);
    crate::driver::network::e1000::arp::send_arp_packet(target_ip);

    for _ in 0..ARP_RETRY_LIMIT {
        if let Some(mac) = ARP_TABLE.lock(|t| t.lookup(&target_ip).copied()) {
            info!("ARP resolved {:?} -> {:?}", target_ip, mac);
            return Some(mac);
        }
    }

    warn!(
        "ARP resolution failed for {:?} after {} attempts",
        target_ip, ARP_RETRY_LIMIT
    );
    None
}

/// 发送 IPv4 报文。
///
/// 参考 Linux IPv4 头布局:
/// `/home/inkbottle/桌面/linux-5.4.29/include/uapi/linux/ip.h:86-106`
pub fn send_ipv4_packet(target_ip: [u8; 4], protocol: Ipv4Protocol, mut payload: NetBuffer) {
    let target_ip = Ipv4Addr::new(target_ip);

    let target_mac = match resolve_target_mac(target_ip) {
        Some(mac) => mac,
        None => {
            warn!(
                "send_ipv4_packet: dropping packet to {:?} — ARP resolution failed",
                target_ip
            );
            return;
        }
    };

    let local_mac = E1000_DEV.lock(|dev| dev.as_ref().expect("no e1000 device").mac);

    let ip_payload_len = payload.data_len() as u16;
    let mut ip_hdr = IPv4Header::new(
        protocol,
        ip_payload_len,
        SourceIp::new(MY_IPV4.octets()),
        DestIp::new(target_ip.octets()),
    );
    ip_hdr.checksum_rfc_for_self();

    payload.new_agreement_head(&ip_hdr);

    let eth_hdr = EthHead::new(target_mac, SourceMac(local_mac), EtherType::IPV4);
    payload.new_agreement_head(&eth_hdr);

    E1000_DEV.lock(|dev| {
        let dev = dev.as_mut().expect("no e1000 device");
        // SAFETY: `dev.tx_ring` 在设备全局锁保护下独占访问
        let tx_ring = unsafe { &mut *((&mut dev.tx_ring) as *mut E1000TxRing) };
        e1000_transmit(&dev.bar, tx_ring, payload);
    });
}
