#![no_std]
#![no_main]

extern crate user_lib;
use user_lib::*;

#[no_mangle]
pub fn main() -> usize {
    let argv = args();
    if argv.len() < 2 {
        println!("usage: rm [-f] <path>...");
        return 1;
    }

    let mut force = false;
    let mut i = 1;
    while i < argv.len() {
        let a = argv[i].as_str();
        if a == "-f" {
            force = true;
            i += 1;
            continue;
        }
        if a.starts_with('-') {
            println!("rm: unknown option: {}", a);
            return 1;
        }
        break;
    }

    if i >= argv.len() {
        println!("usage: rm [-f] <path>...");
        return 1;
    }

    let mut ok = true;
    while i < argv.len() {
        let path = argv[i].as_str();
        let ret = sys_unlink(path);
        if ret < 0 {
            ok = false;
            if !force {
                println!("rm: {}: unlink failed, ret={}", path, ret);
            }
        }
        i += 1;
    }

    if ok || force {
        0
    } else {
        1
    }
}
