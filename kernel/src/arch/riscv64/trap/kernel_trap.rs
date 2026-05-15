use crate::arch::{driver, enable_irq};
use crate::time::set_next_timeInterupt;
use log::{debug, error};
use riscv::register::scause::{self, Trap};
use riscv::register::scause::{Exception, Interrupt};
use riscv::register::{sepc, stval};

use crate::arch::memory::VirAddr;
use crate::trap::pagefaultHandler::PageFaultHandler;

/// 用户空间地址上限（SV39 下用户空间为 [0, 0x80000000)）
const USER_SPACE_END: usize = 0x8000_0000;

#[no_mangle]
pub extern "C" fn kernel_mode_trap_handler() {
    let scauses = scause::read();

    match scauses.cause() {
        Trap::Interrupt(Interrupt::SupervisorExternal) => {
            error!("内核态外部中断");
            driver::plic::dispatch_irq();
        }
        Trap::Interrupt(Interrupt::SupervisorTimer) => {
            set_next_timeInterupt();
        }
        // 内核访问用户空间地址缺页 → 路由到 PageFaultHandler 处理 demand paging
        Trap::Exception(Exception::LoadPageFault)
        | Trap::Exception(Exception::StorePageFault)
        | Trap::Exception(Exception::InstructionPageFault) => {
            let sepc_val = sepc::read();
            let stval_val = stval::read();

            // 仅当 fault 地址在用户空间时才交由 PageFaultHandler
            if stval_val < USER_SPACE_END {
                PageFaultHandler(VirAddr(stval_val), scauses);
            } else {
                panic!(
                    "Unexpected kernel trap: cause={:?} sepc={:#x} stval={:#x}",
                    scauses.cause(),
                    sepc_val,
                    stval_val
                );
            }
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
