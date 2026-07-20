use crate::error::BlueErr;
use crate::{arch::memory::*, task::TASK_MANAER};
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
pub const SYS_READV: usize = 65;
pub const SYS_RT_SIGACTION: usize = 134;
pub const SYS_RT_SIGPROCMASK: usize = 135;
pub const SYS_CLOCK_GETTIME: usize = 113;
pub const SYS_DUP: usize = 23;
pub const SYS_DUP3: usize = 24;
pub const SYS_MPROTECT: usize = 226;

// ── 新增系统调用 (Linux riscv64 ABI) ─────────────────────────
// 参考: include/uapi/asm-generic/unistd.h
pub const SYS_FACCESSAT: usize = 48;
pub const SYS_READLINKAT: usize = 78;
pub const SYS_FTRUNCATE: usize = 46;
pub const SYS_UMASK: usize = 166;
// process
pub const SYS_GETUID: usize = 174;
pub const SYS_GETEUID: usize = 175;
pub const SYS_GETGID: usize = 176;
pub const SYS_GETEGID: usize = 177;
pub const SYS_GETTID: usize = 178;
pub const SYS_SETPGID: usize = 154;
pub const SYS_GETPGID: usize = 155;
pub const SYS_GETSID: usize = 156;
pub const SYS_SETSID: usize = 157;
// signal
pub const SYS_KILL: usize = 129;
pub const SYS_TKILL: usize = 130;
pub const SYS_TGKILL: usize = 131;
pub const SYS_RT_SIGRETURN: usize = 139;
// network (已定义但未 dispatch)
pub const SYS_SOCKET: usize = 198;
pub const SYS_SOCKETPAIR: usize = 199;
pub const SYS_BIND: usize = 200;
pub const SYS_LISTEN: usize = 201;
pub const SYS_ACCEPT: usize = 202;
pub const SYS_CONNECT: usize = 203;
pub const SYS_SENDTO: usize = 206;
pub const SYS_RECVFROM: usize = 207;
pub const SYS_SETSOCKOPT: usize = 208;
pub const SYS_GETSOCKOPT: usize = 209;
pub const SYS_SHUTDOWN: usize = 210;
pub const SYS_ACCEPT4: usize = 242;
// misc
pub const SYS_FCNTL: usize = 25;
pub const SYS_SELECT: usize = 72;

mod network;
mod process;
mod signal;
mod sys_brk;
mod sys_chdir;
mod sys_clock_gettime;
mod sys_clone;
mod sys_close;
mod sys_creat;
mod sys_dup;
mod sys_dup2;
mod sys_execve;
pub(crate) mod sys_exit;
mod sys_exit_group;
mod sys_faccessat;
mod sys_fcntl;
mod sys_fork;
mod sys_fstat;
mod sys_ftruncate;
mod sys_getcwd;
mod sys_getdents64;
mod sys_getpid;
mod sys_getppid;
mod sys_gettimeofday;
mod sys_ioctl;
mod sys_lseek;
mod sys_mkdir;
mod sys_mkdirat;
mod sys_mmap;
mod sys_mount;
mod sys_mprotect;
mod sys_munmap;
mod sys_nanosleep;
mod sys_open;
mod sys_pipe;
mod sys_read;
mod sys_readlinkat;
mod sys_readv;
mod sys_rt_sigaction;
mod sys_rt_sigprocmask;
mod sys_select;
mod sys_set_tid_address;
mod sys_stat;
mod sys_times;
mod sys_umask;
mod sys_umount2;
mod sys_uname;
mod sys_unlink;
mod sys_wait;
mod sys_wait4;
mod sys_write;
mod sys_writev;
mod sys_yield;
pub(crate) mod syscall;

use crate::syscall::sys_brk::sys_brk;
use crate::syscall::sys_chdir::sys_chdir;
use crate::syscall::sys_clock_gettime::sys_clock_gettime;
use crate::syscall::sys_clone::sys_clone;
use crate::syscall::sys_close::sys_close;
use crate::syscall::sys_dup::sys_dup;
use crate::syscall::sys_dup2::sys_dup2;
use crate::syscall::sys_execve::sys_execve;
use crate::syscall::sys_exit::sys_exit;
use crate::syscall::sys_exit_group::sys_exit_group;
use crate::syscall::sys_faccessat::sys_faccessat;
use crate::syscall::sys_fcntl::sys_fcntl;
use crate::syscall::sys_fstat::sys_fstat;
use crate::syscall::sys_ftruncate::sys_ftruncate;
use crate::syscall::sys_getcwd::sys_getcwd;
use crate::syscall::sys_getdents64::sys_getdents64;
use crate::syscall::sys_getpid::sys_getpid;
use crate::syscall::sys_getppid::sys_getppid;
use crate::syscall::sys_gettimeofday::sys_gettimeofday;
use crate::syscall::sys_ioctl::sys_ioctl;
use crate::syscall::sys_lseek::sys_lseek;
use crate::syscall::sys_mkdirat::sys_mkdirat;
use crate::syscall::sys_mmap::sys_mmap;
use crate::syscall::sys_mount::sys_mount;
use crate::syscall::sys_mprotect::sys_mprotect;
use crate::syscall::sys_munmap::sys_munmap;
use crate::syscall::sys_nanosleep::sys_nanosleep;
use crate::syscall::sys_open::sys_open;
use crate::syscall::sys_pipe::sys_pipe;
use crate::syscall::sys_read::sys_read;
use crate::syscall::sys_readlinkat::sys_readlinkat;
use crate::syscall::sys_readv::sys_readv;
use crate::syscall::sys_rt_sigaction::sys_rt_sigaction;
use crate::syscall::sys_rt_sigprocmask::sys_rt_sigprocmask;
use crate::syscall::sys_select::sys_select;
use crate::syscall::sys_set_tid_address::sys_set_tid_address;
use crate::syscall::sys_stat::sys_stat;
use crate::syscall::sys_times::sys_times;
use crate::syscall::sys_umask::sys_umask;
use crate::syscall::sys_umount2::sys_umount2;
use crate::syscall::sys_uname::sys_uname;
use crate::syscall::sys_unlink::sys_unlink;
use crate::syscall::sys_wait4::sys_wait4;
use crate::syscall::sys_write::sys_write;
use crate::syscall::sys_writev::sys_writev;
use crate::syscall::sys_yield::sys_yield;

use crate::syscall::network::sys_accept::sys_accept;
use crate::syscall::network::sys_accept4::sys_accept4;
use crate::syscall::network::sys_bind::sys_bind;
use crate::syscall::network::sys_connect::sys_connect;
use crate::syscall::network::sys_getsockopt::sys_getsockopt;
use crate::syscall::network::sys_listen::sys_listen;
use crate::syscall::network::sys_recvfrom::sys_recvfrom;
use crate::syscall::network::sys_sendto::sys_sendto;
use crate::syscall::network::sys_setsockopt::sys_setsockopt;
use crate::syscall::network::sys_shutdown::sys_shutdown;
use crate::syscall::network::sys_socket::sys_socket;
use crate::syscall::network::sys_socketpair::sys_socketpair;
use crate::syscall::process::sys_getegid::sys_getegid;
use crate::syscall::process::sys_geteuid::sys_geteuid;
use crate::syscall::process::sys_getgid::sys_getgid;
use crate::syscall::process::sys_getpgid::sys_getpgid;
use crate::syscall::process::sys_getsid::sys_getsid;
use crate::syscall::process::sys_gettid::sys_gettid;
use crate::syscall::process::sys_getuid::sys_getuid;
use crate::syscall::process::sys_setpgid::sys_setpgid;
use crate::syscall::process::sys_setsid::sys_setsid;
use crate::syscall::signal::sys_kill::sys_kill;
use crate::syscall::signal::sys_rt_sigreturn::*;
use crate::syscall::signal::sys_tgkill::sys_tgkill;
use crate::syscall::signal::sys_tkill::sys_tkill;
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
        SYS_IOCTL => sys_ioctl(arg[0], arg[1], arg[2]),
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
        SYS_CLOCK_GETTIME => sys_clock_gettime(arg[0], arg[1]),
        SYS_TIMES => sys_times(arg[0]),

        // mkdirat(dirfd, pathname, mode)
        // oscomp user/lib/syscall.c implements mkdir() via mkdirat(AT_FDCWD,...,mode)
        SYS_MKDIRAT => sys_mkdirat(arg[0] as isize, arg[1], arg[2]),
        SYS_UNLINKAT => sys_unlink(arg[1]),

        SYS_GETDENTS64 => sys_getdents64(arg[0], arg[1], arg[2]),
        SYS_PIPE2 => sys_pipe(arg[0]),

        SYS_BRK => sys_brk(VirAddr(arg[0])),

        SYS_CHDIR => sys_chdir(arg[0]),
        SYS_GETCWD => sys_getcwd(arg[0], arg[1]),

        SYS_UNAME => sys_uname(arg[0]),

        SYS_MMAP => sys_mmap(arg[0], arg[1], arg[2], arg[3], arg[4] as i32, arg[5]),
        SYS_MUNMAP => sys_munmap(arg[0], arg[1]),

        SYS_MOUNT => sys_mount(arg[0], arg[1], arg[2], arg[3], arg[4]),
        SYS_UMOUNT2 => sys_umount2(arg[0], arg[1]),
        SYS_MPROTECT => sys_mprotect(arg[0], arg[1], arg[2]),

        SYS_READV => sys_readv(arg[0] as i32, arg[1], arg[2]),
        SYS_RT_SIGACTION => sys_rt_sigaction(arg[0] as i32, arg[1], arg[2], arg[3]),
        SYS_RT_SIGPROCMASK => sys_rt_sigprocmask(arg[0] as i32, arg[1], arg[2], arg[3]),

        SYS_SETPRIORITY | SYS_LINKAT => {
            error!("Unimplemented syscall id={}", id);
            BlueErr::ENOSYS.as_isize()
        }

        // ── 网络系统调用 ──────────────────────────────────────
        SYS_SOCKET => sys_socket(arg[0], arg[1], arg[2]),
        SYS_BIND => sys_bind(arg[0], arg[1], arg[2]),
        SYS_SENDTO => sys_sendto(arg[0], arg[1], arg[2], arg[3], arg[4], arg[5]),
        SYS_RECVFROM => sys_recvfrom(arg[0], arg[1], arg[2], arg[3], arg[4], arg[5]),

        SYS_SELECT => sys_select(arg[0], arg[1], arg[2], arg[3], arg[4]),

        // ── 新增系统调用 ──────────────────────────────────────
        SYS_FACCESSAT => sys_faccessat(arg[0], arg[1], arg[2], arg[3]),
        SYS_READLINKAT => sys_readlinkat(arg[0], arg[1], arg[2], arg[3]),
        SYS_FCNTL => sys_fcntl(arg[0], arg[1], arg[2]),
        SYS_FTRUNCATE => sys_ftruncate(arg[0], arg[1]),
        SYS_UMASK => sys_umask(arg[0]),
        SYS_GETUID => sys_getuid(),
        SYS_GETEUID => sys_geteuid(),
        SYS_GETGID => sys_getgid(),
        SYS_GETEGID => sys_getegid(),
        SYS_GETTID => sys_gettid(),
        SYS_SETPGID => sys_setpgid(arg[0], arg[1]),
        SYS_GETPGID => sys_getpgid(arg[0]),
        SYS_GETSID => sys_getsid(arg[0]),
        SYS_SETSID => sys_setsid(),
        SYS_KILL => sys_kill(arg[0], arg[1]),
        SYS_TKILL => sys_tkill(arg[0], arg[1]),
        SYS_TGKILL => sys_tgkill(arg[0], arg[1], arg[2]),
        SYS_RT_SIGRETURN => sys_rt_sigreturn(),
        SYS_LISTEN => sys_listen(arg[0], arg[1]),
        SYS_CONNECT => sys_connect(arg[0], arg[1], arg[2]),
        SYS_ACCEPT => sys_accept(arg[0], arg[1], arg[2]),
        SYS_ACCEPT4 => sys_accept4(arg[0], arg[1], arg[2], arg[3]),
        SYS_SETSOCKOPT => sys_setsockopt(arg[0], arg[1], arg[2], arg[3], arg[4]),
        SYS_GETSOCKOPT => sys_getsockopt(arg[0], arg[1], arg[2], arg[3], arg[4]),
        SYS_SOCKETPAIR => sys_socketpair(arg[0], arg[1], arg[2], arg[3]),
        SYS_SHUTDOWN => sys_shutdown(arg[0], arg[1]),

        _ => {
            error!("Unknown syscall id={}", id);
            BlueErr::ENOSYS.as_isize()
        }
    };
    ret
}
