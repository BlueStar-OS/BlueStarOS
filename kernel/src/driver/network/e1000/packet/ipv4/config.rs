//! IPv4 本机配置。
//!
//! 本文件只承载 IPv4 地址等静态配置，不混入发包逻辑。

use crate::driver::network::e1000::agreenment::Ipv4Addr;

/// 本机 IPv4 地址。
///
/// 当前仍为临时硬编码，后续应由 DHCP / 静态网络配置模块接管。
pub const MY_IPV4: Ipv4Addr = Ipv4Addr::new([10, 0, 0, 2]);
