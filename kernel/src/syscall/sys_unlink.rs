//! sys_unlink — 删除路径名。
//!
//! ## 作用
//! 删除路径名。
//!
//! ## 参数
//! `path_ptr` 路径。
//!
//! ## 注意事项
//! 当前由 unlinkat 调度降级而来，未处理 dirfd/flags。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/namei.c:4771 (unlinkat)
//!
//! ## 实现情况
//! 已实现基础路径。

use crate::fs::vfs::vfs_unlink;
use crate::syscall::syscall::*;

pub fn sys_unlink(path_ptr: usize) -> isize {
    let path = match read_c_string_from_user(path_ptr) {
        Ok(p) => p,
        Err(e) => {
            error!(
                "sys_unlink: invalid user path ptr={:#x}, err={}",
                path_ptr, e
            );
            return BlueErr::EFAULT.as_isize();
        }
    };
    match vfs_unlink(&path) {
        Ok(_) => 0,
        Err(e) => {
            error!("sys_unlink: vfs_unlink failed: path={} err={}", path, e);
            BlueErr::EIO.as_isize()
        }
    }
}
