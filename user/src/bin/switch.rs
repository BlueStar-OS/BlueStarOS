#![no_std]
#![no_main]

use user_lib::{print, println};
extern crate user_lib;

#[no_mangle]
pub fn main() -> usize {
    println!("If you see this ,switch is runing Success!");
    return 0;
}
