#![no_std]
#![no_main]

extern crate user_lib;
use user_lib::print;
use user_lib::{args, println};
use user_lib::syscall::{sys_clone, sys_getpid, sys_wait, sys_exit};

#[no_mangle]
pub fn main() -> usize {
    let _ = args();

    // Minimal clone test: clone(0,0,0,0,0) behaves like fork.
    let ret = sys_clone(0, 0, 0, 0, 0);
    if ret < 0 {
        println!("clone_test: sys_clone failed ret={}", ret);
        return 1;
    }

    if ret == 0 {
        // child
        let pid = sys_getpid();
        println!("clone_test: child pid={}", pid);
        sys_exit(7);
    }

    // parent
    let child_pid = ret;
    println!("clone_test: parent pid={} child_pid={}", sys_getpid(), child_pid);

    let mut status: isize = -1;
    let waited = sys_wait(&mut status as *mut isize);
    if waited < 0 {
        println!("clone_test: wait failed ret={}", waited);
        return 2;
    }

    println!("clone_test: wait ok pid={} status={}", waited, status);
    0
}
