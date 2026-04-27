use crate::global_asm;
use core::arch::asm;
use core::panicking::panic;

global_asm!(include_str!("./trap.asm"));

pub mod kernel_trap;
pub mod traphandler;
pub mod traplog;
pub mod user_trap;

extern "C" {
    pub fn __aarch64_vector();
}

pub use kernel_trap::kernel_trap_handler;
pub use user_trap::app_entry_point;

#[repr(C)]
pub struct TrapContext {
    pub x: [usize; 31],      //用户寄存器
    pub sp_el0: usize,       //用户栈
    pub elr_el1: usize,      //异常返回地址
    pub spsr_el1: usize,     //被打断现场的状态
    pub ttbr_el1: usize,     //内核页表（返回 EL1 后用于内核可见地址空间）
    pub kernel_sp: usize,    //用户内核栈
    pub trap_handler: usize, // traphandler地址
}

#[no_mangle]
pub fn no_return_start() -> ! {
    panic("Start Function you ret ,WTF????");
}

impl TrapContext {
    pub fn init_app_trap_context(
        entry: usize,
        kernel_satp: usize,
        trap_handler: usize,
        kernel_sp: usize,
        user_sp: usize,
    ) -> Self {
        use log::debug;

        let spsr: usize = 0x0;
        debug!("SPSR_EL1: {:#X}", spsr);
        let mut x = [0usize; 31];
        x[30] = no_return_start as usize;

        TrapContext {
            x,
            sp_el0: user_sp,
            elr_el1: entry,
            spsr_el1: spsr,
            ttbr_el1: kernel_satp,
            kernel_sp,
            trap_handler,
        }
    }
}

pub fn set_kernel_trap_handler() {
    use crate::config::TRAP_BOTTOM_ADDR;

    unsafe {
        let vector_table = TRAP_BOTTOM_ADDR;
        asm!("msr vbar_el1, {}", in(reg) vector_table);
    }
}
