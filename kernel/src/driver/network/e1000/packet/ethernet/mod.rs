//! Ethernet 协议定义。
//!
//! 参考 Linux:
//! - `/home/inkbottle/桌面/linux-5.4.29/include/uapi/linux/if_ether.h:32-41`
//! - `/home/inkbottle/桌面/linux-5.4.29/include/uapi/linux/if_ether.h:163-167`

use core::fmt;

use crate::driver::network::e1000::netbuffer::NetHeader;
use crate::driver::network::e1000::packet::net_endian::Net16;

/// MAC地址长度 (6字节): ETH_ALEN
pub const ETH_ALEN: usize = 6;
/// 以太网类型字段长度 (2字节): ETH_TLEN
pub const ETH_TLEN: usize = 2;
/// 以太网帧头总长度 (14 = 6 + 6 + 2): ETH_HLEN
pub const ETH_HLEN: usize = 14;
/// 以太网最小帧 payload (60字节): ETH_ZLEN
pub const ETH_ZLEN: usize = 60;
/// 以太网最大 payload (1500字节): ETH_DATA_LEN
pub const ETH_DATA_LEN: usize = 1500;
/// 以太网帧总长度不含FCS (1514 = 14 + 1500): ETH_FRAME_LEN
pub const ETH_FRAME_LEN: usize = 1514;
/// FCS长度 (4字节): ETH_FCS_LEN
pub const ETH_FCS_LEN: usize = 4;

/// 源MAC地址 (6字节)
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SourceMac(pub [u8; ETH_ALEN]);

/// 目的MAC地址 (6字节)
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DstMac(pub [u8; ETH_ALEN]);

/// 以太网协议类型 (EtherType, 网络字节序大端)
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct EtherType(Net16);

#[allow(dead_code)]
impl EtherType {
    pub const IPV4: Self = EtherType::new(0x0800);
    pub const ARP: Self = EtherType::new(0x0806);
    pub const RARP: Self = EtherType::new(0x8035);
    pub const VLAN: Self = EtherType::new(0x8100);
    pub const IPV6: Self = EtherType::new(0x86DD);
    pub const ALL: Self = EtherType::new(0x0003);
    pub const LOOPBACK: Self = EtherType::new(0x9000);

    pub const fn new(host: u16) -> Self {
        Self(Net16::new(host))
    }

    pub fn to_host(self) -> u16 {
        self.0.host()
    }
}

impl fmt::Debug for EtherType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let host = self.to_host();
        let name = match *self {
            EtherType::IPV4 => "IPv4",
            EtherType::ARP => "ARP",
            EtherType::IPV6 => "IPv6",
            EtherType::VLAN => "VLAN",
            _ => "Unknown",
        };
        write!(f, "EtherType({:#06x}, {})", host, name)
    }
}

/// 以太网帧头 (14字节, packed 无填充)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct EthHead {
    pub dest_mac: DstMac,
    pub source_mac: SourceMac,
    pub ether_type: EtherType,
}

const _: () = assert!(core::mem::size_of::<EthHead>() == 14);

// SAFETY: `EthHead` 的线速布局固定且与 Ethernet 头一致。
unsafe impl NetHeader for EthHead {
    const WIRE_LEN: usize = ETH_HLEN;
}

impl EthHead {
    pub fn new(dest_mac: DstMac, source_mac: SourceMac, ether_type: EtherType) -> Self {
        Self {
            dest_mac,
            source_mac,
            ether_type,
        }
    }

    /// 参考 Linux: `net/ethernet/eth.c:155-165`
    pub fn from_bytes(bytes: &[u8]) -> Option<&Self> {
        if bytes.len() < ETH_HLEN {
            return None;
        }
        // SAFETY: `EthHead` 为 packed，且已确认字节长度足够。
        Some(unsafe { &*(bytes.as_ptr() as *const EthHead) })
    }

    pub fn from_bytes_mut(bytes: &mut [u8]) -> Option<&mut Self> {
        if bytes.len() < ETH_HLEN {
            return None;
        }
        // SAFETY: `EthHead` 为 packed，且已确认字节长度足够。
        Some(unsafe { &mut *(bytes.as_mut_ptr() as *mut EthHead) })
    }
}

impl fmt::Debug for EthHead {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let self_bytes = self as *const Self as *const u8;
        let dest = unsafe { (self_bytes as *const DstMac).read_unaligned() };
        let src = unsafe { (self_bytes.add(6) as *const SourceMac).read_unaligned() };
        let eth_type = unsafe { (self_bytes.add(12) as *const EtherType).read_unaligned() };
        f.debug_struct("EthHead")
            .field("dest", &dest)
            .field("src", &src)
            .field("proto", &eth_type)
            .finish()
    }
}

impl fmt::Display for EthHead {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let self_b = self as *const Self as *const u8;
        let dest = unsafe { (self_b as *const DstMac).read_unaligned() };
        let src = unsafe { (self_b.add(6) as *const SourceMac).read_unaligned() };
        let etype = unsafe { (self_b.add(12) as *const EtherType).read_unaligned() };
        let host = etype.to_host();
        let proto = match host {
            0x0800 => "IPv4",
            0x0806 => "ARP",
            0x86DD => "IPv6",
            _ => "Unknown",
        };
        write!(
            f,
            "Ethernet: {:?} -> {:?}  type={:#06x} ({})",
            src, dest, host, proto
        )
    }
}

impl SourceMac {
    pub const BROADCAST: Self = SourceMac([0xFF; ETH_ALEN]);
    pub const ZERO: Self = SourceMac([0x00; ETH_ALEN]);

    pub fn new(addr: [u8; ETH_ALEN]) -> Self {
        SourceMac(addr)
    }
}

impl DstMac {
    pub const BROADCAST: Self = DstMac([0xFF; ETH_ALEN]);
    pub const ZERO: Self = DstMac([0x00; ETH_ALEN]);

    pub fn new(addr: [u8; ETH_ALEN]) -> Self {
        DstMac(addr)
    }
}

impl fmt::Debug for SourceMac {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

impl fmt::Debug for DstMac {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}
