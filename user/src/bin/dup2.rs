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
    sys_dup2,
};

#[no_mangle]
pub fn main() -> usize {
    // dup2(): redirect stdout(1) to a pipe write end, then verify that writing to fd=1
    // goes into the pipe.

    let mut fds: [i32; 2] = [0; 2];
    let ret = sys_pipe(fds.as_mut_ptr());
    if ret < 0 {
        println!("[dup2] pipe failed, ret={}", ret);
        return 1;
    }
    let rfd = fds[0] as usize;
    let wfd = fds[1] as usize;

    // dup2(old==new) should succeed and not break stdout.
    let same = sys_dup2(1, 1);
    if same != 1 {
        println!("[dup2] dup2(1,1) failed, ret={}", same);
        let _ = sys_close(rfd);
        let _ = sys_close(wfd);
        return 2;
    }

    let newfd = sys_dup2(wfd, 1);
    if newfd != 1 {
        println!("[dup2] dup2(wfd,1) failed, ret={}", newfd);
        let _ = sys_close(rfd);
        let _ = sys_close(wfd);
        return 3;
    }

    // Now fd=1 points to the pipe write end. Close the original pipe write end.
    let _ = sys_close(wfd);

    let msg: [u8; 3] = [b'x', b'y', b'z'];
    let nw = sys_write(1, msg.as_ptr() as usize, msg.len());
    if nw != msg.len() as isize {
        println!("[dup2] write to redirected stdout failed, nw={}", nw);
        let _ = sys_close(1);
        let _ = sys_close(rfd);
        return 4;
    }

    // Close stdout (pipe writer) to signal EOF.
    let _ = sys_close(1);

    let mut buf: [u8; 8] = [0; 8];
    let nr = sys_read(rfd, buf.as_mut_ptr() as usize, msg.len());
    if nr != msg.len() as isize {
        println!("[dup2] read failed, nr={}", nr);
        let _ = sys_close(rfd);
        return 5;
    }
    if &buf[..msg.len()] != &msg {
        println!("[dup2] data mismatch");
        let _ = sys_close(rfd);
        return 6;
    }

    let _ = sys_close(rfd);
    let ok = b"[dup2] ok\n";
    let _ = sys_write(2, ok.as_ptr() as usize, ok.len());
    0
}
