#![no_std]
#![no_main]

use core::usize;
use user_lib::sys_yield;
use user_lib::{getchar, print, println, readline, StdinBuffer, String};
extern crate user_lib;

#[no_mangle]
pub fn main() -> usize {
    let mut i = 0;
    println!("Yes:{}", i);
    for i in 0..1 {
        println!("this:{}", i);
    }

    sys_yield();

    for i in 0..1 {
        println!("this3:{}", i);
    }

    return 0;
}
