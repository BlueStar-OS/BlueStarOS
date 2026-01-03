#![no_std]
#![no_main]

extern crate user_lib;

use user_lib::{
    args,
    print,
    println,
    sys_close,
    sys_open,
    sys_read,
    sys_write,
    O_RDONLY,
};

#[no_mangle]
pub fn main() -> usize {
    let argv = args();
    if argv.len() < 2 {
        println!("usage: cat <path> [path...]");
        return 1;
    }

    let mut exit_code = 0usize;
    let mut buf = [0u8; 512];

    for i in 1..argv.len() {
        let p = argv[i].clone();
        let fd = sys_open(&p, O_RDONLY);
        if fd < 0 {
            println!("cat: open failed: {}", argv[i]);
            exit_code = 1;
            continue;
        }

        loop {
            let n = sys_read(fd as usize, buf.as_mut_ptr() as usize, buf.len());
            if n < 0 {
                println!("cat: read failed: {}", argv[i]);
                exit_code = 1;
                break;
            }
            if n == 0 {
                break;
            }
            let wrote = sys_write(1, buf.as_ptr() as usize, n as usize);
            if wrote < 0 {
                print!("cat: write failed\n");
                exit_code = 1;
                break;
            }
        }

        let _ = sys_close(fd as usize);
    }
    exit_code
}