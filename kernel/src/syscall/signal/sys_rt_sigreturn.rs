//! sys_rt_sigreturn — 从信号处理函数返回 (signal trampoline)。
//!
//! ## 作用
//! 从用户态信号 trampoline 返回，恢复信号递送前保存的用户寄存器和信号掩码。
//!
//! ## 参数
//! 无普通 ABI 参数；恢复所需上下文来自当前用户栈上的 RISC-V rt signal frame。
//!
//! ## 注意事项
//! 当前仍是占位实现；错误恢复会导致用户态 PC/SP/信号掩码错乱，必须等信号栈帧布局稳定后实现。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: arch/riscv/kernel/signal.c:233。
//!
//! ## 实现情况
//! 未实现，保留显式 TODO/占位；TODO: 依赖完整 signal frame、trap context 恢复和 rt_sigprocmask 原子语义。
//!
//! 对标 Linux:
//! - `__NR_rt_sigreturn` = 139 (include/uapi/asm-generic/unistd.h)
//! - 内核入口: `sys_rt_sigreturn` (arch/riscv/kernel/signal.c)
//!
//! ## Linux riscv64 ABI
//!
//! 无常规参数。寄存器上下文由信号处理函数的 trampoline 代码设置:
//!
//! - a7 = 139 (NR_rt_sigreturn)
//! - a0 = 信号栈帧地址 (指向保存的寄存器上下文)
//!
//! ## 返回值
//!
//! 正常情况下不返回到调用点，而是恢复到信号发生前的用户态上下文。
//! 如果返回 (异常路径)，返回值为 -EFAULT。
//!
//! ## 信号栈帧布局 (RISC-V Linux)
//!
//! ```text
//! ┌──────────────────────────┐  ← sigreturn 后的 sp
//! │ saved registers (gp..x31)│
//! │ saved pc                  │  ← 信号发生时的用户态 PC
//! │ saved sp                  │  ← 信号发生时的用户态 SP
//! │ saved signal mask         │  ← rt_sigprocmask 恢复用
//! │ ucontext_t header         │
//! │ siginfo_t                 │  (SA_SIGINFO 时)
//! └──────────────────────────┘
//! ```
//!
//! 参考: arch/riscv/kernel/signal.c:restore_sigcontext
//!
//! ## 完整信号递送流程
//!
//! ```text
//! 用户态执行
//!   │
//!   ├── 内核捕获信号 (do_signal → handle_signal)
//!   │     ├── 在用户栈上构造 sigframe
//!   │     ├── 修改用户态 PC → signal handler
//!   │     └── 修改用户态 a0 → siginfo_t* (SA_SIGINFO)
//!   │
//!   ├── 用户态执行 signal handler
//!   │
//!   └── handler 调用 rt_sigreturn (a7=139)
//!         ├── 内核从 sigframe 恢复寄存器
//!         ├── 恢复信号掩码 (rt_sigprocmask)
//!         └── 返回到信号发生前的 PC 继续执行
//! ```
//!
//! ## 关键约束
//!
//! - `rt_sigreturn` 必须在信号处理函数的上下文中调用
//! - 用户库 (glibc musl) 的 signal trampoline 会自动调用它
//! - 恢复的信号掩码需要原子操作: 从 sigframe 中读取旧掩码后，
//!   调用 `rt_sigprocmask(SIG_SETMASK, &oldmask, NULL)`
//!
//! ## musl libc 的 trampoline (arch/riscv64)
//!
//! ```text
//! __restore_rt:
//!     li a7, 139     // __NR_rt_sigreturn
//!     ecall
//! ```
//!
//! 参考: arch/riscv/kernel/signal.c:sys_rt_sigreturn
//! 参考: include/uapi/asm-generic/signal.h

use crate::error::BlueErr;

/// sys_rt_sigreturn() -> (不正常返回)
///
/// 从信号处理函数返回，恢复被中断的用户态上下文。
///
/// TODO: 用户自行实现
pub fn sys_rt_sigreturn() -> isize {
    // TODO: 实现步骤
    // 1. 从当前任务的 trap context 获取信号栈帧指针
    // 2. 从栈帧中恢复:
    //    - 通用寄存器 (x1-x31)
    //    - PC (epc)
    //    - SP
    //    - 信号掩码 (saved_sigmask)
    // 3. 调用 rt_sigprocmask(SIG_SETMASK, &saved_sigmask, NULL) 恢复掩码
    // 4. 清除当前任务的 "in signal handler" 标志
    // 5. 通过修改 trap context 的 a0 设置返回值 (通常为信号发生前的 a0)
    // 6. 正常路径不走到这里: 内核在 sigreturn 内直接完成上下文切换

    unimplemented!("sys_rt_sigreturn: user TODO")
}
