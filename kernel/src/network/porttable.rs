//! 全局端口绑定表。
//!
//! 将本地端口号映射到对应的 socket 文件句柄，用于:
//! - `sys_bind`: 端口冲突检测 + 注册
//! - 中断收包路径: 根据 UDP dst_port 查找目标 socket，调用 `push_rx` 分发数据
//!
//! `PortTable` 不解析协议，也不拥有 socket 生命周期。它只保存一份
//! `Arc<dyn File>`，close 时由 socket 主动 `unbind`，最后一个 `Arc` 释放后
//! socket 才真正析构。
//!
//! ## 对标 Linux
//!
//! | Linux | BlueStarOS |
//! |-------|------------|
//! | `struct udp_table` (net/ipv4/udp.c) | `PortTable.table` |
//! | `udp_lib_get_port` (net/ipv4/udp.c:1451) | `PortTable::bind()` |
//! | `__udp4_lib_lookup` (net/ipv4/udp.c:480) | `PortTable::lookup()` |
//! | `udp_lib_unhash` (net/ipv4/udp.c:1578) | `PortTable::unbind()` |

use alloc::{collections::btree_map::BTreeMap, sync::Arc};
use lazy_static::lazy_static;

use crate::{fs::vfs::File, network::NetPort, sync::UPSafeCell};

/// 本地端口绑定表。
///
/// 内部使用 `BTreeMap<NetPort, Arc<dyn File>>` 存储端口到 socket 的映射。
/// 通过 `UPSafeCell` 提供单核内部可变性；它不是可重入锁，因此 IRQ 路径
/// 调用时必须避免与 syscall 路径嵌套借用。
pub struct PortTable {
    table: UPSafeCell<BTreeMap<NetPort, Arc<dyn File>>>,
}

impl Default for PortTable {
    fn default() -> Self {
        Self::new()
    }
}

impl PortTable {
    /// 创建一个空的端口表。
    pub fn new() -> Self {
        PortTable {
            table: UPSafeCell::new(BTreeMap::new()),
        }
    }

    /// 绑定端口: 将 `fd` 注册到 `port`。
    ///
    /// - 成功: 返回 `Ok(true)`
    /// - 端口已被占用: 返回 `Ok(false)`（调用方应返回 `EADDRINUSE`）
    pub fn bind(&self, port: NetPort, fd: Arc<dyn File>) -> bool {
        self.table.lock(|tb| {
            if tb.contains_key(&port) {
                return false;
            }
            tb.insert(port, fd);
            true
        })
    }

    /// 查找端口对应的 socket。
    ///
    /// 中断收包路径调用: 根据 UDP dst_port 查找目标 socket 以分发数据。
    ///
    /// TODO(IRQ-safety): 当前 `lookup()` 会在 e1000 IRQ 收包路径中执行，
    /// 而 `bind()` / `unbind()` 来自 syscall/close 路径。二者都借用同一个
    /// `UPSafeCell`，如果 IRQ 打断了正在持有端口表的代码，会触发双重借用
    /// panic。后续应改为关中断临界区、无锁读表，或把收包分发延迟到 softirq。
    pub fn lookup(&self, port: NetPort) -> Option<Arc<dyn File>> {
        self.table.lock(|tb| tb.get(&port).cloned())
    }

    /// 解除端口绑定。
    ///
    /// 对标 Linux: `udp_lib_unhash` — socket 关闭时清理端口映射。
    pub fn unbind(&self, port: NetPort) {
        self.table.lock(|tb| {
            tb.remove(&port);
        });
    }
}

lazy_static! {
    /// 全局端口绑定表。
    ///
    /// 对标 Linux: `udp_table` 全局变量。
    /// 所有 `sys_bind` / 收包分发 / socket 关闭都通过此实例操作。
    pub static ref PORT_TABLE: PortTable = PortTable::new();
}
