//! sys_stat — 按路径读取文件状态。
//!
//! ## 作用
//! 按路径读取文件状态。
//!
//! ## 参数
//! `path_ptr` 路径；`stat_buf_ptr` 用户 KStat 缓冲区。
//!
//! ## 注意事项
//! 当前服务 newfstatat 降级路径，忽略 dirfd/flags。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/stat.c:536 (newfstatat)
//!
//! ## 实现情况
//! 已实现基础路径。

use crate::fs::vfs::{vfs_stat, KStat};
use crate::syscall::syscall::*;

pub fn sys_stat(path_ptr: usize, stat_buf_ptr: usize) -> isize {
    let path = match read_c_string_from_user(path_ptr) {
        Ok(p) => p,
        Err(e) => {
            error!("sys_stat: invalid user path ptr={:#x}, err={}", path_ptr, e);
            return BlueErr::EFAULT.as_isize();
        }
    };

    let st = match vfs_stat(&path) {
        Ok(s) => s,
        Err(e) => {
            error!("sys_stat: vfs_stat failed: path={} err={}", path, e);
            return BlueErr::ENOENT.as_isize();
        }
    };

    if stat_buf_ptr == 0 {
        error!("sys_stat: null stat_buf_ptr for path={}", path);
        return BlueErr::EFAULT.as_isize();
    }

    let kst: KStat = st.into();

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
            "sys_stat: short copy to user: path={} copied={} need={}",
            path,
            off,
            bytes.len()
        );
        return BlueErr::EFAULT.as_isize();
    }
    0
}
