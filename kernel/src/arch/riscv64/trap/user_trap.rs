use crate::arch::driver;
use crate::arch::driver::keyboard;
use crate::arch::memory::VirAddr;
use crate::arch::set_kernel_trap;
use crate::arch::set_kernel_trap_handler;
use crate::arch::TrapContext;
use crate::config::{__kernel_refume, __kernel_trap, TRAP_BOTTOM_ADDR, TRAP_CONTEXT_ADDR};
use crate::syscall::syscall_handler;
use crate::task::TASK_MANAER;
use crate::time::set_next_timeInterupt;
use crate::trap::pagefaultHandler::PageFaultHandler;
use crate::trap::recycle_pending_kstacks;
use core::arch::asm;
use log::{error, warn};
use riscv::register::satp;
use riscv::register::scause::Interrupt;
use riscv::register::{
    scause::{self, Exception, Trap},
    sepc, sstatus, stval,
};

fn log_user_fault_detail(tag: &str) {
    let scauses = scause::read();
    let sepc_val = sepc::read();
    let stval_val = stval::read();
    let sstatus_val = sstatus::read();
    let satp_val = satp::read();
    error!(
        "{}: cause={:?} sepc={:#x} stval={:#x} va={:#x} sstatus={:#x} satp={:#x}",
        tag,
        scauses.cause(),
        sepc_val,
        stval_val,
        stval_val,
        sstatus_val.bits(),
        satp_val.bits()
    );
}

#[no_mangle]
pub extern "C" fn app_entry_point() {
    set_kernel_trap_handler();
    let user_satp = TASK_MANAER.get_current_stap();
    let restore_va = __kernel_refume as *const () as usize - __kernel_trap as *const () as usize
        + TRAP_BOTTOM_ADDR;
    unsafe {
        asm!(
            "fence.i",
            "jr {restore_va}",
            restore_va = in(reg) restore_va,
            in("a0") TRAP_CONTEXT_ADDR,
            in("a1") user_satp,
            options(noreturn)
        );
    }
}

#[no_mangle]
pub extern "C" fn kernel_trap_handler() {
    recycle_pending_kstacks();

    set_kernel_trap();
    let scauses = scause::read();
    let sepc_val = sepc::read();
    let stval_val = stval::read();
    let (sys_id, sys_args) = {
        let current_trapcx = TASK_MANAER.get_current_trapcx();
        let id = current_trapcx.x[17];
        let args = [
            current_trapcx.x[10],
            current_trapcx.x[11],
            current_trapcx.x[12],
            current_trapcx.x[13],
            current_trapcx.x[14],
            current_trapcx.x[15],
        ];
        (id, args)
    };
    match scauses.cause() {
        Trap::Exception(Exception::UserEnvCall) => {
            {
                let current_trapcx = TASK_MANAER.get_current_trapcx();
                current_trapcx.sepc_entry_point += 4;
            }
            let ret = syscall_handler(sys_id, sys_args);
            {
                let current_trapcx: &mut TrapContext = TASK_MANAER.get_current_trapcx();
                current_trapcx.x[10] = ret as usize;
            }
        }
        Trap::Exception(Exception::IllegalInstruction) => {
            error!("User IllegalInstruction at {:#x}", sepc_val);
            TASK_MANAER.kail_current_task_and_run_next();
        }
        Trap::Exception(Exception::InstructionPageFault) => {
            log_user_fault_detail("User InstructionPageFault");
            PageFaultHandler(VirAddr(stval_val), scauses);
        }
        Trap::Exception(Exception::LoadPageFault) => {
            log_user_fault_detail("User LoadPageFault");
            PageFaultHandler(VirAddr(stval_val), scauses);
        }
        Trap::Exception(Exception::StorePageFault) => {
            log_user_fault_detail("User StorePageFault");
            PageFaultHandler(VirAddr(stval_val), scauses);
        }
        Trap::Interrupt(Interrupt::SupervisorTimer) => {
            set_next_timeInterupt();
            TASK_MANAER.resolve_current_task_signal();
            TASK_MANAER.suspend_and_run_task();
        }
        Trap::Interrupt(Interrupt::SupervisorExternal) => {
            let irq = driver::plic::plic_claim();
            if irq == driver::plic::UART0_IRQ {
                keyboard::keyboard_interrupt_handler();
            } else if irq != 0 {
                warn!("未知外部中断 IRQ={}", irq);
            }
            if irq != 0 {
                driver::plic::plic_complete(irq);
            }
        }
        _ => {
            panic!("Unknown trap from user: {:?}", scauses.cause())
        }
    }
    app_entry_point();
}
