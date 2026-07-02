//! sys_write — 从用户缓冲区写入 fd。
//!
//! ## 作用
//! 从用户缓冲区写入 fd。
//!
//! ## 参数
//! `fd_target` 文件描述符；`source_buffer` 用户缓冲区；`buffer_len` 长度。
//!
//! ## 注意事项
//! 会把用户数据拷贝到 Vec，热路径后续需对象池优化。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/read_write.c:746
//!
//! ## 实现情况
//! 已实现基础路径。

use crate::syscall::syscall::*;
use alloc::vec::Vec;

pub fn sys_write(fd_target: usize, source_buffer: usize, buffer_len: usize) -> isize {
    // 获取当前任务的页表进行地址转换
    let user_satp = TASK_MANAER.get_current_stap();
    let buffer = PageTable::get_mut_slice_from_satp(user_satp, buffer_len, VirAddr(source_buffer));

    // 计算总长度并准备写入缓冲区
    let total_len: usize = buffer.iter().map(|slic| slic.len()).sum();
    let mut write_buffer = Vec::with_capacity(total_len);

    // 将用户空间的数据复制到内核缓冲区
    for slice in buffer {
        write_buffer.extend_from_slice(slice);
    }

    let fd = match TASK_MANAER.get_current_fd(fd_target) {
        Some(Some(fd)) => fd,
        _ => {
            warn!("sys_write: invalid fd={} len={}", fd_target, buffer_len);
            return BlueErr::EBADF.as_isize();
        }
    };

    match fd.write(&write_buffer) {
        Ok(written) => written as isize,
        Err(e) => {
            error!(
                "sys_write: fd.write failed fd={} req_len={} copied_len={}  err={}",
                fd_target,
                buffer_len,
                write_buffer.len(),
                e
            );
            BlueErr::EIO.as_isize()
        }
    }
}
