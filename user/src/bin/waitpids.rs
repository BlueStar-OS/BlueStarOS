#![no_std]
#![no_main]

use user_lib::{WNOHANG, print, println, sys_exec, sys_fork, sys_waitpid};
extern crate user_lib;

#[no_mangle]
pub fn main()->usize{
    let pid = sys_fork();
    if pid==0 {
        sys_exec("/test/fork");
    }
    let mut code:isize =42;
    sys_waitpid(&mut code, pid as i32, 0);
    println!("Wait success .child exit with code :{}",code);

    let pid = sys_fork();
    if pid == 0 {
        sys_exec("/test/dup");
    }
    
    sys_waitpid(&mut code,pid as i32, WNOHANG);
    println!("WNHANG Wait success .child exit with code :{} dup pid:{}",code,pid);


    return 0;
}
