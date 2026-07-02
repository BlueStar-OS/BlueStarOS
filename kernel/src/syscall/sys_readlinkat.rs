//! sys_readlinkat — 读取符号链接目标 (POSIX readlinkat)。
//!
//! ## 作用
//! 读取符号链接本身保存的目标路径，并写入用户缓冲区。
//!
//! ## 参数
//! `dfd` 为目录 fd 或 AT_FDCWD；`pathname` 为用户态路径；`buf`/`bufsiz` 描述用户输出缓冲区。
//!
//! ## 注意事项
//! 当前仍是占位实现；readlinkat 不追加 NUL，且最后一级路径必须按“不跟随符号链接”语义处理。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/stat.c:604。
//!
//! ## 实现情况
//! 未实现，保留显式 TODO/占位；TODO: 依赖 VFS symlink inode、AT_FDCWD 路径解析和用户缓冲区截断写回。
//!
//! 对标 Linux:
//! - `__NR_readlinkat` = 78 (include/uapi/asm-generic/unistd.h)
//! - 内核入口: `do_readlinkat` (fs/stat.c:366)
//!
//! ## Linux riscv64 ABI
//!
//! | 寄存器 | 参数 | 含义 |
//! |--------|------|------|
//! | a0 | dfd | 目录文件描述符 (AT_FDCWD = -100) |
//! | a1 | pathname | 符号链接路径指针 |
//! | a2 | buf | 用户空间输出缓冲区 |
//! | a3 | bufsiz | 缓冲区大小 (字节) |
//!
//! ## 返回值
//!
//! - `>= 0`: 写入 buf 的字节数 (不含 NUL 终止符)
//! - `< 0`: 错误 (负 errno)
//!
//! ## errno
//!
//! | errno | 值 | 触发条件 |
//! |-------|------|----------|
//! | EACCES | 13 | 路径中某目录无搜索权限 |
//! | EINVAL | 22 | bufsiz 为负数 |
//! | ELOOP | 40 | 符号链接循环 |
//! | ENAMETOOLONG | 36 | 路径或链接目标过长 |
//! | ENOENT | 2 | 路径不存在 |
//! | ENOMEM | 12 | 内核内存不足 |
//! | ENOTDIR | 20 | 路径分量非目录 |
//!
//! ## 语义
//!
//! - **不追加 NUL**: 与 `readlink` 一样，返回内容不含 `\0`。
//! - **截断**: 若链接目标长度 > bufsiz，截断到 bufsiz 字节。
//! - **不跟随符号链接**: pathname 本身必须是符号链接，不会递归解析。
//!
//! ## readlink() 的关系
//!
//! POSIX `readlink(path, buf, bufsiz)` 在用户库中通过
//! `readlinkat(AT_FDCWD, path, buf, bufsiz)` 实现。
//! 参考: glibc/sysdeps/unix/sysv/linux/readlink.c

use crate::error::BlueErr;

/// sys_readlinkat(dfd, pathname, buf, bufsiz) -> 读取字节数 或 -errno
///
/// 读取符号链接的目标路径到用户缓冲区。
///
/// TODO: 用户自行实现
pub fn sys_readlinkat(dfd: usize, pathname: usize, buf: usize, bufsiz: usize) -> isize {
    // TODO: 实现步骤
    // 1. 从用户空间拷贝路径字符串 (dfd + pathname)
    // 2. 路径解析，定位到符号链接 inode (不跟随最后一级)
    // 3. 读取符号链接存储的目标路径
    // 4. 截断到 bufsiz 字节
    // 5. 写回用户空间 buf
    // 6. 返回写入字节数

    unimplemented!("sys_readlinkat: user TODO")
}
