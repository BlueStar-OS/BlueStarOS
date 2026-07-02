//! sys_gettimeofday — 读取当前 wall-clock 风格时间。
//!
//! ## 作用
//! 读取当前 wall-clock 风格时间。
//!
//! ## 参数
//! `tv_ptr` timeval 写回地址；`tz_ptr` 时区参数。
//!
//! ## 注意事项
//! tz 被忽略；无真实 RTC，使用内核单调毫秒。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: kernel/time/time.c:140
//!
//! ## 实现情况
//! 已实现基础路径。

use crate::syscall::syscall::*;
use crate::time::{get_time_ms, TimeVal};

pub fn sys_gettimeofday(tv_ptr: usize, _tz_ptr: usize) -> isize {
    if tv_ptr == 0 {
        return BlueErr::EFAULT.as_isize();
    }
    let ms = get_time_ms();
    let sec = ms / 1000;
    let usec = (ms % 1000) * 1000;
    let time_val = TimeVal { sec, usec };
    let satp = TASK_MANAER.get_current_stap();
    let mut tb = PageTable::crate_table_from_satp(satp);
    let phyaddr = tb.translate(VirAddr(tv_ptr));
    if phyaddr.is_none() {
        error!("[sys_gettimeofday]: invalid addr!");
        return BlueErr::EFAULT.as_isize();
    }
    unsafe {
        *(phyaddr.unwrap().0 as *mut TimeVal) = time_val;
    }
    0
}
