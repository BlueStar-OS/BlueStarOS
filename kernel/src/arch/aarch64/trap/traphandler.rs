use super::traplog::{
    current_trap_detail, log_exception_detail, log_unhandled_page_fault, ExceptionClass,
};
use crate::arch::driver::uart_irq_intid;
use crate::arch::memory::VirAddr;
use crate::arch::trap::traplog::log_user_opcode_window;
use crate::arch::TrapContext;
use crate::syscall::syscall_handler;
use crate::task::TASK_MANAER;
use crate::time::set_next_time_interupt;
use core::arch::asm;
use log::{debug, error, warn};

pub(crate) fn handle_page_fault_aarch64(fault_addr: VirAddr, esr: u64) {
    use crate::arch::memory::{PTEFlags, PageTable, VirNumber};
    debug!("Handle Fault Virtual Address:{:#x}", fault_addr.0);
    let contain_vpn: VirNumber = fault_addr.floor_down();
    let tsak_satp = TASK_MANAER.get_current_stap();
    let mut map_layer: PageTable = PageTable::crate_table_from_satp(tsak_satp);

    let ec = (esr >> 26) & 0x3F;
    let iss = esr & 0x1FFFFFF;
    let is_write = (iss & (1 << 6)) != 0;
    let is_instruction = ec == 0x20 || ec == 0x21;

    match &mut map_layer.find_pte_vpn(contain_vpn) {
        Some(pte) => {
            if pte.is_valid() {
                if !is_instruction && pte.flags().contains(PTEFlags::W) && is_write {
                    (*pte).set_isaccess();
                    (*pte).set_isdirty();
                    unsafe {
                        asm!("dsb sy", "tlbi vmalle1", "dsb sy", "isb");
                    }
                    warn!("Update pte access and dirty flags");
                    return;
                } else if !is_instruction && pte.flags().contains(PTEFlags::R) && !is_write {
                    (*pte).set_isaccess();
                    unsafe {
                        asm!("dsb sy", "tlbi vmalle1", "dsb sy", "isb");
                    }
                    warn!("Update pte access flags");
                    return;
                } else if !pte.is_valid() {
                } else {
                    log_unhandled_page_fault(fault_addr, esr, Some(pte.flags()));
                    TASK_MANAER.kail_current_task_and_run_next();
                    return;
                }
            }
        }
        None => {}
    }

    let will_kill = TASK_MANAER.task_que_inner.lock(|inner| {
        let current = inner.current;
        inner.task_queen[current].lock(|tcb| !tcb.memory_set.is_mmap_vpn(contain_vpn))
    });

    if will_kill {
        error!("area not contain mmap addr kill!");
        TASK_MANAER.kail_current_task_and_run_next();
        return;
    }

    debug!("[page_fault_handler]:ligel!");

    TASK_MANAER.task_que_inner.lock(|inner| {
        let current = inner.current;
        inner.task_queen[current].lock(|tcb| {
            tcb.memory_set.findarea_alloc_frame_and_set_pte(contain_vpn);
        });
    });
}

pub(crate) fn dispatch_user_sync() {
    let trap = current_trap_detail();
    let esr = trap.esr;
    let elr = trap.elr;
    let far = trap.far;
    let ec = trap.ec;

    {
        let current_trapcx = TASK_MANAER.get_current_trapcx();
        debug!(
            "TrapContext: elr_el1={:#x}, sp_el0={:#x}, x0={:#x}, x1={:#x}, x2={:#x}, x8={:#x}, x29={:#x}, x30={:#x}",
            current_trapcx.elr_el1,
            current_trapcx.sp_el0,
            current_trapcx.x[0],
            current_trapcx.x[1],
            current_trapcx.x[2],
            current_trapcx.x[8],
            current_trapcx.x[29],
            current_trapcx.x[30],
        );
    }

    let (sys_id, sys_args) = {
        let current_trapcx = TASK_MANAER.get_current_trapcx();
        let id = current_trapcx.x[8] as usize;
        let args = [
            current_trapcx.x[0] as usize,
            current_trapcx.x[1] as usize,
            current_trapcx.x[2] as usize,
            current_trapcx.x[3] as usize,
            current_trapcx.x[4] as usize,
            current_trapcx.x[5] as usize,
        ];
        (id, args)
    };

    match ec {
        ExceptionClass::SVC64 => {
            {
                let current_trapcx = TASK_MANAER.get_current_trapcx();
                debug!("pre elr:{:#x}", current_trapcx.elr_el1);
                // AArch64 ELR_EL1 for SVC already points to the next instruction.
                // Unlike RISC-V ecall, do not advance it again here.
            }

            let ret = syscall_handler(sys_id, sys_args);

            {
                let current_trapcx: &mut TrapContext = TASK_MANAER.get_current_trapcx();
                debug!("lat elr:{:#x}", current_trapcx.elr_el1);
                current_trapcx.x[0] = ret as usize;
            }
        }
        ExceptionClass::InstructionAbortLower => {
            log_exception_detail("User InstructionAbort");
            handle_page_fault_aarch64(VirAddr(elr as usize), esr);
        }
        ExceptionClass::DataAbortLower => {
            log_exception_detail("User DataAbort");
            handle_page_fault_aarch64(VirAddr(far as usize), esr);
        }
        ExceptionClass::PCAlignment => {
            error!("User PC alignment fault at {:#x}", elr);
            TASK_MANAER.kail_current_task_and_run_next();
        }
        ExceptionClass::SPAlignment => {
            error!("User SP alignment fault at {:#x}", far);
            TASK_MANAER.kail_current_task_and_run_next();
        }
        ExceptionClass::BRK => {
            error!("User BRK instruction at {:#x}", elr);
            TASK_MANAER.kail_current_task_and_run_next();
        }
        _ => {
            log_user_opcode_window(elr);
            log_exception_detail("Unknown user exception");
            panic!("Unknown trap from user: {:?}", ec);
        }
    }
}

pub(crate) fn dispatch_kernel_sync() {
    let trap = current_trap_detail();
    let esr = trap.esr;
    let elr = trap.elr;
    let far = trap.far;
    let ec = trap.ec;

    let (sys_id, sys_args) = {
        let current_trapcx = TASK_MANAER.get_current_trapcx();
        let id = current_trapcx.x[8] as usize;
        let args = [
            current_trapcx.x[0] as usize,
            current_trapcx.x[1] as usize,
            current_trapcx.x[2] as usize,
            current_trapcx.x[3] as usize,
            current_trapcx.x[4] as usize,
            current_trapcx.x[5] as usize,
        ];
        (id, args)
    };

    trap.log_debug("Kernel trap");

    match ec {
        ExceptionClass::SVC64 => {
            {
                let current_trapcx = TASK_MANAER.get_current_trapcx();
                debug!("pre elr:{:#x}", current_trapcx.elr_el1);
                // AArch64 ELR_EL1 for SVC already points to the next instruction.
            }
            let ret = syscall_handler(sys_id, sys_args);
            {
                let current_trapcx: &mut TrapContext = TASK_MANAER.get_current_trapcx();
                debug!("lat elr:{:#x}", current_trapcx.elr_el1);
                current_trapcx.x[0] = ret as usize;
            }
        }
        ExceptionClass::InstructionAbortLower | ExceptionClass::InstructionAbortSame => {
            error!(
                "Kernel {:?}: ESR={:#x} ELR={:#x} FAR={:#x}",
                ec, esr, elr, far
            );
            handle_page_fault_aarch64(VirAddr(elr as usize), esr);
        }
        ExceptionClass::DataAbortLower | ExceptionClass::DataAbortSame => {
            error!(
                "Kernel {:?}: ESR={:#x} ELR={:#x} FAR={:#x}",
                ec, esr, elr, far
            );
            handle_page_fault_aarch64(VirAddr(far as usize), esr);
        }
        ExceptionClass::PCAlignment => {
            error!("Kernel PC alignment fault at {:#x}", elr);
            TASK_MANAER.kail_current_task_and_run_next();
        }
        ExceptionClass::SPAlignment => {
            error!("Kernel SP alignment fault at {:#x}", far);
            TASK_MANAER.kail_current_task_and_run_next();
        }
        ExceptionClass::BRK => {
            error!("Kernel BRK instruction at {:#x}", elr);
            TASK_MANAER.kail_current_task_and_run_next();
        }
        _ => {
            panic!(
                "Unknown kernel trap: EC={:?}({:#x}) ESR={:#x} ELR={:#x} FAR={:#x}",
                ec, trap.ec_code, esr, elr, far
            );
        }
    }
}

fn gic_dispatch_irq(schedule_on_timer: bool) {
    use crate::arch::driver::gicd::{gic_read_iar, gic_write_eoir, TIMER_PPI_INTID};
    let mut need_schedule = false;
    loop {
        let irqnr = gic_read_iar();
        if irqnr >= 1020 {
            break;
        }
        match irqnr {
            TIMER_PPI_INTID => {
                set_next_time_interupt();
                if schedule_on_timer {
                    need_schedule = true;
                }
            }
            _ if irqnr == uart_irq_intid() => {
                crate::arch::driver::keyboard::keyboard_interrupt_handler();
            }
            _ => {
                warn!("未知中断 INTID={}", irqnr);
            }
        }
        gic_write_eoir(irqnr);
    }
    if need_schedule {
        TASK_MANAER.resolve_current_task_signal();
        TASK_MANAER.suspend_and_run_task();
    }
}

pub(crate) fn gic_handle_irq_in_kernel() {
    gic_dispatch_irq(false);
}

pub(crate) fn gic_handle_irq_in_user() {
    gic_dispatch_irq(true);
}
