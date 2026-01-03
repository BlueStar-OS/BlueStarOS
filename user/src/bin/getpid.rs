#![no_std]
#![no_main]

extern crate user_lib;
use user_lib::print;
use user_lib::{println, sys_getpid, sys_getppid, sys_fork, sys_wait, sys_exit};

#[no_mangle]
pub fn main() -> usize {
    let pid = sys_getpid();
    let ppid = sys_getppid();
    println!("[getpid] pid={} ppid={}", pid, ppid);

    let cpid = sys_fork();
    if cpid < 0 {
        println!("[getpid] fork failed, ret={}", cpid);
        return 1;
    }

    if cpid == 0 {
        let my = sys_getpid();
        let myppid = sys_getppid();
        println!("[getpid-child] pid={} ppid={}", my, myppid);
        // child should observe ppid == parent's pid (best effort)
        sys_exit(0);
    }

    let mut code: isize = 0;
    let w = sys_wait(&mut code as *mut isize);
    println!("[getpid-parent] waited={} code={}", w, code);
    0
}
