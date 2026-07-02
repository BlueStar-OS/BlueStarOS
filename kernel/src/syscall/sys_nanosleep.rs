//! sys_nanosleep — 让当前任务至少睡眠指定 timespec。
//!
//! ## 作用
//! 让当前任务至少睡眠指定 timespec。
//!
//! ## 参数
//! `req_ptr` 请求 timespec；`rem_ptr` 剩余时间写回地址。
//!
//! ## 注意事项
//! 使用调度让出轮询等待；无高精度 hrtimer/信号中断剩余时间语义。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: kernel/time/hrtimer.c:2184
//!
//! ## 实现情况
//! 已实现基础路径。

use crate::syscall::syscall::*;
use crate::time::get_time_ms;

pub fn sys_nanosleep(req_ptr: usize, rem_ptr: usize) -> isize {
    if req_ptr == 0 {
        return BlueErr::EFAULT.as_isize();
    }

    let user_satp = TASK_MANAER.get_current_stap();
    let mut tb = PageTable::crate_table_from_satp(user_satp);
    let req_pa = tb.translate(VirAddr(req_ptr));
    if req_pa.is_none() {
        return BlueErr::EFAULT.as_isize();
    }
    let req = unsafe { &*(req_pa.unwrap().0 as *const Timespec) };

    if req.tv_sec < 0 || req.tv_nsec < 0 {
        return BlueErr::EINVAL.as_isize();
    }

    let ns_total = (req.tv_sec as i128)
        .saturating_mul(1_000_000_000i128)
        .saturating_add(req.tv_nsec as i128);
    let ms = if ns_total <= 0 {
        0usize
    } else {
        ((ns_total + 999_999i128) / 1_000_000i128) as usize
    };
    let start = get_time_ms();
    let target = start.saturating_add(ms);

    while get_time_ms() < target {
        TASK_MANAER.suspend_and_run_task();
    }

    if rem_ptr != 0 {
        let rem_pa = tb.translate(VirAddr(rem_ptr));
        if let Some(pa) = rem_pa {
            unsafe {
                *(pa.0 as *mut Timespec) = Timespec {
                    tv_sec: 0,
                    tv_nsec: 0,
                };
            }
        }
    }
    0
}
