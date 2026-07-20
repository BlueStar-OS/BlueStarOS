//! sys_faccessat — 检查文件访问权限 (POSIX faccessat)。
//!
//! ## 作用
//! 按目录 fd、路径、访问模式和 flags 检查调用者是否可访问目标文件。
//!
//! ## 参数
//! `dfd` 为目录 fd 或 AT_FDCWD；`filename` 为用户态路径；`mode` 为 F_OK/R_OK/W_OK/X_OK；`flags` 为 AT_* 标志。
//!
//! ## 注意事项
//! 当前仍是占位实现；Linux 权限检查依赖真实 uid/euid、VFS path walk、LSM 和 mount 只读状态。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/open.c:539 / fs/open.c:544。
//!
//! ## 实现情况
//! 未实现，保留显式 TODO/占位；TODO: 依赖 cred、VFS 权限模型、符号链接控制和 `faccessat2` flags。
//!
//! 对标 Linux:
//! - `__NR_faccessat` = 48 (include/uapi/asm-generic/unistd.h)
//! - 内核入口: `do_faccessat` (fs/open.c:393)
//!
//! ## Linux riscv64 ABI
//!
//! | 寄存器 | 参数 | 含义 |
//! |--------|------|------|
//! | a0 | dfd | 目录文件描述符 (AT_FDCWD = -100 表示当前工作目录) |
//! | a1 | filename | 用户空间路径字符串指针 |
//! | a2 | mode | 访问模式 (R_OK\|W_OK\|X_OK\|F_OK) |
//! | a3 | flags | 标志位 (AT_EACCESS / AT_SYMLINK_NOFOLLOW) |
//!
//! ## 返回值
//!
//! - `= 0`: 权限检查通过
//! - `< 0`: 错误 (负 errno)
//!
//! ## errno
//!
//! | errno | 值 | 触发条件 |
//! |-------|------|----------|
//! | EACCES | 13 | 权限不足 |
//! | ELOOP | 40 | 符号链接循环 |
//! | ENAMETOOLONG | 36 | 路径过长 |
//! | ENOENT | 2 | 路径不存在 |
//! | ENOTDIR | 20 | 路径分量非目录 |
//! | EROFS | 30 | 只读文件系统上请求写权限 |
//!
//! ## mode 位 (include/uapi/linux/fs.h)
//!
//! | 常量 | 值 | 含义 |
//! |------|------|------|
//! | F_OK | 0 | 文件是否存在 |
//! | R_OK | 4 | 是否可读 |
//! | W_OK | 2 | 是否可写 |
//! | X_OK | 1 | 是否可执行 |
//!
//! ## flags 位
//!
//! | 常量 | 值 | 含义 |
//! |------|------|------|
//! | AT_EACCESS | 0x200 | 使用 euid/egid 而非 uid/gid |
//! | AT_SYMLINK_NOFOLLOW | 0x100 | 不跟随符号链接 |
//!
//! ## 与 access() 的关系
//!
//! POSIX `access(path, mode)` 在用户库中通过 `faccessat(AT_FDCWD, path, mode, 0)` 实现。
//! 参考: glibc/sysdeps/unix/sysv/linux/access.c


/// `faccessat` 的 mode 位。
///
/// 对标 Linux: `include/uapi/linux/fs.h`
pub const F_OK: usize = 0;
pub const R_OK: usize = 4;
pub const W_OK: usize = 2;
pub const X_OK: usize = 1;

/// `faccessat` 的 flags 位。
///
/// 对标 Linux: `include/uapi/linux/fcntl.h`
pub const AT_EACCESS: usize = 0x200;
pub const AT_SYMLINK_NOFOLLOW: usize = 0x100;

/// sys_faccessat(dfd, filename, mode, flags) -> 0 或 -errno
///
/// 检查调用进程是否有权限以指定模式访问给定路径。
///
/// TODO: 用户自行实现
pub fn sys_faccessat(_dfd: usize, _filename: usize, _mode: usize, _flags: usize) -> isize {
    // TODO: 实现步骤
    // 1. 从用户空间拷贝路径字符串 (dfd + filename)
    // 2. 路径解析 (跟随符号链接，除非 AT_SYMLINK_NOFOLLOW)
    // 3. 根据 mode 检查 inode 权限位 (rwx)
    // 4. 使用 euid/egid (AT_EACCESS) 或 uid/gid 进行权限判定
    // 5. 返回 0 (通过) 或负 errno

    unimplemented!("sys_faccessat: user TODO")
}
