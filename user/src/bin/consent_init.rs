#![no_std]
#![no_main]

extern crate alloc;
extern crate user_lib;

use alloc::string::String;
use alloc::vec::Vec;
use user_lib::{print, println, sys_close, sys_exec_args, sys_fork, sys_getdents64, sys_mkdir, sys_open, sys_wait, chdir};
use user_lib::syscall::{O_DIRECTORY, O_RDONLY};

#[inline]
fn parse_dirent_names(dir_path: &str) -> Vec<String> {
    let fd = sys_open(dir_path, O_RDONLY | O_DIRECTORY);
    if fd < 0 {
       // println!("[consent_init] open dir failed path={} fd={}", dir_path, fd);
        return Vec::new();
    }

    let mut out: Vec<String> = Vec::new();
    let mut buf: Vec<u8> = alloc::vec![0; 8192];

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

            let name_off = off + 19;
            let name_end = off + reclen;
            let mut z = name_off;
            while z < name_end && buf[z] != 0 {
                z += 1;
            }
            if z > name_off {
                let name = core::str::from_utf8(&buf[name_off..z]).unwrap_or("");
                if !name.is_empty() {
                    out.push(String::from(name));
                }
            }

            off += reclen;
        }
    }

    let _ = sys_close(fd as usize);
    out
}

fn run_one(bin_name: &str) {
    let mut path = String::from("/bin/");
    path.push_str(bin_name);

    // argv[0]=bin_name
    let mut arg0 = String::from(bin_name);
    arg0.push('\0');
    let mut p = path.clone();
    p.push('\0');
    let argv_ptrs: [usize; 2] = [arg0.as_ptr() as usize, 0];

    let pid = sys_fork();
    if pid == 0 {
        let ret = sys_exec_args(&p, argv_ptrs.as_ptr());
       // println!("[consent_init] exec {} failed ret={}", path, ret);
        user_lib::syscall::sys_exit(127);
    }
    if pid < 0 {
        //println!("[consent_init] fork failed for {} pid={}", path, pid);
        return;
    }

    let mut code: isize = 0;
    let waited = sys_wait(&mut code as *mut isize);
   // println!("[consent_init] done {} waited={} code={}", path, waited, code);
}

#[no_mangle]
pub fn main() -> usize {
    // Run tests from /bin so relative paths (./mnt, ./text.txt) resolve to /bin/...
    let _ = chdir("/bin");

    // Ensure ./mnt exists for mount/openat tests.
    let _ = sys_mkdir("mnt");

    let mut names = parse_dirent_names("/bin");
    names.sort();

    for name in names.iter() {
        if name.is_empty() || name == "." || name == ".." {
            continue;
        }
        if name.as_str() == "consent_init" {
            continue;
        }
        // Skip obvious non-ELF helpers.
        if name.as_str().ends_with(".txt") {
            continue;
        }

      //  println!("========== START {} ==========", name);
        run_one(name);
       // println!("========== END {} ==========", name);
    }

    0
}
