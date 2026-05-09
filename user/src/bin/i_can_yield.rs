#![no_std]
#![no_main]

use core::usize;
use user_lib::sys_yield;
use user_lib::{getchar, print, println, readline, StdinBuffer, String};
extern crate user_lib;

#[no_mangle]
pub fn main() -> usize {
    for i in 0..1 {
        sys_yield();
        println!("YIELD!");
    }
    return 0;
}
