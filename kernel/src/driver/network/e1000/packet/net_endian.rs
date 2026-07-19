//! 网络字节序整数 newtype。
//!
//! 本文件只负责协议字段的线速大端存储语义。

use core::fmt;

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
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
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
