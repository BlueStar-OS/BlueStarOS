#![no_main]
#![no_std]
#![feature(linkage,panic_info_message,)]
extern crate alloc;
mod panic;
mod syscall;
mod console;
pub use alloc::string::String;
use alloc::vec::Vec;
use buddy_system_allocator::LockedHeap;
use core::arch::global_asm;
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


use crate::panic::panic;

pub use self::syscall::*;
pub use self::console::*;
