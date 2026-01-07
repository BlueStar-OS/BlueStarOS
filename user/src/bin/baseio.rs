#![no_std]
#![no_main]
//read 和 write系统调用
use core::usize;
use user_lib::String;
use user_lib::print;
use user_lib::println;
use user_lib::{
    O_APPEND, O_CREAT, O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY, SEEK_END, SEEK_SET, sys_close, sys_creat,
    sys_lseek, sys_open, sys_read, sys_write,
};
extern crate user_lib;

#[no_mangle]
pub fn main() -> usize {
    let path = "/test/hello.txt";
    let msg = b"hello from BlueStarO
    S";

    println!("[fs] creat: {}", path);
    let fd = sys_creat(path);
    if fd < 0 {
        println!("[fs] creat failed, ret={}", fd);
        return 1;
    }
    println!("[fs] creat ok, fd={}", fd);

    let nw = sys_write(fd as usize, msg.as_ptr() as usize, msg.len());
    println!("[fs] write ret={}", nw);
    if nw as usize != msg.len() {
        println!("[fs] write size mismatch");
        let _ = sys_close(fd as usize);
        return 2;
    }

    let c = sys_close(fd as usize);
    println!("[fs] close ret={}", c);
    if c < 0 {
        return 3;
    }

    println!("[fs] reopen with open(O_RDONLY): {}", path);
    let fd2 = sys_open(path, O_RDONLY);
    println!("[fs] open ret={}", fd2);
    if fd2 < 0 {
        return 4;
    }

    let off2 = sys_lseek(fd2 as usize, 0, SEEK_SET);
    println!("[fs] lseek2(0,SEEK_SET) ret={}", off2);
    if off2 < 0 {
        let _ = sys_close(fd2 as usize);
        return 5;
    }

    let mut buf2 = [0u8; 64];
    let nr2 = sys_read(fd2 as usize, buf2.as_mut_ptr() as usize, buf2.len());
    println!("[fs] read2 ret={}", nr2);
    let _ = sys_close(fd2 as usize);
    if nr2 < 0 {
        return 6;
    }
    if nr2 as usize != msg.len() || &buf2[..msg.len()] != msg {
        println!("[fs] reopen read mismatch");
        return 7;
    }
    println!("read data:{:?}",String::from_utf8(buf2[..nr2 as usize].to_vec()));

    // Permission test: O_RDONLY must reject write.
    let fd_ro = sys_open(path, O_RDONLY);
    if fd_ro < 0 {
        println!("[fs] open(O_RDONLY) failed ret={}", fd_ro);
        return 8;
    }
    let nw_ro = sys_write(fd_ro as usize, msg.as_ptr() as usize, msg.len());
    println!("[fs] write on O_RDONLY ret={}", nw_ro);
    let _ = sys_close(fd_ro as usize);
    if nw_ro >= 0 {
        println!("[fs] ERROR: write should fail on O_RDONLY");
        return 9;
    }

    // Permission test: O_WRONLY must reject read.
    let fd_wo = sys_open(path, O_WRONLY);
    if fd_wo < 0 {
        println!("[fs] open(O_WRONLY) failed ret={}", fd_wo);
        return 10;
    }
    let mut tmp = [0u8; 8];
    let nr_wo = sys_read(fd_wo as usize, tmp.as_mut_ptr() as usize, tmp.len());
    println!("[fs] read on O_WRONLY ret={}", nr_wo);
    let _ = sys_close(fd_wo as usize);
    if nr_wo >= 0 {
        println!("[fs] ERROR: read should fail on O_WRONLY");
        return 11;
    }

    // RDWR should allow both.
    let fd_rw = sys_open(path, O_RDWR);
    if fd_rw < 0 {
        println!("[fs] open(O_RDWR) failed ret={}", fd_rw);
        return 12;
    }
    let nr_rw = sys_read(fd_rw as usize, tmp.as_mut_ptr() as usize, tmp.len());
    println!("[fs] read on O_RDWR ret={}", nr_rw);
    if nr_rw < 0 {
        let _ = sys_close(fd_rw as usize);
        return 13;
    }
    let nw_rw = sys_write(fd_rw as usize, b"X".as_ptr() as usize, 1);
    println!("[fs] write on O_RDWR ret={}", nw_rw);
    let _ = sys_close(fd_rw as usize);
    if nw_rw != 1 {
        return 14;
    }

    // TRUNC semantics: open with O_TRUNC|O_WRONLY must truncate to length 0.
    let fd_tr = sys_open(path, O_TRUNC | O_WRONLY);
    println!("[fs] open(O_TRUNC|O_WRONLY) ret={}", fd_tr);
    if fd_tr < 0 {
        return 15;
    }
    let _ = sys_close(fd_tr as usize);
    let fd_chk = sys_open(path, O_RDONLY);
    if fd_chk < 0 {
        return 16;
    }
    let mut chk = [0u8; 4];
    let nr_chk = sys_read(fd_chk as usize, chk.as_mut_ptr() as usize, chk.len());
    println!("[fs] read after trunc ret={}", nr_chk);
    let _ = sys_close(fd_chk as usize);
    if nr_chk != 0 {
        println!("[fs] ERROR: expected empty file after trunc");
        return 17;
    }

    // APPEND semantics: write must always go to end, regardless of lseek.
    let fd_ap = sys_open(path, O_WRONLY | O_APPEND);
    println!("[fs] open(O_WRONLY|O_APPEND) ret={}", fd_ap);
    if fd_ap < 0 {
        return 18;
    }
    let nw1 = sys_write(fd_ap as usize, b"A".as_ptr() as usize, 1);
    let off0 = sys_lseek(fd_ap as usize, 0, SEEK_SET);
    println!("[fs] lseek(0,SEEK_SET) on append fd ret={}", off0);
    let nw2 = sys_write(fd_ap as usize, b"B".as_ptr() as usize, 1);
    println!("[fs] append writes ret1={} ret2={}", nw1, nw2);
    let _ = sys_close(fd_ap as usize);
    if nw1 != 1 || nw2 != 1 {
        return 19;
    }
    let fd_chk2 = sys_open(path, O_RDONLY);
    if fd_chk2 < 0 {
        return 20;
    }
    let _ = sys_lseek(fd_chk2 as usize, -2, SEEK_END);
    let mut tail = [0u8; 2];
    let nr_tail = sys_read(fd_chk2 as usize, tail.as_mut_ptr() as usize, tail.len());
    let _ = sys_close(fd_chk2 as usize);
    println!("[fs] tail after append nr={} data={:?}", nr_tail, tail);
    if nr_tail != 2 || tail != *b"AB" {
        println!("[fs] ERROR: append semantics mismatch");
        return 21;
    }

    // CREAT semantics: open non-existent with O_CREAT should succeed.
    let new_path = "/test/creat_open.txt";
    let fd_new = sys_open(new_path, O_CREAT | O_WRONLY);
    println!("[fs] open(O_CREAT|O_WRONLY) {} ret={}", new_path, fd_new);
    if fd_new < 0 {
        return 22;
    }
    let nw_new = sys_write(fd_new as usize, b"C".as_ptr() as usize, 1);
    let _ = sys_close(fd_new as usize);
    if nw_new != 1 {
        return 23;
    }

    println!("[fs] PASS");
    0
}
