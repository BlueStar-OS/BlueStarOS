//! 协议头类型定义
//!
//! 以太网帧头、IPv4 头、UDP 头
//! 参考 Linux: include/uapi/linux/if_ether.h
//!           include/uapi/linux/ip.h
//!           include/uapi/linux/udp.h

use core::fmt;

// ===== 以太网常量 =====
// 参考 Linux: include/uapi/linux/if_ether.h 第32-41行

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

//  TODO:ip地址

// ===== MAC地址 newtype =====

/// 源MAC地址 (6字节)
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SourceMac(pub [u8; ETH_ALEN]);

/// 目的MAC地址 (6字节)
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DstMac(pub [u8; ETH_ALEN]);

// ===== EtherType newtype =====

/// 以太网协议类型 (EtherType, 网络字节序大端)
///
/// 对应 Linux 的 `__be16 h_proto`
/// 参考 Linux: include/uapi/linux/if_ether.h 第47-118行
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct EtherType(Net16);

/// 常用 EtherType 常量
///
/// 值定义参考 Linux: include/uapi/linux/if_ether.h
#[allow(dead_code)]
impl EtherType {
    /// Internet Protocol packet: ETH_P_IP = 0x0800
    pub const IPV4: Self = EtherType::new(0x0800);
    /// Address Resolution packet: ETH_P_ARP = 0x0806
    pub const ARP: Self = EtherType::new(0x0806);
    /// Reverse Addr Res packet: ETH_P_RARP = 0x8035
    pub const RARP: Self = EtherType::new(0x8035);
    /// 802.1Q VLAN Extended Header: ETH_P_8021Q = 0x8100
    pub const VLAN: Self = EtherType::new(0x8100);
    /// IPv6 over bluebook: ETH_P_IPV6 = 0x86DD
    pub const IPV6: Self = EtherType::new(0x86DD);
    /// Every packet: ETH_P_ALL = 0x0003
    pub const ALL: Self = EtherType::new(0x0003);
    /// Ethernet loopback: ETH_P_LOOPBACK = 0x9000
    pub const LOOPBACK: Self = EtherType::new(0x9000);

    /// 从主机字节序构造 EtherType。
    pub const fn new(host: u16) -> Self {
        Self(Net16::new(host))
    }

    /// 转为主机字节序
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

// ===== 以太网帧头 =====

/// 以太网帧头 (14字节, packed 无填充)
///
/// 参考 Linux: include/uapi/linux/if_ether.h 第163-167行
///
/// ```c
/// struct ethhdr {
///     unsigned char h_dest[ETH_ALEN];    // 6字节 目的MAC
///     unsigned char h_source[ETH_ALEN];  // 6字节 源MAC
///     __be16       h_proto;              // 2字节 EtherType
/// } __attribute__((packed));
/// ```
///
/// 数据流过程:
/// ```
/// [DstMac 6B] [SourceMac 6B] [EtherType 2B] [payload ... ]
/// ```
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct EthHead {
    /// 目的MAC地址
    pub dest_mac: DstMac,
    /// 源MAC地址
    pub source_mac: SourceMac,
    /// 以太网协议类型 (大端)
    pub ether_type: EtherType,
}

// type guard
const _: () = assert!(core::mem::size_of::<EthHead>() == 14);

impl EthHead {
    /// 构造一个以太网帧头。
    pub fn new(dest_mac: DstMac, source_mac: SourceMac, ether_type: EtherType) -> Self {
        Self {
            dest_mac,
            source_mac,
            ether_type,
        }
    }

    /// 从字节切片解析以太网帧头
    ///
    /// 对应 Linux: eth_type_trans 内部通过 (struct ethhdr *)skb->data 直接转换
    /// 参考 Linux: net/ethernet/eth.c 第155-165行
    pub fn from_bytes(bytes: &[u8]) -> Option<&Self> {
        if bytes.len() < ETH_HLEN {
            return None;
        }
        // # Safety: EthHead 是 #[repr(C, packed)]，且 bytes 保证长度 >= 14
        // 保证对齐：传入的 skb->data 通常已按 2/4 字节对齐，packed 结构体不受对齐影响
        Some(unsafe { &*(bytes.as_ptr() as *const EthHead) })
    }

    /// 从字节切片解析可变引用
    pub fn from_bytes_mut(bytes: &mut [u8]) -> Option<&mut Self> {
        if bytes.len() < ETH_HLEN {
            return None;
        }
        Some(unsafe { &mut *(bytes.as_mut_ptr() as *mut EthHead) })
    }
}

impl fmt::Debug for EthHead {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // `&self.field` 在 packed 结构体上创建引用是 UB, 改用 raw ptr + 字节偏移
        // # Safety: EthHead 是 #[repr(C, packed)], 字节偏移是有保证的:
        //   offset 0: DstMac   (6B)
        //   offset 6: SourceMac(6B)
        //   offset 12: EtherType(2B)
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
        // SAFETY: EthHead 是 #[repr(C, packed)], 字节偏移有保证:
        //   offset 0: DstMac   (6B)
        //   offset 6: SourceMac(6B)
        //   offset 12: EtherType(2B)
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

// ===== newtype 实用方法 =====

impl SourceMac {
    /// 广播地址: FF:FF:FF:FF:FF:FF
    pub const BROADCAST: Self = SourceMac([0xFF; ETH_ALEN]);
    /// 零地址
    pub const ZERO: Self = SourceMac([0x00; ETH_ALEN]);

    pub fn new(addr: [u8; ETH_ALEN]) -> Self {
        SourceMac(addr)
    }
}

impl DstMac {
    /// 广播地址: FF:FF:FF:FF:FF:FF
    pub const BROADCAST: Self = DstMac([0xFF; ETH_ALEN]);
    /// 零地址
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

// ===== IP 地址 newtype =====

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

// ===== IPv4 地址通用类型 =====

/// IPv4 地址 (4字节, 网络字节序)
///
/// 提供 Ord 用于 BTreeMap 等有序容器。
/// 参考 Linux: include/uapi/linux/in.h `struct in_addr`
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ipv4Addr(Net32);

impl Ipv4Addr {
    pub const ZERO: Self = Ipv4Addr::new([0; 4]);

    pub const fn new(addr: [u8; 4]) -> Self {
        Self(Net32::new(u32::from_be_bytes(addr)))
    }

    /// 从网络字节序 u32 创建
    pub fn from_u32(net_order: u32) -> Self {
        Self(Net32::new(u32::from_be(net_order)))
    }

    /// 转网络字节序 u32
    pub fn to_u32(self) -> u32 {
        self.0.host().to_be()
    }

    /// 返回按线速顺序排列的 4 个字节。
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

// ===== UDP 端口 newtype =====

/// UDP 源端口 (网络字节序)
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SourcePort(Net16);

impl SourcePort {
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

// ===== 网络序整数 newtype =====

/// 8 位网络序字段。
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Net8(u8);

impl Net8 {
    pub const ZERO: Self = Self::new(0);

    pub const fn new(host: u8) -> Self {
        Self(host)
    }

    pub const fn host(self) -> u8 {
        self.0
    }
}

impl fmt::Debug for Net8 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.host())
    }
}

/// 16 位网络序字段。
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Net16(u16);

impl Net16 {
    pub const ZERO: Self = Self::new(0);

    pub const fn new(host: u16) -> Self {
        Self(host.to_be())
    }

    pub fn host(self) -> u16 {
        u16::from_be(self.0)
    }
}

impl fmt::Debug for Net16 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.host())
    }
}

/// 32 位网络序字段。
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Net32(u32);

impl Net32 {
    pub const ZERO: Self = Self::new(0);

    pub const fn new(host: u32) -> Self {
        Self(host.to_be())
    }

    pub fn host(self) -> u32 {
        u32::from_be(self.0)
    }
}

impl fmt::Debug for Net32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.host())
    }
}

/// 64 位网络序字段。
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Net64(u64);

impl Net64 {
    pub const ZERO: Self = Self::new(0);

    pub const fn new(host: u64) -> Self {
        Self(host.to_be())
    }

    pub fn host(self) -> u64 {
        u64::from_be(self.0)
    }
}

impl fmt::Debug for Net64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.host())
    }
}

// ===== IPv4 / UDP 头部语义常量 =====

/// IPv4 版本号。
///
/// 参考 Linux: /home/inkbottle/桌面/linux-5.4.29/include/uapi/linux/ip.h:87-95
pub const IPV4_VERSION: u8 = 4;
/// IPv4 无选项首部的 IHL 值，单位为 32-bit word。
///
/// 参考 Linux: /home/inkbottle/桌面/linux-5.4.29/include/uapi/linux/ip.h:87-95
pub const IPV4_MIN_IHL_WORDS: u8 = 5;
/// IPv4 无选项首部长度，单位字节。
///
/// 参考 Linux: /home/inkbottle/桌面/linux-5.4.29/include/uapi/linux/ip.h:87-105
pub const IPV4_HEADER_LEN: u16 = 20;
/// UDP 固定首部长度，单位字节。
///
/// 参考 Linux: /home/inkbottle/桌面/linux-5.4.29/include/uapi/linux/udp.h:20-25
pub const UDP_HEADER_LEN: u16 = 8;
/// 常规 IPv4 默认 TTL。
pub const IPV4_DEFAULT_TTL: u8 = 64;
/// 不分片且 fragment offset 为 0。
pub const IPV4_FRAG_OFF_NONE: Net16 = Net16::ZERO;

/// IPv4 `version` + `ihl` 组合字段。
///
/// 对应 Linux `struct iphdr` 第 1 字节:
/// `ihl:4` + `version:4`
/// 参考 Linux: /home/inkbottle/桌面/linux-5.4.29/include/uapi/linux/ip.h:87-95
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

/// IPv4 DSCP + ECN 字段。
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

/// IPv4 TTL 字段。
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

/// IPv4 协议号字段。
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

// ===== IPv4 报文头 =====

/// IPv4 报文头 (20 字节, 不含选项)
///
/// 参考 Linux: include/uapi/linux/ip.h:96-113 `struct iphdr`
/// ```c
/// struct iphdr {
///     __u8    ihl:4,      // 首部长度 (×4字节)
///             version:4;  // 版本号 (IPv4=4)
///     __u8    tos;
///     __be16  tot_len;
///     __be16  id;
///     __be16  frag_off;
///     __u8    ttl;
///     __u8    protocol;
///     __be16  check;
///     __be32  saddr;
///     __be32  daddr;
/// };
/// ```
///
/// 数据流过程:
/// ```
/// [EthHead 14B] [IPv4Header 20B] [UdpHeader 8B] [payload ... ]
/// ```
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct IPv4Header {
    /// version(4) + ihl(4) 组合字节
    pub version_ihl: Ipv4VersionIhl,
    /// DSCP + ECN
    pub tos: Ipv4DscpEcn,
    /// IP 报文总长度 (含首部, 网络字节序)
    pub total_length: Net16,
    /// 标识符 (网络字节序)
    pub id: Net16,
    /// flags(3) + fragment_offset(13) (网络字节序)
    pub flags_frag_offset: Net16,
    /// 生存时间
    pub ttl: Ipv4Ttl,
    /// 上层协议 (TCP=6, UDP=17)
    pub protocol: Ipv4Protocol,
    /// 首部校验和 (网络字节序)
    pub checksum: Net16,
    /// 源 IP 地址
    pub source_addr: SourceIp,
    /// 目的 IP 地址
    pub dest_addr: DestIp,
}

/// 编译期断言: IPv4Header 必须是 20 字节
const _: () = assert!(core::mem::size_of::<IPv4Header>() == 20);

impl IPv4Header {
    /// 构造一个不带选项的 IPv4 基本首部。
    ///
    /// 所有多字节字段都以网络字节序保存，对齐 Linux `struct iphdr`
    /// 的 `__be16` / `__be32` 约定。
    /// 参考 Linux: /home/inkbottle/桌面/linux-5.4.29/include/uapi/linux/ip.h:96-105
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

    /// 以大端序返回 IPv4 头部字节切片 (零成本转换)
    ///
    /// 字段已按网络字节序存储，直接 reinterpret。
    pub fn to_be_bytes(&self) -> &[u8] {
        // SAFETY: IPv4Header 是 #[repr(C, packed)]，无填充，且所有字段都以线速布局存放。
        unsafe { core::slice::from_raw_parts(self as *const Self as *const u8, 20) }
    }

    /// 计算并填入 IPv4 首部校验和 (RFC 1071)
    ///
    /// 先将 checksum 置零，累加所有 10 个 16-bit 字（反码求和），
    /// 将进位折叠到低位，最后按位取反，以网络字节序写入 checksum。
    /// 参考: RFC 1071 §2, Linux: /home/inkbottle/桌面/linux-5.4.29/net/ipv4/ip_output.c:91-95
    pub fn checksum_rfc_for_self(&mut self) {
        // 1. 校验和字段必须清零后再计算
        self.checksum = Net16::ZERO;

        // 2. 以 u16 为单位累加 (20 字节 = 10 × u16)
        //    使用 read_unaligned 避免 packed 结构体的对齐问题
        let mut sum: u32 = 0;
        let base = self as *const Self as *const u16;
        for i in 0..10 {
            // SAFETY: IPv4Header 固定 20 字节，10 个 u16，字偏移安全
            sum = sum.wrapping_add(unsafe { base.add(i).read_unaligned() } as u32);
        }

        // 3. 折叠进位: 高 16 位的进位反复加回低 16 位
        while sum >> 16 != 0 {
            sum = (sum & 0xFFFF) + (sum >> 16);
        }

        // 4. 取反 → 以网络字节序 (大端) 存储
        self.checksum = Net16::new(!(sum as u16));
    }
}

impl fmt::Debug for IPv4Header {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // SAFETY: IPv4Header is #[repr(C, packed)], use raw ptr arithmetic
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

// ===== UDP 报文头 =====

/// UDP 报文头 (8 字节)
///
/// 参考 Linux: include/uapi/linux/udp.h:20-26 `struct udphdr`
/// ```c
/// struct udphdr {
///     __be16  source;
///     __be16  dest;
///     __be16  len;
///     __be16  check;
/// };
/// ```
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct UdpHeader {
    /// 源端口 (网络字节序)
    pub source_port: SourcePort,
    /// 目的端口 (网络字节序)
    pub dest_port: DestPort,
    /// UDP 报文长度 (含首部 8 字节, 网络字节序)
    pub length: Net16,
    /// 校验和 (网络字节序, 0 表示未计算)
    pub checksum: Net16,
}

/// 编译期断言: UdpHeader 必须是 8 字节
const _: () = assert!(core::mem::size_of::<UdpHeader>() == 8);

impl UdpHeader {
    /// 构造一个 UDP 固定首部。
    ///
    /// `payload_len` 不含 8 字节 UDP 头。
    /// 参考 Linux: /home/inkbottle/桌面/linux-5.4.29/include/uapi/linux/udp.h:20-25
    pub fn new(source_port: SourcePort, dest_port: DestPort, payload_len: u16) -> Self {
        Self {
            source_port,
            dest_port,
            length: Net16::new(UDP_HEADER_LEN + payload_len),
            checksum: Net16::ZERO,
        }
    }

    /// 以大端序返回 UDP 头部字节切片 (零成本转换)
    ///
    /// 字段已按网络字节序存储，直接 reinterpret。
    pub fn to_be_bytes(&self) -> &[u8] {
        // SAFETY: UdpHeader 是 #[repr(C, packed)]，无填充，且所有字段都以线速布局存放。
        unsafe { core::slice::from_raw_parts(self as *const Self as *const u8, 8) }
    }
}

impl fmt::Debug for UdpHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // SAFETY: UdpHeader is #[repr(C, packed)], use raw ptr arithmetic
        let self_b = self as *const Self as *const u8;
        let src = unsafe { (self_b as *const SourcePort).read_unaligned() };
        let dst = unsafe { (self_b.add(2) as *const DestPort).read_unaligned() };
        let len = unsafe { (self_b.add(4) as *const Net16).read_unaligned() };
        write!(f, "UDP: {:?} -> {:?}  len={}", src, dst, len.host())
    }
}
