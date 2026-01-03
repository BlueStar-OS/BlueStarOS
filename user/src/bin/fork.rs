#![no_std]
#![no_main]

use core::usize;

use user_lib::{StdinBuffer, String, getchar, print, println, readline, sys_exec, sys_fork, sys_wait, sys_yield};
extern crate user_lib;

#[no_mangle]
pub fn main() -> usize {
    print!("Will fork\n");
    let pids = sys_fork();
    println!("fork ret = {}", pids);
    if pids == 0 {
        let ret = sys_exec("/test/printf");
        println!("exec failed, ret={}", ret);
        return 1;
    } else if pids > 0 {
        println!("parent, child pid = {},will wait", pids);
        let mut code: isize = 0;
        let _ = sys_wait(&mut code as *mut isize);
        return 0;
    }
    print!("return ??? your pid is :{}",pids);
    2
}
