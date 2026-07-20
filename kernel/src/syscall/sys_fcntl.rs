//! sys_fcntl — 文件描述符控制 (POSIX fcntl)。
//!
//! ## 作用
//! 对 fd 执行复制、描述符标志、文件状态标志和文件锁等控制操作。
//!
//! ## 参数
//! `fd` 为文件描述符；`cmd` 为 fcntl 命令；`arg` 为命令相关参数或用户指针。
//!
//! ## 注意事项
//! 当前仍是占位实现；Linux 中 fd flags 与 file status flags 分属 fd 表和 `struct file`，不能混写。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/fcntl.c:574。
//!
//! ## 实现情况
//! 未实现，保留显式 TODO/占位；TODO: 依赖 fd table 标志位、OpenFlags 可变状态和 POSIX file lock 语义。
//!
//! 对标 Linux:
//! - `__NR_fcntl` / `__NR3264_fcntl` = 25 (include/uapi/asm-generic/unistd.h)
//! - 内核入口: `do_vfs_ioctl` / `ksys_fcntl` (fs/fcntl.c)
//!
//! ## Linux riscv64 ABI
//!
//! | 寄存器 | 参数 | 含义 |
//! |--------|------|------|
//! | a0 | fd | 文件描述符 |
//! | a1 | cmd | 命令码 |
//! | a2 | arg | 命令参数 (含义取决于 cmd) |
//!
//! ## 返回值
//!
//! - `>= 0`: 成功 (含义取决于 cmd)
//! - `< 0`: 错误 (负 errno)
//!
//! ## errno
//!
//! | errno | 值 | 触发条件 |
//! |-------|------|----------|
//! | EBADF | 9 | fd 无效 |
//! | EACCES / EAGAIN | 13/11 | 锁冲突 (F_SETLK) |
//! | EINVAL | 22 | cmd 或 arg 无效 |
//! | EMFILE | 24 | F_DUPFD 达到上限 |
//!
//! ## 常用 cmd (include/uapi/linux/fcntl.h)
//!
//! | cmd | 值 | 语义 | arg 含义 |
//! |-----|------|------|----------|
//! | F_DUPFD | 0 | 复制 fd，返回 >= arg 的最小可用 fd | 最小 fd |
//! | F_GETFD | 1 | 获取文件描述符标志 | 忽略 |
//! | F_SETFD | 2 | 设置文件描述符标志 | FD_CLOEXEC |
//! | F_GETFL | 3 | 获取文件状态标志 (O_RDONLY 等) | 忽略 |
//! | F_SETFL | 4 | 设置文件状态标志 | O_APPEND 等 |
//! | F_SETLK | 6 | 非阻塞文件锁 | `struct flock *` |
//! | F_SETLKW | 7 | 阻塞文件锁 | `struct flock *` |
//! | F_GETLK | 5 | 查询文件锁状态 | `struct flock *` |
//! | F_DUPFD_CLOEXEC | 1030 | 复制 fd + 设置 CLOEXEC | 最小 fd |
//!
//! ## 文件描述符标志 vs 文件状态标志
//!
//! | 类型 | 存储位置 | 管理的 flag |
//! |------|----------|-------------|
//! | 文件描述符标志 (F_GETFD/F_SETFD) | 每个 fd 独立 | FD_CLOEXEC |
//! | 文件状态标志 (F_GETFL/F_SETFL) | struct file 共享 | O_APPEND, O_NONBLOCK 等 |
//!
//! ## O_ACCMODE 处理
//!
//! F_GETFL 返回的低 2 bit 是访问模式 (O_RDONLY=0, O_WRONLY=1, O_RDWR=2)。
//! F_SETFL 不能修改访问模式，只能修改 O_APPEND/O_NONBLOCK 等状态标志。


/// 文件描述符标志。
pub const FD_CLOEXEC: usize = 1;

/// fcntl 命令码。
///
/// 对标 Linux: `include/uapi/linux/fcntl.h`
pub const F_DUPFD: usize = 0;
pub const F_GETFD: usize = 1;
pub const F_SETFD: usize = 2;
pub const F_GETFL: usize = 3;
pub const F_SETFL: usize = 4;
pub const F_GETLK: usize = 5;
pub const F_SETLK: usize = 6;
pub const F_SETLKW: usize = 7;
pub const F_DUPFD_CLOEXEC: usize = 1030;

/// POSIX `struct flock` 布局 (文件锁描述)。
///
/// 对标 Linux: `include/uapi/asm-generic/fcntl.h`
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Flock {
    /// 锁类型: F_RDLCK(0), F_WRLCK(1), F_UNLCK(2)
    pub l_type: i16,
    /// 锁起始位置的基准: SEEK_SET(0), SEEK_CUR(1), SEEK_END(2)
    pub l_whence: i16,
    /// 锁起始偏移 (字节)
    pub l_start: i64,
    /// 锁长度 (0 = 到文件末尾)
    pub l_len: i64,
    /// 拥有锁的进程 PID (F_GETLK 返回)
    pub l_pid: i32,
}

/// 锁类型常量。
pub const F_RDLCK: i16 = 0;
pub const F_WRLCK: i16 = 1;
pub const F_UNLCK: i16 = 2;

/// sys_fcntl(fd, cmd, arg) -> 结果值 或 -errno
///
/// 对文件描述符执行控制操作。
///
/// TODO: 用户自行实现
pub fn sys_fcntl(_fd: usize, _cmd: usize, _arg: usize) -> isize {
    // TODO: 实现步骤
    // match cmd {
    //     F_DUPFD => 从 fd 复制，找到 >= arg 的最小可用 fd
    //     F_GETFD => 返回 fd 的描述符标志 (FD_CLOEXEC)
    //     F_SETFD => 设置 fd 的描述符标志
    //     F_GETFL => 返回 file 的 OpenFlags
    //     F_SETFL => 修改 file 的 O_APPEND/O_NONBLOCK 等 (不改访问模式)
    //     F_DUPFD_CLOEXEC => F_DUPFD + FD_CLOEXEC
    //     F_GETLK / F_SETLK / F_SETLKW => 文件锁操作 (当前可返回 ENOSYS)
    // }

    unimplemented!("sys_fcntl: user TODO")
}
