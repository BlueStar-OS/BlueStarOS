//! sys_mkdirat — 创建目录。
//!
//! ## 作用
//! 创建目录。
//!
//! ## 参数
//! `dirfd` 起始目录 fd；`path_ptr` 路径；`mode` 权限。
//!
//! ## 注意事项
//! dirfd/mode 当前忽略，权限位未落地。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/namei.c:4501
//!
//! ## 实现情况
//! 降级实现。

use crate::fs::vfs::vfs_mkdir;
use crate::syscall::syscall::*;

pub fn sys_mkdirat(dirfd: isize, path_ptr: usize, _mode: usize) -> isize {
    // NOTE: oscomp uses mkdir() implemented via mkdirat(AT_FDCWD,...,mode).
    // We currently ignore dirfd/mode and rely on VFS/ext4 without permission bits.
    let _ = dirfd;
    let path = match read_c_string_from_user(path_ptr) {
        Ok(p) => p,
        Err(e) => {
            error!(
                "sys_mkdir: invalid user path ptr={:#x}, err={}",
                path_ptr, e
            );
            return BlueErr::EFAULT.as_isize();
        }
    };
    match vfs_mkdir(&path) {
        Ok(_) => 0,
        Err(e) => {
            error!("sys_mkdir: vfs_mkdir failed: path={} err={}", path, e);
            BlueErr::EIO.as_isize()
        }
    }
}
