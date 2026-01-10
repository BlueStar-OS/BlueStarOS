#![no_std]
#![no_main]

extern crate alloc;
extern crate user_lib;

use alloc::string::String;
use alloc::vec::Vec;
use user_lib::{print, println, sys_close, sys_exec_args, sys_fork, sys_getdents64, sys_mkdir, sys_open, sys_read, sys_wait, sys_write, chdir};
use user_lib::syscall::{O_CREAT, O_DIRECTORY, O_RDONLY, O_TRUNC, O_WRONLY};

const DT_REG: u8 = 8;

#[inline]
fn parse_dirent_names(dir_path: &str) -> Vec<String> {
    let fd = sys_open(dir_path, O_RDONLY | O_DIRECTORY);
    if fd < 0 {
        //println!("[consent_init] open dir failed path={} fd={}", dir_path, fd);
        return Vec::new();
    }

    let mut out: Vec<String> = Vec::new();
    let mut buf: Vec<u8> = alloc::vec![0; 4096];

    loop {
        let n = sys_getdents64(fd as usize, buf.as_mut_ptr() as usize, buf.len());
        if n < 0 {
            //println!("[consent_init] getdents64 failed ret={}", n);
            break;
        }
        if n == 0 {
            break;
        }

        let mut off = 0usize;
        while off < n as usize {
            if off + 19 > n as usize {
                break;
            }
            let reclen = u16::from_le_bytes([buf[off + 16], buf[off + 17]]) as usize;
            if reclen == 0 || off + reclen > n as usize {
                break;
            }

            let dt = buf[off + 18];

            let name_off = off + 19;
            let name_end = off + reclen;
            let mut z = name_off;
            while z < name_end && buf[z] != 0 {
                z += 1;
            }
            if z > name_off {
                let name = core::str::from_utf8(&buf[name_off..z]).unwrap_or("");
                if !name.is_empty() && dt == DT_REG {
                    out.push(String::from(name));
                }
            }

            off += reclen;
        }
    }

    let _ = sys_close(fd as usize);
    out
}

fn copy_file(src_path: &str, dst_path: &str) {
    let src_fd = sys_open(src_path, O_RDONLY);
    if src_fd < 0 {
        return;
    }
    let dst_fd = sys_open(dst_path, O_WRONLY | O_CREAT | O_TRUNC);
    if dst_fd < 0 {
        let _ = sys_close(src_fd as usize);
        return;
    }

    let mut buf = [0u8; 4096];
    loop {
        let n = sys_read(src_fd as usize, buf.as_mut_ptr() as usize, buf.len());
        if n <= 0 {
            break;
        }
        let _ = sys_write(dst_fd as usize, buf.as_ptr() as usize, n as usize);
    }

    let _ = sys_close(src_fd as usize);
    let _ = sys_close(dst_fd as usize);
}

fn copy_sd_root_to_ramfs_root() {
    let mut names = parse_dirent_names("/sd");
    names.sort();
    for name in names.iter() {
        if name.is_empty() || name == "." || name == ".." {
            continue;
        }
        let src = alloc::format!("/sd/{}", name);
        let dst = alloc::format!("/{}", name);
        copy_file(&src, &dst);
    }
}

fn run_one(bin_name: &str) {
    let mut path = String::from("/");
    path.push_str(bin_name);

    // argv[0]=bin_name
    let mut arg0 = String::from(bin_name);
    arg0.push('\0');
    let mut p = path.clone();
    p.push('\0');
    let argv_ptrs: [usize; 2] = [arg0.as_ptr() as usize, 0];

    let pid = sys_fork();
    if pid == 0 {
        //println!("Fork suc will exec");
        let ret = sys_exec_args(&p, argv_ptrs.as_ptr());
        //println!("[consent_init] exec {} failed ret={}", path, ret);
       // println!("[consent_init] exec {} failed ret={}", path, ret);
        user_lib::syscall::sys_exit(127);
    }
    if pid < 0 {
        //println!("[consent_init] fork failed for {} pid={}", path, pid);
        //println!("[consent_init] fork failed for {} pid={}", path, pid);
        return;
    }

    let mut code: isize = 0;
    let waited = sys_wait(&mut code as *mut isize);
    //println!("[consent_init] done {} waited={} code={}", path, waited, code);
   // println!("[consent_init] done {} waited={} code={}", path, waited, code);
}

#[no_mangle]
pub fn main() -> usize {

    

    copy_sd_root_to_ramfs_root();

    let _ = chdir("/");

    let _ = sys_mkdir("/mnt");

    let mut names = parse_dirent_names("/");
    names.sort();
    //println!("all program:{:?} \n",names);

    for name in names.iter() {
        if name.is_empty() || name == "." || name == ".." {
            continue;
        }
        if name.as_str() == "consent_init" || name.as_str() == "cinit" {
            continue;
        }
        run_one(name);
    }

    0
}
