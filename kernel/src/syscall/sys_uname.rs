//! sys_uname — 写回内核与机器信息。
//!
//! ## 作用
//! 写回内核与机器信息。
//!
//! ## 参数
//! `buf` 用户 utsname 缓冲区。
//!
//! ## 注意事项
//! 返回 BlueStarOS 固定字符串；布局按 Linux new_utsname 65 字节字段。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: kernel/sys.c:1351
//!
//! ## 实现情况
//! 已实现。

use crate::syscall::syscall::*;
use crate::ARCHITECTURE;

pub fn sys_uname(buf: usize) -> isize {
    if buf == 0 {
        return BlueErr::EFAULT.as_isize();
    }

    fn fill_field(dst: &mut [u8; UTNAME_FIELD_LEN], s: &str) {
        dst.fill(0);
        let bytes = s.as_bytes();
        let n = core::cmp::min(bytes.len(), UTNAME_FIELD_LEN - 1); //give \0 one byte
        dst[..n].copy_from_slice(&bytes[..n]);
        dst[n] = 0;
    }

    fn user_range_writable(satp: usize, start: usize, len: usize) -> bool {
        if len == 0 {
            return true;
        }
        let mut pt = PageTable::crate_table_from_satp(satp);
        let start_addr = VirAddr(start);
        let end_addr = VirAddr(start.saturating_add(len));
        let mut addr = start_addr;
        while addr < end_addr {
            let vpn = addr.floor_down();
            let Some(pte) = pt.find_pte_vpn(vpn) else {
                return false;
            };
            if !pte.is_valid() {
                return false;
            }
            let flags = pte.flags();
            if !flags.contains(PTEFlags::U) || !flags.contains(PTEFlags::W) {
                return false;
            }
            let next_page: VirAddr = VirNumber(vpn.0 + 1).into();
            addr = next_page;
        }
        true
    }

    fn copy_to_user(satp: usize, dst: usize, src: &[u8]) -> bool {
        let mut pt = PageTable::crate_table_from_satp(satp);
        for (i, b) in src.iter().enumerate() {
            let vaddr = VirAddr(dst.saturating_add(i));
            let Some(paddr) = pt.translate(vaddr) else {
                return false;
            };
            unsafe {
                *(paddr.0 as *mut u8) = *b;
            }
        }
        true
    }

    let user_satp = TASK_MANAER.get_current_stap();
    let total_len = core::mem::size_of::<UtsName>();
    if !user_range_writable(user_satp, buf, total_len) {
        return BlueErr::EFAULT.as_isize();
    }

    let mut u = UtsName::new();
    fill_field(&mut u.sysname, "Linux");
    fill_field(&mut u.nodename, "BlueStarOS");
    fill_field(&mut u.release, "0.1.0");
    fill_field(&mut u.version, "#1");

    fill_field(&mut u.machine, ARCHITECTURE);
    fill_field(&mut u.domainname, "(none)");

    let bytes: &[u8] =
        unsafe { core::slice::from_raw_parts((&u as *const UtsName) as *const u8, total_len) };
    if !copy_to_user(user_satp, buf, bytes) {
        return BlueErr::EFAULT.as_isize();
    }
    0
}
