//! sys_lseek — 调整文件偏移。
//!
//! ## 作用
//! 调整文件偏移。
//!
//! ## 参数
//! `fd` 文件描述符；`offset` 偏移；`whence` 基准。
//!
//! ## 注意事项
//! whence 语义由 File::lseek 实现。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/read_write.c:410
//!
//! ## 实现情况
//! 已实现。

use crate::syscall::syscall::*;

pub fn sys_lseek(fd: usize, offset: isize, whence: usize) -> isize {
    let file = match TASK_MANAER.get_current_fd(fd) {
        Some(Some(f)) => f,
        _ => {
            warn!(
                "sys_lseek: invalid fd={} offset={} whence={}",
                fd, offset, whence
            );
            return BlueErr::EBADF.as_isize();
        }
    };
    match file.lseek(offset, whence) {
        Ok(off) => off as isize,
        Err(e) => {
            error!(
                "sys_lseek: failed fd={} offset={} whence={} err={}",
                fd, offset, whence, e
            );
            BlueErr::EINVAL.as_isize()
        }
    }
}
