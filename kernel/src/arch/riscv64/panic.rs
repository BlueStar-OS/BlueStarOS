use core::arch::asm;

/// 栈帧回溯，解析返回地址 → 函数名+偏移
pub(crate) fn backtrace() {
    let mut fp: usize;
    unsafe {
        asm!("mv {0}, s0", out(reg) fp, options(nomem, nostack, preserves_flags));
    }
    // RISC-V frame pointer convention (with frame pointers enabled):
    //   [fp-16] = previous fp
    //   [fp-8]  = saved ra
    for i in 0..32usize {
        if fp == 0 || (fp & 0xf) != 0 {
            break;
        }
        let prev_fp_ptr = (fp.wrapping_sub(16)) as *const usize;
        let ra_ptr = (fp.wrapping_sub(8)) as *const usize;

        let prev_fp = unsafe { prev_fp_ptr.read_volatile() };
        let ra = unsafe { ra_ptr.read_volatile() };

        if let Some((name, off)) = crate::symbols::lookup(ra) {
            if off == 0 {
                crate::kprintln!("[bt#{:02}] ra={:#018x} in {}", i, ra, name);
            } else {
                crate::kprintln!("[bt#{:02}] ra={:#018x} in {}+0x{:x}", i, ra, name, off);
            }
        } else {
            crate::kprintln!("[bt#{:02}] ra={:#018x} fp={:#x}", i, ra, fp);
        }

        if prev_fp == 0 || prev_fp == fp {
            break;
        }
        fp = prev_fp;
    }
}
