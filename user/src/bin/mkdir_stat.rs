#![no_std]
#![no_main]

extern crate user_lib;
use user_lib::print;
use user_lib::{
    println,
    sys_mkdir,
    sys_stat,
    sys_unlink,
    VfsStat,
};

#[inline]
fn ft_name(ft: u32) -> &'static str {
    match ft {
        4 => "dir",
        8 => "file",
        _ => "unk",
    }
}

fn do_stat(path: &str) -> isize {
    let mut st = VfsStat::default();
    let ret = sys_stat(path, &mut st as *mut VfsStat);
    if ret < 0 {
        println!("[stat] {} -> failed ret={}", path, ret);
        return ret;
    }
    println!(
        "[stat] {} -> inode={} size={} type={}",
        path,
        st.inode,
        st.size,
        ft_name(st.file_type)
    );
    0
}

#[no_mangle]
pub fn main() -> usize {
    let dir = "/test/dir1\0";
    println!("[mkdir] {}", dir);
    let r = sys_mkdir(dir);
    println!("[mkdir] ret={}", r);

    let _ = do_stat("/test\0");
    let _ = do_stat(dir);
    let _ = do_stat("/test/hello.txt\0");

    // unlink 目录：应失败（unlink 只删除文件）
    let ur = sys_unlink(dir);
    println!("[unlink] dir {} -> ret={}", dir, ur);

    // unlink 文件：应成功，然后 stat 应失败
    let file = "/test/hello.txt\0";
    let ur2 = sys_unlink(file);
    println!("[unlink] file {} -> ret={}", file, ur2);
    let _ = do_stat(file);

    0
}
