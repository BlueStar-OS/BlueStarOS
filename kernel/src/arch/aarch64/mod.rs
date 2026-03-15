// AArch64 架构相关实现

pub mod task;
pub mod memory;
pub mod trap;
pub mod sbi;
pub mod time;
pub mod driver;

use core::arch::{global_asm, asm};
use crate::arch::memory::early_mmu_init;
use crate::arch::memory::eaylymmu::turn_early_mmu;
use crate::arch::task::TaskContext;
use crate::config::*;
use crate::arch::driver::gicd;
use crate::arch::driver::keyboard;
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


    // AArch64: 初始化 GIC 中断控制器 + UART RX 中断
    gicd::gic_init();
    gicd::gic_enable_spi(crate::arch::driver::gicd::UART2_INTID);
    keyboard::enable_uart_rx_interrupt();

}



/// 应用程序入口点
/// 从内核态切换到用户态
#[no_mangle]
pub extern "C" fn app_entry_point() {
    use crate::task::TASK_MANAER;
    use crate::config::{straper, TRAP_BOTTOM_ADDR};
    use log::debug;

    set_kernel_trap_handler();
    let user_satp = TASK_MANAER.get_current_stap();

    // 计算 __kernel_refume 在 trampoline 高地址中的位置
    // .text.traper section: straper(低地址) 映射到 TRAP_BOTTOM_ADDR(高地址)
    extern "C" { fn __kernel_refume(); }
    let refume_offset = __kernel_refume as usize - straper as usize;
    let refume_va = TRAP_BOTTOM_ADDR + refume_offset;

    debug!("app_entry: user_satp={:#x}, refume_va={:#x}", user_satp, refume_va);

    // 跳到 trampoline 高地址执行 __kernel_refume
    // 在那里切换 TTBR0/TTBR1 到用户页表后，当前代码（低地址）不可达，
    // 但 trampoline（高地址）在用户页表中也有映射，所以不会崩
    unsafe {
        asm!(
            "mov x0, {trap_cx}",
            "mov x1, {user_ttbr0}",
            "br {refume_va}",
            trap_cx = in(reg) TRAP_CONTEXT_ADDR,
            user_ttbr0 = in(reg) user_satp,
            refume_va = in(reg) refume_va,
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
