//! sys_times — 读取进程 times 计数。
//!
//! ## 作用
//! 读取进程 times 计数。
//!
//! ## 参数
//! `tms_ptr` tms 写回地址。
//!
//! ## 注意事项
//! 当前用户/内核/子进程时间均用系统 tick 近似。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: kernel/sys.c:1064
//!
//! ## 实现情况
//! 降级实现。

use crate::syscall::syscall::*;
use crate::time::get_time_tick;

pub fn sys_times(tms_ptr: usize) -> isize {
    // 返回从系统启动至今所经过的时钟滴答数
    if tms_ptr == 0 {
        return BlueErr::EFAULT.as_isize();
    }
    let time_tick = get_time_tick(); // 系统tick数
    let satp = TASK_MANAER.get_current_stap();
    let mut tb = PageTable::crate_table_from_satp(satp);
    let phyaddr = tb.translate(VirAddr(tms_ptr));
    if phyaddr.is_none() {
        error!("[sys_gettimeofday]: invalid addr!");
        return BlueErr::EFAULT.as_isize();
    }
    let tms_st = Tms {
        tms_stime: time_tick,
        tms_utime: time_tick,
        tms_cutime: time_tick,
        tms_cstime: time_tick,
    };
    unsafe {
        *(phyaddr.unwrap().0 as *mut Tms) = tms_st;
    }
    time_tick as isize
}
