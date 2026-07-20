//! sys_gettid — 获取线程 ID (Linux-specific)。
//!
//! ## 作用
//! 返回当前执行线程的内核线程 ID。
//!
//! ## 参数
//! 无参数。
//!
//! ## 注意事项
//! 当前仍是占位实现；在无线程组基础设施时不能把 gettid 与 getpid 的 Linux 强语义混为一谈。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: kernel/sys.c:1005。
//!
//! ## 实现情况
//! 未实现，保留显式 TODO/占位；TODO: 依赖 task pid/tgid 拆分、clone 线程语义和线程组管理。
//!
//! 对标 Linux:
//! - `__NR_gettid` = 178 (include/uapi/asm-generic/unistd.h)
//! - 内核入口: `sys_gettid` (kernel/sys.c:902)
//!
//! ## Linux riscv64 ABI
//!
//! 无参数。
//!
//! ## 返回值
//!
//! - 线程 ID (TID)，在内核中是 `struct task_struct->pid`
//!
//! ## 语义
//!
//! - 在单线程进程中，`gettid() == getpid()`
//! - 在多线程进程中，每个线程有唯一 TID，但共享 PID
//! - TID 由内核在 `clone()` / `clone3()` 时分配
//! - 主线程的 TID 等于进程 PID
//!
//! ## getpid vs gettid
//!
//! | 系统调用 | 返回 | 多线程行为 |
//! |----------|------|------------|
//! | getpid | TGID (线程组 ID) | 所有线程返回相同值 |
//! | gettid | PID (内核 task pid) | 每个线程返回不同值 |
//!
//! 在 Linux 内核中:
//! - `task_struct->pid`: 每个线程唯一 (gettid 返回)
//! - `task_struct->tgid`: 线程组 ID (getpid 返回)
//! - 主线程: `pid == tgid`
//!
//! ## 使用场景
//!
//! - `tgkill(tid, sig)`: 向特定线程发信号
//! - `/proc/[tid]/`: 每个线程有独立 proc 目录
//! - perf/strace: 按线程追踪
//!
//! 参考: POSIX.1-2008 (非 POSIX 标准，Linux-specific)
//! 参考: gettid(2) man page


/// sys_gettid() -> 当前线程 ID
///
/// TODO: 用户自行实现
pub fn sys_gettid() -> isize {
    // TODO: 当前返回 getpid() (单线程，TID == PID)。
    // 多线程支持后应返回 task_struct->pid (内核线程号，非 TGID)。
    // TASK_MANAER.get_current_pid() as isize
    unimplemented!()
}
