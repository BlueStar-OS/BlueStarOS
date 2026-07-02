//! sys_rt_sigprocmask — 检查或更改当前线程信号掩码。
//!
//! ## 作用
//! 按 SIG_BLOCK/SIG_UNBLOCK/SIG_SETMASK 查询或更新当前线程的阻塞信号集合。
//!
//! ## 参数
//! `how` 为掩码操作；`nset` 为新掩码用户指针；`oset` 为旧掩码输出指针；`sigsetsize` 为用户 sigset_t 大小。
//!
//! ## 注意事项
//! 当前信号掩码未真正接入调度/递送路径，只能显式返回空掩码，避免静默伪装完整 Linux 语义。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: kernel/signal.c:3318。
//!
//! ## 实现情况
//! 已实现 sigsetsize/how 校验和 oset 写回；TODO: 依赖 per-thread blocked mask、pending signal 队列和 signal delivery 临界区。
//!
//! # 处理流程
//! 1. 校验 sigsetsize == sizeof(sigset_t)
//! 2. 当前实现：不实际维护信号掩码，始终返回空掩码（0）
//! 3. 若 oset != NULL，写入 0（表示无信号被阻塞）
//! 4. 若 nset != NULL，忽略（当前不做实际阻塞）
//! 5. 返回 0 表示成功
use crate::arch::memory::*;
use crate::task::TASK_MANAER;
use core::mem::size_of;
use log::warn;

/// 将字节切片写入用户态内存（当前任务页表）
fn write_to_user(dst: usize, src: &[u8]) -> bool {
    let satp = TASK_MANAER.get_current_stap();
    let mut pt = PageTable::crate_table_from_satp(satp);
    for (i, byte) in src.iter().enumerate() {
        let vaddr = VirAddr(dst.wrapping_add(i));
        let Some(paddr) = pt.translate(vaddr) else {
            return false;
        };
        // SAFETY: paddr 来自当前任务页表对单字节用户地址的成功翻译；
        // 写入粒度为 u8，不跨越本次已校验地址。
        unsafe {
            *(paddr.0 as *mut u8) = *byte;
        }
    }
    true
}

/// rt_sigprocmask 实现
///
/// 参数顺序符合 Linux riscv64 ABI：
///   a0 = how, a1 = nset, a2 = oset, a3 = sigsetsize
pub fn sys_rt_sigprocmask(how: i32, nset: usize, oset: usize, sigsetsize: usize) -> isize {
    // sigsetsize 校验（Linux kernel/signal.c:3022-3023）
    if sigsetsize != size_of::<usize>() {
        return -crate::error::BlueErr::EINVAL.as_isize();
    }

    // how 参数校验（include/uapi/asm-generic/signal-defs.h:7-15）
    if how < 0 || how > 2 {
        return -crate::error::BlueErr::EINVAL.as_isize();
    }

    // 如果调用者要求获取旧掩码，返回 0（表示无信号被阻塞）
    if oset != 0 {
        let empty_mask: usize = 0;
        // SAFETY: empty_mask 是当前栈上的有效 usize，对其只读地重解释为连续字节用于 copy_to_user。
        let slice = unsafe {
            core::slice::from_raw_parts(
                &empty_mask as *const usize as *const u8,
                size_of::<usize>(),
            )
        };
        if !write_to_user(oset, slice) {
            return -crate::error::BlueErr::EFAULT.as_isize();
        }
    }

    // 如果有新的 nset，记录日志（当前不实际阻塞信号）
    if nset != 0 {
        let how_str = match how {
            0 => "SIG_BLOCK",
            1 => "SIG_UNBLOCK",
            2 => "SIG_SETMASK",
            _ => "UNKNOWN",
        };
        warn!(
            "sys_rt_sigprocmask: how={} ({}), ignoring (no signal support yet)",
            how, how_str
        );
    }

    0
}
