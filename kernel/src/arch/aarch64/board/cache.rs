//! 真机的cache更严格，我们应该封装一套cache清理函数
//!
//!
//!

use virtio_drivers::PhysAddr;

pub fn sync_icache_for_user(addr: PhysAddr, len: usize) {}
