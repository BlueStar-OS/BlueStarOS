//! sys_pipe — 创建管道并写回两个 fd。
//!
//! ## 作用
//! 创建管道并写回两个 fd。
//!
//! ## 参数
//! `fds_ptr` 用户 int[2] 写回地址。
//!
//! ## 注意事项
//! flags 由 pipe2 调度处忽略；这里只实现无 flags pipe。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/pipe.c:1054
//!
//! ## 实现情况
//! 已实现。

use crate::fs::component::pipe::pipe::{make_pipe, PipeHandle};
use crate::fs::vfs::File;
use crate::syscall::syscall::*;
use alloc::sync::Arc;

pub fn sys_pipe(fds_ptr: usize) -> isize {
    if fds_ptr == 0 {
        return BlueErr::EFAULT.as_isize();
    }

    let (read_end, write_end) = make_pipe();
    let read_fd: Arc<dyn File> = Arc::new(PipeHandle::new(read_end));
    let write_fd: Arc<dyn File> = Arc::new(PipeHandle::new(write_end));

    let rfd: i32 = TASK_MANAER.alloc_fd_for_current(read_fd);
    if rfd < 0 {
        return BlueErr::EMFILE.as_isize();
    }
    let wfd: i32 = TASK_MANAER.alloc_fd_for_current(write_fd);
    if wfd < 0 {
        return BlueErr::EMFILE.as_isize();
    }

    let user_satp = TASK_MANAER.get_current_stap();
    let mut slices = PageTable::get_mut_slice_from_satp(
        user_satp,
        core::mem::size_of::<usize>() * 2,
        VirAddr(fds_ptr),
    );

    let mut tmp: [u8; core::mem::size_of::<i32>() * 2] = [0u8; core::mem::size_of::<i32>() * 2];
    tmp[..core::mem::size_of::<i32>()].copy_from_slice(&rfd.to_ne_bytes());
    tmp[core::mem::size_of::<i32>()..].copy_from_slice(&wfd.to_ne_bytes());

    let mut off = 0usize;
    for s in slices.iter_mut() {
        if off >= tmp.len() {
            break;
        }
        let n = core::cmp::min(s.len(), tmp.len() - off);
        s[..n].copy_from_slice(&tmp[off..off + n]);
        off += n;
    }
    if off != tmp.len() {
        return BlueErr::EFAULT.as_isize();
    }
    0
}
