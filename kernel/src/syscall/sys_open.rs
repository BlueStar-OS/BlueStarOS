//! sys_open — 打开路径并分配 fd。
//!
//! ## 作用
//! 打开路径并分配 fd。
//!
//! ## 参数
//! `path_ptr` 路径；`flags_bits` open flags。
//!
//! ## 注意事项
//! 当前由 openat 调度降级而来，忽略 dirfd/mode。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/open.c:1463 (openat)
//!
//! ## 实现情况
//! 已实现基础路径。

use crate::fs::vfs::{vfs_open, OpenFlags};
use crate::syscall::syscall::*;

pub fn sys_open(path_ptr: usize, flags_bits: usize) -> isize {
    let path = match read_c_string_from_user(path_ptr) {
        Ok(p) => p,
        Err(e) => {
            error!("sys_open: invalid user path ptr={:#x}, err={}", path_ptr, e);
            return BlueErr::EFAULT.as_isize();
        }
    };

    let acc = flags_bits & OpenFlags::ACCMODE_MASK;
    if acc > 2 {
        error!(
            "sys_open: invalid acc bits: path={} flags_bits={:#x}",
            path, flags_bits
        );
        return BlueErr::EINVAL.as_isize();
    }
    let flags = OpenFlags::from_bits_truncate(flags_bits);

    let opened = match vfs_open(&path, flags) {
        Ok(r) => r,
        Err(e) => {
            error!(
                "sys_open: vfs_open failed: path={} flags_bits={:#x} err={}",
                path, flags_bits, e
            );
            return BlueErr::ENOENT.as_isize();
        }
    };
    let fd = TASK_MANAER.alloc_fd_for_current(opened);
    if fd < 0 {
        error!(
            "sys_open: alloc fd failed: path={} flags_bits={:#x}",
            path, flags_bits
        );
    }
    fd as isize
}
