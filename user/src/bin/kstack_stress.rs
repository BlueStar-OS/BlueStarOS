#![no_std]
#![no_main]

use user_lib::{print, println, sys_exit, sys_fork, sys_wait};
extern crate user_lib;

#[no_mangle]
pub fn main() -> usize {
    // Stress fork/exit/wait to validate per-task kernel stack recycle.
    // If kernel stacks are not recycled, system should eventually crash/hang.
    let mut i: usize = 0;
    let rounds: usize = 20000;

    while i < rounds {
        let pid = sys_fork();
        if pid == 0 {
            sys_exit(0);
        }

        if pid < 0 {
            println!("fork failed at i={}, pid={}", i, pid);
            return 1;
        }

        let mut code: isize = -1;
        let w = sys_wait(&mut code as *mut isize);
        if w < 0 {
            println!("wait failed at i={}, ret={}, code={}", i, w, code);
            return 2;
        }

        if (i & 0x3f) == 0 {
            print!(".");
        }
        i += 1;
    }

    println!("\nkstack_stress done, rounds={}", rounds);
    0
}
