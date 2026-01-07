#![no_std]
#![no_main]

extern crate user_lib;
extern crate alloc;
use user_lib::print;
use user_lib::{
    println,
    sys_pipe,
    sys_fork,
    sys_read,
    sys_write,
    sys_close,
    sys_wait,
    sys_exit,
};

#[no_mangle]
pub fn main() -> usize {
    let mut fds: [i32; 2] = [0; 2];
    let ret = sys_pipe(fds.as_mut_ptr());
    if ret < 0 {
        println!("[pipe_test] pipe failed, ret={}", ret);
        return 1;
    }
    let rfd = fds[0] as usize;
    let wfd = fds[1] as usize;

    let pid = sys_fork();
    if pid < 0 {
        println!("[pipe_test] fork failed, ret={}", pid);
        let _ = sys_close(rfd);
        let _ = sys_close(wfd);
        return 2;
    }

    if pid == 0 {
        print!("I'm child proc:pipe fd: read:{} write:{} \n",fds[0],fds[1]);
        let _ = sys_close(rfd);

        let msg: &[u8] = b"hello-pipe";
        let n = sys_write(wfd, msg.as_ptr() as usize, msg.len());
        if n != msg.len() as isize {
            println!("[pipe_test] child write failed, n={}", n);
            let _ = sys_close(wfd);
            sys_exit(11);
        }

        let _ = sys_close(wfd);
        sys_exit(0);
    }

    let _ = sys_close(wfd);

    let mut buf: [u8; 32] = [0; 32];
    let n = sys_read(rfd, buf.as_mut_ptr() as usize, buf.len());
    if n <= 0 {
        println!("[pipe_test] parent read failed, n={}", n);
        let _ = sys_close(rfd);
        return 3;
    }

    let expected: &[u8] = b"hello-pipe";
    if n as usize != expected.len() || &buf[..expected.len()] != expected {
        println!("[pipe_test] content mismatch, n={}", n);
        let _ = sys_close(rfd);
        return 4;
    }

    let n2 = sys_read(rfd, buf.as_mut_ptr() as usize, buf.len());
    if n2 != 0 {
        println!("[pipe_test] expected EOF(0), got n2={}", n2);
        let _ = sys_close(rfd);
        return 5;
    }

    let _ = sys_close(rfd);

    let mut code: isize = 0;
    let waited = sys_wait(&mut code as *mut isize);
    if waited < 0 {
        println!("[pipe_test] wait failed, ret={}", waited);
        return 6;
    }
    if code != 0 {
        println!("[pipe_test] child exit code={}, pid={}", code, waited);
        return 7;
    }

    let mut fds2: [i32; 2] = [0; 2];
    let ret2 = sys_pipe(fds2.as_mut_ptr());
    if ret2 < 0 {
        println!("[pipe_test] pipe2 failed, ret={}", ret2);
        return 8;
    }
    let r2 = fds2[0] as usize;
    let w2 = fds2[1] as usize;

    let _ = sys_close(r2);
    let one: [u8; 1] = [b'x'];
    let nw = sys_write(w2, one.as_ptr() as usize, 1);
    if nw >= 0 {
        println!("[pipe_test] expected write error when no reader, got nw={}", nw);
        let _ = sys_close(w2);
        return 9;
    }
    let _ = sys_close(w2);

    println!("[pipe_test] ok");
    0
}
