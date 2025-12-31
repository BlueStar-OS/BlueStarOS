#![no_std]
#![no_main]

use core::usize;
extern crate user_lib;
use user_lib::*;
///MKDIR
#[no_mangle]
pub fn main()->usize{
    let argv = args();
    if argv.len() < 2 {
        println!("usage: mkdir <path>");
        return 1;
    }
    let mut p = argv[1].clone();
    p.push('\0');
    let ret = sys_mkdir(&p);
    if ret < 0 {
        println!("mkdir failed, ret={}", ret);
        return 1;
    }
    0
}
