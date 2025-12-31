mod syscall;
use log::error;

use crate::syscall::syscall::*;
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
///id: 系统调用号
///args:接受1个usize参数
///返回值：通过 x10 (a0) 寄存器返回给用户态
pub fn syscall_handler(id:usize,arg:[usize;3]) -> isize {
    match id {
        GET_TIME => {
            0  // 暂未实现
        }
        SYS_WRITE => {
            ///bufferpoint fd_type buffer_len
            sys_write(arg[0], arg[1], arg[2])
        }
        SYS_READ => {
            sys_read(arg[0], arg[1], arg[2])
        }
        SYS_EXIT=>{
            //error!("exit call");
            sys_exit(arg[0])
        }
        SYS_YIELD=>{
            sys_yield()
        }
        SYS_MAP=>{
            sys_map(arg[0], arg[1])
        }
        SYS_UNMAP=>{
            sys_unmap(arg[0], arg[1])
        }
        SYS_OPEN=>{
            sys_open(arg[0], arg[1])
        }
        SYS_CLOSE=>{
            sys_close(arg[0])
        }
        SYS_LSEEK=>{
            sys_lseek(arg[0], arg[1] as isize, arg[2])
        }
        SYS_FORK=>{
            sys_fork()
        }
        SYS_EXEC=>{
            sys_exec(arg[0],arg[1],arg[2])
        }

        SYS_WAIT=>{
            sys_wait(arg[0])
        }

        SYS_CREAT=>{
            sys_creat(arg[0])
        }

        SYS_MKDIR => {
            sys_mkdir(arg[0])
        }

        SYS_UNLINK => {
            sys_unlink(arg[0])
        }

        SYS_STAT => {
            sys_stat(arg[0], arg[1])
        }

        SYS_GETDENTS64 => {
            sys_getdents64(arg[0], arg[1], arg[2])
        }
        
        SYS_PIPE => {
            sys_pipe(arg[0])
        }
        
        _ => {
            panic!("Unknown Syscall type: {}", id);
        }
    }
}