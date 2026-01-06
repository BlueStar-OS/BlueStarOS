use crate::{UtsName, print};
use alloc::string::String;
use bitflags::bitflags;

// Linux riscv64 syscall numbers (subset)
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

pub const AT_FDCWD: isize = -100;

/// syscall 封装：Linux ABI 版本（最多 6 个参数）
pub fn sys_call(id: usize, args: [usize; 6]) -> isize {
    let mut ret: isize;
    unsafe {
        core::arch::asm!(
            "ecall",
            inlateout("x10") args[0] => ret,
            in("x11") args[1],
            in("x12") args[2],
            in("x13") args[3],
            in("x14") args[4],
            in("x15") args[5],
            in("x17") id
        );
    }
    ret
}


pub fn sys_uname(buf: &mut UtsName) -> isize {
    sys_call(SYS_UNAME, [buf as *mut _ as usize, 0, 0, 0, 0, 0])
}

pub fn sys_mount(source: &str, target: &str, fstype: &str, flags: usize, data: &str) -> isize {
    let mut s_source = String::from(source);
    s_source.push('\0');
    let mut s_target = String::from(target);
    s_target.push('\0');
    let mut s_fstype = String::from(fstype);
    s_fstype.push('\0');
    let mut s_data = String::from(data);
    s_data.push('\0');
    sys_call(
        SYS_MOUNT,
        [
            s_source.as_ptr() as usize,
            s_target.as_ptr() as usize,
            s_fstype.as_ptr() as usize,
            flags,
            s_data.as_ptr() as usize,
            0,
        ],
    )
}

pub fn sys_umount2(target: &str, flags: usize) -> isize {
    let mut s_target = String::from(target);
    s_target.push('\0');
    sys_call(SYS_UMOUNT2, [s_target.as_ptr() as usize, flags, 0, 0, 0, 0])
}

bitflags! {
    pub struct MmapProt: usize {
        const READ = 0x1;
        const WRITE = 0x2;
        const EXEC = 0x4;
    }
}

bitflags! {
    pub struct MmapFlags: usize {
        const SHARED = 0x01;
        const PRIVATE = 0x02;
        const FIXED = 0x10;
        const ANONYMOUS = 0x20;
    }
}

pub fn sys_mmap(addr: usize, len: usize, prot: usize, flags: usize, fd: isize, offset: usize) -> isize {
    sys_call(
        SYS_MMAP,
        [addr, len, prot, flags, fd as usize, offset],
    )
}

pub fn sys_map(startAddr:usize,len:usize)->isize{
    // Keep compatibility with old tests: anonymous private RW mapping at fixed hint address.
    sys_mmap(
        startAddr,
        len,
        (MmapProt::READ | MmapProt::WRITE).bits(),
        (MmapFlags::PRIVATE | MmapFlags::ANONYMOUS).bits(),
        -1,
        0,
    )
}

pub fn sys_unmap(startAddr:usize,len:usize)->isize{
    sys_call(SYS_MUNMAP,[startAddr,len,0,0,0,0])
}

pub fn sys_read(fd:usize,buffer_ptr:usize,buffer_len:usize)->isize{
    sys_call(SYS_READ, [fd,buffer_ptr,buffer_len,0,0,0])
}

pub fn sys_write(fd:usize,buffer_ptr:usize,buffer_len:usize)->isize{
    sys_call(SYS_WRITE, [fd,buffer_ptr,buffer_len,0,0,0])
}

pub fn sys_dup(oldfd: usize) -> isize {
    sys_call(SYS_DUP, [oldfd, 0, 0, 0, 0, 0])
}

pub fn sys_dup2(oldfd: usize, newfd: usize) -> isize {
    // dup2 is commonly implemented via dup3(old, new, flags=0)
    sys_call(SYS_DUP3, [oldfd, newfd, 0, 0, 0, 0])
}

pub fn sys_getpid() -> isize {
    sys_call(SYS_GETPID, [0, 0, 0, 0, 0, 0])
}

pub fn sys_getppid() -> isize {
    sys_call(SYS_GETPPID, [0, 0, 0, 0, 0, 0])
}

pub const O_RDONLY: usize = 0;
pub const O_WRONLY: usize = 1;
pub const O_RDWR: usize = 2;
pub const O_CREAT: usize = 1 << 6;
pub const O_TRUNC: usize = 1 << 9;
pub const O_APPEND: usize = 1 << 10;

pub const SEEK_SET: usize = 0;
pub const SEEK_CUR: usize = 1;
pub const SEEK_END: usize = 2;

pub fn sys_open(path: &str, flags: usize) -> isize {
    let mut st = String::from(path);
    st.push('\0');
    sys_call(
        SYS_OPENAT,
        [
            AT_FDCWD as usize,
            st.as_ptr() as usize,
            flags,
            0,
            0,
            0,
        ],
    )
}

pub fn sys_creat(path: &str) -> isize {
    let mut st = String::from(path);
    st.push('\0');
    sys_call(
        SYS_OPENAT,
        [
            AT_FDCWD as usize,
            st.as_ptr() as usize,
            (1 << 6) | (1 << 9) | 1,
            0,
            0,
            0,
        ],
    )
}

pub fn sys_mkdir(path: &str) -> isize {
    let mut st = String::from(path);
    st.push('\0');
    sys_call(
        SYS_MKDIRAT,
        [AT_FDCWD as usize, st.as_ptr() as usize, 0, 0, 0, 0],
    )
}

pub fn sys_unlink(path: &str) -> isize {
    let mut st = String::from(path);
    st.push('\0');
    sys_call(
        SYS_UNLINKAT,
        [AT_FDCWD as usize, st.as_ptr() as usize, 0, 0, 0, 0],
    )
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct KStat {
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_mode: u32,
    pub st_nlink: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub st_rdev: u64,
    pub __pad: u64,
    pub st_size: i64,
    pub st_blksize: u32,
    pub __pad2: i32,
    pub st_blocks: u64,
    pub st_atime_sec: i64,
    pub st_atime_nsec: i64,
    pub st_mtime_sec: i64,
    pub st_mtime_nsec: i64,
    pub st_ctime_sec: i64,
    pub st_ctime_nsec: i64,
    pub __unused: [u32; 2],
}

pub fn sys_stat(path: &str, stat_buf: *mut KStat) -> isize {
    let mut st = String::from(path);
    st.push('\0');
    // Use newfstatat(AT_FDCWD, path, stat_buf, flags=0) so we can stat by path.
    sys_call(
        SYS_NEWFSTATAT,
        [
            AT_FDCWD as usize,
            st.as_ptr() as usize,
            stat_buf as usize,
            0,
            0,
            0,
        ],
    )
    
}

pub fn sys_fstat(fd: usize, stat_buf: *mut KStat) -> isize {
    sys_call(SYS_FSTAT, [fd, stat_buf as usize, 0, 0, 0, 0])
}

pub fn sys_getdents64(fd: usize, buf_ptr: usize, buf_len: usize) -> isize {
    sys_call(SYS_GETDENTS64, [fd, buf_ptr, buf_len, 0, 0, 0])
}

pub fn sys_close(fd: usize) -> isize {
    sys_call(SYS_CLOSE, [fd, 0, 0, 0, 0, 0])
}

pub fn sys_lseek(fd: usize, offset: isize, whence: usize) -> isize {
    sys_call(SYS_LSEEK, [fd, offset as usize, whence, 0, 0, 0])
}

pub fn sys_fork()->isize{
    sys_call(SYS_CLONE, [0, 0, 0, 0, 0, 0])
}
pub fn sys_exec(path:&str)->isize{
    let mut st = String::from(path);
    st.push('\0');
    sys_call(SYS_EXECVE, [st.as_ptr() as usize, 0, 0, 0, 0, 0])
}

pub fn sys_exec_args(path: &str, argv_ptrs: *const usize, argc: usize) -> isize {
    let mut st = String::from(path);
    st.push('\0');
    // NOTE: kernel currently treats SYS_EXECVE as sys_exec(path, argv, argc).
    // Pass argc in a2 to keep existing argv parsing working.
    sys_call(
        SYS_EXECVE,
        [st.as_ptr() as usize, argv_ptrs as usize, argc, 0, 0, 0],
    )
}

pub fn sys_pipe(fds_ptr: *mut i32) -> isize {
    // pipe2(fds, flags=0)
    sys_call(SYS_PIPE2, [fds_ptr as usize, 0, 0, 0, 0, 0])
}

pub fn sys_chdir(path: &str) -> isize {
    let mut p = String::from(path);
    p.push('\0');
    sys_call(SYS_CHDIR, [p.as_ptr() as usize, 0, 0, 0, 0, 0])
}

pub fn sys_getcwd(buf_ptr: usize, buf_len: usize) -> isize {
    sys_call(SYS_GETCWD, [buf_ptr, buf_len, 0, 0, 0, 0])
}

///wait任意子进程，返回回收的子进程pid，-1表示无子进程或错误
///exit_code_ptr: 若非空，内核会写回子进程退出码
pub fn sys_wait(exit_code_ptr: *mut isize)->isize{
    // wait4(pid=-1, wstatus, options=0, rusage=NULL)
    sys_call(SYS_WAIT4, [usize::MAX, exit_code_ptr as usize, 0, 0, 0, 0])
}

///永远不返回 里面有loop封装为！
pub fn sys_exit(exit_code:usize)->!{//sys_exit 
    sys_call(SYS_EXIT, [exit_code,0,0,0,0,0]);
    loop {
        
    }
}

///主动放弃一次cpu
pub fn sys_yield(){
    sys_call(SYS_SCHED_YIELD, [0,0,0,0,0,0]);
}

