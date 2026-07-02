//! sys_ioctl — 文件描述符控制入口，转发到 File::ioctl。
//!
//! ## 作用
//! 文件描述符控制入口，转发到 File::ioctl。
//!
//! ## 参数
//! `fd` 文件描述符；`cmd` 控制命令；`arg` 用户/整数参数。
//!
//! ## 注意事项
//! 当前只实现 VFS 回调转发；未实现 Linux ioctl 命令全集。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/ioctl.c:583
//!
//! ## 实现情况
//! 已实现最小转发路径。

use crate::fs::vfs::File;
use crate::syscall::syscall::*;

pub fn sys_ioctl(fd: usize, cmd: usize, arg: usize) -> isize {
    let file = match TASK_MANAER.get_current_fd(fd) {
        Some(Some(file)) => file,
        _ => {
            warn!("sys_ioctl: invalid fd={} cmd={:#x} arg={:#x}", fd, cmd, arg);
            return BlueErr::EBADF.as_isize();
        }
    };

    match file.ioctl(cmd as u32, arg) {
        Ok(return_value) => return_value as isize,
        Err(error) => {
            let errno = vfs_error_to_blue_errno(error);
            error!(
                "sys_ioctl: file.ioctl failed fd={} cmd={:#x} arg={:#x} err={} errno={}",
                fd,
                cmd,
                arg,
                error,
                errno.code()
            );
            errno.as_isize()
        }
    }
}
