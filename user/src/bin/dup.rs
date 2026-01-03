#![no_std]
#![no_main]

extern crate user_lib;
extern crate alloc;
use user_lib::print;
use user_lib::{
    println,
    sys_pipe,
    sys_close,
    sys_write,
    sys_read,
    sys_dup,
};

#[no_mangle]
pub fn main() -> usize {
    // dup() should create a new fd pointing to the same open file description.
    // Here we validate that dup() keeps a pipe write end usable after closing the original.
    let mut fds: [i32; 2] = [0; 2];
    let ret = sys_pipe(fds.as_mut_ptr());
    if ret < 0 {
        println!("[dup] pipe failed, ret={}", ret);
        return 1;
    }
    let rfd = fds[0] as usize;
    let wfd = fds[1] as usize;

    let wfd2 = sys_dup(wfd) as isize;
    if wfd2 < 0 {
        println!("[dup] dup failed, ret={}", wfd2);
        let _ = sys_close(rfd);
        let _ = sys_close(wfd);
        return 2;
    }
    let wfd2 = wfd2 as usize;

    // Close original writer, keep duplicated writer.
    let _ = sys_close(wfd);

    let msg: [u8; 3] = [b'a', b'b', b'c'];
    let nw = sys_write(wfd2, msg.as_ptr() as usize, msg.len());
    if nw != msg.len() as isize {
        println!("[dup] write via dup fd failed, nw={}", nw);
        let _ = sys_close(wfd2);
        let _ = sys_close(rfd);
        return 3;
    }

    // Close writer and read back.
    let _ = sys_close(wfd2);

    let mut buf: [u8; 8] = [0; 8];
    let nr = sys_read(rfd, buf.as_mut_ptr() as usize, msg.len());
    if nr != msg.len() as isize {
        println!("[dup] read failed, nr={}", nr);
        let _ = sys_close(rfd);
        return 4;
    }
    if &buf[..msg.len()] != &msg {
        println!("[dup] data mismatch");
        let _ = sys_close(rfd);
        return 5;
    }

    let _ = sys_close(rfd);
    println!("[dup] ok");
    0
}
