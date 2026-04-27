//! VFS 模块入口。
//!
//! ## 子模块结构
//!
//! | 模块 | 职责 |
//! |------|------|
//! | `vfs` | 核心 trait：`File`、`VfsFs`、`VfsStat`、`KStat`、`OpenFlags` |
//! | `vblock` | 块设备抽象：`BlockDevTrait` + `BLOCKDEVFILE` |
//! | `dm_liner` | dm-linear 线性映射条目 `DmlinerEntry` |
//! | `device` | 设备文件子 trait `DeviceFile`（含 `ioctl`） |
//! | `vfserror` | 统一错误类型 `VfsFsError` |
//! | `api` | 面向系统调用的高层 VFS 操作（open/mkdir/mount…） |
//! | `root` | 根文件系统管理：挂载点表、块设备扫描、根 FS 初始化 |
//! | `cache` | 块缓存与文件缓存 |
//!
//! ## 全局状态
//!
//! `GLOBAL_BLOCKS` 是唯一的全局可变状态——所有物理/虚拟块设备
//! 驱动探测完成后在此注册，VFS 层遍历它生成设备文件节点。

mod api;
pub mod cache;
pub mod device;
pub mod dm_liner;
pub mod root;
mod vblock;
mod vfs;
mod vfserror;

use alloc::sync::Arc;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;

use crate::sync::UPSafeCell;

pub use self::api::*;
pub use self::device::*;
pub use self::dm_liner::*;
pub use self::vblock::*;
pub use self::vfs::*;
pub use self::vfserror::*;

lazy_static! {
    /// 全局块设备注册表。
    ///
    /// 设计目标：
    /// 1. 各平台驱动在探测成功后统一向这里注册块设备；
    /// 2. VFS 层不关心"当前是 QEMU 还是真机板子"；
    /// 3. 根文件系统初始化阶段只负责遍历这里的设备并生成
    ///    `/vda`、`/vdb` 等整盘设备节点。
    pub static ref GLOBAL_BLOCKS: UPSafeCell<Vec<Arc<Mutex<dyn BlockDevTrait>>>> =
        UPSafeCell::new(Vec::new());
}

/// 注册一个已探测并初始化成功的块设备。
///
/// 调用时机：驱动 `probe` 回调中，设备初始化成功后立即调用。
/// 注册后，下一次 `scan_and_build_vblock_device()` 会为其创建设备文件。
pub fn register_global_block_device(device: Arc<Mutex<dyn BlockDevTrait>>) {
    GLOBAL_BLOCKS.lock().push(device);
}

/// 清空全局块设备注册表。
///
/// DTB 每次重新探测前应先清空，避免重复注册同一批设备。
pub fn clear_global_block_devices() {
    GLOBAL_BLOCKS.lock().clear();
}

/// 获取当前已注册的全部块设备快照。
///
/// 返回 `Vec` 的副本以释放全局锁；调用方不应长期持有此快照。
pub fn global_block_devices() -> Vec<Arc<Mutex<dyn BlockDevTrait>>> {
    GLOBAL_BLOCKS.lock().iter().cloned().collect()
}
