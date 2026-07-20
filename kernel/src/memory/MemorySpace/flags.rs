//! 地址空间相关的位标志与映射类型定义。
//!
//! 本文件集中放置内存映射子系统用到的所有 `bitflags!`/枚举语义类型：
//! - [`MapAreaFlags`]：`MapArea` 的访问权限标志，位布局与 `PTEFlags` 完全一致；
//! - [`MmapProt`]/[`MmapFlags`]：`mmap` 系统调用的保护位与行为标志；
//! - [`CloneFlags`]：`clone` 系统调用的语义标志（放在此处统一管理位标志类型）；
//! - [`MapType`]：区分“恒等映射”与“普通映射”。
//! 以及 `MapAreaFlags -> PTEFlags` 的转换实现。

use crate::arch::memory::PTEFlags;
use bitflags::bitflags;

bitflags! {//MapAreaFlags 和 PTEFlags 起始全为0
    #[derive(Debug,Clone, Copy)]
    pub struct MapAreaFlags: usize {
        ///Valid - bit 0
        const V = 1 << 0;
        ///Readable - bit 1
        const R = 1 << 1;
        ///Writable - bit 2
        const W = 1 << 2;
        ///Excutable - bit 3
        const X = 1 << 3;
        ///Accessible in U mode - bit 4
        const U = 1 << 4;
        ///Global mapping - bit 5 (AArch64: nG bit inverted)
        const G = 1 << 5;
        ///Accessed - bit 6 (AArch64: AF bit, must be 1)
        const A = 1 << 6;
        ///Dirty - bit 7 (AArch64: DBM bit for hardware dirty tracking)
        const D = 1 << 7;
        ///Device memory - bit 8 (AArch64: use AttrIndx=1 for Device nGnRE)
        ///CRITICAL: Must be set for MMIO devices (UART, etc.)
        ///Using Normal memory for devices causes undefined behavior!
        const DEV = 1 << 8;
    }
}

bitflags! {
    #[derive(Debug,Clone, Copy)]

    pub struct MmapProt: usize {
        const READ = 0x1;
        const WRITE = 0x2;
        const EXEC = 0x4;
    }
}

bitflags! {
    #[derive(Debug,Clone, Copy)]
    pub struct MmapFlags: usize {
        const SHARED = 0x01;
        const PRIVATE = 0x02;
        const FIXED = 0x10;
        const ANONYMOUS = 0x20;
    }
}

impl From<MapAreaFlags> for PTEFlags {
    fn from(value: MapAreaFlags) -> Self {
        match PTEFlags::from_bits(value.bits()) {
            Some(pteflags) => pteflags,
            None => {
                panic!("MapAreaFlags translate to PTEFlags Failed!")
            }
        }
    }
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum MapType {
    Indentical, //直接分配页帧
    Maped,      //不直接分配页帧
}

bitflags! {
    pub struct CloneFlags:usize{
        const CSIGNAL            = 0x000000ffusize; // 低 8 位：子进程退出/停止时向父进程发送的信号（如 SIGCHLD）

        const CLONE_VM           = 0x00000100usize; // 共享内存地址空间（线程语义；不共享则类似 fork 的独立地址空间）
        const CLONE_FS           = 0x00000200usize; // 共享文件系统信息（cwd/root/umask 等）
        const CLONE_FILES        = 0x00000400usize; // 共享打开文件表（fd table）
        const CLONE_SIGHAND      = 0x00000800usize; // 共享信号处理器（signal handlers）
        const CLONE_PIDFD        = 0x00001000usize; // 返回 pidfd（较新内核特性）
        const CLONE_PTRACE       = 0x00002000usize; // 让新进程继承被 ptrace 跟踪的状态
        const CLONE_VFORK        = 0x00004000usize; // vfork 语义：父进程阻塞直到子进程 exec/exit
        const CLONE_PARENT       = 0x00008000usize; // 新进程的父进程设为当前进程的父进程（"兄弟" 关系）
        const CLONE_THREAD       = 0x00010000usize; // 同一线程组（共享 TGID；通常需要配合 VM/FILES/SIGHAND）
        const CLONE_NEWNS        = 0x00020000usize; // 新的 mount namespace（挂载命名空间）
        const CLONE_SYSVSEM      = 0x00040000usize; // 共享 System V semaphore undo 列表
        const CLONE_SETTLS       = 0x00080000usize; // 设置 TLS（线程本地存储，如 %fs/%gs 基址）
        const CLONE_PARENT_SETTID= 0x00100000usize; // 在父进程地址空间写入子线程 TID（parent_tidptr）
        const CLONE_CHILD_CLEARTID=0x00200000usize; // 在线程退出时清零 child_tidptr 并做 futex 唤醒
        const CLONE_DETACHED     = 0x00400000usize; // 旧标志：分离线程（历史遗留，现代内核基本忽略）
        const CLONE_UNTRACED     = 0x00800000usize; // 新进程不可被 ptrace 跟踪（或不继承跟踪）
        const CLONE_CHILD_SETTID = 0x01000000usize; // 在子进程地址空间写入自身 TID（child_tidptr）
        const CLONE_NEWCGROUP    = 0x02000000usize; // 新的 cgroup namespace
        const CLONE_NEWUTS       = 0x04000000usize; // 新的 UTS namespace（hostname/domainname）
        const CLONE_NEWIPC       = 0x08000000usize; // 新的 IPC namespace（System V IPC/消息队列等）
        const CLONE_NEWUSER      = 0x10000000usize; // 新的 user namespace（uid/gid 映射）
        const CLONE_NEWPID       = 0x20000000usize; // 新的 PID namespace
        const CLONE_NEWNET       = 0x40000000usize; // 新的 network namespace
        const CLONE_IO           = 0x80000000usize; // 共享 I/O 上下文（ioprio 等）

        const CLONE_CLEAR_SIGHAND= 0x1_0000_0000usize; // 清除共享信号处理器（配合特定 clone 场景，较新/少用）
    }
}
