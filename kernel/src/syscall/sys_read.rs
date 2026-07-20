//! sys_read — 从 fd 读取到用户缓冲区。
//!
//! ## 作用
//! 从 fd 读取到用户缓冲区。
//!
//! ## 参数
//! `fd_target` 文件描述符；`source_buffer` 用户缓冲区；`buffer_len` 长度。
//!
//! ## 注意事项
//! 会分配临时 Vec；热路径后续需对象池优化。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/read_write.c:722
//!
//! ## 实现情况
//! 已实现基础路径。

use crate::syscall::syscall::*;
use alloc::vec;

pub fn sys_read(fd_target: usize, source_buffer: usize, buffer_len: usize) -> isize {
    // 获取当前任务的页表进行地址转换
    let user_satp = TASK_MANAER.get_current_stap();
    let mut buffer =
        PageTable::get_mut_slice_from_satp(user_satp, buffer_len, VirAddr(source_buffer));

    // 计算总缓冲区大小
    let total_len: usize = buffer.iter().map(|slic| slic.len()).sum();
    let mut read_buffer = vec![0u8; total_len];

    let fd = match TASK_MANAER.get_current_fd(fd_target) {
        Some(Some(fd)) => fd,
        _ => {
            warn!("sys_read: invalid fd={} len={}", fd_target, buffer_len);
            return BlueErr::EBADF.as_isize();
        }
    };

    let read_len = match fd.read(&mut read_buffer) {
        Ok(len) => len,
        Err(e) => {
            error!(
                "sys_read: fd.read failed fd={} len={} err={}",
                fd_target, buffer_len, e
            );
            return BlueErr::EIO.as_isize();
        }
    };

    let mut offset = 0usize;
    for slice in buffer.iter_mut() {
        if offset >= read_len {
            break;
        }
        let n = core::cmp::min(slice.len(), read_len - offset);
        slice[..n].copy_from_slice(&read_buffer[offset..offset + n]);
        offset += n;
    }

    // 验证写入后的结果
    if read_len > 0 {
        let mut pt = PageTable::crate_table_from_satp(user_satp);
        match pt.translate(VirAddr(source_buffer)) {
            Some(pa) => {
                let _val = unsafe { *(pa.0 as *const u8) };
            }
            None => {
                error!(
                    "sys_read: TRANSLATE FAILED after write! buf=0x{:x}",
                    source_buffer,
                );
            }
        }
    }

    read_len as isize
}
