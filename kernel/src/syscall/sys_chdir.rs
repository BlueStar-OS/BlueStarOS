//! sys_chdir — 切换当前工作目录。
//!
//! ## 作用
//! 切换当前工作目录。
//!
//! ## 参数
//! `path_ptr` 目标路径。
//!
//! ## 注意事项
//! 依赖 VFS stat 判断目录。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/open.c:555
//!
//! ## 实现情况
//! 已实现。

use crate::fs::vfs::{normalize_path, vfs_stat, VFS_DT_DIR};
use crate::syscall::syscall::*;

pub fn sys_chdir(path_ptr: usize) -> isize {
    let path = match read_c_string_from_user(path_ptr) {
        Ok(p) => p,
        Err(e) => {
            error!(
                "sys_chdir: invalid user path ptr={:#x}, err={}",
                path_ptr, e
            );
            return BlueErr::EFAULT.as_isize();
        }
    };

    let abs = match normalize_path(&path) {
        Ok(p) => p,
        Err(_) => return BlueErr::ENOENT.as_isize(),
    };

    let st = match vfs_stat(&abs) {
        Ok(s) => s,
        Err(e) => {
            error!("sys_chdir: vfs_stat failed: path={} err={}", abs, e);
            return BlueErr::ENOENT.as_isize();
        }
    };
    if st.file_type != VFS_DT_DIR {
        return BlueErr::ENOTDIR.as_isize();
    }

    TASK_MANAER.set_current_cwd(abs);
    0
}
