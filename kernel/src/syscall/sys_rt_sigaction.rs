/// rt_sigaction 系统调用 —— 设置/获取信号处理函数
///
/// # Linux 参考
/// - 函数原型：kernel/signal.c:4233  SYSCALL_DEFINE4(rt_sigaction, int, sig,
///                     const struct sigaction __user *, act,
///                     struct sigaction __user *, oact,
///                     size_t, sigsetsize)
/// - sigaction 结构：include/uapi/asm-generic/signal.h:104
/// - riscv64 信号头：arch/riscv64 使用 generic, 无独立 signal.h
///
/// # 输入
/// - `sig`        : 信号编号 (Linux: 1..64, include/uapi/asm-generic/signal.h:7 _NSIG=64)
/// - `act`        : 新的信号处理配置（用户态指针，可为 NULL）
/// - `oact`       : 输出旧的信号处理配置（用户态指针，可为 NULL）
/// - `sigsetsize` : sigset_t 大小校验（必须等于 sizeof(sigset_t) = 8）
///
/// # 处理流程
/// 1. 校验 sigsetsize == sizeof(sigset_t)（riscv64: 8 字节）
/// 2. 当前实现：返回空 sigaction（全部 SIG_DFL）
/// 3. 若 oact != NULL，写入全零结构（表示全部默认处理）
/// 4. 若 act != NULL，仅记录日志，不做实际动作
/// 5. 返回 0 表示成功
///
/// # 返回
/// -  0 : 成功
/// - <0 : 错误码（-EINVAL, -EFAULT）

use crate::arch::memory::*;
use crate::task::TASK_MANAER;
use core::mem::size_of;
use log::warn;

/// RISC-V 64 sigaction 结构（24 字节）
///
/// Linux include/uapi/asm-generic/signal.h:104
/// RISC-V 不定义 SA_RESTORER（arch/riscv/include/uapi/asm/ 无 signal.h），
/// 所以结构体只有 3 个字段。
///
/// 注意：Linux 内核中 __kernel 分支不包含 sa_restorer（signal.h:107-109 `#ifndef __KERNEL__`），
/// 内核态看到的 sigaction 结构就是这三个字段。
#[repr(C)]
struct SigAction {
    sa_handler: usize,   // +0: 信号处理函数指针 (__sighandler_t, 8 字节)
    sa_flags: usize,     // +8: 标志位 (8 字节)
    sa_mask: usize,      // +16: 信号掩码 (riscv64: _NSIG_WORDS=1, 8 字节)
}

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

/// rt_sigaction 实现
///
/// 参数顺序符合 Linux riscv64 ABI：
///   a0 = sig, a1 = act, a2 = oact, a3 = sigsetsize
pub fn sys_rt_sigaction(
    sig: i32,
    act: usize,
    oact: usize,
    sigsetsize: usize,
) -> isize {
    // sigsetsize 校验（Linux kernel/signal.c:4242-4243）
    if sigsetsize != size_of::<usize>() {
        return -crate::error::BlueErr::EINVAL.as_isize();
    }

    // 校验信号编号范围（_NSIG=64, include/uapi/asm-generic/signal.h:7）
    if sig <= 0 || sig as usize > 64 {
        return -crate::error::BlueErr::EINVAL.as_isize();
    }

    // 如果调用者要求获取旧的 sigaction，返回空结构（表示全部 SIG_DFL）
    if oact != 0 {
        let empty = SigAction {
            sa_handler: 0, // SIG_DFL
            sa_flags: 0,
            sa_mask: 0,
        };
        let slice = unsafe {
            core::slice::from_raw_parts(
                &empty as *const SigAction as *const u8,
                size_of::<SigAction>(),
            )
        };
        if !write_to_user(oact, slice) {
            return -crate::error::BlueErr::EFAULT.as_isize();
        }
    }

    // 如果有新的 act，记录日志（当前不做任何实际处理）
    if act != 0 {
        warn!(
            "sys_rt_sigaction: sig={}, ignoring handler setup (no signal support yet)",
            sig
        );
    }

    0
}
