//! sys_clock_gettime — 读取指定 clock id 的 timespec。
//!
//! ## 作用
//! 读取指定 clock id 的 timespec。
//!
//! ## 参数
//! `clock_id` 时钟类型；`tp_ptr` timespec 写回地址。
//!
//! ## 注意事项
//! 多个 clock id 暂映射到同一单调纳秒源。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: kernel/time/posix-stubs.c:60 / kernel/time/posix-timers.c:1134
//!
//! ## 实现情况
//! 已实现基础路径。

use crate::syscall::syscall::*;
use crate::time::get_time_ns;

pub fn sys_clock_gettime(clock_id: usize, tp_ptr: usize) -> isize {
    if tp_ptr == 0 {
        return BlueErr::EFAULT.as_isize();
    }
    // 参考 K3 Linux 6.18.3: include/uapi/linux/time.h 中的 clock id 约定，
    // 入口见 kernel/time/posix-stubs.c:60 / kernel/time/posix-timers.c:1134。
    let ns = get_time_ns();
    let (sec, nsec) = match clock_id {
        // CLOCK_REALTIME | CLOCK_MONOTONIC | CLOCK_MONOTONIC_RAW
        // | CLOCK_REALTIME_COARSE | CLOCK_MONOTONIC_COARSE | CLOCK_BOOTTIME
        0 | 1 | 4 | 5 | 6 | 7 => (ns / 1_000_000_000, ns % 1_000_000_000),
        _ => return BlueErr::EINVAL.as_isize(),
    };

    let timespec = Timespec {
        tv_sec: sec as i64,
        tv_nsec: nsec as i64,
    };

    let satp = TASK_MANAER.get_current_stap();
    let mut tb = PageTable::crate_table_from_satp(satp);
    let phyaddr = tb.translate(VirAddr(tp_ptr));
    if phyaddr.is_none() {
        error!("[sys_clock_gettime]: invalid addr!");
        return BlueErr::EFAULT.as_isize();
    }
    unsafe {
        *(phyaddr.unwrap().0 as *mut Timespec) = timespec;
    }
    0
}
