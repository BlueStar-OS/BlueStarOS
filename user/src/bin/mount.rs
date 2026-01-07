#![no_std]
#![no_main]

extern crate user_lib;
use user_lib::print;
use user_lib::{args, println};
use user_lib::syscall::sys_mount;

#[no_mangle]
pub fn main() -> usize {
    // usage: mount <source> <target> [fstype]
    // fstype: auto|ext4|fat32
    let argv = args();
    if argv.len() < 3 {
        println!("usage: mount <source> <target> [fstype]");
        return 1;
    }

    let source = argv[1].as_str();
    let target = argv[2].as_str();
    let fstype = if argv.len() >= 4 { argv[3].as_str() } else { "auto" };

    let ret = sys_mount(source, target, fstype, 0, "");
    if ret < 0 {
        println!("mount failed ret={} source={} target={} fstype={}", ret, source, target, fstype);
        return 1;
    }
    0
}
