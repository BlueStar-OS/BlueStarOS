//! sys_writev — 按 iovec 分散缓冲向同一 fd 写入。
//!
//! ## 作用
//! 按 iovec 分散缓冲向同一 fd 写入。
//!
//! ## 参数
//! `fd` 文件描述符；`iov_vec` 用户 iovec 数组；`iov_cnt` 项数。
//!
//! ## 注意事项
//! 逐项复用 sys_write；当前未完整检查 iov_cnt 溢出和原子性。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/read_write.c:1168
//!
//! ## 实现情况
//! 已实现基础路径。

use crate::syscall::sys_write::sys_write;
use crate::syscall::syscall::*;

pub fn sys_writev(fd: i32, iov_vec: usize, iov_cnt: i32) -> isize {
    let iovec_base = core::mem::size_of::<UserIovec>();
    let usize_size = core::mem::size_of::<usize>();
    // 读取iovec写入
    let satp = TASK_MANAER.get_current_stap();
    let mut tb = PageTable::crate_table_from_satp(satp);
    let mut write_re = 0;
    for i in 0..iov_cnt {
        // 处理可能跨页
        let user_base = if let Some(re) =
            tb.read_bytes_from_userspace(VirAddr(iov_vec + i as usize * iovec_base), usize_size)
        {
            usize::from_le_bytes(re.try_into().unwrap())
        } else {
            return BlueErr::EFAULT.as_isize();
        };
        let len = if let Some(re) = tb.read_bytes_from_userspace(
            VirAddr(iov_vec + i as usize * iovec_base + usize_size),
            usize_size,
        ) {
            usize::from_le_bytes(re.try_into().unwrap())
        } else {
            return BlueErr::EFAULT.as_isize();
        };
        let re = sys_write(fd as usize, user_base, len);
        write_re += re;
    }
    write_re
}
