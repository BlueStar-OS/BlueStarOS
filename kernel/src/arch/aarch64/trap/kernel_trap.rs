use log::error;

use super::traplog::log_exception_detail;
use super::traphandler::{dispatch_kernel_sync, gic_handle_irq_in_kernel};
use super::user_trap::app_entry_point;



#[no_mangle]
pub extern "C" fn kernel_trap_handler() {
    use crate::trap::recycle_pending_kstacks;
    recycle_pending_kstacks();
    dispatch_kernel_sync();
    app_entry_point();
}

#[no_mangle]
pub extern "C" fn kernel_irq_handler() {
    gic_handle_irq_in_kernel();
}

#[no_mangle]
pub extern "C" fn sync_el1_spx() {
    log_exception_detail("Kernel sync exception (SP_EL1)");
    panic!("Kernel exception - this should not happen!");
}

#[no_mangle]
pub extern "C" fn irq_el1_spx() {
    kernel_irq_handler();
}

#[no_mangle]
pub extern "C" fn fiq_el1_spx() {
    panic!("FIQ in kernel (SP_EL1)");
}

#[no_mangle]
pub extern "C" fn serror_el1_spx() {
    log_exception_detail("SError in kernel (SP_EL1)");
    panic!("Kernel SError!");
}

#[no_mangle]
pub extern "C" fn sync_el1_sp0() {
    log_exception_detail("Kernel sync exception (SP_EL0)");
    panic!("Kernel exception with SP_EL0!");
}

#[no_mangle]
pub extern "C" fn irq_el1_sp0() {
    panic!("IRQ in kernel (SP_EL0)");
}

#[no_mangle]
pub extern "C" fn fiq_el1_sp0() {
    panic!("FIQ in kernel (SP_EL0)");
}

#[no_mangle]
pub extern "C" fn serror_el1_sp0() {
    log_exception_detail("SError in kernel (SP_EL0)");
    panic!("Kernel SError with SP_EL0!");
}
