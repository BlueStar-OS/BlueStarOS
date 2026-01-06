#![no_main]
#![no_std]
#![feature(linkage,panic_info_message,)]
extern crate alloc;
mod panic;
pub mod syscall;
mod console;
pub use alloc::string::String;
use alloc::vec::Vec;
use buddy_system_allocator::LockedHeap;
use core::arch::global_asm;
use crate::alloc::string::ToString;

///用户/kernel结构体
pub const utsname_field_len:usize = 65;//byte
#[repr(C)]
#[derive(Debug)]
pub struct UtsName{
    pub sysname:[u8;utsname_field_len], //当前操作系统名
    pub nodename:[u8;utsname_field_len], //主机名hostname
    pub release:[u8;utsname_field_len], //当前发布级别
    pub version:[u8;utsname_field_len], //内核版本字符串
    pub machine:[u8;utsname_field_len], //当前硬件结构
    pub domainname:[u8;utsname_field_len], //NIS DOMAIN name
}
impl UtsName {
    pub fn new()->Self{
        Self { sysname: [0;utsname_field_len], nodename: [0;utsname_field_len], release: [0;utsname_field_len], version: [0;utsname_field_len],
             machine: [0;utsname_field_len], domainname: [0;utsname_field_len] }
    }
}

///BlueStarOS标准用户库
const USER_HEAP_SIZE:usize=40960;
static mut USER_HEAP_SPACE:[usize;USER_HEAP_SIZE]=[0;USER_HEAP_SIZE];
#[global_allocator]
static mut USER_HEAP_ALLOCTER:LockedHeap=LockedHeap::empty();

global_asm!(r#"
    .section .text.entry
    .globl _start
_start:
    mv a0, sp
    call __user_start
"#);

#[no_mangle]
pub extern "C" fn __user_start(sp0: usize) -> ! {
    unsafe {
        USER_HEAP_ALLOCTER.lock().init(USER_HEAP_SPACE.as_ptr() as usize, USER_HEAP_SIZE);
    }

    init_args_from_stack(sp0);

    let code=main();
    sys_exit(code);
    panic!("_start UnReachBle!");
}

static mut ARGS: Option<Vec<String>> = None;

fn init_args_from_stack(sp: usize) {
    let argc = unsafe { *(sp as *const usize) };
    let mut p = sp + core::mem::size_of::<usize>();

    let mut v: Vec<String> = Vec::new();
    for _ in 0..argc {
        let mut s = String::new();
        loop {
            let b = unsafe { *(p as *const u8) };
            p += 1;
            if b == 0 {
                break;
            }
            s.push(b as char);
        }
        while p % 8 != 0 {
            p += 1;
        }
        v.push(s);
    }
    unsafe {
        ARGS = Some(v);
    }
}

pub fn argc() -> usize {
    unsafe { ARGS.as_ref().map(|v| v.len()).unwrap_or(0) }
}

pub fn arg(i: usize) -> Option<&'static str> {
    unsafe {
        ARGS.as_ref()
            .and_then(|v| v.get(i))
            .map(|s| s.as_str())
    }
}

pub fn args() -> &'static [String] {
    unsafe {
        match ARGS.as_ref() {
            Some(v) => v.as_slice(),
            None => &[],
        }
    }
}



#[linkage ="weak"]
#[no_mangle]
fn main()->usize{
  return 1;
}

pub fn getchar()->char{
   let mut ch: u8 = 0;
   let _ = syscall::sys_read(FD_TYPE_STDIN, &mut ch as *mut u8 as usize, 1);
   ch as char
}

pub fn readline(ptr:usize,len:usize)->isize{//返回读取的字符数量 目前实现比较原始，后期封装
  syscall::sys_read(FD_TYPE_STDIN, ptr, len)
}

pub fn map(start:usize,len:usize)->isize{
  syscall::sys_map(start, len)
}

pub fn unmap(start:usize,len:usize)->isize{
  syscall::sys_unmap(start, len)
}

pub fn chdir(path: &str) -> isize {
    syscall::sys_chdir(path)
}

pub fn getcwd() -> Option<String> {
    let mut buf: [u8; 256] = [0u8; 256];
    let ret = syscall::sys_getcwd(buf.as_mut_ptr() as usize, buf.len());
    if ret <= 0 {
        return None;
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    Some(String::from_utf8_lossy(&buf[..end]).to_string())
}


use crate::panic::panic;

pub use self::syscall::*;
pub use self::console::*;
