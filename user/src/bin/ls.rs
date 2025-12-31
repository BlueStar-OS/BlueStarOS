#![no_std]
#![no_main]

extern crate user_lib;
use user_lib::print;
use user_lib::{
    args,
    String,
    println,
    O_RDONLY,
    sys_close,
    sys_getdents64,
    sys_open,
};

const DT_DIR: u8 = 4;
const DT_REG: u8 = 8;
const DT_LNK: u8 = 10;

const COLOR_BLUE: &str = "\x1b[34m";
const COLOR_RED: &str = "\x1b[31m";
const COLOR_RESET: &str = "\x1b[0m";

#[inline]
fn c_strlen(ptr: *const u8) -> usize {
    let mut n = 0usize;
    unsafe {
        while *ptr.add(n) != 0 {
            n += 1;
        }
    }
    n
}

#[inline]
fn dt_name(dt: u8) -> &'static str {
    match dt {
        DT_DIR => "dir",
        DT_REG => "file",
        DT_LNK => "link",
        _ => "unk",
    }
}

#[inline]
fn print_name(dt: u8, name: &str) {
    match dt {
        DT_DIR => {
            print!("{}{}{}", COLOR_BLUE, name, COLOR_RESET);
        }
        DT_LNK => {
            print!("{}{}{}", COLOR_RED, name, COLOR_RESET);
        }
        _ => {
            print!("{}", name);
        }
    }
}

#[inline]
fn print_suffix(dt: u8) {
    match dt {
        DT_DIR => print!("/"),
        DT_LNK => print!("@"),
        _ => {}
    }
}

#[no_mangle]
pub fn main() -> usize {
    let argv = args();
    let mut path_s = if argv.len() >= 2 {
        argv[1].clone()
    } else {
        String::from("/test")
    };
    path_s.push('\0');
    let fd = sys_open(&path_s, O_RDONLY);
    if fd < 0 {
        println!("[ls] open failed, ret={}", fd);
        return 1;
    }

    let mut buf = [0u8; 512];
    loop {
        let n = sys_getdents64(fd as usize, buf.as_mut_ptr() as usize, buf.len());
        if n < 0 {
            println!("[ls] getdents64 failed, ret={}", n);
            let _ = sys_close(fd as usize);
            return 2;
        }
        if n == 0 {
            break;
        }

        let mut off = 0usize;
        while off < n as usize {
            if off + 19 > n as usize {
                break;
            }

            // linux_dirent64 layout:
            // d_ino: u64 @0
            // d_off: u64 @8
            // d_reclen: u16 @16
            // d_type: u8 @18
            // d_name: [u8] @19 (null-terminated)
            let reclen = u16::from_le_bytes([buf[off + 16], buf[off + 17]]) as usize;
            if reclen == 0 {
                break;
            }
            if off + reclen > n as usize {
                break;
            }

            let d_type = buf[off + 18];
            let name_ptr = unsafe { buf.as_ptr().add(off + 19) };
            let name_len = c_strlen(name_ptr);
            if off + 19 + name_len + 1 <= n as usize {
                let name_bytes = unsafe { core::slice::from_raw_parts(name_ptr, name_len) };
                if let Ok(name) = core::str::from_utf8(name_bytes) {
                    print_name(d_type, name);
                    print_suffix(d_type);
                    print!("\n");
                } else {
                    println!("{}\t<non-utf8>", dt_name(d_type));
                }
            }

            off += reclen;
        }
    }

    let _ = sys_close(fd as usize);
    0
}
