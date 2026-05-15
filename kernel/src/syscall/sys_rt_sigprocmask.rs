/// rt_sigprocmask 系统调用 —— 检查/更改信号掩码
///
/// # Linux 参考
/// - 函数原型：kernel/signal.c:3015  SYSCALL_DEFINE4(rt_sigprocmask, int, how,
///                     sigset_t __user *, nset,
///                     sigset_t __user *, oset,
///                     size_t, sigsetsize)
/// - how 参数：include/uapi/asm-generic/signal-defs.h:7-15
///   - SIG_BLOCK   = 0  （将 nset 中的信号加入当前阻塞集）
///   - SIG_UNBLOCK = 1  （将 nset 中的信号从阻塞集移除）
///   - SIG_SETMASK = 2  （用 nset 完全替换当前阻塞集）
/// - sigset_t：include/uapi/asm-generic/signal.h:90-92
///   riscv64 上 _NSIG_WORDS=1, sigset_t = unsigned long = 8 字节
///
/// # 输入
/// - `how`        : 操作方式（SIG_BLOCK=0, SIG_UNBLOCK=1, SIG_SETMASK=2）
/// - `nset`       : 新的信号掩码（用户态指针，可为 NULL）
/// - `oset`       : 输出旧的信号掩码（用户态指针，可为 NULL）
/// - `sigsetsize` : sigset_t 大小校验（必须等于 sizeof(sigset_t) = 8）
///
/// # 处理流程
/// 1. 校验 sigsetsize == sizeof(sigset_t)
/// 2. 当前实现：不实际维护信号掩码，始终返回空掩码（0）
/// 3. 若 oset != NULL，写入 0（表示无信号被阻塞）
/// 4. 若 nset != NULL，忽略（当前不做实际阻塞）
/// 5. 返回 0 表示成功
///
/// # 返回
/// -  0 : 成功
/// - <0 : 错误码（-EINVAL, -EFAULT）
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
