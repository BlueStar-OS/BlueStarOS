//! sys_fstat — 按 fd 读取文件状态。
//!
//! ## 作用
//! 按 fd 读取文件状态。
//!
//! ## 参数
//! `fd` 文件描述符；`stat_buf_ptr` 用户 KStat 缓冲区。
//!
//! ## 注意事项
//! 依赖 VFS fstat_kstat。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/stat.c:450
//!
//! ## 实现情况
//! 已实现。

use crate::fs::vfs::{vfs_fstat_kstat, KStat};
use crate::syscall::syscall::*;

pub fn sys_fstat(fd: usize, stat_buf_ptr: usize) -> isize {
    if stat_buf_ptr == 0 {
        error!("sys_fstat: null stat_buf_ptr fd={}", fd);
        return BlueErr::EFAULT.as_isize();
    }

    let file = match TASK_MANAER.get_current_fd(fd) {
        Some(Some(f)) => f,
        _ => {
            warn!("sys_fstat: invalid fd={}", fd);
            return BlueErr::EBADF.as_isize();
        }
    };

    let kst: KStat = match vfs_fstat_kstat(&file) {
        Ok(s) => s,
        Err(e) => {
            error!("sys_fstat: vfs_fstat_kstat failed: fd={} err={}", fd, e);
            return BlueErr::EIO.as_isize();
        }
    };

    let user_satp = TASK_MANAER.get_current_stap();
    let mut slices = PageTable::get_mut_slice_from_satp(
        user_satp,
        core::mem::size_of::<KStat>(),
        VirAddr(stat_buf_ptr),
    );

    let bytes: &[u8] = unsafe {
        core::slice::from_raw_parts(
            (&kst as *const KStat) as *const u8,
            core::mem::size_of::<KStat>(),
        )
    };
    let mut off = 0usize;
    for s in slices.iter_mut() {
        if off >= bytes.len() {
            break;
        }
        let n = core::cmp::min(s.len(), bytes.len() - off);
        s[..n].copy_from_slice(&bytes[off..off + n]);
        off += n;
    }
    if off != bytes.len() {
        error!(
            "sys_fstat: short copy to user: fd={} copied={} need={}",
            fd,
            off,
            bytes.len()
        );
        return BlueErr::EFAULT.as_isize();
    }
    0
}
