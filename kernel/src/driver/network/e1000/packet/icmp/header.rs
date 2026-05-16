//! ICMP 头定义。
//!
//! 参考 Linux:
//! - `/home/inkbottle/桌面/linux-5.4.29/include/uapi/linux/icmp.h:23-78`

use core::fmt;

use crate::driver::network::e1000::netbuffer::NetHeader;
use crate::driver::network::e1000::packet::net_endian::{Net16, Net32, Net8};

/// ICMP Echo Reply。
pub const ICMP_ECHOREPLY: u8 = 0;
/// ICMP Destination Unreachable。
pub const ICMP_DEST_UNREACH: u8 = 3;
/// ICMP Echo Request。
pub const ICMP_ECHO: u8 = 8;
/// ICMP Time Exceeded。
pub const ICMP_TIME_EXCEEDED: u8 = 11;
/// ICMP Parameter Problem。
pub const ICMP_PARAMETERPROB: u8 = 12;

/// ICMP 固定首部长度，单位字节。
pub const ICMP_HEADER_LEN: usize = 8;

/// ICMP `type` 字段。
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct IcmpType(Net8);

impl IcmpType {
    pub const ECHOREPLY: Self = Self(Net8::new(ICMP_ECHOREPLY));
    pub const DEST_UNREACH: Self = Self(Net8::new(ICMP_DEST_UNREACH));
    pub const ECHO: Self = Self(Net8::new(ICMP_ECHO));
    pub const TIME_EXCEEDED: Self = Self(Net8::new(ICMP_TIME_EXCEEDED));
    pub const PARAMETERPROB: Self = Self(Net8::new(ICMP_PARAMETERPROB));

    pub const fn new(host: u8) -> Self {
        Self(Net8::new(host))
    }

    pub const fn host(self) -> u8 {
        self.0.host()
    }
}

impl fmt::Debug for IcmpType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match *self {
            Self::ECHOREPLY => "EchoReply",
            Self::DEST_UNREACH => "DestUnreach",
            Self::ECHO => "EchoRequest",
            Self::TIME_EXCEEDED => "TimeExceeded",
            Self::PARAMETERPROB => "ParameterProb",
            _ => "Unknown",
        };
        write!(f, "{}({})", name, self.host())
    }
}

/// Destination Unreachable 的 `code` 语义。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IcmpDestUnreachCode {
    NetUnreach,
    HostUnreach,
    ProtUnreach,
    PortUnreach,
    FragNeeded,
    SourceRouteFailed,
    NetUnknown,
    HostUnknown,
    HostIsolated,
    NetProhibited,
    HostProhibited,
    NetUnreachTos,
    HostUnreachTos,
    PacketFiltered,
    PrecedenceViolation,
    PrecedenceCutoff,
    Other(u8),
}

impl IcmpDestUnreachCode {
    pub const fn host(self) -> u8 {
        match self {
            Self::NetUnreach => 0,
            Self::HostUnreach => 1,
            Self::ProtUnreach => 2,
            Self::PortUnreach => 3,
            Self::FragNeeded => 4,
            Self::SourceRouteFailed => 5,
            Self::NetUnknown => 6,
            Self::HostUnknown => 7,
            Self::HostIsolated => 8,
            Self::NetProhibited => 9,
            Self::HostProhibited => 10,
            Self::NetUnreachTos => 11,
            Self::HostUnreachTos => 12,
            Self::PacketFiltered => 13,
            Self::PrecedenceViolation => 14,
            Self::PrecedenceCutoff => 15,
            Self::Other(v) => v,
        }
    }
}

impl From<u8> for IcmpDestUnreachCode {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::NetUnreach,
            1 => Self::HostUnreach,
            2 => Self::ProtUnreach,
            3 => Self::PortUnreach,
            4 => Self::FragNeeded,
            5 => Self::SourceRouteFailed,
            6 => Self::NetUnknown,
            7 => Self::HostUnknown,
            8 => Self::HostIsolated,
            9 => Self::NetProhibited,
            10 => Self::HostProhibited,
            11 => Self::NetUnreachTos,
            12 => Self::HostUnreachTos,
            13 => Self::PacketFiltered,
            14 => Self::PrecedenceViolation,
            15 => Self::PrecedenceCutoff,
            other => Self::Other(other),
        }
    }
}

/// Time Exceeded 的 `code` 语义。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IcmpTimeExceededCode {
    TtlExceeded,
    FragReassemblyExceeded,
    Other(u8),
}

impl IcmpTimeExceededCode {
    pub const fn host(self) -> u8 {
        match self {
            Self::TtlExceeded => 0,
            Self::FragReassemblyExceeded => 1,
            Self::Other(v) => v,
        }
    }
}

impl From<u8> for IcmpTimeExceededCode {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::TtlExceeded,
            1 => Self::FragReassemblyExceeded,
            other => Self::Other(other),
        }
    }
}

/// Parameter Problem 的 `code` 语义。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IcmpParameterProblemCode {
    PointerIndicatesError,
    MissingRequiredOption,
    BadLength,
    Other(u8),
}

impl IcmpParameterProblemCode {
    pub const fn host(self) -> u8 {
        match self {
            Self::PointerIndicatesError => 0,
            Self::MissingRequiredOption => 1,
            Self::BadLength => 2,
            Self::Other(v) => v,
        }
    }
}

impl From<u8> for IcmpParameterProblemCode {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::PointerIndicatesError,
            1 => Self::MissingRequiredOption,
            2 => Self::BadLength,
            other => Self::Other(other),
        }
    }
}

/// ICMP 语义化 kind。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IcmpKind {
    EchoReply,
    EchoRequest,
    DestUnreach(IcmpDestUnreachCode),
    TimeExceeded(IcmpTimeExceededCode),
    ParameterProblem(IcmpParameterProblemCode),
    Other { icmp_type: u8, code: u8 },
}

impl IcmpKind {
    pub const fn icmp_type(self) -> IcmpType {
        match self {
            Self::EchoReply => IcmpType::ECHOREPLY,
            Self::EchoRequest => IcmpType::ECHO,
            Self::DestUnreach(_) => IcmpType::DEST_UNREACH,
            Self::TimeExceeded(_) => IcmpType::TIME_EXCEEDED,
            Self::ParameterProblem(_) => IcmpType::PARAMETERPROB,
            Self::Other { icmp_type, .. } => IcmpType::new(icmp_type),
        }
    }

    pub const fn icmp_code(self) -> IcmpCode {
        match self {
            Self::EchoReply | Self::EchoRequest => IcmpCode::ZERO,
            Self::DestUnreach(code) => IcmpCode::new(code.host()),
            Self::TimeExceeded(code) => IcmpCode::new(code.host()),
            Self::ParameterProblem(code) => IcmpCode::new(code.host()),
            Self::Other { code, .. } => IcmpCode::new(code),
        }
    }
}

impl From<(IcmpType, IcmpCode)> for IcmpKind {
    fn from(value: (IcmpType, IcmpCode)) -> Self {
        match value.0 {
            IcmpType::ECHOREPLY => Self::EchoReply,
            IcmpType::ECHO => Self::EchoRequest,
            IcmpType::DEST_UNREACH => Self::DestUnreach(IcmpDestUnreachCode::from(value.1.host())),
            IcmpType::TIME_EXCEEDED => {
                Self::TimeExceeded(IcmpTimeExceededCode::from(value.1.host()))
            }
            IcmpType::PARAMETERPROB => {
                Self::ParameterProblem(IcmpParameterProblemCode::from(value.1.host()))
            }
            other => Self::Other {
                icmp_type: other.host(),
                code: value.1.host(),
            },
        }
    }
}

/// ICMP `code` 原始线速字段。
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct IcmpCode(Net8);

impl IcmpCode {
    pub const ZERO: Self = Self(Net8::ZERO);

    pub const fn new(host: u8) -> Self {
        Self(Net8::new(host))
    }

    pub const fn host(self) -> u8 {
        self.0.host()
    }
}

impl fmt::Debug for IcmpCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.host())
    }
}

/// ICMP 线速报文头。
///
/// 对齐 Linux `struct icmphdr` 的公共前 8 字节布局。
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct IcmpHeader {
    /// ICMP 类型。
    pub icmp_type: IcmpType,
    /// ICMP 细分代码。
    pub code: IcmpCode,
    /// ICMP 校验和。
    pub checksum: Net16,
    /// 变体负载的前 4 字节。
    pub rest: Net32,
}

const _: () = assert!(core::mem::size_of::<IcmpHeader>() == ICMP_HEADER_LEN);

// SAFETY: `IcmpHeader` 的线速布局固定且与 ICMP 固定头一致。
unsafe impl NetHeader for IcmpHeader {
    const WIRE_LEN: usize = ICMP_HEADER_LEN;
}

impl IcmpHeader {
    /// 通用构造函数。
    pub fn new(kind: IcmpKind, rest_host: u32) -> Self {
        Self {
            icmp_type: kind.icmp_type(),
            code: kind.icmp_code(),
            checksum: Net16::ZERO,
            rest: Net32::new(rest_host),
        }
    }

    /// Echo Request / Reply 构造函数。
    pub fn new_echo(kind: IcmpKind, ident: u16, sequence: u16) -> Self {
        let rest_host = ((ident as u32) << 16) | sequence as u32;
        Self::new(kind, rest_host)
    }

    /// 以大端序返回 ICMP 头部字节切片。
    pub fn to_be_bytes(&self) -> &[u8] {
        // SAFETY: `IcmpHeader` 为 packed 且固定 8 字节。
        unsafe {
            core::slice::from_raw_parts(self as *const Self as *const u8, core::mem::size_of::<Self>())
        }
    }

    /// Echo 报文的 identifier。
    pub fn echo_ident(self) -> u16 {
        (self.rest.host() >> 16) as u16
    }

    /// Echo 报文的 sequence。
    pub fn echo_sequence(self) -> u16 {
        (self.rest.host() & 0xFFFF) as u16
    }

    /// 将头部线速 `type/code` 解释为语义化 kind。
    pub fn kind(self) -> IcmpKind {
        IcmpKind::from((self.icmp_type, self.code))
    }
}

impl fmt::Debug for IcmpHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let self_b = self as *const Self as *const u8;
        let icmp_type = unsafe { (self_b as *const IcmpType).read_unaligned() };
        let code = unsafe { (self_b.add(1) as *const IcmpCode).read_unaligned() };
        let checksum = unsafe { (self_b.add(2) as *const Net16).read_unaligned() };
        let rest = unsafe { (self_b.add(4) as *const Net32).read_unaligned() };
        write!(
            f,
            "ICMP: type={:?} code={:?} checksum={:#06x} rest={:#010x}",
            icmp_type,
            code,
            checksum.host(),
            rest.host()
        )
    }
}
