#![no_std]
#![no_main]
use user_lib::print;
extern crate user_lib;

use user_lib::{args, chdir, println};

#[no_mangle]
pub fn main() -> usize {
    let a = args();
    if a.len() != 2 {
        println!("usage: cd <abs_path>");
        return 1;
    }
    let ret = chdir(a[1].as_str());
    if ret < 0 {
        println!("cd failed, ret={}", ret);
        return 1;
    }
    0
}
