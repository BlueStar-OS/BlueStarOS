//! `mkdirat` 系统调用。
//!
//! ## 参数
//! `dirfd` 为起始目录 fd，`path_ptr` 为用户态路径指针，`mode` 为权限位。
//!
//! 当前实现暂未解释 `dirfd` 与权限位，路径创建由 VFS 处理。
//! 行为参考 Linux `fs/namei.c` 中的目录创建路径。

use crate::fs::vfs::vfs_mkdir;
use crate::syscall::syscall::*;

pub fn sys_mkdirat(dirfd: isize, path_ptr: usize, _mode: usize) -> isize {
    // 当前 VFS 仍按绝对/当前工作目录路径工作，因此暂时忽略 dirfd 和 mode。
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
