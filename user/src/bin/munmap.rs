#![no_std]
#![no_main]
use user_lib::print;
use user_lib::println;
use user_lib::syscall::{
    sys_close, sys_creat, sys_exit, sys_fork, sys_mmap, sys_open, sys_read, sys_unmap, sys_wait,
    sys_write, MmapFlags, MmapProt, O_RDONLY, O_RDWR,
};
extern crate user_lib;

#[no_mangle]
pub fn main()->usize{
    let mut pass: usize = 0;
    let mut fail: usize = 0;

    fn report(pass: &mut usize, fail: &mut usize, name: &str, expect_ok: bool, ret: isize) {
        let ok = ret != -1;
        if ok == expect_ok {
            *pass += 1;
            println!("[PASS] {} | expect_ok={} actual_ok={} ret={:#x}", name, expect_ok, ok, ret as usize);
        } else {
            *fail += 1;
            println!("[FAIL] {} | expect_ok={} actual_ok={} ret={:#x}", name, expect_ok, ok, ret as usize);
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

    fn touch_u8(addr: usize, v: u8) -> u8 {
        unsafe {
            let p = addr as *mut u8;
            *p = v;
            *p
        }
    }

    fn probe_read_u8(pass: &mut usize, fail: &mut usize, name: &str, addr: usize, expect_ok: bool) {
        let pid = sys_fork();
        if pid == 0 {
            unsafe {
                let v = core::ptr::read_volatile(addr as *const u8);
                core::ptr::read_volatile(&v as *const u8);
            }
            sys_exit(0);
        }
        if pid > 0 {
            let mut code: isize = 0;
            let waited = sys_wait(&mut code as *mut isize);
            report(pass, fail, name, true, waited);
            let ok = code == 0;
            report_bool(pass, fail, name, expect_ok, ok);
        } else {
            report(pass, fail, name, true, pid);
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
        if nr == 1 { Some(b[0]) } else { None }
    }

    println!("==== munmap strict test (POSIX/Linux-oriented) ====");
    println!("Note: this OS currently returns -1 on failure and does not expose errno.");

    let base: usize = 0x0060_0000;
    let page: usize = 4096;

    // 1) munmap(len==0) must fail (EINVAL).
    let ret1 = sys_unmap(base, 0);
    report(&mut pass, &mut fail, "munmap len==0 must fail", false, ret1);

    // 2) munmap(unmapped range) must fail (EINVAL).
    let ret2 = sys_unmap(base + 0x20000, page);
    report(&mut pass, &mut fail, "munmap unmapped range must fail", false, ret2);

    // 3) munmap(addr not page-aligned) must fail on Linux (EINVAL).
    let ret3 = sys_unmap(base + 1, page);
    report(&mut pass, &mut fail, "munmap unaligned addr must fail", false, ret3);

    // 4) Basic: map 1 page, touch, munmap, then accessing should fault.
    let ret4_map = sys_mmap(
        base,
        page,
        (MmapProt::READ | MmapProt::WRITE).bits(),
        (MmapFlags::PRIVATE | MmapFlags::ANONYMOUS).bits(),
        -1,
        0,
    );
    report(&mut pass, &mut fail, "mmap 1 page for munmap", true, ret4_map);
    if ret4_map != -1 {
        let mapped = ret4_map as usize;
        let v = touch_u8(mapped, 0x66);
        report_bool(&mut pass, &mut fail, "touch before munmap", true, v == 0x66);

        let ret4_un = sys_unmap(mapped, page);
        report(&mut pass, &mut fail, "munmap full page should succeed", true, ret4_un);

        let pid = sys_fork();
        if pid == 0 {
            unsafe {
                let v = core::ptr::read_volatile(mapped as *const u8);
                // consume the value so the compiler can't prove it's unused
                core::ptr::read_volatile(&v as *const u8);
            }
            sys_exit(0);
        }
        if pid > 0 {
            let mut code: isize = 0;
            let waited = sys_wait(&mut code as *mut isize);
            report(&mut pass, &mut fail, "wait child after post-munmap access", true, waited);
            report_bool(&mut pass, &mut fail, "post-munmap access should not exit(0)", true, code != 0);
        }
    }

    // 5) Partial munmap semantics: map 3 pages, unmap middle page.
    // Expect: page0/page2 remain accessible; page1 access should fault.
    let base5 = base + 0x40000;
    let ret5_map = sys_mmap(
        base5,
        page * 3,
        (MmapProt::READ | MmapProt::WRITE).bits(),
        (MmapFlags::PRIVATE | MmapFlags::ANONYMOUS).bits(),
        -1,
        0,
    );
    report(&mut pass, &mut fail, "mmap 3 pages for partial munmap", true, ret5_map);
    if ret5_map != -1 {
        let a0 = base5;
        let a1 = base5 + page;
        let a2 = base5 + page * 2;
        report_bool(&mut pass, &mut fail, "touch page0", true, touch_u8(a0, 0x10) == 0x10);
        report_bool(&mut pass, &mut fail, "touch page1", true, touch_u8(a1, 0x11) == 0x11);
        report_bool(&mut pass, &mut fail, "touch page2", true, touch_u8(a2, 0x12) == 0x12);

        let ret5_un = sys_unmap(a1, page);
        report(&mut pass, &mut fail, "munmap middle page should succeed", true, ret5_un);

        // page0 still ok
        probe_read_u8(&mut pass, &mut fail, "page0 after partial munmap ok", a0, true);

        // page1 should fault
        probe_read_u8(&mut pass, &mut fail, "page1 after partial munmap should fault", a1, false);

        // page2 still ok
        probe_read_u8(&mut pass, &mut fail, "page2 after partial munmap ok", a2, true);
    }

    // 6) munmap prefix: map 4 pages, unmap first 2 pages.
    let base6 = base + 0x80000;
    let ret6_map = sys_mmap(
        base6,
        page * 4,
        (MmapProt::READ | MmapProt::WRITE).bits(),
        (MmapFlags::PRIVATE | MmapFlags::ANONYMOUS).bits(),
        -1,
        0,
    );
    report(&mut pass, &mut fail, "mmap 4 pages for prefix munmap", true, ret6_map);
    if ret6_map != -1 {
        let p0 = base6;
        let p1 = base6 + page;
        let p2 = base6 + page * 2;
        let p3 = base6 + page * 3;
        let _ = touch_u8(p0, 0x30);
        let _ = touch_u8(p1, 0x31);
        let _ = touch_u8(p2, 0x32);
        let _ = touch_u8(p3, 0x33);
        let ret6_un = sys_unmap(base6, page * 2);
        report(&mut pass, &mut fail, "munmap prefix(2 pages) should succeed", true, ret6_un);
        probe_read_u8(&mut pass, &mut fail, "prefix page0 should fault", p0, false);
        probe_read_u8(&mut pass, &mut fail, "prefix page1 should fault", p1, false);
        probe_read_u8(&mut pass, &mut fail, "prefix page2 should ok", p2, true);
        probe_read_u8(&mut pass, &mut fail, "prefix page3 should ok", p3, true);
    }

    // 7) munmap suffix: map 4 pages, unmap last 2 pages.
    let base7 = base + 0xA0000;
    let ret7_map = sys_mmap(
        base7,
        page * 4,
        (MmapProt::READ | MmapProt::WRITE).bits(),
        (MmapFlags::PRIVATE | MmapFlags::ANONYMOUS).bits(),
        -1,
        0,
    );
    report(&mut pass, &mut fail, "mmap 4 pages for suffix munmap", true, ret7_map);
    if ret7_map != -1 {
        let p0 = base7;
        let p1 = base7 + page;
        let p2 = base7 + page * 2;
        let p3 = base7 + page * 3;
        let _ = touch_u8(p0, 0x40);
        let _ = touch_u8(p1, 0x41);
        let _ = touch_u8(p2, 0x42);
        let _ = touch_u8(p3, 0x43);
        let ret7_un = sys_unmap(base7 + page * 2, page * 2);
        report(&mut pass, &mut fail, "munmap suffix(2 pages) should succeed", true, ret7_un);
        probe_read_u8(&mut pass, &mut fail, "suffix page0 should ok", p0, true);
        probe_read_u8(&mut pass, &mut fail, "suffix page1 should ok", p1, true);
        probe_read_u8(&mut pass, &mut fail, "suffix page2 should fault", p2, false);
        probe_read_u8(&mut pass, &mut fail, "suffix page3 should fault", p3, false);
    }

    // 8) munmap hole spanning 2 pages: map 5 pages, unmap pages[1..3].
    let base8 = base + 0xC0000;
    let ret8_map = sys_mmap(
        base8,
        page * 5,
        (MmapProt::READ | MmapProt::WRITE).bits(),
        (MmapFlags::PRIVATE | MmapFlags::ANONYMOUS).bits(),
        -1,
        0,
    );
    report(&mut pass, &mut fail, "mmap 5 pages for 2-page hole munmap", true, ret8_map);
    if ret8_map != -1 {
        let p0 = base8;
        let p1 = base8 + page;
        let p2 = base8 + page * 2;
        let p3 = base8 + page * 3;
        let p4 = base8 + page * 4;
        let _ = touch_u8(p0, 0x50);
        let _ = touch_u8(p1, 0x51);
        let _ = touch_u8(p2, 0x52);
        let _ = touch_u8(p3, 0x53);
        let _ = touch_u8(p4, 0x54);
        let ret8_un = sys_unmap(base8 + page, page * 2);
        report(&mut pass, &mut fail, "munmap hole(2 pages) should succeed", true, ret8_un);
        probe_read_u8(&mut pass, &mut fail, "hole p0 ok", p0, true);
        probe_read_u8(&mut pass, &mut fail, "hole p1 fault", p1, false);
        probe_read_u8(&mut pass, &mut fail, "hole p2 fault", p2, false);
        probe_read_u8(&mut pass, &mut fail, "hole p3 ok", p3, true);
        probe_read_u8(&mut pass, &mut fail, "hole p4 ok", p4, true);
    }

    // 9) munmap len=1 should still unmap one page (length rounds up).
    let base9 = base + 0xE0000;
    let ret9_map = sys_mmap(
        base9,
        page,
        (MmapProt::READ | MmapProt::WRITE).bits(),
        (MmapFlags::PRIVATE | MmapFlags::ANONYMOUS).bits(),
        -1,
        0,
    );
    report(&mut pass, &mut fail, "mmap 1 page for len=1 munmap", true, ret9_map);
    if ret9_map != -1 {
        let _ = touch_u8(base9, 0x60);
        let ret9_un = sys_unmap(base9, 1);
        report(&mut pass, &mut fail, "munmap len=1 should succeed", true, ret9_un);
        probe_read_u8(&mut pass, &mut fail, "after munmap len=1 should fault", base9, false);
    }

    // 10) cross-area munmap: create two MAP_FIXED areas with a gap, unmap spanning tail/head.
    let base10 = 0x0070_0000;
    let ret10_a = sys_mmap(
        base10,
        page * 2,
        (MmapProt::READ | MmapProt::WRITE).bits(),
        (MmapFlags::PRIVATE | MmapFlags::ANONYMOUS | MmapFlags::FIXED).bits(),
        -1,
        0,
    );
    report(&mut pass, &mut fail, "MAP_FIXED area A (2 pages)", true, ret10_a);
    let ret10_b = sys_mmap(
        base10 + page * 3,
        page * 2,
        (MmapProt::READ | MmapProt::WRITE).bits(),
        (MmapFlags::PRIVATE | MmapFlags::ANONYMOUS | MmapFlags::FIXED).bits(),
        -1,
        0,
    );
    report(&mut pass, &mut fail, "MAP_FIXED area B (2 pages)", true, ret10_b);
    if ret10_a != -1 && ret10_b != -1 {
        let a0 = base10;
        let a1 = base10 + page;
        let b0 = base10 + page * 3;
        let b1 = base10 + page * 4;
        let _ = touch_u8(a0, 0x70);
        let _ = touch_u8(a1, 0x71);
        let _ = touch_u8(b0, 0x72);
        let _ = touch_u8(b1, 0x73);
        // unmap a1 and b0 together
        let ret10_un = sys_unmap(a1, page * 3);
        report(&mut pass, &mut fail, "cross-area munmap should succeed", true, ret10_un);
        probe_read_u8(&mut pass, &mut fail, "cross-area a0 ok", a0, true);
        probe_read_u8(&mut pass, &mut fail, "cross-area a1 fault", a1, false);
        probe_read_u8(&mut pass, &mut fail, "cross-area b0 fault", b0, false);
        probe_read_u8(&mut pass, &mut fail, "cross-area b1 ok", b1, true);
    }

    // 11) Drop(MapSet) cleanup: child exits without munmap; MAP_SHARED should write back.
    let base11 = base + 0x140000;
    let path11 = "/test/munmap_drop_shared.bin";
    let fd11 = sys_creat(path11);
    report(&mut pass, &mut fail, "creat drop(shared) file", true, fd11);
    if fd11 != -1 {
        write_fill_4096(&mut pass, &mut fail, fd11 as usize, b'A', "init drop(shared) file page0");
        let _ = sys_close(fd11 as usize);
    }
    let fd11_rw = sys_open(path11, O_RDWR);
    report(&mut pass, &mut fail, "open(O_RDWR) drop(shared)", true, fd11_rw);
    let pid11 = if fd11_rw != -1 { sys_fork() } else { -1 };
    if pid11 == 0 {
        let mapped = sys_mmap(
            base11,
            page,
            (MmapProt::READ | MmapProt::WRITE).bits(),
            MmapFlags::SHARED.bits(),
            fd11_rw,
            0,
        );
        if mapped != -1 {
            unsafe {
                core::ptr::write_volatile(mapped as *mut u8, b'S');
            }
        }
        let _ = sys_close(fd11_rw as usize);
        sys_exit(0);
    }
    if pid11 > 0 {
        let _ = sys_close(fd11_rw as usize);
        let mut code: isize = 0;
        let waited = sys_wait(&mut code as *mut isize);
        report(&mut pass, &mut fail, "wait child drop(shared)", true, waited);
        report_bool(&mut pass, &mut fail, "child drop(shared) should exit(0)", true, code == 0);
        let b = read_first_byte(path11);
        report_bool(&mut pass, &mut fail, "drop(shared) should write back", true, b == Some(b'S'));
    }

    // 12) Drop(MapSet) cleanup: MAP_PRIVATE should not write back.
    let base12 = base + 0x160000;
    let path12 = "/test/munmap_drop_private.bin";
    let fd12 = sys_creat(path12);
    report(&mut pass, &mut fail, "creat drop(private) file", true, fd12);
    if fd12 != -1 {
        write_fill_4096(&mut pass, &mut fail, fd12 as usize, b'A', "init drop(private) file page0");
        let _ = sys_close(fd12 as usize);
    }
    let fd12_rw = sys_open(path12, O_RDWR);
    report(&mut pass, &mut fail, "open(O_RDWR) drop(private)", true, fd12_rw);
    let pid12 = if fd12_rw != -1 { sys_fork() } else { -1 };
    if pid12 == 0 {
        let mapped = sys_mmap(
            base12,
            page,
            (MmapProt::READ | MmapProt::WRITE).bits(),
            MmapFlags::PRIVATE.bits(),
            fd12_rw,
            0,
        );
        if mapped != -1 {
            unsafe {
                core::ptr::write_volatile(mapped as *mut u8, b'P');
            }
        }
        let _ = sys_close(fd12_rw as usize);
        sys_exit(0);
    }
    if pid12 > 0 {
        let _ = sys_close(fd12_rw as usize);
        let mut code: isize = 0;
        let waited = sys_wait(&mut code as *mut isize);
        report(&mut pass, &mut fail, "wait child drop(private)", true, waited);
        report_bool(&mut pass, &mut fail, "child drop(private) should exit(0)", true, code == 0);
        let b = read_first_byte(path12);
        report_bool(&mut pass, &mut fail, "drop(private) should not write back", true, b == Some(b'A'));
    }

    println!("==== munmap strict test done: pass={} fail={} ====", pass, fail);
    if fail == 0 { 0 } else { 1 }
}
