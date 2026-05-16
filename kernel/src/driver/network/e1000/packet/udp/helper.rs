//! UDP 校验和计算 (含 IPv4 伪头)。
//!
//! UDP 校验和需要覆盖伪头 + UDP 头 + 数据区，以保证端到端完整性。
//!
//! 伪头布局 (12 字节, 不发送, 只参与校验和计算):
//! ```text
//! +--------+--------+--------+--------+
//! |         Source IP (4B)            |
//! +--------+--------+--------+--------+
//! |         Dest   IP (4B)            |
//! +--------+--------+--------+--------+
//! | zero   | proto  |   UDP Length    |
//! +--------+--------+--------+--------+
//! ```
//!
//! 参考 Linux:
//! - `/home/inkbottle/桌面/linux-5.4.29/net/ipv4/udp.c:882-886`
//! - `/home/inkbottle/桌面/linux-5.4.29/arch/x86/include/asm/checksum_64.h:87-99`

use crate::driver::network::e1000::{agreenment::{UdpHeader, UdpPrseHeader}, packet::net_endian::Net16};

/// IPv4 协议号: UDP
const IPPROTO_UDP: u8 = 17;

/// 计算 UDP 校验和 (含 IPv4 伪头)。
///
/// `src_ip` / `dst_ip` 为网络序 4 字节地址。
/// `udp_total_len` 为 UDP 总长度 (头+数据), 主机序。
/// `udp_packet` 为完整的 UDP 报文 (头+数据), 校验和字段应先置 0。
///
/// 返回值为网络序校验和, 可直接填入 `UdpHeader::checksum`。
pub fn udp_checksum(src_ip: [u8; 4], dst_ip: [u8; 4], udp_total_len: u16, udp_packet: &[u8]) -> Net16 {
    let prseheader  = UdpPrseHeader::new(src_ip, dst_ip, udp_total_len);
    let mut checksum:u32 = 0;

    for i in prseheader.as_bytes().chunks(2){
            checksum+=u16::from_be_bytes([i[0],i[1]])as u32;            
    }

    for i in udp_packet.chunks(2) {
        if i.len() == 2 {
            checksum+=u16::from_be_bytes([i[0],i[1]])as u32;            
        }else {
            checksum+=u16::from_be_bytes([i[0],0])as u32;            
        }
    }
    
    while checksum>>16 >0 {
        checksum= (checksum & 0xffff) + (checksum>>16);
    }
    Net16::new(!checksum as u16)

}

/// 验证 UDP 校验和。
///
/// 对收到的 UDP 包做校验: 伪头 + 完整 UDP 报文 (含 checksum 字段) 之和应为 0。
/// 返回 `true` 表示校验通过 (或 checksum 字段为 0 表示发送端未计算)。
pub fn udp_verify_checksum(
    src_ip: [u8; 4],
    dst_ip: [u8; 4],
    udp_total_len: u16,
    udp_packet: &[u8],
) -> bool {
    // checksum == 0 表示发送端未计算校验和 (IPv4 允许)
    let checksum_raw = u16::from_be_bytes([udp_packet[6], udp_packet[7]]);
    if checksum_raw == 0 {
        return true;
    }

    let mut sum: u32 = 0;

    // 伪头
    let src_w1 = u16::from_be_bytes([src_ip[0], src_ip[1]]);
    let src_w2 = u16::from_be_bytes([src_ip[2], src_ip[3]]);
    sum = sum.wrapping_add(src_w1 as u32);
    sum = sum.wrapping_add(src_w2 as u32);

    let dst_w1 = u16::from_be_bytes([dst_ip[0], dst_ip[1]]);
    let dst_w2 = u16::from_be_bytes([dst_ip[2], dst_ip[3]]);
    sum = sum.wrapping_add(dst_w1 as u32);
    sum = sum.wrapping_add(dst_w2 as u32);

    sum = sum.wrapping_add(((0u16 << 8) | IPPROTO_UDP as u16) as u32);
    sum = sum.wrapping_add(udp_total_len as u32);

    // UDP 报文 (含 checksum 字段)
    let mut chunks = udp_packet.chunks_exact(2);
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

    // 反码求和后, 全部 1-bit 求和应为 0xFFFF, 取反后为 0
    sum as u16 == 0xFFFF
}
