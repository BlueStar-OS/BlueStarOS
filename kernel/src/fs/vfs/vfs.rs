//! VFS 核心抽象层。
//!
//! ## 分层关系
//!
//! ```text
//! syscall (open/read/write/stat/...)
//!   │
//!   ▼
//! VFS api 层  ── 路径解析、挂载点查找
//!   │
//!   ├─► File trait        ← 每个打开的文件（inode、设备、管道）都实现它
//!   └─► VfsFs trait       ← 每种文件系统（ramfs/ext4/fat32）都实现它
//! ```
//!
//! `File` 描述"一个已打开的文件"，`VfsFs` 描述"一类文件系统实例"。
//! 两者组合完成从路径到字节的完整调用链。

use crate::fs::vfs::vfserror::VfsFsError;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use bitflags::bitflags;
use core::any::Any;
use spin::Mutex;

/// 挂载点表中每个条目的类型：`Arc<Mutex<dyn VfsFs>>`
pub type MountFs = Arc<Mutex<dyn VfsFs>>;

/// 目录项类型，用于 `VfsStat::file_type` 和 `getdents64` 返回。
pub enum EntryType {
    File,
    Dir,
}

// ── 文件打开标志 ──────────────────────────────────────────────

bitflags! {
    /// `open` / `openat` 的标志位，与 Linux `fcntl.h` 的 `O_*` 值对齐。
    #[derive(Debug, Clone, Copy)]
    pub struct OpenFlags: usize {
        /// 只读
        const RONLY = 0;
        /// 只写
        const WRONLY = 1 << 0;
        /// 读写
        const RDWR = 1 << 1;
        /// 若文件不存在则创建
        const CREAT = 1 << 6;
        /// 打开时截断长度为零
        const TRUNC = 1 << 9;
        /// 写入时追加到文件末尾
        const APPEND = 1 << 10;
        /// 要求路径指向的是一个目录
        const DIRECTORY = 1 << 21;
    }
}

impl OpenFlags {
    /// `O_ACCMODE` —— 低 2 bit 提取访问模式
    pub const ACCMODE_MASK: usize = 0x3;

    /// 返回纯访问模式 bits（RONLY / WRONLY / RDWR）
    pub fn accmode(self) -> usize {
        self.bits() & Self::ACCMODE_MASK
    }

    /// 文件是否可读
    pub fn readable(self) -> bool {
        self.accmode() != 0x001
    }

    /// 文件是否可写
    pub fn writable(self) -> bool {
        matches!(self.accmode(), 0x001 | 0x002)
    }
}

// ── 文件属性 ──────────────────────────────────────────────────

/// VFS 内部使用的精简 stat 结构。
///
/// 只包含系统运行必需的核心字段，不暴露 uid/gid/时间戳等
/// 当前尚未实现的 POSIX 语义。用户态通过 `KStat` 看到补齐后的结果。
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VfsStat {
    /// inode 编号（ramfs 下通常为 0）
    pub inode: u32,
    /// 文件大小（字节）
    pub size: u64,
    /// 权限模式位（目前未强制检查）
    pub mode: u32,
    /// 文件类型：`VFS_DT_REG` / `VFS_DT_DIR` / `VFS_DT_LNK`
    pub file_type: u32,
}

pub const VFS_DT_UNKNOWN: u32 = 0;
pub const VFS_DT_REG: u32 = 8;
pub const VFS_DT_DIR: u32 = 4;
pub const VFS_DT_LNK: u32 = 10;

// ── 目录项 ────────────────────────────────────────────────────

/// Linux `dirent64` 结构体的内存布局。
///
/// `d_name` 为变长字段，紧跟在此结构体之后；`d_reclen` 给出本条目的
/// 总字节数（含对齐），据此跳转到下一条目录项。
#[repr(C, packed)]
#[derive(Clone, Copy, Debug, Default)]
pub struct LinuxDirent64 {
    /// inode 号
    pub d_ino: u64,
    /// 当前目录文件中的偏移（用于 telldir/seekdir）
    pub d_off: u64,
    /// 本条目录项的总长度（含 `d_name` 和对齐填充）
    pub d_reclen: u16,
    /// 文件类型（`VFS_DT_*`）
    pub d_type: u8,
    // d_name 紧跟在此之后，不在此结构体中
}

// ── 文件偏移 ──────────────────────────────────────────────────

/// 文件内部读写偏移量（文件光标）。
///
/// newtype 包装 `u64`，避免与 LBA、字节长度等其他 `u64` 混淆。
#[derive(Clone, Copy, Debug)]
pub struct FileOffset(pub u64);

// ── File trait ────────────────────────────────────────────────

/// 通用文件抽象：每个已打开的文件句柄都实现此 trait。
///
/// ## 两个读写层次
///
/// | 方法 | 语义 |
/// |------|------|
/// | `read` / `write` | 从当前光标顺序读写，读后推进光标 |
/// | `read_at` / `write_at` | 指定偏移随机读写，不触动光标 |
///
/// 块设备文件同时支持两层（内部维护光标），管道等流式文件通常
/// 只实现 `read`/`write`，`read_at`/`write_at` 返回 `NotSupported`。
///
/// ## `Any` 超 trait
///
/// `File: Any` 允许调用方通过 `downcast_ref` 将 `&dyn File` 还原为
/// 具体类型（如 `&BLOCKDEVFILE`），从而访问块设备指针和分区映射信息。
pub trait File: Send + Sync + Any {
    /// 从当前光标位置读取，读后光标前移 `n` 字节。
    fn read(&self, buf: &mut [u8]) -> Result<usize, VfsFsError>;

    /// 从当前光标位置写入，写后光标前移 `n` 字节。
    fn write(&self, buf: &[u8]) -> Result<usize, VfsFsError>;

    /// 从指定偏移读取，不影响光标。
    ///
    /// 默认返回 `NotSupported`；支持随机访问的文件必须覆盖此方法。
    fn read_at(&self, _offset: usize, _buf: &mut [u8]) -> Result<usize, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    /// 从指定偏移写入，不影响光标。
    ///
    /// 默认返回 `NotSupported`；支持随机访问的文件必须覆盖此方法。
    fn write_at(&self, _offset: usize, _buf: &[u8]) -> Result<usize, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    /// 调整文件光标位置。
    ///
    /// `whence`: 0=SEEK_SET, 1=SEEK_CUR, 2=SEEK_END。
    /// 返回新的光标位置（字节偏移）。
    ///
    /// 默认返回 `NotSupported`。
    fn lseek(&self, _offset: isize, _whence: usize) -> Result<usize, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    /// 读取目录项，返回一段或多段 `LinuxDirent64` 序列。
    ///
    /// 非目录文件应返回 `NotSupported`。
    fn getdents64(&self, _max_len: usize) -> Result<Vec<u8>, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    /// 获取文件属性（大小、类型）。
    fn stat(&self) -> Result<VfsStat, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    /// 将缓冲数据写回到存储介质。
    ///
    /// 默认空操作；有缓存的文件系统可覆盖此方法。
    fn flush(&self) -> Result<(), VfsFsError> {
        Ok(())
    }

    /// 将 `&self` 转为 `&dyn Any`，用于向下转型到具体文件类型。
    ///
    /// 仅在 `Self: Sized` 时可用——即只能对具体类型调用，
    /// 不能对 `dyn File` 直接调用。
    fn as_any(&self) -> &dyn Any
    where
        Self: Sized,
    {
        self
    }
}

// ── kstat ─────────────────────────────────────────────────────

/// Linux `struct kstat` 的兼容布局。
///
/// 由 `VfsStat` 转换而来，补零填充当前未实现的 POSIX 字段
/// （uid/gid、时间戳等），使系统调用返回结构在二进制层面与
/// Linux 用户态程序兼容。
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct KStat {
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_mode: u32,
    pub st_nlink: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub st_rdev: u64,
    pub __pad: u64,
    pub st_size: i64,
    pub st_blksize: u32,
    pub __pad2: i32,
    pub st_blocks: u64,
    pub st_atime_sec: i64,
    pub st_atime_nsec: i64,
    pub st_mtime_sec: i64,
    pub st_mtime_nsec: i64,
    pub st_ctime_sec: i64,
    pub st_ctime_nsec: i64,
    pub __unused: [u32; 2],
}

impl From<VfsStat> for KStat {
    /// 从内部 `VfsStat` 构造 Linux 兼容的 `KStat`。
    ///
    /// 不支持的字段（uid/gid、时间戳等）全部填零；
    /// `st_blocks` 按 512 字节为单位计算扇区数。
    fn from(v: VfsStat) -> Self {
        let size_i64 = core::cmp::min(v.size, i64::MAX as u64) as i64;
        let blocks = v.size.div_ceil(512);
        Self {
            st_dev: 0,
            st_ino: v.inode as u64,
            st_mode: v.mode,
            st_nlink: 1,
            st_uid: 0,
            st_gid: 0,
            st_rdev: 0,
            __pad: 0,
            st_size: size_i64,
            st_blksize: 4096,
            __pad2: 0,
            st_blocks: blocks,
            st_atime_sec: 0,
            st_atime_nsec: 0,
            st_mtime_sec: 0,
            st_mtime_nsec: 0,
            st_ctime_sec: 0,
            st_ctime_nsec: 0,
            __unused: [0, 0],
        }
    }
}

// ── VfsFs trait ────────────────────────────────────────────────

/// 文件系统实例 trait。
///
/// 每种可挂载的文件系统（ramfs、ext4、fat32）都提供一个实现了此 trait
/// 的类型实例。挂载点表 `RootFs::mount_poinr` 将路径映射到 `MountFs`
/// （即 `Arc<Mutex<dyn VfsFs>>`）。
///
/// ## 设计约束
///
/// - 所有路径参数均为**挂载点内的相对路径**（VFS 层已做前缀剥离）。
/// - `&mut self` 方法隐含调用方持有 `Mutex` 锁，保证单线程访问。
/// - 默认实现返回 `NotSupported`；文件系统只覆盖自身支持的操作。
pub trait VfsFs: Send + Sync {
    /// 挂载此文件系统实例。
    ///
    /// 通常在 `MountFs` 被插入挂载点表后立即调用；
    /// 文件系统可在此完成超级块读取、位图初始化等一次性工作。
    fn mount(&mut self) -> Result<(), VfsFsError>;

    /// 卸载此文件系统实例。
    fn umount(&mut self) -> Result<(), VfsFsError>;

    /// 返回文件系统类型名（如 `"ext4"`、`"ramfs"`）。
    fn name(&self) -> Result<String, VfsFsError>;

    /// 创建目录。
    fn mkdir(&mut self, _path: &str) -> Result<(), VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    /// 创建普通文件。
    fn mkfile(&mut self, _path: &str) -> Result<(), VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    /// 移动文件/目录（跨目录）。
    fn mv(&mut self, _src: &str, _dest: &str) -> Result<(), VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    /// 重命名（同目录内改名）。
    fn rename(&mut self, _path: &str, _new_name: &str) -> Result<(), VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    /// 打开路径，返回文件句柄。
    ///
    /// `mount_fs`: 自身的 `MountFs` 克隆，供返回的 `File` 在后续
    /// 操作中访问文件系统内部结构（例如 ext4 inode 的读写需要
    /// 持有文件系统引用）。
    fn open(
        &mut self,
        _mount_fs: MountFs,
        _path: &str,
        _flags: OpenFlags,
    ) -> Result<Arc<dyn File>, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    /// 截断文件至指定大小。
    fn truncate(&mut self, _path: &str, _size: u64) -> Result<(), VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    /// 删除文件。
    fn unlink(&mut self, _path: &str) -> Result<(), VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    /// 获取文件属性。
    fn stat(&mut self, _path: &str) -> Result<VfsStat, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    /// 将 `&self` 转为 `&dyn Any`，用于向下转型到具体文件系统类型
    /// （例如从 `dyn VfsFs` 转回 `&RamFs`）。
    fn as_any(&self) -> &dyn Any;

    /// 可变引用版本的向下转型。
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
