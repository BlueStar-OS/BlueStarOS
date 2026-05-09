#![no_std]
#![no_main]

use user_lib::{print, println};
extern crate user_lib;

#[no_mangle]
pub fn main() -> usize {
    println!("Hello,World!,This a sys_write test");
    return 0;
}
