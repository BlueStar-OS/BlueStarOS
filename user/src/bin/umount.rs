#![no_std]
#![no_main]

extern crate user_lib;
use user_lib::print;
use user_lib::{args, println};
use user_lib::syscall::sys_umount2;

#[no_mangle]
pub fn main() -> usize {
    // usage: umount <target>
    let argv = args();
    if argv.len() != 2 {
        println!("usage: umount <target>");
        return 1;
    }

    let target = argv[1].as_str();
    let ret = sys_umount2(target, 0);
    if ret < 0 {
        println!("umount failed ret={} target={}", ret, target);
        return 1;
    }
    0
}
