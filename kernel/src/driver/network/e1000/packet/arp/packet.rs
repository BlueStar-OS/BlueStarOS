//! ARP 线速报文头定义。
//!
//! 参考 Linux:
//! - `/home/inkbottle/桌面/linux-5.4.29/include/uapi/linux/if_arp.h:145-154`

use alloc::fmt::Write;

use crate::driver::network::e1000::agreenment::{Ipv4Addr, Net16, Net8};
use crate::driver::network::e1000::netbuffer::NetHeader;

/// ARP Request opcode。
pub const ARP_OPCODE_REQUEST: u16 = 1;
/// ARP Reply opcode。
pub const ARP_OPCODE_REPLY: u16 = 2;

/// ARP 报文头部，固定 28 字节。
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct ArpHeader {
    /// 硬件类型，Ethernet = 1。
    pub hw_type: Net16,
    /// 协议类型，IPv4 = 0x0800。
    pub proto_type: Net16,
    /// 硬件地址长度，MAC = 6。
    pub hw_len: Net8,
    /// 协议地址长度，IPv4 = 4。
    pub proto_len: Net8,
    /// 操作码，请求 = 1，回复 = 2。
    pub opcode: Net16,
    /// 发送端 MAC 地址。
    pub sender_mac: [u8; 6],
    /// 发送端 IPv4 地址。
    pub sender_ip: Ipv4Addr,
    /// 目标 MAC 地址。
    pub target_mac: [u8; 6],
    /// 目标 IPv4 地址。
    pub target_ip: Ipv4Addr,
}

const _: () = assert!(core::mem::size_of::<ArpHeader>() == 28);

// SAFETY: `ArpHeader` 的布局固定且与线速 ARP 头一致。
unsafe impl NetHeader for ArpHeader {
    const WIRE_LEN: usize = 28;
}

impl ArpHeader {
    /// 构造一个 ARP 报文头。
    // clippy::too_many_arguments: 这些入参一一对应 ARP 报文头的各个协议字段，
    // 是硬件/协议定义的固有形状，聚合成结构体只会引入一层无意义的中间类型。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        hw_type: u16,
        proto_type: u16,
        hw_len: u8,
        proto_len: u8,
        opcode: u16,
        sender_mac: [u8; 6],
        sender_ip: Ipv4Addr,
        target_mac: [u8; 6],
        target_ip: Ipv4Addr,
    ) -> Self {
        Self {
            hw_type: Net16::new(hw_type),
            proto_type: Net16::new(proto_type),
            hw_len: Net8::new(hw_len),
            proto_len: Net8::new(proto_len),
            opcode: Net16::new(opcode),
            sender_mac,
            sender_ip,
            target_mac,
            target_ip,
        }
    }

    /// 返回网络字节序视图。
    #[allow(clippy::wrong_self_convention)]
    pub fn to_be_bytes(&self) -> &[u8] {
        // SAFETY: `ArpHeader` 为 packed 且线速布局固定 28 字节。
        unsafe { core::slice::from_raw_parts(self as *const Self as *const u8, 28) }
    }
}

impl core::fmt::Display for ArpHeader {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let self_b = self as *const Self as *const u8;
        // SAFETY: ArpHeader 是 #[repr(C, packed)]，按线速布局用未对齐读提取字段。
        let hw_type = unsafe { (self_b as *const Net16).read_unaligned() }.host();
        let proto_type = unsafe { (self_b.add(2) as *const Net16).read_unaligned() }.host();
        let opcode = unsafe { (self_b.add(6) as *const Net16).read_unaligned() }.host();
        let sender_mac = unsafe { (self_b.add(8) as *const [u8; 6]).read_unaligned() };
        let sender_ip = unsafe { (self_b.add(14) as *const Ipv4Addr).read_unaligned() };
        let target_mac = unsafe { (self_b.add(18) as *const [u8; 6]).read_unaligned() };
        let target_ip = unsafe { (self_b.add(24) as *const Ipv4Addr).read_unaligned() };
        let op_str = match opcode {
            ARP_OPCODE_REQUEST => "Request",
            ARP_OPCODE_REPLY => "Reply",
            _ => "Unknown",
        };
        write!(
            f,
            "ARP: hw={:#06x}{} proto={:#06x}{} op={} ({})\n  \
             sender: {} @ {}\n  target: {} @ {}",
            hw_type,
            if hw_type == 1 { " (Ethernet)" } else { "" },
            proto_type,
            if proto_type == 0x0800 { " (IPv4)" } else { "" },
            opcode,
            op_str,
            fmt_mac(&sender_mac),
            sender_ip,
            fmt_mac(&target_mac),
            target_ip,
        )
    }
}

/// 将 MAC 地址格式化为 `xx:xx:xx:xx:xx:xx`。
fn fmt_mac(mac: &[u8; 6]) -> alloc::string::String {
    let mut s = alloc::string::String::new();
    for (i, byte) in mac.iter().enumerate() {
        if i > 0 {
            let _ = s.write_char(':');
        }
        let _ = write!(s, "{:02x}", byte);
    }
    s
}
