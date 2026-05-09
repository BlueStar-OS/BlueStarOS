//!内核主函数
//! bss段初始化，日志系统初始化，物理页帧分配系统初始化，内核地址空间初始化，激活内核地址空间
//#!
//#![deny(missing_docs)]
//#![deny(warnings)]
#![no_std]
#![no_main]
#![feature(panic_internals, panic_info_message, const_trait_impl, error_in_core)]

extern crate alloc;

// 导出 paste crate 供宏使用
pub use paste;

mod arch; // 架构抽象层
mod driver; // driver 必须在 console 之前加载，因为 console 依赖 driver
#[macro_use]
mod console;
mod config;
mod error;
mod fs;
mod logger;
mod memory;
mod panic;
mod symbols;
mod sync;
mod syscall;
mod task;
mod time;
mod tool;
mod trap;
use crate::arch::*;
use crate::config::*;
use crate::driver::dtb;
use crate::driver::gpu::inital_gpu;
use crate::fs::vfs::*;
use crate::logger::*;
use crate::memory::*;
use crate::root::RootFs;
use crate::task::run_first_task;
use crate::time::*;
use core::arch::global_asm;
use core::ptr::write_volatile;
use log::*;
pub use sbi::*;

global_asm!(include_str!("app.asm"));

/// clear BSS segment
pub fn clear_bss() {
    extern "C" {
        pub fn sbss();
        pub fn ebss();
    }
    (sbss as *const () as usize..ebss as *const () as usize).for_each(|a| unsafe { (a as *mut u8).write_volatile(0) });
}

/// the rust entry-point of os
#[no_mangle]
pub fn blue_main() -> ! {
    //永远不会返回
    kprintln!("Clean BSS");
    clear_bss();

    //平台初始化 必须放在clearbss之后！ 因为其中有bss的静态初始化页表相关
    arch_init();

    debug!("stext {:#x}", __kernel_trap as *const () as usize);

    debug!("traper {:#x}", straper as *const () as usize);
    debug!(
        "trap refume virtualaddr:{:#x}",
        __kernel_refume as *const () as usize - __kernel_trap as *const () as usize + TRAP_BOTTOM_ADDR
    );
    inital_gpu();
    warn!("initial file system");
    RootFs::init_rootfs();

    run_first_task();

    warn!("All right,kernel Will end\n");
    panic!("Kernel End");
}
