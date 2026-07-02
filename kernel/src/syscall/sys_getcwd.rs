//! sys_getcwd — 读取当前工作目录。
//!
//! ## 作用
//! 读取当前工作目录。
//!
//! ## 参数
//! `user_buf_ptr` 用户缓冲区；`buf_len` 缓冲区长度。
//!
//! ## 注意事项
//! 缓冲区不足时当前返回 0，未精确返回 -ERANGE。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/d_path.c:412
//!
//! ## 实现情况
//! 降级实现。

use crate::syscall::syscall::*;
use alloc::vec::Vec;

pub fn sys_getcwd(user_buf_ptr: usize, buf_len: usize) -> isize {
    if user_buf_ptr == 0 || buf_len == 0 {
        return 0;
    }

    let cwd = TASK_MANAER.get_current_cwd();
    let mut tmp: Vec<u8> = Vec::new();
    tmp.extend_from_slice(cwd.as_bytes());
    tmp.push(0);

    if tmp.len() > buf_len {
        return 0;
    }

    let user_satp = TASK_MANAER.get_current_stap();
    let mut slices =
        PageTable::get_mut_slice_from_satp(user_satp, tmp.len(), VirAddr(user_buf_ptr));
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
        return 0;
    }
    user_buf_ptr as isize
}
