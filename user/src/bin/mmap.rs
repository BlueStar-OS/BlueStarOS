#![no_std]
#![no_main]

use user_lib::println;
use user_lib::print;
use user_lib::syscall::{sys_close, sys_creat, sys_mmap, sys_open, sys_read, sys_write, MmapFlags, MmapProt, O_RDONLY, O_RDWR};
extern crate user_lib;

#[no_mangle]
pub fn main()->usize{
    let mut pass: usize = 0;
    let mut fail: usize = 0;

    fn report(pass: &mut usize, fail: &mut usize, name: &str, expect_ok: bool, ret: isize) {
        let ok = ret != -1;
        let verdict = ok == expect_ok;
        if verdict {
            *pass += 1;
            println!("[PASS] {} | expect_ok={} actual_ok={} ret={:#x}", name, expect_ok, ok, ret as usize);
        } else {
            *fail += 1;
            println!("[FAIL] {} | expect_ok={} actual_ok={} ret={:#x}", name, expect_ok, ok, ret as usize);
        }
    }

    fn touch_u8(addr: usize, v: u8) -> u8 {
        unsafe {
            let p = addr as *mut u8;
            *p = v;
            *p
        }
    }

    fn report_bool(pass: &mut usize, fail: &mut usize, name: &str, expect: bool, actual: bool) {
        if expect == actual {
            *pass += 1;
            println!("[PASS] {} | expect={} actual={}", name, expect, actual);
        } else {
            *fail += 1;
            println!("[FAIL] {} | expect={} actual={}", name, expect, actual);
        }
    }

    fn write_fill_4096(pass: &mut usize, fail: &mut usize, fd: usize, byte: u8, tag: &str) {
        let mut buf = [0u8; 256];
        buf.fill(byte);
        let mut left = 4096usize;
        while left > 0 {
            let n = if left >= buf.len() { buf.len() } else { left };
            let nw = sys_write(fd, buf.as_ptr() as usize, n);
            if nw < 0 {
                report_bool(pass, fail, tag, true, false);
                return;
            }
            left = left.saturating_sub(nw as usize);
            if nw as usize != n {
                report_bool(pass, fail, tag, true, false);
                return;
            }
        }
        report_bool(pass, fail, tag, true, true);
    }

    fn read_first_byte(path: &str) -> Option<u8> {
        let fd = sys_open(path, O_RDONLY);
        if fd < 0 {
            return None;
        }
        let mut b = [0u8; 1];
        let nr = sys_read(fd as usize, b.as_mut_ptr() as usize, 1);
        let _ = sys_close(fd as usize);
        if nr == 1 {
            Some(b[0])
        } else {
            None
        }
    }

    println!("==== mmap strict test (POSIX/Linux-oriented) ====");
    println!("Note: this OS currently returns -1 on failure and does not expose errno.");

    // 1) Basic anonymous mapping at fixed hint address (should succeed).
    let addr1: usize = 0x0060_0000;
    let len1: usize = 4096;
    let ret1 = sys_mmap(
        addr1,
        len1,
        (MmapProt::READ | MmapProt::WRITE).bits(),
        (MmapFlags::PRIVATE | MmapFlags::ANONYMOUS).bits(),
        -1,
        0,
    );
    report(&mut pass, &mut fail, "anon private rw @hint", true, ret1);
    if ret1 != -1 {
        let mapped = ret1 as usize;
        let r = touch_u8(mapped, 66);
        if r == 66 {
            pass += 1;
            println!("[PASS] touch mapped memory | expect=66 actual={} addr={:#x}", r, mapped);
        } else {
            fail += 1;
            println!("[FAIL] touch mapped memory | expect=66 actual={} addr={:#x}", r, mapped);
        }
    }

    // 2) len == 0 => EINVAL on Linux => should fail here.
    let ret2 = sys_mmap(
        addr1,
        0,
        MmapProt::READ.bits(),
        (MmapFlags::PRIVATE | MmapFlags::ANONYMOUS).bits(),
        -1,
        0,
    );
    report(&mut pass, &mut fail, "len==0 must fail", false, ret2);

    // 3) offset not page-aligned => EINVAL => should fail.
    let ret3 = sys_mmap(
        addr1 + 0x20000,
        4096,
        MmapProt::READ.bits(),
        (MmapFlags::PRIVATE | MmapFlags::ANONYMOUS).bits(),
        -1,
        1,
    );
    report(&mut pass, &mut fail, "offset not aligned must fail", false, ret3);

    // 4) Missing SHARED/PRIVATE => EINVAL => should fail.
    let ret4 = sys_mmap(
        addr1 + 0x30000,
        4096,
        MmapProt::READ.bits(),
        MmapFlags::ANONYMOUS.bits(),
        -1,
        0,
    );
    report(&mut pass, &mut fail, "flags missing private/shared must fail", false, ret4);

    // 5) Both SHARED and PRIVATE => EINVAL => should fail.
    let ret5 = sys_mmap(
        addr1 + 0x40000,
        4096,
        MmapProt::READ.bits(),
        (MmapFlags::ANONYMOUS | MmapFlags::PRIVATE | MmapFlags::SHARED).bits(),
        -1,
        0,
    );
    report(&mut pass, &mut fail, "flags shared+private must fail", false, ret5);

    // 6) Unknown prot bits => EINVAL => should fail.
    let ret6 = sys_mmap(
        addr1 + 0x50000,
        4096,
        MmapProt::READ.bits() | 0x8000_0000,
        (MmapFlags::PRIVATE | MmapFlags::ANONYMOUS).bits(),
        -1,
        0,
    );
    report(&mut pass, &mut fail, "unknown prot bits must fail", false, ret6);

    // 7) Unknown flag bits => EINVAL => should fail.
    let ret7 = sys_mmap(
        addr1 + 0x60000,
        4096,
        MmapProt::READ.bits(),
        (MmapFlags::PRIVATE | MmapFlags::ANONYMOUS).bits() | 0x8000_0000,
        -1,
        0,
    );
    report(&mut pass, &mut fail, "unknown flags bits must fail", false, ret7);

    // 8) MAP_FIXED requires page-aligned addr => EINVAL => should fail.
    let ret8 = sys_mmap(
        addr1 + 1,
        4096,
        MmapProt::READ.bits(),
        (MmapFlags::PRIVATE | MmapFlags::ANONYMOUS | MmapFlags::FIXED).bits(),
        -1,
        0,
    );
    report(&mut pass, &mut fail, "MAP_FIXED unaligned addr must fail", false, ret8);

    // 9) addr==0 (non-fixed): kernel chooses address. Linux expects success.
    // Your kernel implements find_free_range based on brk; should succeed.
    let ret9 = sys_mmap(
        0,
        4096,
        (MmapProt::READ | MmapProt::WRITE).bits(),
        (MmapFlags::PRIVATE | MmapFlags::ANONYMOUS).bits(),
        -1,
        0,
    );
    report(&mut pass, &mut fail, "addr==0 kernel choose", true, ret9);
    if ret9 != -1 {
        let mapped = ret9 as usize;
        let r = touch_u8(mapped, 77);
        if r == 77 {
            pass += 1;
            println!("[PASS] touch addr==0 mapping | expect=77 actual={} addr={:#x}", r, mapped);
        } else {
            fail += 1;
            println!("[FAIL] touch addr==0 mapping | expect=77 actual={} addr={:#x}", r, mapped);
        }
    }

    // 10) MAP_FIXED conflict replacement semantics:
    // First map at A, write marker; then MAP_FIXED map again at same A; write new marker.
    // We can't observe physical replacement directly, but we can verify the second mmap succeeds.
    let addr10: usize = 0x0070_0000;
    let ret10a = sys_mmap(
        addr10,
        4096,
        (MmapProt::READ | MmapProt::WRITE).bits(),
        (MmapFlags::PRIVATE | MmapFlags::ANONYMOUS).bits(),
        -1,
        0,
    );
    report(&mut pass, &mut fail, "MAP_FIXED replace: initial map", true, ret10a);
    if ret10a != -1 {
        let v0 = touch_u8(addr10, 0x11);
        if v0 == 0x11 {
            pass += 1;
            println!("[PASS] pre-replace write marker | expect=0x11 actual={:#x}", v0);
        } else {
            fail += 1;
            println!("[FAIL] pre-replace write marker | expect=0x11 actual={:#x}", v0);
        }
    }
    let ret10b = sys_mmap(
        addr10,
        4096,
        (MmapProt::READ | MmapProt::WRITE).bits(),
        (MmapFlags::PRIVATE | MmapFlags::ANONYMOUS | MmapFlags::FIXED).bits(),
        -1,
        0,
    );
    report(&mut pass, &mut fail, "MAP_FIXED replace: second map must succeed", true, ret10b);
    if ret10b != -1 {
        let v1 = touch_u8(addr10, 0x22);
        if v1 == 0x22 {
            pass += 1;
            println!("[PASS] post-replace write marker | expect=0x22 actual={:#x}", v1);
        } else {
            fail += 1;
            println!("[FAIL] post-replace write marker | expect=0x22 actual={:#x}", v1);
        }
    }

    // 11) file-backed mmap (MAP_PRIVATE) basic read: should succeed.
    // Prepare a 2-page file: page0 filled with 'A', page1 filled with 'B'.
    let path = "/test/mmap_shared_test.bin";
    let fd_create = sys_creat(path);
    report(&mut pass, &mut fail, "creat test file", true, fd_create);
    if fd_create != -1 {
        write_fill_4096(&mut pass, &mut fail, fd_create as usize, b'A', "write page0(4096)");
        write_fill_4096(&mut pass, &mut fail, fd_create as usize, b'B', "write page1(4096)");
        let _ = sys_close(fd_create as usize);
    }

    let fd_ro = sys_open(path, O_RDONLY);
    report(&mut pass, &mut fail, "open(O_RDONLY) test file", true, fd_ro);
    let ret11 = if fd_ro != -1 {
        sys_mmap(
            addr1 + 0x80000,
            4096,
            MmapProt::READ.bits(),
            MmapFlags::PRIVATE.bits(),
            fd_ro,
            0,
        )
    } else {
        -1
    };
    report(&mut pass, &mut fail, "file-backed MAP_PRIVATE read should succeed", true, ret11);
    if ret11 != -1 {
        let mapped = ret11 as usize;
        let v = unsafe { *(mapped as *const u8) };
        report_bool(&mut pass, &mut fail, "file-backed MAP_PRIVATE first byte == 'A'", true, v == b'A');
    }
    if fd_ro != -1 {
        let _ = sys_close(fd_ro as usize);
    }

    // 12) MAP_SHARED + MAP_ANONYMOUS should be a valid combination.
    let ret12 = sys_mmap(
        addr1 + 0x90000,
        4096,
        (MmapProt::READ | MmapProt::WRITE).bits(),
        (MmapFlags::SHARED | MmapFlags::ANONYMOUS).bits(),
        -1,
        0,
    );
    report(&mut pass, &mut fail, "anon shared rw (valid flags)", true, ret12);
    if ret12 != -1 {
        let mapped = ret12 as usize;
        let r = touch_u8(mapped, 88);
        report_bool(&mut pass, &mut fail, "anon shared rw touch", true, r == 88);
    }

    // 13) Anonymous mapping requires fd == -1.
    let ret13 = sys_mmap(
        addr1 + 0xA0000,
        4096,
        MmapProt::READ.bits(),
        (MmapFlags::PRIVATE | MmapFlags::ANONYMOUS).bits(),
        0,
        0,
    );
    report(&mut pass, &mut fail, "anon with fd!= -1 must fail", false, ret13);

    // 14) file-backed MAP_SHARED + PROT_WRITE requires fd to be writable.
    // Open RO then request shared+writable: must fail.
    let fd_ro2 = sys_open(path, O_RDONLY);
    report(&mut pass, &mut fail, "open(O_RDONLY) for shared+writable", true, fd_ro2);
    let ret14 = if fd_ro2 != -1 {
        sys_mmap(
            addr1 + 0xB0000,
            4096,
            (MmapProt::READ | MmapProt::WRITE).bits(),
            MmapFlags::SHARED.bits(),
            fd_ro2,
            0,
        )
    } else {
        -1
    };
    report(&mut pass, &mut fail, "file-backed MAP_SHARED+PROT_WRITE on O_RDONLY must fail", false, ret14);
    if fd_ro2 != -1 {
        let _ = sys_close(fd_ro2 as usize);
    }

    // 15) file-backed MAP_SHARED + PROT_WRITE with O_RDWR should succeed.
    let fd_rw = sys_open(path, O_RDWR);
    report(&mut pass, &mut fail, "open(O_RDWR) test file", true, fd_rw);
    let ret15_file = if fd_rw != -1 {
        sys_mmap(
            addr1 + 0xB4000,
            4096,
            (MmapProt::READ | MmapProt::WRITE).bits(),
            MmapFlags::SHARED.bits(),
            fd_rw,
            0,
        )
    } else {
        -1
    };
    report(&mut pass, &mut fail, "file-backed MAP_SHARED rw on O_RDWR should succeed", true, ret15_file);
    if ret15_file != -1 {
        let mapped = ret15_file as usize;
        let r = touch_u8(mapped, 0x5A);
        report_bool(&mut pass, &mut fail, "file-backed MAP_SHARED touch writable", true, r == 0x5A);
    }
    if fd_rw != -1 {
        let _ = sys_close(fd_rw as usize);
    }

    // 15b) MAP_SHARED write should be written back to the file.
    let path2 = "/test/mmap_writeback_shared.bin";
    let fd2 = sys_creat(path2);
    report(&mut pass, &mut fail, "creat writeback(shared) file", true, fd2);
    if fd2 != -1 {
        write_fill_4096(&mut pass, &mut fail, fd2 as usize, b'A', "init shared file page0");
        let _ = sys_close(fd2 as usize);
    }
    let fd2_rw = sys_open(path2, O_RDWR);
    report(&mut pass, &mut fail, "open(O_RDWR) writeback(shared)", true, fd2_rw);
    let ret15b = if fd2_rw != -1 {
        sys_mmap(
            addr1 + 0xC4000,
            4096,
            (MmapProt::READ | MmapProt::WRITE).bits(),
            MmapFlags::SHARED.bits(),
            fd2_rw,
            0,
        )
    } else {
        -1
    };
    report(&mut pass, &mut fail, "file-backed MAP_SHARED writeback mmap", true, ret15b);
    if ret15b != -1 {
        let mapped = ret15b as usize;
        let r = touch_u8(mapped, b'S');
        report_bool(&mut pass, &mut fail, "MAP_SHARED store to mapping", true, r == b'S');
    }
    if fd2_rw != -1 {
        let _ = sys_close(fd2_rw as usize);
    }
    let wb = read_first_byte(path2);
    report_bool(&mut pass, &mut fail, "MAP_SHARED writeback visible in file", true, wb == Some(b'S'));

    // 15c) MAP_PRIVATE write must not be written back to the file.
    let path3 = "/test/mmap_writeback_private.bin";
    let fd3 = sys_creat(path3);
    report(&mut pass, &mut fail, "creat writeback(private) file", true, fd3);
    if fd3 != -1 {
        write_fill_4096(&mut pass, &mut fail, fd3 as usize, b'A', "init private file page0");
        let _ = sys_close(fd3 as usize);
    }
    let fd3_rw = sys_open(path3, O_RDWR);
    report(&mut pass, &mut fail, "open(O_RDWR) writeback(private)", true, fd3_rw);
    let ret15c = if fd3_rw != -1 {
        sys_mmap(
            addr1 + 0xC8000,
            4096,
            (MmapProt::READ | MmapProt::WRITE).bits(),
            MmapFlags::PRIVATE.bits(),
            fd3_rw,
            0,
        )
    } else {
        -1
    };
    report(&mut pass, &mut fail, "file-backed MAP_PRIVATE writeback mmap", true, ret15c);
    if ret15c != -1 {
        let mapped = ret15c as usize;
        let r = touch_u8(mapped, b'P');
        report_bool(&mut pass, &mut fail, "MAP_PRIVATE store to mapping", true, r == b'P');
    }
    if fd3_rw != -1 {
        let _ = sys_close(fd3_rw as usize);
    }
    let wb2 = read_first_byte(path3);
    report_bool(&mut pass, &mut fail, "MAP_PRIVATE store not visible in file", true, wb2 == Some(b'A'));

    // 16) close(fd) after mmap: mapping should remain usable (backing Arc held by kernel).
    let fd_ro3 = sys_open(path, O_RDONLY);
    report(&mut pass, &mut fail, "open(O_RDONLY) for close-after-mmap", true, fd_ro3);
    let ret16_file = if fd_ro3 != -1 {
        sys_mmap(
            addr1 + 0xB8000,
            4096,
            MmapProt::READ.bits(),
            MmapFlags::PRIVATE.bits(),
            fd_ro3,
            0,
        )
    } else {
        -1
    };
    report(&mut pass, &mut fail, "file-backed MAP_PRIVATE then close(fd) should succeed", true, ret16_file);
    if fd_ro3 != -1 {
        let _ = sys_close(fd_ro3 as usize);
    }
    if ret16_file != -1 {
        let mapped = ret16_file as usize;
        let v = unsafe { *(mapped as *const u8) };
        report_bool(&mut pass, &mut fail, "after close(fd), mapped first byte still == 'A'", true, v == b'A');
    }

    // 17) file-backed offset: map second page (offset=4096), expect first byte == 'B'.
    let fd_ro4 = sys_open(path, O_RDONLY);
    report(&mut pass, &mut fail, "open(O_RDONLY) for offset test", true, fd_ro4);
    let ret17_file = if fd_ro4 != -1 {
        sys_mmap(
            addr1 + 0xBC000,
            4096,
            MmapProt::READ.bits(),
            MmapFlags::PRIVATE.bits(),
            fd_ro4,
            4096,
        )
    } else {
        -1
    };
    report(&mut pass, &mut fail, "file-backed MAP_PRIVATE offset=4096 should succeed", true, ret17_file);
    if ret17_file != -1 {
        let mapped = ret17_file as usize;
        let v = unsafe { *(mapped as *const u8) };
        report_bool(&mut pass, &mut fail, "offset=4096 mapped first byte == 'B'", true, v == b'B');
    }
    if fd_ro4 != -1 {
        let _ = sys_close(fd_ro4 as usize);
    }

    // 18) file-backed fd == -1 without MAP_ANONYMOUS must fail.
    let ret18_file = sys_mmap(
        addr1 + 0xC0000,
        4096,
        MmapProt::READ.bits(),
        MmapFlags::PRIVATE.bits(),
        -1,
        0,
    );
    report(&mut pass, &mut fail, "file-backed fd==-1 must fail", false, ret18_file);

    // 19) Non-MAP_FIXED hint conflict: mapping same hint twice should relocate.
    // POSIX: addr is hint, conflict should not overwrite; kernel picks another address.
    let hint15: usize = 0x0080_0000;
    let ret15a = sys_mmap(
        hint15,
        4096,
        (MmapProt::READ | MmapProt::WRITE).bits(),
        (MmapFlags::PRIVATE | MmapFlags::ANONYMOUS).bits(),
        -1,
        0,
    );
    report(&mut pass, &mut fail, "hint conflict: first map", true, ret15a);
    let ret15b = sys_mmap(
        hint15,
        4096,
        (MmapProt::READ | MmapProt::WRITE).bits(),
        (MmapFlags::PRIVATE | MmapFlags::ANONYMOUS).bits(),
        -1,
        0,
    );
    report(&mut pass, &mut fail, "hint conflict: second map should succeed", true, ret15b);
    if ret15b != -1 {
        report_bool(&mut pass, &mut fail, "hint conflict: second addr != hint", true, (ret15b as usize) != hint15);
    }

    // 20) MAP_FIXED with unaligned length should still be allowed (kernel rounds up).
    let addr16: usize = 0x0090_0000;
    let ret16 = sys_mmap(
        addr16,
        1,
        (MmapProt::READ | MmapProt::WRITE).bits(),
        (MmapFlags::PRIVATE | MmapFlags::ANONYMOUS | MmapFlags::FIXED).bits(),
        -1,
        0,
    );
    report(&mut pass, &mut fail, "MAP_FIXED len=1 (round up) should succeed", true, ret16);
    if ret16 != -1 {
        let r = touch_u8(addr16, 0x33);
        report_bool(&mut pass, &mut fail, "MAP_FIXED len=1 touch", true, r == 0x33);
    }

    // 21) MAP_FIXED overflow address (expect fail).
    // Linux would EINVAL/ENOMEM; we expect fail.
    let near_max = usize::MAX & !0xfff;
    let ret17 = sys_mmap(
        near_max,
        4096,
        MmapProt::READ.bits(),
        (MmapFlags::PRIVATE | MmapFlags::ANONYMOUS | MmapFlags::FIXED).bits(),
        -1,
        0,
    );
    report(&mut pass, &mut fail, "MAP_FIXED addr near MAX must fail", false, ret17);

    // 22) len overflow (round-up overflow) should fail.
    let ret18 = sys_mmap(
        0,
        usize::MAX,
        MmapProt::READ.bits(),
        (MmapFlags::PRIVATE | MmapFlags::ANONYMOUS).bits(),
        -1,
        0,
    );
    report(&mut pass, &mut fail, "len overflow must fail", false, ret18);

    // 23) fd negative (non -1) with file-backed flags: should fail.
    let ret19 = sys_mmap(
        addr1 + 0xD0000,
        4096,
        MmapProt::READ.bits(),
        MmapFlags::PRIVATE.bits(),
        -2,
        0,
    );
    report(&mut pass, &mut fail, "file-backed fd<0 must fail", false, ret19);

    // 24) fd huge out of range with file-backed flags: should fail.
    let ret20 = sys_mmap(
        addr1 + 0xE0000,
        4096,
        MmapProt::READ.bits(),
        MmapFlags::PRIVATE.bits(),
        0x7fff_ffff,
        0,
    );
    report(&mut pass, &mut fail, "file-backed fd out of range must fail", false, ret20);

    println!("==== mmap strict test done: pass={} fail={} ====", pass, fail);
    if fail == 0 { 0 } else { 1 }
}
