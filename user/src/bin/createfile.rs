#![no_std]
#![no_main]
//read 和 write系统调用
use core::usize;
use user_lib::String;
use user_lib::print;
use user_lib::println;
use user_lib::{
    O_RDONLY, SEEK_SET, sys_close, sys_creat, sys_lseek, sys_open, sys_read, sys_write,
};
extern crate user_lib;

#[no_mangle]
pub fn main() -> usize {
    let path = "/test/hello.txt\0";
    let msg = b"hello from BlueStarOS\n";

    println!("[fs] creat: {}", path);
    let fd = sys_creat(path);
    if fd < 0 {
        println!("[fs] creat failed, ret={}", fd);
        return 1;
    }
    println!("[fs] creat ok, fd={}", fd);

    let nw = sys_write(fd as usize, msg.as_ptr() as usize, msg.len());
    println!("[fs] write ret={}", nw);
    if nw as usize != msg.len() {
        println!("[fs] write size mismatch");
        let _ = sys_close(fd as usize);
        return 2;
    }

    let c = sys_close(fd as usize);
    println!("[fs] close ret={}", c);
    if c < 0 {
        return 3;
    }

    println!("[fs] reopen with open(O_RDONLY): {}", path);
    let fd2 = sys_open(path, O_RDONLY);
    println!("[fs] open ret={}", fd2);
    if fd2 < 0 {
        return 4;
    }

    let off2 = sys_lseek(fd2 as usize, 0, SEEK_SET);
    println!("[fs] lseek2(0,SEEK_SET) ret={}", off2);
    if off2 < 0 {
        let _ = sys_close(fd2 as usize);
        return 5;
    }

    let mut buf2 = [0u8; 64];
    let nr2 = sys_read(fd2 as usize, buf2.as_mut_ptr() as usize, buf2.len());
    println!("[fs] read2 ret={}", nr2);
    let _ = sys_close(fd2 as usize);
    if nr2 < 0 {
        return 6;
    }
    if nr2 as usize != msg.len() || &buf2[..msg.len()] != msg {
        println!("[fs] reopen read mismatch");
        return 7;
    }


    println!("read data:{:?}",String::from_utf8(buf2.to_vec()));

    println!("[fs] PASS");
    0
}
