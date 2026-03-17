use crate::arch::driver;
use crate::arch::driver::keyboard;
use crate::arch::shutdown;
use crate::task::TASK_MANAER;
use crate::time::set_next_timeInterupt;
use riscv::register::satp;
use riscv::register::scause::Interrupt;
use riscv::register::{scause::{self, Trap}, sepc, sstatus, stval};
#[no_mangle]
pub extern "C" fn kernel_mode_trap_handler() {
    let scauses = scause::read();

    match scauses.cause() {
        Trap::Interrupt(Interrupt::SupervisorExternal) => {
            let irq = driver::plic::plic_claim();
            if irq == driver::plic::UART0_IRQ {
                keyboard::keyboard_interrupt_handler();
            }
            if irq != 0 {
                driver::plic::plic_complete(irq);
            }
        }
        Trap::Interrupt(Interrupt::SupervisorTimer) => {
            set_next_timeInterupt();
        }
        _ => {
            let sepc_val = sepc::read();
            let stval_val = stval::read();
            panic!(
                "Unexpected kernel trap: cause={:?} sepc={:#x} stval={:#x}",
                scauses.cause(),
                sepc_val,
                stval_val
            );
        }
    }
}
