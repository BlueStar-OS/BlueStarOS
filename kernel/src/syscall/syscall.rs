//! syscall 共享工具与用户内存辅助函数。
//!
//! 本文件只保存多个 syscall 共同依赖的类型和 helper。具体 syscall 实现必须放在
//! `kernel/src/syscall/sys_*.rs` 中，保持“一 syscall 一文件”。
//!
//! Linux 参考版本: K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)。

use crate::alloc::string::ToString;
pub(crate) use crate::arch::memory::*;
pub(crate) use crate::error::BlueErr;
use crate::fs::vfs::VfsFsError;
pub(crate) use crate::task::TASK_MANAER;
use alloc::string::String;
use alloc::vec::Vec;
pub(crate) use log::{debug, error, warn};

/// Linux `struct new_utsname` 单字段长度。
///
/// 参考: K3 Linux 6.18.3 `include/uapi/linux/utsname.h`。
pub(crate) const UTNAME_FIELD_LEN: usize = 65;

/// Linux `struct __kernel_timespec` 兼容布局。
///
/// 参考: K3 Linux 6.18.3 `include/uapi/linux/time_types.h`。
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Timespec {
    pub(crate) tv_sec: i64,
    pub(crate) tv_nsec: i64,
}

/// Linux `struct iovec` 兼容布局，用于 readv/writev。
///
/// 参考: K3 Linux 6.18.3 `include/uapi/linux/uio.h`。
#[repr(C)]
pub(crate) struct UserIovec {
    pub(crate) iovec_base: usize,
    pub(crate) iovec_len: usize,
}

/// Linux `struct tms` 兼容布局，用于 times。
///
/// 参考: K3 Linux 6.18.3 `include/uapi/linux/times.h`。
#[repr(C)]
pub(crate) struct Tms {
    pub(crate) tms_utime: usize,
    pub(crate) tms_stime: usize,
    pub(crate) tms_cutime: usize,
    pub(crate) tms_cstime: usize,
}

/// Linux `struct new_utsname` 兼容布局，用于 uname。
///
/// 参考: K3 Linux 6.18.3 `include/uapi/linux/utsname.h`。
#[repr(C)]
#[derive(Debug)]
pub(crate) struct UtsName {
    pub(crate) sysname: [u8; UTNAME_FIELD_LEN],
    pub(crate) nodename: [u8; UTNAME_FIELD_LEN],
    pub(crate) release: [u8; UTNAME_FIELD_LEN],
    pub(crate) version: [u8; UTNAME_FIELD_LEN],
    pub(crate) machine: [u8; UTNAME_FIELD_LEN],
    pub(crate) domainname: [u8; UTNAME_FIELD_LEN],
}

impl UtsName {
    /// 构造全零 uname 缓冲区，字段稍后由 sys_uname 写入 NUL 结尾字符串。
    pub(crate) fn new() -> Self {
        Self {
            sysname: [0; UTNAME_FIELD_LEN],
            nodename: [0; UTNAME_FIELD_LEN],
            release: [0; UTNAME_FIELD_LEN],
            version: [0; UTNAME_FIELD_LEN],
            machine: [0; UTNAME_FIELD_LEN],
            domainname: [0; UTNAME_FIELD_LEN],
        }
    }
}

/// 把 VFS 层错误映射为 Linux 风格 errno。
///
/// 参考: K3 Linux 6.18.3 `fs/ioctl.c:583` 及 VFS syscall errno 约定。
pub(crate) fn vfs_error_to_blue_errno(error: VfsFsError) -> BlueErr {
    match error {
        VfsFsError::BadFd => BlueErr::EBADF,
        VfsFsError::Invalid => BlueErr::EINVAL,
        VfsFsError::PermissionDenied => BlueErr::EACCES,
        VfsFsError::NotFound => BlueErr::ENOENT,
        VfsFsError::NotDir => BlueErr::ENOTDIR,
        VfsFsError::IsDir => BlueErr::EISDIR,
        VfsFsError::AlreadyExists => BlueErr::EEXIST,
        VfsFsError::Busy => BlueErr::EBUSY,
        VfsFsError::NoSpace => BlueErr::ENOSPC,
        VfsFsError::BrokenPipe => BlueErr::EPIPE,
        VfsFsError::NoDevice => BlueErr::ENODEV,
        VfsFsError::Mounted
        | VfsFsError::Unmounted
        | VfsFsError::MountFail
        | VfsFsError::UnmountFail => BlueErr::EBUSY,
        VfsFsError::NotSupported => BlueErr::ENOTTY,
        VfsFsError::IO => BlueErr::EIO,
    }
}

/// 从当前任务用户空间读取 NUL 结尾的 C 字符串。
///
/// 输入: `path_ptr` 是当前任务用户虚拟地址；输出: 成功返回 Rust `String`。
/// 副作用: 逐字节翻译当前任务页表，不修改用户内存。
/// 注意: 限制最大 4096 字节，防止无界扫描用户地址空间。
/// 参考: K3 Linux 6.18.3 `fs/open.c:1463` / `fs/exec.c:2005` 等路径型 syscall。
pub(crate) fn read_c_string_from_user(path_ptr: usize) -> Result<String, VfsFsError> {
    let user_satp = TASK_MANAER.get_current_stap();
    read_c_string_from_user_with_satp(user_satp, path_ptr)
}

/// 从指定用户页表读取 NUL 结尾的 C 字符串。
///
/// 输入: `user_satp` 指定页表，`path_ptr` 是该页表下的用户虚拟地址。
/// 输出: 成功返回 UTF-8 字符串；失败返回 VFS Invalid。
/// 副作用: 无；只读用户内存。
/// 注意: 逐字节跨页读取，避免一次性切片在页尾触发内核 fault。
/// 参考: K3 Linux 6.18.3 `fs/exec.c:2005` 的 execve 用户字符串语义。
pub(crate) fn read_c_string_from_user_with_satp(
    user_satp: usize,
    path_ptr: usize,
) -> Result<String, VfsFsError> {
    const MAX_PATH_LEN: usize = 4096;

    let mut table = PageTable::crate_table_from_satp(user_satp);
    let mut data: Vec<u8> = Vec::new();

    for off in 0..MAX_PATH_LEN {
        let vaddr = VirAddr(path_ptr + off);
        let paddr = match table.translate(vaddr) {
            Some(p) => p,
            None => {
                error!(
                    "read_c_string_from_user_with_satp: translate failed: satp={:#x} path_ptr={:#x} off={} vaddr={:#x}",
                    user_satp, path_ptr, off, vaddr.0
                );
                return Err(VfsFsError::Invalid);
            }
        };
        // SAFETY: `paddr` 来自当前用户页表对 `vaddr` 的成功翻译；这里只读 1 字节，
        // 不跨越物理页边界，也不保留悬垂引用。
        let b = unsafe { *(paddr.0 as *const u8) };
        if b == 0 {
            let s = core::str::from_utf8(&data)
                .map_err(|_| VfsFsError::Invalid)?
                .to_string();
            return Ok(s);
        }
        data.push(b);
    }

    error!(
        "read_c_string_from_user_with_satp: no NUL within {} bytes: satp={:#x} path_ptr={:#x}",
        MAX_PATH_LEN, user_satp, path_ptr
    );
    Err(VfsFsError::Invalid)
}
