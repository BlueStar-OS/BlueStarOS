//! 设备文件抽象。
//!
//! `DeviceFile` 是 `File` 的子 trait，为设备节点（块设备、字符设备
//! 等）提供 `ioctl` 扩展入口。普通文件（管道、ramfs 文件）不实现此 trait。

use crate::fs::vfs::File;
use crate::VfsFsError;

/// `ioctl` 命令号 newtype。
///
/// 包装 `u64` 以在将来支持命令编码/解码方法。
pub struct IOCTL_CMD(u64);

/// `ioctl` 用户态参数指针 newtype。
///
/// 包装 `usize` 以标明语义——这不是普通整数，是用户空间地址。
pub struct IOCTL_ARGPTR(usize);

/// 设备文件接口：在 `File` 基础上增加设备控制操作。
///
/// ## 设计意图
///
/// 不是所有 `File` 都是设备。`DeviceFile` 提供清晰的类型分界：
/// - `Arc<dyn File>` → 通用文件，可用 `read`/`write`/`stat`
/// - `Arc<dyn DeviceFile>` → 设备文件，额外支持 `ioctl`
///
/// ## 后续扩展
///
/// dm-linear 创建/删除逻辑卷、LVM 元数据下发等操作均可通过
/// `ioctl` 从用户态传入（命令号 + 用户空间结构体指针）。
pub trait DeviceFile: File {
    /// 设备控制接口。
    ///
    /// `cmd`: 命令号（类似 Linux 的 `_IO`/`_IOW`/`_IOR` 宏编码）。
    /// `arg`: 用户空间参数指针，具体含义由命令决定。
    fn ioctl(&self, cmd: u32, arg: usize) -> Result<usize, VfsFsError>;
}
