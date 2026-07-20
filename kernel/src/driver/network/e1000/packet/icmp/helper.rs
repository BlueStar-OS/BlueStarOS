//! ICMP helper 函数。
//!
//! 参考 Linux:
//! - `/home/inkbottle/桌面/linux-5.4.29/net/ipv4/icmp.c:996-1040`

use crate::driver::network::e1000::packet::net_endian::Net16;

/// 计算 ICMP 报文校验和。
///
/// 采用 RFC 1071 的反码求和，覆盖整个 ICMP 报文（头+数据）。
pub fn icmp_checksum(packet: &[u8]) -> Net16 {
    let mut sum: u32 = 0;
    let mut chunks = packet.chunks_exact(2);
    for chunk in &mut chunks {
        let word = u16::from_be_bytes([chunk[0], chunk[1]]);
        sum = sum.wrapping_add(word as u32);
    }
    if let Some(&last) = chunks.remainder().first() {
        sum = sum.wrapping_add(u16::from_be_bytes([last, 0]) as u32);
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    Net16::new(!(sum as u16))
}
