//! sys_getdents64 — 读取目录项到用户缓冲区。
//!
//! ## 作用
//! 读取目录项到用户缓冲区。
//!
//! ## 参数
//! `fd` 目录 fd；`user_buf_ptr` 用户缓冲区；`len` 长度。
//!
//! ## 注意事项
//! 目录项编码由 VFS 层提供。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/readdir.c:396
//!
//! ## 实现情况
//! 已实现。

use crate::fs::vfs::vfs_getdents64;
use crate::syscall::syscall::*;

pub fn sys_getdents64(fd: usize, user_buf_ptr: usize, len: usize) -> isize {
    if user_buf_ptr == 0 {
        warn!("sys_getdents64: null user_buf_ptr fd={} len={}", fd, len);
        return BlueErr::EFAULT.as_isize();
    }
    let file = match TASK_MANAER.get_current_fd(fd) {
        Some(Some(f)) => f,
        _ => {
            warn!("sys_getdents64: invalid fd={} len={}", fd, len);
            return BlueErr::EBADF.as_isize();
        }
    };

    let data = match vfs_getdents64(&file, len) {
        Ok(v) => v,
        Err(e) => {
            error!(
                "sys_getdents64: vfs_getdents64 failed fd={} len={} err={}",
                fd, len, e
            );
            return BlueErr::EIO.as_isize();
        }
    };

    let user_satp = TASK_MANAER.get_current_stap();
    let mut slices =
        PageTable::get_mut_slice_from_satp(user_satp, data.len(), VirAddr(user_buf_ptr));
    let mut off = 0usize;
    for s in slices.iter_mut() {
        if off >= data.len() {
            break;
        }
        let n = core::cmp::min(s.len(), data.len() - off);
        s[..n].copy_from_slice(&data[off..off + n]);
        off += n;
    }
    if off != data.len() {
        error!(
            "sys_getdents64: short copy to user fd={} copied={} need={}",
            fd,
            off,
            data.len()
        );
        return BlueErr::EFAULT.as_isize();
    }
    data.len() as isize
}
