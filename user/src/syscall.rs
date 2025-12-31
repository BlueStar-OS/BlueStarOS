use crate::print;

pub const GET_TIME:usize   =0;     //获取系统时间
pub const SYS_WRITE:usize  =1;     //stdin write系统调用
pub const SYS_READ:usize   =2;     //stdin read系统调用
pub const SYS_EXIT:usize   =3;     //exit程序结束，运行下一个程序
pub const SYS_YIELD:usize  =4;     //主动放弃cpu
pub const SYS_MAP:usize    =5;     //mmap映射系统调用
pub const SYS_UNMAP:usize  =6;     //unmap映射系统调用
pub const SYS_OPEN:usize   =7;     //open
pub const SYS_CLOSE:usize  =8;     //close
pub const SYS_LSEEK:usize  =9;     //lseek
pub const SYS_FORK:usize   =10;    //fork系统调用
pub const SYS_EXEC:usize   =11;    //exec系统调用
pub const SYS_WAIT:usize   =12;    //wait系统调用
pub const SYS_CREAT:usize  =13;    //creat
pub const SYS_MKDIR:usize  =14;    //mkdir
pub const SYS_UNLINK:usize =15;    //unlink
pub const SYS_STAT:usize   =16;    //stat
pub const SYS_GETDENTS64:usize =17; //getdents64
pub const SYS_PIPE:usize   =18;    //pipe
///syscall封装 3个参数版本
pub fn sys_call(id: usize, args: [usize; 3]) -> isize {
    let mut ret: isize;
    unsafe {
        core::arch::asm!(
            "ecall",
            inlateout("x10") args[0] => ret,
            in("x11") args[1],
            in("x12") args[2],
            in("x17") id
        );
    }
    ret
}

pub fn sys_map(startAddr:usize,len:usize)->isize{
    sys_call(SYS_MAP,[startAddr,len,0])
}

pub fn sys_unmap(startAddr:usize,len:usize)->isize{
    sys_call(SYS_UNMAP,[startAddr,len,0])
}

pub fn sys_read(fd:usize,buffer_ptr:usize,buffer_len:usize)->isize{
    sys_call(SYS_READ, [fd,buffer_ptr,buffer_len])
}

pub fn sys_write(fd:usize,buffer_ptr:usize,buffer_len:usize)->isize{
    sys_call(SYS_WRITE, [fd,buffer_ptr,buffer_len])
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
    sys_call(SYS_OPEN, [path.as_ptr() as usize, flags, 0])
}

pub fn sys_creat(path: &str) -> isize {
    sys_call(SYS_CREAT, [path.as_ptr() as usize, 0, 0])
}

pub fn sys_mkdir(path: &str) -> isize {
    sys_call(SYS_MKDIR, [path.as_ptr() as usize, 0, 0])
}

pub fn sys_unlink(path: &str) -> isize {
    sys_call(SYS_UNLINK, [path.as_ptr() as usize, 0, 0])
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VfsStat {
    pub inode: u32,
    pub size: u64,
    pub mode: u32,
    pub file_type: u32,
}

pub fn sys_stat(path: &str, stat_buf: *mut VfsStat) -> isize {
    sys_call(SYS_STAT, [path.as_ptr() as usize, stat_buf as usize, 0])
}

pub fn sys_getdents64(fd: usize, buf_ptr: usize, buf_len: usize) -> isize {
    sys_call(SYS_GETDENTS64, [fd, buf_ptr, buf_len])
}

pub fn sys_close(fd: usize) -> isize {
    sys_call(SYS_CLOSE, [fd, 0, 0])
}

pub fn sys_lseek(fd: usize, offset: isize, whence: usize) -> isize {
    sys_call(SYS_LSEEK, [fd, offset as usize, whence])
}

pub fn sys_fork()->isize{
    sys_call(SYS_FORK, [0,0,0])//args为空，fork不需要参数
}
pub fn sys_exec(path:&str)->isize{
    sys_call(SYS_EXEC, [path.as_ptr() as usize,0,0])
}

pub fn sys_exec_args(path: &str, argv_ptrs: *const usize, argc: usize) -> isize {
    sys_call(
        SYS_EXEC,
        [path.as_ptr() as usize, argv_ptrs as usize, argc],
    )
}

pub fn sys_pipe(fds_ptr: *mut usize) -> isize {
    sys_call(SYS_PIPE, [fds_ptr as usize, 0, 0])
}

///wait任意子进程，返回回收的子进程pid，-1表示无子进程或错误
///exit_code_ptr: 若非空，内核会写回子进程退出码
pub fn sys_wait(exit_code_ptr: *mut isize)->isize{
    sys_call(SYS_WAIT, [exit_code_ptr as usize,0,0])
}

///永远不返回 里面有loop封装为！
pub fn sys_exit(exit_code:usize)->!{//sys_exit 
    sys_call(SYS_EXIT, [exit_code,0,0]);
    loop {
        
    }
}

///主动放弃一次cpu
pub fn sys_yield(){
    sys_call(SYS_YIELD, [0;3]);
}

