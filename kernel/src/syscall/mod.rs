mod syscall;
use log::{error, warn};
use crate::memory::VirAddr;
use crate::syscall::syscall::*;
// Linux riscv64 syscall numbers (subset used by the oscomp test suite)
pub const SYS_GETCWD: usize = 17;
pub const SYS_UNLINKAT: usize = 35;
pub const SYS_LINKAT: usize = 37;
pub const SYS_UMOUNT2: usize = 39;
pub const SYS_MOUNT: usize = 40;
pub const SYS_MKDIRAT: usize = 34;
pub const SYS_CHDIR: usize = 49;
pub const SYS_OPENAT: usize = 56;
pub const SYS_CLOSE: usize = 57;
pub const SYS_PIPE2: usize = 59;
pub const SYS_GETDENTS64: usize = 61;
pub const SYS_LSEEK: usize = 62;
pub const SYS_READ: usize = 63;
pub const SYS_WRITE: usize = 64;
pub const SYS_NEWFSTATAT: usize = 79;
pub const SYS_FSTAT: usize = 80;
pub const SYS_EXIT: usize = 93;
pub const SYS_NANOSLEEP: usize = 101;
pub const SYS_SETPRIORITY: usize = 140;
pub const SYS_TIMES: usize = 153;
pub const SYS_UNAME: usize = 160;
pub const SYS_GETTIMEOFDAY: usize = 169;
pub const SYS_GETPID: usize = 172;
pub const SYS_GETPPID: usize = 173;
pub const SYS_BRK: usize = 214;
pub const SYS_MUNMAP: usize = 215;
pub const SYS_CLONE: usize = 220;
pub const SYS_EXECVE: usize = 221;
pub const SYS_MMAP: usize = 222;
pub const SYS_WAIT4: usize = 260;
pub const SYS_SCHED_YIELD: usize = 124;
pub const SYS_DUP: usize = 23;
pub const SYS_DUP3: usize = 24;
///id: 系统调用号
///args:接受1个usize参数
///返回值：通过 x10 (a0) 寄存器返回给用户态
pub fn syscall_handler(id:usize,arg:[usize;6]) -> isize {
    match id {
        SYS_WRITE => sys_write(arg[0], arg[1], arg[2]),
        SYS_READ => sys_read(arg[0], arg[1], arg[2]),
        SYS_EXIT => sys_exit(arg[0]),
        SYS_SCHED_YIELD => sys_yield(),

        SYS_GETPID => sys_getpid(),
        SYS_GETPPID => sys_getppid(),

        SYS_DUP => sys_dup(arg[0] as i32),
        // Linux riscv64 userspace often implements dup2 via dup3(old, new, flags=0)
        SYS_DUP3 => {
            if arg[2] != 0 {
                -1
            } else {
                sys_dup2(arg[0] as i32, arg[1] as i32)
            }
        }

        // NOTE: oscomp user/lib/syscall.c implements open() via openat(AT_FDCWD,...)
        // We currently ignore dirfd/mode and reuse sys_open's semantics.
        SYS_OPENAT => sys_open(arg[1], arg[2]),

        SYS_CLOSE=>{
            sys_close(arg[0])
        }
        SYS_LSEEK=>{
            sys_lseek(arg[0], arg[1] as isize, arg[2])
        }
        // newfstatat(dirfd, pathname, statbuf, flags)
        // For now we ignore dirfd/flags and reuse the existing path-based sys_stat.
        SYS_NEWFSTATAT => sys_stat(arg[1], arg[2]),
        // fstat(fd, statbuf)
        SYS_FSTAT => sys_fstat(arg[0], arg[1]),
        SYS_CLONE => sys_fork(),
        SYS_EXECVE => sys_exec(arg[0], arg[1], arg[2]),
        SYS_WAIT4 => sys_wait(arg[1]),

        // mkdirat(dirfd, pathname, mode)
        // oscomp user/lib/syscall.c implements mkdir() via mkdirat(AT_FDCWD,...,mode)
        SYS_MKDIRAT => {sys_mkdirat(arg[0] as isize, arg[1], arg[2])},
        SYS_UNLINKAT => sys_unlink(arg[1]),

        SYS_GETDENTS64 => sys_getdents64(arg[0], arg[1], arg[2]),
        SYS_PIPE2 => sys_pipe(arg[0]),

        SYS_BRK => sys_brk(VirAddr(arg[0])) as isize,

        SYS_CHDIR => sys_chdir(arg[0]),
        SYS_GETCWD => sys_getcwd(arg[0], arg[1]),

        SYS_UNAME => sys_uname(arg[0]),

        SYS_MMAP => sys_mmap(arg[0], arg[1], arg[2], arg[3], arg[4] as i32, arg[5]),
        SYS_MUNMAP => sys_munmap(arg[0], arg[1]),

        SYS_MOUNT => sys_mount(arg[0], arg[1], arg[2], arg[3], arg[4]),
        SYS_UMOUNT2 => sys_umount2(arg[0], arg[1]),

        // Not implemented yet in this kernel:
        SYS_GETTIMEOFDAY | SYS_TIMES | SYS_NANOSLEEP | SYS_SETPRIORITY | SYS_LINKAT => {
            error!("Unimplemented syscall id={}", id);
            -1
        }

        _ => {
            error!("Unknown syscall id={}", id);
            -1
        }
    }
}