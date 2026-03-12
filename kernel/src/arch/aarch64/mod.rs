// AArch64 架构相关实现

pub mod task;
pub mod memory;
pub mod trap;
pub mod sbi;
pub mod time;

use core::arch::{global_asm, asm};
use crate::arch::memory::early_mmu_init;
use crate::arch::memory::eaylymmu::turn_early_mmu;
use crate::arch::task::TaskContext;
use crate::config::*;
pub use sbi::*;

// 引入入口
global_asm!(include_str!("./entry.asm"));

// 导出trap相关类型和函数
pub use trap::{set_kernel_trap_handler, set_kernel_forbid, TrapContext};
pub use trap::__aarch64_vector;

// 导出任务切换函数
pub use task::__switch;


/// 平台初始化函数
pub fn arch_init(){
    
    unsafe {
        // 填充静态页表
        early_mmu_init();
    }

    // 打开早期的mmu，因为不开默认全是device memory
    turn_early_mmu();
}



/// 应用程序入口点
/// 从内核态切换到用户态
#[no_mangle]
pub extern "C" fn app_entry_point() {
    use crate::task::TASK_MANAER;

    set_kernel_trap_handler();
    let user_satp = TASK_MANAER.get_current_stap();

    // AArch64: 直接调用 __kernel_refume
    // 参数: x0 = TrapContext地址, x1 = 用户页表(ttbr0_el1)
    unsafe {
        asm!(
            "mov x0, {trap_cx}",
            "mov x1, {user_ttbr0}",
            "b __kernel_refume",
            trap_cx = in(reg) TRAP_CONTEXT_ADDR,
            user_ttbr0 = in(reg) user_satp,
            options(noreturn)
        );
    }
}

/// 内核陷阱处理函数
#[no_mangle]
pub extern "C" fn kernel_trap_handler() {
    use log::error;
    use crate::trap::recycle_pending_kstacks;

    // 回收内核栈
    recycle_pending_kstacks();

    set_kernel_forbid();

    // 读取ESR_EL1获取异常原因
    let esr: u64;
    let elr: u64;
    let far: u64;
    unsafe {
        asm!("mrs {}, esr_el1", out(reg) esr);
        asm!("mrs {}, elr_el1", out(reg) elr);
        asm!("mrs {}, far_el1", out(reg) far);
    }

    let ec = (esr >> 26) & 0x3F;

    error!("Kernel trap: EC={:#x} ESR={:#x} ELR={:#x} FAR={:#x}", ec, esr, elr, far);

    match ec {
        0x15 => {
            // SVC (系统调用)
            error!("Unexpected SVC in kernel mode");
        }
        0x24 | 0x25 => {
            // Data abort
            error!("Kernel data abort at {:#x}", far);
        }
        0x20 | 0x21 => {
            // Instruction abort
            error!("Kernel instruction abort at {:#x}", far);
        }
        _ => {
            error!("Unknown kernel exception");
        }
    }

    panic!("Kernel trap handler - should not reach here");
}

/// 愿意处理全局中断使能 (AArch64实现)
pub fn rather_global_interrupt() {
    unsafe {
        // 启用IRQ中断 (清除DAIF.I位)
        asm!("msr daifclr, #2");
    }
}

/// 开启全局时间中断使能 (AArch64实现)
pub fn enable_timer_interupt() {
    unsafe {
        // 启用EL1物理定时器中断
        // CNTP_CTL_EL0: bit 0 = enable, bit 1 = imask (0=not masked)
        asm!("msr cntp_ctl_el0, {}", in(reg) 1u64);
    }
}
