use crate::task::TASK_MANAER;
use crate::syscall::VirAddr;


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
    //warn!("enter mmap");
    let inner = TASK_MANAER.task_que_inner.lock();
    let current = inner.current;
    drop(inner);
    let fd_backing = match TASK_MANAER.get_current_fd(fd as usize) {
        Some(v) => v,
        _ => None,
    };
    let inner = TASK_MANAER.task_que_inner.lock();
    let mut tcb = inner.task_queen[current].lock();

    tcb.memory_set
        .mmap(VirAddr(addr), len, prot, flags, fd, offset, fd_backing)
}