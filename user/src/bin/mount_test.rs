#![no_std]
#![no_main]

extern crate user_lib;

use user_lib::{args, println,print};
use user_lib::syscall::{sys_mkdir, sys_mount, sys_open, sys_getdents64, sys_close, O_RDONLY};

#[no_mangle]
pub fn main() -> usize {
    // usage: mount_test [target]
    let argv = args();
    let target = if argv.len() >= 2 { argv[1].as_str() } else { "/mnt/ext4" };

    // Ensure mountpoint directories exist.
    let _ = sys_mkdir("/mnt");
    let _ = sys_mkdir(target);

    // Mount ext4.
    let ret = sys_mount("", target, "ext4", 0, "");
    if ret < 0 {
        println!("mount failed ret={} target={}", ret, target);
        return 1;
    }
    println!("mount ok target={}", target);

    // Verify by opening the directory and calling getdents64.
    let fd = sys_open(target, O_RDONLY);
    if fd < 0 {
        println!("open mountpoint failed fd={} target={}", fd, target);
        return 2;
    }

    let mut buf = [0u8; 4096];
    let n = sys_getdents64(fd as usize, buf.as_mut_ptr() as usize, buf.len());
    let _ = sys_close(fd as usize);

    if n < 0 {
        println!("getdents64 failed ret={} target={}", n, target);
        return 3;
    }

    println!("getdents64 ok n={} target={}", n, target);
    0
}
