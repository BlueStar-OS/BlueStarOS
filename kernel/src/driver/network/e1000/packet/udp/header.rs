//! UDP 协议头与端口语义定义。
//!
//! 参考 Linux:
//! - `/home/inkbottle/桌面/linux-5.4.29/include/uapi/linux/udp.h:23-28`

use core::fmt;

use crate::driver::network::e1000::agreenment::Net8;
use crate::driver::network::e1000::ipv4::{Ipv4Addr, Ipv4Protocol};
use crate::driver::network::e1000::netbuffer::NetHeader;
use crate::driver::network::e1000::packet::net_endian::Net16;

/// UDP 固定首部长度，单位字节。
pub const UDP_HEADER_LEN: usize = 8;

/// UDP 源端口 (网络字节序)
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SourcePort(Net16);

impl SourcePort {
    pub const ZERO: Self = Self(Net16::ZERO);

    pub const fn new(host: u16) -> Self {
        Self(Net16::new(host))
    }

    pub fn host(self) -> u16 {
        self.0.host()
    }
}

impl fmt::Debug for SourcePort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.host())
    }
}

/// UDP 目的端口 (网络字节序)
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DestPort(Net16);

impl DestPort {
    pub const ZERO: Self = Self(Net16::ZERO);

    pub const fn new(host: u16) -> Self {
        Self(Net16::new(host))
    }

    pub fn host(self) -> u16 {
        self.0.host()
    }
}

impl fmt::Debug for DestPort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.host())
    }
}

/// UDP 报文头 (8 字节)
///
/// 参考 Linux:
/// `/home/inkbottle/桌面/linux-5.4.29/include/uapi/linux/udp.h:23-28`
#[repr(C)]
#[derive(Clone, Copy)]
pub struct UdpHeader {
    pub source_port: SourcePort,
    pub dest_port: DestPort,
    pub length: Net16,
    pub checksum: Net16,
}

const _: () = assert!(core::mem::size_of::<UdpHeader>() == UDP_HEADER_LEN);

// SAFETY: `UdpHeader` 的线速布局固定且与 UDP 头一致。
unsafe impl NetHeader for UdpHeader {
    const WIRE_LEN: usize = UDP_HEADER_LEN;
}

impl UdpHeader {
    /// 构造 UDP 头。
    ///
    /// `payload_len` 为 UDP 数据区长度 (不含 UDP 头本身)。
    /// `length` 字段 = UDP_HEADER_LEN + payload_len。
    /// `checksum` 初始化为 0，后续由 helper 回填。
    pub fn new(source_port: SourcePort, dest_port: DestPort, payload_len: u16) -> Self {
        Self {
            source_port,
            dest_port,
            length: Net16::new(UDP_HEADER_LEN as u16 + payload_len),
            checksum: Net16::ZERO,
        }
    }

    /// 以大端序返回 UDP 头部字节切片。
    // clippy::wrong_self_convention: 返回的切片借自 `self` 自身内存，必须按 `&self`
    // 借用；若按值接收 `self`，返回的切片会指向已销毁的临时量，造成悬垂引用。
    #[allow(clippy::wrong_self_convention)]
    pub fn to_be_bytes(&self) -> &[u8] {
        // SAFETY: `UdpHeader` 为 packed 且固定 8 字节。
        unsafe { core::slice::from_raw_parts(self as *const Self as *const u8, UDP_HEADER_LEN) }
    }

    /// UDP 总长度 (头 + 数据)，主机序。
    pub fn total_len(self) -> u16 {
        self.length.host()
    }
}

impl fmt::Debug for UdpHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let self_b = self as *const Self as *const u8;
        let src = unsafe { (self_b as *const SourcePort).read_unaligned() };
        let dst = unsafe { (self_b.add(2) as *const DestPort).read_unaligned() };
        let len = unsafe { (self_b.add(4) as *const Net16).read_unaligned() };
        write!(f, "UDP: {:?} -> {:?}  len={}", src, dst, len.host())
    }
}

/// UDP 伪头 (12 字节) — 不发送, 仅用于校验和计算。
///
/// 布局:
/// ```text
/// +--------+--------+--------+--------+
/// |         Source IP (4B)            |
/// +--------+--------+--------+--------+
/// |         Dest   IP (4B)            |
/// +--------+--------+--------+--------+
/// | zero   | proto  |   UDP Length    |
/// +--------+--------+--------+--------+
/// ```
///
/// 参考 Linux:
/// `/home/inkbottle/桌面/linux-5.4.29/arch/x86/include/asm/checksum_64.h:87-99`
#[repr(C)]
#[derive(Clone, Copy)]
pub struct UdpPrseHeader {
    pub sourceip: [u8; 4],
    pub dstip: [u8; 4],
    pub zero: u8,
    pub proto: Ipv4Protocol,
    pub udp_len: Net16,
}

const _: () = assert!(core::mem::size_of::<UdpPrseHeader>() == 12);

impl UdpPrseHeader {
    /// 构造 UDP 伪头。
    ///
    /// - `src_ip` / `dst_ip`: 从 IPv4 头中取出的源/目的地址
    /// - `udp_total_len`: UDP 总长度 (头 + 数据), 主机序
    pub fn new(src_ip: [u8; 4], dst_ip: [u8; 4], udp_total_len: u16) -> Self {
        Self {
            sourceip: src_ip,
            dstip: dst_ip,
            zero: 0,
            proto: Ipv4Protocol::UDP,
            udp_len: Net16::new(udp_total_len),
        }
    }

    /// 以大端序返回伪头字节切片, 可直接喂给校验和函数。
    pub fn as_bytes(&self) -> &[u8] {
        // SAFETY: `UdpPrseHeader` 为 repr(C) 且固定 12 字节, 无填充。
        unsafe {
            core::slice::from_raw_parts(
                self as *const Self as *const u8,
                core::mem::size_of::<Self>(),
            )
        }
    }
}

impl fmt::Debug for UdpPrseHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PseudoHeader {{ src={:?} dst={:?} proto={:?} udp_len={} }}",
            self.sourceip,
            self.dstip,
            self.proto,
            self.udp_len.host()
        )
    }
}
