use core::arch::asm;
fn backtrace() {
    let mut fp: usize;
    unsafe {
        asm!("mv {0}, s0", out(reg) fp, options(nomem, nostack, preserves_flags));
    }
    // RISC-V frame pointer convention (with frame pointers enabled):
    // [fp-16] = previous fp
    // [fp-8]  = saved ra
    for i in 0..32usize {
        if fp == 0 || (fp & 0xf) != 0 {
            break;
        }
        let prev_fp_ptr = (fp.wrapping_sub(16)) as *const usize;
        let ra_ptr = (fp.wrapping_sub(8)) as *const usize;

        let prev_fp = unsafe { prev_fp_ptr.read_volatile() };
        let ra = unsafe { ra_ptr.read_volatile() };

        crate::kprintln!("[bt#{:02}] fp={:#x} ra={:#x}", i, fp, ra);

        if prev_fp == 0 || prev_fp == fp {
            break;
        }
        fp = prev_fp;
    }
}
