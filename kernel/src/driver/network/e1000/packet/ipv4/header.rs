//! IPv4 协议头与地址语义定义。
//!
//! 参考 Linux:
//! - `/home/inkbottle/桌面/linux-5.4.29/include/uapi/linux/ip.h:86-106`
//! - `/home/inkbottle/桌面/linux-5.4.29/net/ipv4/ip_output.c:91-95`

use core::fmt;

use crate::driver::network::e1000::netbuffer::NetHeader;
use crate::driver::network::e1000::packet::net_endian::{Net16, Net32, Net8};

/// 源 IP 地址 (4字节)
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SourceIp(Net32);

impl SourceIp {
    pub const ZERO: Self = SourceIp::new([0; 4]);

    pub const fn new(addr: [u8; 4]) -> Self {
        Self(Net32::new(u32::from_be_bytes(addr)))
    }

    pub fn octets(self) -> [u8; 4] {
        self.0.host().to_be_bytes()
    }
}

impl fmt::Debug for SourceIp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let octets = self.octets();
        write!(f, "{}.{}.{}.{}", octets[0], octets[1], octets[2], octets[3])
    }
}

/// 目的 IP 地址 (4字节)
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DestIp(Net32);

impl DestIp {
    pub const ZERO: Self = DestIp::new([0; 4]);

    pub const fn new(addr: [u8; 4]) -> Self {
        Self(Net32::new(u32::from_be_bytes(addr)))
    }

    pub fn octets(self) -> [u8; 4] {
        self.0.host().to_be_bytes()
    }
}

impl fmt::Debug for DestIp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let octets = self.octets();
        write!(f, "{}.{}.{}.{}", octets[0], octets[1], octets[2], octets[3])
    }
}

/// IPv4 地址 (4字节, 网络字节序)
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ipv4Addr(Net32);

impl Ipv4Addr {
    pub const ZERO: Self = Ipv4Addr::new([0; 4]);

    pub const fn new(addr: [u8; 4]) -> Self {
        Self(Net32::new(u32::from_be_bytes(addr)))
    }

    pub fn from_u32(net_order: u32) -> Self {
        Self(Net32::new(u32::from_be(net_order)))
    }

    pub fn to_u32(self) -> u32 {
        self.0.host().to_be()
    }

    pub fn octets(self) -> [u8; 4] {
        self.0.host().to_be_bytes()
    }
}

impl PartialOrd for Ipv4Addr {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Ipv4Addr {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.octets().cmp(&other.octets())
    }
}

impl fmt::Display for Ipv4Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let octets = self.octets();
        write!(f, "{}.{}.{}.{}", octets[0], octets[1], octets[2], octets[3])
    }
}

impl fmt::Debug for Ipv4Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

pub const IPV4_VERSION: u8 = 4;
pub const IPV4_MIN_IHL_WORDS: u8 = 5;
pub const IPV4_HEADER_LEN: u16 = 20;
pub const IPV4_DEFAULT_TTL: u8 = 64;
pub const IPV4_FRAG_OFF_NONE: Net16 = Net16::ZERO;

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Ipv4VersionIhl(Net8);

impl Ipv4VersionIhl {
    pub const NO_OPTIONS: Self = Self::new(IPV4_VERSION, IPV4_MIN_IHL_WORDS);

    pub const fn new(version: u8, ihl_words: u8) -> Self {
        Self(Net8::new((version << 4) | (ihl_words & 0x0F)))
    }

    pub const fn version(self) -> u8 {
        self.0.host() >> 4
    }

    pub const fn ihl_words(self) -> u8 {
        self.0.host() & 0x0F
    }

    pub const fn ihl_bytes(self) -> u8 {
        self.ihl_words() * 4
    }
}

impl fmt::Debug for Ipv4VersionIhl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "version={} ihl={}B", self.version(), self.ihl_bytes())
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Ipv4DscpEcn(Net8);

impl Ipv4DscpEcn {
    pub const ZERO: Self = Self(Net8::ZERO);
}

impl fmt::Debug for Ipv4DscpEcn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#04x}", self.0.host())
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Ipv4Ttl(Net8);

impl Ipv4Ttl {
    pub const DEFAULT: Self = Self(Net8::new(IPV4_DEFAULT_TTL));
}

impl fmt::Debug for Ipv4Ttl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.host())
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Ipv4Protocol(Net8);

impl Ipv4Protocol {
    pub const ICMP: Self = Self(Net8::new(1));
    pub const TCP: Self = Self(Net8::new(6));
    pub const UDP: Self = Self(Net8::new(17));

    pub fn host(self) -> u8 {
        self.0.host()
    }
}

impl fmt::Debug for Ipv4Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match *self {
            Self::ICMP => "ICMP",
            Self::TCP => "TCP",
            Self::UDP => "UDP",
            _ => "Unknown",
        };
        write!(f, "{}({})", name, self.host())
    }
}

/// IPv4 报文头 (20 字节, 不含选项)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct IPv4Header {
    pub version_ihl: Ipv4VersionIhl,
    pub tos: Ipv4DscpEcn,
    pub total_length: Net16,
    pub id: Net16,
    pub flags_frag_offset: Net16,
    pub ttl: Ipv4Ttl,
    pub protocol: Ipv4Protocol,
    pub checksum: Net16,
    pub source_addr: SourceIp,
    pub dest_addr: DestIp,
}

const _: () = assert!(core::mem::size_of::<IPv4Header>() == 20);

// SAFETY: `IPv4Header` 的线速布局固定且与 IPv4 基本头一致。
unsafe impl NetHeader for IPv4Header {
    const WIRE_LEN: usize = IPV4_HEADER_LEN as usize;
}

impl IPv4Header {
    pub fn new(
        protocol: Ipv4Protocol,
        payload_len: u16,
        source_addr: SourceIp,
        dest_addr: DestIp,
    ) -> Self {
        Self {
            version_ihl: Ipv4VersionIhl::NO_OPTIONS,
            tos: Ipv4DscpEcn::ZERO,
            total_length: Net16::new(IPV4_HEADER_LEN + payload_len),
            id: Net16::ZERO,
            flags_frag_offset: IPV4_FRAG_OFF_NONE,
            ttl: Ipv4Ttl::DEFAULT,
            protocol,
            checksum: Net16::ZERO,
            source_addr,
            dest_addr,
        }
    }

    pub fn to_be_bytes(&self) -> &[u8] {
        // SAFETY: `IPv4Header` 为 packed 且固定 20 字节。
        unsafe { core::slice::from_raw_parts(self as *const Self as *const u8, 20) }
    }

    pub fn checksum_rfc_for_self(&mut self) {
        self.checksum = Net16::ZERO;
        let mut sum: u32 = 0;
        let base = self as *const Self as *const u16;
        for i in 0..10 {
            sum = sum.wrapping_add(unsafe { u16::from_be(base.add(i).read_unaligned()) } as u32);
        }
        while sum >> 16 != 0 {
            sum = (sum & 0xFFFF) + (sum >> 16);
        }
        self.checksum = Net16::new(!(sum as u16));
    }
    pub fn checksum_cloc(&self) -> Net16{
        let mut sum: u32 = 0;
        let base = self as *const Self as *const u16;
        for i in 0..10 {
            sum = sum.wrapping_add(unsafe { base.add(i).read_unaligned() } as u32);
        }
        while sum >> 16 != 0 {
            sum = (sum & 0xFFFF) + (sum >> 16);
        }
        Net16::new(!(sum as u16))
    }
}

impl fmt::Debug for IPv4Header {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let self_b = self as *const Self as *const u8;
        let total_len = unsafe { (self_b.add(2) as *const Net16).read_unaligned() };
        let src = unsafe { (self_b.add(12) as *const SourceIp).read_unaligned() };
        let dst = unsafe { (self_b.add(16) as *const DestIp).read_unaligned() };
        let proto = unsafe { (self_b.add(9) as *const Ipv4Protocol).read_unaligned() };
        let proto_str = match proto {
            Ipv4Protocol::TCP => "TCP",
            Ipv4Protocol::UDP => "UDP",
            p => {
                return write!(
                    f,
                    "IPv4: {:?} -> {:?}  proto={} len={}",
                    src,
                    dst,
                    p.host(),
                    total_len.host()
                )
            }
        };
        write!(
            f,
            "IPv4: {:?} -> {:?}  proto={} len={}",
            src,
            dst,
            proto_str,
            total_len.host()
        )
    }
}
