//! sys_umask — 设置文件创建掩码 (POSIX umask)。
//!
//! ## 作用
//! 设置当前进程文件创建权限掩码，并返回旧掩码。
//!
//! ## 参数
//! `mask` 为新的权限屏蔽位，只应保留低 9 位传统 Unix 权限位。
//!
//! ## 注意事项
//! 当前返回固定默认值，是显式降级；Linux umask 属于进程凭据/文件系统上下文状态，fork 后应继承。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: kernel/sys.c:1960。
//!
//! ## 实现情况
//! 仅占位返回默认 `0022`；TODO: 依赖 task/cred/fs_struct 中的 per-process umask 存储和 fork/clone 继承语义。
//!
//! 对标 Linux:
//! - `__NR_umask` = 166 (include/uapi/asm-generic/unistd.h)
//! - 内核入口: `sys_umask` (kernel/sys.c:1504)
//!
//! ## Linux riscv64 ABI
//!
//! | 寄存器 | 参数 | 含义 |
//! |--------|------|------|
//! | a0 | mask | 新的 umask 值 (八进制权限位) |
//!
//! ## 返回值
//!
//! - 旧的 umask 值 (始终成功)
//!
//! ## umask 语义
//!
//! umask 是进程级别的权限掩码，在 open/create 文件时从请求权限中
//! 屏蔽掉对应的位:
//!
//! ```text
//! 实际权限 = requested_mode & ~umask
//! ```
//!
//! 例如:
//! - umask = 0022 (默认)
//! - open(..., 0666) → 实际权限 0644 (rw-r--r--)
//! - mkdir(..., 0777) → 实际权限 0755 (rwxr-xr-x)
//!
//! ## 权限位 (include/uapi/stat.h)
//!
//! | 位 | 八进制 | 含义 |
//! |----|--------|------|
//! | S_IRWXU | 0700 | 所有者 rwx |
//! | S_IRUSR | 0400 | 所有者读 |
//! | S_IWUSR | 0200 | 所有者写 |
//! | S_IXUSR | 0100 | 所有者执行 |
//! | S_IRWXG | 0070 | 组 rwx |
//! | S_IRWXO | 0007 | 其他 rwx |
//!
//! ## 存储位置
//!
//! Linux 中存储在 `struct cred->fs_umask` (include/linux/cred.h)。
//! fork 时子进程继承父进程的 umask。
//!
//! ## 线程安全
//!
//! umask 是 per-process 的，不是 per-thread 的。
//! 但在 Linux 中，`sys_umask` 修改的是 current->cred->fs_umask，
//! 多线程共享同一 cred 结构 (通过 RCU 或 copy_cred 机制)。
//!
//! 参考: POSIX.1-2017, umask(3p)


/// sys_umask(mask) -> 旧的 umask 值
///
/// 设置进程的文件创建掩码，返回旧值。
///
/// TODO: 用户自行实现
pub fn sys_umask(_mask: usize) -> isize {
    // TODO: 当前返回 0022 (默认 umask)。
    // 实现时:
    // 1. 从当前任务的 cred 结构读取旧 umask
    // 2. 设置新 umask = mask & 0777
    // 3. 返回旧值
    0o022
}
