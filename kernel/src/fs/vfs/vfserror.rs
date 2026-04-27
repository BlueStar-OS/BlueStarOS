//! VFS 统一错误类型。
//!
//! 所有 VFS 操作（文件读写、挂载、目录遍历等）共用这一个错误枚举，
//! 避免各实现模块各自定义错误类型导致接口碎片化。

use core::fmt::{Display, Formatter, Result};

/// VFS 层统一错误码。
///
/// 设计意图：不区分"哪个文件系统出错"——调用方只关心操作是否成功，
/// 失败时通过 Display 输出可读信息即可。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsFsError {
    /// 文件系统已挂载，重复挂载被拒绝
    Mounted,
    /// 文件系统已卸载，后续操作无效
    Unmounted,
    /// 底层 I/O 失败（磁盘读写错误、设备无响应等）
    IO,
    /// 管道写端已关闭，继续写入被拒绝
    BrokenPipe,
    /// 挂载操作失败
    MountFail,
    /// 卸载操作失败（设备正忙等原因）
    UnmountFail,
    /// 路径或文件不存在
    NotFound,
    /// 文件或目录已存在，创建操作被拒绝
    AlreadyExists,
    /// 路径中的某一级不是目录（例如在文件上做路径展开）
    NotDir,
    /// 操作目标是目录，但需要的是普通文件
    IsDir,
    /// 参数无效（非法 whence、负数偏移等）
    Invalid,
    /// 文件描述符无效或已关闭
    BadFd,
    /// 权限不足
    PermissionDenied,
    /// 操作不被该文件系统或文件类型支持
    NotSupported,
    /// 资源正忙（设备被占用、挂载点正在使用等）
    Busy,
    /// 磁盘空间不足
    NoSpace,
    /// 设备不存在或未注册
    NoDevice,
}

impl Display for VfsFsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Self::Mounted => write!(f, "Mounted"),
            Self::Unmounted => write!(f, "Unmounted"),
            Self::IO => write!(f, "IO"),
            Self::BrokenPipe => write!(f, "BrokenPipe"),
            Self::MountFail => write!(f, "MountFail"),
            Self::UnmountFail => write!(f, "UnmountFail"),
            Self::NotFound => write!(f, "NotFound"),
            Self::AlreadyExists => write!(f, "AlreadyExists"),
            Self::NotDir => write!(f, "NotDir"),
            Self::IsDir => write!(f, "IsDir"),
            Self::Invalid => write!(f, "Invalid"),
            Self::BadFd => write!(f, "BadFd"),
            Self::PermissionDenied => write!(f, "PermissionDenied"),
            Self::NotSupported => write!(f, "NotSupported"),
            Self::Busy => write!(f, "Busy"),
            Self::NoSpace => write!(f, "NoSpace"),
            Self::NoDevice => write!(f, "NoDevice"),
        }
    }
}

use core::error::Error;
impl Error for VfsFsError {}
