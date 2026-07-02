//! sys_mmap — 建立用户虚拟地址映射。
//!
//! ## 作用
//! 在当前进程地址空间中创建一段映射，返回映射起始虚拟地址。
//!
//! ## 参数
//! `addr` 为用户期望地址；`len` 为映射长度；`prot` 为访问权限；`flags` 为映射标志；`fd`/`offset` 描述可选文件后端。
//!
//! ## 注意事项
//! Linux mmap 强语义包含 VMA 合并/拆分、文件页缓存、MAP_FIXED 覆盖和权限校验；当前实现委托 `MemorySet::mmap`，语义完整度取决于内存子系统。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: arch/riscv/kernel/sys_riscv.c:24。
//!
//! ## 实现情况
//! 已接入当前任务地址空间的 mmap 路径；TODO: 继续补齐 Linux VMA 合并/拆分、文件映射、MAP_FIXED_NOREPLACE 和页缓存一致性语义。
//!
use log::debug;

use crate::syscall::VirAddr;
use crate::task::TASK_MANAER;

///mmap系统调用
/// Linux/POSIX: mmap(addr, len, prot, flags, fd, offset)
/// 返回：成功返回映射起始地址；失败返回 -1
///
/// 参数说明（Linux riscv64 ABI，用户态用 ecall 传参）：
/// `addr`  : 映射起始虚拟地址（用户 hint）。若带 `MAP_FIXED` 则必须使用该地址。
/// `len`   : 映射长度（字节）。内核按页对齐到覆盖该区间。
/// `prot`  : 访问权限位（PROT_*）：
///           - PROT_READ=0x1
///           - PROT_WRITE=0x2
///           - PROT_EXEC=0x4
/// `flags` : 映射标志（MAP_*），至少需要指定其一：
///           - MAP_SHARED=0x01 或 MAP_PRIVATE=0x02
///           - MAP_FIXED=0x10（可选）
///           - MAP_ANONYMOUS=0x20（匿名映射）
/// `fd`    : 文件描述符。匿名映射时要求 `fd == -1`；文件映射时为有效 fd。
/// `offset`: 文件偏移（字节）。必须页对齐（offset % PAGE_SIZE == 0）。匿名映射时通常为 0。
///
/// 当前最小实现：仅支持匿名映射（`MAP_ANONYMOUS` 且 `fd == -1`），并要求 `addr != 0`。
pub fn sys_mmap(
    addr: usize,
    len: usize,
    prot: usize,
    flags: usize,
    fd: i32,
    offset: usize,
) -> isize {
    let fd_backing = TASK_MANAER.get_current_fd(fd as usize).unwrap_or_default();
    let result = TASK_MANAER.task_que_inner.lock(|inner| {
        let current = inner.current;
        inner.task_queen[current].lock(|tcb| {
            tcb.memory_set
                .mmap(VirAddr(addr), len, prot, flags, fd, offset, fd_backing)
        })
    });

    // 记录 mmap 调试信息
    if result < 0 {
        debug!(
            "sys_mmap FAILED: addr=0x{:x}, len=0x{:x}, prot=0x{:x}, flags=0x{:x}, fd={}, offset=0x{:x}, error={}",
            addr, len, prot, flags, fd, offset, result
        );
    } else {
        debug!(
            "sys_mmap OK: addr=0x{:x}, len=0x{:x}, prot=0x{:x}, flags=0x{:x}, fd={}, offset=0x{:x}, ret=0x{:x}",
            addr, len, prot, flags, fd, offset, result
        );
    }

    result
}
