pub mod syscall;
use crate::{arch::memory::*, task::TASK_MANAER};
use crate::error::BlueErr;
use crate::syscall::syscall::*;
use log::{error, warn};
// Linux riscv64 syscall numbers (subset used by the oscomp test suite)
pub const SYS_GETCWD: usize = 17;
pub const SYS_IOCTL: usize = 29;
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
pub const SYS_WRITEV: usize = 66;
pub const SYS_NEWFSTATAT: usize = 79;
pub const SYS_FSTAT: usize = 80;
pub const SYS_EXIT: usize = 93;
pub const SYS_EXIT_GROUP: usize = 94;
pub const SYS_SET_TID_ADDRESS: usize = 96;
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
pub const SYS_MPROTECT: usize = 226;

mod sys_mprotect;
mod sys_mmap;
use crate::syscall::sys_mprotect::sys_mprotect;
use crate::syscall::sys_mmap::sys_mmap;
#[inline]
/// log x0-x31
fn log_x_regs(stage: &str, id: usize, trap_cx: &[usize; 32]) {
    warn!(
        "[syscall:{}] id={} x0={:#x} x1={:#x} x2={:#x} x3={:#x} x4={:#x} x5={:#x} x6={:#x} x7={:#x} x8={:#x} x9={:#x} x10={:#x} x11={:#x} x12={:#x} x13={:#x} x14={:#x} x15={:#x} x16={:#x} x17={:#x} x18={:#x} x19={:#x} x20={:#x} x21={:#x} x22={:#x} x23={:#x} x24={:#x} x25={:#x} x26={:#x} x27={:#x} x28={:#x} x29={:#x} x30={:#x} x31={:#x}",
        stage,
        id,
        trap_cx[0],
        trap_cx[1],
        trap_cx[2],
        trap_cx[3],
        trap_cx[4],
        trap_cx[5],
        trap_cx[6],
        trap_cx[7],
        trap_cx[8],
        trap_cx[9],
        trap_cx[10],
        trap_cx[11],
        trap_cx[12],
        trap_cx[13],
        trap_cx[14],
        trap_cx[15],
        trap_cx[16],
        trap_cx[17],
        trap_cx[18],
        trap_cx[19],
        trap_cx[20],
        trap_cx[21],
        trap_cx[22],
        trap_cx[23],
        trap_cx[24],
        trap_cx[25],
        trap_cx[26],
        trap_cx[27],
        trap_cx[28],
        trap_cx[29],
        trap_cx[30],
        trap_cx[31],
    );
}
///id: 系统调用号
///args:接受1个usize参数
///返回值：通过 x10 (a0) 寄存器返回给用户态
pub fn syscall_handler(id: usize, arg: [usize; 6]) -> isize {
    if id == SYS_MMAP {
        let current_task = TASK_MANAER.get_current_trapcx();
        log_x_regs("before", id, &current_task.x);
    }

    let ret = match id {
        SYS_SET_TID_ADDRESS => sys_set_tid_address(arg[0]),
        SYS_EXIT_GROUP => sys_exit_group(arg[0]),
        SYS_WRITEV => sys_writev(arg[0] as i32, arg[1], arg[2] as i32),
        SYS_WRITE => sys_write(arg[0], arg[1], arg[2]),
        SYS_READ => sys_read(arg[0], arg[1], arg[2]),
        SYS_EXIT => sys_exit(arg[0]),
        SYS_SCHED_YIELD => sys_yield(),

        SYS_NANOSLEEP => sys_nanosleep(arg[0], arg[1]),

        SYS_GETPID => sys_getpid(),
        SYS_GETPPID => sys_getppid(),

        SYS_DUP => sys_dup(arg[0] as i32),
        // Linux riscv64 userspace often implements dup2 via dup3(old, new, flags=0)
        SYS_DUP3 => {
            if arg[2] != 0 {
                BlueErr::EINVAL.as_isize()
            } else {
                sys_dup2(arg[0] as i32, arg[1] as i32)
            }
        }

        // NOTE: oscomp user/lib/syscall.c implements open() via openat(AT_FDCWD,...)
        // We currently ignore dirfd/mode and reuse sys_open's semantics.
        SYS_OPENAT => sys_open(arg[1], arg[2]),

        SYS_CLOSE => sys_close(arg[0]),
        SYS_LSEEK => sys_lseek(arg[0], arg[1] as isize, arg[2]),
        // newfstatat(dirfd, pathname, statbuf, flags)
        // For now we ignore dirfd/flags and reuse the existing path-based sys_stat.
        SYS_NEWFSTATAT => sys_stat(arg[1], arg[2]),
        // fstat(fd, statbuf)
        SYS_FSTAT => sys_fstat(arg[0], arg[1]),
        SYS_CLONE => sys_clone(arg[0], arg[1], arg[2], arg[3], arg[4]),
        SYS_EXECVE => sys_execve(arg[0], arg[1], arg[2]),
        SYS_WAIT4 => sys_wait4(arg[0] as i32, arg[1], arg[2] as i32),

        SYS_GETTIMEOFDAY => sys_gettimeofday(arg[0], arg[1]),
        SYS_TIMES => sys_times(arg[0]),

        // mkdirat(dirfd, pathname, mode)
        // oscomp user/lib/syscall.c implements mkdir() via mkdirat(AT_FDCWD,...,mode)
        SYS_MKDIRAT => sys_mkdirat(arg[0] as isize, arg[1], arg[2]),
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
        SYS_MPROTECT => sys_mprotect(arg[0], arg[1], arg[2]),
        // Not implemented yet in this kernel:
        SYS_SETPRIORITY | SYS_LINKAT => {
            error!("Unimplemented syscall id={}", id);
            BlueErr::ENOSYS.as_isize()
        }

        _ => {
            error!("Unknown syscall id={}", id);
            BlueErr::ENOSYS.as_isize()
        }
    };
    ret
}
