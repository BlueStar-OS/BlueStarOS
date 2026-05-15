//! ARP 缓存表。

use alloc::collections::btree_map::BTreeMap;
use lazy_static::lazy_static;

use crate::driver::network::e1000::agreenment::{DstMac, Ipv4Addr};
use crate::sync::UPSafeCell;

/// ARP 缓存表: IP -> MAC。
pub struct ArpTable {
    table: BTreeMap<Ipv4Addr, DstMac>,
}

impl ArpTable {
    /// 创建空的 ARP 缓存表。
    pub fn new() -> Self {
        Self {
            table: BTreeMap::new(),
        }
    }

    /// 查询 IP 对应的 MAC。
    pub fn lookup(&self, ip: &Ipv4Addr) -> Option<&DstMac> {
        self.table.get(ip)
    }

    /// 插入或更新 IP -> MAC 映射。
    pub fn insert(&mut self, ip: Ipv4Addr, mac: DstMac) {
        self.table.insert(ip, mac);
    }
}

lazy_static! {
    /// 全局 ARP 缓存表。
    pub static ref ARP_TABLE: UPSafeCell<ArpTable> = UPSafeCell::new(ArpTable::new());
}
