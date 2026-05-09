#![no_std]
#![no_main]

extern crate alloc;

use user_lib::sys_uname;
use user_lib::MmapFlags;
use user_lib::MmapProt;
use user_lib::UtsName;
use user_lib::{print, println, sys_mmap, utsname_field_len};

#[no_mangle]
fn main() -> usize {
    let addr = 0x6000;

    0
}
