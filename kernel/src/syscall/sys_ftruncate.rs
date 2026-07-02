//! sys_ftruncate — 按 fd 截断文件 (POSIX ftruncate)。
//!
//! ## 作用
//! 将已打开文件截断或扩展到指定长度。
//!
//! ## 参数
//! `fd` 为文件描述符；`length` 为目标文件长度。
//!
//! ## 注意事项
//! 当前仍是占位实现；Linux 需要处理写权限、inode 锁、page cache、mtime/ctime 和 sparse file 语义。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/open.c:213。
//!
//! ## 实现情况
//! 未实现，保留显式 TODO/占位；TODO: 依赖 VFS truncate、inode 尺寸更新、块释放/零填充和元数据时间戳。
//!
//! 对标 Linux:
//! - `__NR_ftruncate` / `__NR3264_ftruncate` = 46 (include/uapi/asm-generic/unistd.h)
//! - 内核入口: `do_sys_ftruncate` (fs/open.c:190)
//!
//! ## Linux riscv64 ABI
//!
//! | 寄存器 | 参数 | 含义 |
//! |--------|------|------|
//! | a0 | fd | 文件描述符 (必须可写) |
//! | a1 | length | 新的文件长度 (字节) |
//!
//! ## 返回值
//!
//! - `= 0`: 成功
//! - `< 0`: 错误 (负 errno)
//!
//! ## errno
//!
//! | errno | 值 | 触发条件 |
//! |-------|------|----------|
//! | EBADF | 9 | fd 无效或不可写 |
//! | EINVAL | 22 | length 为负数 |
//! | EFBIG | 27 | length 超出文件系统限制 |
//! | EIO | 5 | I/O 错误 |
//! | EISDIR | 21 | fd 指向目录 |
//! | EPERM | 1 | 文件系统不允许截断 |
//! | EROFS | 30 | 只读文件系统 |
//!
//! ## 语义
//!
//! - 若 length < 当前文件大小: 截断，多余数据丢弃
//! - 若 length > 当前文件大小: 扩展，空洞部分读为 0 (POSIX sparse file)
//! - 文件偏移量不受影响
//! - 修改文件的 st_mtime 和 st_ctime
//!
//! ## 与 truncate() 的关系
//!
//! POSIX `truncate(path, length)` 在用户库中通过
//! `open(path, O_WRONLY)` + `ftruncate(fd, length)` + `close(fd)` 实现。
//! 参考: glibc/sysdeps/posix/truncate.c

use crate::error::BlueErr;

/// sys_ftruncate(fd, length) -> 0 或 -errno
///
/// 将打开的文件截断/扩展到指定长度。
///
/// TODO: 用户自行实现
pub fn sys_ftruncate(fd: usize, length: usize) -> isize {
    // TODO: 实现步骤
    // 1. 通过 TASK_MANAER 获取 fd 对应的 Arc<dyn File>
    // 2. 检查文件是否可写 (O_ACCMODE)
    // 3. 调用 file.truncate(length as u64) 或通过 VFS 层操作 inode
    // 4. 若 length < size: 释放尾部块，更新 inode size
    // 5. 若 length > size: 零填充扩展 (或标记为空洞)
    // 6. 返回 0

    unimplemented!("sys_ftruncate: user TODO")
}
