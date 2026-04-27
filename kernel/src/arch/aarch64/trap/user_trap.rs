use super::set_kernel_trap_handler;
use super::traphandler::{dispatch_user_sync, gic_handle_irq_in_user};
use super::traplog::log_exception_detail;
use core::arch::asm;

#[inline]
fn raw_irq_probe(tag: &[u8]) {
    for &b in tag {
        crate::arch::driver::uart::putc(b);
    }
}

#[no_mangle]
pub extern "C" fn app_entry_point() {
    use crate::config::{straper, TRAP_BOTTOM_ADDR};
    use crate::task::TASK_MANAER;
    use log::debug;

    set_kernel_trap_handler();
    let user_satp = TASK_MANAER.get_current_stap();
    let trap_cx = TASK_MANAER.get_current_trapcx();
    let trap_cx_ptr = trap_cx as *mut _ as usize;

    extern "C" {
        fn __kernel_refume();
    }
    let refume_offset = __kernel_refume as usize - straper as usize;
    let refume_va = TRAP_BOTTOM_ADDR + refume_offset;

    debug!(
        "app_entry: user_satp={:#x}, refume_va={:#x}, trap_elr={:#x}, trap_sp={:#x}, trap_x30={:#x}",
        user_satp,
        refume_va,
        trap_cx.elr_el1,
        trap_cx.sp_el0,
        trap_cx.x[30]
    );
    unsafe {
        asm!(
            "br {refume_va}",
            in("x0") trap_cx_ptr,
            in("x1") user_satp,
            refume_va = in(reg) refume_va,
            options(noreturn)
        );
    }
}
#[no_mangle]
pub extern "C" fn sync_el0_64() {
    use crate::trap::recycle_pending_kstacks;

    recycle_pending_kstacks();
    dispatch_user_sync();
    app_entry_point();
}

#[no_mangle]
pub extern "C" fn irq_el0_64() {
    use crate::trap::recycle_pending_kstacks;

    //raw_irq_probe(b"<uirq>");
    // log::warn!("[IRQ] enter user irq_el0_64");
    recycle_pending_kstacks();
    gic_handle_irq_in_user();
    app_entry_point();
}

#[no_mangle]
pub extern "C" fn fiq_el0_64() {
    panic!("FIQ from EL0");
}

#[no_mangle]
pub extern "C" fn serror_el0_64() {
    log_exception_detail("SError from EL0");
    panic!("SError from EL0");
}

#[no_mangle]
pub extern "C" fn sync_el0_32() {
    panic!("32-bit user mode not supported");
}

#[no_mangle]
pub extern "C" fn irq_el0_32() {
    panic!("IRQ from 32-bit EL0");
}

#[no_mangle]
pub extern "C" fn fiq_el0_32() {
    panic!("FIQ from 32-bit EL0");
}

#[no_mangle]
pub extern "C" fn serror_el0_32() {
    panic!("SError from 32-bit EL0");
}
