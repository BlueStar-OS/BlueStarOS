#![no_std]
#![no_main]

extern crate alloc;

use user_lib::{print, println, utsname_field_len};
use user_lib::UtsName;
use user_lib::sys_uname;

fn cstr_field_to_str(field: &[u8; utsname_field_len]) -> &str {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    core::str::from_utf8(&field[..end]).unwrap_or("<non-utf8>")
}

#[no_mangle]
fn main() -> i32 {
    let mut u = UtsName::new();
    let ret = sys_uname(&mut u);
    if ret < 0 {
        println!("uname syscall failed ret={}", ret);
        return -1;
    }

    println!("sysname:   {}", cstr_field_to_str(&u.sysname));
    println!("nodename:  {}", cstr_field_to_str(&u.nodename));
    println!("release:   {}", cstr_field_to_str(&u.release));
    println!("version:   {}", cstr_field_to_str(&u.version));
    println!("machine:   {}", cstr_field_to_str(&u.machine));
    println!("domainname:{}", cstr_field_to_str(&u.domainname));

    0
}
