mod api;
mod filecache;
pub mod root;
mod vblock;
///vfs module
mod vfs;
mod vfserror;
use alloc::sync::Arc;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;

use crate::sync::UPSafeCell;

pub use self::api::*;
pub use self::filecache::*;
pub use self::vblock::*;
pub use self::vfs::*;
pub use self::vfserror::*;

lazy_static! {
    /// 全局块设备注册表。
    ///
    /// 设计目标：
    /// 1. 各平台驱动在探测成功后统一向这里注册块设备；
    /// 2. VFS 层不再关心“当前是 QEMU 还是真机板子”；
    /// 3. 根文件系统初始化阶段只负责遍历这里的设备并生成 `/vda`、`/vda1` 等节点。
    pub static ref GLOBAL_BLOCKS: UPSafeCell<Vec<Arc<Mutex<dyn BlueBlk>>>> =
        UPSafeCell::new(Vec::new());
}

/// 注册一个已经探测并初始化成功的块设备。
pub fn register_global_block_device(device: Arc<Mutex<dyn BlueBlk>>) {
    GLOBAL_BLOCKS.lock().push(device);
}

/// 清空全局块设备注册表。
///
/// DTB 每次重新探测前应先清空，避免重复注册同一批设备。
pub fn clear_global_block_devices() {
    GLOBAL_BLOCKS.lock().clear();
}

/// 获取当前已经注册的全部块设备快照。
///
/// 返回 `Vec` 的副本，避免调用方长时间持有全局锁。
pub fn global_block_devices() -> Vec<Arc<Mutex<dyn BlueBlk>>> {
    GLOBAL_BLOCKS.lock().iter().cloned().collect()
}
