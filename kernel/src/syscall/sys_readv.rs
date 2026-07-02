//! sys_readv — 向量化读 scatter-gather read。
//!
//! ## 作用
//! 从同一个 fd 读取数据，并按用户态 iovec 数组分散写入多个缓冲区。
//!
//! ## 参数
//! `fd` 为文件描述符；`vec` 为用户态 iovec 数组地址；`vlen` 为 iovec 元素数量。
//!
//! ## 注意事项
//! 当前逐个 iovec 调用 `sys_read`，不是 Linux `iov_iter` 的一次性 VFS 读；部分成功后遇错会返回已读字节数。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/read_write.c:1162。
//!
//! ## 实现情况
//! 已实现基础 iovec 遍历、EFAULT/EINVAL 和部分读返回；TODO: 补齐 Linux UIO_MAXIOV、总长度溢出、原子性和 `iov_iter` 语义。
//!
//! # 处理流程
//! 1. 遍历 iovec 数组，从 vec 逐个拷贝 iovec 到内核
//! 2. 对每个 iovec，调用 sys_read(fd, iov_base, iov_len) 读入 buf
//! 3. 累加已读字节数；若某次 read 返回 0 (EOF) 或出错则提前终止
//! 4. 返回总读取字节数；出错返回 -errno
use crate::arch::memory::*;
use crate::syscall::sys_read::sys_read;
use crate::task::TASK_MANAER;
use core::mem::size_of;
use core::mem::size_of_val;
use log::debug;

/// iovec 结构（与 Linux include/uapi/linux/uio.h:17 一致）
/// RISC-V 64：iov_base=8 字节, iov_len=8 字节, 共 16 字节
#[repr(C, packed)]
struct IoVec {
    iov_base: usize, // 用户态缓冲区地址（void __user *）
    iov_len: usize,  // 缓冲区长度（size_t）
}

/// readv 实现：逐 iovec 调用内核内部 read 逻辑
///
/// 参数顺序符合 Linux riscv64 ABI：
///   a0 = fd, a1 = vec, a2 = vlen
pub fn sys_readv(fd: i32, vec: usize, vlen: usize) -> isize {
    // 校验 iovec 数量（Linux: UIO_MAXIOV=1024, include/uapi/linux/uio.h:28）
    if vlen == 0 {
        return 0;
    }
    if vlen > 1024 {
        return -crate::error::BlueErr::EINVAL.as_isize();
    }

    // 获取当前进程页表（用于读用户态内存）
    let satp = TASK_MANAER.get_current_stap();
    let mut tb = PageTable::crate_table_from_satp(satp);

    let mut total_read: isize = 0;
    let iovec_size = size_of::<IoVec>();

    for i in 0..vlen {
        let entry_addr = vec + i * iovec_size;

        // 从用户态读取 iov_base（前 usize 字节）
        let user_base = match tb.read_bytes_from_userspace(VirAddr(entry_addr), size_of::<usize>())
        {
            Some(bytes) => match bytes.try_into() {
                Ok(raw) => usize::from_le_bytes(raw),
                Err(_) => return -crate::error::BlueErr::EFAULT.as_isize(),
            },
            None => {
                if total_read > 0 {
                    return total_read;
                }
                return -crate::error::BlueErr::EFAULT.as_isize();
            }
        };

        // 从用户态读取 iov_len（后 usize 字节）
        let user_len = match tb
            .read_bytes_from_userspace(VirAddr(entry_addr + size_of::<usize>()), size_of::<usize>())
        {
            Some(bytes) => match bytes.try_into() {
                Ok(raw) => usize::from_le_bytes(raw),
                Err(_) => return -crate::error::BlueErr::EFAULT.as_isize(),
            },
            None => {
                if total_read > 0 {
                    return total_read;
                }
                return -crate::error::BlueErr::EFAULT.as_isize();
            }
        };

        debug!(
            "sys_readv: i={} fd={} iov[{i}]={{base=0x{:x}, len=0x{:x}}} entry_addr=0x{:x}",
            i, fd, user_base, user_len, entry_addr,
        );

        // 空长度条目直接跳过
        if user_len == 0 {
            continue;
        }

        // 调用内核内部读逻辑（支持跨页）
        let n = sys_read(fd as usize, user_base, user_len);
        if n < 0 {
            if total_read > 0 {
                return total_read;
            }
            return n;
        }
        total_read += n;
        if n == 0 || (n as usize) < user_len {
            // EOF 或未读满，提前终止
            break;
        }
    }

    total_read
}
